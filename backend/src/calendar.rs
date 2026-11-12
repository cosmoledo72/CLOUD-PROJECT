//! Working calendar (production calendar of the Russian Federation).
//!
//! A day is working when it is Mon–Fri, unless the calendar says otherwise:
//! `holiday` and `off` days are non-working, `work` and `short` days are
//! working (a `short` day is one hour shorter — used by time tracking).
//! The shipped official calendar (`calendar_days`) is overlaid by the
//! portal's own changes (`tenant_calendar_days`).
//!
//! [`WorkCalendar`] maps dates to working-day ordinals so the scheduler can
//! do arithmetic in working days; in "calendar days" mode every day counts.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{bad_request, ApiResult},
    state::AppState,
};

/// First and last dates the working-day index covers. Outside the window the
/// scheduler falls back to the edge (no project plans a century ahead).
const WINDOW_FROM: (i32, u32, u32) = (1990, 1, 1);
const WINDOW_TO: (i32, u32, u32) = (2150, 12, 31);

/// Is this kind of special day a working day?
pub fn kind_is_work(kind: &str) -> bool {
    matches!(kind, "work" | "short")
}

fn default_is_work(d: NaiveDate) -> bool {
    !matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Date ↔ ordinal mapping in calendar days or working days.
#[derive(Clone, Debug)]
pub enum WorkCalendar {
    /// Every day counts (projects in "calendar days" mode).
    Calendar,
    /// Working days only.
    Work {
        base: NaiveDate,
        /// `prefix[i]` = number of working days in `[base, base + i)`.
        prefix: Vec<i64>,
    },
}

impl WorkCalendar {
    pub fn calendar_days() -> Self {
        WorkCalendar::Calendar
    }

    /// Working days given the special days (`true` = working).
    pub fn from_special(special: &HashMap<NaiveDate, bool>) -> Self {
        let base = NaiveDate::from_ymd_opt(WINDOW_FROM.0, WINDOW_FROM.1, WINDOW_FROM.2).expect("valid");
        let end = NaiveDate::from_ymd_opt(WINDOW_TO.0, WINDOW_TO.1, WINDOW_TO.2).expect("valid");
        let n = (end - base).num_days() as usize + 1;
        let mut prefix = Vec::with_capacity(n + 1);
        prefix.push(0);
        let mut acc = 0;
        let mut d = base;
        for _ in 0..n {
            if special.get(&d).copied().unwrap_or_else(|| default_is_work(d)) {
                acc += 1;
            }
            prefix.push(acc);
            d += Duration::days(1);
        }
        WorkCalendar::Work { base, prefix }
    }

    fn index(base: NaiveDate, prefix: &[i64], d: NaiveDate) -> usize {
        ((d - base).num_days()).clamp(0, prefix.len() as i64 - 2) as usize
    }

    pub fn is_work(&self, d: NaiveDate) -> bool {
        match self {
            WorkCalendar::Calendar => true,
            WorkCalendar::Work { base, prefix } => {
                let i = Self::index(*base, prefix, d);
                prefix[i + 1] > prefix[i]
            }
        }
    }

    /// Ordinal of the first working day on or after `d`.
    pub fn ceil(&self, d: NaiveDate) -> i64 {
        match self {
            WorkCalendar::Calendar => d.num_days_from_ce() as i64,
            WorkCalendar::Work { base, prefix } => prefix[Self::index(*base, prefix, d)],
        }
    }

    /// Ordinal of the last working day on or before `d`.
    pub fn floor(&self, d: NaiveDate) -> i64 {
        match self {
            WorkCalendar::Calendar => d.num_days_from_ce() as i64,
            WorkCalendar::Work { base, prefix } => prefix[Self::index(*base, prefix, d) + 1] - 1,
        }
    }

    /// The working day with ordinal `o`.
    pub fn date(&self, o: i64) -> NaiveDate {
        match self {
            WorkCalendar::Calendar => NaiveDate::from_num_days_from_ce_opt(o as i32).expect("date in range"),
            WorkCalendar::Work { base, prefix } => {
                // The first index whose prefix after it exceeds `o` is that working day.
                let i = prefix[1..].partition_point(|&p| p <= o).min(prefix.len() - 2);
                *base + Duration::days(i as i64)
            }
        }
    }

    /// Move a date onto the nearest working day at or after it.
    pub fn next_work(&self, d: NaiveDate) -> NaiveDate {
        self.date(self.ceil(d))
    }

    /// Keep task dates on working days: the start moves forward, the due
    /// date moves back but never before the start.
    pub fn snap(&self, start: Option<NaiveDate>, due: Option<NaiveDate>) -> (Option<NaiveDate>, Option<NaiveDate>) {
        if matches!(self, WorkCalendar::Calendar) {
            return (start, due);
        }
        let s = start.map(|d| self.next_work(d));
        let d = due.map(|d| {
            let back = self.date(self.floor(d));
            let lower = s.unwrap_or(back);
            if back < lower { lower } else { back }
        });
        (s, d)
    }
}

/// Special days (`date → is_working`) of the portal: shipped calendar
/// overlaid with the portal's own changes.
pub async fn special_days(state: &AppState, tenant_id: Uuid) -> ApiResult<HashMap<NaiveDate, bool>> {
    let rows: Vec<(NaiveDate, String)> = sqlx::query_as(
        "SELECT day, kind FROM calendar_days c
          WHERE NOT EXISTS (SELECT 1 FROM tenant_calendar_days t WHERE t.tenant_id = $1 AND t.day = c.day)
         UNION ALL
         SELECT day, kind FROM tenant_calendar_days WHERE tenant_id = $1",
    )
    .bind(tenant_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|(d, k)| (d, kind_is_work(&k))).collect())
}

