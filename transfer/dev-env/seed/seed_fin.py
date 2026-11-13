import sys, datetime as dt, random
sys.path.insert(0, "/home/claude/cp/scripts")
from smoke_test import Client
o = Client()
o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
if o.call("GET", "/finance/payments")["payments"]:
    print("already"); sys.exit()
cats = {c["name"]: c["id"] for c in o.call("GET", "/finance/categories")["categories"]}
ps = {p["name"][:20]: p for p in o.call("GET", "/projects")["projects"]}
print(list(ps))
random.seed(4)
today = dt.date.today()
def pay(kind, d, amount, cat, proj=None, status="fact", desc="", company=""):
    o.call("POST", "/finance/payments", {"kind": kind, "status": status, "pay_date": d.isoformat(), "amount": amount,
        "category_id": cats.get(cat), "project_id": ps[proj]["id"] if proj else None, "description": desc, "company_name": company})
P = {k: k for k in ps}
names = list(ps)
inc = [("Мост через р. Сусуя", 375000, "ГКУ «Сахалинупрдор»", 120), ("Обследование корпуса", 320000, "ООО «Сахалин-Энерджи»", 60),
       ("Реконструкция берего", 1455000, "Росморпорт", 25), ("Реставрация усадьбы ", 2190000, "Минкультуры Сахалинской обл.", 200),
       ("Капитальный ремонт ш", 1170000, "Администрация Корсаковского ГО", 15), ("Авторский надзор: ле", 480000, "ГКУ «УКС»", 90),
       ("Инженерные изыскания", 645000, "АО «Сахалинская ипотечная компания»", 40)]
for name, amt, comp, ago in inc:
    pay("income", today - dt.timedelta(days=ago), amt, "Оплата по договору", name, desc="Аванс 30% по договору", company=comp)
for m in range(10, -1, -1):
    d = (today.replace(day=1) - dt.timedelta(days=30 * m)).replace(day=5)
    if d > today: continue
    pay("expense", d, 640000 + random.randint(0, 6) * 10000, "Зарплата", desc="Зарплата за месяц")
    pay("expense", d + dt.timedelta(days=3), 85000, "Офис", desc="Аренда офиса")
    if m < 8:
        pay("income", d + dt.timedelta(days=10), 300000 + random.randint(0, 9) * 50000, "Оплата по договору",
            random.choice(["Реставрация усадьбы ", "Реконструкция берего", "Капитальный ремонт ш"]), desc="Промежуточный платёж", company="")
pay("expense", today - dt.timedelta(days=20), 420000, "Субподряд", "Реконструкция берего", desc="Водолазное обследование", company="ООО «Подводтехсервис»")
pay("expense", today - dt.timedelta(days=12), 96000, "Лаборатория", "Мост через р. Сусуя", desc="Испытания бетона", company="ООО «СтройЛаб»")
pay("expense", today - dt.timedelta(days=8), 58000, "Командировки", "Реконструкция берего", desc="Командировка в Холмск")
pay("expense", today - dt.timedelta(days=40), 310000, "Субподряд", "Реставрация усадьбы ", desc="Обмерные работы", company="ИП Сергеев")
pay("income", today - dt.timedelta(days=4), 450000, "Оплата по договору", "Мост через р. Сусуя", status="plan", desc="Второй этап", company="ГКУ «Сахалинупрдор»")
pay("income", today + dt.timedelta(days=12), 1940000, "Оплата по договору", "Реконструкция берего", status="plan", desc="Оплата после сдачи этапа ПП", company="Росморпорт")
pay("income", today + dt.timedelta(days=25), 1500000, "Оплата по договору", "Капитальный ремонт ш", status="plan", desc="Этап П", company="Администрация Корсаковского ГО")
print(len(o.call("GET", "/finance/payments")["payments"]))
