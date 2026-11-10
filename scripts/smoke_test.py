#!/usr/bin/env python3
"""End-to-end smoke test for the WorkHub API.

Covers sign-up, TOTP enrolment and two-step login, folders, upload, ACL
(member vs. personal folder), sharing, ONLYOFFICE editor config, Document
Server download and save callback (simulated), versions, projects, tasks,
dependencies with cycle detection, and time tracking.

Usage:
    API=http://localhost:8080 DOCS_JWT_SECRET=... python3 scripts/smoke_test.py

Only the Python standard library is used. The Document Server is simulated:
the test serves the "edited" file from a tiny local HTTP server and signs the
callback with DOCS_JWT_SECRET exactly like ONLYOFFICE does.
"""

import base64
import hashlib
import hmac
import http.cookiejar
import http.server
import json
import os
import struct
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

API = os.environ.get("API", "http://localhost:8080").rstrip("/") + "/api"
DOCS_SECRET = os.environ.get("DOCS_JWT_SECRET", "docs-secret-0123456789")
FAKE_DS_PORT = int(os.environ.get("FAKE_DS_PORT", "9999"))


class Client:
    """Minimal JSON client with its own cookie jar (one per user)."""

    def __init__(self):
        self.jar = http.cookiejar.CookieJar()
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(self.jar))

    def call(self, method, path, body=None, expect=200, raw=False, headers=None, url=None):
        data, hdrs = None, dict(headers or {})
        if isinstance(body, (bytes, bytearray)):
            data = bytes(body)
        elif body is not None:
            data = json.dumps(body).encode()
            hdrs["Content-Type"] = "application/json"
        req = urllib.request.Request(url or API + path, data=data, method=method, headers=hdrs)
        try:
            with self.opener.open(req) as r:
                status, payload = r.status, r.read()
        except urllib.error.HTTPError as e:
            status, payload = e.code, e.read()
        if status != expect:
            raise AssertionError(f"{method} {path}: expected {expect}, got {status}: {payload[:300]!r}")
        if raw:
            return payload
        return json.loads(payload) if payload else None

    def upload(self, folder_id, name, content, overwrite=False, expect=200):
        boundary = uuid.uuid4().hex
        body = (
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
            f"Content-Type: application/octet-stream\r\n\r\n"
        ).encode() + content + f"\r\n--{boundary}--\r\n".encode()
        q = "?overwrite=true" if overwrite else ""
        return self.call("POST", f"/folders/{folder_id}/upload{q}", body, expect=expect,
                         headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})


def b64url(b):
    return base64.urlsafe_b64encode(b).rstrip(b"=").decode()


