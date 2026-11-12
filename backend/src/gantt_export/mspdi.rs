//! MS Project XML (MSPDI): tasks with the outline, dates, durations,
//! milestones and percent complete; assignees as work resources; links of
//! all four kinds with lags; the portal's working calendar (weekends,
//! holidays and transferred working days) as the project calendar. Opens in
//! MS Project, ProjectLibre and GanttProject.
//!
//! Leaf tasks carry a «start no earlier than» constraint on their start, so
//! MS Project keeps the portal's dates when it recalculates the schedule.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use chrono::{Datelike, Duration, NaiveDate};

use super::Data;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            c if (c as u32) < 0x20 && !matches!(c, '\n' | '\r' | '\t') => {}
            c => o.push(c),
        }
    }
    o
}

fn dt(d: NaiveDate, time: &str) -> String {
    format!("{}T{time}", d.format("%Y-%m-%d"))
}

const WORK_TIMES: &str = "<WorkingTimes><WorkingTime><FromTime>08:00:00</FromTime><ToTime>12:00:00</ToTime></WorkingTime><WorkingTime><FromTime>13:00:00</FromTime><ToTime>17:00:00</ToTime></WorkingTime></WorkingTimes>";

pub fn write(data: &Data) -> String {
    let work = data.day_mode == "work";
    let (lo, hi) = data.span();
    let mut x = String::with_capacity(64 * 1024);
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    x.push_str("<Project xmlns=\"http://schemas.microsoft.com/project\">\n");
    let _ = writeln!(x, "  <SaveVersion>14</SaveVersion>");
    let _ = writeln!(x, "  <Name>{}.xml</Name>", esc(&data.title_line()));
    let _ = writeln!(x, "  <Title>{}</Title>", esc(&data.title_line()));
    let _ = writeln!(x, "  <Company>{}</Company>", esc(&data.portal_name));
    let _ = writeln!(x, "  <Author>{}</Author>", esc(&data.exported_by));
    let _ = writeln!(x, "  <CreationDate>{}</CreationDate>", dt(data.today, "08:00:00"));
    let _ = writeln!(x, "  <ScheduleFromStart>1</ScheduleFromStart>");
    let _ = writeln!(x, "  <StartDate>{}</StartDate>", dt(lo, "08:00:00"));
    let _ = writeln!(x, "  <FinishDate>{}</FinishDate>", dt(hi, "17:00:00"));
    let _ = writeln!(x, "  <CalendarUID>1</CalendarUID>");
    let _ = writeln!(x, "  <DefaultStartTime>08:00:00</DefaultStartTime>");
    let _ = writeln!(x, "  <DefaultFinishTime>17:00:00</DefaultFinishTime>");
    let _ = writeln!(x, "  <MinutesPerDay>480</MinutesPerDay>");
    let _ = writeln!(x, "  <MinutesPerWeek>2400</MinutesPerWeek>");
    let _ = writeln!(x, "  <DaysPerMonth>20</DaysPerMonth>");
    let _ = writeln!(x, "  <DurationFormat>{}</DurationFormat>", if work { 7 } else { 39 });
    let _ = writeln!(x, "  <WeekStartDay>1</WeekStartDay>");
    let _ = writeln!(x, "  <CurrentDate>{}</CurrentDate>", dt(data.today, "08:00:00"));
    let _ = writeln!(x, "  <StatusDate>{}</StatusDate>", dt(data.today, "17:00:00"));

    // --- Calendar ----------------------------------------------------------------------
    x.push_str("  <Calendars>\n    <Calendar>\n      <UID>1</UID>\n      <Name>Производственный календарь</Name>\n      <IsBaseCalendar>1</IsBaseCalendar>\n      <BaseCalendarUID>-1</BaseCalendarUID>\n      <WeekDays>\n");
    for day in 1..=7 {
        // 1 — Sunday … 7 — Saturday.
        if day == 1 || day == 7 {
            let _ = writeln!(x, "        <WeekDay><DayType>{day}</DayType><DayWorking>0</DayWorking></WeekDay>");
        } else {
            let _ = writeln!(x, "        <WeekDay><DayType>{day}</DayType><DayWorking>1</DayWorking>{WORK_TIMES}</WeekDay>");
        }
    }
    // Exceptions within the years of the chart: runs of days off on weekdays and working weekends.
    let from = NaiveDate::from_ymd_opt(lo.year(), 1, 1).expect("valid");
    let to = NaiveDate::from_ymd_opt(hi.year(), 12, 31).expect("valid");
    let special: BTreeMap<NaiveDate, bool> = data.special.iter().filter(|(d, _)| **d >= from && **d <= to).map(|(d, w)| (*d, *w)).collect();
    let weekend = |d: NaiveDate| d.weekday().num_days_from_monday() >= 5;
    let mut runs: Vec<(NaiveDate, NaiveDate, bool)> = Vec::new();
    for (&d, &working) in &special {
        if working == !weekend(d) {
            continue; // same as the default week
        }
        match runs.last_mut() {
            Some(r) if r.2 == working && r.1 + Duration::days(1) == d => r.1 = d,
            _ => runs.push((d, d, working)),
        }
    }
    for (a, b, working) in &runs {
        let _ = writeln!(
            x,
            "        <WeekDay><DayType>0</DayType><DayWorking>{}</DayWorking><TimePeriod><FromDate>{}</FromDate><ToDate>{}</ToDate></TimePeriod>{}</WeekDay>",
            u8::from(*working),
            dt(*a, "00:00:00"),
            dt(*b, "23:59:00"),
            if *working { WORK_TIMES } else { "" }
        );
    }
    x.push_str("      </WeekDays>\n    </Calendar>\n  </Calendars>\n");

    // --- Tasks ---------------------------------------------------------------------------
    let uid: HashMap<uuid::Uuid, usize> = data.rows.iter().enumerate().map(|(i, r)| (r.id, i + 1)).collect();
    let mut preds: HashMap<uuid::Uuid, Vec<&super::Link>> = HashMap::new();
    for l in &data.links {
        preds.entry(l.task).or_default().push(l);
    }
    x.push_str("  <Tasks>\n");
    let _ = writeln!(
        x,
        "    <Task><UID>0</UID><ID>0</ID><Name>{}</Name><Type>1</Type><IsNull>0</IsNull><OutlineNumber>0</OutlineNumber><OutlineLevel>0</OutlineLevel><Summary>1</Summary><Start>{}</Start><Finish>{}</Finish></Task>",
        esc(&data.title_line()),
        dt(lo, "08:00:00"),
        dt(hi, "17:00:00")
    );
    for (i, r) in data.rows.iter().enumerate() {
        let id = i + 1;
        let s = r.start.unwrap_or(data.today);
        let e = r.end.unwrap_or(s).max(s);
        let _ = write!(x, "    <Task><UID>{id}</UID><ID>{id}</ID><Name>{}</Name><Type>{}</Type><IsNull>0</IsNull>", esc(&r.title), if r.is_parent { 1 } else { 0 });
        let _ = write!(x, "<WBS>{0}</WBS><OutlineNumber>{0}</OutlineNumber><OutlineLevel>{1}</OutlineLevel>", r.wbs, r.depth + 1);
        let (start_t, finish) = if r.milestone { (dt(s, "08:00:00"), dt(s, "08:00:00")) } else { (dt(s, "08:00:00"), dt(e, "17:00:00")) };
        let duration = if r.milestone {
            "PT0H0M0S".to_string()
        } else if work {
            format!("PT{}H0M0S", data.duration(r).unwrap_or(1).max(1) * 8)
        } else {
            format!("PT{}H0M0S", (e - s).num_days() * 24 + 9)
        };
        let _ = write!(x, "<Start>{start_t}</Start><Finish>{finish}</Finish><Duration>{duration}</Duration><DurationFormat>{}</DurationFormat>", if work { 7 } else { 39 });
        let _ = write!(x, "<Milestone>{}</Milestone><Summary>{}</Summary><Manual>0</Manual>", u8::from(r.milestone), u8::from(r.is_parent));
        let _ = write!(x, "<PercentComplete>{0}</PercentComplete><PercentWorkComplete>{0}</PercentWorkComplete>", r.progress);
        if r.critical {
            x.push_str("<Critical>1</Critical>");
        }
        if !r.is_parent && r.start.is_some() {
            let _ = write!(x, "<ConstraintType>4</ConstraintType><ConstraintDate>{}</ConstraintDate>", dt(s, "08:00:00"));
        }
        if let Some((a, b)) = r.base {
            let _ = write!(x, "<Baseline><Number>0</Number><Start>{}</Start><Finish>{}</Finish></Baseline>", dt(a, "08:00:00"), dt(b, "17:00:00"));
        }
        if !r.description.trim().is_empty() {
            let _ = write!(x, "<Notes>{}</Notes>", esc(r.description.trim()));
        }
        for l in preds.get(&r.id).map(|v| v.as_slice()).unwrap_or(&[]) {
            let Some(&p) = uid.get(&l.pred) else { continue };
            let kind = match l.kind.as_str() {
                "FF" => 0,
                "SF" => 2,
                "SS" => 3,
                _ => 1,
            };
            let lag = l.lag as i64 * if work { 480 } else { 1440 } * 10;
            let _ = write!(
                x,
                "<PredecessorLink><PredecessorUID>{p}</PredecessorUID><Type>{kind}</Type><CrossProject>0</CrossProject><LinkLag>{lag}</LinkLag><LagFormat>{}</LagFormat></PredecessorLink>",
                if work { 7 } else { 39 }
            );
        }
        x.push_str("</Task>\n");
    }
    x.push_str("  </Tasks>\n");

    // --- Resources and assignments --------------------------------------------------------------
    let mut res: Vec<String> = data.rows.iter().filter_map(|r| r.assignee.clone()).collect();
    res.sort();
    res.dedup();
    let rid: HashMap<&str, usize> = res.iter().enumerate().map(|(i, n)| (n.as_str(), i + 1)).collect();
    x.push_str("  <Resources>\n    <Resource><UID>0</UID><ID>0</ID><Type>1</Type><IsNull>0</IsNull></Resource>\n");
    for (n, i) in res.iter().map(|n| (n, rid[n.as_str()])) {
        let _ = writeln!(x, "    <Resource><UID>{i}</UID><ID>{i}</ID><Name>{}</Name><Type>1</Type><IsNull>0</IsNull><MaxUnits>1.00</MaxUnits><CalendarUID>1</CalendarUID></Resource>", esc(n));
    }
    x.push_str("  </Resources>\n  <Assignments>\n");
    let mut aid = 0;
    for (i, r) in data.rows.iter().enumerate() {
        if r.is_parent || r.milestone {
            continue;
        }
        if let Some(&res) = r.assignee.as_deref().and_then(|a| rid.get(a)) {
            aid += 1;
            let _ = writeln!(x, "    <Assignment><UID>{aid}</UID><TaskUID>{}</TaskUID><ResourceUID>{res}</ResourceUID><Units>1</Units><PercentWorkComplete>{}</PercentWorkComplete></Assignment>", i + 1, r.progress);
        }
    }
    x.push_str("  </Assignments>\n</Project>\n");
    x
}

#[cfg(test)]
mod tests {
    use super::esc;

    #[test]
    fn escapes_xml() {
        assert_eq!(esc("A & B <c> \"d\" 'e'\u{1}"), "A &amp; B &lt;c&gt; &quot;d&quot; &apos;e&apos;");
    }
}
