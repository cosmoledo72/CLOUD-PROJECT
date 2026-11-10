#!/usr/bin/env python3
"""End-to-end test of the "All tasks" section and the task card:
portal-wide task numbers, cross-project board with filters and visibility,
task labels and checklists.

Usage:
    API=http://localhost:8080 python3 scripts/tasks_test.py
"""

import os
import sys
import uuid
from urllib.parse import quote

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def board(c, **q):
    qs = "&".join(f"{k}={quote(str(v))}" for k, v in q.items())
    return c.call("GET", "/tasks" + (f"?{qs}" if qs else ""))


def titles(c, **q):
    return sorted(t["title"] for t in board(c, **q)["tasks"])


def main():
    slug = "tsk" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    for key, name, role in [("a", "Анна", "member"), ("b", "Борис", "member"), ("c", "Вера", "member")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name,
                                                 "password": f"{key}-password-123", "role": role})["id"]
    a, b, c = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abc")

    # Two projects: Анна manages both, Борис is a member of P1 only, Вера is a viewer of P2.
    p1 = a.call("POST", "/projects", {"name": "Корпус 12"})["id"]
    p2 = a.call("POST", "/projects", {"name": "Мост"})["id"]
    a.call("POST", f"/projects/{p1}/members", {"user_id": ids["b"], "role": "member"})
    a.call("POST", f"/projects/{p2}/members", {"user_id": ids["c"], "role": "viewer"})

    t1 = a.call("POST", f"/projects/{p1}/tasks", {"title": "Обмерные чертежи", "assignee_id": ids["b"], "priority": 4,
                                                  "start_date": "2026-11-02", "due_date": "2026-11-06"})["task"]
    t2 = a.call("POST", f"/projects/{p1}/tasks", {"title": "Фотофиксация", "start_date": "2026-11-20", "due_date": "2026-11-25"})["task"]
    t3 = a.call("POST", f"/projects/{p2}/tasks", {"title": "Обследование опор", "assignee_id": ids["a"], "priority": 3,
                                                  "due_date": "2026-12-10"})["task"]
    t4 = a.call("POST", f"/projects/{p2}/tasks", {"title": "Отчёт"})["task"]
    nums = [t["number"] for t in (t1, t2, t3, t4)]
    assert nums == list(range(nums[0], nums[0] + 4)), nums
    assert t1["project_name"] == "Корпус 12" and t1["checklist_total"] == 0 and t1["label_ids"] == []

    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Чужой", "email": "x@example.com",
                                        "display_name": "Икс", "password": "other-password-1"})
    ot = other.call("POST", f"/projects/{other.call('POST', '/projects', {'name': 'Свой'})['id']}/tasks", {"title": "Чужая"})["task"]
    assert ot["number"] == 1  # numbering is per portal
    other.call("GET", f"/tasks/{t1['id']}", expect=404)
    assert [t["id"] for t in board(other)["tasks"]] == [ot["id"]]
    step("Task numbers: sequential per portal, isolated between portals")

    # ------------------------------------------------------------ board and visibility
    assert titles(a) == ["Обмерные чертежи", "Обследование опор", "Отчёт", "Фотофиксация"]
    assert titles(b) == ["Обмерные чертежи", "Фотофиксация"]
    assert titles(c) == ["Обследование опор", "Отчёт"]
    assert len(board(owner)["tasks"]) == 4  # admins see every project
    bp = {p["id"]: p for p in board(c)["projects"]}
    assert set(bp) == {p2} and bp[p2]["role"] == "viewer"
    step("Board: tasks of every visible project; members, viewers and admins")

    assert titles(a, project=p2) == ["Обследование опор", "Отчёт"]
    assert titles(a, project=f"{p1},{p2}", assignee=ids["b"]) == ["Обмерные чертежи"]
    assert titles(a, assignee="none") == ["Отчёт", "Фотофиксация"]
    assert titles(a, assignee=f"{ids['a']},none", project=p2) == ["Обследование опор", "Отчёт"]
    assert titles(a, priority="3,4") == ["Обмерные чертежи", "Обследование опор"]
    assert titles(a, **{"from": "2026-11-21", "to": "2026-11-30"}) == ["Фотофиксация"]
    assert titles(a, **{"from": "2026-12-01"}) == ["Обследование опор"]
    assert titles(a, q="обм") == ["Обмерные чертежи"]
    assert titles(a, q=f"#{t3['number']}") == ["Обследование опор"]
    assert titles(a, q="100%") == []
    assert titles(b, project=p2) == []  # foreign project id is ignored, not leaked
    a.call("GET", "/tasks?priority=9", expect=400)
    a.call("GET", "/tasks?project=bogus", expect=400)
    a.call("GET", "/tasks?from=2026-12-01&to=2026-11-01", expect=400)
    a.call("PATCH", f"/tasks/{t2['id']}", {"status": "done"})
    assert titles(a, status="done") == ["Фотофиксация"]
    assert titles(a, status="open,in_progress,review") == ["Обмерные чертежи", "Обследование опор", "Отчёт"]
    step("Board: filters by project, assignee (incl. none), priority, dates, text and number, status")

    a.call("PATCH", f"/projects/{p2}", {"status": "closed"})
    assert titles(a) == ["Обмерные чертежи", "Фотофиксация"]
    assert len(board(a, closed="true")["tasks"]) == 4
    a.call("PATCH", f"/projects/{p2}", {"status": "active"})
    a.call("POST", f"/tasks/{t2['id']}/dependencies", {"depends_on_id": t1["id"]})
    deps = board(a)["dependencies"]
    assert [(d["task_id"], d["depends_on_id"]) for d in deps] == [(t2["id"], t1["id"])]
    assert board(a, project=p2)["dependencies"] == []
    step("Board: closed projects hidden by default; dependencies for the Gantt view")

    # ------------------------------------------------------------ task card
    card = b.call("GET", f"/tasks/{t1['id']}")
    assert card["task"]["number"] == t1["number"] and card["can_edit"] and card["can_manage"]
    assert {m["display_name"] for m in card["members"]} == {"Анна", "Борис"}
    assert card["created_by_name"] == "Анна"
    vc = c.call("GET", f"/tasks/{t4['id']}")
    assert not vc["can_edit"] and not vc["can_manage"]
    vt = c.call("GET", f"/tasks/{t3['id']}")
    assert not vt["can_edit"]
    b.call("GET", f"/tasks/{t3['id']}", expect=404)
    step("Task card: number, members, author and rights")

    # ------------------------------------------------------------ labels
    l1 = b.call("POST", "/task-labels", {"name": "Ожидаем данные", "color": "#F59E0B"})
    assert l1["color"] == "#f59e0b"
    l2 = a.call("POST", "/task-labels", {"name": "Срочно", "color": "#dc2626"})["id"]
    b.call("POST", "/task-labels", {"name": "срочно"}, expect=409)
    b.call("POST", "/task-labels", {"name": "X", "color": "red"}, expect=400)
    b.call("POST", "/task-labels", {"name": " "}, expect=400)
    b.call("PATCH", f"/task-labels/{l2}", {"name": "Горит"}, expect=403)
    owner.call("PATCH", f"/task-labels/{l2}", {"name": "Горит", "color": "#b91c1c"})
    other.call("PATCH", f"/task-labels/{l2}", {"name": "Чужое"}, expect=404)
    labels = a.call("GET", "/task-labels")
    assert [x["name"] for x in labels["labels"]] == ["Ожидаем данные", "Горит"] and not labels["can_manage"]
    assert other.call("GET", "/task-labels")["labels"] == []

    upd = b.call("PUT", f"/tasks/{t1['id']}/labels", {"label_ids": [l2, l1["id"], l2]})["task"]
    assert upd["label_ids"] == [l1["id"], l2]
    a.call("PUT", f"/tasks/{t3['id']}/labels", {"label_ids": [l2]})
    c.call("PUT", f"/tasks/{t4['id']}/labels", {"label_ids": [l2]}, expect=403)
    b.call("PUT", f"/tasks/{t1['id']}/labels", {"label_ids": [str(uuid.uuid4())]}, expect=400)
    assert titles(a, label=l2) == ["Обмерные чертежи", "Обследование опор"]
    assert titles(a, label=l1["id"]) == ["Обмерные чертежи"]
    assert next(x for x in a.call("GET", "/task-labels")["labels"] if x["id"] == l2)["task_count"] == 2
    owner.call("DELETE", f"/task-labels/{l2}")
    assert a.call("GET", f"/tasks/{t1['id']}")["task"]["label_ids"] == [l1["id"]]
    step("Labels: created by staff, managed by admins, unique names, filter, cascade on delete")

    # ------------------------------------------------------------ checklist
    r = b.call("POST", f"/tasks/{t1['id']}/checklist", {"body": "Подвал", "assignee_id": ids["a"], "due_date": "2026-11-03"})
    i1 = r["id"]
    i2 = b.call("POST", f"/tasks/{t1['id']}/checklist", {"body": "1 этаж"})["id"]
    i3 = a.call("POST", f"/tasks/{t1['id']}/checklist", {"body": "Кровля"})["id"]
    b.call("POST", f"/tasks/{t1['id']}/checklist", {"body": " "}, expect=400)
    b.call("POST", f"/tasks/{t1['id']}/checklist", {"body": "Чердак", "assignee_id": ids["c"]}, expect=400)
    c.call("POST", f"/tasks/{t4['id']}/checklist", {"body": "Нельзя"}, expect=403)
    notes = a.call("GET", "/notifications")["items"]
    assert any(n["kind"] == "checklist_assigned" and "Подвал" in n["title"] for n in notes)

    cl = b.call("PATCH", f"/checklist-items/{i2}", {"done": True})["checklist"]
    assert [x["body"] for x in cl] == ["Подвал", "1 этаж", "Кровля"]
    done = next(x for x in cl if x["id"] == i2)
    assert done["done"] and done["done_by"] == ids["b"] and done["done_at"]
    # Ticking twice keeps the original author and time.
    again = next(x for x in a.call("PATCH", f"/checklist-items/{i2}", {"done": True})["checklist"] if x["id"] == i2)
    assert again["done_by"] == ids["b"] and again["done_at"] == done["done_at"]
    cl = b.call("PATCH", f"/checklist-items/{i3}", {"sort_order": 0.5, "body": "Кровля и чердак", "assignee_id": ids["b"]})["checklist"]
    assert [x["body"] for x in cl] == ["Кровля и чердак", "Подвал", "1 этаж"]
    cl = b.call("PATCH", f"/checklist-items/{i1}", {"due_date": None, "assignee_id": None})["checklist"]
    assert next(x for x in cl if x["id"] == i1)["due_date"] is None
    t = a.call("GET", f"/tasks/{t1['id']}")["task"]
    assert (t["checklist_total"], t["checklist_done"]) == (3, 1)
    bt = next(x for x in board(a)["tasks"] if x["id"] == t1["id"])
    assert (bt["checklist_total"], bt["checklist_done"]) == (3, 1)

    # An item assignee without project edit rights may only tick their own item.
    i4 = a.call("POST", f"/tasks/{t4['id']}/checklist", {"body": "Согласовать с заказчиком"})["id"]
    a.call("POST", f"/projects/{p2}/members", {"user_id": ids["c"], "role": "viewer"})
    a.call("PATCH", f"/checklist-items/{i4}", {"assignee_id": ids["c"]})
    assert any(n["kind"] == "checklist_assigned" for n in c.call("GET", "/notifications")["items"])
    c.call("PATCH", f"/checklist-items/{i4}", {"done": True})
    c.call("PATCH", f"/checklist-items/{i4}", {"body": "Другое"}, expect=403)
    c.call("DELETE", f"/checklist-items/{i4}", expect=403)
    b.call("PATCH", f"/checklist-items/{i4}", {"done": False}, expect=404)  # not in Борис's projects
    other.call("PATCH", f"/checklist-items/{i1}", {"done": True}, expect=404)
    cl = b.call("PATCH", f"/checklist-items/{i2}", {"done": False})["checklist"]
    undone = next(x for x in cl if x["id"] == i2)
    assert not undone["done"] and undone["done_by"] is None and undone["done_at"] is None
    assert len(b.call("DELETE", f"/checklist-items/{i2}")["checklist"]) == 2
    step("Checklist: items with assignee and date, ordering, ticking, rights, notifications")

    # Comment counter on the card.
    b.call("POST", f"/tasks/{t1['id']}/comments", {"body": "Готово наполовину"})
    assert a.call("GET", f"/tasks/{t1['id']}")["task"]["comment_count"] == 1
    a.call("DELETE", f"/tasks/{t1['id']}")
    assert titles(a, project=p1) == ["Фотофиксация"]
    step("Comment counter; deleting a task removes its checklist and labels")

    print("ALL TASK BOARD TESTS PASSED")


if __name__ == "__main__":
    main()
