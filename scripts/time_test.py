#!/usr/bin/env python3
"""End-to-end test of time tracking (F3): the timer, editable time entries
with rights and audit, labour pay of tasks (hourly by the employee's rate or
a fixed amount for the volume), hour rates, labour cost in the project's
finances and the "Отчёты" report with its XLSX export.

Usage:
    API=http://localhost:8080 python3 scripts/time_test.py
"""

import io
import os
import sys
import time
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "tm" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    for key, name in (("gip", "Глеб"), ("eng", "Егор"), ("other", "Ося"), ("fin", "Фаина")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name,
                                                  "password": "user-password-123", "role": "member"})["id"]
    owner.call("PATCH", f"/users/{ids['fin']}", {"finance_role": "view"})
    c = {}
    for key in ids:
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    gip, eng, other, fin = c["gip"], c["eng"], c["other"], c["fin"]
    today = owner.call("GET", "/crm/tasks")["today"]

    pid = gip.call("POST", "/projects", {"name": "Обследование школы"})["id"]
    gip.call("POST", f"/projects/{pid}/members", {"user_id": ids["eng"], "role": "member"})
    gip.call("POST", f"/projects/{pid}/members", {"user_id": ids["other"], "role": "viewer"})
    t1 = gip.call("POST", f"/projects/{pid}/tasks", {"title": "Обмеры"})["task"]["id"]
    t2 = gip.call("POST", f"/projects/{pid}/tasks", {"title": "Отчёт"})["task"]["id"]
    sub = gip.call("POST", f"/projects/{pid}/tasks", {"title": "Фотофиксация", "parent_id": t1})["task"]["id"]

    # --- Entries --------------------------------------------------------------
    e1 = eng.call("POST", f"/tasks/{t1}/time", {"work_date": "2026-11-02", "minutes": 90, "note": "  первый выезд "})["id"]
    eng.call("POST", f"/tasks/{t1}/time", {"work_date": "2026-11-02", "minutes": 0}, expect=400)
    eng.call("POST", f"/tasks/{t1}/time", {"work_date": "2026-11-02", "minutes": 1441}, expect=400)
    other.call("POST", f"/tasks/{t1}/time", {"work_date": "2026-11-02", "minutes": 30}, expect=403)
    gip_e = gip.call("POST", f"/tasks/{sub}/time", {"work_date": "2026-11-03", "minutes": 60})["id"]
    r = other.call("GET", f"/tasks/{t1}/time")
    assert r["total_minutes"] == 90 and r["can_log"] is False
    assert r["entries"][0]["note"] == "первый выезд" and r["entries"][0]["can_edit"] is False and r["entries"][0]["source"] == "manual"
    assert r["entries"][0]["hour_rate"] is None
    assert eng.call("GET", f"/tasks/{t1}/time")["entries"][0]["can_edit"] is True
    # Own entries — the author; any entry — the project manager.
    eng.call("PATCH", f"/time/{e1}", {"minutes": 120, "note": "выезд, обмеры"})
    eng.call("PATCH", f"/time/{gip_e}", {"minutes": 10}, expect=403)
    other.call("PATCH", f"/time/{e1}", {"minutes": 10}, expect=403)
    gip.call("PATCH", f"/time/{e1}", {"work_date": "2026-11-04"})
    eng.call("PATCH", f"/time/{e1}", {"minutes": 2000}, expect=400)
    e = eng.call("GET", f"/tasks/{t1}/time")["entries"][0]
    assert (e["minutes"], e["work_date"], e["note"]) == (120, "2026-11-04", "выезд, обмеры")
    tmp = eng.call("POST", f"/tasks/{t2}/time", {"work_date": "2026-11-05", "minutes": 15})["id"]
    eng.call("DELETE", f"/time/{tmp}")
    eng.call("DELETE", f"/time/{tmp}", expect=404)
    assert len(owner.call("GET", "/admin/audit?action=time.updated")["entries"]) == 2
    d = owner.call("GET", "/admin/audit?action=time.deleted")["entries"]
    assert len(d) == 1 and d[0]["details"]["minutes"] == 15
    step("Entries with comments: members log, authors and project managers edit and delete, audit kept")

    # --- Timer ----------------------------------------------------------------
    assert eng.call("GET", "/time/timer")["timer"] is None
    other.call("POST", f"/tasks/{t1}/timer", expect=403)
    r = eng.call("POST", f"/tasks/{t1}/timer")
    assert r["timer"]["task_id"] == t1 and r["timer"]["task_title"] == "Обмеры" and r["stopped"] is None
    assert eng.call("POST", f"/tasks/{t1}/timer")["timer"]["task_id"] == t1, "starting again keeps the running timer"
    # Under a minute: switching records nothing.
    r = eng.call("POST", f"/tasks/{t2}/timer")
    assert r["timer"]["task_id"] == t2 and r["stopped"] is None
    t = eng.call("GET", "/time/timer")["timer"]
    assert t["project_name"] == "Обследование школы" and t["elapsed_seconds"] >= 0
    # Pretend the timer has run for 1 h 20 min, then stop with a comment.
    import subprocess
    subprocess.run(["psql", "-qc", f"UPDATE time_timers SET started_at = now() - interval '80 minutes' WHERE user_id = '{ids['eng']}'"], check=True)
    r = eng.call("POST", "/time/timer/stop", {"note": "черновик отчёта"})
    assert r["entry"]["minutes"] == 80, r
    e = eng.call("GET", f"/tasks/{t2}/time")["entries"][0]
    assert (e["minutes"], e["source"], e["note"], e["work_date"]) == (80, "timer", "черновик отчёта", today)
    assert eng.call("GET", "/time/timer")["timer"] is None
    assert eng.call("POST", "/time/timer/stop")["entry"] is None
    # Discard drops the time.
    eng.call("POST", f"/tasks/{t1}/timer")
    eng.call("DELETE", "/time/timer")
    assert eng.call("GET", "/time/timer")["timer"] is None
    # The timer is personal.
    gip.call("POST", f"/tasks/{t1}/timer")
    assert eng.call("GET", "/time/timer")["timer"] is None and gip.call("GET", "/time/timer")["timer"]["task_id"] == t1
    gip.call("DELETE", "/time/timer")
    step("Timer: one per employee, switching tasks stops the previous, stop records the time with a comment, discard drops it")

    # --- Rates and pay ----------------------------------------------------------
    eng.call("GET", "/time/rates", expect=403)
    gip.call("GET", "/time/rates", expect=403)
    r = fin.call("GET", "/time/rates")
    assert r["can_edit"] is False and {x["display_name"] for x in r["rates"]} >= {"Глеб", "Егор"}
    fin.call("PUT", f"/time/rates/{ids['eng']}", {"hour_rate": 1000}, expect=403)
    owner.call("PUT", f"/time/rates/{ids['eng']}", {"hour_rate": -5}, expect=400)
    # Old entries keep no rate unless asked; here — apply to the unrated ones.
    r = owner.call("PUT", f"/time/rates/{ids['eng']}", {"hour_rate": 1200, "apply_to_unrated": True})
    assert r["applied"] == 2, r
    owner.call("PUT", f"/time/rates/{ids['gip']}", {"hour_rate": 1500})
    # A rate change does not touch entries that already have a rate.
    owner.call("PUT", f"/time/rates/{ids['eng']}", {"hour_rate": 1800})
    eng.call("POST", f"/tasks/{t1}/time", {"work_date": "2026-11-06", "minutes": 30})  # at 1800
    assert {x["display_name"]: x["hour_rate"] for x in owner.call("GET", "/time/rates")["rates"]}["Егор"] == 1800
    assert len(owner.call("GET", "/admin/audit?action=user.rate_changed")["entries"]) == 3

    eng.call("GET", f"/tasks/{t1}/pay", expect=403)
    p = gip.call("GET", f"/tasks/{t1}/pay")
    assert p["pay_mode"] == "hourly" and p["can_edit"] and p["sees_rates"] is False and p["labor"] is None
    p = fin.call("GET", f"/tasks/{t1}/pay")
    # t1: 120 min × 1200 + 30 min × 1800 = 2400 + 900
    assert p["can_edit"] is False and p["labor"]["accrued"] == 3300.0 and p["minutes"] == 150, p
    assert fin.call("GET", f"/tasks/{t1}/time")["entries"][0]["hour_rate"] == 1800
    fin.call("PUT", f"/tasks/{t2}/pay", {"pay_mode": "fixed", "pay_amount": 25000}, expect=403)
    gip.call("PUT", f"/tasks/{t2}/pay", {"pay_mode": "fixed"}, expect=400)
    gip.call("PUT", f"/tasks/{t2}/pay", {"pay_mode": "piece"}, expect=400)
    gip.call("PUT", f"/tasks/{t2}/pay", {"pay_mode": "fixed", "pay_amount": 25000})
    p = fin.call("GET", f"/tasks/{t2}/pay")
    assert p["pay_mode"] == "fixed" and p["labor"] == {"accrued": 0.0, "expected": 25000.0}
    assert len(owner.call("GET", "/admin/audit?action=task.pay_changed")["entries"]) == 1

    f = owner.call("GET", f"/projects/{pid}/finance")
    # Hourly: t1 3300 + sub 60 min × 1500 (rate set after — not applied) = 3300; fixed t2 not done yet.
    assert (f["labor"], f["labor_expected"]) == (3300.0, 25000.0), (f["labor"], f["labor_expected"])
    gip.call("PATCH", f"/tasks/{t2}", {"status": "done"})
    f = owner.call("GET", f"/projects/{pid}/finance")
    assert (f["labor"], f["labor_expected"]) == (28300.0, 0.0), (f["labor"], f["labor_expected"])
    assert f["cash"] == -28300.0
    gip.call("PATCH", f"/tasks/{t2}", {"status": "in_progress"})
    assert owner.call("GET", f"/projects/{pid}/finance")["labor_expected"] == 25000.0
    gip.call("PATCH", f"/tasks/{t2}", {"status": "done"})
    step("Hour rates fixed on each entry; task pay hourly or fixed (counted when done); labour in the project's finances")

    # --- Report -----------------------------------------------------------------
    q = f"/time/report?from=2026-11-01&to={max(today, '2026-11-30')}"
    r = owner.call("GET", q)
    assert r["sees_money"] and r["totals"]["minutes"] == 120 + 60 + 80 + 30
    assert r["totals"]["hourly_cost"] == 3300.0, r["totals"]
    assert r["totals"]["fixed_cost"] == 25000.0 and r["fixed"][0]["task_title"] == "Отчёт"
    sub_row = [x for x in r["entries"] if x["task_id"] == sub][0]
    assert sub_row["parent_title"] == "Обмеры" and sub_row["cost"] is None and sub_row["user_name"] == "Глеб"
    fixed_row = [x for x in r["entries"] if x["task_id"] == t2][0]
    assert fixed_row["pay_mode"] == "fixed" and fixed_row["cost"] is None
    # An employee sees only their own hours, without money.
    r = eng.call("GET", q)
    assert {x["user_name"] for x in r["entries"]} == {"Егор"} and r["totals"]["minutes"] == 230
    assert r["sees_money"] is False and r["totals"]["hourly_cost"] is None and all(x["cost"] is None for x in r["entries"]) and r["fixed"] == []
    # A project manager sees everyone in their projects.
    r = gip.call("GET", q)
    assert r["totals"]["minutes"] == 290 and r["sees_money"] is False
    assert {p["display_name"] for p in r["people"]} == {"Глеб", "Егор", "Ося"}
    # Filters.
    assert owner.call("GET", q + f"&users={ids['gip']}")["totals"]["minutes"] == 60
    assert owner.call("GET", q + "&statuses=done")["totals"]["minutes"] == 80
    assert owner.call("GET", q + f"&projects={uuid.uuid4()}")["totals"]["minutes"] == 0
    assert owner.call("GET", "/time/report?from=2026-11-05&to=2026-11-05")["entries"] == []
    owner.call("GET", "/time/report?from=2026-11-10&to=2026-11-01", expect=400)
    owner.call("GET", q + "&statuses=lost", expect=400)
    owner.call("GET", q + "&users=nope", expect=400)
    # Another project's manager sees nothing of this one.
    pid2 = other.call("POST", "/projects", {"name": "Чужой"})["id"]
    assert other.call("GET", q)["entries"] == [] and pid2

    # XLSX
    raw = owner.call("GET", q.replace("/time/report?", "/time/report.xlsx?"), raw=True)
    z = zipfile.ZipFile(io.BytesIO(raw))
    wb = z.read("xl/workbook.xml").decode()
    assert "Записи" in wb and "Итоги" in wb and "Фикс. оплата" in wb, wb
    strings = z.read("xl/sharedStrings.xml").decode()
    assert "черновик отчёта" in strings and "Ставка, ₽/ч" in strings
    raw = eng.call("GET", q.replace("/time/report?", "/time/report.xlsx?"), raw=True)
    z = zipfile.ZipFile(io.BytesIO(raw))
    assert "Фикс. оплата" not in z.read("xl/workbook.xml").decode()
    assert "Ставка" not in z.read("xl/sharedStrings.xml").decode()
    step("Report: own hours for employees, project hours for managers, money for finance; filters; XLSX with entries, totals, fixed pay")

    print("ALL TIME TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
