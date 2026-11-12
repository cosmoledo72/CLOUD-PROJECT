//! XLSX of the chart: «График» — the task table and the chart as shaded cells
//! by day, week or month (headers and the table frozen, printed on A3
//! landscape one page wide); «Задачи» — a flat list with predecessors in
//! MS Project notation, for filtering and further work.

use std::collections::HashMap;

use chrono::{Datelike, Duration, NaiveDate};
use rust_xlsxwriter::{Color, ExcelDateTime, Format, FormatAlign, FormatBorder, Workbook, XlsxError};

use super::{link_label, month_name, Data, MONTHS_SHORT};

#[derive(Clone, Copy, PartialEq)]
enum Gran {
    Day,
    Week,
    Month,
}

fn rgb(c: (u8, u8, u8)) -> Color {
    Color::RGB(((c.0 as u32) << 16) | ((c.1 as u32) << 8) | c.2 as u32)
}
fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}
fn xdate(d: NaiveDate) -> Result<ExcelDateTime, XlsxError> {
    ExcelDateTime::from_ymd(d.year() as u16, d.month() as u8, d.day() as u8)
}
fn month_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid")
}
fn next_month(d: NaiveDate) -> NaiveDate {
    let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).expect("valid")
}

pub fn write(data: &Data, scale: &str) -> Result<Vec<u8>, XlsxError> {
    let mut wb = Workbook::new();
    let accent = data.accent;
    let white = (255, 255, 255);
    let border = Color::RGB(0xD3D8DE);

    let title = Format::new().set_bold().set_font_size(14);
    let sub = Format::new().set_font_color(Color::RGB(0x6B7582));
    let head = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0xEEF1F5))
        .set_border(FormatBorder::Thin)
        .set_border_color(border)
        .set_align(FormatAlign::Center)
        .set_align(FormatAlign::VerticalCenter)
        .set_text_wrap();
    let cell = Format::new().set_border(FormatBorder::Hair).set_border_color(border).set_align(FormatAlign::VerticalCenter);
    let center = cell.clone().set_align(FormatAlign::Center);
    let date_f = center.clone().set_num_format("dd.mm.yy");
    let num_f = cell.clone().set_align(FormatAlign::Right);

    // --- Time buckets --------------------------------------------------------------
    let (lo, hi) = data.span();
    let days = (hi - lo).num_days() + 1;
    let gran = match scale {
        "day" => Gran::Day,
        "week" => Gran::Week,
        "month" => Gran::Month,
        _ if days <= 92 => Gran::Day,
        _ if days <= 3 * 365 => Gran::Week,
        _ => Gran::Month,
    };
    let mut buckets: Vec<(NaiveDate, NaiveDate)> = Vec::new();
    match gran {
        Gran::Day => {
            let mut d = lo - Duration::days(1);
            while d <= hi + Duration::days(2) {
                buckets.push((d, d));
                d += Duration::days(1);
            }
        }
        Gran::Week => {
            let mut d = lo - Duration::days(lo.weekday().num_days_from_monday() as i64);
            while d <= hi {
                buckets.push((d, d + Duration::days(6)));
                d += Duration::days(7);
            }
        }
        Gran::Month => {
            let mut d = month_start(lo);
            while d <= hi {
                let n = next_month(d);
                buckets.push((d, n - Duration::days(1)));
                d = n;
            }
        }
    }

    // --- «График» ----------------------------------------------------------------------
    let ws = wb.add_worksheet().set_name("График")?;
    let unit = if data.day_mode == "work" { "Длит., раб. дн." } else { "Длит., дн." };
    let cols: [(&str, f64); 7] = [("№", 5.0), ("Задача", 46.0), ("Исполнитель", 20.0), ("Начало", 9.5), ("Окончание", 9.5), (unit, 8.0), ("%", 5.5)];
    let first_t = cols.len() as u16;
    ws.write_string_with_format(0, 0, "Календарный график работ", &title)?;
    ws.write_string_with_format(1, 0, data.title_line(), &sub)?;
    ws.write_string_with_format(
        2,
        0,
        format!("Состояние на {} · {} · {}", super::ru_date(data.today), data.portal_name, data.exported_by),
        &sub,
    )?;
    let (h1, h2) = (3u32, 4u32);
    for (i, (name, w)) in cols.iter().enumerate() {
        ws.merge_range(h1, i as u16, h2, i as u16, name, &head)?;
        ws.set_column_width(i as u16, *w)?;
    }
    // Upper band: months (years for the month scale), lower: days / weeks / months.
    let mut groups: Vec<(u16, u16, String)> = Vec::new();
    for (i, (s, _)) in buckets.iter().enumerate() {
        let key = if gran == Gran::Month { s.year().to_string() } else { format!("{} {}", month_name(*s), s.year()) };
        let col = first_t + i as u16;
        match groups.last_mut() {
            Some(g) if g.2 == key => g.1 = col,
            _ => groups.push((col, col, key)),
        }
    }
    for (a, b, t) in &groups {
        if a == b {
            ws.write_string_with_format(h1, *a, t, &head)?;
        } else {
            ws.merge_range(h1, *a, h1, *b, t, &head)?;
        }
    }
    let off_col = Format::new().set_background_color(Color::RGB(0xF1F2F4)).set_border(FormatBorder::Hair).set_border_color(border);
    let head_off = head.clone().set_background_color(Color::RGB(0xDDE1E7));
    let head_today = head.clone().set_background_color(Color::RGB(0xF6D5D1)).set_font_color(Color::RGB(0xC0392B));
    let col_w = match gran {
        Gran::Day => 3.0,
        Gran::Week => 5.2,
        Gran::Month => 5.0,
    };
    for (i, (s, e)) in buckets.iter().enumerate() {
        let col = first_t + i as u16;
        let label = match gran {
            Gran::Day => s.day().to_string(),
            Gran::Week => s.format("%d.%m").to_string(),
            Gran::Month => MONTHS_SHORT[s.month0() as usize].to_string(),
        };
        let is_today = data.today >= *s && data.today <= *e;
        let off = gran == Gran::Day && !data.is_work(*s);
        let f = if is_today { &head_today } else if off { &head_off } else { &head };
        ws.write_string_with_format(h2, col, &label, f)?;
        ws.set_column_width(col, col_w)?;
        if off {
            ws.set_column_format(col, &off_col)?;
        }
    }
    ws.set_row_height(h2, 24)?;

    let bar = Format::new().set_background_color(rgb(mix(accent, white, 0.7))).set_border(FormatBorder::Hair).set_border_color(rgb(mix(accent, white, 0.5)));
    let bar_done = Format::new().set_background_color(rgb(accent)).set_border(FormatBorder::Hair).set_border_color(rgb(accent));
    let crit = Format::new().set_background_color(Color::RGB(0xF6D5D1)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xE3A39B));
    let crit_done = Format::new().set_background_color(Color::RGB(0xC0392B)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0xC0392B));
    let group = Format::new().set_background_color(Color::RGB(0x2B313A)).set_border(FormatBorder::Hair).set_border_color(Color::RGB(0x2B313A));
    let ms = Format::new().set_align(FormatAlign::Center).set_bold().set_font_color(Color::RGB(0x1D232B));
    let ms_crit = ms.clone().set_font_color(Color::RGB(0xC0392B));

    let first_row = h2 + 1;
    for (i, r) in data.rows.iter().enumerate() {
        let row = first_row + i as u32;
        ws.write_number_with_format(row, 0, (i + 1) as f64, &center)?;
        let mut tf = cell.clone().set_indent(r.depth.min(15) as u8);
        if r.is_parent {
            tf = tf.set_bold();
        }
        if r.done {
            tf = tf.set_font_color(Color::RGB(0x6B7582));
        }
        let name = if r.milestone { format!("◆ {}", r.title) } else { r.title.clone() };
        ws.write_string_with_format(row, 1, &name, &tf)?;
        ws.write_string_with_format(row, 2, r.assignee.as_deref().unwrap_or(""), &cell)?;
        match r.start {
            Some(d) => ws.write_datetime_with_format(row, 3, &xdate(d)?, &date_f)?,
            None => ws.write_blank(row, 3, &date_f)?,
        };
        match r.end.filter(|_| !r.milestone) {
            Some(d) => ws.write_datetime_with_format(row, 4, &xdate(d)?, &date_f)?,
            None => ws.write_blank(row, 4, &date_f)?,
        };
        match data.duration(r) {
            Some(d) => ws.write_number_with_format(row, 5, d as f64, &num_f)?,
            None => ws.write_blank(row, 5, &num_f)?,
        };
        if r.milestone {
            ws.write_blank(row, 6, &num_f)?;
        } else {
            ws.write_number_with_format(row, 6, r.progress as f64, &num_f)?;
        }
        let (Some(s), Some(e)) = (r.start, r.end) else { continue };
        let c = data.show_critical && r.critical && !r.done;
        let hit: Vec<usize> = buckets.iter().enumerate().filter(|(_, (bs, be))| *be >= s && *bs <= e).map(|(k, _)| k).collect();
        if r.milestone {
            if let Some(&k) = hit.first() {
                ws.write_string_with_format(row, first_t + k as u16, "◆", if c { &ms_crit } else { &ms })?;
            }
            continue;
        }
        let done_n = ((hit.len() as f32) * r.progress as f32 / 100.0).round() as usize;
        for (j, &k) in hit.iter().enumerate() {
            let f = if r.is_parent { &group } else if j < done_n { if c { &crit_done } else { &bar_done } } else if c { &crit } else { &bar };
            ws.write_blank(row, first_t + k as u16, f)?;
        }
    }
    let last_row = first_row + data.rows.len().max(1) as u32 - 1;
    let last_col = first_t + buckets.len().max(1) as u16 - 1;
    ws.set_freeze_panes(first_row, first_t)?;
    ws.set_landscape();
    ws.set_paper_size(8); // A3
    ws.set_print_fit_to_pages(1, 0);
    ws.set_repeat_rows(h1, h2)?;
    ws.set_repeat_columns(0, first_t - 1)?;
    ws.set_print_area(0, 0, last_row, last_col)?;
    ws.set_header(&format!("&L&8{}&R&8Лист &P из &N", data.title_line().replace('&', "&&")));

    // --- «Задачи» ----------------------------------------------------------------------
    let ws = wb.add_worksheet().set_name("Задачи")?;
    let index: HashMap<uuid::Uuid, usize> = data.rows.iter().enumerate().map(|(i, r)| (r.id, i + 1)).collect();
    let mut preds: HashMap<uuid::Uuid, Vec<String>> = HashMap::new();
    for l in &data.links {
        if let Some(&n) = index.get(&l.pred) {
            preds.entry(l.task).or_default().push(link_label(n, &l.kind, l.lag));
        }
    }
    let heads = [
        ("№", 5.0),
        ("СДР", 8.0),
        ("Номер", 8.0),
        ("Задача", 46.0),
        ("Тип", 9.0),
        ("Начало", 11.0),
        ("Окончание", 11.0),
        (unit, 9.0),
        ("Исполнитель", 20.0),
        ("%", 6.0),
        ("Предшественники", 16.0),
        ("Критическая", 11.0),
        ("Базовое начало", 11.0),
        ("Базовое окончание", 11.0),
        ("Описание", 50.0),
    ];
    for (i, (h, w)) in heads.iter().enumerate() {
        ws.write_string_with_format(0, i as u16, *h, &head)?;
        ws.set_column_width(i as u16, *w)?;
    }
    let date2 = Format::new().set_num_format("dd.mm.yyyy");
    for (i, r) in data.rows.iter().enumerate() {
        let row = i as u32 + 1;
        ws.write_number(row, 0, (i + 1) as f64)?;
        ws.write_string(row, 1, &r.wbs)?;
        ws.write_string(row, 2, format!("#{}", r.number))?;
        let mut tf = Format::new().set_indent(r.depth.min(15) as u8);
        if r.is_parent {
            tf = tf.set_bold();
        }
        ws.write_string_with_format(row, 3, &r.title, &tf)?;
        ws.write_string(row, 4, if r.milestone { "веха" } else if r.is_parent { "группа" } else { "задача" })?;
        if let Some(d) = r.start {
            ws.write_datetime_with_format(row, 5, &xdate(d)?, &date2)?;
        }
        if let Some(d) = r.end {
            ws.write_datetime_with_format(row, 6, &xdate(d)?, &date2)?;
        }
        if let Some(d) = data.duration(r) {
            ws.write_number(row, 7, d as f64)?;
        }
        ws.write_string(row, 8, r.assignee.as_deref().unwrap_or(""))?;
        ws.write_number(row, 9, r.progress as f64)?;
        ws.write_string(row, 10, preds.get(&r.id).map(|v| v.join("; ")).unwrap_or_default())?;
        ws.write_string(row, 11, if r.critical { "да" } else { "" })?;
        if let Some((a, b)) = r.base {
            ws.write_datetime_with_format(row, 12, &xdate(a)?, &date2)?;
            ws.write_datetime_with_format(row, 13, &xdate(b)?, &date2)?;
        }
        ws.write_string(row, 14, r.description.chars().take(2000).collect::<String>())?;
    }
    ws.set_freeze_panes(1, 0)?;
    if !data.rows.is_empty() {
        ws.autofilter(0, 0, data.rows.len() as u32, (heads.len() - 1) as u16)?;
    }
    wb.save_to_buffer()
}
