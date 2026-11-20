//! PDF of the chart: A3 or A4 landscape; a table (№, task, assignee, start,
//! end, duration, %) and the time chart side by side; rows that do not fit go
//! to further sheets, a time span that does not fit — to sheets to the right
//! (the table is repeated). Every sheet has a title, the time scale, a legend
//! and the brand stamp (logo, project, names, date, sheet N of M).
//!
//! Coordinates are in millimetres from the top-left corner of the sheet; the
//! helpers flip them for PDF (origin bottom-left).

use std::collections::HashMap;
use std::io::Cursor;

use chrono::{Datelike, Duration, NaiveDate};
use printpdf::{
    image_crate as image,
    path::{PaintMode, WindingOrder},
    Color, IndirectFontRef, Line, LineDashPattern, Mm, PdfDocument, PdfLayerReference, Point, Polygon, Rect, Rgb,
};

use super::{month_name, ru_date, Data, Row, MONTHS_SHORT};

const FONT: &[u8] = include_bytes!("../../assets/fonts/DejaVuSansCondensed.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../../assets/fonts/DejaVuSansCondensed-Bold.ttf");
const PT: f32 = 0.352_778; // mm per point

type C = (u8, u8, u8);
const INK: C = (0x1d, 0x23, 0x2b);
const MUTED: C = (0x6b, 0x75, 0x82);
const LINE: C = (0xd3, 0xd8, 0xde);
const LINE_SOFT: C = (0xe8, 0xeb, 0xef);
const HEAD_BG: C = (0xee, 0xf1, 0xf5);
const OFF_BG: C = (0xf3, 0xf4, 0xf6);
const RED: C = (0xc0, 0x39, 0x2b);
const RED_SOFT: C = (0xf6, 0xd5, 0xd1);
const SUMMARY: C = (0x2b, 0x31, 0x3a);
const BASE: C = (0xa3, 0xab, 0xb5);

fn mix(a: C, b: C, t: f32) -> C {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}
fn color(c: C) -> Color {
    Color::Rgb(Rgb::new(c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0, None))
}

/// Text widths from the font's advances.
struct Metrics {
    regular: ttf_parser::Face<'static>,
    bold: ttf_parser::Face<'static>,
}

impl Metrics {
    fn new() -> Self {
        Self { regular: ttf_parser::Face::parse(FONT, 0).expect("font"), bold: ttf_parser::Face::parse(FONT_BOLD, 0).expect("font") }
    }
    fn width(&self, s: &str, size: f32, bold: bool) -> f32 {
        let f = if bold { &self.bold } else { &self.regular };
        let upem = f.units_per_em() as f32;
        let units: f32 = s.chars().map(|c| f.glyph_index(c).and_then(|g| f.glyph_hor_advance(g)).unwrap_or(upem as u16 / 2) as f32).sum();
        units / upem * size * PT
    }
    /// The text cut to `max` mm with an ellipsis.
    fn fit(&self, s: &str, size: f32, bold: bool, max: f32) -> String {
        if self.width(s, size, bold) <= max {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let t: String = chars[..mid].iter().collect::<String>() + "…";
            if self.width(&t, size, bold) <= max { lo = mid } else { hi = mid - 1 }
        }
        if lo == 0 { String::new() } else { chars[..lo].iter().collect::<String>().trim_end().to_string() + "…" }
    }
}

/// Drawing on one sheet with top-left millimetre coordinates.
struct Canvas<'a> {
    layer: PdfLayerReference,
    h: f32,
    regular: &'a IndirectFontRef,
    bold: &'a IndirectFontRef,
    m: &'a Metrics,
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

