import sys, os
sys.path.insert(0, "/home/claude/cp/scripts")
from smoke_test import Client
o = Client()
o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
# demo has no mail: applications go straight to admins
anon = Client()
for n, e, c in [("Ирина Котова", "i.kotova@partner-garant.ru", "ГИП, отдел обследований. Прошу добавить в проекты по Холмску."),
                ("Дмитрий Лебедев", "lebedev@mail.example", "")]:
    anon.call("POST", "/public/register", {"tenant": "demo", "display_name": n, "email": e, "password": "password-12345", "comment": c})
gs = o.call("GET", "/groups")["groups"]
if not any(g["name"] == "Инженеры" for g in gs):
    o.call("POST", "/groups", {"name": "Инженеры"})
    o.call("POST", "/groups", {"name": "Сметчики"})
gs = o.call("GET", "/groups")["groups"]
ps = o.call("GET", "/projects")["projects"][:2]
r = o.call("POST", "/admin/invitations", {"emails": "Анна Смирнова <anna.smirnova@partner-garant.ru>\nb.ivanov@partner-garant.ru",
    "role": "member", "group_ids": [gs[0]["id"]], "projects": [{"project_id": p["id"], "role": "member"} for p in ps],
    "message": "Подключайтесь к проектам по Холмску — документы уже в папках проектов."})
print(r)
print([p["name"] for p in ps])
