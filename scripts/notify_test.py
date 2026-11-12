#!/usr/bin/env python3
"""End-to-end test of notifications: events from tasks, comments, mentions,
shares and the CRM; preferences; the live stream; email digests; due-date
reminders in the portal time zone.

The backend must run with short timers and a local SMTP server allowed:
    NOTIFY_EMAIL_DELAY_SECS=2 DUE_SCAN_SECS=2 DUE_REMINDER_HOUR=0 SMTP_ALLOW_PRIVATE=true

Usage:
    API=http://localhost:8080 python3 scripts/notify_test.py
"""

import datetime as dt
import json
import os
import sys
import threading
import time
import uuid
from zoneinfo import ZoneInfo

sys.path.insert(0, os.path.dirname(__file__))
import mail_test as mt  # noqa: E402
from smoke_test import API, Client, step  # noqa: E402


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def items(c, **q):
    qs = "&".join(f"{k}={v}" for k, v in q.items())
    return c.call("GET", "/notifications" + (f"?{qs}" if qs else ""))


def kinds(c):
    return [n["kind"] for n in items(c)["items"]]


def wait_for(fn, timeout=12, what="condition"):
    deadline = time.time() + timeout
    while time.time() < deadline:
        r = fn()
        if r:
            return r
        time.sleep(0.3)
    raise AssertionError(f"timed out waiting for {what}")


class Stream:
    """Reads the SSE stream of a client in a background thread."""

    def __init__(self, client):
        self.events = []
        self.ready = threading.Event()
        self.resp = client.opener.open(API + "/notifications/stream", timeout=60)
        threading.Thread(target=self.run, daemon=True).start()
        assert self.ready.wait(5), "stream did not start"

    def run(self):
        ev = None
        try:
            for raw in self.resp:
                line = raw.decode().rstrip("\n").rstrip("\r")
                if line.startswith("event:"):
                    ev = line[6:].strip()
                elif line.startswith("data:") and ev == "hello":
                    self.ready.set()
                elif line.startswith("data:") and ev == "message":
                    self.events.append(json.loads(line[5:].strip()))
        except Exception:
            pass

    def close(self):
        self.resp.close()