impl Canvas<'_> {
    fn p(&self, x: f32, y: f32) -> (Point, bool) {
        (Point::new(Mm(x), Mm(self.h - y)), false)
    }
    fn rect(&self, x: f32, y: f32, w: f32, h: f32, fill: Option<C>, stroke: Option<(C, f32)>) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let mode = match (fill, stroke) {
            (Some(f), Some((s, t))) => {
                self.layer.set_fill_color(color(f));
                self.layer.set_outline_color(color(s));
                self.layer.set_outline_thickness(t / PT);
                PaintMode::FillStroke
            }
            (Some(f), None) => {
                self.layer.set_fill_color(color(f));
                PaintMode::Fill
            }
            (None, Some((s, t))) => {
                self.layer.set_outline_color(color(s));
                self.layer.set_outline_thickness(t / PT);
                PaintMode::Stroke
            }
            (None, None) => return,
        };
        self.layer.add_rect(Rect::new(Mm(x), Mm(self.h - y - h), Mm(x + w), Mm(self.h - y)).with_mode(mode));
    }
    fn line(&self, pts: &[(f32, f32)], c: C, t: f32, dash: bool) {
        self.layer.set_outline_color(color(c));
        self.layer.set_outline_thickness(t / PT);
        if dash {
            self.layer.set_line_dash_pattern(LineDashPattern { dash_1: Some(3), gap_1: Some(2), ..Default::default() });
        }
        self.layer.add_line(Line { points: pts.iter().map(|&(x, y)| self.p(x, y)).collect(), is_closed: false });
        if dash {
            self.layer.set_line_dash_pattern(LineDashPattern::default());
        }
    }
    fn poly(&self, pts: &[(f32, f32)], c: C) {
        self.layer.set_fill_color(color(c));
        self.layer.add_polygon(Polygon {
            rings: vec![pts.iter().map(|&(x, y)| self.p(x, y)).collect()],
            mode: PaintMode::Fill,
            winding_order: WindingOrder::NonZero,
        });
    }
    /// Text with its baseline at `y`; `x` is the left edge, centre or right edge by `align`.
    fn text(&self, s: &str, x: f32, y: f32, size: f32, bold: bool, c: C, align: Align) {
        if s.is_empty() {
            return;
        }
        let w = self.m.width(s, size, bold);
        let x = match align {
            Align::Left => x,
            Align::Center => x - w / 2.0,
            Align::Right => x - w,
        };
        self.layer.set_fill_color(color(c));
        self.layer.use_text(s, size, Mm(x), Mm(self.h - y), if bold { self.bold } else { self.regular });
    }
    /// Text fitted into a box of width `w` (cut with an ellipsis).
    fn cell(&self, s: &str, x: f32, y: f32, w: f32, size: f32, bold: bool, c: C, align: Align) {
        let pad = 1.0;
        let t = self.m.fit(s, size, bold, w - 2.0 * pad);
        let x = match align {
            Align::Left => x + pad,
            Align::Center => x + w / 2.0,
            Align::Right => x + w - pad,
        };
        self.text(&t, x, y, size, bold, c, align);
    }
}

struct Layout {
    w: f32,
    h: f32,
    margin: f32,
    row_h: f32,
    font: f32,
    cols: Vec<(&'static str, f32, Align)>,
    stamp_h: f32,
    ppd_day: f32,
    ppd_week: f32,
    ppd_month: f32,
}

fn layout(paper: &str, unit: &str) -> Layout {
    let dur: &'static str = if unit == "раб. дн." { "Длит., раб. дн." } else { "Длит., дн." };
    if paper == "a4" {
        Layout {
            w: 297.0,
            h: 210.0,
            margin: 8.0,
            row_h: 5.0,
            font: 6.4,
            cols: vec![("№", 7.0, Align::Center), ("Задача", 58.0, Align::Left), ("Исполнитель", 24.0, Align::Left), ("Начало", 12.5, Align::Center), ("Оконча- ние", 12.5, Align::Center), (dur, 11.0, Align::Right), ("%", 7.5, Align::Right)],
            stamp_h: 13.0,
            ppd_day: 4.2,
            ppd_week: 1.2,
            ppd_month: 0.3,
        }
    } else {
        Layout {
            w: 420.0,
            h: 297.0,
            margin: 10.0,
            row_h: 5.8,
            font: 7.4,
            cols: vec![("№", 8.0, Align::Center), ("Задача", 82.0, Align::Left), ("Исполнитель", 32.0, Align::Left), ("Начало", 14.5, Align::Center), ("Оконча- ние", 14.5, Align::Center), (dur, 13.5, Align::Right), ("%", 9.0, Align::Right)],
            stamp_h: 15.0,
            ppd_day: 5.0,
            ppd_week: 1.5,
            ppd_month: 0.36,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Gran {
    Day,
    Week,
    Month,
}

fn month_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), 1).expect("valid")
}
fn next_month(d: NaiveDate) -> NaiveDate {
    let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).expect("valid")
}
fn monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// Time windows (one per sheet column) and millimetres per day.
fn time_plan(data: &Data, l: &Layout, chart_w: f32, scale: &str) -> (Gran, f32, Vec<(NaiveDate, NaiveDate)>) {
    let (lo, hi) = data.span();
    let (lo, hi) = (lo - Duration::days(2), hi + Duration::days(3));
    let days0 = (hi - lo).num_days() as f32 + 1.0;
    let gran = match scale {
        "day" => Gran::Day,
        "week" => Gran::Week,
        "month" => Gran::Month,
        _ => {
            let ppd = chart_w / days0;
            if ppd >= 3.2 { Gran::Day } else if ppd >= 0.75 { Gran::Week } else { Gran::Month }
        }
    };
    let (from, to) = match gran {
        Gran::Day => (lo, hi),
        Gran::Week => (monday(lo), monday(hi) + Duration::days(6)),
        Gran::Month => (month_start(lo), next_month(hi) - Duration::days(1)),
    };
    let days = (to - from).num_days() as f32 + 1.0;
    let fit = chart_w / days;
    let ppd = if scale == "auto" {
        fit
    } else {
        let target = match gran {
            Gran::Day => l.ppd_day,
            Gran::Week => l.ppd_week,
            Gran::Month => l.ppd_month,
        };
        target.max(fit)
    };
    let mut per = (chart_w / ppd).floor().max(1.0) as i64;
    if gran == Gran::Week && per >= 7 {
        per -= per % 7;
    }
    let mut windows = Vec::new();
    let mut s = from;
    while s <= to {
        let e = (s + Duration::days(per - 1)).min(to);
        windows.push((s, e));
        s = e + Duration::days(1);
    }
    (gran, ppd, windows)
}

