#!/usr/bin/env python3
"""End-to-end test of the Gantt export (C2): PDF (A3/A4 landscape, several
sheets, brand stamp), XLSX (chart and task list) and MS Project XML (tasks,
outline, links, calendar); stamp names suggested and remembered per project;
filters and permissions.

Usage:
    API=http://localhost:8080 python3 scripts/gantt_export_test.py
"""

import io
import os
import re
import shutil
import subprocess
import sys
import tempfile
import uuid
import xml.etree.ElementTree as ET
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

NS = {"p": "http://schemas.microsoft.com/project"}


def pdf_pages(b):
    return len(re.findall(rb"/Type\s*/Page[^s]", b))


def pdf_text(b):
    """Text of a PDF via pdftotext when it is installed (else None)."""
    if not shutil.which("pdftotext"):
        return None
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
        f.write(b)
    try:
        return subprocess.run(["pdftotext", "-layout", f.name, "-"], capture_output=True, text=True, check=True).stdout
    finally:
        os.unlink(f.name)


def main():
    slug = "gx" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    ids = {}
    for key, name in (("gip", "Глеб Ковалёв"), ("eng", "Егор Лаптев"), ("view", "Вера Орлова"), ("out", "Олег Чужой")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": "user-password-123", "role": "member"})["id"]
    c = {}
    for key in ids:
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    gip, eng, view, out = c["gip"], c["eng"], c["view"], c["out"]

    pid = gip.call("POST", "/projects", {"name": "Обследование здания школы № 9"})["id"]
    gip.call("POST", f"/projects/{pid}/members", {"user_id": ids["eng"], "role": "member"})
    gip.call("POST", f"/projects/{pid}/members", {"user_id": ids["view"], "role": "viewer"})

    def task(title, start=None, due=None, **kw):
        return gip.call("POST", f"/projects/{pid}/tasks", {"title": title, "start_date": start, "due_date": due, **kw})["task"]["id"]

    gip.call("POST", f"/projects/{pid}/tasks", {"title": "x"}, expect=200)  # a task without dates
    phase = task("Натурные работы")
    a = task("Обмеры", "2026-12-21", "2026-12-25", parent_id=phase, assignee_id=ids["eng"])
    b = task("Фотофиксация & «дефекты»", "2026-12-28", "2027-01-15", parent_id=phase, assignee_id=ids["eng"])
    r = task("Отчёт о техническом состоянии", "2027-01-18", "2027-02-26", assignee_id=ids["gip"])
    ms = task("Сдача отчёта", None, "2027-03-01", is_milestone=True)
    gip.call("POST", f"/tasks/{b}/dependencies", {"depends_on_id": a})
    gip.call("POST", f"/tasks/{r}/dependencies", {"depends_on_id": b, "kind": "SS", "lag_days": 2})
    gip.call("POST", f"/tasks/{ms}/dependencies", {"depends_on_id": r})
    gip.call("PATCH", f"/tasks/{a}", {"status": "done"})
    gip.call("POST", f"/projects/{pid}/baseline")

    # --- Settings and permissions --------------------------------------------------
    out.call("GET", f"/projects/{pid}/gantt/export", expect=404)
    out.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf"}, expect=404)
    s = eng.call("GET", f"/projects/{pid}/gantt/export")["stamp"]
    assert s == {"developer": "Егор Лаптев", "checker": "", "gip": "Глеб Ковалёв"}, s
    gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "doc"}, expect=400)
    gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "paper": "a2"}, expect=400)
    gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "scale": "year"}, expect=400)
    gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "task_ids": [a], "hide_done": True}, expect=400)
    step("Export settings: stamp names suggested (exporting user, project manager as ГИП); bad parameters refused")

    # --- PDF -------------------------------------------------------------------------
    stamp = {"developer": "  Е.  Лаптев ", "checker": "В. Орлова", "gip": "Г. Ковалёв"}
    pdf = gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "paper": "a3", "baseline": True, "stamp": stamp}, raw=True)
    assert pdf[:5] == b"%PDF-" and pdf_pages(pdf) == 1, pdf_pages(pdf)
    assert b"/MediaBox" in pdf and re.search(rb"/MediaBox\s*\[\s*0\s+0\s+1190\.\d+\s+841\.\d+", pdf), "A3 landscape"
    t = pdf_text(pdf)
    if t is not None:
        for want in ("КАЛЕНДАРНЫЙ ГРАФИК РАБОТ", "Обследование здания школы № 9", "Фотофиксация & «дефекты»", "Е. Лаптев", "В. Орлова",
                     "Г. Ковалёв", "Лист", "1 из 1", "Егор Лаптев", "Натурные работы", "Сдача отчёта", "раб. дн."):
            assert want in t, (want, t[:2000])
    # Remembered for the project (cleaned).
    assert eng.call("GET", f"/projects/{pid}/gantt/export")["stamp"] == {"developer": "Е. Лаптев", "checker": "В. Орлова", "gip": "Г. Ковалёв"}
    # A viewer may export but does not change the remembered names.
    view.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "stamp": {"developer": "Чужой", "checker": "", "gip": ""}}, raw=True)
    assert gip.call("GET", f"/projects/{pid}/gantt/export")["stamp"]["developer"] == "Е. Лаптев"
    a4 = gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "pdf", "paper": "a4", "scale": "day"}, raw=True)
    assert re.search(rb"/MediaBox\s*\[\s*0\s+0\s+841\.\d+\s+595\.\d+", a4), "A4 landscape"
    assert pdf_pages(a4) > 1, "a day scale over three months continues on further sheets"
    t = pdf_text(a4)
    if t is not None:
        assert f"1 из {pdf_pages(a4)}" in t and "шкала продолжается на соседних листах" in t
    step("PDF: A3/A4 landscape, Cyrillic text, brand stamp with names, extra sheets for a long day scale; names remembered (not by viewers)")

    # --- XLSX ------------------------------------------------------------------------
    x = eng.call("POST", f"/projects/{pid}/gantt/export", {"format": "xlsx"}, raw=True)
    z = zipfile.ZipFile(io.BytesIO(x))
    wb = z.read("xl/workbook.xml").decode()
    assert "График" in wb and "Задачи" in wb
    ss = z.read("xl/sharedStrings.xml").decode()
    for want in ("Календарный график работ", "Обмеры", "Фотофиксация &amp; «дефекты»", "Предшественники", "2.1", "#", "4НН+2", "веха"):
        assert want in ss, want
    step("XLSX: «График» with the shaded chart, «Задачи» with outline numbers and predecessors in MS Project notation")

    # --- MS Project XML ---------------------------------------------------------------
    xml = eng.call("POST", f"/projects/{pid}/gantt/export", {"format": "xml", "hide_done": True}, raw=True)
    root = ET.fromstring(xml)
    tasks = root.findall("p:Tasks/p:Task", NS)
    names = [tk.findtext("p:Name", namespaces=NS) for tk in tasks]
    assert "Обмеры" not in names, "done tasks left out"
    assert "Фотофиксация & «дефекты»" in names and names[0].startswith("Обследование")
    by = {tk.findtext("p:Name", namespaces=NS): tk for tk in tasks}
    f = by["Фотофиксация & «дефекты»"]
    assert f.findtext("p:OutlineLevel", namespaces=NS) == "2" and f.findtext("p:Start", namespaces=NS) == "2026-12-28T08:00:00"
    # 28.12–15.01: 28–30.12 and 11–15.01 are working days (31.12 and 1–10.01 are off) — 8 days.
    assert f.findtext("p:Duration", namespaces=NS) == "PT64H0M0S", f.findtext("p:Duration", namespaces=NS)
    rep = by["Отчёт о техническом состоянии"]
    link = rep.find("p:PredecessorLink", NS)
    assert link.findtext("p:Type", namespaces=NS) == "3" and link.findtext("p:LinkLag", namespaces=NS) == "9600"
    assert by["Сдача отчёта"].findtext("p:Milestone", namespaces=NS) == "1"
    assert by["Натурные работы"].findtext("p:Summary", namespaces=NS) == "1"
    periods = {p.findtext("p:FromDate", namespaces=NS): p.findtext("p:ToDate", namespaces=NS)
               for p in root.findall("p:Calendars/p:Calendar/p:WeekDays/p:WeekDay/p:TimePeriod", NS)}
    # 31.12.2026–01.01.2027 off (one run), then 04–08.01.2027 (weekends are already off).
    assert periods.get("2026-12-31T00:00:00") == "2027-01-01T23:59:00", periods
    assert periods.get("2027-01-04T00:00:00") == "2027-01-08T23:59:00", periods
    res = [r.findtext("p:Name", namespaces=NS) for r in root.findall("p:Resources/p:Resource", NS)]
    assert "Егор Лаптев" in res and "Глеб Ковалёв" in res
    step("MS Project XML: outline, working-day durations, SS+2 link, milestone, summary, holidays as calendar exceptions, resources")

    # --- Calendar-day projects and the audit --------------------------------------------
    gip.call("PATCH", f"/projects/{pid}", {"day_mode": "calendar"})
    root = ET.fromstring(gip.call("POST", f"/projects/{pid}/gantt/export", {"format": "xml"}, raw=True))
    f = [tk for tk in root.findall("p:Tasks/p:Task", NS) if tk.findtext("p:Name", namespaces=NS) == "Фотофиксация & «дефекты»"][0]
    assert f.findtext("p:DurationFormat", namespaces=NS) == "39"
    audit = owner.call("GET", "/admin/audit?action=project.gantt_exported")["entries"]
    assert len(audit) >= 6 and {e["details"]["format"] for e in audit} == {"pdf", "xlsx", "xml"}
    step("Calendar-day projects export elapsed durations; every export is in the audit log")

    print("ALL GANTT EXPORT TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
