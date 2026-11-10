#!/usr/bin/env python3
"""End-to-end test of the CRM module: access roles and visibility,
counterparty validation and duplicates, XLSX/CSV import, contacts, pipeline
stages, deals, interaction history and projects created from deals.

Usage:
    API=http://localhost:8080 python3 scripts/crm_test.py
Requires: openpyxl (only to build the XLSX sample).
"""

import io
import os
import sys
import uuid
from urllib.parse import quote

sys.path.insert(0, os.path.dirname(__file__))
from iter3_test import multipart  # noqa: E402
from smoke_test import Client, step  # noqa: E402

SBER = {"name": "ПАО Сбербанк", "inn": "7707083893", "kpp": "773601001", "ogrn": "1027700132195",
        "bik": "044525225", "corr_account": "30101810400000000225", "account": "40702810938000000001"}


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def main():
    slug = "crm" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Owner", "password": "owner-password-1"})
    ids = {}
    for key, name in [("a", "Анна"), ("b", "Борис"), ("m", "Менеджер"), ("n", "Без CRM")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": f"{key}-password-123"})["id"]
    ids["g"] = owner.call("POST", "/users", {"email": "g@example.com", "display_name": "Гость", "password": "g-password-123", "role": "guest"})["id"]
    owner.call("PATCH", f"/users/{ids['m']}", {"crm_role": "manager"})
    owner.call("PATCH", f"/users/{ids['n']}", {"crm_role": "none"})
    a, b, m, n, g = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abmng")
    assert m.call("GET", "/auth/me")["crm_role"] == "manager"
    assert g.call("GET", "/auth/me")["crm_role"] == "none"
    n.call("GET", "/crm/companies", expect=403)
    g.call("GET", "/crm/deals", expect=403)
    step("CRM roles: manager, user, none; guests have no CRM")

    # ------------------------------------------------------------ companies
    a.call("POST", "/crm/companies", {**SBER, "inn": "7707083894"}, expect=400)
    a.call("POST", "/crm/companies", {**SBER, "account": "40702810038000000001"}, expect=400)
    sber = a.call("POST", "/crm/companies", SBER)["id"]
    a.call("POST", "/crm/companies", {**SBER, "name": "Дубль"}, expect=409)
    branch = a.call("POST", "/crm/companies", {"name": "Сбербанк, филиал", "inn": "7707083893", "kpp": "773645001"})["id"]
    chk = b.call("GET", "/crm/companies/check-inn?inn=7707083893&kpp=773601001")
    assert chk["valid"] and len(chk["matches"]) == 2 and any(x["same_kpp"] for x in chk["matches"])
    hidden = a.call("POST", "/crm/companies", {"name": "Секретный заказчик", "visibility": "responsible"})["id"]
    gid = owner.call("POST", "/groups", {"name": "Продажи"})["id"]
    owner.call("PUT", f"/groups/{gid}/members", {"user_ids": [ids["b"]]})
    grp = a.call("POST", "/crm/companies", {"name": "Группа видит", "visibility": "group", "group_id": gid})["id"]

    names_b = {c["name"] for c in b.call("GET", "/crm/companies")["companies"]}
    assert "Секретный заказчик" not in names_b and "Группа видит" in names_b and "ПАО Сбербанк" in names_b
    b.call("GET", f"/crm/companies/{hidden}", expect=404)
    m.call("GET", f"/crm/companies/{hidden}")
    owner.call("GET", f"/crm/companies/{hidden}")
    card = b.call("GET", f"/crm/companies/{sber}")
    assert card["can_edit"] is False and card["company"]["inn"] == "7707083893"
    b.call("PUT", f"/crm/companies/{sber}", {**SBER, "phone": "x"}, expect=403)
    a.call("PUT", f"/crm/companies/{sber}", {**SBER, "phone": "+7 495 500-55-50", "tags": ["банк", "ключевой"],
                                             "responsible_id": ids["a"]})
    a.call("PUT", f"/crm/companies/{sber}", {**SBER, "responsible_id": ids["b"]}, expect=403)  # only managers hand over
    assert [c["name"] for c in a.call("GET", "/crm/companies?tag=" + quote("ключевой"))["companies"]] == ["ПАО Сбербанк"]
    assert len(a.call("GET", "/crm/companies?q=7707083893")["companies"]) == 2
    step("companies: INN/OGRN/account checks, duplicates by INN+KPP, visibility (all/responsible/group), edit rights, search, tags")

    # ------------------------------------------------------------- contacts
    ivanov = a.call("POST", "/crm/contacts", {"company_id": sber, "last_name": "Иванов", "first_name": "Пётр",
                                              "position": "Главный инженер", "email": "ivanov@example.com"})["id"]
    b.call("POST", "/crm/contacts", {"company_id": sber, "last_name": "Чужой"}, expect=403)
    b.call("POST", "/crm/contacts", {"company_id": hidden, "last_name": "Чужой"}, expect=404)
    solo = b.call("POST", "/crm/contacts", {"last_name": "Сидорова", "first_name": "Ольга"})["id"]
    assert {c["id"] for c in b.call("GET", f"/crm/contacts?company_id={sber}")["contacts"]} == {ivanov}
    assert any(c["id"] == solo for c in a.call("GET", "/crm/contacts?q=" + quote("сидор"))["contacts"])
    a.call("POST", "/crm/contacts", {"company_id": sber}, expect=400)
    step("contacts: per company, standalone, edit rights, search")

    # --------------------------------------------------------------- import
    csv_text = "Наименование;ИНН;КПП;Телефон;Теги\nООО «Ромашка»;500100732259;;+7 900 000-00-00;новый\nОшибка;123;;;\nДубль Сбера;7707083893;773601001;;\n"
    body, hdr = multipart({}, "list.csv", csv_text.encode("cp1251"))
    r = a.call("POST", "/crm/companies/import", body, headers=hdr)
    assert r["created"] == 1 and len(r["skipped"]) == 2, r
    reasons = {s["row"]: s["reason"] for s in r["skipped"]}
    assert "ИНН" in reasons[3] and "дубликат" in reasons[4], reasons
    try:
        import openpyxl
        wb = openpyxl.Workbook()
        ws = wb.active
        ws.append(["Наименование", "ИНН", "Email"])
        ws.append(["ООО «Лютик»", 7707083893, "info@example.com"])  # numeric INN cell, no KPP -> new entity
        ws.append(["", None, None])
        buf = io.BytesIO()
        wb.save(buf)
        body, hdr = multipart({}, "list.xlsx", buf.getvalue())
        r = a.call("POST", "/crm/companies/import", body, headers=hdr)
        assert r["created"] == 1 and r["total"] == 1, r
    except ImportError:
        print("  (openpyxl not installed: XLSX import not checked)")
    body, hdr = multipart({}, "list.csv", "Имя\nX\n".encode())
    a.call("POST", "/crm/companies/import", body, headers=hdr, expect=400)
    n.call("POST", "/crm/companies/import", body, headers=hdr, expect=403)
    step("import: CSV in cp1251 with ';', XLSX with numeric INN, row-level errors and duplicates")

    # --------------------------------------------------------------- stages
    stages = a.call("GET", "/crm/stages")["stages"]
    assert [s["kind"] for s in stages] == ["open", "open", "open", "open", "won", "lost"]
    a.call("PUT", "/crm/stages", {"stages": stages}, expect=403)
    new = [dict(s) for s in stages]
    new[0]["name"] = "Входящий запрос"
    new.insert(2, {"name": "Расчёт стоимости", "kind": "open", "probability": 35, "color": "#0EA5E9"})
    stages = m.call("PUT", "/crm/stages", {"stages": new})["stages"]
    assert stages[0]["name"] == "Входящий запрос" and stages[2]["color"] == "#0ea5e9" and len(stages) == 7
    m.call("PUT", "/crm/stages", {"stages": [s for s in stages if s["kind"] != "won"]}, expect=400)
    st = {s["name"]: s["id"] for s in stages}
    step("pipeline: default stages, manager-only editing, validation")

    # ---------------------------------------------------------------- deals
    deal = a.call("POST", "/crm/deals", {"title": "Обследование корпуса 12", "company_id": sber, "contact_id": ivanov,
                                         "amount": 1250000.5, "expected_close": "2026-12-20"})["deal"]
    assert deal["stage_id"] == stages[0]["id"] and deal["company_name"] == "ПАО Сбербанк" and deal["amount"] == 1250000.5
    branch_contact = a.call("POST", "/crm/contacts", {"company_id": branch, "last_name": "Филиалов"})["id"]
    a.call("POST", "/crm/deals", {"title": "X", "company_id": sber, "contact_id": branch_contact}, expect=400)
    secret = a.call("POST", "/crm/deals", {"title": "Тайная сделка", "company_id": hidden})["deal"]["id"]
    b_deals = {d["title"] for d in b.call("GET", "/crm/deals")["deals"]}
    assert "Обследование корпуса 12" in b_deals and "Тайная сделка" not in b_deals
    b.call("PATCH", f"/crm/deals/{deal['id']}", {"amount": 1}, expect=403)
    b_own = b.call("POST", "/crm/deals", {"title": "Сделка Бориса", "company_id": grp})["deal"]["id"]
    a.call("PATCH", f"/crm/deals/{secret}", {"responsible_id": ids["b"]})
    assert "Тайная сделка" in {d["title"] for d in b.call("GET", "/crm/deals")["deals"]}  # responsible always sees

    d = a.call("PATCH", f"/crm/deals/{deal['id']}", {"stage_id": st["Расчёт стоимости"]})["deal"]
    assert d["closed_at"] is None and d["probability"] == 35
    d = a.call("PATCH", f"/crm/deals/{deal['id']}", {"stage_id": st["Выиграна"]})["deal"]
    assert d["closed_at"] and d["stage_kind"] == "won"
    lost = a.call("POST", "/crm/deals", {"title": "Проигранная", "company_id": sber})["deal"]["id"]
    a.call("PATCH", f"/crm/deals/{lost}", {"stage_id": st["Проиграна"], "lost_reason": "Дешевле у конкурента"})
    m.call("PUT", "/crm/stages", {"stages": [s for s in stages if s["name"] != "Проиграна"] + [{"name": "Отказ", "kind": "lost", "probability": 0, "color": "#991b1b"}]}, expect=400)
    full = a.call("GET", f"/crm/deals/{deal['id']}")
    texts = [x["body"] for x in full["activities"]]
    assert any("Расчёт стоимости → Выиграна" in t for t in texts) and any("Создана сделка" in t for t in texts), texts
    lost_feed = a.call("GET", f"/crm/deals/{lost}")["activities"]
    assert any("Причина: Дешевле у конкурента" in x["body"] for x in lost_feed)
    assert len(a.call("GET", "/crm/deals?closed_days=0")["deals"]) == 2  # only open deals remain
    step("deals: create, visibility via company and responsible, edit rights, stage moves, won/lost, history entries")

    # ----------------------------------------------------------- activities
    call = a.call("POST", "/crm/activities", {"company_id": sber, "contact_id": ivanov, "kind": "call",
                                             "body": "Обсудили сроки обследования"})["id"]
    b.call("POST", "/crm/activities", {"deal_id": deal["id"], "kind": "note", "body": "Заметка Бориса"})
    b.call("POST", "/crm/activities", {"company_id": hidden, "kind": "note", "body": "x"}, expect=404)
    a.call("POST", "/crm/activities", {"company_id": sber, "kind": "system", "body": "x"}, expect=400)
    feed = a.call("GET", f"/crm/companies/{sber}")["activities"]
    kinds = [x["kind"] for x in feed]
    assert "call" in kinds and "system" in kinds and any(x["body"] == "Заметка Бориса" for x in feed)  # deal notes reach the company feed
    b.call("DELETE", f"/crm/activities/{call}", expect=403)
    sys_id = next(x["id"] for x in feed if x["kind"] == "system")
    a.call("DELETE", f"/crm/activities/{sys_id}", expect=403)
    m.call("DELETE", f"/crm/activities/{sys_id}")
    a.call("DELETE", f"/crm/activities/{call}")
    step("history: calls/notes, deal entries in company feed, delete rights (author, manager)")

    # -------------------------------------------------------------- projects
    owner.call("POST", "/admin/project-fields/preset")
    dflt = a.call("GET", f"/crm/deals/{deal['id']}/project")
    assert dflt["fields"] == {"client": {"id": sber, "name": "ПАО Сбербанк", "inn": "7707083893"},
                              "client_inn": "7707083893", "contract_amount": 1250000.5}, dflt
    a.call("POST", f"/crm/deals/{deal['id']}/project", {}, expect=400)  # required "code" and "object_name" missing
    pr = a.call("POST", f"/crm/deals/{deal['id']}/project", {"fields": {"code": "125-24-ТО", "object_name": "Корпус 12"}})
    a.call("POST", f"/crm/deals/{deal['id']}/project", {"fields": {"code": "X", "object_name": "Y"}}, expect=409)
    proj = a.call("GET", f"/projects/{pr['project_id']}")["project"]
    assert proj["company_id"] == sber and proj["company_name"] == "ПАО Сбербанк" and proj["deal_id"] == deal["id"]
    assert proj["fields"]["client"]["name"] == "ПАО Сбербанк" and proj["fields"]["contract_amount"] == 1250000.5
    assert a.call("GET", f"/crm/deals/{deal['id']}")["deal"]["project_name"] == "Обследование корпуса 12"
    assert [p["id"] for p in a.call("GET", f"/crm/companies/{sber}")["projects"]] == [pr["project_id"]]
    lst = a.call("GET", "/crm/companies?q=" + quote("сбербанк"))["companies"]
    assert next(c for c in lst if c["id"] == sber)["projects"] == 1

    plain = owner.call("POST", "/projects", {"name": "Без заказчика", "fields": {"code": "1", "object_name": "2", "client": "старое"}})["id"]
    owner.call("PUT", f"/projects/{plain}/company", {"company_id": branch})
    p2 = owner.call("GET", f"/projects/{plain}")["project"]
    assert p2["company_name"] == "Сбербанк, филиал" and p2["fields"]["client"]["id"] == branch and p2["fields"]["client_inn"] == "7707083893"
    assert owner.call("GET", f"/projects/{plain}")["project"]["fields"]["client"]["name"] == "Сбербанк, филиал"
    owner.call("PUT", f"/projects/{plain}/company", {"company_id": None})
    assert owner.call("GET", f"/projects/{plain}")["project"]["company_id"] is None
    step("projects: from won deal with requisites, one project per deal, link/unlink customer on existing project")

    # The "Заказчик" requisite is a directory field: picking a company links the project.
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": {"id": sber}}})
    p3 = owner.call("GET", f"/projects/{plain}")["project"]
    assert p3["company_id"] == sber and p3["fields"]["client"] == {"id": sber, "name": "ПАО Сбербанк", "inn": "7707083893"}
    # Typing a name that is not in the directory keeps the text and unlinks.
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": "ИП Сидоров"}})
    p3 = owner.call("GET", f"/projects/{plain}")["project"]
    assert p3["company_id"] is None and p3["fields"]["client"] == {"id": None, "name": "ИП Сидоров", "inn": None}
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": {"id": str(uuid.uuid4())}}}, expect=400)
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": {"id": None, "name": " "}}}, expect=400)  # required
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": {"id": branch, "name": "игнорируется"}}})
    p3 = owner.call("GET", f"/projects/{plain}")["project"]
    assert p3["company_id"] == branch and p3["fields"]["client"]["name"] == "Сбербанк, филиал"
    # Document templates print the customer's name.
    defs = {d["key"]: d for d in owner.call("GET", "/project-fields")["fields"]}
    assert defs["client"]["field_type"] == "company"
    owner.call("PUT", f"/projects/{plain}/company", {"company_id": None})
    assert "client" not in owner.call("GET", f"/projects/{plain}")["project"]["fields"]
    b.call("PUT", f"/projects/{plain}/company", {"company_id": sber}, expect=404)  # not a project member
    owner.call("PUT", f"/projects/{plain}/fields", {"values": {"client": {"id": sber}}})
    step("requisite «Заказчик» picks a CRM company, links the project; free text unlinks")

    # Deleting a company keeps deals and projects, clears links.
    owner.call("DELETE", f"/crm/companies/{sber}")
    assert a.call("GET", f"/projects/{pr['project_id']}")["project"]["company_id"] is None
    kept = owner.call("GET", f"/projects/{plain}")["project"]["fields"]["client"]
    assert kept == {"id": None, "name": "ПАО Сбербанк", "inn": "7707083893"}, kept
    assert a.call("GET", f"/crm/deals/{deal['id']}")["deal"]["company_id"] is None
    b.call("DELETE", f"/crm/deals/{b_own}")

    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "X", "email": "x@example.com",
                                        "display_name": "X", "password": "other-password-1"})
    other.call("GET", f"/crm/companies/{grp}", expect=404)
    other.call("GET", f"/crm/deals/{deal['id']}", expect=404)
    assert other.call("GET", "/crm/companies")["companies"] == []
    assert len(other.call("GET", "/crm/stages")["stages"]) == 6
    step("company deletion keeps deals/projects; tenant isolation")
    print("ALL CRM TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