/// Split a header into at most two lines by words.
fn wrap2(m: &Metrics, s: &str, size: f32, max: f32) -> Vec<String> {
    if m.width(s, size, true) <= max || !s.contains(' ') {
        return vec![s.to_string()];
    }
    let words: Vec<&str> = s.split(' ').collect();
    let mut best = vec![s.to_string()];
    let mut best_w = f32::MAX;
    for i in 1..words.len() {
        let (a, b) = (words[..i].join(" "), words[i..].join(" "));
        let w = m.width(&a, size, true).max(m.width(&b, size, true));
        if w < best_w {
            best_w = w;
            best = vec![a, b];
        }
    }
    best
}

pub fn write(data: &Data, paper: &str, scale: &str) -> anyhow::Result<Vec<u8>> {
    let l = layout(paper, data.unit());
    let m = Metrics::new();
    let table_w: f32 = l.cols.iter().map(|c| c.1).sum();
    let left = l.margin;
    let chart_x = left + table_w;
    let chart_w = l.w - 2.0 * l.margin - table_w;
    let title_h = 12.0;
    let head_h = 9.0;
    let legend_h = 5.0;
    let top = l.margin + title_h;
    let rows_top = top + head_h;
    let stamp_y = l.h - l.margin - l.stamp_h;
    let rows_bottom = stamp_y - legend_h - 1.0;
    let per_page = (((rows_bottom - rows_top) / l.row_h).floor() as usize).max(1);
    let (gran, ppd, windows) = time_plan(data, &l, chart_w, scale);
    let chunks: Vec<&[Row]> = data.rows.chunks(per_page).collect();
    let total = chunks.len() * windows.len();
    let row_no: HashMap<uuid::Uuid, usize> = data.rows.iter().enumerate().map(|(i, r)| (r.id, i)).collect();

    let title = format!("Календарный график работ — {}", data.title_line());
    let (doc, page1, layer1) = PdfDocument::new(&title, Mm(l.w), Mm(l.h), "График");
    let doc = doc.with_conformance(printpdf::PdfConformance::Custom(printpdf::CustomPdfConformance {
        requires_icc_profile: false,
        requires_xmp_metadata: false,
        ..Default::default()
    }));
    let regular = doc.add_external_font_with_subsetting(Cursor::new(FONT), true)?;
    let bold = doc.add_external_font_with_subsetting(Cursor::new(FONT_BOLD), true)?;
    let accent: C = data.accent;
    let bar_fill = mix(accent, (255, 255, 255), 0.72);

    let mut sheet = 0usize;
    for (ci, rows) in chunks.iter().enumerate() {
        for &(w0, w1) in &windows {
            sheet += 1;
            let layer = if sheet == 1 {
                doc.get_page(page1).get_layer(layer1)
            } else {
                let (p, ly) = doc.add_page(Mm(l.w), Mm(l.h), "График");
                doc.get_page(p).get_layer(ly)
            };
            let cv = Canvas { layer, h: l.h, regular: &regular, bold: &bold, m: &m };
            let x_of = |d: NaiveDate| chart_x + (d - w0).num_days() as f32 * ppd;
            let win_days = (w1 - w0).num_days() + 1;
            let chart_end = chart_x + win_days as f32 * ppd;
            let body_h = rows.len() as f32 * l.row_h;

            // --- Title ------------------------------------------------------------
            cv.text("КАЛЕНДАРНЫЙ ГРАФИК РАБОТ", left, l.margin + 4.2, if paper == "a4" { 10.0 } else { 11.5 }, true, INK, Align::Left);
            cv.text(&m.fit(&data.title_line(), 8.0, false, l.w - 2.0 * l.margin - 80.0), left, l.margin + 9.2, 8.0, false, MUTED, Align::Left);
            cv.text(&format!("Состояние на {}", ru_date(data.today)), l.w - l.margin, l.margin + 4.2, 7.5, false, INK, Align::Right);
            let mut legend_bits = vec![if data.day_mode == "work" { "длительность — в рабочих днях" } else { "длительность — в календарных днях" }];
            if windows.len() > 1 {
                legend_bits.push("шкала продолжается на соседних листах");
            }
            cv.text(&legend_bits.join(" · "), l.w - l.margin, l.margin + 9.2, 6.5, false, MUTED, Align::Right);

            // --- Table header --------------------------------------------------------
            cv.rect(left, top, table_w, head_h, Some(HEAD_BG), None);
            let mut x = left;
            for (name, w, _) in &l.cols {
                let hs = l.font - 0.4;
                let lines = wrap2(&m, name, hs, *w - 1.6);
                let lh = hs * PT * 1.15;
                let y0 = top + head_h / 2.0 + hs * PT * 0.35 - lh * (lines.len() as f32 - 1.0) / 2.0;
                for (k, ln) in lines.iter().enumerate() {
                    cv.cell(ln, x, y0 + k as f32 * lh, *w, hs, true, INK, Align::Center);
                }
                x += w;
                cv.line(&[(x, top), (x, rows_top + body_h)], LINE, 0.15, false);
            }

            // --- Time header ------------------------------------------------------------
            cv.rect(chart_x, top, chart_end - chart_x, head_h, Some(HEAD_BG), None);
            let band = head_h / 2.0;
            let mut upper: Vec<(NaiveDate, NaiveDate, String)> = Vec::new();
            let mut lower: Vec<(NaiveDate, NaiveDate, String)> = Vec::new();
            match gran {
                Gran::Day | Gran::Week => {
                    let mut s = w0;
                    while s <= w1 {
                        let e = (next_month(s) - Duration::days(1)).min(w1);
                        upper.push((s, e, format!("{} {}", month_name(s), s.year())));
                        s = e + Duration::days(1);
                    }
                    if gran == Gran::Day {
                        let mut d = w0;
                        while d <= w1 {
                            lower.push((d, d, d.day().to_string()));
                            d += Duration::days(1);
                        }
                    } else {
                        let mut d = monday(w0);
                        while d <= w1 {
                            let s = d.max(w0);
                            let e = (d + Duration::days(6)).min(w1);
                            lower.push((s, e, d.format("%d.%m").to_string()));
                            d += Duration::days(7);
                        }
                    }
                }
                Gran::Month => {
                    let mut s = w0;
                    while s <= w1 {
                        let e = (NaiveDate::from_ymd_opt(s.year(), 12, 31).expect("valid")).min(w1);
                        upper.push((s, e, s.year().to_string()));
                        s = e + Duration::days(1);
                    }
                    let mut s = w0;
                    while s <= w1 {
                        let e = (next_month(s) - Duration::days(1)).min(w1);
                        lower.push((s, e, MONTHS_SHORT[s.month0() as usize].to_string()));
                        s = e + Duration::days(1);
                    }
                }
            }
            let hs = l.font - 0.8;
            for (s, e, t) in &upper {
                let (x0, x1) = (x_of(*s), x_of(*e) + ppd);
                cv.line(&[(x0, top), (x0, top + band)], LINE, 0.15, false);
                cv.cell(t, x0, top + band - 1.2, x1 - x0, hs, true, INK, Align::Left);
            }
            for (s, e, t) in &lower {
                let (x0, x1) = (x_of(*s), x_of(*e) + ppd);
                if gran == Gran::Day && !data.is_work(*s) {
                    cv.rect(x0, top + band, x1 - x0, band, Some(mix(HEAD_BG, LINE, 0.6)), None);
                }
                if m.width(t, hs - 0.6, false) <= x1 - x0 - 0.6 {
                    cv.text(t, (x0 + x1) / 2.0, top + head_h - 1.3, hs - 0.6, false, MUTED, Align::Center);
                }
            }
            cv.line(&[(chart_x, top + band), (chart_end, top + band)], LINE, 0.15, false);

            // --- Off days and grid ---------------------------------------------------------
            if gran == Gran::Day || (gran == Gran::Week && ppd >= 1.1) {
                let mut d = w0;
                while d <= w1 {
                    if !data.is_work(d) {
                        cv.rect(x_of(d), rows_top, ppd, body_h, Some(OFF_BG), None);
                    }
                    d += Duration::days(1);
                }
            }
            for (s, _, _) in &lower {
                cv.line(&[(x_of(*s), rows_top), (x_of(*s), rows_top + body_h)], LINE_SOFT, 0.12, false);
            }

            // --- Rows ------------------------------------------------------------------------
            let base_index = ci * per_page;
            let mut ys: HashMap<uuid::Uuid, f32> = HashMap::new();
            for (i, r) in rows.iter().enumerate() {
                let y = rows_top + i as f32 * l.row_h;
                let mid = y + l.row_h / 2.0;
                ys.insert(r.id, mid);
                let base_y = y + l.row_h / 2.0 + l.font * PT * 0.36;
                let mut x = left;
                let n = (base_index + i + 1).to_string();
                let dur = data.duration(r).map(|d| d.to_string()).unwrap_or_default();
                let fmt = |d: Option<NaiveDate>| d.map(|d| d.format("%d.%m.%y").to_string()).unwrap_or_default();
                let cells = [
                    n,
                    r.title.clone(),
                    r.assignee.clone().unwrap_or_default(),
                    fmt(r.start),
                    if r.milestone { String::new() } else { fmt(r.end) },
                    dur,
                    if r.milestone { String::new() } else { r.progress.to_string() },
                ];
                for (k, (_, w, align)) in l.cols.iter().enumerate() {
                    if k == 1 {
                        let indent = r.depth as f32 * 2.6;
                        let label = if r.milestone { format!("◆ {}", r.title) } else { r.title.clone() };
                        let c = if r.done { MUTED } else { INK };
                        cv.cell(&label, x + indent, base_y, w - indent, l.font, r.is_parent, c, *align);
                    } else {
                        cv.cell(&cells[k], x, base_y, *w, l.font, false, if k == 0 { MUTED } else { INK }, *align);
                    }
                    x += w;
                }
                cv.line(&[(left, y + l.row_h), (chart_end, y + l.row_h)], LINE_SOFT, 0.12, false);

                // Bars (clipped to this sheet's window).
                let (Some(s), Some(e)) = (r.start, r.end) else { continue };
                let crit = data.show_critical && r.critical && !r.done;
                if data.show_baseline {
                    if let Some((bs, be)) = r.base {
                        if be >= w0 && bs <= w1 {
                            let (x0, x1) = (x_of(bs.max(w0)), x_of(be.min(w1)) + ppd);
                            cv.rect(x0, y + l.row_h - 1.35, (x1 - x0).max(0.6), 0.9, Some(BASE), None);
                        }
                    }
                }
                if e < w0 || s > w1 {
                    continue;
                }
                let bh = l.row_h * 0.5;
                let by = y + (l.row_h - bh) / 2.0 - if data.show_baseline && r.base.is_some() { 0.5 } else { 0.0 };
                if r.milestone {
                    let cx = x_of(s) + ppd / 2.0;
                    let d = l.row_h * 0.32;
                    let c = if crit { RED } else if r.done { MUTED } else { INK };
                    cv.poly(&[(cx, mid - d), (cx + d, mid), (cx, mid + d), (cx - d, mid)], c);
                    continue;
                }
                let (x0, x1) = (x_of(s.max(w0)), x_of(e.min(w1)) + ppd);
                if r.is_parent {
                    let th = l.row_h * 0.22;
                    let ty = mid - th / 2.0;
                    let c = if crit { RED } else { SUMMARY };
                    cv.rect(x0, ty, x1 - x0, th, Some(c), None);
                    let tri = th * 1.6;
                    if s >= w0 {
                        cv.poly(&[(x0, ty), (x0 + tri, ty), (x0, ty + th + tri)], c);
                    }
                    if e <= w1 {
                        cv.poly(&[(x1, ty), (x1 - tri, ty), (x1, ty + th + tri)], c);
                    }
                    continue;
                }
                let (fill, edge) = if crit { (RED_SOFT, RED) } else if r.done { (mix(accent, (255, 255, 255), 0.55), accent) } else { (bar_fill, accent) };
                cv.rect(x0, by, (x1 - x0).max(0.8), bh, Some(fill), Some((edge, 0.2)));
                if r.progress > 0 {
                    // Done share of the whole bar, clipped to this window.
                    let fx0 = x_of(s);
                    let fx1 = fx0 + (x_of(e) + ppd - fx0) * r.progress as f32 / 100.0;
                    let (a, b) = (fx0.max(x0), fx1.min(x1));
                    if b > a {
                        cv.rect(a, by + bh * 0.25, b - a, bh * 0.5, Some(edge), None);
                    }
                }
            }

            // --- Links ------------------------------------------------------------------------
            for lk in &data.links {
                let (Some(&ty), Some(&sy)) = (ys.get(&lk.task), ys.get(&lk.pred)) else { continue };
                let (Some(t), Some(p)) = (row_no.get(&lk.task).map(|&i| &data.rows[i]), row_no.get(&lk.pred).map(|&i| &data.rows[i])) else { continue };
                let (Some(ps), Some(pe), Some(ts), Some(te)) = (p.start, p.end, t.start, t.end) else { continue };
                let from_end = matches!(lk.kind.as_str(), "FS" | "FF");
                let to_start = matches!(lk.kind.as_str(), "FS" | "SS");
                let sd = if from_end { pe } else { ps };
                let td = if to_start { ts } else { te };
                if sd < w0 || sd > w1 || td < w0 || td > w1 {
                    continue;
                }
                let edge = |d: NaiveDate, end: bool, ms: bool| if ms { x_of(d) + ppd / 2.0 } else if end { x_of(d) + ppd } else { x_of(d) };
                let sx = edge(sd, from_end, p.milestone);
                let tx = edge(td, !to_start, t.milestone);
                let out = if from_end { 1.0 } else { -1.0 };
                let inn = if to_start { 1.0 } else { -1.0 };
                let gap = 1.4;
                let p1 = (sx + out * gap, sy);
                let pre = (tx - inn * gap, ty);
                let pts: Vec<(f32, f32)> = if out > 0.0 && inn > 0.0 && pre.0 >= p1.0 {
                    vec![(sx, sy), (p1.0, sy), (p1.0, ty), (tx, ty)]
                } else {
                    let my = if ty > sy { sy + l.row_h / 2.0 } else { sy - l.row_h / 2.0 };
                    vec![(sx, sy), p1, (p1.0, my), (pre.0, my), pre, (tx, ty)]
                };
                let c = if data.show_critical && t.critical && p.critical { RED } else { MUTED };
                cv.line(&pts, c, 0.22, false);
                let a = 0.9;
                cv.poly(&[(tx, ty), (tx - inn * a * 1.3, ty - a * 0.7), (tx - inn * a * 1.3, ty + a * 0.7)], c);
            }

            // --- Today ------------------------------------------------------------------------
            if data.today >= w0 && data.today <= w1 {
                let tx = x_of(data.today) + ppd / 2.0;
                cv.line(&[(tx, rows_top), (tx, rows_top + body_h)], RED, 0.3, true);
            }

            // --- Frame -----------------------------------------------------------------------
            cv.line(&[(chart_x, top), (chart_x, rows_top + body_h)], LINE, 0.3, false);
            cv.line(&[(left, rows_top), (chart_end, rows_top)], LINE, 0.25, false);
            cv.rect(left, top, chart_end - left, head_h + body_h, None, Some((mix(LINE, INK, 0.35), 0.3)));

            // --- Legend ----------------------------------------------------------------------
            let ly = stamp_y - 2.3;
            let mut lx = left;
            let ls = 6.2;
            let item = |cv: &Canvas, lx: &mut f32, draw: &dyn Fn(&Canvas, f32), label: &str| {
                draw(cv, *lx);
                cv.text(label, *lx + 7.0, ly + 0.9, ls, false, MUTED, Align::Left);
                *lx += 7.0 + m.width(label, ls, false) + 5.0;
            };
            item(&cv, &mut lx, &|cv, x| {
                cv.rect(x, ly - 1.2, 5.5, 2.4, Some(bar_fill), Some((accent, 0.2)));
                cv.rect(x, ly - 0.6, 2.8, 1.2, Some(accent), None);
            }, "задача и выполнение");
            item(&cv, &mut lx, &|cv, x| {
                let c = x + 2.7;
                cv.poly(&[(c, ly - 1.3), (c + 1.3, ly), (c, ly + 1.3), (c - 1.3, ly)], INK);
            }, "веха");
            item(&cv, &mut lx, &|cv, x| cv.rect(x, ly - 0.6, 5.5, 1.2, Some(SUMMARY), None), "группа задач");
            if data.show_critical {
                item(&cv, &mut lx, &|cv, x| cv.rect(x, ly - 1.2, 5.5, 2.4, Some(RED_SOFT), Some((RED, 0.2))), "критический путь");
            }
            if data.show_baseline {
                item(&cv, &mut lx, &|cv, x| cv.rect(x, ly - 0.45, 5.5, 0.9, Some(BASE), None), "базовый план");
            }
            item(&cv, &mut lx, &|cv, x| cv.line(&[(x + 2.7, ly - 1.4), (x + 2.7, ly + 1.4)], RED, 0.3, true), "сегодня");
            item(&cv, &mut lx, &|cv, x| cv.rect(x, ly - 1.2, 5.5, 2.4, Some(OFF_BG), Some((LINE, 0.15))), "выходные и праздники");

            // --- Stamp -------------------------------------------------------------------------
            stamp(&cv, data, &l, stamp_y, sheet, total)?;
        }
    }
    Ok(doc.save_to_bytes()?)
}

