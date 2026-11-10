#!/usr/bin/env python3
"""End-to-end test of outgoing mail, invitations and password reset.

Starts a tiny SMTP server in-process and points the portal at it, so the
backend must run with SMTP_ALLOW_PRIVATE=true and PUBLIC_URL set.

Usage:
    API=http://localhost:8080 python3 scripts/mail_test.py
"""

import email
import email.policy
import os
import re
import socketserver
import sys
import threading
import time
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

INBOX: list = []


class SmtpHandler(socketserver.StreamRequestHandler):
    """Minimal SMTP: EHLO, MAIL, RCPT, DATA, RSET, NOOP, QUIT. Optionally rejects."""

    reject = False

    def reply(self, line):
        self.wfile.write((line + "\r\n").encode())

    def handle(self):
        self.reply("220 sink ESMTP")
        rcpt, data = [], None
        while True:
            line = self.rfile.readline()
            if not line:
                return
            cmd = line.decode(errors="replace").strip()
            up = cmd.upper()
            if up.startswith("EHLO") or up.startswith("HELO"):
                self.reply("250-sink")
                self.reply("250 8BITMIME")
            elif up.startswith("MAIL FROM"):
                if SmtpHandler.reject:
                    self.reply("550 5.7.1 sender rejected by test")
                    continue
                rcpt = []
                self.reply("250 ok")
            elif up.startswith("RCPT TO"):
                rcpt.append(cmd.split(":", 1)[1].strip(" <>"))
                self.reply("250 ok")
            elif up == "DATA":
                self.reply("354 go ahead")
                buf = []
                while True:
                    l = self.rfile.readline()
                    if l in (b".\r\n", b".\n", b""):
                        break
                    if l.startswith(b".."):
                        l = l[1:]
                    buf.append(l)
                data = b"".join(buf)
                INBOX.append((rcpt, email.message_from_bytes(data, policy=email.policy.default)))
                self.reply("250 queued")
            elif up in ("RSET", "NOOP"):
                self.reply("250 ok")
            elif up == "QUIT":
                self.reply("221 bye")
                return
            else:
                self.reply("502 not implemented")


def start_sink():
    srv = socketserver.ThreadingTCPServer(("127.0.0.1", 0), SmtpHandler)
    srv.daemon_threads = True
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv.server_address[1]


def wait_mail(to, subject_part, timeout=20):
    deadline = time.time() + timeout
    while time.time() < deadline:
        for rcpt, msg in INBOX:
            if to in rcpt and subject_part in msg["subject"]:
                INBOX.remove((rcpt, msg))
                return msg
        time.sleep(0.2)
    raise AssertionError(f"no mail to {to} with '{subject_part}'; inbox: {[(r, m['subject']) for r, m in INBOX]}")


def text_of(msg):
    return msg.get_body(preferencelist=("plain",)).get_content()


def link(msg, kind):
    m = re.search(rf"https?://\S+/{kind}/([0-9a-f]{{64}})", text_of(msg))
    assert m, text_of(msg)
    return m.group(1)


