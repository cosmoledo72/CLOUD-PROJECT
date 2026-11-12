#!/usr/bin/env python3
"""End-to-end test of the visual knowledge base editor (F9): HTML pages are
cleaned on the server (scripts, foreign images, unsafe styles), plain text is
searchable; Markdown is still accepted; page history and restoring a
version; edit conflicts and «save anyway»; images and files inside pages
(inline images, downloads, rights, quota, public link); pinned pages;
archive with subpages, restore and permanent deletion; who is on the page.

Usage:
    API=http://localhost:8080 python3 scripts/kb_editor_test.py
"""

import os
import struct
import sys
import urllib.parse
import uuid
import zlib

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import API, Client, step  # noqa: E402


def png():
    raw = b"".join(b"\x00" + b"\x46\x6e\xa0" * 4 for _ in range(4))

    def ch(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    return b"\x89PNG\r\n\x1a\n" + ch(b"IHDR", struct.pack(">IIBBBBB", 4, 4, 8, 2, 0, 0, 0)) + ch(b"IDAT", zlib.compress(raw)) + ch(b"IEND", b"")


def upload(c, page, name, data, expect=200):
    boundary = uuid.uuid4().hex
    body = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
            f"Content-Type: application/octet-stream\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
    return c.call("POST", f"/kb/pages/{page}/files", body, expect=expect, headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})


def head(c, path):
    """Status and headers of a GET (body ignored)."""
    import urllib.error
    import urllib.request
    req = urllib.request.Request(API + path)
    try:
        with c.opener.open(req) as r:
            return r.status, {k.lower(): v for k, v in r.headers.items()}, r.read()
    except urllib.error.HTTPError as e:
        return e.code, dict(e.headers), b""