fn stamp(cv: &Canvas, data: &Data, l: &Layout, y: f32, sheet: usize, total: usize) -> anyhow::Result<()> {
    let x = l.margin;
    let w = l.w - 2.0 * l.margin;
    let h = l.stamp_h;
    let a4 = l.w < 300.0;
    let logo_w = if a4 { 40.0 } else { 52.0 };
    let person_w = if a4 { 30.0 } else { 38.0 };
    let date_w = if a4 { 20.0 } else { 24.0 };
    let sheet_w = if a4 { 22.0 } else { 26.0 };
    let object_w = w - logo_w - 3.0 * person_w - date_w - sheet_w;
    cv.rect(x, y, w, h, Some((255, 255, 255)), Some((mix(LINE, INK, 0.35), 0.3)));
    cv.rect(x, y, w, 0.9, Some(data.accent), None);

    // Logo or the portal name.
    let pad = 2.0;
    let mut drawn = false;
    if let Some((bytes, mime)) = &data.logo {
        drawn = draw_logo(cv, bytes, mime, x + pad, y + 1.6, logo_w - 2.0 * pad, h - 2.6).unwrap_or(false);
    }
    if !drawn {
        cv.cell(&data.portal_name, x, y + h / 2.0 + 1.6, logo_w, 9.0, true, data.accent, Align::Left);
    }
    let mut cx = x + logo_w;
    let label_y = y + 4.6;
    let value_y = y + h - 3.0;
    let ls = 5.8;
    let vs = if a4 { 7.0 } else { 7.8 };
    let sep = |cx: f32| cv.line(&[(cx, y + 0.9), (cx, y + h)], LINE, 0.2, false);

    sep(cx);
    cv.cell(if data.code.is_empty() { "Проект" } else { "Шифр · проект" }, cx, label_y, object_w, ls, false, MUTED, Align::Left);
    let obj = data.title_line();
    // Up to two lines for the object name.
    let words: Vec<&str> = obj.split_whitespace().collect();
    let mut line1 = String::new();
    let mut rest_at = words.len();
    for (i, wd) in words.iter().enumerate() {
        let t = if line1.is_empty() { wd.to_string() } else { format!("{line1} {wd}") };
        if cv.m.width(&t, vs, true) > object_w - 2.0 {
            rest_at = i;
            break;
        }
        line1 = t;
    }
    if rest_at < words.len() {
        cv.cell(&line1, cx, value_y - 3.4, object_w, vs, true, INK, Align::Left);
        cv.cell(&words[rest_at..].join(" "), cx, value_y, object_w, vs, true, INK, Align::Left);
    } else {
        cv.cell(&line1, cx, value_y, object_w, vs, true, INK, Align::Left);
    }
    cx += object_w;
    for (label, name) in [("Разработал", &data.stamp.developer), ("Проверил", &data.stamp.checker), ("ГИП", &data.stamp.gip)] {
        sep(cx);
        cv.cell(label, cx, label_y, person_w, ls, false, MUTED, Align::Left);
        cv.cell(name, cx, value_y, person_w, vs, false, INK, Align::Left);
        // Room for a signature.
        cv.line(&[(cx + 1.0, value_y + 1.2), (cx + person_w - 1.0, value_y + 1.2)], LINE_SOFT, 0.15, false);
        cx += person_w;
    }
    sep(cx);
    cv.cell("Дата", cx, label_y, date_w, ls, false, MUTED, Align::Left);
    cv.cell(&ru_date(data.today), cx, value_y, date_w, vs, false, INK, Align::Left);
    cx += date_w;
    sep(cx);
    cv.cell("Лист", cx, label_y, sheet_w, ls, false, MUTED, Align::Left);
    cv.cell(&format!("{sheet} из {total}"), cx, value_y, sheet_w, vs + 0.6, true, INK, Align::Left);
    Ok(())
}

