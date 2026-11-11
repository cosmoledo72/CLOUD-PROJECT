#!/usr/bin/env python3
"""End-to-end test of editing a project: renaming renames the project's
document folder, the name is validated, only managers may edit.

Usage:
    API=http://localhost:8080 python3 scripts/project_edit_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "pe" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    uid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк",
                                        "password": "m-password-123", "role": "member"})["id"]
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})

    pid = owner.call("POST", "/projects", {"name": "Корпус 12", "description": "Старое"})["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": uid, "role": "member"})
    folder = owner.call("GET", f"/projects/{pid}")["project"]["folder_id"]

    owner.call("PATCH", f"/projects/{pid}", {"name": "  Корпус 12А  ", "description": "  Новое описание "})
    p = owner.call("GET", f"/projects/{pid}")["project"]
    assert (p["name"], p["description"]) == ("Корпус 12А", "Новое описание"), p
    assert owner.call("GET", f"/folders/{folder}")["folder"]["name"] == "Корпус 12А"
    assert any(x["name"] == "Корпус 12А" for x in owner.call("GET", "/projects")["projects"])
    assert owner.call("POST", f"/projects/{pid}/tasks", {"title": "Т"})["task"]["project_name"] == "Корпус 12А"
    rows = owner.call("GET", "/admin/audit?action=project.renamed")["entries"]
    assert any(r["action"] == "project.renamed" for r in rows), rows
    step("Rename: project, its document folder and lists show the new name; audited")

    # Description alone does not touch the name; the same name is not a rename.
    owner.call("PATCH", f"/projects/{pid}", {"description": ""})
    owner.call("PATCH", f"/projects/{pid}", {"name": "Корпус 12А"})
    p = owner.call("GET", f"/projects/{pid}")["project"]
    assert (p["name"], p["description"]) == ("Корпус 12А", "")
    step("Description cleared; unchanged name is kept")

    for bad in ["", "   ", "Корпус 1/2", "a\\b", "x" * 256]:
        r = owner.call("PATCH", f"/projects/{pid}", {"name": bad}, expect=400)
        assert "Название проекта" in r["message"], r
    owner.call("POST", "/projects", {"name": "Дом 3/5"}, expect=400)
    owner.call("PATCH", f"/projects/{pid}", {"status": "archived"}, expect=400)
    step("Invalid names and statuses are rejected with a clear message")

    m.call("PATCH", f"/projects/{pid}", {"name": "Чужое"}, expect=403)
    assert owner.call("GET", f"/projects/{pid}")["project"]["name"] == "Корпус 12А"
    step("Only project managers may rename")

    print("ALL PROJECT EDIT TESTS PASSED")


if __name__ == "__main__":
    main()
