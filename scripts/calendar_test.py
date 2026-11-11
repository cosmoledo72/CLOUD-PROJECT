#!/usr/bin/env python3
"""End-to-end test of the working calendar (C1): the shipped production
calendar of the Russian Federation, the portal's own changes, projects that
count working days and auto-shift over weekends and holidays.

Usage:
    API=http://localhost:8080 python3 scripts/calendar_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "cal" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    mid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк", "password": "m-password-123", "role": "member"})["id"]
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})

    # --- Shipped calendar ---------------------------------------------------
    expected = {
        2025: [17, 20, 21, 22, 18, 19, 23, 21, 22, 23, 19, 22],
        2026: [15, 19, 21, 22, 19, 21, 23, 21, 22, 22, 20, 22],
        2027: [15, 19, 22, 22, 19, 21, 22, 22, 22, 21, 20, 22],
    }
    for year, months in expected.items():
        r = m.call("GET", f"/calendar?year={year}")
        assert r["official"] is True and r["months"] == months, (year, r["months"])
        assert r["work_days"] == 247 and r["rest_days"] == 118 and r["short_days"] == 4, year
        assert r["hours_40"] == 247 * 8 - 4
    days = {d["date"]: d for d in m.call("GET", "/calendar?year=2027")["days"]}
    assert days["2027-02-20"]["kind"] == "short", "working Saturday 20.02.2027 is also shortened"
    assert days["2027-11-05"]["kind"] == "off" and days["2027-01-07"]["title"] == "Рождество Христово"
    assert m.call("GET", "/calendar?year=2026")["official_years"][:3] == [2025, 2026, 2027]
    m.call("GET", "/calendar", expect=400)
    m.call("GET", "/calendar?from=2026-01-01&to=2030-01-01", expect=400)
    r = m.call("GET", "/calendar?from=2026-12-28&to=2027-01-12")
    assert [d["date"] for d in r["days"]][:2] == ["2026-12-31", "2027-01-01"]
    step("Shipped calendar 2025–2027: 247 working days, months as in the decrees, transfers and shortened days")

    # --- Portal changes -----------------------------------------------------
    m.call("PUT", "/admin/calendar/days/2026-11-13", {"kind": "off"}, expect=403)
    owner.call("PUT", "/admin/calendar/days/2026-11-13", {"kind": "bad"}, expect=400)
    owner.call("PUT", "/admin/calendar/days/2026-11-13", {"kind": "off", "title": "Корпоративный выходной"})
    r = m.call("GET", "/calendar?year=2026")
    assert r["work_days"] == 246 and r["months"][10] == 19
    d = {x["date"]: x for x in r["days"]}["2026-11-13"]
    assert d["custom"] and d["kind"] == "off" and d["title"] == "Корпоративный выходной" and d["official_kind"] is None
    # Overriding a shipped holiday shows what the official calendar says.
    owner.call("PUT", "/admin/calendar/days/2026-12-31", {"kind": "work", "title": "Рабочий"})
    d = {x["date"]: x for x in m.call("GET", "/calendar?year=2026")["days"]}["2026-12-31"]
    assert d["kind"] == "work" and d["official_kind"] == "off"
    owner.call("DELETE", "/admin/calendar/days/2026-12-31")
    d = {x["date"]: x for x in m.call("GET", "/calendar?year=2026")["days"]}["2026-12-31"]
    assert d["kind"] == "off" and not d["custom"]
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Другое", "email": "x@example.com",
                                        "display_name": "Икс", "password": "owner-password-1"})
    assert other.call("GET", "/calendar?year=2026")["work_days"] == 247, "portal changes stay within the portal"
    # A year without official data: Labour Code holidays with weekend transfer.
    r = owner.call("GET", "/calendar?year=2028")
    assert r["official"] is False and r["days"] == []
    m.call("POST", "/admin/calendar/2028/holidays", expect=403)
    assert owner.call("POST", "/admin/calendar/2028/holidays")["added"] == 15
    assert owner.call("POST", "/admin/calendar/2028/holidays")["added"] == 0, "filling twice adds nothing"
    days = {x["date"]: x for x in owner.call("GET", "/calendar?year=2028")["days"]}
    assert days["2028-11-04"]["kind"] == "holiday" and days["2028-11-06"]["kind"] == "off"
    assert days["2028-11-06"]["title"] == "Перенос с 4 ноября"
    assert owner.call("POST", "/admin/calendar/2026/holidays")["added"] == 0, "official years are left alone"
    assert len(owner.call("GET", "/admin/audit?action=calendar.day_set")["entries"]) == 2
    step("Admins change days for their portal (corporate day off, reset to official) and fill a new year by the Labour Code")

    # --- Projects in working days -------------------------------------------
    pid = owner.call("POST", "/projects", {"name": "Обследование"})["id"]
    assert owner.call("GET", f"/projects/{pid}")["project"]["day_mode"] == "work", "new projects count working days"

    def task(title, start, due, **kw):
        return owner.call("POST", f"/projects/{pid}/tasks", {"title": title, "start_date": start, "due_date": due, **kw})["task"]

    # Saturday start → Monday; Sunday due → Friday.
    t = task("Выезд", "2026-11-07", "2026-11-15")
    assert (t["start_date"], t["due_date"], t["day_mode"]) == ("2026-11-09", "2026-11-12", "work"), t
    # (13.11 is the portal's corporate day off, so the due date lands on Thursday.)
    survey = task("Обмеры", "2026-12-21", "2026-12-25")
    report = task("Отчёт", "2026-12-28", "2026-12-30")  # 3 working days
    owner.call("POST", f"/tasks/{report['id']}/dependencies", {"depends_on_id": survey["id"]})
    r = owner.call("PATCH", f"/tasks/{survey['id']}", {"due_date": "2026-12-28"})
    moved = {x["id"]: x for x in r["shifted"]}
    # 29–30.12 work, 31.12 and 1–10.01 are off: three working days are 29.12, 30.12, 11.01.
    assert (moved[report["id"]]["start_date"], moved[report["id"]]["due_date"]) == ("2026-12-29", "2027-01-11"), moved
    # A lag of 2 working days.
    ms = task("Сдача", None, "2027-01-11", is_milestone=True)
    owner.call("POST", f"/tasks/{ms['id']}/dependencies", {"depends_on_id": report["id"], "lag_days": 2})
    ms = owner.call("GET", f"/tasks/{ms['id']}")["task"]
    assert ms["start_date"] == ms["due_date"] == "2027-01-13", ms
    g = owner.call("GET", f"/projects/{pid}/tasks")
    assert g["schedule"]["project_end"] == "2027-01-13"
    assert g["schedule"]["slack"][t["id"]] > 0
    step("Working-day projects: dates snap to working days, auto-shift skips weekends and New Year holidays, lags in working days")

    # --- Switching a project to calendar days -------------------------------
    owner.call("POST", f"/projects/{pid}/members", {"user_id": mid, "role": "member"})
    m.call("PATCH", f"/projects/{pid}", {"day_mode": "calendar"}, expect=403)
    owner.call("PATCH", f"/projects/{pid}", {"day_mode": "weeks"}, expect=400)
    owner.call("PATCH", f"/projects/{pid}", {"day_mode": "calendar"})
    assert owner.call("GET", f"/projects/{pid}")["project"]["day_mode"] == "calendar"
    t2 = task("Камеральные работы", "2026-11-07", "2026-11-08")
    assert (t2["start_date"], t2["due_date"]) == ("2026-11-07", "2026-11-08"), "calendar mode keeps weekend dates"
    r = owner.call("PATCH", f"/tasks/{survey['id']}", {"due_date": "2026-12-29"})
    moved = {x["id"]: x for x in r["shifted"]}
    assert (moved[report["id"]]["start_date"], moved[report["id"]]["due_date"]) == ("2026-12-30", "2027-01-12"), moved
    step("Calendar-day projects keep every day: weekends allowed, shifts by calendar days")

    print("ALL CALENDAR TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