/// Fit a PNG, JPEG or SVG logo into a box. Returns false when it cannot be drawn.
fn draw_logo(cv: &Canvas, bytes: &[u8], mime: &str, x: f32, y: f32, w: f32, h: f32) -> anyhow::Result<bool> {
    if mime == "image/svg+xml" {
        let svg = printpdf::Svg::parse(std::str::from_utf8(bytes)?).map_err(|e| anyhow::anyhow!("{e:?}"))?;
        let r = svg.into_xobject(&cv.layer);
        // At 300 dpi the SVG's pixel size gives its natural size in mm.
        let (nw, nh) = (r.width.0 as f32 / 300.0 * 25.4, r.height.0 as f32 / 300.0 * 25.4);
        if nw <= 0.0 || nh <= 0.0 {
            return Ok(false);
        }
        let k = (w / nw).min(h / nh);
        let (_dw, dh) = (nw * k, nh * k);
        r.add_to_layer(
            &cv.layer,
            printpdf::SvgTransform {
                translate_x: Some(Mm(x).into()),
                translate_y: Some(Mm(cv.h - y - (h + dh) / 2.0).into()),
                scale_x: Some(k),
                scale_y: Some(k),
                dpi: Some(300.0),
                ..Default::default()
            },
        );
        return Ok(true);
    }
    let img = image::load_from_memory(bytes)?;
    // Flatten transparency onto white: PDF images here have no alpha.
    let rgba = img.to_rgba8();
    let mut rgb = image::RgbImage::new(rgba.width(), rgba.height());
    for (px, out) in rgba.pixels().zip(rgb.pixels_mut()) {
        let a = px[3] as f32 / 255.0;
        *out = image::Rgb([0, 1, 2].map(|i| (px[i] as f32 * a + 255.0 * (1.0 - a)).round() as u8));
    }
    let (pw, ph) = (rgb.width() as f32, rgb.height() as f32);
    if pw < 1.0 || ph < 1.0 {
        return Ok(false);
    }
    // dpi so that the image fits the box.
    let dpi = (pw * 25.4 / w).max(ph * 25.4 / h);
    let (dw, dh) = (pw / dpi * 25.4, ph / dpi * 25.4);
    let _ = dw;
    let image = printpdf::Image::from_dynamic_image(&image::DynamicImage::ImageRgb8(rgb));
    image.add_to_layer(
        cv.layer.clone(),
        printpdf::ImageTransform { translate_x: Some(Mm(x)), translate_y: Some(Mm(cv.h - y - (h + dh) / 2.0)), dpi: Some(dpi), ..Default::default() },
    );
    Ok(true)
}
