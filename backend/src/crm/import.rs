//! Import of counterparties from XLSX or CSV.
//!
//! The first row holds column names; Russian and English headers are
//! recognised ("Наименование"/"name", "ИНН"/"inn", ...). CSV may be UTF-8 or
//! Windows-1251 (Excel's default for Russian locales), separated by `;` or `,`.
//! Each row is validated like a manually entered card; duplicates by INN+KPP
//! and invalid rows are skipped and reported.

use std::io::Cursor;

use axum::{
    extract::{Multipart, State},
    Json,
};
use bytes::Bytes;
use calamine::{Data, Reader, Xlsx};
use serde::Serialize;
use serde_json::{json, Value};

use super::companies::{clean, insert_company, CompanyInput};
use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiError, ApiResult},
    files::extension_of,
    state::AppState,
};

const MAX_ROWS: usize = 5000;
const MAX_BYTES: usize = 10 * 1024 * 1024;

/// Map a header cell to a company field.
fn field_for(header: &str) -> Option<&'static str> {
    let h: String = header.trim().to_lowercase().replace(['ё'], "е").chars().filter(|c| !matches!(c, '.' | '*' | ':')).collect();
    Some(match h.as_str() {
        "наименование" | "название" | "краткое наименование" | "контрагент" | "организация" | "name" => "name",
        "полное наименование" | "full_name" | "full name" => "full_name",
        "инн" | "inn" => "inn",
        "кпп" | "kpp" => "kpp",
        "огрн" | "огрнип" | "ogrn" => "ogrn",
        "адрес" | "юридический адрес" | "юр адрес" | "legal_address" | "address" => "legal_address",
        "почтовый адрес" | "фактический адрес" | "postal_address" => "postal_address",
        "телефон" | "тел" | "phone" => "phone",
        "email" | "e-mail" | "эл почта" | "электронная почта" | "почта" => "email",
        "сайт" | "website" | "web" => "website",
        "руководитель" | "директор" | "генеральный директор" | "director" => "director",
        "банк" | "bank" | "bank_name" => "bank_name",
        "бик" | "bik" => "bik",
        "р/с" | "рс" | "расчетный счет" | "счет" | "account" => "account",
        "к/с" | "кс" | "корр счет" | "корреспондентский счет" | "corr_account" => "corr_account",
        "примечание" | "комментарий" | "notes" => "notes",
        "теги" | "метки" | "tags" => "tags",
        _ => return None,
    })
}

