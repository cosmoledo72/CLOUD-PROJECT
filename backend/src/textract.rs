//! Plain text of documents for the portal search (C5): DOCX/DOCM/DOTX (body,
//! headers, footers, notes), XLSX/XLSM (sheet names and cell values), PPTX
//! (slide text), PDF with a text layer, TXT/CSV/MD/JSON/XML (UTF-8 or
//! Windows-1251). Old binary formats (DOC, XLS, RTF) and scans are found by
//! name only. Also the background indexer that keeps `file_texts` in step
//! with the current version of every file.

use std::io::{Cursor, Read};
use std::time::Duration;

use uuid::Uuid;

use crate::{state::AppState, storage::BlobStore};

/// Longest text kept per file (PostgreSQL limits a tsvector to 1 MB).
const MAX_CHARS: usize = 300_000;
/// Files larger than this are found by name only.
const MAX_BYTES: i64 = 60 * 1024 * 1024;

pub fn supported(ext: &str) -> bool {
    matches!(ext, "docx" | "docm" | "dotx" | "xlsx" | "xlsm" | "pptx" | "pdf" | "txt" | "csv" | "md" | "json" | "xml" | "log")
}

/// Extract the text; `None` when the format is not supported or the file is broken.
pub fn extract(ext: &str, bytes: &[u8]) -> Option<String> {
    let text = match ext {
        "docx" | "docm" | "dotx" => ooxml_text(bytes, |n| {
            n == "word/document.xml" || (n.starts_with("word/header") || n.starts_with("word/footer") || n == "word/footnotes.xml" || n == "word/endnotes.xml") && n.ends_with(".xml")
        })?,
        "pptx" => ooxml_text(bytes, |n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml"))?,
        "xlsx" | "xlsm" => xlsx_text(bytes)?,
        "pdf" => pdf_text(bytes)?,
        "txt" | "csv" | "md" | "json" | "xml" | "log" => decode(bytes),
        _ => return None,
    };
    Some(clip(&normalize(&text)))
}

fn clip(s: &str) -> String {
    if s.chars().count() <= MAX_CHARS {
        return s.to_string();
    }
    s.chars().take(MAX_CHARS).collect()
}

/// Collapse runs of spaces and blank lines.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank = 0;
    for line in s.lines() {
        let l = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(&l);
        out.push('\n');
    }
    out.trim().to_string()
}

fn decode(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1251.decode(b).0.into_owned(),
    }
}

/// Text of the selected XML parts of an Office Open XML package: text runs,
/// paragraph and line breaks, tabs.
fn ooxml_text(bytes: &[u8], want: impl Fn(&str) -> bool) -> Option<String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut names: Vec<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok().map(|f| f.name().to_string())).filter(|n| want(n)).collect();
    // Body first, then headers/footers; slides in their numeric order.
    names.sort_by_key(|n| {
        let num: u32 = n.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        (!n.ends_with("document.xml"), n.chars().filter(|c| !c.is_ascii_digit()).collect::<String>(), num)
    });
    let mut out = String::new();
    for n in names {
        let mut xml = String::new();
        if let Ok(mut f) = zip.by_name(&n) {
            if f.size() > 200 * 1024 * 1024 || f.read_to_string(&mut xml).is_err() {
                continue;
            }
        }
        out.push_str(&xml_text(&xml));
        out.push('\n');
    }
    Some(out)
}

/// Text content of WordprocessingML/DrawingML: `<w:t>`/`<a:t>` runs; `</w:p>`,
/// `</a:p>`, `<w:br/>` break lines; `<w:tab/>` is a tab.
fn xml_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len() / 4);
    let mut i = 0;
    let b = xml.as_bytes();
    let mut in_text = false;
    while i < b.len() {
        if b[i] == b'<' {
            let end = match xml[i..].find('>') {
                Some(e) => i + e,
                None => break,
            };
            let tag = &xml[i + 1..end];
            let name = tag.trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("");
            let closing = tag.starts_with('/');
            let local = name.rsplit(':').next().unwrap_or(name);
            match local {
                "t" => in_text = !closing && !tag.ends_with('/'),
                "p" if closing => out.push('\n'),
                "br" | "cr" => out.push('\n'),
                "tab" => out.push('\t'),
                _ => {}
            }
            i = end + 1;
        } else {
            let next = xml[i..].find('<').map(|e| i + e).unwrap_or(b.len());
            if in_text {
                out.push_str(&unescape(&xml[i..next]));
            }
            i = next;
        }
    }
    out
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        let tail = &rest[p..];
        let Some(semi) = tail.find(';') else {
            out.push_str(tail);
            return out;
        };
        let ent = &tail[1..semi];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            e if e.starts_with("#x") => u32::from_str_radix(&e[2..], 16).ok().and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => out.push(c),
            None => out.push_str(&tail[..=semi]),
        }
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    out
}