def main():
    slug = "kb" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    ids, c = {}, {}
    for key, name, role in (("eng", "Егор Лаптев", "member"), ("gst", "Гость", "guest")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": "user-password-123", "role": role})["id"]
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    eng, gst = c["eng"], c["gst"]

    # --- HTML is cleaned, text is searchable -----------------------------------------------
    html = ('<h2>Приёмка</h2><p>Срок <span style="background-color: #fdecc8; position: fixed">пять дней</span> '
            '<strong>обязателен</strong><script>alert(1)</script><img src="https://evil.example/x.png" onerror="x()"></p>'
            '<ul data-type="taskList"><li data-type="taskItem" data-checked="true"><label><input type="checkbox" checked></label>'
            '<div><p>Проверить штампы</p></div></li></ul><a href="javascript:alert(1)">плохая</a> <a href="/kb?p=1">страница</a>')
    pid = eng.call("POST", "/kb/pages", {"title": "Порядок приёмки", "content": html})["id"]
    pg = eng.call("GET", f"/kb/pages/{pid}")["page"]
    h = pg["content"]
    assert "<h2>Приёмка</h2>" in h and 'style="background-color: #fdecc8"' in h and "position" not in h, h
    assert "script" not in h and "evil" not in h and "onerror" not in h and "javascript" not in h, h
    assert 'data-type="taskItem"' in h and 'href="/kb?p=1"' in h and pg["version"] == 1
    hits = eng.call("GET", "/kb/search?q=" + urllib.parse.quote("штампы"))["results"]
    assert hits and hits[0]["id"] == pid and "<" not in hits[0]["snippet"], hits
    assert any(g["type"] == "kb" for g in eng.call("GET", "/search?q=" + urllib.parse.quote("обязателен"))["groups"])
    md = eng.call("POST", "/kb/pages", {"title": "Из Markdown", "body": "# Заголовок\n\n- [x] готово\n\n**жирный**"})["id"]
    mh = eng.call("GET", f"/kb/pages/{md}")["page"]["content"]
    assert "<h1>Заголовок</h1>" in mh and 'data-type="taskList"' in mh and "<strong>жирный</strong>" in mh, mh
    gst.call("GET", f"/kb/pages/{pid}", expect=403)
    step("Editor HTML is cleaned (scripts, foreign images, unsafe styles and links); text is searchable; Markdown still converts")

    # --- History and conflicts -----------------------------------------------------------------
    eng.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки", "content": "<p>Версия два</p>", "base_version": 1})
    same = eng.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки", "content": "<p>Версия два</p>", "base_version": 2})["page"]
    assert same["version"] == 2, "an unchanged save does not create a version"
    owner.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки отчёта", "content": "<p>Версия три</p>", "base_version": 2})
    eng.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки", "content": "<p>Моя правка</p>", "base_version": 2}, expect=409)
    forced = eng.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки", "content": "<p>Моя правка</p>", "base_version": 2, "force": True})["page"]
    assert forced["version"] == 4 and forced["updated_by_name"] == "Егор Лаптев"
    vs = eng.call("GET", f"/kb/pages/{pid}/versions")["versions"]
    assert [v["version"] for v in vs] == [4, 3, 2, 1] and vs[1]["author_name"] == "Ольга Смирнова", vs
    v1 = eng.call("GET", f"/kb/pages/{pid}/versions/1")
    assert "Приёмка" in v1["content"] and v1["title"] == "Порядок приёмки"
    r = eng.call("POST", f"/kb/pages/{pid}/versions/3/restore")["page"]
    assert r["version"] == 5 and "Версия три" in r["content"] and r["title"] == "Порядок приёмки отчёта"
    step("Every save is a version (who and when); unchanged saves are skipped; conflicts need «save anyway»; old versions come back as new ones")

    # --- Files and images ----------------------------------------------------------------------------
    used0 = eng.call("GET", "/storage/usage")["used"]
    img = upload(eng, pid, "схема.png", png())
    assert img["image"] and img["url"] == f"/api/kb/files/{img['id']}" and img["author_name"] == "Егор Лаптев", img
    doc = upload(eng, pid, "Акт приёмки.pdf", b"%PDF-1.4 test")
    assert not doc["image"] and doc["extension"] == "pdf"
    st, hd, body = head(owner, f"/kb/files/{img['id']}")
    assert st == 200 and hd["content-type"] == "image/png" and "inline" in hd["content-disposition"] and body[:4] == b"\x89PNG", hd
    st, hd, _ = head(owner, f"/kb/files/{doc['id']}")
    assert hd["content-type"] == "application/octet-stream" and "attachment" in hd["content-disposition"]
    st, hd, _ = head(owner, f"/kb/files/{img['id']}?download=1")
    assert "attachment" in hd["content-disposition"]
    assert head(gst, f"/kb/files/{img['id']}")[0] == 403
    stranger = Client()
    stranger.call("POST", "/auth/signup", {"tenant_slug": "kx" + uuid.uuid4().hex[:6], "tenant_name": "Другая", "email": "x@example.com",
                                           "display_name": "Чужой", "password": "owner-password-1"})
    assert head(stranger, f"/kb/files/{img['id']}")[0] == 404
    assert eng.call("GET", "/storage/usage")["used"] == used0 + len(png()) + len(b"%PDF-1.4 test")
    page_html = f'<p>Схема:</p><img src="{img["url"]}"><div data-type="file-card" data-id="{doc["id"]}" data-name="Акт приёмки.pdf"><a href="/api/kb/files/{doc["id"]}?download=1">Акт приёмки.pdf</a></div>'
    cur = eng.call("GET", f"/kb/pages/{pid}")["page"]["version"]
    saved = eng.call("PUT", f"/kb/pages/{pid}", {"title": "Порядок приёмки отчёта", "content": page_html, "base_version": cur})["page"]["content"]
    assert f'src="/api/kb/files/{img["id"]}"' in saved and 'data-type="file-card"' in saved
    # The page shared by a public link shows its own pictures, and only them.
    link = eng.call("PUT", "/links", {"resource_type": "kb_page", "resource_id": pid})["link"]
    anon = Client()
    assert anon.call("GET", f"/public/s/{link['token']}")["page"]["content"] == saved
    assert head(anon, f"/public/s/{link['token']}/kbfile/{img['id']}")[1]["content-type"] == "image/png"
    other = upload(eng, md, "чужая.png", png())
    assert head(anon, f"/public/s/{link['token']}/kbfile/{other['id']}")[0] == 404
    step("Images and file cards: stored with the page, images inline and files as downloads, staff only, counted in the quota, shown by public links")

    # --- Pins, presence ---------------------------------------------------------------------------------
    eng.call("PATCH", f"/kb/pages/{pid}/pin", {"pinned": True})
    assert next(p for p in owner.call("GET", "/kb/pages")["pages"] if p["id"] == pid)["pinned"] is True
    eng.call("POST", f"/kb/pages/{pid}/presence", {"editing": True})
    seen = owner.call("POST", f"/kb/pages/{pid}/presence", {"editing": False})["viewers"]
    assert seen == [{"id": ids["eng"], "name": "Егор Лаптев", "editing": True}], seen
    assert owner.call("GET", f"/kb/pages/{pid}")["viewers"][0]["editing"] is True
    eng.call("POST", f"/kb/pages/{pid}/presence", {"leave": True})
    assert owner.call("POST", f"/kb/pages/{pid}/presence", {})["viewers"] == []
    step("Pinned pages for everyone; who is looking at a page and who is editing it")

    # --- Archive ------------------------------------------------------------------------------------------
    child = eng.call("POST", "/kb/pages", {"parent_id": pid, "title": "Акты", "content": "<p>акты приёмки</p>"})["id"]
    gst.call("DELETE", f"/kb/pages/{pid}", expect=403)
    owner2 = Client()
    owner2.call("POST", "/auth/login", {"tenant": slug, "email": "o@example.com", "password": "owner-password-1"})
    third = Client()
    tid = owner.call("POST", "/users", {"email": "t@example.com", "display_name": "Третий", "password": "user-password-123", "role": "member"})["id"]
    third.call("POST", "/auth/login", {"tenant": slug, "email": "t@example.com", "password": "user-password-123"})
    third.call("DELETE", f"/kb/pages/{pid}", expect=403)
    eng.call("DELETE", f"/kb/pages/{pid}?permanent=true", expect=400)
    assert eng.call("DELETE", f"/kb/pages/{pid}")["archived"] == 2
    tree = [p["id"] for p in owner.call("GET", "/kb/pages")["pages"]]
    assert pid not in tree and child not in tree
    assert owner.call("GET", f"/kb/pages/{pid}")["page"]["archived_at"]
    assert not owner.call("GET", "/kb/search?q=" + urllib.parse.quote("акты"))["results"]
    anon.call("GET", f"/public/s/{link['token']}", expect=404)
    arch = owner.call("GET", "/kb/archive")["pages"]
    assert [a["id"] for a in arch] == [pid] and arch[0]["subpages"] == 1, arch
    eng.call("PUT", f"/kb/pages/{pid}", {"title": "x", "content": "<p>x</p>"}, expect=404)
    assert owner.call("POST", f"/kb/pages/{pid}/restore")["restored"] == 2
    tree = [p["id"] for p in owner.call("GET", "/kb/pages")["pages"]]
    assert pid in tree and child in tree
    assert next(p for p in owner.call("GET", "/kb/pages")["pages"] if p["id"] == pid)["pinned"] is False
    eng.call("DELETE", f"/kb/pages/{md}")
    eng.call("DELETE", f"/kb/pages/{md}?permanent=true")
    owner.call("GET", f"/kb/pages/{md}", expect=404)
    assert head(owner, f"/kb/files/{other['id']}")[0] == 404
    actions = {e["action"] for e in owner.call("GET", "/admin/audit?limit=200")["entries"]}
    assert {"kb.page_archived", "kb.page_restored", "kb.page_deleted", "kb.version_restored"} <= actions, actions
    del owner2, tid
    step("Archive instead of deletion (with subpages, hidden from tree, search and links), restore, permanent deletion with files")

    print("ALL KB EDITOR TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
