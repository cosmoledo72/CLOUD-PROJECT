#!/usr/bin/env python3
"""End-to-end test of interface settings (view settings): the project list
fields each user chooses for themselves and the portal default set by an
administrator.

Usage:
    API=http://localhost:8080 python3 scripts/view_settings_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

KEY = "/view-settings/projects_list"


def main():
    slug = "vs" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк", "password": "m-password-123", "role": "member"})
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})

    assert m.call("GET", KEY) == {"value": None, "own": False, "default": None}
    m.call("GET", "/view-settings/unknown", expect=400)
    m.call("PUT", KEY, {"value": [1, 2]}, expect=400)
    m.call("PUT", KEY, {"value": {"columns": ["x" * 20000]}}, expect=400)
    Client().call("GET", KEY, expect=401)
    step("Nothing stored at first; unknown keys, non-objects and oversized values rejected; sign-in required")

    mine = {"columns": ["code", "$name", "client", "$status"], "subtitle": "stage", "card": ["code"], "group": "$status"}
    r = m.call("PUT", KEY, {"value": mine})
    assert r["value"] == mine and r["own"] is True
    assert owner.call("GET", KEY)["value"] is None, "one user's choice does not affect others"
    step("A user's own field set is stored and is private to them")

    dflt = {"columns": ["$name", "deadline"], "subtitle": None, "card": [], "group": None}
    m.call("PUT", "/admin" + KEY, {"value": dflt}, expect=403)
    owner.call("PUT", "/admin" + KEY, {"value": dflt})
    assert owner.call("GET", KEY) == {"value": dflt, "own": False, "default": dflt}
    r = m.call("GET", KEY)
    assert r["value"] == mine and r["own"] and r["default"] == dflt, "own setting wins over the default"
    r = m.call("DELETE", KEY)
    assert r == {"value": dflt, "own": False, "default": dflt}, "reset falls back to the portal default"
    owner.call("PUT", "/admin" + KEY, {"value": None})
    assert m.call("GET", KEY)["value"] is None
    rows = owner.call("GET", "/admin/audit?action=portal.view_default")["entries"]
    assert len(rows) == 2
    step("Portal default set by admins only; own setting wins; reset returns to the default; changes audited")

    print("ALL VIEW SETTINGS TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
