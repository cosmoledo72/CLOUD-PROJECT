#!/usr/bin/env python3
"""End-to-end test of CRM to-dos: links to companies and deals, visibility,
rights, completion into the interaction history, reminders and counters.

The backend must run with short timers:
    NOTIFY_EMAIL_DELAY_SECS=2 DUE_SCAN_SECS=2 DUE_REMINDER_HOUR=0

Usage:
    API=http://localhost:8080 python3 scripts/crm_tasks_test.py
"""

import datetime as dt
import os
import sys
import time
import uuid
from zoneinfo import ZoneInfo

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

TZ = "Asia/Sakhalin"


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def notes(c, kind):
    return [n for n in c.call("GET", "/notifications?limit=100")["items"] if n["kind"] == kind]


def wait_for(fn, timeout=12, what="condition"):
    deadline = time.time() + timeout
    while time.time() < deadline:
        r = fn()
        if r:
            return r
        time.sleep(0.3)
    raise AssertionError(f"timed out waiting for {what}")


def main():
    slug = "crt" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    owner.call("PATCH", "/admin/portal", {"name": "Бюро", "timezone": TZ})
    ids = {}
    for key, name, crm in [("a", "Анна", "user"), ("b", "Борис", "user"), ("n", "Нина", "none")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": f"{key}-password-123"})["id"]
        owner.call("PATCH", f"/users/{ids[key]}", {"crm_role": crm})
    a, b, n = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abn")
    now = dt.datetime.now(ZoneInfo(TZ))
    today = now.date()
    d = lambda k: (today + dt.timedelta(days=k)).isoformat()  # noqa: E731

    open_co = a.call("POST", "/crm/companies", {"name": "ООО «Открытая»", "visibility": "all"})["id"]
    hidden = a.call("POST", "/crm/companies", {"name": "ООО «Личная»", "visibility": "responsible", "responsible_id": ids["a"]})["id"]
    contact = a.call("POST", "/crm/contacts", {"company_id": open_co, "last_name": "Петров", "first_name": "Иван", "phone": "+7 900 000-00-00"})["id"]
    other_contact = a.call("POST", "/crm/contacts", {"company_id": hidden, "last_name": "Чужой"})["id"]
    deal = a.call("POST", "/crm/deals", {"title": "Обследование склада", "company_id": open_co, "amount": 500000})["deal"]

    # ------------------------------------------------------------ create and validate
    t1 = a.call("POST", "/crm/tasks", {"kind": "call", "title": "Перезвонить по КП", "due_date": d(0), "due_time": "23:59",
                                       "deal_id": deal["id"], "contact_id": contact})["task"]
    assert t1["company_id"] == open_co and t1["company_name"] == "ООО «Открытая»" and t1["deal_title"] == "Обследование склада"
    assert t1["contact_name"] == "Петров Иван" and t1["contact_phone"] and t1["due_time"].startswith("23:59")
    assert t1["assignee_id"] == ids["a"] and t1["can_edit"]
    a.call("POST", "/crm/tasks", {"title": " ", "due_date": d(0)}, expect=400)
    a.call("POST", "/crm/tasks", {"kind": "party", "title": "x", "due_date": d(0)}, expect=400)
    a.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "due_time": "25:00"}, expect=400)
    a.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "deal_id": deal["id"], "company_id": hidden}, expect=400)
    a.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "company_id": open_co, "contact_id": other_contact}, expect=400)
    a.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "assignee_id": ids["n"]}, expect=400)  # no CRM
    a.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "company_id": hidden, "assignee_id": ids["b"]}, expect=400)  # can't see
    b.call("POST", "/crm/tasks", {"title": "x", "due_date": d(0), "company_id": hidden}, expect=404)
    n.call("GET", "/crm/tasks", expect=403)
    step("Tasks link to deal/company/contact; links, kinds, times and assignees are validated")

    # ------------------------------------------------------------ assignment and visibility
    t2 = a.call("POST", "/crm/tasks", {"kind": "email", "title": "Отправить КП", "due_date": d(1), "company_id": open_co,
                                       "assignee_id": ids["b"]})["task"]
    n1 = wait_for(lambda: notes(b, "crm_task_assigned"), what="assigned")
    assert n1[0]["title"] == "Вам поручено: Отправить КП" and "Анна" in n1[0]["body"] and n1[0]["link"] == f"/crm/companies/{open_co}"
    t3 = a.call("POST", "/crm/tasks", {"kind": "todo", "title": "Личное дело", "due_date": d(3), "company_id": hidden})["task"]
    standalone = b.call("POST", "/crm/tasks", {"kind": "meeting", "title": "Планёрка отдела продаж", "due_date": d(2)})["task"]
    mine_b = [t["title"] for t in b.call("GET", "/crm/tasks")["tasks"]]
    assert mine_b == ["Отправить КП", "Планёрка отдела продаж"], mine_b
    all_b = [t["title"] for t in b.call("GET", "/crm/tasks?scope=all")["tasks"]]
    assert "Личное дело" not in all_b and "Перезвонить по КП" in all_b
    b.call("GET", f"/crm/tasks?company_id={hidden}")  # filter on a hidden company returns nothing visible
    assert not b.call("GET", f"/crm/tasks?company_id={hidden}")["tasks"]
    by_deal = a.call("GET", f"/crm/tasks?deal_id={deal['id']}")["tasks"]
    assert [t["id"] for t in by_deal] == [t1["id"]]
    assert {t["id"] for t in a.call("GET", f"/crm/tasks?company_id={open_co}")["tasks"]} == {t1["id"], t2["id"]}
    assert a.call("GET", "/crm/tasks")["today"] == d(0)
    # Rights: B may edit the task assigned to him but not A's own task.
    b.call("PATCH", f"/crm/tasks/{t2['id']}", {"notes": "приложить сметы"})
    b.call("PATCH", f"/crm/tasks/{t1['id']}", {"title": "чужое"}, expect=403)
    b.call("DELETE", f"/crm/tasks/{t1['id']}", expect=403)
    b.call("PATCH", f"/crm/tasks/{t3['id']}", {"title": "x"}, expect=404)
    owner.call("PATCH", f"/crm/tasks/{t3['id']}", {"notes": "руководитель может"})
    step("Assignment notifies; lists: mine, all visible, by deal and company; edit rights")

    # ------------------------------------------------------------ deal card shows the next step
    dl = a.call("GET", f"/crm/deals/{deal['id']}")["deal"]
    assert dl["next_task_date"] == d(0) and dl["next_task_title"] == "Перезвонить по КП"
    kanban = {x["id"]: x for x in a.call("GET", "/crm/deals")["deals"]}
    assert kanban[deal["id"]]["next_task_title"] == "Перезвонить по КП"
    step("Deals show their nearest open task")

    # ------------------------------------------------------------ reminders
    # Timed task 23:59 today: reminder only within 15 minutes before; it is far away, so none.
    soon = (now + dt.timedelta(minutes=10)).strftime("%H:%M")
    tomorrow_cross = soon < now.strftime("%H:%M")  # near midnight the "soon" time wraps to tomorrow
    t_soon = a.call("POST", "/crm/tasks", {"title": "Звонок через 10 минут", "due_date": d(1 if tomorrow_cross else 0), "due_time": soon,
                                           "company_id": open_co})["task"]
    t_day = a.call("POST", "/crm/tasks", {"kind": "todo", "title": "Сегодня в течение дня", "due_date": d(0), "company_id": open_co})["task"]
    t_late = a.call("POST", "/crm/tasks", {"title": "Вчерашнее", "due_date": d(-1), "company_id": open_co})["task"]
    a.call("POST", "/crm/tasks", {"title": "Послезавтра", "due_date": d(2), "company_id": open_co})

    def titles(kind):
        return sorted(x["title"] for x in notes(a, kind))
    expected_due = ["Сегодня: Сегодня в течение дня", f"В {soon}: Звонок через 10 минут"]
    if tomorrow_cross:
        expected_due = ["Сегодня: Сегодня в течение дня"]
    wait_for(lambda: titles("crm_task_due") == sorted(expected_due), what=f"due reminders {expected_due}")
    wait_for(lambda: titles("crm_task_overdue") == ["Просрочено: Вчерашнее"], what="overdue")
    time.sleep(4)
    assert titles("crm_task_due") == sorted(expected_due), titles("crm_task_due")
    assert "Перезвонить по КП" not in " ".join(titles("crm_task_due")), "23:59 is not within 15 minutes"
    od = notes(a, "crm_task_overdue")[0]
    assert od["link"] == f"/crm/companies/{open_co}" and "ООО «Открытая»" in od["body"]
    # Moving the time gives a new reminder.
    if not tomorrow_cross:
        a.call("PATCH", f"/crm/tasks/{t_soon['id']}", {"due_time": (now + dt.timedelta(minutes=5)).strftime("%H:%M")})
        wait_for(lambda: len(titles("crm_task_due")) == 3, what="reminder for the new time")
    s = a.call("GET", "/crm/tasks/summary")
    assert s["overdue"] == 1 and s["today"] >= 2, s
    step("Reminders: 15 minutes before a timed task, in the morning for all-day tasks, overdue once; counters")

    # ------------------------------------------------------------ completion
    r = a.call("PATCH", f"/crm/tasks/{t1['id']}", {"done": True, "result": "Договорились: КП до пятницы"})["task"]
    assert r["done_at"] and r["done_by_name"] == "Анна" and r["result"] == "Договорились: КП до пятницы"
    acts = a.call("GET", f"/crm/deals/{deal['id']}")["activities"]
    last = [x for x in acts if x["kind"] == "call"]
    assert last and last[0]["body"] == "Перезвонить по КП: Договорились: КП до пятницы", last
    assert any(x["body"].startswith("Перезвонить по КП") for x in a.call("GET", f"/crm/companies/{open_co}")["activities"])
    a.call("PATCH", f"/crm/tasks/{t1['id']}", {"done": True})  # repeat: no second history entry
    assert len([x for x in a.call("GET", f"/crm/deals/{deal['id']}")["activities"] if x["kind"] == "call"]) == 1
    a.call("PATCH", f"/crm/tasks/{t_day['id']}", {"done": True})
    assert any(x["body"] == "Выполнено: Сегодня в течение дня" and x["kind"] == "note"
               for x in a.call("GET", f"/crm/companies/{open_co}")["activities"])
    done = a.call("GET", "/crm/tasks?status=done")["tasks"]
    assert [t["title"] for t in done][:2] == ["Сегодня в течение дня", "Перезвонить по КП"]
    assert t1["id"] not in {t["id"] for t in a.call("GET", "/crm/tasks")["tasks"]}
    a.call("PATCH", f"/crm/tasks/{t1['id']}", {"done": False})
    assert a.call("GET", f"/crm/deals/{deal['id']}")["deal"]["next_task_title"] in ("Перезвонить по КП", None)
    standalone_done = b.call("PATCH", f"/crm/tasks/{standalone['id']}", {"done": True, "result": "Провели"})["task"]
    assert standalone_done["done_at"]  # tasks without company/deal complete without history
    step("Completion records the result in the deal/company history once; reopen; done list")

    # ------------------------------------------------------------ reassignment, deletion, cascade
    a.call("PATCH", f"/crm/tasks/{t_late['id']}", {"assignee_id": ids["b"]})
    wait_for(lambda: [x for x in notes(b, "crm_task_assigned") if "Вчерашнее" in x["title"]], what="reassign notice")
    assert b.call("GET", "/crm/tasks/summary")["overdue"] == 1
    a.call("DELETE", f"/crm/tasks/{t_soon['id']}")
    a.call("DELETE", f"/crm/deals/{deal['id']}")
    remaining = {t["id"] for t in a.call("GET", "/crm/tasks?scope=all")["tasks"]}
    assert t1["id"] not in remaining and t_soon["id"] not in remaining
    # Tenant isolation.
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Другое", "email": "x@example.com",
                                        "display_name": "X", "password": "other-password-1"})
    assert not other.call("GET", "/crm/tasks?scope=all")["tasks"]
    other.call("PATCH", f"/crm/tasks/{t2['id']}", {"title": "x"}, expect=404)
    step("Reassignment notifies the new assignee; deleting a deal removes its tasks; tenant isolation")
    print("\nALL CRM TASK TESTS PASSED")


if __name__ == "__main__":
    main()
