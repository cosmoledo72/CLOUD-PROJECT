//! Project scheduling: dependency-driven auto-shift and critical path.
//!
//! Dates are inclusive calendar days: a task from 1st to 3rd lasts 3 days and
//! a finish-to-start successor may start on the 4th. Milestones are single
//! days (start == due) and may coincide with the finish of their predecessor.
//!
//! Dependency constraints (successor S depends on predecessor P, lag L days):
//!
//! | kind | constraint                                   |
//! |------|----------------------------------------------|
//! | FS   | S.start ≥ P.due + 1 + L  (milestone S: P.due + L) |
//! | SS   | S.start ≥ P.start + L                        |
//! | FF   | S.due   ≥ P.due + L                          |
//! | SF   | S.due   ≥ P.start + L                        |
//!
//! Auto-shift only moves successors later (never earlier) and keeps their
//! duration. Completed tasks are never moved. Undated tasks are ignored.

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{Duration, NaiveDate};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: NaiveDate,
    pub due: NaiveDate,
}

impl Span {
    /// Normalise optional dates the same way the Gantt chart draws them.
    pub fn from_dates(start: Option<NaiveDate>, due: Option<NaiveDate>) -> Option<Span> {
        let s = start.or(due)?;
        let d = due.unwrap_or(s).max(s);
        Some(Span { start: s, due: d })
    }

    fn duration(&self) -> i64 {
        (self.due - self.start).num_days()
    }

