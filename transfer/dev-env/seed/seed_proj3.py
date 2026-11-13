import sys
sys.path.insert(0, "/home/claude/cp/scripts")
from smoke_test import Client
o = Client()
o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
us = [u for u in o.call("GET", "/users")["users"] if not u["invite_pending"] and u["is_active"]]
print([u["display_name"] for u in us])
ps = o.call("GET", "/projects")["projects"]
for i, p in enumerate(sorted(ps, key=lambda x: x["name"])):
    f = dict(p.get("fields") or {})
    f["chief_engineer"] = us[i % len(us)]["id"]
    try: o.call("PUT", f"/projects/{p['id']}/fields", {"values": f})
    except AssertionError as e: print(e)
