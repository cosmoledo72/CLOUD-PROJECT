import sys, random, json
sys.path.insert(0, '/home/claude/cp/scripts')
from smoke_test import Client
random.seed(7)
o = Client(); o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
users = {u["display_name"]: u["id"] for u in o.call("GET", "/users")["users"]}
mgrs = [users["Ольга Смирнова"], users["Борис Ковалёв"], users["Анна Петрова"]]
st = {s["name"]: s["id"] for s in o.call("GET", "/crm/stages")["stages"]}
opens = ["Коммерческое предложение", "Тендер", "Согласование договора"]
cos = []
for n, inn in [("АО «Морстройтехнология»", "7704211201"), ("ФКУ «Ространсмодернизация»", "7701885820"), ("ООО «Сочи-Порт»", "2320109650"), ("ГКУ «Дирекция Крымского моста»", "7707083893")]:
    try: cos.append(o.call("POST", "/crm/companies", {"name": n})["id"])
    except AssertionError: pass
titles = ["Обследование причала №", "Проект усиления пирса №", "Обмерные работы, корпус ", "Изыскания под берегоукрепление, участок ", "Экспертиза ограждающих конструкций, объект ", "Мониторинг деформаций, секция "]
reasons = ["Цена выше конкурентов"] * 5 + ["Проиграли тендер"] * 4 + ["Сроки не устроили"] * 2 + ["Заказчик отложил"] * 2 + [""]
out = []
for i in range(38):
    t = random.choice(titles) + str(random.randint(2, 19))
    amt = random.choice([450, 780, 1200, 1650, 2100, 2900, 3400, 900]) * 1000
    r = mgrs[i % 3] if i % 7 else mgrs[0]
    d = o.call("POST", "/crm/deals", {"title": t, "amount": amt, "company_id": random.choice(cos), "responsible_id": r})["deal"]["id"]
    depth = random.choice([0, 1, 1, 2, 2, 3])
    for s in opens[:depth]:
        o.call("PATCH", f"/crm/deals/{d}", {"stage_id": st[s]})
    fate = random.random()
    end = "open"
    if fate < 0.38 and depth >= 1:
        o.call("PATCH", f"/crm/deals/{d}", {"stage_id": st["Выиграна"]}); end = "won"
    elif fate < 0.75:
        o.call("PATCH", f"/crm/deals/{d}", {"stage_id": st["Проиграна"], "lost_reason": random.choice(reasons)}); end = "lost"
    out.append((d, random.randint(5, 175), end))
json.dump(out, open('seed28.json', 'w'))
print(len(out))