    fn shifted(&self, days: i64) -> Span {
        Span { start: self.start + Duration::days(days), due: self.due + Duration::days(days) }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub span: Option<Span>,
    pub done: bool,
    pub milestone: bool,
}

#[derive(Clone, Debug)]
pub struct Dep {
    pub task: Uuid,
    pub depends_on: Uuid,
    pub kind: String,
    pub lag: i64,
}

/// How many days the successor must move so that `dep` holds (0 if satisfied).
fn required_shift(dep: &Dep, pred: Span, succ: Span, succ_milestone: bool) -> i64 {
    let lag = Duration::days(dep.lag);
    match dep.kind.as_str() {
        "SS" => (pred.start + lag - succ.start).num_days(),
        "FF" => (pred.due + lag - succ.due).num_days(),
        "SF" => (pred.start + lag - succ.due).num_days(),
        _ => {
            let gap = if succ_milestone { 0 } else { 1 };
            (pred.due + Duration::days(gap) + lag - succ.start).num_days()
        }
    }
    .max(0)
}

/// Push successors of `changed` tasks forward until every dependency holds.
/// Returns the new spans of the tasks that moved.
pub fn propagate(nodes: &HashMap<Uuid, Node>, deps: &[Dep], changed: &[Uuid]) -> HashMap<Uuid, Span> {
    let mut spans: HashMap<Uuid, Span> = nodes.iter().filter_map(|(id, n)| n.span.map(|s| (*id, s))).collect();
    let mut successors: HashMap<Uuid, Vec<&Dep>> = HashMap::new();
    for d in deps {
        successors.entry(d.depends_on).or_default().push(d);
    }
    let mut moved: HashMap<Uuid, Span> = HashMap::new();
    let mut queue: VecDeque<Uuid> = changed.iter().copied().collect();
    let mut guard = 0usize;

    while let Some(pred_id) = queue.pop_front() {
        guard += 1;
        if guard > 100_000 {
            break; // defensive: dependencies are acyclic, but never loop forever
        }
        let Some(pred) = spans.get(&pred_id).copied() else { continue };
        for dep in successors.get(&pred_id).map(Vec::as_slice).unwrap_or_default() {
            let Some(node) = nodes.get(&dep.task) else { continue };
            if node.done {
                continue;
            }
            let Some(succ) = spans.get(&dep.task).copied() else { continue };
            let shift = required_shift(dep, pred, succ, node.milestone);
            if shift > 0 {
                let new = succ.shifted(shift);
                spans.insert(dep.task, new);
                moved.insert(dep.task, new);
                queue.push_back(dep.task);
            }
        }
    }
    moved
}

/// Every dated task with no unmet requirement to move — used by "reschedule all".
pub fn propagate_all(nodes: &HashMap<Uuid, Node>, deps: &[Dep]) -> HashMap<Uuid, Span> {
    let roots: Vec<Uuid> = nodes.iter().filter(|(_, n)| n.span.is_some()).map(|(id, _)| *id).collect();
    propagate(nodes, deps, &roots)
}

#[derive(Debug, Default)]
pub struct CriticalPath {
    /// Total float in days for each dated task (negative = dependency violated).
    pub slack: HashMap<Uuid, i64>,
    /// Tasks with zero or negative float.
    pub critical: HashSet<Uuid>,
    pub project_end: Option<NaiveDate>,
}

/// Backward pass of the critical path method over the current dates.
pub fn critical_path(nodes: &HashMap<Uuid, Node>, deps: &[Dep]) -> CriticalPath {
    let spans: HashMap<Uuid, Span> = nodes.iter().filter_map(|(id, n)| n.span.map(|s| (*id, s))).collect();
    let Some(end) = spans.values().map(|s| s.due).max() else { return CriticalPath::default() };
    let deps: Vec<&Dep> = deps.iter().filter(|d| spans.contains_key(&d.task) && spans.contains_key(&d.depends_on)).collect();

    // Topological order (Kahn) over dated tasks; process successors first.
    let mut indegree: HashMap<Uuid, usize> = spans.keys().map(|id| (*id, 0)).collect();
    let mut succs: HashMap<Uuid, Vec<&Dep>> = HashMap::new();
    let mut preds: HashMap<Uuid, Vec<&Dep>> = HashMap::new();
    for d in &deps {
        *indegree.get_mut(&d.task).expect("dated") += 1;
        succs.entry(d.depends_on).or_default().push(d);
        preds.entry(d.task).or_default().push(d);
    }
    let mut queue: VecDeque<Uuid> = indegree.iter().filter(|(_, n)| **n == 0).map(|(id, _)| *id).collect();
    let mut order = Vec::with_capacity(spans.len());
    while let Some(id) = queue.pop_front() {
        order.push(id);
        for d in succs.get(&id).map(Vec::as_slice).unwrap_or_default() {
            let e = indegree.get_mut(&d.task).expect("dated");
            *e -= 1;
            if *e == 0 {
                queue.push_back(d.task);
            }
        }
    }

    let mut late_due: HashMap<Uuid, NaiveDate> = spans.keys().map(|id| (*id, end)).collect();
    for id in order.iter().rev() {
        let s = spans[id];
        let ld = late_due[id];
        let ls = ld - Duration::days(s.duration());
        let milestone = nodes.get(id).is_some_and(|n| n.milestone);
        for d in preds.get(id).map(Vec::as_slice).unwrap_or_default() {
            let p = spans[&d.depends_on];
            let lag = Duration::days(d.lag);
            let pdur = Duration::days(p.duration());
            let bound = match d.kind.as_str() {
                "SS" => ls - lag + pdur,
                "FF" => ld - lag,
                "SF" => ld - lag + pdur,
                _ => ls - lag - Duration::days(if milestone { 0 } else { 1 }),
            };
            let e = late_due.get_mut(&d.depends_on).expect("dated");
            if bound < *e {
                *e = bound;
            }
        }
    }

    let mut out = CriticalPath { project_end: Some(end), ..Default::default() };
    for (id, s) in &spans {
        let slack = (late_due[id] - s.due).num_days();
        out.slack.insert(*id, slack);
        if slack <= 0 {
            out.critical.insert(*id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn node(s: &str, e: &str) -> Node {
        Node { span: Some(Span { start: d(s), due: d(e) }), done: false, milestone: false }
    }
    fn dep(task: Uuid, on: Uuid, kind: &str, lag: i64) -> Dep {
        Dep { task, depends_on: on, kind: kind.into(), lag }
    }

    #[test]
    fn fs_chain_shifts_and_keeps_duration() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(a, node("2026-11-01", "2026-11-12")); // moved later by the user
        nodes.insert(b, node("2026-11-06", "2026-11-08"));
        nodes.insert(c, node("2026-11-09", "2026-11-10"));
        let deps = vec![dep(b, a, "FS", 0), dep(c, b, "FS", 2)];
        let moved = propagate(&nodes, &deps, &[a]);
        assert_eq!(moved[&b], Span { start: d("2026-11-13"), due: d("2026-11-15") });
        assert_eq!(moved[&c], Span { start: d("2026-11-18"), due: d("2026-11-19") });
    }

    #[test]
    fn never_pulls_earlier_and_skips_done_tasks() {
        let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(a, node("2026-11-01", "2026-11-03"));
        nodes.insert(b, node("2026-11-20", "2026-11-22"));
        let mut done = node("2026-11-02", "2026-11-02");
        done.done = true;
        nodes.insert(c, done);
        let deps = vec![dep(b, a, "FS", 0), dep(c, a, "FS", 0)];
        assert!(propagate(&nodes, &deps, &[a]).is_empty());
    }

    #[test]
    fn ss_ff_sf_and_milestones() {
        let (p, ss, ff, sf, m) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(p, node("2026-11-10", "2026-11-20"));
        nodes.insert(ss, node("2026-11-01", "2026-11-04"));
        nodes.insert(ff, node("2026-11-01", "2026-11-05"));
        nodes.insert(sf, node("2026-11-01", "2026-11-02"));
        let mut ms = node("2026-11-01", "2026-11-01");
        ms.milestone = true;
        nodes.insert(m, ms);
        let deps = vec![dep(ss, p, "SS", 1), dep(ff, p, "FF", 0), dep(sf, p, "SF", 0), dep(m, p, "FS", 0)];
        let moved = propagate(&nodes, &deps, &[p]);
        assert_eq!(moved[&ss], Span { start: d("2026-11-11"), due: d("2026-11-14") });
        assert_eq!(moved[&ff], Span { start: d("2026-11-16"), due: d("2026-11-20") });
        assert_eq!(moved[&sf], Span { start: d("2026-11-09"), due: d("2026-11-10") });
        assert_eq!(moved[&m], Span { start: d("2026-11-20"), due: d("2026-11-20") });
    }

    #[test]
    fn diamond_takes_the_latest_predecessor() {
        let (a, b, c, e) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(a, node("2026-11-01", "2026-11-02"));
        nodes.insert(b, node("2026-11-03", "2026-11-10"));
        nodes.insert(c, node("2026-11-03", "2026-11-04"));
        nodes.insert(e, node("2026-11-05", "2026-11-06"));
        let deps = vec![dep(b, a, "FS", 0), dep(c, a, "FS", 0), dep(e, b, "FS", 0), dep(e, c, "FS", 0)];
        let moved = propagate_all(&nodes, &deps);
        assert_eq!(moved[&e], Span { start: d("2026-11-11"), due: d("2026-11-12") });
        assert!(!moved.contains_key(&c));
    }

    #[test]
    fn critical_path_and_slack() {
        // a → b → d is the long chain; a → c → d has 3 days of float on c.
        let (a, b, c, e) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(a, node("2026-11-01", "2026-11-02"));
        nodes.insert(b, node("2026-11-03", "2026-11-10"));
        nodes.insert(c, node("2026-11-03", "2026-11-07"));
        nodes.insert(e, node("2026-11-11", "2026-11-12"));
        let free = Uuid::new_v4();
        nodes.insert(free, node("2026-11-01", "2026-11-05"));
        nodes.insert(Uuid::new_v4(), Node { span: None, done: false, milestone: false });
        let deps = vec![dep(b, a, "FS", 0), dep(c, a, "FS", 0), dep(e, b, "FS", 0), dep(e, c, "FS", 0)];
        let cp = critical_path(&nodes, &deps);
        assert_eq!(cp.project_end, Some(d("2026-11-12")));
        for id in [a, b, e] {
            assert_eq!(cp.slack[&id], 0, "critical");
            assert!(cp.critical.contains(&id));
        }
        assert_eq!(cp.slack[&c], 3);
        assert_eq!(cp.slack[&free], 7);
        assert_eq!(cp.slack.len(), 5, "undated tasks are skipped");
    }

    #[test]
    fn violated_dependency_gives_negative_slack() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let mut nodes = HashMap::new();
        nodes.insert(a, node("2026-11-01", "2026-11-10"));
        nodes.insert(b, node("2026-11-05", "2026-11-06"));
        let cp = critical_path(&nodes, &[dep(b, a, "FS", 0)]);
        assert!(cp.slack[&a] < 0, "violated dependency must show negative float");
        assert!(cp.critical.contains(&a));
    }
}
