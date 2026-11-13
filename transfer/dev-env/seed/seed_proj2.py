import sys
sys.path.insert(0, "/home/claude/cp/scripts")
from smoke_test import Client
o = Client()
o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
me = o.call("GET", "/auth/me")["id"]
ps = {p["name"]: p for p in o.call("GET", "/projects")["projects"]}
def upd(name, f):
    p = ps[name]
    o.call("PUT", f"/projects/{p['id']}/fields", {"values": {**p.get("fields", {}), **f}})
data = [
 ("Мост через р. Сусуя", {"code": "А0671", "work_type": "Техническое обследование", "stage": "Обследование", "client": {"id": None, "name": "ГКУ «Сахалинупрдор»", "inn": None}, "deadline": "2026-11-20", "chief_engineer": me, "contract_amount": 1250000}),
 ("Обследование корпуса 12", {"code": "А0675", "work_type": "Техническое обследование", "stage": "Обследование", "client": {"id": None, "name": "ООО «Сахалин-Энерджи»", "inn": None}, "deadline": "2027-01-15", "chief_engineer": me, "contract_amount": 640000}),
 ("Реставрация усадьбы Кривошеина", {"code": "А0590", "work_type": "Сохранение ОКН", "stage": "П+РД", "client": {"id": None, "name": "Минкультуры Сахалинской обл.", "inn": None}, "deadline": "2027-04-30", "chief_engineer": me, "contract_amount": 7300000}),
]
for n, f in data:
    try: upd(n, f)
    except AssertionError as e: print(e)
new = [
 ("Капитальный ремонт школы № 9, г. Корсаков", {"code": "А0702", "work_type": "Проектирование", "stage": "П", "client": {"id": None, "name": "Администрация Корсаковского ГО", "inn": None}, "deadline": "2027-02-28", "contract_amount": 3900000}),
 ("Инженерные изыскания: жилой квартал «Юго-Западный»", {"code": "А0695", "work_type": "Инженерные изыскания", "stage": "ПП", "client": {"id": None, "name": "АО «Сахалинская ипотечная компания»", "inn": None}, "deadline": "2026-12-05", "contract_amount": 2150000}),
 ("Авторский надзор: ледовая арена, Южно-Сахалинск", {"code": "А0612", "work_type": "Авторский надзор", "stage": "РД", "client": {"id": None, "name": "ГКУ «Управление капстроительства»", "inn": None}, "deadline": "2026-10-30", "contract_amount": 480000}),
]
for n, f in new:
    if n in ps: continue
    try:
        pid = o.call("POST", "/projects", {"name": n, "fields": {**f, "chief_engineer": me}})["id"]
    except AssertionError as e:
        print(e); continue
for n, st in [("Авторский надзор: ледовая арена, Южно-Сахалинск", "closed"), ("Реставрация усадьбы Кривошеина", "paused")]:
    p = {x["name"]: x for x in o.call("GET", "/projects")["projects"]}[n]
    try: o.call("PATCH", f"/projects/{p['id']}", {"status": st})
    except AssertionError as e: print(e)
print(len(o.call("GET", "/projects")["projects"]))
