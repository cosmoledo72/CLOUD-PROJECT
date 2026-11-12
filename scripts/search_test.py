#!/usr/bin/env python3
"""End-to-end test of the portal-wide search (F8 + C5): names and codes,
text inside DOCX/XLSX/PDF (Russian word forms, prefixes, highlighted
snippets), typos, task numbers, comments and chat messages, CRM, people;
access rules (project membership, other people's documents, shares, CRM
visibility, chat membership, guests) and switched-off modules; project and
date filters, paging.

Usage:
    API=http://localhost:8080 python3 scripts/search_test.py
"""

import io
import os
import sys
import time
import urllib.parse
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

FONT = os.path.join(os.path.dirname(__file__), "..", "backend", "assets", "fonts", "DejaVuSansCondensed.ttf")


def esc(s):
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def docx(paragraphs):
    b = io.BytesIO()
    with zipfile.ZipFile(b, "w") as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                   '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
        z.writestr("_rels/.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
        body = "".join(f"<w:p><w:r><w:t xml:space=\"preserve\">{esc(p)}</w:t></w:r></w:p>" for p in paragraphs)
        z.writestr("word/document.xml", '<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
                   f"<w:body>{body}</w:body></w:document>")
    return b.getvalue()


def xlsx(rows):
    b = io.BytesIO()
    strings = [c for r in rows for c in r]
    with zipfile.ZipFile(b, "w") as z:
        z.writestr("[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                   '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                   '<Default Extension="xml" ContentType="application/xml"/>'
                   '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
                   '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
                   '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>')
        z.writestr("_rels/.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="r1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>')
        z.writestr("xl/workbook.xml", '<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                   'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Лист1" sheetId="1" r:id="rId1"/></sheets></workbook>')
        z.writestr("xl/_rels/workbook.xml.rels", '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
                   '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>')
        z.writestr("xl/sharedStrings.xml", '<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                   + "".join(f"<si><t>{esc(s)}</t></si>" for s in strings) + "</sst>")
        cells, k = [], 0
        for i, r in enumerate(rows, 1):
            cs = []
            for j, _ in enumerate(r):
                cs.append(f'<c r="{chr(65 + j)}{i}" t="s"><v>{k}</v></c>')
                k += 1
            cells.append(f'<row r="{i}">{"".join(cs)}</row>')
        z.writestr("xl/worksheets/sheet1.xml", '<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
                   f'<sheetData>{"".join(cells)}</sheetData></worksheet>')
    return b.getvalue()


def pdf(lines):
    """A PDF with Cyrillic text (reportlab with the bundled DejaVu font), or None."""
    try:
        from reportlab.lib.pagesizes import A4
        from reportlab.pdfbase import pdfmetrics
        from reportlab.pdfbase.ttfonts import TTFont
        from reportlab.pdfgen import canvas
    except ImportError:
        return None
    pdfmetrics.registerFont(TTFont("DejaVu", FONT))
    b = io.BytesIO()
    c = canvas.Canvas(b, pagesize=A4)
    c.setFont("DejaVu", 12)
    for i, line in enumerate(lines):
        c.drawString(60, 780 - i * 18, line)
    c.save()
    return b.getvalue()


def search(c, q, **kw):
    params = urllib.parse.urlencode({"q": q, **kw})
    return {g["type"]: g for g in c.call("GET", f"/search?{params}")["groups"]}


def titles(groups, kind):
    return [h["title"] for h in groups.get(kind, {"items": []})["items"]]


def main():
    slug = "sr" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    ids, c = {}, {}
    for key, name, role in (("eng", "Егор Лаптев", "member"), ("out", "Олег Чужой", "member"), ("gst", "Гость Заказчика", "guest")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": "user-password-123",
                                                 "role": role})["id"]
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    eng, out, gst = c["eng"], c["out"], c["gst"]
    eng.call("PUT", "/auth/profile", {"display_name": "Егор Лаптев", "position": "Инженер-конструктор"})

    # --- Data ---------------------------------------------------------------------------
    pid = owner.call("POST", "/projects", {"name": "Обследование здания школы № 9", "description": "Оценка технического состояния кирпичных стен"})["id"]
    other = owner.call("POST", "/projects", {"name": "Реконструкция причала"})["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["eng"], "role": "member"})
    t1 = owner.call("POST", f"/projects/{pid}/tasks", {"title": "Обмеры фасада", "description": "Снять размеры оконных проёмов и карнизов"})["task"]
    owner.call("POST", f"/projects/{other}/tasks", {"title": "Обмеры причальной стенки"})
    owner.call("POST", f"/tasks/{t1['id']}/comments", {"body": "Нужно проверить трещины в кладке у входа"})
    chan = next(ch for ch in owner.call("GET", "/chats")["channels"] if ch.get("project_id") == pid)
    owner.call("POST", f"/chats/{chan['id']}/messages", {"body": "Согласовали выезд на объект в четверг"})

    folder = owner.call("GET", f"/projects/{pid}")["project"]["folder_id"]
    f_docx = owner.upload(folder, "Техническое заключение.docx",
                          docx(["Раздел 3. Состояние несущих конструкций", "Выявлены дефекты: глубокие трещины, коррозия закладных деталей."]))
    f_xlsx = owner.upload(folder, "Ведомость.xlsx", xlsx([["№", "Дефект"], ["1", "Коррозия арматуры перемычек"], ["2", "Отслоение штукатурки"]]))
    pdf_bytes = pdf(["Отчёт об обмерах", "Прогиб перекрытия над спортзалом составляет 42 мм"])
    if pdf_bytes:
        owner.upload(folder, "Отчёт об обмерах.pdf", pdf_bytes)
    owner.upload(folder, "Старый договор.doc", b"\xd0\xcf\x11\xe0 not an OLE file " + "Прогиб".encode())
    roots = eng.call("GET", "/folders/roots")
    eng.upload(roots["my"], "Личные заметки.docx", docx(["Черновик расчёта балки"]))
    shared = eng.upload(roots["my"], "Расчёт балки.docx", docx(["Проверка балки Б-1 на прогиб"]))
    shared_id = shared["ids"][0]

    # Text extraction runs in the background right after upload.
    deadline = time.time() + 20
    while time.time() < deadline:
        g = search(owner, "коррозия")
        if len(titles(g, "files")) >= 2:
            break
        time.sleep(0.5)

    # --- Morphology, prefixes, snippets ----------------------------------------------------
    g = search(owner, "коррозия")
    assert set(titles(g, "files")) == {"Техническое заключение.docx", "Ведомость.xlsx"}, titles(g, "files")
    hit = next(h for h in g["files"]["items"] if h["title"] == "Ведомость.xlsx")
    assert "⟦Коррозия⟧" in hit["snippet"] and hit["url"].startswith("/editor/"), hit
    g = search(owner, "несущая конструкция")
    assert titles(g, "files") == ["Техническое заключение.docx"], g
    assert "⟦несущих⟧" in g["files"]["items"][0]["snippet"] and "⟦конструкций⟧" in g["files"]["items"][0]["snippet"]
    assert titles(search(owner, "несущ"), "files") == ["Техническое заключение.docx"], "prefix"
    if pdf_bytes:
        g = search(owner, "прогибы перекрытий")
        assert titles(g, "files") == ["Отчёт об обмерах.pdf"], titles(g, "files")
    # Old binary formats: by name only.
    assert titles(search(owner, "договор"), "files") == ["Старый договор.doc"]
    assert "Старый договор.doc" not in titles(search(owner, "прогиб"), "files")
    step("Text inside DOCX, XLSX and PDF: Russian word forms, word prefixes, highlighted snippets; DOC by name only")

    # --- Names, typos, numbers, descriptions --------------------------------------------------
    g = search(owner, "обмеры")
    assert set(titles(g, "tasks")) == {"Обмеры фасада", "Обмеры причальной стенки"}
    assert g["tasks"]["items"][0]["url"].startswith("/projects/") and "?task=" in g["tasks"]["items"][0]["url"]
    assert "Отчёт об обмерах.pdf" in titles(g, "files") or not pdf_bytes
    assert titles(search(owner, "обследованее"), "projects") == ["Обследование здания школы № 9"], "typo"
    assert "Обмеры фасада" in titles(search(owner, f"#{t1['number']}"), "tasks")
    g = search(owner, "проёмов")
    assert titles(g, "tasks") == ["Обмеры фасада"] and "⟦проёмов⟧" in g["tasks"]["items"][0]["snippet"], g
    g = search(owner, "кирпичные")
    assert titles(g, "projects") == ["Обследование здания школы № 9"] and g["projects"]["items"][0]["snippet"]
    search(owner, "50%_\\")  # LIKE characters are escaped
    assert search(owner, "о") == {}, "one character: nothing"
    step("Projects and tasks by name, description, typos (4+ letters) and task number; LIKE characters escaped")

    # --- Comments, chats, people, CRM ------------------------------------------------------------
    g = search(owner, "трещины")
    assert titles(g, "comments") == [f"#{t1['number']} Обмеры фасада"] and "⟦трещины⟧" in g["comments"]["items"][0]["snippet"]
    assert "Техническое заключение.docx" in titles(g, "files")
    g = search(owner, "выезд")
    assert titles(g, "messages") == ["Обследование здания школы № 9"] and g["messages"]["items"][0]["url"] == f"/chats?c={chan['id']}"
    g = search(owner, "лаптев")
    assert titles(g, "people") == ["Егор Лаптев"] and "Инженер-конструктор" in g["people"]["items"][0]["subtitle"]
    owner.call("POST", "/crm/companies", {"name": "ООО «Школьный двор»", "inn": "7701885820"})
    owner.call("POST", "/crm/companies", {"name": "ООО «Тайна»", "visibility": "responsible", "responsible_id": ids["eng"]})
    assert titles(search(owner, "770188"), "companies") == ["ООО «Школьный двор»"], "by INN"
    assert titles(search(owner, "школьный"), "companies") == ["ООО «Школьный двор»"]
    step("Task comments and chat messages with snippets, people by name and position, companies by name and INN")

    # --- Access ------------------------------------------------------------------------------------
    g = search(out, "обмеры")
    assert titles(g, "tasks") == [] and titles(g, "files") == [], g
    for q in ("трещины", "выезд", "коррозия", "кирпичные"):
        assert not ({"tasks", "files", "comments", "messages", "projects"} & set(search(out, q))), q
    owner.call("PATCH", f"/users/{ids['out']}", {"crm_role": "none"})
    out.call("POST", "/auth/login", {"tenant": slug, "email": "out@example.com", "password": "user-password-123"})
    assert "companies" not in search(out, "школьный"), "no CRM role"
    owner.call("PATCH", f"/users/{ids['out']}", {"crm_role": "user"})
    out.call("POST", "/auth/login", {"tenant": slug, "email": "out@example.com", "password": "user-password-123"})
    assert titles(search(out, "школьный"), "companies") == ["ООО «Школьный двор»"]
    assert "companies" not in search(out, "тайна"), "responsible-only company"
    # Other people's «My documents» stay private even for administrators; shares open them.
    assert "Личные заметки.docx" not in titles(search(owner, "черновик"), "files")
    assert titles(search(eng, "черновик"), "files") == ["Личные заметки.docx"]
    assert "Расчёт балки.docx" not in titles(search(out, "балки"), "files")
    if shared_id:
        eng.call("POST", "/shares", {"resource_type": "file", "resource_id": shared_id, "user_id": ids["out"], "access": 1})
        assert titles(search(out, "балки"), "files") == ["Расчёт балки.docx"]
    # Membership opens the project's tasks, files, comments and chat.
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["out"], "role": "viewer"})
    g = search(out, "трещины")
    assert {"comments", "files"} <= set(g), g
    assert titles(search(out, "выезд"), "messages") == ["Обследование здания школы № 9"]
    # Guests: no people directory, no knowledge base, no CRM.
    assert not ({"people", "kb", "companies"} & set(search(gst, "лаптев") | search(gst, "школьный")))
    step("Rights: project membership, private and shared documents, CRM role and visibility, chat access, guests")

    # --- Knowledge base, modules ------------------------------------------------------------------
    owner.call("POST", "/kb/pages", {"title": "Регламент обследования", "body": "Порядок натурного обследования и фотофиксации дефектов"})
    g = search(eng, "фотофиксация")
    assert titles(g, "kb") == ["Регламент обследования"] and "⟦фотофиксации⟧" in g["kb"]["items"][0]["snippet"]
    owner.call("PUT", "/admin/modules", {"disabled": ["kb", "chats"]})
    eng.call("POST", "/auth/login", {"tenant": slug, "email": "eng@example.com", "password": "user-password-123"})
    assert "kb" not in search(eng, "фотофиксация") and "messages" not in search(eng, "выезд")
    owner.call("PUT", "/admin/modules", {"disabled": ["projects"]})
    eng.call("POST", "/auth/login", {"tenant": slug, "email": "eng@example.com", "password": "user-password-123"})
    g = search(eng, "обмеры")
    assert not ({"tasks", "projects", "comments"} & set(g)), g
    owner.call("PUT", "/admin/modules", {"disabled": []})
    step("Knowledge base pages; switched-off modules drop out of the results")

    # --- Filters and paging --------------------------------------------------------------------------
    g = search(owner, "обмеры", project=other)
    assert titles(g, "tasks") == ["Обмеры причальной стенки"] and "projects" not in g and "people" not in g
    g = search(owner, "обмеры", type="tasks", since="2099-01-01")
    assert g["tasks"]["items"] == []
    for i in range(7):
        owner.call("POST", f"/projects/{other}/tasks", {"title": f"Обмеры секции {i + 1}"})
    g = search(owner, "обмеры")
    assert len(g["tasks"]["items"]) == 5 and g["tasks"]["more"], "palette: 5 per group"
    p1 = search(owner, "обмеры", type="tasks", limit=4)["tasks"]
    p2 = search(owner, "обмеры", type="tasks", limit=4, offset=4)["tasks"]
    p3 = search(owner, "обмеры", type="tasks", limit=4, offset=8)["tasks"]
    assert p1["more"] and p2["more"] and not p3["more"] and len(p3["items"]) == 1
    assert len({h["id"] for h in p1["items"] + p2["items"] + p3["items"]}) == 9
    step("Project and date filters, 5 hits per group with «more», paging of one section")

    print("ALL SEARCH TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
