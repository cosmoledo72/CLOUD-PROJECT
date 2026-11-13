import sys, os, time
sys.path.insert(0, '/home/claude/cp/scripts')
from smoke_test import Client
from search_test import docx, xlsx, pdf
pid = open('/home/claude/run/pid.txt').read().strip()
o = Client(); o.call("POST", "/auth/login", {"tenant": "demo", "email": "o@example.com", "password": "owner-password-1"})
p = o.call("GET", f"/projects/{pid}")["project"]
f = p["folder_id"]
o.upload(f, "Техническое заключение по причалу.docx", docx(["Раздел 3. Техническое состояние несущих конструкций причала",
  "Обследование показало: коррозия шпунта в зоне переменного уровня до 30 % сечения, трещины в железобетонной надстройке.",
  "Рекомендуется усиление швартовных тумб и замена отбойных устройств."]), overwrite=True)
o.upload(f, "Ведомость дефектов.xlsx", xlsx([["№", "Элемент", "Дефект"], ["1", "Шпунт ШК-1", "Коррозия металла, утонение стенки"], ["2", "Надстройка", "Трещины раскрытием до 0,5 мм"]]), overwrite=True)
o.upload(f, "Протокол испытаний бетона.pdf", pdf(["Протокол испытаний № 14", "Прочность бетона надстройки соответствует классу B25.", "Коррозия арматуры не выявлена в кернах 1–4."]), overwrite=True)
tasks = o.call("GET", f"/projects/{pid}/tasks")["tasks"]
t = tasks[0]
o.call("POST", f"/tasks/{t['id']}/comments", {"body": "Коррозия шпунта сильнее, чем в прошлом отчёте — нужен дополнительный замер толщины."})
ch = next(c for c in o.call("GET", "/chats")["channels"] if c.get("project_id") == pid)
o.call("POST", f"/chats/{ch['id']}/messages", {"body": "Водолазы подтвердили коррозию шпунта у 3-й секции, фото выложу в папку."})
o.call("POST", "/kb/pages", {"title": "Методика обследования гидротехнических сооружений", "body": "Порядок оценки коррозии металлических элементов и трещин бетона по СП 41.13330."})
print("ok", t["title"])
