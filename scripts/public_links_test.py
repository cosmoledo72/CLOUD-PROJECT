#!/usr/bin/env python3
"""End-to-end test of public links (F10 + C4): links to a folder, a file and
a knowledge base page opened without an account; browsing subfolders,
downloads and ZIP inside the shared scope only; read-only viewer config;
password (unlock key, wrong password, password change); expiry; revoke and
a new token; who may manage links; the first-open notification; the
administrators' list and the portal-wide switch; audit.

Usage:
    API=http://localhost:8080 DOCS_JWT_SECRET=… python3 scripts/public_links_test.py
"""

import datetime
import io
import os
import sys
import urllib.parse
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "pl" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    owner.call("PATCH", "/admin/portal", {"name": "ПАРТНЕР-ГАРАНТ", "timezone": "Asia/Sakhalin"})
    ids, c = {}, {}
    for key, name in (("eng", "Егор Лаптев"), ("out", "Олег Чужой")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": "user-password-123", "role": "member"})["id"]
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    eng, out = c["eng"], c["out"]
    anon = Client()

    my = eng.call("GET", "/folders/roots")["my"]
    shared = eng.call("POST", "/folders", {"parent_id": my, "name": "Для заказчика"})["id"]
    photos = eng.call("POST", "/folders", {"parent_id": shared, "name": "Фото"})["id"]
    private = eng.call("POST", "/folders", {"parent_id": my, "name": "Черновики"})["id"]
    rep = eng.upload(shared, "Заключение.docx", b"PK fake docx")["ids"][0]
    img = eng.upload(photos, "IMG_01.jpg", b"\xff\xd8\xff photo")["ids"][0]
    draft = eng.upload(private, "Черновик.txt", "не для заказчика".encode())["ids"][0]

    # --- Who may manage ---------------------------------------------------------------
    out.call("GET", f"/links?resource_type=folder&resource_id={shared}", expect=404)
    r = eng.call("GET", f"/links?resource_type=folder&resource_id={shared}")
    assert r["link"] is None and r["can_manage"] and r["enabled"] and r["default_days"] == 30, r
    eng.call("POST", "/shares", {"resource_type": "folder", "resource_id": shared, "user_id": ids["out"], "access": 1})
    assert out.call("GET", f"/links?resource_type=folder&resource_id={shared}")["can_manage"] is False
    out.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared}, expect=403)
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "expires_on": "2020-01-01"}, expect=400)
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "password": "12"}, expect=400)
    step("Links are managed with full access only; readers see that a link exists; bad dates and short passwords refused")

    # --- Folder link ----------------------------------------------------------------------
    link = eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared})["link"]
    token = link["token"]
    today = datetime.date.fromisoformat(link["created_at"][:10])
    exp = datetime.date.fromisoformat(link["expires_on"])
    assert 29 <= (exp - today).days <= 31 and not link["has_password"] and len(token) == 22, link
    assert out.call("GET", f"/links?resource_type=folder&resource_id={shared}")["exists"] is True
    p = anon.call("GET", f"/public/s/{token}")
    assert p["type"] == "folder" and p["name"] == "Для заказчика" and not p["locked"] and p["shared_by"] == "Егор Лаптев", p
    assert p["brand"]["name"] == "ПАРТНЕР-ГАРАНТ" and p["folder"]["total_files"] == 2
    assert [f["name"] for f in p["folder"]["files"]] == ["Заключение.docx"] and [f["name"] for f in p["folder"]["folders"]] == ["Фото"]
    sub = anon.call("GET", f"/public/s/{token}?folder={photos}")
    assert [f["name"] for f in sub["folder"]["files"]] == ["IMG_01.jpg"] and [x["name"] for x in sub["folder"]["crumbs"]] == ["Для заказчика", "Фото"]
    anon.call("GET", f"/public/s/{token}?folder={private}", expect=404)
    assert anon.call("GET", f"/public/s/{token}/download/{img}", raw=True) == b"\xff\xd8\xff photo"
    anon.call("GET", f"/public/s/{token}/download/{draft}", expect=404)
    z = zipfile.ZipFile(io.BytesIO(anon.call("GET", f"/public/s/{token}/zip", raw=True)))
    assert sorted(z.namelist()) == ["Для заказчика/", "Для заказчика/Заключение.docx", "Для заказчика/Фото/", "Для заказчика/Фото/IMG_01.jpg"], z.namelist()
    z = zipfile.ZipFile(io.BytesIO(anon.call("GET", f"/public/s/{token}/zip?folder={photos}", raw=True)))
    assert z.namelist() == ["Фото/", "Фото/IMG_01.jpg"], z.namelist()
    v = anon.call("GET", f"/public/s/{token}/view/{rep}")
    assert v["config"]["editorConfig"]["mode"] == "view" and v["config"]["document"]["permissions"]["edit"] is False and v["config"]["token"]
    anon.call("GET", f"/public/s/{token}/view/{draft}", expect=404)
    anon.call("GET", "/public/s/nonexistent-token-xyz", expect=404)
    step("Folder link: browse subfolders, download and ZIP inside the shared folder only, read-only viewer, brand and sender")

    # --- Counters and the first-open notification ---------------------------------------------
    anon.call("GET", f"/public/s/{token}")
    l2 = eng.call("GET", f"/links?resource_type=folder&resource_id={shared}")["link"]
    assert l2["opens"] == 2 and l2["downloads"] == 3 and l2["last_opened_at"], l2
    notes = [n for n in eng.call("GET", "/notifications")["items"] if n["kind"] == "public_link_opened"]
    assert len(notes) == 1 and "Для заказчика" in notes[0]["title"], notes
    step("Opens and downloads counted; the person who shared it is notified once, on the first open")

    # --- Password -------------------------------------------------------------------------------
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "password": "секрет-1"})
    p = anon.call("GET", f"/public/s/{token}")
    assert p["locked"] and "folder" not in p and "name" not in p, p
    anon.call("GET", f"/public/s/{token}/download/{rep}", expect=401)
    anon.call("GET", f"/public/s/{token}/zip", expect=401)
    anon.call("POST", f"/public/s/{token}/unlock", {"password": "неверно"}, expect=401)
    k = anon.call("POST", f"/public/s/{token}/unlock", {"password": "секрет-1"})["k"]
    kq = urllib.parse.quote(k)
    assert anon.call("GET", f"/public/s/{token}?k={kq}")["folder"]["files"]
    assert anon.call("GET", f"/public/s/{token}/download/{rep}?k={kq}", raw=True) == b"PK fake docx"
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "password": "секрет-2"})
    anon.call("GET", f"/public/s/{token}/download/{rep}?k={kq}", expect=401)
    assert anon.call("GET", f"/public/s/{token}?k={kq}")["locked"]
    l3 = eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "remove_password": True})["link"]
    assert not l3["has_password"] and l3["token"] == token and l3["expires_on"] == link["expires_on"], l3
    assert not anon.call("GET", f"/public/s/{token}")["locked"]
    step("Password: locked until unlocked, the key works for downloads, a new password cancels old keys; password removed")

    # --- Expiry, revoke, new token ------------------------------------------------------------------
    l4 = eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "no_expiry": True})["link"]
    assert l4["expires_on"] is None and l4["expires_at"] is None
    l5 = eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "expires_on": today.isoformat()})["link"]
    assert l5["expires_on"] == today.isoformat(), l5  # works till the end of today
    assert anon.call("GET", f"/public/s/{token}")["expires_on"] == today.isoformat()
    out.call("DELETE", f"/links/{l5['id']}", expect=403)
    eng.call("DELETE", f"/links/{l5['id']}")
    anon.call("GET", f"/public/s/{token}", expect=404)
    anon.call("GET", f"/public/s/{token}/download/{rep}", expect=404)
    again = eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared})["link"]
    assert again["token"] != token and again["opens"] == 0
    step("Expiry «till the end of the day» or none; revoked link stops at once; a new link gets a new address")

    # --- File and knowledge base page -----------------------------------------------------------------
    fl = eng.call("PUT", "/links", {"resource_type": "file", "resource_id": rep})["link"]
    p = anon.call("GET", f"/public/s/{fl['token']}")
    assert p["type"] == "file" and p["file"]["name"] == "Заключение.docx" and p["name"] == "Заключение.docx"
    anon.call("GET", f"/public/s/{fl['token']}/download/{img}", expect=404)
    anon.call("GET", f"/public/s/{fl['token']}/zip", expect=404)
    page = owner.call("POST", "/kb/pages", {"title": "Порядок приёмки", "icon": "📋", "body": "# Приёмка\n\nПередаём **отчёт** и акт."})["id"]
    kl = eng.call("PUT", "/links", {"resource_type": "kb_page", "resource_id": page})["link"]
    p = anon.call("GET", f"/public/s/{kl['token']}")
    assert p["type"] == "kb_page" and p["page"]["title"] == "Порядок приёмки" and "**отчёт**" in p["page"]["body"], p
    # A deleted file takes its link with it.
    eng.call("DELETE", f"/files/{rep}")
    anon.call("GET", f"/public/s/{fl['token']}", expect=404)
    step("File link (only that file) and knowledge base page link; a deleted file's link stops working")

    # --- Administration and audit -----------------------------------------------------------------------
    eng.call("GET", "/admin/links", expect=403)
    al = owner.call("GET", "/admin/links")
    names = {x["name"] for x in al["links"]}
    assert {"Для заказчика", "Порядок приёмки", "Заключение.docx"} <= names and al["enabled"], names
    assert next(x for x in al["links"] if x["name"] == "Заключение.docx")["missing"] is True
    owner.call("PUT", "/admin/links/settings", {"enabled": False})
    anon.call("GET", f"/public/s/{again['token']}", expect=404)
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": photos}, expect=403)
    assert eng.call("GET", f"/links?resource_type=folder&resource_id={shared}")["enabled"] is False
    owner.call("PUT", "/admin/links/settings", {"enabled": True})
    assert anon.call("GET", f"/public/s/{again['token']}")["name"] == "Для заказчика"
    owner.call("DELETE", f"/links/{kl['id']}")  # administrators revoke any link
    anon.call("GET", f"/public/s/{kl['token']}", expect=404)
    actions = {e["action"] for e in owner.call("GET", "/admin/audit?limit=200")["entries"]}
    assert {"link.created", "link.updated", "link.revoked", "link.opened", "links.settings"} <= actions, actions
    step("Administrators: all links of the portal, revoke any, switch links off and on; everything in the audit log")

    # --- Brute force ------------------------------------------------------------------------------------
    eng.call("PUT", "/links", {"resource_type": "folder", "resource_id": shared, "password": "длинный-пароль"})
    for i in range(10):
        anon.call("POST", f"/public/s/{again['token']}/unlock", {"password": f"x{i}"}, expect=401)
    anon.call("POST", f"/public/s/{again['token']}/unlock", {"password": "длинный-пароль"}, expect=429)
    step("Password guessing is limited: 10 wrong tries in 15 minutes lock unlocking")

    print("ALL PUBLIC LINK TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
