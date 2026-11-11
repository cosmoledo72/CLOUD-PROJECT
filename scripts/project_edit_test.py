#!/usr/bin/env python3
"""End-to-end test of editing a project: renaming renames the project's
document folder; the «Наименование объекта» requisite and the project name are
kept equal in both directions; only managers may edit.

Usage:
    API=http://localhost:8080 python3 scripts/project_edit_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def signup():
    slug = "pe" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    return slug, owner


def project(c, pid):
    return c.call("GET", f"/projects/{pid}")["project"]


def folder_name(c, p):
    return c.call("GET", f"/folders/{p['folder_id']}")["folder"]["name"]


def main():
    # ------------------------------------------------ portal without requisites
    slug, owner = signup()
    uid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк",
                                        "password": "m-password-123", "role": "member"})["id"]
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})

    pid = owner.call("POST", "/projects", {"name": "Корпус 12", "description": "Старое"})["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": uid, "role": "member"})

    owner.call("PATCH", f"/projects/{pid}", {"name": "  Корпус 12А\n ", "description": "  Новое описание "})
    p = project(owner, pid)
    assert (p["name"], p["description"]) == ("Корпус 12А", "Новое описание"), p
    assert folder_name(owner, p) == "Корпус 12А"
    assert any(x["name"] == "Корпус 12А" for x in owner.call("GET", "/projects")["projects"])
    assert owner.call("POST", f"/projects/{pid}/tasks", {"title": "Т"})["task"]["project_name"] == "Корпус 12А"
    rows = owner.call("GET", "/admin/audit?action=project.renamed")["entries"]
    assert any(r["action"] == "project.renamed" for r in rows), rows
    step("Rename: project, its document folder and lists show the new name; audited")

    owner.call("PATCH", f"/projects/{pid}", {"description": ""})
    p = project(owner, pid)
    assert (p["name"], p["description"]) == ("Корпус 12А", "")
    owner.call("PATCH", f"/projects/{pid}", {"name": "Корпус 1/2 \\ блок А"})
    p = project(owner, pid)
    assert p["name"] == "Корпус 1/2 \\ блок А" and folder_name(owner, p) == "Корпус 1-2 - блок А"
    for bad in ["", "   ", "x" * 1001]:
        r = owner.call("PATCH", f"/projects/{pid}", {"name": bad}, expect=400)
        assert "Название проекта" in r["message"], r
    owner.call("PATCH", f"/projects/{pid}", {"status": "archived"}, expect=400)
    m.call("PATCH", f"/projects/{pid}", {"name": "Чужое"}, expect=403)
    step("Slashes allowed in names (folder gets a safe name); empty/too long rejected; managers only")

    # ------------------------------------------------ «Наименование объекта» is the name
    _, o2 = signup()
    o2.call("POST", "/admin/project-fields/preset")
    base = {"code": "А0688", "client": {"id": None, "name": "Росморпорт", "inn": None}}

    # Created with the name only: the requisite is filled from it.
    pid = o2.call("POST", "/projects", {"name": "Порт Холмск", "fields": base})["id"]
    p = project(o2, pid)
    assert p["name"] == "Порт Холмск" and p["fields"]["object_name"] == "Порт Холмск", p

    # Editing the requisite renames the project and its folder.
    full = "Реконструкция береговых сооружений автомобильно-железнодорожного паромного сообщения\n«Ванино-Холмск» в морском порту Холмск"
    o2.call("PUT", f"/projects/{pid}/fields", {"values": {"object_name": full}})
    p = project(o2, pid)
    flat = " ".join(full.split())
    assert p["name"] == flat, p["name"]
    assert p["fields"]["object_name"] == full  # the requisite keeps its line breaks
    assert folder_name(o2, p) == flat
    assert next(x for x in o2.call("GET", "/projects")["projects"] if x["id"] == pid)["name"] == flat
    step("Editing «Наименование объекта» renames the project, the list and the folder")

    # Renaming the project updates the requisite.
    o2.call("PATCH", f"/projects/{pid}", {"name": "Порт Холмск, причал 3"})
    p = project(o2, pid)
    assert p["fields"]["object_name"] == "Порт Холмск, причал 3" == p["name"]
    # Saving other requisites does not touch the name.
    o2.call("PUT", f"/projects/{pid}/fields", {"values": {"stage": "П"}})
    assert project(o2, pid)["name"] == "Порт Холмск, причал 3"
    step("Renaming the project updates the requisite; other requisites leave the name alone")

    # Created with the requisite filled: it wins over the short name.
    pid2 = o2.call("POST", "/projects", {"name": "черновик", "fields": {**base, "object_name": "Склад ГСМ"}})["id"]
    assert project(o2, pid2)["name"] == "Склад ГСМ"
    step("New project: a filled «Наименование объекта» becomes the name")

    print("ALL PROJECT EDIT TESTS PASSED")


if __name__ == "__main__":
    main()
