#!/usr/bin/env python3
"""End-to-end test of portal modules (Администрирование → Модули): switching
modules off hides them from /auth/me, blocks their API with 403
module_disabled, stops their notifications and keeps the data.

Usage:
    API=http://localhost:8080 python3 scripts/modules_test.py
(the backend should run with DUE_SCAN_SECS=2 DUE_REMINDER_HOUR=0 for the reminder check)
"""

import datetime
import os
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

ALL = ["projects", "planning", "time", "templates", "storage", "kb", "chats", "crm", "finance", "team"]


def set_off(c, *keys):
    return {m["key"]: m["enabled"] for m in c.call("PUT", "/admin/modules", {"disabled": list(keys)})["modules"]}


def blocked(c, method, path, module, body=None):
    r = c.call(method, path, body, expect=403)
    assert r["error"] == "module_disabled" and r["module"] == module, (path, r)


def main():
    slug = "mod" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    uid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк",
                                        "password": "m-password-123", "role": "member"})["id"]
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})

    r = owner.call("GET", "/admin/modules")["modules"]
    assert [x["key"] for x in r] == ALL and all(x["enabled"] and x["switched_on"] for x in r)
    assert owner.call("GET", "/auth/me")["modules"] == ALL
    m.call("GET", "/admin/modules", expect=403)
    m.call("PUT", "/admin/modules", {"disabled": ["kb"]}, expect=403)
    owner.call("PUT", "/admin/modules", {"disabled": ["mail"]}, expect=400)
    step("All modules on by default; only admins manage them; unknown keys rejected")

    # Data created while everything is on.
    pid = owner.call("POST", "/projects", {"name": "Корпус 12"})["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": uid, "role": "member"})
    folder = owner.call("GET", f"/projects/{pid}")["project"]["folder_id"]
    t1 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Обмеры"})["task"]
    t2 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Отчёт"})["task"]
    page = owner.call("POST", "/kb/pages", {"title": "Регламент"})
    page_id = page.get("id") or page["page"]["id"]

    # ------------------------------------------------------------ knowledge base
    en = set_off(owner, "kb")
    assert not en["kb"] and en["projects"]
    assert "kb" not in m.call("GET", "/auth/me")["modules"]
    blocked(m, "GET", "/kb/pages", "kb")
    blocked(m, "GET", f"/kb/pages/{page_id}", "kb")
    Client().call("GET", "/kb/pages", expect=401)  # not signed in: still 401
    set_off(owner)
    assert m.call("GET", f"/kb/pages/{page_id}")
    step("KB off: hidden from /auth/me, API 403 module_disabled; back on — the page is there")

    # ------------------------------------------------------------ planning, time, templates
    set_off(owner, "planning", "time", "templates")
    me = owner.call("GET", "/auth/me")["modules"]
    assert "projects" in me and not {"planning", "time", "templates"} & set(me)
    owner.call("GET", f"/projects/{pid}/tasks")
    owner.call("PATCH", f"/tasks/{t1['id']}", {"title": "Обмерные чертежи"})
    blocked(owner, "POST", f"/tasks/{t2['id']}/dependencies", "planning", {"depends_on_id": t1["id"]})
    blocked(owner, "POST", f"/projects/{pid}/baseline", "planning")
    blocked(owner, "POST", f"/projects/{pid}/reschedule", "planning")
    blocked(owner, "POST", f"/tasks/{t1['id']}/time", "time", {"work_date": "2026-11-11", "minutes": 30})
    blocked(owner, "GET", f"/tasks/{t1['id']}/time", "time")
    blocked(owner, "POST", f"/projects/{pid}/generate", "templates", {"template_id": str(uuid.uuid4())})
    set_off(owner)
    owner.call("POST", f"/tasks/{t2['id']}/dependencies", {"depends_on_id": t1["id"]})
    step("Planning, time and templates switch off separately; projects and tasks keep working")

    # ------------------------------------------------------------ projects (with children)
    en = set_off(owner, "projects")
    assert not any(en[k] for k in ("projects", "planning", "time", "templates")) and en["crm"]
    mods = owner.call("GET", "/admin/modules")["modules"]
    planning = next(x for x in mods if x["key"] == "planning")
    assert planning["switched_on"] and not planning["enabled"]
    for path in ["/projects", f"/projects/{pid}", "/tasks", f"/tasks/{t1['id']}", "/task-labels", "/project-fields"]:
        blocked(m, "GET", path, "projects")
    blocked(m, "GET", f"/tasks/{t1['id']}/time", "time")
    # Project documents stay reachable (links in chats, notifications).
    assert m.call("GET", f"/folders/{folder}")["folder"]["id"] == folder
    set_off(owner)
    assert [t["title"] for t in m.call("GET", "/tasks")["tasks"]] == ["Обмерные чертежи", "Отчёт"]
    step("Projects off: tasks, planning and time off with them; data back when switched on")

    # ------------------------------------------------------------ storage, chats, team, CRM
    set_off(owner, "storage", "chats", "team", "crm")
    for path, mod in [("/folders/roots", "storage"), ("/shared", "storage"), ("/trash", "storage"),
                      ("/chats", "chats"), ("/chats/unread", "chats"), ("/team", "team"),
                      ("/crm/companies", "crm"), ("/crm/deals", "crm"), ("/crm/tasks/summary", "crm"),
                      ("/crm/documents", "crm")]:
        blocked(m, "GET", path, mod)
    blocked(owner, "POST", f"/crm/deals/{uuid.uuid4()}/project", "crm", {})
    assert m.call("GET", "/auth/me")["crm_role"] == "none"
    assert m.call("GET", f"/folders/{folder}")  # project folder: part of projects
    set_off(owner, "projects")
    blocked(owner, "POST", f"/crm/deals/{uuid.uuid4()}/project", "projects", {})
    set_off(owner)
    assert m.call("GET", "/auth/me")["crm_role"] == "user"
    assert m.call("GET", "/folders/roots")["my"]
    step("Storage, chats, team and CRM off: sections blocked, CRM role reported as none")

    # ------------------------------------------------------------ notifications
    prefs = lambda c: {p["kind"] for p in c.call("GET", "/notifications/prefs")["prefs"]}
    assert "task_due_soon" in prefs(m) and "deal_assigned" in prefs(m)
    set_off(owner, "projects", "crm")
    p = prefs(m)
    assert not {"task_due_soon", "task_assigned", "checklist_assigned", "deal_assigned", "crm_task_due"} & p
    assert {"mention", "file_shared"} <= p
    set_off(owner)

    # A due-date reminder is not sent while projects are off, and is sent once they are back on.
    # The due date is set directly in the database while the module is off (the API is closed
    # then), so this part needs TEST_DATABASE_URL and psql.
    db_url = os.environ.get("TEST_DATABASE_URL")
    if db_url:
        import subprocess
        t3 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Сдать отчёт", "assignee_id": uid})["task"]
        set_off(owner, "projects")
        subprocess.run(["psql", db_url, "-qc", f"UPDATE tasks SET due_date = current_date WHERE id = '{t3['id']}'"], check=True)

        def due_notes():
            return [x for x in m.call("GET", "/notifications")["items"]
                    if x["kind"] in ("task_due_soon", "task_overdue") and "Сдать отчёт" in x["title"] + x.get("body", "")]

        time.sleep(5)
        assert due_notes() == [], "reminder sent while the module was off"
        set_off(owner)
        for _ in range(20):
            if due_notes():
                break
            time.sleep(1)
        assert due_notes(), "reminder not sent after switching the module back on"
    else:
        print("    (reminder check skipped: set TEST_DATABASE_URL)")
    step("Notifications: kinds of off modules hidden in settings; reminders paused while off")

    rows = owner.call("GET", "/admin/audit?action=portal.modules")["entries"]
    assert rows and rows[0]["action"] == "portal.modules"
    step("Changes are written to the audit log")

    print("ALL MODULE TESTS PASSED")


if __name__ == "__main__":
    main()
