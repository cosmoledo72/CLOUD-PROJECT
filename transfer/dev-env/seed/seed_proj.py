import sys
sys.path.insert(0, "/home/claude/cp/scripts")
from smoke_test import Client
o = Client()
o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
ps = o.call("GET", "/projects")["projects"]
defs = o.call("GET", "/project-fields")["fields"]
print(len(ps), [(p["name"], p["status"], p.get("fields", {}).get("code")) for p in ps])
print([(d["key"], d["label"], d["field_type"], d["show_in_list"], d["section"]) for d in defs])
opts = {d["key"]: d.get("options") for d in defs}
print(opts["work_type"], opts["stage"])
me = o.call("GET", "/auth/me")["id"]
users = o.call("GET", "/users")["users"]
others = [u["id"] for u in users if not u["invite_pending"]]
cos = o.call("GET", "/crm/companies").get("companies", [])
print([c["name"] for c in cos][:5])
