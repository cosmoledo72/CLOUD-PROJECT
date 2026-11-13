import sys; sys.path.insert(0, '/home/claude/cp/scripts')
from smoke_test import Client
o = Client()
o.call("POST", "/auth/signup", {"tenant_slug": "demo", "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com", "display_name": "Ольга Смирнова", "password": "owner-password-1"})
ids = {}
for k, n in [("a", "Анна Петрова"), ("b", "Борис Ковалёв")]:
    ids[k] = o.call("POST", "/users", {"email": f"{k}@example.com", "display_name": n, "password": f"{k}-password-123", "role": "member"})["id"]
me = o.call("GET", "/auth/me")["id"]
L = {}
for n, c in [("Ожидаем данные", "#ca8a04"), ("На экспертизе", "#7c3aed"), ("Доработка", "#dc2626"), ("Полевые работы", "#16a34a")]:
    L[n] = o.call("POST", "/task-labels", {"name": n, "color": c})["id"]
p1 = o.call("POST", "/projects", {"name": "Обследование корпуса 12"})["id"]
p2 = o.call("POST", "/projects", {"name": "Мост через р. Сусуя"})["id"]
p3 = o.call("POST", "/projects", {"name": "Реставрация усадьбы"})["id"]
for p in (p1, p2, p3):
    for k in "ab":
        o.call("POST", f"/projects/{p}/members", {"user_id": ids[k], "role": "member"})
T = [
 (p1, "Обмерные чертежи фасадов", ids["a"], 4, "2026-11-02", "2026-11-09", ["Полевые работы"]),
 (p1, "Фотофиксация дефектов", ids["b"], 2, "2026-11-10", "2026-11-14", ["Полевые работы"]),
 (p1, "Отчёт о техническом состоянии", me, 3, "2026-11-16", "2026-11-27", ["Доработка"]),
 (p1, "Согласование программы обследования", ids["a"], 2, None, "2026-11-12", []),
 (p2, "Инструментальное обследование опор", ids["b"], 3, "2026-11-11", "2026-11-20", ["Полевые работы"]),
 (p2, "Лабораторные испытания образцов", None, 2, "2026-11-18", "2026-11-28", ["Ожидаем данные"]),
 (p2, "Расчёт грузоподъёмности", ids["a"], 4, "2026-11-30", "2026-12-11", []),
 (p3, "Историко-архивные исследования", ids["a"], 2, "2026-10-26", "2026-11-06", ["Ожидаем данные"]),
 (p3, "Эскизный проект реставрации", me, 3, "2026-11-23", "2026-12-18", ["На экспертизе"]),
 (p3, "Сдать раздел в экспертизу", me, 2, None, None, []),
]
tid = []
for p, t, a, pr, s, d, ls in T:
    x = o.call("POST", f"/projects/{p}/tasks", {"title": t, "assignee_id": a, "priority": pr, "start_date": s, "due_date": d})["task"]
    tid.append(x["id"])
    if ls: o.call("PUT", f"/tasks/{x['id']}/labels", {"label_ids": [L[n] for n in ls]})
o.call("POST", f"/tasks/{tid[1]}/dependencies", {"depends_on_id": tid[0]})
o.call("POST", f"/tasks/{tid[2]}/dependencies", {"depends_on_id": tid[1]})
o.call("PATCH", f"/tasks/{tid[0]}", {"status": "in_progress", "progress": 60})
o.call("PATCH", f"/tasks/{tid[7]}", {"status": "done"})
for b, who, d, done in [("Северный фасад", ids["a"], "2026-11-04", True), ("Южный фасад", ids["a"], "2026-11-06", True), ("Дворовый фасад", ids["b"], "2026-11-08", False), ("Кровля и парапеты", None, None, False)]:
    r = o.call("POST", f"/tasks/{tid[0]}/checklist", {"body": b, "assignee_id": who, "due_date": d})
    if done: o.call("PATCH", f"/checklist-items/{r['id']}", {"done": True})
o.call("POST", f"/tasks/{tid[0]}/comments", {"body": "Северный и южный готовы, дворовый — после доступа во двор."})
o.call("POST", f"/tasks/{tid[0]}/comments", {"body": "Доступ согласован на 8 ноября."})
print(tid[0])