def main():
    port = start_sink()
    slug = "mail" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро «Почта»", "email": "o@example.com",
                                        "display_name": "Ольга Владелец", "password": "owner-password-1"})

    # ------------------------------------------------------------ settings
    s = owner.call("GET", "/admin/mail")
    assert s["settings"]["enabled"] is False and s["available"] in (True, False)
    no_mail = not s["available"]
    owner.call("PUT", "/admin/mail", {"enabled": True, "host": "", "from_email": "x@example.com"}, expect=400)
    owner.call("PUT", "/admin/mail", {"enabled": True, "host": "127.0.0.1", "port": port, "from_email": "bad"}, expect=400)
    cfg = {"enabled": True, "host": "127.0.0.1", "port": port, "security": "none", "username": "",
           "from_email": "portal@example.com", "from_name": "Бюро «Почта»", "reply_to": "office@example.com"}
    r = owner.call("POST", "/admin/mail/test", {"to": "o@example.com", "settings": cfg})
    assert r["ok"], r
    msg = wait_mail("o@example.com", "Проверка почты")
    assert "portal@example.com" in msg["from"] and "office@example.com" in msg["reply-to"]
    assert msg.get_body(preferencelist=("html",)) is not None
    step("SMTP settings validated; test message sent through unsaved form settings")

    # A user created before mail is configured gets a link the admin can pass on.
    if no_mail:
        pre = owner.call("POST", "/users", {"email": "early@example.com", "display_name": "Ранний"})
        assert pre["emailed"] is False and "/invite/" in pre["invite_url"]
        step("Invitation without mail returns the link for the admin")

    owner.call("PUT", "/admin/mail", {**cfg, "password": "smtp-secret"})
    s = owner.call("GET", "/admin/mail")["settings"]
    assert s["enabled"] and s["has_password"] and "password" not in s and s["port"] == port
    owner.call("PUT", "/admin/mail", cfg)  # password omitted: kept
    assert owner.call("GET", "/admin/mail")["settings"]["has_password"]
    owner.call("PUT", "/admin/mail", {**cfg, "password": ""})  # cleared
    assert not owner.call("GET", "/admin/mail")["settings"]["has_password"]
    step("SMTP password stored encrypted, never returned, kept or cleared on demand")

    # ------------------------------------------------------------ invitation
    r = owner.call("POST", "/users", {"email": "ivan@example.com", "display_name": "Иван Петров"})
    assert r["emailed"] is True
    uid = r["id"]
    msg = wait_mail("ivan@example.com", "Приглашение")
    token = link(msg, "invite")
    assert token in r["invite_url"] and "Ольга Владелец" in text_of(msg)
    users = {u["id"]: u for u in owner.call("GET", "/users")["users"]}
    assert users[uid]["invite_pending"] is True
    anon = Client()
    anon.call("POST", "/auth/login", {"tenant": slug, "email": "ivan@example.com", "password": "!"}, expect=401)
    info = anon.call("POST", "/public/invite/check", {"token": token})
    assert info["email"] == "ivan@example.com" and info["tenant_slug"] == slug
    anon.call("POST", "/public/invite/check", {"token": "0" * 64}, expect=400)
    anon.call("POST", "/public/invite/accept", {"token": token, "password": "short"}, expect=400)
    step("Invitation emailed; invited user cannot log in yet; link checks out")

    # Resending replaces the old link.
    r2 = owner.call("POST", f"/admin/users/{uid}/invite")
    token2 = link(wait_mail("ivan@example.com", "Приглашение"), "invite")
    assert token2 != token and token2 in r2["invite_url"]
    anon.call("POST", "/public/invite/check", {"token": token}, expect=400)
    res = anon.call("POST", "/public/invite/accept", {"token": token2, "password": "ivan-password-1", "display_name": "Иван С. Петров"})
    assert res["tenant_slug"] == slug
    anon.call("POST", "/public/invite/accept", {"token": token2, "password": "ivan-password-2"}, expect=400)
    ivan = Client()
    ivan.call("POST", "/auth/login", {"tenant": slug, "email": "ivan@example.com", "password": "ivan-password-1"})
    assert ivan.call("GET", "/auth/me")["display_name"] == "Иван С. Петров"
    assert not {u["id"]: u for u in owner.call("GET", "/users")["users"]}[uid]["invite_pending"]
    owner.call("POST", f"/admin/users/{uid}/invite", expect=400)
    ivan.call("POST", f"/admin/users/{uid}/invite", expect=403)
    step("Resend invalidates the old link; accepting sets password and name once")

    # ------------------------------------------------------------ password reset
    r = anon.call("POST", "/public/password/forgot", {"tenant": slug, "email": "ivan@example.com"})
    assert r["ok"]
    msg = wait_mail("ivan@example.com", "Восстановление пароля")
    rtoken = link(msg, "reset")
    # Unknown accounts get the same answer and no mail.
    assert anon.call("POST", "/public/password/forgot", {"tenant": slug, "email": "nobody@example.com"})["ok"]
    assert anon.call("POST", "/public/password/forgot", {"tenant": "no-such-portal", "email": "ivan@example.com"})["ok"]
    assert anon.call("POST", "/public/password/check", {"token": rtoken})["email"] == "ivan@example.com"
    anon.call("POST", "/public/invite/check", {"token": rtoken}, expect=400)  # kinds do not mix
    anon.call("POST", "/public/password/reset", {"token": rtoken, "password": "ivan-new-password-1"})
    ivan.call("GET", "/auth/me", expect=401)  # sessions revoked
    wait_mail("ivan@example.com", "Пароль изменён")
    anon.call("POST", "/public/password/reset", {"token": rtoken, "password": "again-password-1"}, expect=400)
    Client().call("POST", "/auth/login", {"tenant": slug, "email": "ivan@example.com", "password": "ivan-password-1"}, expect=401)
    Client().call("POST", "/auth/login", {"tenant": slug, "email": "ivan@example.com", "password": "ivan-new-password-1"})
    time.sleep(1.0)
    assert not any("nobody@example.com" in r for r, _ in INBOX)
    step("Reset by link: one-time, revokes sessions, notifies; no account enumeration")

    # Deactivated users cannot use links.
    anon.call("POST", "/public/password/forgot", {"tenant": slug, "email": "ivan@example.com"})
    rt = link(wait_mail("ivan@example.com", "Восстановление пароля"), "reset")
    owner.call("PATCH", f"/users/{uid}", {"is_active": False})
    anon.call("POST", "/public/password/check", {"token": rt}, expect=400)
    owner.call("PATCH", f"/users/{uid}", {"is_active": True})
    step("Links of deactivated users stop working")

    # Per-account throttling of reset requests.
    for _ in range(3):
        anon.call("POST", "/public/password/forgot", {"tenant": slug, "email": "throttle@example.com"})
    anon.call("POST", "/public/password/forgot", {"tenant": slug, "email": "throttle@example.com"}, expect=429)
    step("Reset requests are rate limited per account")

    # ------------------------------------------------------------ delivery failures and log
    SmtpHandler.reject = True
    owner.call("POST", "/users", {"email": "petr@example.com", "display_name": "Пётр"})
    deadline = time.time() + 15
    while time.time() < deadline:
        log = owner.call("GET", "/admin/mail")["outbox"]
        bad = [m for m in log if m["to_email"] == "petr@example.com"]
        if bad and bad[0]["attempts"] >= 1:
            break
        time.sleep(0.3)
    assert bad[0]["status"] == "queued" and "550" in (bad[0]["last_error"] or ""), bad
    r = owner.call("POST", "/admin/mail/test", {"to": "o@example.com"})
    assert not r["ok"] and "550" in r["error"]
    SmtpHandler.reject = False
    owner.call("POST", f"/admin/mail/outbox/{bad[0]['id']}/retry")
    wait_mail("petr@example.com", "Приглашение")
    log = {m["id"]: m for m in owner.call("GET", "/admin/mail")["outbox"]}
    assert log[bad[0]["id"]]["status"] == "sent"
    assert all(m["status"] == "sent" for m in log.values() if m["to_email"] == "ivan@example.com")
    step("Rejected mail is retried with backoff; manual retry delivers; log shows status")

    # ------------------------------------------------------------ admin-set password completes an invitation
    r = owner.call("POST", "/users", {"email": "olga@example.com", "display_name": "Ольга"})
    tok = link(wait_mail("olga@example.com", "Приглашение"), "invite")
    owner.call("POST", f"/admin/users/{r['id']}/reset-password", {"password": "olga-password-1"})
    anon.call("POST", "/public/invite/check", {"token": tok}, expect=400)
    Client().call("POST", "/auth/login", {"tenant": slug, "email": "olga@example.com", "password": "olga-password-1"})
    step("Admin-set password completes a pending invitation and voids the link")

    # Tenants are isolated: settings of one portal are not visible to another.
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "Другое", "email": "x@example.com",
                                        "display_name": "X", "password": "other-password-1"})
    o = other.call("GET", "/admin/mail")
    assert not o["settings"]["enabled"] and not o["outbox"]
    member = Client()
    member.call("POST", "/auth/login", {"tenant": slug, "email": "ivan@example.com", "password": "ivan-new-password-1"})
    member.call("GET", "/admin/mail", expect=403)
    step("Mail settings and log are per portal and admin-only")

    actions = {e["action"] for e in owner.call("GET", "/admin/audit?limit=200")["entries"]}
    for a in ("mail.settings_updated", "mail.test", "user.invited", "user.invite_accepted", "auth.reset_requested", "auth.password_reset_by_link"):
        assert a in actions, a
    step("Audit log records mail settings, invitations and resets")
    print("\nALL MAIL TESTS PASSED")


if __name__ == "__main__":
    main()
