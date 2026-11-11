#!/usr/bin/env python3
"""End-to-end test of task attachments (C3) and project labels and badges (F6).

Usage:
    API=http://localhost:8080 python3 scripts/attachments_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def attach(c, task_id, files, expect=200):
    """Upload [(name, bytes)] to a task as multipart."""
    boundary = uuid.uuid4().hex
    body = b""
    for name, content in files:
        body += (
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
            f"Content-Type: application/octet-stream\r\n\r\n"
        ).encode() + content + b"\r\n"
    body += f"--{boundary}--\r\n".encode()
    return c.call("POST", f"/tasks/{task_id}/attachments", body, expect=expect,
                  headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})


def main():
    slug = "att" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})

    def user(email, name):
        uid = owner.call("POST", "/users", {"email": email, "display_name": name, "password": "user-password-1", "role": "member"})["id"]
        c = Client()
        c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": "user-password-1"})
        return uid, c

    gip_id, gip = user("g@example.com", "Глеб")
    mem_id, mem = user("m@example.com", "Марк")
    view_id, viewer = user("v@example.com", "Вера")
    _, outsider = user("x@example.com", "Ксения")

    pid = owner.call("POST", "/projects", {"name": "Обследование склада"})["id"]
    for uid, role in [(gip_id, "manager"), (mem_id, "member"), (view_id, "viewer")]:
        owner.call("POST", f"/projects/{pid}/members", {"user_id": uid, "role": role})
    root = owner.call("GET", f"/projects/{pid}")["project"]["folder_id"]
    task = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Обмеры: корпус 1/2", "assignee_id": mem_id})["task"]
    tid, num = task["id"], task["number"]
    assert task["attachment_count"] == 0

    # --- Upload from the task card -------------------------------------------
    r = attach(mem, tid, [("акт.pdf", b"%PDF-1"), ("фото.jpg", b"JPEG")])
    assert r["added"] == 2 and [a["name"] for a in r["attachments"]] == ["акт.pdf", "фото.jpg"]
    a0 = r["attachments"][0]
    assert a0["added_by_name"] == "Марк" and a0["size_bytes"] == 6 and a0["extension"] == "pdf"
    # The files lie in «Задачи/#N Обмеры корпус 1 2» of the project folder.
    top = owner.call("GET", f"/folders/{root}")
    tasks_dir = next(f for f in top["folders"] if f["name"] == "Задачи")
    sub = owner.call("GET", f"/folders/{tasks_dir['id']}")["folders"]
    assert [f["name"] for f in sub] == [f"#{num} Обмеры корпус 1 2"], sub
    assert a0["folder_id"] == sub[0]["id"]
    # The same name again gets a suffix instead of failing.
    r = attach(mem, tid, [("акт.pdf", b"second")])
    assert [a["name"] for a in r["attachments"]] == ["акт.pdf", "фото.jpg", "акт (2).pdf"]
    assert mem.call("GET", f"/files/{a0['file_id']}/download", raw=True) == b"%PDF-1"
    attach(viewer, tid, [("x.txt", b"x")], expect=403)
    attach(outsider, tid, [("x.txt", b"x")], expect=404)
    attach(mem, tid, [], expect=400)
    card = viewer.call("GET", f"/tasks/{tid}")
    assert len(card["attachments"]) == 3 and card["task"]["attachment_count"] == 3 and card["can_detach_any"] is False
    assert viewer.call("GET", f"/tasks/{tid}/attachments")["can_edit"] is False
    rows = owner.call("GET", f"/projects/{pid}/tasks")["tasks"]
    assert rows[0]["attachment_count"] == 3
    step("Uploads go to «Задачи/#N Название», same names get a suffix, counters in task rows; viewers and outsiders cannot attach")

    # --- Attach files that already lie in the project folder ------------------
    docs = owner.call("POST", "/folders", {"parent_id": root, "name": "Исходные данные"})["id"]
    plan = owner.upload(docs, "план.dwg", b"DWG")["ids"][0]
    r = mem.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": [plan, plan]})
    assert r["added"] == 1 and len(r["attachments"]) == 4
    assert mem.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": [plan]})["added"] == 0, "attaching twice is a no-op"
    my_root = owner.call("GET", "/folders/roots")["my"]
    private = owner.upload(my_root, "личное.txt", b"secret")["ids"][0]
    owner.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": [private]}, expect=400)
    mem.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": [private]}, expect=404)
    mem.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": []}, expect=400)
    viewer.call("POST", f"/tasks/{tid}/attachments/link", {"file_ids": [plan]}, expect=403)
    step("Existing files of the project folder can be attached; files outside the project folder cannot")

    # --- Detach, trash and restore -------------------------------------------
    photo = r["attachments"][1]["file_id"]
    gip_file = attach(gip, tid, [("смета.xlsx", b"XLSX")])["attachments"][-1]["file_id"]
    mem.call("DELETE", f"/tasks/{tid}/attachments/{gip_file}", expect=403)
    viewer.call("DELETE", f"/tasks/{tid}/attachments/{photo}", expect=403)
    mem.call("DELETE", f"/tasks/{tid}/attachments/{photo}")
    gip.call("DELETE", f"/tasks/{tid}/attachments/{plan}")
    gip.call("DELETE", f"/tasks/{tid}/attachments/{plan}", expect=404)
    assert owner.call("GET", f"/files/{photo}")["file"]["name"] == "фото.jpg", "detaching keeps the file"
    names = [a["name"] for a in owner.call("GET", f"/tasks/{tid}/attachments")["attachments"]]
    assert names == ["акт.pdf", "акт (2).pdf", "смета.xlsx"], names
    # A file in the trash disappears from the task and comes back on restore.
    owner.call("DELETE", f"/files/{a0['file_id']}")
    assert owner.call("GET", f"/tasks/{tid}")["task"]["attachment_count"] == 2
    owner.call("POST", f"/trash/file/{a0['file_id']}/restore")
    assert owner.call("GET", f"/tasks/{tid}")["task"]["attachment_count"] == 3
    # The whole task folder in the trash: attachments hidden, the next upload makes a new folder.
    owner.call("DELETE", f"/folders/{sub[0]['id']}")
    assert owner.call("GET", f"/tasks/{tid}")["attachments"] == []
    r = attach(mem, tid, [("новый.txt", b"n")])
    assert [a["name"] for a in r["attachments"]] == ["новый.txt"] and r["attachments"][0]["folder_id"] != sub[0]["id"]
    assert len(owner.call("GET", "/admin/audit?action=task.attachment_added")["entries"]) == 5
    step("Detach by the one who attached or a project manager keeps the file; trashed files hide and return on restore")

    # --- Project labels ------------------------------------------------------
    r = owner.call("GET", "/project-labels")
    assert r["labels"] == [] and r["can_create"] and r["can_manage"]
    assert gip.call("GET", "/project-labels")["can_create"] is True
    assert mem.call("GET", "/project-labels")["can_create"] is False
    mem.call("POST", "/project-labels", {"name": "Чужая"}, expect=403)
    pay = gip.call("POST", "/project-labels", {"name": "Ожидаем оплату", "color": "#D97706"})
    assert pay["color"] == "#d97706"
    exp = gip.call("POST", "/project-labels", {"name": "На экспертизе", "color": "#2563eb"})["id"]
    gip.call("POST", "/project-labels", {"name": "ожидаем оплату"}, expect=409)
    gip.call("POST", "/project-labels", {"name": "x", "color": "red"}, expect=400)
    mem.call("PUT", f"/projects/{pid}/labels", {"label_ids": [exp]}, expect=403)
    gip.call("PUT", f"/projects/{pid}/labels", {"label_ids": [str(uuid.uuid4())]}, expect=400)
    assert gip.call("PUT", f"/projects/{pid}/labels", {"label_ids": [exp, pay["id"], exp]})["label_ids"] == sorted([exp, pay["id"]])
    p = mem.call("GET", f"/projects/{pid}")["project"]
    assert p["label_ids"] == [pay["id"], exp], "labels come in catalogue order"
    assert [x["label_ids"] for x in mem.call("GET", "/projects")["projects"]] == [[pay["id"], exp]]
    assert {l["name"]: l["project_count"] for l in mem.call("GET", "/project-labels")["labels"]} == {"Ожидаем оплату": 1, "На экспертизе": 1}
    # The catalogue is separate from task labels.
    assert owner.call("GET", "/task-labels")["labels"] == []
    gip.call("PATCH", f"/project-labels/{exp}", {"name": "Экспертиза"}, expect=403)
    owner.call("PATCH", f"/project-labels/{exp}", {"name": "Экспертиза", "color": "#7c3aed"})
    owner.call("PATCH", f"/project-labels/{exp}", {"name": "Ожидаем оплату"}, expect=409)
    gip.call("DELETE", f"/project-labels/{pay['id']}", expect=403)
    owner.call("DELETE", f"/project-labels/{pay['id']}")
    assert mem.call("GET", f"/projects/{pid}")["project"]["label_ids"] == [exp]
    step("Project labels: managers create and assign, admins rename and delete, separate from task labels")

    # --- Project badge -------------------------------------------------------
    assert p["badge_color"] is None and p["badge_icon"] is None
    mem.call("PATCH", f"/projects/{pid}", {"badge_color": "#0f766e"}, expect=403)
    gip.call("PATCH", f"/projects/{pid}", {"badge_color": "teal"}, expect=400)
    gip.call("PATCH", f"/projects/{pid}", {"badge_icon": "Warehouse!"}, expect=400)
    gip.call("PATCH", f"/projects/{pid}", {"badge_color": "#0F766E", "badge_icon": "warehouse"})
    p = mem.call("GET", f"/projects/{pid}")["project"]
    assert (p["badge_color"], p["badge_icon"]) == ("#0f766e", "warehouse")
    gip.call("PATCH", f"/projects/{pid}", {"description": "Склад №3"})
    p = mem.call("GET", f"/projects/{pid}")["project"]
    assert (p["badge_color"], p["badge_icon"]) == ("#0f766e", "warehouse"), "other edits keep the badge"
    board = mem.call("GET", "/tasks")
    assert board["projects"][0]["badge_icon"] == "warehouse" and board["tasks"][0]["attachment_count"] == 1
    ch = next(c for c in mem.call("GET", "/chats")["channels"] if c["kind"] == "project")
    assert ch["badge_color"] == "#0f766e"
    gip.call("PATCH", f"/projects/{pid}", {"badge_icon": None})
    p = mem.call("GET", f"/projects/{pid}")["project"]
    assert (p["badge_color"], p["badge_icon"]) == ("#0f766e", None), "null returns the icon to the letter"
    step("Project badge: colour and icon set by managers, shown in the board and chats, reset with null")

    print("ALL ATTACHMENT AND PROJECT LABEL TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
