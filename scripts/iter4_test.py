#!/usr/bin/env python3
"""End-to-end test of Gantt scheduling: auto-shift, reschedule, critical
path / slack and the baseline plan.

Usage:
    API=http://localhost:8080 python3 scripts/iter4_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "t4" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Org", "email": "o@example.com",
                                        "display_name": "Owner", "password": "owner-password-1"})
    mid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Member", "password": "member-password-1"})["id"]
    viewer_id = owner.call("POST", "/users", {"email": "v@example.com", "display_name": "Viewer", "password": "viewer-password-1"})["id"]
    member, viewer = Client(), Client()
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "member-password-1"})
    viewer.call("POST", "/auth/login", {"tenant": slug, "email": "v@example.com", "password": "viewer-password-1"})

    pid = owner.call("POST", "/projects", {"name": "График"})["id"]
    # This test checks scheduling in calendar days (working days: calendar_test.py).
    owner.call("PATCH", f"/projects/{pid}", {"day_mode": "calendar"})
    owner.call("POST", f"/projects/{pid}/members", {"user_id": mid, "role": "member"})
    owner.call("POST", f"/projects/{pid}/members", {"user_id": viewer_id, "role": "viewer"})

    def task(title, start, due, **kw):
        return owner.call("POST", f"/projects/{pid}/tasks", {"title": title, "start_date": start, "due_date": due, **kw})["task"]["id"]

    survey = task("Обследование", "2026-11-02", "2026-11-06")
    lab = task("Лаборатория", "2026-11-04", "2026-11-12")
    calc = task("Расчёты", "2026-11-09", "2026-11-13")
    report = task("Отчёт", "2026-11-16", "2026-11-20")
    handover = task("Сдача", None, "2026-11-20", is_milestone=True)
    loose = task("Фотофиксация", "2026-11-02", "2026-11-03")

    # Dependencies: survey → calc → report → handover; lab →(FF) report.
    assert owner.call("POST", f"/tasks/{calc}/dependencies", {"depends_on_id": survey})["shifted"] == []
    owner.call("POST", f"/tasks/{report}/dependencies", {"depends_on_id": calc})
    owner.call("POST", f"/tasks/{handover}/dependencies", {"depends_on_id": report})
    owner.call("POST", f"/tasks/{report}/dependencies", {"depends_on_id": lab, "kind": "FF", "lag_days": 0})

    # Adding a violated dependency shifts the successor immediately.
    extra = task("Доп. замеры", "2026-11-03", "2026-11-04")
    r = owner.call("POST", f"/tasks/{extra}/dependencies", {"depends_on_id": survey, "lag_days": 1})
    assert [t["id"] for t in r["shifted"]] == [extra]
    assert r["shifted"][0]["start_date"] == "2026-11-08" and r["shifted"][0]["due_date"] == "2026-11-09"
    step("dependency creation shifts a violated successor (FS + lag)")

    # Moving the survey 5 days later cascades along the chain.
    r = member.call("PATCH", f"/tasks/{survey}", {"start_date": "2026-11-07", "due_date": "2026-11-11"})
    moved = {t["title"]: (t["start_date"], t["due_date"]) for t in r["shifted"]}
    assert moved["Расчёты"] == ("2026-11-12", "2026-11-16"), moved
    assert moved["Отчёт"] == ("2026-11-17", "2026-11-21"), moved
    assert moved["Сдача"] == ("2026-11-21", "2026-11-21"), moved
    assert moved["Доп. замеры"] == ("2026-11-13", "2026-11-14"), moved
    assert "Лаборатория" not in moved and "Фотофиксация" not in moved
    step("moving a task cascades through FS chain, milestone stays on the finish date")

    # Non-date edits do not trigger scheduling.
    assert member.call("PATCH", f"/tasks/{survey}", {"title": "Обследование конструкций"})["shifted"] == []

    # Critical path: survey → calc → report → handover; lab and loose have float.
    data = member.call("GET", f"/projects/{pid}/tasks")
    sch = data["schedule"]
    crit = set(sch["critical"])
    assert {survey, calc, report, handover} <= crit, sch
    assert lab not in crit and loose not in crit
    assert sch["slack"][lab] == 9 and sch["project_end"] == "2026-11-21"
    assert sch["slack"][extra] == 7
    step("critical path and slack (FS + FF dependencies)")

    # Auto-schedule off: dates move independently, violations show as negative slack.
    owner.call("PATCH", f"/projects/{pid}", {"auto_schedule": False})
    assert owner.call("GET", f"/projects/{pid}")["project"]["auto_schedule"] is False
    r = owner.call("PATCH", f"/tasks/{calc}", {"start_date": "2026-11-17", "due_date": "2026-11-19"})
    assert r["shifted"] == []
    sch = owner.call("GET", f"/projects/{pid}/tasks")["schedule"]
    assert sch["slack"][calc] < 0
    viewer.call("POST", f"/projects/{pid}/reschedule", expect=403)
    r = member.call("POST", f"/projects/{pid}/reschedule")
    moved = {t["title"]: (t["start_date"], t["due_date"]) for t in r["shifted"]}
    assert moved["Отчёт"] == ("2026-11-20", "2026-11-24") and moved["Сдача"] == ("2026-11-24", "2026-11-24"), moved
    assert all(v >= 0 for v in owner.call("GET", f"/projects/{pid}/tasks")["schedule"]["slack"].values())
    step("auto-schedule switch, negative slack on violation, explicit reschedule")

    # Completed tasks are never moved.
    owner.call("PATCH", f"/projects/{pid}", {"auto_schedule": True})
    owner.call("PATCH", f"/tasks/{handover}", {"status": "done"})
    r = owner.call("PATCH", f"/tasks/{report}", {"start_date": "2026-11-25", "due_date": "2026-11-27"})
    assert r["shifted"] == []
    step("completed tasks are fixed")

    # Baseline.
    member.call("POST", f"/projects/{pid}/baseline", expect=403)
    assert owner.call("POST", f"/projects/{pid}/baseline")["tasks"] == 7
    owner.call("PATCH", f"/tasks/{loose}", {"start_date": "2026-11-05", "due_date": "2026-11-06"})
    data = owner.call("GET", f"/projects/{pid}/tasks")
    base = {b["task_id"]: b for b in data["baseline"]}
    assert base[loose]["start_date"] == "2026-11-02" and base[loose]["due_date"] == "2026-11-03"
    assert owner.call("GET", f"/projects/{pid}")["project"]["baseline_saved_at"]
    owner.call("DELETE", f"/projects/{pid}/baseline")
    assert owner.call("GET", f"/projects/{pid}/tasks")["baseline"] == []
    assert owner.call("GET", f"/projects/{pid}")["project"]["baseline_saved_at"] is None
    step("baseline save / compare / clear (managers only)")

    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "z", "tenant_name": "O", "email": "z@example.com",
                                        "display_name": "Z", "password": "other-password-1"})
    other.call("POST", f"/projects/{pid}/reschedule", expect=404)
    other.call("POST", f"/projects/{pid}/baseline", expect=404)
    step("tenant isolation")
    print("ALL ITERATION-4 TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
