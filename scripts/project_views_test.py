#!/usr/bin/env python3
"""End-to-end test of the project list views (F7): managers, members and
milestones in the list for the board views, filters and the summary Gantt;
bulk status and label changes with per-project permissions.

Usage:
    API=http://localhost:8080 python3 scripts/project_views_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "pv" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    for email, name in (("g@example.com", "Глеб"), ("m@example.com", "Марк")):
        ids[name] = owner.call("POST", "/users", {"email": email, "display_name": name, "password": "user-password-123", "role": "member"})["id"]
    gip, mem = Client(), Client()
    gip.call("POST", "/auth/login", {"tenant": slug, "email": "g@example.com", "password": "user-password-123"})
    mem.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "user-password-123"})

    # --- List data ----------------------------------------------------------
    a = gip.call("POST", "/projects", {"name": "Обследование школы"})["id"]
    b = gip.call("POST", "/projects", {"name": "Проект склада"})["id"]
    c = owner.call("POST", "/projects", {"name": "Реставрация усадьбы"})["id"]
    gip.call("POST", f"/projects/{a}/members", {"user_id": ids["Марк"], "role": "member"})
    owner.call("POST", f"/projects/{c}/members", {"user_id": ids["Глеб"], "role": "manager"})
    owner.call("POST", f"/projects/{c}/members", {"user_id": ids["Марк"], "role": "viewer"})
    gip.call("POST", f"/projects/{a}/tasks", {"title": "Выезд", "start_date": "2026-12-01", "due_date": "2026-12-04"})
    gip.call("POST", f"/projects/{a}/tasks", {"title": "Сдача отчёта", "due_date": "2026-12-18", "is_milestone": True})
    gip.call("POST", f"/projects/{a}/tasks", {"title": "Согласование", "due_date": "2026-12-10", "is_milestone": True})
    gip.call("POST", f"/projects/{a}/tasks", {"title": "Без даты", "is_milestone": True})

    r = mem.call("GET", "/projects")
    got = {p["id"]: p for p in r["projects"]}
    assert set(got) == {a, c}, "a member sees only own projects"
    assert got[a]["manager_ids"] == [ids["Глеб"]] and set(got[a]["member_ids"]) == {ids["Глеб"], ids["Марк"]}
    assert [m["title"] for m in got[a]["milestones"]] == ["Согласование", "Сдача отчёта"], got[a]["milestones"]
    assert got[a]["milestones"][0] == {"date": "2026-12-10", "title": "Согласование", "done": False}
    assert got[c]["milestones"] == []
    # Two managers on one project: both listed (alphabetically), the project shows in both columns.
    me = owner.call("GET", "/auth/me")["id"]
    assert got[c]["manager_ids"] == [ids["Глеб"], me], got[c]["manager_ids"]
    names = {p["id"]: p["display_name"] for p in r["people"]}
    assert names == {ids["Глеб"]: "Глеб", ids["Марк"]: "Марк", me: "Ольга"}, names
    assert len(owner.call("GET", "/projects")["projects"]) == 3
    step("The list carries managers, members, dated milestones and the people's names for views and filters")

    # --- Bulk changes -------------------------------------------------------
    gip.call("POST", "/projects/bulk", {"ids": [], "status": "paused"}, expect=400)
    gip.call("POST", "/projects/bulk", {"ids": [a], "status": "frozen"}, expect=400)
    gip.call("POST", "/projects/bulk", {"ids": [a], "status": "paused", "add_label": str(uuid.uuid4())}, expect=400)
    gip.call("POST", "/projects/bulk", {"ids": [a]}, expect=400)
    mem.call("POST", "/projects/bulk", {"ids": [a], "status": "paused"}, expect=403)
    gip.call("POST", "/projects/bulk", {"ids": [str(uuid.uuid4())], "status": "paused"}, expect=404)
    assert gip.call("POST", "/projects/bulk", {"ids": [a, b, c, a], "status": "paused"})["changed"] == 3
    assert gip.call("POST", "/projects/bulk", {"ids": [a, b], "status": "paused"})["changed"] == 0, "unchanged projects are not counted"
    assert {p["status"] for p in owner.call("GET", "/projects")["projects"]} == {"paused"}
    # The member manages none of them — all or nothing.
    owner.call("POST", f"/projects/{b}/members", {"user_id": ids["Марк"], "role": "manager"})
    mem.call("POST", "/projects/bulk", {"ids": [a, b], "status": "active"}, expect=403)
    assert mem.call("GET", f"/projects/{b}")["project"]["status"] == "paused"
    assert mem.call("POST", "/projects/bulk", {"ids": [b], "status": "active"})["changed"] == 1

    lab = owner.call("POST", "/project-labels", {"name": "Экспертиза", "color": "#2563eb"})["id"]
    gip.call("POST", "/projects/bulk", {"ids": [a], "add_label": str(uuid.uuid4())}, expect=400)
    assert gip.call("POST", "/projects/bulk", {"ids": [a, c], "add_label": lab})["changed"] == 2
    assert gip.call("POST", "/projects/bulk", {"ids": [a, b, c], "add_label": lab})["changed"] == 1, "already labelled projects are skipped"
    assert all(lab in p["label_ids"] for p in owner.call("GET", "/projects")["projects"])
    assert gip.call("POST", "/projects/bulk", {"ids": [a, b], "remove_label": lab})["changed"] == 2
    assert {p["id"] for p in owner.call("GET", "/projects")["projects"] if lab in p["label_ids"]} == {c}
    # Ten labels at most per project.
    many = [owner.call("POST", "/project-labels", {"name": f"Метка {i}"})["id"] for i in range(10)]
    owner.call("PUT", f"/projects/{a}/labels", {"label_ids": many})
    gip.call("POST", "/projects/bulk", {"ids": [a, b], "add_label": lab}, expect=400)
    assert lab not in mem.call("GET", f"/projects/{b}")["project"]["label_ids"], "a refused request changes nothing"

    audit = owner.call("GET", "/admin/audit?action=project.status_changed")["entries"]
    assert len(audit) == 4 and all(e["details"].get("bulk") for e in audit), len(audit)
    assert len(owner.call("GET", "/admin/audit?action=project.label_added")["entries"]) == 3
    assert len(owner.call("GET", "/admin/audit?action=project.label_removed")["entries"]) == 2
    step("Bulk status and label changes: managers of every chosen project only, all or nothing, ten-label limit, audit per project")

    print("ALL PROJECT VIEWS TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
