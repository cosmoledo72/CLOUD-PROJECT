#!/usr/bin/env python3
"""End-to-end test of the sign-in page backend (iteration 34).

- GET /api/public/system: first start flag and whether portals may be created;
- «Запомнить меня»: a remembered session lives REMEMBER_TTL_HOURS (30 days by
  default) instead of SESSION_TTL_HOURS (7 days), also through the 2FA step.

Usage:
    API=http://localhost:8080 python3 scripts/login_page_test.py
"""

import os
import sys
import uuid
import urllib.request
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import API, Client, step, totp_now  # noqa: E402


def cookie_days(c, method, path, body):
    """Call and return the lifetime of the session cookie in days (from Expires)."""
    import json
    req = urllib.request.Request(API + path, data=json.dumps(body).encode(), method=method,
                                 headers={"Content-Type": "application/json"})
    with c.opener.open(req) as r:
        assert r.status == 200
        set_cookie = [v for k, v in r.headers.items() if k.lower() == "set-cookie" and v.startswith("wh_sid=")]
        r.read()
    assert set_cookie, "no session cookie"
    expires = [p.split("=", 1)[1] for p in set_cookie[-1].split("; ") if p.lower().startswith("expires=")][0]
    return (parsedate_to_datetime(expires) - datetime.now(timezone.utc)).total_seconds() / 86400


def main():
    s = Client().call("GET", "/public/system")
    assert set(s) == {"first_run", "signup_enabled"}, s
    assert s["first_run"] is False and s["signup_enabled"] is True, s
    step("/public/system: not the first start (portals exist), portal creation allowed in the test setup")

    slug = "lp" + uuid.uuid4().hex[:8]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "LP", "email": "o@example.com",
                                        "display_name": "Owner", "password": "owner-password-1"})
    creds = {"tenant": slug, "email": "o@example.com", "password": "owner-password-1"}
    d = cookie_days(Client(), "POST", "/auth/login", creds)
    assert 6.9 < d < 7.1, d
    d = cookie_days(Client(), "POST", "/auth/login", {**creds, "remember": True})
    assert 29.9 < d < 30.1, d
    step("Login: 7 days by default, 30 days with «Запомнить меня»")

    setup = owner.call("POST", "/auth/totp/setup")
    owner.call("POST", "/auth/totp/enable", {"code": totp_now(setup["secret"])})
    c = Client()
    assert c.call("POST", "/auth/login", {**creds, "remember": True})["mfa_required"] is True
    d = cookie_days(c, "POST", "/auth/totp/verify", {"code": totp_now(setup["secret"])})
    assert 29.9 < d < 30.1, d
    c2 = Client()
    c2.call("POST", "/auth/login", creds)
    d = cookie_days(c2, "POST", "/auth/totp/verify", {"code": totp_now(setup["secret"])})
    assert 6.9 < d < 7.1, d
    step("With 2FA the choice is carried through the code step")
    print("ALL LOGIN PAGE TESTS PASSED")


if __name__ == "__main__":
    main()