fn xlsx_text(bytes: &[u8]) -> Option<String> {
    use calamine::{Data, Reader, Xlsx};
    let mut wb: Xlsx<_> = Xlsx::new(Cursor::new(bytes.to_vec())).ok()?;
    let mut out = String::new();
    for name in wb.sheet_names().to_vec() {
        out.push_str(&name);
        out.push('\n');
        if let Ok(range) = wb.worksheet_range(&name) {
            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .filter_map(|c| match c {
                        Data::Empty | Data::Error(_) => None,
                        Data::String(s) => Some(s.clone()),
                        Data::Float(f) => Some(if f.fract() == 0.0 && f.abs() < 1e15 { format!("{}", *f as i64) } else { f.to_string() }),
                        Data::Int(i) => Some(i.to_string()),
                        Data::Bool(b) => Some(b.to_string()),
                        other => Some(other.to_string()),
                    })
                    .collect();
                if !cells.is_empty() {
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
                if out.len() > MAX_CHARS * 4 {
                    return Some(out);
                }
            }
        }
    }
    Some(out)
}

fn pdf_text(bytes: &[u8]) -> Option<String> {
    let b = bytes.to_vec();
    // The PDF parser may panic on malformed files: keep the indexer alive.
    std::panic::catch_unwind(move || pdf_extract::extract_text_from_mem(&b).ok()).ok().flatten()
}

// ---------------------------------------------------------------------------
// Indexer
// ---------------------------------------------------------------------------

/// (Re)index one file: extract the text of its current version.
pub async fn index_file(state: &AppState, file_id: Uuid) -> anyhow::Result<()> {
    let row: Option<(String, i32, i64, String)> = sqlx::query_as(
        "SELECT f.extension, f.current_version, f.size_bytes, v.storage_key
           FROM files f JOIN file_versions v ON v.file_id = f.id AND v.version = f.current_version
          WHERE f.id = $1",
    )
    .bind(file_id)
    .fetch_optional(&state.db)
    .await?;
    let Some((ext, version, size, key)) = row else { return Ok(()) };
    let ext = ext.to_lowercase();
    let text = if supported(&ext) && size <= MAX_BYTES {
        match state.store.get(&key).await {
            Ok(bytes) => tokio::task::spawn_blocking(move || extract(&ext, &bytes)).await.ok().flatten().unwrap_or_default(),
            Err(_) => String::new(),
        }
    } else {
        String::new()
    };
    // A stray NUL breaks the text column.
    let text = text.replace('\0', " ");
    sqlx::query(
        "INSERT INTO file_texts (file_id, version, content, indexed_at) VALUES ($1, $2, $3, now())
         ON CONFLICT (file_id) DO UPDATE SET version = EXCLUDED.version, content = EXCLUDED.content, indexed_at = now()",
    )
    .bind(file_id)
    .bind(version)
    .bind(&text)
    .execute(&state.db)
    .await?;
    Ok(())
}

/// Index a file in the background after it was created or saved.
pub fn queue(state: &AppState, file_id: Uuid) {
    let state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = index_file(&state, file_id).await {
            tracing::warn!("indexing file {file_id}: {e:#}");
        }
    });
}

/// Index files whose text is missing or older than their current version:
/// right after start (existing files) and then every few minutes as a safety net.
pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            let ids: Vec<Uuid> = sqlx::query_scalar(
                "SELECT f.id FROM files f LEFT JOIN file_texts t ON t.file_id = f.id
                  WHERE f.deleted_at IS NULL AND (t.file_id IS NULL OR t.version <> f.current_version)
                  ORDER BY f.updated_at DESC LIMIT 50",
            )
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
            if ids.is_empty() {
                tokio::time::sleep(Duration::from_secs(300)).await;
                continue;
            }
            for id in ids {
                if let Err(e) = index_file(&state, id).await {
                    tracing::warn!("indexing file {id}: {e:#}");
                    // Mark as done (empty) so a broken file is not retried forever.
                    let _ = sqlx::query(
                        "INSERT INTO file_texts (file_id, version, content) SELECT id, current_version, '' FROM files WHERE id = $1
                         ON CONFLICT (file_id) DO UPDATE SET version = EXCLUDED.version, content = ''",
                    )
                    .bind(id)
                    .execute(&state.db)
                    .await;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_xml() {
        let x = r#"<w:document><w:body><w:p><w:r><w:t>Техническое</w:t></w:r><w:r><w:t xml:space="preserve"> заключение &amp; отчёт</w:t></w:r></w:p><w:p><w:r><w:t>Шифр</w:t><w:tab/><w:t>А0688</w:t></w:r></w:p><w:p><w:r><w:instrText>PAGE</w:instrText></w:r></w:p></w:body></w:document>"#;
        assert_eq!(normalize(&xml_text(x)), "Техническое заключение & отчёт\nШифр А0688");
    }

    #[test]
    fn plain_text_cp1251() {
        let (b, _, _) = encoding_rs::WINDOWS_1251.encode("Ведомость дефектов");
        assert_eq!(extract("txt", &b).unwrap(), "Ведомость дефектов");
        assert_eq!(extract("txt", "\u{feff}Обмеры".as_bytes()).unwrap(), "Обмеры");
    }

    #[test]
    fn entities() {
        assert_eq!(unescape("a &lt;b&gt; &#1046;&#x416; &unknown; x"), "a <b> ЖЖ &unknown; x");
    }

    #[test]
    fn unsupported() {
        assert!(extract("doc", b"x").is_none());
        assert!(!supported("rtf"));
    }
}