def jwt_hs256(payload, secret):
    head = b64url(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64url(json.dumps(payload).encode())
    sig = hmac.new(secret.encode(), f"{head}.{body}".encode(), hashlib.sha256).digest()
    return f"{head}.{body}.{b64url(sig)}"


def jwt_payload(token):
    part = token.split(".")[1]
    return json.loads(base64.urlsafe_b64decode(part + "=" * (-len(part) % 4)))


def totp_now(secret_b32):
    key = base64.b32decode(secret_b32 + "=" * (-len(secret_b32) % 8))
    counter = struct.pack(">Q", int(time.time()) // 30)
    h = hmac.new(key, counter, hashlib.sha1).digest()
    o = h[-1] & 0x0F
    return f"{(struct.unpack('>I', h[o:o + 4])[0] & 0x7FFFFFFF) % 1_000_000:06d}"


def fake_document_server(content):
    """Serve `content` at /cache/edited.docx, as the Document Server would."""

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Length", str(len(content)))
            self.end_headers()
            self.wfile.write(content)

        def log_message(self, *a):
            pass

    srv = http.server.HTTPServer(("127.0.0.1", FAKE_DS_PORT), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def step(msg):
    print(f"  ✓ {msg}")


def main():
    slug = "t" + uuid.uuid4().hex[:8]
    owner = Client()
    me = owner.call("POST", "/auth/signup", {
        "tenant_slug": slug, "tenant_name": "Test Org", "email": "owner@example.com",
        "display_name": "Owner", "password": "correct-horse-battery",
    })
    assert me["role"] == "owner"
    step(f"signup → tenant '{slug}'")

    # TOTP enrolment and two-step login.
    setup = owner.call("POST", "/auth/totp/setup")
    owner.call("POST", "/auth/totp/enable", {"code": totp_now(setup["secret"])})
    fresh = Client()
    r = fresh.call("POST", "/auth/login", {"tenant": slug, "email": "owner@example.com", "password": "correct-horse-battery"})
    assert r["mfa_required"] is True
    fresh.call("GET", "/auth/me", expect=401)  # pending session must not grant access
    fresh.call("POST", "/auth/totp/verify", {"code": "000000" if totp_now(setup["secret"]) != "000000" else "111111"}, expect=401)
    fresh.call("POST", "/auth/totp/verify", {"code": totp_now(setup["secret"])})
    assert fresh.call("GET", "/auth/me")["totp_enabled"] is True
    owner.call("POST", "/auth/login", {"tenant": slug, "email": "owner@example.com", "password": "wrong-password!"}, expect=401)
    step("TOTP enrolment, two-step login, wrong code rejected")

    # Second user (member).
    owner.call("POST", "/users", {"email": "member@example.com", "display_name": "Member", "password": "member-password-1"})
    member = Client()
    member.call("POST", "/auth/login", {"tenant": slug, "email": "member@example.com", "password": "member-password-1"})
    member_id = member.call("GET", "/auth/me")["id"]
    step("admin created member, member logged in")

    roots = owner.call("GET", "/folders/roots")
    my, common = roots["my"], roots["common"]
    sub = owner.call("POST", "/folders", {"parent_id": my, "name": "Reports"})["id"]
    up = owner.upload(sub, "report.docx", b"PK-fake-docx-v1")
    fid = up["ids"][0]
    owner.upload(sub, "report.docx", b"dup", expect=409)
    owner.upload(sub, "report.docx", b"PK-fake-docx-v2", overwrite=True)
    assert owner.call("GET", f"/files/{fid}/download", raw=True) == b"PK-fake-docx-v2"
    step("folder, upload, duplicate → 409, overwrite → version 2")

    # ACL: member cannot see owner's personal folder until shared.
    member.call("GET", f"/folders/{sub}", expect=404)
    member.call("GET", f"/files/{fid}/download", expect=404)
    owner.call("POST", "/shares", {"resource_type": "folder", "resource_id": sub, "user_id": member_id, "access": 1})
    assert member.call("GET", f"/folders/{sub}")["access"] == "Read"
    member.upload(sub, "x.txt", b"x", expect=403)
    shared = member.call("GET", "/shared")
    assert any(f["id"] == sub for f in shared["folders"])
    member_common = member.call("GET", f"/folders/{common}")
    assert member_common["access"] == "Edit"
    step("ACL: personal folder hidden, read share works, write blocked, common = edit")

    # ONLYOFFICE: new blank doc, editor config, DS download, save callback.
    doc = owner.call("POST", f"/folders/{common}/new", {"kind": "docx", "name": "Act of inspection"})["id"]
    ed = owner.call("GET", f"/files/{doc}/editor")
    cfg = ed["config"]
    assert cfg["documentType"] == "word" and cfg["editorConfig"]["mode"] == "edit"
    signed = jwt_payload(cfg["token"])
    assert signed["document"]["key"] == cfg["document"]["key"]
    ed2 = member.call("GET", f"/files/{doc}/editor")
    assert ed2["config"]["document"]["key"] == cfg["document"]["key"], "co-editors must share the key"

    dl_url = cfg["document"]["url"]
    blank = owner.call("GET", None, url=dl_url, raw=True)
    assert blank[:2] == b"PK"
    owner.call("GET", None, url=dl_url.replace("?t=", "?t=x"), expect=401)

    edited = b"PK-edited-by-onlyoffice"
    srv = fake_document_server(edited)
    cb_url = cfg["editorConfig"]["callbackUrl"]
    body = {"key": cfg["document"]["key"], "status": 2,
            "url": f"http://localhost:{FAKE_DS_PORT}/cache/edited.docx", "users": [me["id"]]}
    bad = owner.call("POST", None, url=cb_url, body={**body, "token": jwt_hs256(body, "wrong-secret")})
    assert bad["error"] == 1
    ok = owner.call("POST", None, url=cb_url, body={"token": jwt_hs256(body, DOCS_SECRET)})
    assert ok["error"] == 0
    srv.shutdown()
    assert owner.call("GET", f"/files/{doc}/download", raw=True) == edited
    versions = owner.call("GET", f"/files/{doc}/versions")["versions"]
    assert versions[0]["version"] == 2 and versions[0]["author_name"] == "Owner"
    ed3 = owner.call("GET", f"/files/{doc}/editor")
    assert ed3["config"]["document"]["key"] != cfg["document"]["key"], "key must rotate after save"
    step("ONLYOFFICE: blank doc, signed config, shared key, DS download, callback save, key rotation")

    # Projects and Gantt.
    p = owner.call("POST", "/projects", {"name": "Survey of building 12", "description": "Inspection"})
    pid = p["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": member_id, "role": "member"})
    t1 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Field survey", "start_date": "2026-11-10", "due_date": "2026-11-20", "assignee_id": member_id})["task"]
    t2 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Report", "start_date": "2026-11-21", "due_date": "2026-11-30"})["task"]
    ms = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Hand-over", "due_date": "2026-12-01", "is_milestone": True})["task"]
    assert ms["start_date"] == ms["due_date"] == "2026-12-01"
    owner.call("POST", f"/tasks/{t2['id']}/dependencies", {"depends_on_id": t1["id"]})
    owner.call("POST", f"/tasks/{ms['id']}/dependencies", {"depends_on_id": t2["id"]})
    owner.call("POST", f"/tasks/{t1['id']}/dependencies", {"depends_on_id": ms["id"]}, expect=400)
    owner.call("POST", f"/projects/{pid}/tasks", {"title": "Bad", "start_date": "2026-12-05", "due_date": "2026-12-01"}, expect=400)
    upd = member.call("PATCH", f"/tasks/{t1['id']}", {"status": "done", "due_date": "2026-11-19"})["task"]
    assert upd["progress"] == 100 and upd["due_date"] == "2026-11-19"
    upd = owner.call("PATCH", f"/tasks/{t2['id']}", {"start_date": None})["task"]
    assert upd["start_date"] is None
    member.call("POST", f"/tasks/{t1['id']}/time", {"work_date": "2026-11-12", "minutes": 240})
    gantt = member.call("GET", f"/projects/{pid}/tasks")
    assert len(gantt["tasks"]) == 3 and len(gantt["dependencies"]) == 2
    assert next(t for t in gantt["tasks"] if t["id"] == t1["id"])["spent_minutes"] == 240
    # Project folder is reachable to members via project ACL.
    assert member.call("GET", f"/folders/{p['folder_id']}")["access"] == "Edit"
    step("projects: members, tasks, milestone, deps, cycle rejected, PATCH null, time, project folder ACL")

    # Tenant isolation.
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Other", "email": "o@example.com",
                                        "display_name": "Other", "password": "other-password-1"})
    other.call("GET", f"/files/{doc}", expect=404)
    other.call("GET", f"/projects/{pid}", expect=404)
    other.call("GET", f"/folders/{common}", expect=404)
    step("tenant isolation")

    owner.call("POST", "/auth/logout")
    owner.call("GET", "/auth/me", expect=401)
    step("logout")
    print("ALL SMOKE TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