/// The calendar a project schedules by.
pub async fn for_project(state: &AppState, project_id: Uuid) -> ApiResult<WorkCalendar> {
    let (tenant_id, mode): (Uuid, String) = sqlx::query_as("SELECT tenant_id, day_mode FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&state.db)
        .await?;
    if mode == "calendar" {
        return Ok(WorkCalendar::calendar_days());
    }
    Ok(WorkCalendar::from_special(&special_days(state, tenant_id).await?))
}

// ---------------------------------------------------------------------------
// API
// ---------------------------------------------------------------------------

#[derive(Serialize, sqlx::FromRow)]
struct DayRow {
    date: NaiveDate,
    kind: String,
    title: String,
    /// Changed by the portal (not from the shipped calendar).
    custom: bool,
    /// The shipped kind under a portal change, if any.
    official_kind: Option<String>,
}

#[derive(Deserialize)]
pub struct RangeQuery {
    pub year: Option<i32>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

async fn days_in(state: &AppState, tenant_id: Uuid, from: NaiveDate, to: NaiveDate) -> ApiResult<Vec<DayRow>> {
    Ok(sqlx::query_as::<_, DayRow>(
        "SELECT COALESCE(t.day, c.day) AS date, COALESCE(t.kind, c.kind) AS kind, COALESCE(t.title, c.title) AS title,
                t.day IS NOT NULL AS custom, CASE WHEN t.day IS NOT NULL THEN c.kind END AS official_kind
           FROM (SELECT * FROM calendar_days WHERE day BETWEEN $2 AND $3) c
           FULL JOIN (SELECT * FROM tenant_calendar_days WHERE tenant_id = $1 AND day BETWEEN $2 AND $3) t ON t.day = c.day
          ORDER BY 1",
    )
    .bind(tenant_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await?)
}

/// `GET /api/calendar?year=` or `?from=&to=` — special days of the period.
/// With `year`: also working days by month and whether the official
/// calendar for the year ships with the portal.
pub async fn get(State(state): State<AppState>, user: CurrentUser, Query(q): Query<RangeQuery>) -> ApiResult<Json<Value>> {
    let (from, to) = match (q.year, q.from, q.to) {
        (Some(y), _, _) if (1990..=2150).contains(&y) => (
            NaiveDate::from_ymd_opt(y, 1, 1).expect("valid"),
            NaiveDate::from_ymd_opt(y, 12, 31).expect("valid"),
        ),
        (None, Some(f), Some(t)) if f <= t && (t - f).num_days() <= 3 * 366 => (f, t),
        _ => return bad_request("give year (1990–2150) or from and to (up to 3 years)"),
    };
    let days = days_in(&state, user.tenant_id, from, to).await?;
    let official_years: Vec<i32> = sqlx::query_scalar("SELECT DISTINCT extract(year FROM day)::int FROM calendar_days ORDER BY 1")
        .fetch_all(&state.db)
        .await?;
    let mut out = json!({ "from": from, "to": to, "days": days, "official_years": official_years });
    if let Some(y) = q.year {
        let special: HashMap<NaiveDate, (bool, bool)> =
            days.iter().map(|d| (d.date, (kind_is_work(&d.kind), d.kind == "short"))).collect();
        let mut months = [0i64; 12];
        let mut short = 0;
        let mut d = from;
        while d <= to {
            let (work, is_short) = special.get(&d).copied().unwrap_or((default_is_work(d), false));
            if work {
                months[d.month0() as usize] += 1;
                if is_short {
                    short += 1;
                }
            }
            d += Duration::days(1);
        }
        let total: i64 = months.iter().sum();
        let days_in_year = (to - from).num_days() + 1;
        out["year"] = json!(y);
        out["months"] = json!(months);
        out["work_days"] = json!(total);
        out["rest_days"] = json!(days_in_year - total);
        out["short_days"] = json!(short);
        // Hours for a 40-hour week: 8 h a day, an hour less on shortened days.
        out["hours_40"] = json!(total * 8 - short);
        out["official"] = json!(official_years.contains(&y));
    }
    Ok(Json(out))
}

#[derive(Deserialize)]
pub struct SetDay {
    pub kind: String,
    #[serde(default)]
    pub title: String,
}

/// `PUT /api/admin/calendar/days/{date}` — the portal's own kind for a day.
pub async fn set_day(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(day): Path<NaiveDate>,
    Json(req): Json<SetDay>,
) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if !matches!(req.kind.as_str(), "holiday" | "off" | "work" | "short") {
        return bad_request("kind must be holiday, off, work or short");
    }
    let title: String = req.title.trim().chars().take(200).collect();
    sqlx::query(
        "INSERT INTO tenant_calendar_days (tenant_id, day, kind, title, updated_by) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (tenant_id, day) DO UPDATE SET kind = $3, title = $4, updated_by = $5, updated_at = now()",
    )
    .bind(user.tenant_id)
    .bind(day)
    .bind(&req.kind)
    .bind(&title)
    .bind(user.user_id)
    .execute(&state.db)
    .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "calendar.day_set", Some("calendar"), None, json!({ "day": day, "kind": req.kind }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// `DELETE /api/admin/calendar/days/{date}` — back to the shipped calendar.
pub async fn reset_day(State(state): State<AppState>, user: CurrentUser, Path(day): Path<NaiveDate>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    sqlx::query("DELETE FROM tenant_calendar_days WHERE tenant_id = $1 AND day = $2")
        .bind(user.tenant_id)
        .bind(day)
        .execute(&state.db)
        .await?;
    state
        .audit(user.tenant_id, Some(user.user_id), "calendar.day_reset", Some("calendar"), None, json!({ "day": day }))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// Non-working holidays of the Labour Code (art. 112) for any year.
const TK_HOLIDAYS: &[(u32, u32, &str)] = &[
    (1, 1, "Новогодние каникулы"),
    (1, 2, "Новогодние каникулы"),
    (1, 3, "Новогодние каникулы"),
    (1, 4, "Новогодние каникулы"),
    (1, 5, "Новогодние каникулы"),
    (1, 6, "Новогодние каникулы"),
    (1, 7, "Рождество Христово"),
    (1, 8, "Новогодние каникулы"),
    (2, 23, "День защитника Отечества"),
    (3, 8, "Международный женский день"),
    (5, 1, "Праздник Весны и Труда"),
    (5, 9, "День Победы"),
    (6, 12, "День России"),
    (11, 4, "День народного единства"),
];

/// Holidays of a year by the Labour Code, with a holiday falling on a
/// weekend moved to the next working day (except the January ones, which the
/// Government transfers by decree).
pub fn tk_holidays(year: i32) -> Vec<(NaiveDate, &'static str, String)> {
    let mut out: Vec<(NaiveDate, &'static str, String)> = Vec::new();
    for (m, d, title) in TK_HOLIDAYS {
        let day = NaiveDate::from_ymd_opt(year, *m, *d).expect("valid");
        out.push((day, "holiday", (*title).to_string()));
    }
    for (m, d, _) in TK_HOLIDAYS.iter().filter(|(m, _, _)| *m != 1) {
        let day = NaiveDate::from_ymd_opt(year, *m, *d).expect("valid");
        if default_is_work(day) {
            continue;
        }
        let mut next = day + Duration::days(1);
        while !default_is_work(next) || out.iter().any(|(x, _, _)| *x == next) {
            next += Duration::days(1);
        }
        out.push((next, "off", format!("Перенос с {}", day_month(day))));
    }
    out.sort_by_key(|x| x.0);
    out
}

/// «8 марта» — day and month in the genitive case.
pub fn day_month(d: NaiveDate) -> String {
    const M: [&str; 12] = [
        "января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря",
    ];
    format!("{} {}", d.day(), M[d.month0() as usize])
}

/// `POST /api/admin/calendar/{year}/holidays` — fill a year that has no
/// official calendar with the Labour Code holidays. Days already set
/// (shipped or by the portal) are left alone.
pub async fn fill_holidays(State(state): State<AppState>, user: CurrentUser, Path(year): Path<i32>) -> ApiResult<Json<Value>> {
    user.require_admin()?;
    if !(2000..=2150).contains(&year) {
        return bad_request("year out of range");
    }
    let mut added = 0u64;
    for (day, kind, title) in tk_holidays(year) {
        added += sqlx::query(
            "INSERT INTO tenant_calendar_days (tenant_id, day, kind, title, updated_by)
             SELECT $1, $2, $3, $4, $5
              WHERE NOT EXISTS (SELECT 1 FROM calendar_days WHERE day = $2)
             ON CONFLICT (tenant_id, day) DO NOTHING",
        )
        .bind(user.tenant_id)
        .bind(day)
        .bind(kind)
        .bind(&title)
        .bind(user.user_id)
        .execute(&state.db)
        .await?
        .rows_affected();
    }
    state
        .audit(user.tenant_id, Some(user.user_id), "calendar.holidays_filled", Some("calendar"), None, json!({ "year": year, "added": added }))
        .await;
    Ok(Json(json!({ "added": added })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// A working calendar with the given special days.
    fn ru(days: &[(&str, bool)]) -> WorkCalendar {
        WorkCalendar::from_special(&days.iter().map(|(s, w)| (d(s), *w)).collect())
    }

    #[test]
    fn ordinals_round_trip_and_skip_weekends() {
        let c = ru(&[]);
        // Fri 2026-11-13 → Mon 2026-11-16 is the next working day.
        assert_eq!(c.ceil(d("2026-11-14")), c.ceil(d("2026-11-16")));
        assert_eq!(c.floor(d("2026-11-15")), c.floor(d("2026-11-13")));
        assert_eq!(c.date(c.floor(d("2026-11-13")) + 1), d("2026-11-16"));
        assert_eq!(c.floor(d("2026-11-22")) - c.ceil(d("2026-11-09")) + 1, 10);
        assert!(!c.is_work(d("2026-11-14")));
    }

    #[test]
    fn new_year_holidays_and_working_saturday() {
        let c = ru(&[
            ("2026-12-31", false),
            ("2027-01-01", false),
            ("2027-01-04", false),
            ("2027-01-05", false),
            ("2027-01-06", false),
            ("2027-01-07", false),
            ("2027-01-08", false),
            ("2027-02-20", true),
            ("2027-02-22", false),
            ("2027-02-23", false),
        ]);
        // The working day after 30.12.2026 is 11.01.2027.
        assert_eq!(c.date(c.floor(d("2026-12-30")) + 1), d("2027-01-11"));
        // Saturday 20.02.2027 works; 22–23.02 do not.
        assert!(c.is_work(d("2027-02-20")));
        assert_eq!(c.date(c.floor(d("2027-02-19")) + 1), d("2027-02-20"));
        assert_eq!(c.date(c.floor(d("2027-02-20")) + 1), d("2027-02-24"));
    }

    #[test]
    fn snap_keeps_dates_on_working_days() {
        let c = ru(&[("2026-05-11", false)]);
        // Saturday start → Tuesday (Monday 11.05 is a day off); Sunday due → Friday.
        assert_eq!(c.snap(Some(d("2026-05-09")), Some(d("2026-05-15"))), (Some(d("2026-05-12")), Some(d("2026-05-15"))));
        assert_eq!(c.snap(Some(d("2026-05-04")), Some(d("2026-05-10"))), (Some(d("2026-05-04")), Some(d("2026-05-08"))));
        // A weekend-only span collapses onto the start.
        assert_eq!(c.snap(Some(d("2026-05-09")), Some(d("2026-05-10"))), (Some(d("2026-05-12")), Some(d("2026-05-12"))));
        // Calendar mode leaves dates as they are.
        let cal = WorkCalendar::calendar_days();
        assert_eq!(cal.snap(Some(d("2026-05-09")), None), (Some(d("2026-05-09")), None));
    }

    #[test]
    fn labour_code_holidays_with_weekend_transfer() {
        let h = tk_holidays(2028);
        // 2028: 8 March is Wednesday, 1 May Monday, 9 May Tuesday, 12 June Monday,
        // 4 November Saturday → Monday 6 November; 23 February Wednesday.
        let off: Vec<_> = h.iter().filter(|x| x.1 == "off").map(|x| x.0).collect();
        assert_eq!(off, vec![d("2028-11-06")]);
        assert_eq!(h.iter().filter(|x| x.1 == "holiday").count(), 14);
    }
}
