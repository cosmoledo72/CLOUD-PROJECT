#!/usr/bin/env python3
"""End-to-end test of 2FA hardening (iteration 32, D3).

- TOTP secrets are stored encrypted (checked in the database when TEST_DATABASE_URL is set);
- 10 recovery codes are issued when 2FA is turned on, each works once at login;
- codes are re-issued with the password (old ones stop working);
- turning 2FA off (by the user or by an administrator) removes the codes.

Usage:
    API=http://localhost:8080 [TEST_DATABASE_URL=postgres://...] python3 scripts/mfa_test.py
"""

import os
import re
import subprocess
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step, totp_now  # noqa: E402

DB = os.environ.get("TEST_DATABASE_URL", "")
CODE_RE = re.compile(r"^[A-Z2-9]{4}-[A-Z2-9]{4}$")


def psql(sql):
    out = subprocess.run(["psql", DB, "-XAtc", sql], check=True, capture_output=True, text=True)
    return out.stdout.strip()


def login(slug, email, password):
    c = Client()
    r = c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    assert r["mfa_required"] is True
    return c


def main():
    slug = "mfa" + uuid.uuid4().hex[:8]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "MFA Org", "email": "o@example.com",
                                        "display_name": "Owner", "password": "owner-password-1"})
    assert owner.call("GET", "/auth/me")["recovery_codes_left"] == 0
    member_id = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Member",
                                              "password": "member-password-1"})["id"]

    # --- Enrolment: encrypted secret, 10 codes ---------------------------------
    setup = owner.call("POST", "/auth/totp/setup")
    r = owner.call("POST", "/auth/totp/enable", {"code": totp_now(setup["secret"])})
    codes = r["recovery_codes"]
    assert len(codes) == 10 and len(set(codes)) == 10, codes
    assert all(CODE_RE.match(c) for c in codes), codes
    assert owner.call("GET", "/auth/me")["recovery_codes_left"] == 10
    if DB:
        uid = owner.call("GET", "/auth/me")["id"]
        row = psql(f"SELECT totp_secret IS NULL, length(totp_secret_enc) > 12, "
                   f"position(convert_to('{setup['secret']}', 'UTF8') in totp_secret_enc) = 0 FROM users WHERE id = '{uid}'")
        assert row == "t|t|t", row
        hashes = psql(f"SELECT count(*) FROM user_recovery_codes WHERE user_id = '{uid}' AND code_hash <> convert_to('{codes[0]}', 'UTF8')")
        assert hashes == "10", hashes
    step("2FA on: the secret is stored encrypted, 10 one-time recovery codes XXXX-XXXX (only hashes are stored)")

    # --- Login with a recovery code ---------------------------------------------
    c = login(slug, "o@example.com", "owner-password-1")
    c.call("POST", "/auth/totp/verify", {"code": codes[0].lower().replace("-", " ")})
    me = c.call("GET", "/auth/me")
    assert me["recovery_codes_left"] == 9, me["recovery_codes_left"]
    c2 = login(slug, "o@example.com", "owner-password-1")
    c2.call("POST", "/auth/totp/verify", {"code": codes[0]}, expect=401)  # used already
    c2.call("POST", "/auth/totp/verify", {"code": "ABCD-EFGH"}, expect=401)  # not issued
    c2.call("POST", "/auth/totp/verify", {"code": codes[1]})
    assert c2.call("GET", "/auth/me")["recovery_codes_left"] == 8
    c3 = login(slug, "o@example.com", "owner-password-1")
    c3.call("POST", "/auth/totp/verify", {"code": totp_now(setup["secret"])})  # the app still works
    audit = owner.call("GET", "/admin/audit?action=auth.recovery_code_used&limit=10")["entries"]
    assert len(audit) == 2, audit
    step("Login by a recovery code (any case, dash or space); a code works once; the app code still works; audited")

    # --- Re-issue ---------------------------------------------------------------
    owner.call("POST", "/auth/totp/recovery-codes", {"password": "wrong-password-1"}, expect=403)
    new = owner.call("POST", "/auth/totp/recovery-codes", {"password": "owner-password-1"})["recovery_codes"]
    assert len(new) == 10 and not set(new) & set(codes)
    assert owner.call("GET", "/auth/me")["recovery_codes_left"] == 10
    c4 = login(slug, "o@example.com", "owner-password-1")
    c4.call("POST", "/auth/totp/verify", {"code": codes[2]}, expect=401)  # old set is gone
    c4.call("POST", "/auth/totp/verify", {"code": new[0]})
    card = owner.call("GET", f"/users/{owner.call('GET', '/auth/me')['id']}/profile")["user"]
    assert card["recovery_codes_left"] == 9, card.get("recovery_codes_left")
    step("New codes with the password (wrong password — 403); the previous set stops working; the card shows codes left")

    # --- Member: a colleague does not see the count; admin reset clears codes ---
    member = Client()
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "member-password-1"})
    ms = member.call("POST", "/auth/totp/setup")
    mcodes = member.call("POST", "/auth/totp/enable", {"code": totp_now(ms["secret"])})["recovery_codes"]
    member.call("POST", "/auth/totp/recovery-codes", {"password": "member-password-1"})
    owner_id = owner.call("GET", "/auth/me")["id"]
    assert "recovery_codes_left" not in member.call("GET", f"/users/{owner_id}/profile")["user"]
    assert owner.call("GET", f"/users/{member_id}/profile")["user"]["recovery_codes_left"] == 10
    owner.call("POST", f"/admin/users/{member_id}/reset-mfa")
    if DB:
        assert psql(f"SELECT count(*) FROM user_recovery_codes WHERE user_id = '{member_id}'") == "0"
        assert psql(f"SELECT totp_secret_enc IS NULL FROM users WHERE id = '{member_id}'") == "t"
    m2 = Client()
    r = m2.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "member-password-1"})
    assert r["mfa_required"] is False and len(mcodes) == 10
    step("Codes left: the user and administrators see it, colleagues do not; admin «reset 2FA» removes secret and codes")

    # --- Turning 2FA off by the user -------------------------------------------
    owner.call("POST", "/auth/totp/disable", {"password": "owner-password-1"})
    assert owner.call("GET", "/auth/me")["recovery_codes_left"] == 0
    owner.call("POST", "/auth/totp/recovery-codes", {"password": "owner-password-1"}, expect=400)
    step("2FA off: codes are removed, re-issue without 2FA is refused")

    print("ALL MFA TESTS PASSED")


if __name__ == "__main__":
    main()