/// Excel stores INN/OGRN typed as numbers as floats; print them without decimals.
fn cell_text(c: &Data) -> String {
    match c {
        Data::Empty => String::new(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e18 => format!("{f:.0}"),
        Data::Int(i) => i.to_string(),
        other => other.to_string().trim().to_string(),
    }
}

fn read_xlsx(data: &[u8]) -> ApiResult<Vec<Vec<String>>> {
    let mut wb: Xlsx<_> = Xlsx::new(Cursor::new(data)).map_err(|e| ApiError::BadRequest(format!("cannot read XLSX: {e}")))?;
    let range = wb
        .worksheet_range_at(0)
        .ok_or_else(|| ApiError::BadRequest("the workbook has no sheets".into()))?
        .map_err(|e| ApiError::BadRequest(format!("cannot read the first sheet: {e}")))?;
    Ok(range.rows().take(MAX_ROWS + 1).map(|r| r.iter().map(cell_text).collect()).collect())
}

fn read_csv(data: &[u8]) -> ApiResult<Vec<Vec<String>>> {
    let text = match std::str::from_utf8(data) {
        Ok(s) => s.trim_start_matches('\u{feff}').to_string(),
        Err(_) => encoding_rs::WINDOWS_1251.decode(data).0.into_owned(),
    };
    let first = text.lines().next().unwrap_or_default();
    let delimiter = if first.matches(';').count() >= first.matches(',').count() { b';' } else { b',' };
    let mut rdr = csv::ReaderBuilder::new().delimiter(delimiter).has_headers(false).flexible(true).from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for rec in rdr.records().take(MAX_ROWS + 1) {
        let rec = rec.map_err(|e| ApiError::BadRequest(format!("CSV error: {e}")))?;
        rows.push(rec.iter().map(|s| s.trim().to_string()).collect());
    }
    Ok(rows)
}

#[derive(Serialize)]
struct Skipped {
    row: usize,
    name: String,
    reason: String,
}

pub async fn import(State(state): State<AppState>, user: CurrentUser, mut multipart: Multipart) -> ApiResult<Json<Value>> {
    user.require_crm()?;
    let mut file: Option<(String, Bytes)> = None;
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::BadRequest(e.to_string()))? {
        if let Some(name) = field.file_name().map(str::to_string) {
            file = Some((name, field.bytes().await.map_err(|e| ApiError::BadRequest(e.to_string()))?));
        }
    }
    let (name, data) = file.ok_or_else(|| ApiError::BadRequest("no file uploaded".into()))?;
    if data.len() > MAX_BYTES {
        return Err(ApiError::TooLarge("the file must not exceed 10 MB".into()));
    }
    let rows = match extension_of(&name).as_str() {
        "xlsx" => tokio::task::spawn_blocking(move || read_xlsx(&data)).await.map_err(|e| ApiError::Internal(e.into()))??,
        "csv" | "txt" => read_csv(&data)?,
        _ => return bad_request("supported formats: .xlsx, .csv"),
    };
    let mut it = rows.into_iter();
    let header = it.next().ok_or_else(|| ApiError::BadRequest("the file is empty".into()))?;
    let columns: Vec<Option<&'static str>> = header.iter().map(|h| field_for(h)).collect();
    if !columns.contains(&Some("name")) {
        return bad_request("column «Наименование» (name) not found in the first row");
    }

    let mut created = 0usize;
    let mut skipped: Vec<Skipped> = Vec::new();
    let mut total = 0usize;
    for (i, row) in it.enumerate() {
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        total += 1;
        if total > MAX_ROWS {
            return bad_request(format!("too many rows (max {MAX_ROWS})"));
        }
        let mut c = CompanyInput { visibility: "all".into(), responsible_id: Some(user.user_id), ..Default::default() };
        for (col, value) in columns.iter().zip(row.iter()) {
            let v = value.trim().to_string();
            match col {
                Some("name") => c.name = v,
                Some("full_name") => c.full_name = v,
                Some("inn") => c.inn = Some(v),
                Some("kpp") => c.kpp = Some(v),
                Some("ogrn") => c.ogrn = Some(v),
                Some("legal_address") => c.legal_address = v,
                Some("postal_address") => c.postal_address = v,
                Some("phone") => c.phone = v,
                Some("email") => c.email = v,
                Some("website") => c.website = v,
                Some("director") => c.director = v,
                Some("bank_name") => c.bank_name = v,
                Some("bik") => c.bik = Some(v),
                Some("account") => c.account = Some(v),
                Some("corr_account") => c.corr_account = Some(v),
                Some("notes") => c.notes = v,
                Some("tags") => c.tags = v.split([',', ';']).map(str::to_string).collect(),
                _ => {}
            }
        }
        let row_no = i + 2; // 1-based, after the header
        let display = c.name.clone();
        let result = match clean(c) {
            Ok(c) => insert_company(&state, &user, &c).await.map(|_| ()),
            Err(e) => Err(e),
        };
        match result {
            Ok(()) => created += 1,
            Err(ApiError::BadRequest(m)) => skipped.push(Skipped { row: row_no, name: display, reason: m }),
            Err(ApiError::Conflict(_)) => skipped.push(Skipped { row: row_no, name: display, reason: "дубликат: такой ИНН/КПП уже есть".into() }),
            Err(e) => return Err(e),
        }
    }
    state.audit(user.tenant_id, Some(user.user_id), "crm.companies_imported", None, None, json!({ "created": created, "skipped": skipped.len() })).await;
    Ok(Json(json!({ "created": created, "skipped": skipped, "total": total })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers() {
        assert_eq!(field_for(" ИНН "), Some("inn"));
        assert_eq!(field_for("Наименование*"), Some("name"));
        assert_eq!(field_for("Расчётный счёт"), Some("account"));
        assert_eq!(field_for("E-mail"), Some("email"));
        assert_eq!(field_for("Что-то ещё"), None);
    }

    #[test]
    fn csv_cp1251_semicolon() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode("Наименование;ИНН\nООО Ромашка;7707083893\n");
        let rows = read_csv(&bytes).unwrap();
        assert_eq!(rows[1], vec!["ООО Ромашка", "7707083893"]);
    }

    #[test]
    fn numeric_cells() {
        assert_eq!(cell_text(&Data::Float(7707083893.0)), "7707083893");
        assert_eq!(cell_text(&Data::String(" x ".into())), "x");
    }
}
