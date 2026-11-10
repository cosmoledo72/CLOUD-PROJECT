#!/usr/bin/env python3
"""End-to-end test of the knowledge base, chats, team directory and the
finance filters of the sales-document registry.

Usage:
    API=http://localhost:8080 python3 scripts/kb_chat_test.py
"""

import os
import sys
import time
import uuid
from urllib.parse import quote

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402
from notify_test import Stream  # noqa: E402


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def wait_for(fn, timeout=8, what="condition"):
    deadline = time.time() + timeout
    while time.time() < deadline:
        r = fn()
        if r:
            return r
        time.sleep(0.2)
    raise AssertionError(f"timed out waiting for {what}")


def main():
    slug = "kbc" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    for key, name, role in [("a", "Анна", "member"), ("b", "Борис", "member"), ("c", "Вера", "member"), ("g", "Гость", "guest")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name,
                                                 "password": f"{key}-password-123", "role": role})["id"]
    a, b, c, g = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abcg")

    # ------------------------------------------------------------ knowledge base
    root = a.call("POST", "/kb/pages", {"title": "Регламенты", "icon": "📘", "body": "# Регламенты\n\nОбщие правила."})["id"]
    child = a.call("POST", "/kb/pages", {"parent_id": root, "title": "Отпуска",
                                         "body": "Заявление на отпуск подаётся за две недели до начала отпуска."})["id"]
    grand = b.call("POST", "/kb/pages", {"parent_id": child, "title": "Шаблон заявления"})["id"]
    a.call("POST", "/kb/pages", {"title": "  "}, expect=400)
    a.call("POST", "/kb/pages", {"title": "x", "icon": "слишком"}, expect=400)
    g.call("GET", "/kb/pages", expect=403)
    tree = b.call("GET", "/kb/pages")["pages"]
    assert {p["id"] for p in tree} == {root, child, grand}
    pg = b.call("GET", f"/kb/pages/{grand}")
    assert [x["title"] for x in pg["breadcrumbs"]] == ["Регламенты", "Отпуска"] and pg["can_delete"]
    assert not b.call("GET", f"/kb/pages/{child}")["can_delete"]
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Чужой", "email": "x@example.com",
                                        "display_name": "Икс", "password": "other-password-1"})
    other.call("GET", f"/kb/pages/{root}", expect=404)
    step("KB: tree, breadcrumbs, validation, guests and other portals excluded")

    hits = b.call("GET", "/kb/search?q=" + quote("отпуском"))["results"]  # stemmed match
    assert hits and hits[0]["id"] == child and "«" in hits[0]["snippet"]
    assert b.call("GET", "/kb/search?q=" + quote("Шабл"))["results"][0]["id"] == grand  # title prefix
    assert b.call("GET", "/kb/search?q=%25")["results"] == []
    step("KB: Russian full-text search with snippets and title substring match")

    base = a.call("GET", f"/kb/pages/{child}")["page"]["updated_at"]
    upd = b.call("PUT", f"/kb/pages/{child}", {"title": "Отпуска и отгулы", "body": "Новый текст", "base_updated_at": base})["page"]
    assert upd["updated_by_name"] == "Борис"
    a.call("PUT", f"/kb/pages/{child}", {"title": "Мои правки", "body": "...", "base_updated_at": base}, expect=409)
    a.call("PATCH", f"/kb/pages/{root}/move", {"parent_id": grand}, expect=400)  # cycle
    a.call("PATCH", f"/kb/pages/{root}/move", {"parent_id": root}, expect=400)
    a.call("PATCH", f"/kb/pages/{grand}/move", {"parent_id": None, "sort_order": 5})
    assert b.call("GET", f"/kb/pages/{grand}")["breadcrumbs"] == []
    b.call("DELETE", f"/kb/pages/{root}", expect=403)
    assert a.call("DELETE", f"/kb/pages/{root}")["deleted"] == 2
    assert [p["id"] for p in owner.call("GET", "/kb/pages")["pages"]] == [grand]
    step("KB: edit conflicts, move with cycle check, delete subtree (author or admin)")

    # ------------------------------------------------------------ team
    gid = owner.call("POST", "/groups", {"name": "Отдел обследований"})["id"]
    owner.call("PUT", f"/groups/{gid}/members", {"user_ids": [ids["a"], ids["b"]]})
    a.call("PUT", "/auth/profile", {"display_name": "Анна Петрова", "position": "Инженер", "phone": "+7 (900) 111-22-33"})
    a.call("PUT", "/auth/profile", {"display_name": "Анна", "phone": "звоните"}, expect=400)
    a.call("PUT", "/auth/profile", {"display_name": " "}, expect=400)
    me = a.call("GET", "/auth/me")
    assert me["position"] == "Инженер" and me["display_name"] == "Анна Петрова"
    team = b.call("GET", "/team")
    anna = next(m for m in team["members"] if m["id"] == ids["a"])
    assert anna["position"] == "Инженер" and anna["phone"] == "+7 (900) 111-22-33" and gid in anna["group_ids"]
    assert next(gr for gr in team["groups"] if gr["id"] == gid)["members"] == 2
    g.call("GET", "/team", expect=403)
    step("Team: directory with groups and contacts, self-service profile")

    # ------------------------------------------------------------ chats
    pid = owner.call("POST", "/projects", {"name": "Корпус 12"})["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["a"], "role": "member"})
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["b"], "role": "member"})
    chans = a.call("GET", "/chats")["channels"]
    proj = next(ch for ch in chans if ch["kind"] == "project")
    assert proj["name"] == "Корпус 12" and proj["project_id"] == pid
    assert not [ch for ch in c.call("GET", "/chats")["channels"] if ch["kind"] == "project"]
    c.call("GET", f"/chats/{proj['id']}/messages", expect=404)
    g.call("GET", "/chats", expect=403)
    assert g.call("GET", "/chats/unread")["total"] == 0

    sb = Stream(b)
    m1 = a.call("POST", f"/chats/{proj['id']}/messages", {"body": "  Всем привет!  "})["message"]
    assert m1["body"] == "Всем привет!" and m1["author_name"] == "Анна Петрова"
    ev = wait_for(lambda: [e for e in sb.events if e["type"] == "chat_message"], what="chat event")[0]
    assert ev["channel_id"] == proj["id"] and ev["message"]["id"] == m1["id"]
    a.call("POST", f"/chats/{proj['id']}/messages", {"body": " "}, expect=400)
    a.call("POST", f"/chats/{proj['id']}/messages", {"body": "x" * 10001}, expect=400)
    assert b.call("GET", "/chats/unread")["total"] == 1 and a.call("GET", "/chats/unread")["total"] == 0
    r = b.call("POST", f"/chats/{proj['id']}/read")
    assert r["unread_total"] == 0
    wait_for(lambda: [e for e in sb.events if e["type"] == "chat_read"], what="read event")
    step("Chats: project channel by membership, live delivery, unread counters")

    d1 = a.call("POST", "/chats/direct", {"user_id": ids["c"]})["id"]
    assert a.call("POST", "/chats/direct", {"user_id": ids["c"]})["id"] == d1
    assert c.call("POST", "/chats/direct", {"user_id": ids["a"]})["id"] == d1
    a.call("POST", "/chats/direct", {"user_id": ids["a"]}, expect=400)
    a.call("POST", "/chats/direct", {"user_id": ids["g"]}, expect=400)
    c.call("POST", f"/chats/{d1}/messages", {"body": "Анна, посмотри отчёт"})
    dm = next(ch for ch in a.call("GET", "/chats")["channels"] if ch["id"] == d1)
    assert dm["kind"] == "direct" and dm["name"] == "Вера" and dm["unread"] == 1 and dm["last_body"] == "Анна, посмотри отчёт"
    b.call("GET", f"/chats/{d1}", expect=404)
    step("Chats: direct messages are unique per pair and private")

    grp = a.call("POST", "/chats", {"name": "Обед", "member_ids": [ids["b"]]})["id"]
    info = b.call("GET", f"/chats/{grp}")
    assert {m["id"] for m in info["members"]} == {ids["a"], ids["b"]} and not info["can_manage"]
    assert a.call("GET", f"/chats/{grp}")["can_manage"]
    b.call("PUT", f"/chats/{grp}", {"name": "x", "member_ids": []}, expect=403)
    a.call("PUT", f"/chats/{grp}", {"name": "Обед в 13:00", "member_ids": [ids["c"]]})
    b.call("GET", f"/chats/{grp}", expect=404)
    assert c.call("GET", f"/chats/{grp}")["channel"]["name"] == "Обед в 13:00"
    a.call("POST", "/chats", {"name": "", "member_ids": []}, expect=400)

    msgs = [a.call("POST", f"/chats/{grp}/messages", {"body": f"сообщение {i}"})["message"] for i in range(5)]
    page1 = c.call("GET", f"/chats/{grp}/messages?limit=3")
    assert [m["body"] for m in page1["messages"]] == ["сообщение 2", "сообщение 3", "сообщение 4"] and page1["has_more"]
    before = page1["messages"][0]["created_at"].replace("+", "%2B")
    page2 = c.call("GET", f"/chats/{grp}/messages?limit=3&before={before}")
    assert [m["body"] for m in page2["messages"]] == ["сообщение 0", "сообщение 1"] and not page2["has_more"]
    c.call("PATCH", f"/chat-messages/{msgs[0]['id']}", {"body": "чужое"}, expect=403)
    e = a.call("PATCH", f"/chat-messages/{msgs[0]['id']}", {"body": "исправлено"})["message"]
    assert e["edited_at"] and e["body"] == "исправлено"
    c.call("DELETE", f"/chat-messages/{msgs[1]['id']}", expect=403)
    a.call("DELETE", f"/chat-messages/{msgs[1]['id']}")
    assert len(c.call("GET", f"/chats/{grp}/messages")["messages"]) == 4
    c.call("POST", f"/chats/{grp}/leave")
    a.call("POST", f"/chats/{grp}/leave")
    a.call("GET", f"/chats/{grp}", expect=404)
    sb.close()
    step("Chats: groups with members, paging, edit and delete own messages, leaving")

    # ------------------------------------------------------------ finance filters
    owner.call("GET", "/crm/documents?status=signed&from=2026-01-01&to=2026-12-31&kind=invoice&q=" + quote("ООО"))
    owner.call("GET", "/crm/documents?from=2026-13-01", expect=400, raw=True)
    step("Finance: document registry accepts period, status and text filters")
    print("ALL KB, CHAT AND TEAM TESTS PASSED")


if __name__ == "__main__":
    main()