def main():
    port = mt.start_sink()
    slug = "ntf" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    owner.call("PUT", "/admin/mail", {"enabled": True, "host": "127.0.0.1", "port": port, "security": "none",
                                      "from_email": "portal@example.com", "from_name": "Бюро"})
    ids = {}
    for key, name in [("a", "Анна"), ("b", "Борис"), ("v", "Вера")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name,
                                                 "password": f"{key}-password-123"})["id"]
    owner_id = owner.call("GET", "/auth/me")["id"]
    a, b, v = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abv")

    # ------------------------------------------------------------ preferences
    prefs = a.call("GET", "/notifications/prefs")
    pk = {p["kind"]: p for p in prefs["prefs"]}
    assert pk["task_assigned"]["in_app"] and pk["task_assigned"]["email"] and prefs["email_available"]
    a.call("PUT", "/notifications/prefs", [{"kind": "nope", "in_app": True, "email": True}], expect=400)
    step("Preferences list kinds with defaults; unknown kinds rejected")

    # ------------------------------------------------------------ live stream + assignment
    stream = Stream(a)
    pid = owner.call("POST", "/projects", {"name": "Обследование корпуса 3"})["id"]
    for key in "abv":
        owner.call("POST", f"/projects/{pid}/members", {"user_id": ids[key], "role": "member"})
    added = wait_for(lambda: [e for e in stream.events if e["type"] == "notification"
                              and e["item"]["kind"] == "project_added"], what="project_added over SSE")
    assert "Обследование корпуса 3" in added[0]["item"]["title"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["a"], "role": "manager"})  # role change: no repeat
    task = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Обмерные работы", "assignee_id": ids["a"]})["task"]
    ev = wait_for(lambda: [e for e in stream.events if e["type"] == "notification"
                           and e["item"]["kind"] == "task_assigned"], what="task_assigned over SSE")[0]["item"]
    assert ev["actor_name"] == "Ольга" and ev["link"] == f"/projects/{pid}?task={task['id']}"
    assert kinds(a).count("project_added") == 1
    # Assigning yourself does not notify.
    a.call("POST", f"/projects/{pid}/tasks", {"title": "Своя задача", "assignee_id": ids["a"]})
    assert kinds(a).count("task_assigned") == 1
    step("Live stream delivers project and assignment notices; no self-notifications or repeats")

    # ------------------------------------------------------------ email digest
    msg = mt.wait_mail("a@example.com", "Обмерные работы", timeout=15)
    assert "/projects/" in mt.text_of(msg)
    # Read in the app before the delay: no email.
    owner.call("PATCH", f"/tasks/{task['id']}", {"assignee_id": ids["b"]})
    nb = wait_for(lambda: [n for n in items(b)["items"] if n["kind"] == "task_assigned"], what="b assigned")
    b.call("POST", "/notifications/read", {"ids": [nb[0]["id"]]})
    time.sleep(5)
    assert not [m for r, m in mt.INBOX if "b@example.com" in r], "read notification must not be emailed"
    step("Unread notifications are emailed after a delay; read ones are not")

    # ------------------------------------------------------------ comments and mentions
    tid = task["id"]
    v.call("POST", f"/tasks/{tid}/comments", {"body": "Нужны ли обмеры подвала? @Анна", "mentions": [ids["a"], "00000000-0000-0000-0000-000000000000"]})
    na = wait_for(lambda: [n for n in items(a)["items"] if n["kind"] == "mention"], what="mention")
    assert "Вера" in na[0]["title"] and "подвала" in na[0]["body"]
    nb = wait_for(lambda: [n for n in items(b)["items"] if n["kind"] == "task_comment"], what="comment for assignee")
    assert "Вера:" in nb[0]["body"]
    on = [n for n in items(owner)["items"] if n["kind"] == "task_comment"]
    assert on, "task author hears about comments"
    assert "task_comment" not in kinds(a), "a mention replaces the plain comment notice"
    b.call("POST", f"/tasks/{tid}/comments", {"body": "Да, нужны"})
    assert wait_for(lambda: [n for n in items(v)["items"] if n["kind"] == "task_comment"], what="earlier commenter")
    cs = a.call("GET", f"/tasks/{tid}/comments")["comments"]
    assert [c["author_name"] for c in cs] == ["Вера", "Борис"]
    a.call("DELETE", f"/task-comments/{cs[1]['id']}")  # a is project manager now
    v.call("DELETE", f"/task-comments/{cs[0]['id']}")  # own comment
    assert not a.call("GET", f"/tasks/{tid}/comments")["comments"]
    # Outsiders cannot comment or be mentioned into a project they cannot see.
    x_id = owner.call("POST", "/users", {"email": "x@example.com", "display_name": "Посторонний", "password": "x-password-123"})["id"]
    x = login(slug, "x@example.com", "x-password-123")
    x.call("POST", f"/tasks/{tid}/comments", {"body": "?"}, expect=404)
    v.call("POST", f"/tasks/{tid}/comments", {"body": "@Посторонний", "mentions": [x_id]})
    time.sleep(0.5)
    assert not items(x)["items"]
    step("Comments notify participants; mentions only reach users who can see the task")

    # ------------------------------------------------------------ done, shares
    owner.call("POST", f"/projects/{pid}/tasks", {"title": "Отчёт", "assignee_id": ids["b"]})
    t2 = [t for t in owner.call("GET", f"/projects/{pid}/tasks")["tasks"] if t["title"] == "Отчёт"][0]
    b.call("PATCH", f"/tasks/{t2['id']}", {"status": "done"})
    assert wait_for(lambda: [n for n in items(owner)["items"] if n["kind"] == "task_done"], what="task_done")
    roots = owner.call("GET", "/folders/roots")
    sub = owner.call("POST", "/folders", {"parent_id": roots["my"], "name": "Исходные данные"})["id"]
    gid = owner.call("POST", "/groups", {"name": "Инженеры"})["id"]
    owner.call("PUT", f"/groups/{gid}/members", {"user_ids": [ids["a"], ids["v"]]})
    owner.call("POST", "/shares", {"resource_type": "folder", "resource_id": sub, "group_id": gid, "access": 3})
    for c in (a, v):
        n = wait_for(lambda: [n for n in items(c)["items"] if n["kind"] == "file_shared"], what="file_shared")
        assert "Исходные данные" in n[0]["title"] and "редактирование" in n[0]["body"] and n[0]["link"] == f"/files/{sub}"
    assert "file_shared" not in kinds(b)
    step("Done tasks notify their author; shares notify the user or group members")

    # ------------------------------------------------------------ CRM
    deal = b.call("POST", "/crm/deals", {"title": "Обследование ТЦ", "amount": 1500000, "responsible_id": ids["a"]})["deal"]
    n = wait_for(lambda: [n for n in items(a)["items"] if n["kind"] == "deal_assigned"], what="deal_assigned")
    assert "1 500 000 ₽" in n[0]["body"]
    stages = b.call("GET", "/crm/stages")["stages"]
    b.call("PATCH", f"/crm/deals/{deal['id']}", {"stage_id": stages[1]["id"]})
    assert wait_for(lambda: [n for n in items(a)["items"] if n["kind"] == "deal_stage"], what="deal_stage")
    a.call("PATCH", f"/crm/deals/{deal['id']}", {"stage_id": stages[2]["id"]})  # own move: no notice
    assert kinds(a).count("deal_stage") == 1
    comp = b.call("POST", "/crm/companies", {"name": "ООО «Ромашка»", "responsible_id": ids["v"], "visibility": "responsible"})["id"]
    n = wait_for(lambda: [n for n in items(v)["items"] if n["kind"] == "company_assigned"], what="company_assigned")
    assert n[0]["title"].endswith("ООО «Ромашка»")
    # Mention in a CRM note: only users who can see the company get it.
    v.call("POST", "/crm/activities", {"company_id": comp, "kind": "call", "body": "Договорились о встрече, @Анна @Ольга",
                                       "mentions": [ids["a"], owner_id]})
    assert wait_for(lambda: [n for n in items(owner)["items"] if n["kind"] == "mention" and "Ромашка" in n["title"]], what="crm mention")
    time.sleep(0.5)
    assert not [n for n in items(a)["items"] if n["kind"] == "mention" and "Ромашка" in n["title"]], "hidden company"
    v.call("POST", "/crm/activities", {"deal_id": deal["id"], "kind": "note", "body": "@Анна посмотри", "mentions": [ids["a"]]})
    assert wait_for(lambda: [n for n in items(a)["items"] if n["kind"] == "mention" and "Обследование ТЦ" in n["title"]], what="deal mention")
    # Users without CRM do not see CRM kinds in their settings.
    owner.call("PATCH", f"/users/{ids['b']}", {"crm_role": "none"})
    assert "deal_assigned" not in {p["kind"] for p in b.call("GET", "/notifications/prefs")["prefs"]}
    step("CRM: responsible changes, stage moves by others, mentions respect visibility")

    # ------------------------------------------------------------ preferences take effect
    a.call("PUT", "/notifications/prefs", [{"kind": "task_assigned", "in_app": False, "email": False},
                                           {"kind": "mention", "in_app": True, "email": False}])
    pk = {p["kind"]: p for p in a.call("GET", "/notifications/prefs")["prefs"]}
    assert not pk["task_assigned"]["in_app"] and not pk["mention"]["email"] and pk["mention"]["in_app"]
    before = kinds(a).count("task_assigned")
    owner.call("POST", f"/projects/{pid}/tasks", {"title": "Не сообщать", "assignee_id": ids["a"]})
    time.sleep(0.5)
    assert kinds(a).count("task_assigned") == before
    a.call("PUT", "/notifications/prefs", [{"kind": "task_assigned", "in_app": True, "email": True}])  # back to default
    step("Turned-off kinds are not delivered")

    # ------------------------------------------------------------ read state
    unread = items(a)["unread"]
    assert unread > 0 and items(a, unread="true")["items"]
    a.call("POST", "/notifications/read", {}, expect=400)
    r = a.call("POST", "/notifications/read", {"all": True})
    assert r["unread"] == 0 and items(a)["unread"] == 0
    assert wait_for(lambda: [e for e in stream.events if e["type"] == "read" and e["unread"] == 0], what="read event")
    page1 = items(a, limit=2)
    assert len(page1["items"]) == 2 and page1["has_more"]
    page2 = items(a, limit=2, before=page1["items"][-1]["created_at"].replace("+", "%2B"))
    assert page2["items"][0]["id"] not in {i["id"] for i in page1["items"]}
    # Nobody can mark or read another user's notifications.
    other_ids = [n["id"] for n in items(v)["items"]]
    a.call("POST", "/notifications/read", {"ids": other_ids})
    assert items(v)["unread"] == len([n for n in items(v)["items"] if not n["read_at"]]) > 0
    stream.close()
    step("Read all / by id, paging, per-user isolation, read events over the stream")

    # ------------------------------------------------------------ due-date reminders
    owner.call("PATCH", "/admin/portal", {"name": "Бюро", "timezone": "Mars/Olympus"}, expect=400)
    owner.call("PATCH", "/admin/portal", {"name": "Бюро", "timezone": "Asia/Sakhalin"})
    assert owner.call("GET", "/admin/portal")["timezone"] == "Asia/Sakhalin"
    # Calendar days: in a working-days project a due date on a day off moves to a working day.
    owner.call("PATCH", f"/projects/{pid}", {"day_mode": "calendar"})
    today = dt.datetime.now(ZoneInfo("Asia/Sakhalin")).date()
    d = lambda n: (today + dt.timedelta(days=n)).isoformat()  # noqa: E731
    mk = lambda title, due, who="v": owner.call("POST", f"/projects/{pid}/tasks", {"title": title, "due_date": due, "assignee_id": ids[who]})["task"]  # noqa: E731
    t_tom, t_today, t_late, t_far, t_old = mk("Завтра", d(1)), mk("Сегодня", d(0)), mk("Опоздали", d(-2)), mk("Нескоро", d(5)), mk("Давно", d(-30))
    t_done = mk("Сделано", d(-1))
    owner.call("PATCH", f"/tasks/{t_done['id']}", {"status": "done"})

    def due_titles():
        return sorted(n["title"] for n in items(v, limit=100)["items"] if n["kind"] in ("task_due_soon", "task_overdue"))
    got = wait_for(lambda: (lambda t: t if len(t) >= 3 else None)(due_titles()), what="due reminders")
    assert got == ["Завтра срок: Завтра", "Просрочена задача: Опоздали", "Сегодня срок: Сегодня"], got
    time.sleep(5)  # more scans
    assert due_titles() == got, "reminders are not repeated"
    # Moving the due date gives a new reminder for the new date.
    owner.call("PATCH", f"/tasks/{t_late['id']}", {"due_date": d(-1)})
    wait_for(lambda: due_titles().count("Просрочена задача: Опоздали") == 2, what="new overdue date")
    step("Due-date reminders: today, tomorrow, overdue — once per date, portal time zone, done tasks skipped")

    print("\nALL NOTIFICATION TESTS PASSED")


if __name__ == "__main__":
    main()
