#!/usr/bin/env python3
"""End-to-end test of sales documents: organisation requisites, numbering,
sales templates, generation for a deal with correct substitutions (amount in
words, VAT, basis contract), company folders governed by CRM visibility.

Usage:
    API=http://localhost:8080 python3 scripts/sales_docs_test.py
"""

import datetime as dt
import io
import os
import re
import sys
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

NO_ACCESS = 404  # resources the user cannot see are reported as missing


def text_of(docx_bytes):
    with zipfile.ZipFile(io.BytesIO(docx_bytes)) as z:
        xml = z.read("word/document.xml").decode()
    # Join paragraph texts; tags removed.
    xml = re.sub(r"</w:p>", "\n", xml)
    xml = re.sub(r"<w:tab/>", "\t", xml)
    return re.sub(r"<[^>]+>", "", xml).replace("&quot;", '"').replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">")


def main():
    slug = "sd" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ООО «Инж-Групп»", "email": "o@example.com",
                                        "display_name": "Ольга Директорова", "password": "owner-password-1"})
    ids = {}
    for key, name, crm in [("a", "Анна", "user"), ("b", "Борис", "user"), ("n", "Нина", "none")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": f"{key}-password-123"})["id"]
        owner.call("PATCH", f"/users/{ids[key]}", {"crm_role": crm})
    clients = {}
    for k in "abn":
        c = Client()
        c.call("POST", "/auth/login", {"tenant": slug, "email": f"{k}@example.com", "password": f"{k}-password-123"})
        clients[k] = c
    a, b, n = clients["a"], clients["b"], clients["n"]

    # ------------------------------------------------------------ organisation requisites
    org = {"full_name": "Общество с ограниченной ответственностью «Инж-Групп»", "inn": "7707083893", "kpp": "773601001",
           "ogrn": "1027700132195", "legal_address": "117312, г. Москва, ул. Вавилова, д. 19", "phone": "+7 495 000-00-00",
           "email": "info@inzh.ru", "director": "Орлов Д. А.", "director_title": "Генеральный директор", "accountant": "Иванова Е. П.",
           "bank_name": "ПАО Сбербанк", "bik": "044525225", "account": "40702810938000000001", "corr_account": "30101810400000000225",
           "vat_rate": "22"}
    owner.call("PUT", "/admin/organization", {**org, "inn": "7707083894"}, expect=400)
    owner.call("PUT", "/admin/organization", {**org, "account": "40702810938000000002"}, expect=400)
    owner.call("PUT", "/admin/organization", {**org, "vat_rate": "18"}, expect=400)
    a.call("PUT", "/admin/organization", org, expect=403)
    owner.call("PUT", "/admin/organization", org)
    got = owner.call("GET", "/admin/organization")
    assert got["requisites"]["inn"] == "7707083893" and "22" in got["vat_rates"]
    step("Organisation requisites: checksums, VAT rates, admin only")

    # ------------------------------------------------------------ numbering
    num = {x["kind"]: x for x in owner.call("GET", "/admin/numbering")["numbering"]}
    assert num["invoice"]["pattern"] == "{n}" and num["invoice"]["next"] == 1 and num["quote"]["example"].startswith("КП-1/")
    owner.call("PUT", "/admin/numbering", [{"kind": "act", "pattern": "без номера"}], expect=400)
    owner.call("PUT", "/admin/numbering", [{"kind": "contract", "pattern": "{n:3}-ТО/{yy}", "next": 41}])
    num = {x["kind"]: x for x in owner.call("GET", "/admin/numbering")["numbering"]}
    yy = f"{dt.date.today().year % 100:02d}"
    assert num["contract"]["example"] == f"041-ТО/{yy}", num["contract"]
    assert a.call("GET", "/crm/documents/next-number?kind=contract")["number"] == f"041-ТО/{yy}"
    step("Numbering: patterns {n}/{n:3}/{yy}, continue from a given number, preview")

    # ------------------------------------------------------------ templates
    r = owner.call("POST", "/admin/templates/sales-samples")
    assert len(r["created"]) == 4
    assert owner.call("POST", "/admin/templates/sales-samples")["created"] == []  # idempotent
    tpls = {t["doc_kind"]: t for t in owner.call("GET", "/templates")["templates"] if t["scope"] == "sales"}
    assert set(tpls) == {"quote", "contract", "invoice", "act"}
    assert "doc.amount_words" in tpls["invoice"]["placeholders"] and "org.bik" in tpls["invoice"]["placeholders"]
    cat = {p["key"] for p in owner.call("GET", "/templates/placeholders?scope=sales")["placeholders"]}
    assert {"client.inn", "org.account", "doc.vat_text", "deal.title", "date"} <= cat
    assert "client.inn" not in {p["key"] for p in owner.call("GET", "/templates/placeholders")["placeholders"]}
    pid = owner.call("POST", "/projects", {"name": "Проект"})["id"]
    owner.call("POST", f"/projects/{pid}/generate", {"template_id": tpls["invoice"]["id"]}, expect=400)
    owner.call("PUT", f"/admin/templates/{tpls['act']['id']}", {"name": "Акт", "scope": "sales", "doc_kind": "bogus"}, expect=400)
    step("Sales templates: samples, scope and kind, placeholder catalogue, not usable for projects")

    # ------------------------------------------------------------ generation
    co = a.call("POST", "/crm/companies", {
        "name": "АО «Заказчик»", "full_name": "Акционерное общество «Заказчик»", "inn": "7701885820",
        "legal_address": "г. Москва, ул. Строителей, 1", "director": "Петров П. П.", "visibility": "responsible",
        "responsible_id": ids["a"]})["id"]
    k = a.call("POST", "/crm/contacts", {"company_id": co, "last_name": "Сидорова", "first_name": "Мария", "position": "Инженер"})["id"]
    deal = a.call("POST", "/crm/deals", {"title": "Обследование корпуса 3", "company_id": co, "contact_id": k, "amount": 1220000})["deal"]
    nocomp = a.call("POST", "/crm/deals", {"title": "Без контрагента", "amount": 5})["deal"]
    a.call("POST", f"/crm/deals/{nocomp['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"]}, expect=400)
    a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "act", "template_id": tpls["invoice"]["id"]}, expect=400)
    dflt = a.call("GET", f"/crm/deals/{deal['id']}/documents")
    assert dflt["vat_rate"] == "22" and dflt["amount"] == 1220000 and dflt["org_filled"] and dflt["contracts"] == []

    today = dt.date.today().isoformat()
    con = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "contract", "template_id": tpls["contract"]["id"], "date": today,
                                                                "subject": "Техническое обследование корпуса 3"})
    contract = con["document"]
    assert contract["number"] == f"041-ТО/{yy}" and contract["status"] == "draft" and contract["file_name"].startswith(f"Договор № 041-ТО-{yy}")
    inv = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"], "date": today,
                                                                "basis_id": contract["id"], "subject": "Аванс 30% по договору",
                                                                "amount": 366000})["document"]
    assert inv["number"] == "1" and inv["vat_amount"] == 66000.0 and inv["basis_number"] == contract["number"]
    txt = text_of(a.call("GET", f"/files/{inv['file_id']}/download", raw=True))
    for must in ["Счёт на оплату № 1 от", "Акционерное общество «Заказчик»", "ИНН 7701885820", "ИНН 7707083893",
                 "Аванс 30% по договору", "366 000,00", "Триста шестьдесят шесть тысяч рублей 00 копеек",
                 "В том числе НДС 22% — 66 000,00 руб.", f"Договор № {contract['number']} от", "ПАО Сбербанк", "044525225",
                 "40702810938000000001", "Орлов Д. А.", "Иванова Е. П."]:
        assert must in txt, (must, txt[:2000])
    assert "{{" not in txt
    ctxt = text_of(a.call("GET", f"/files/{contract['file_id']}/download", raw=True))
    assert "Один миллион двести двадцать тысяч рублей 00 копеек" in ctxt and "Петров П. П." in ctxt
    inv2 = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"],
                                                                 "vat_rate": "none", "amount": 854000})["document"]
    assert inv2["number"] == "2" and inv2["vat_amount"] == 0
    assert "НДС не облагается" in text_of(a.call("GET", f"/files/{inv2['file_id']}/download", raw=True))
    q = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "quote", "template_id": tpls["quote"]["id"]})["document"]
    assert q["number"] == f"КП-1/{yy}"
    qtxt = text_of(a.call("GET", f"/files/{q['file_id']}/download", raw=True))
    assert "Уважаемый(ая) Сидорова Мария" in qtxt and "Анна" in qtxt
    act = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "act", "template_id": tpls["act"]["id"],
                                                                "basis_id": contract["id"]})["document"]
    atxt = text_of(a.call("GET", f"/files/{act['file_id']}/download", raw=True))
    assert "Акт № 1 от" in atxt and "Вышеперечисленные работы" in atxt and "АО «Заказчик»" in atxt
    a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "act", "template_id": tpls["act"]["id"], "basis_id": inv["id"]}, expect=400)
    step("Generation: contract, invoice with basis and VAT 22%, invoice without VAT, quote, act; amounts in words")

    # Manual numbers, duplicates, cancellation frees the number.
    a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"], "number": "1"}, expect=409)
    a.call("PATCH", f"/crm/documents/{inv2['id']}", {"status": "cancelled"})
    again = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"], "number": "2"})["document"]
    assert again["number"] == "2"
    a.call("PATCH", f"/crm/documents/{inv2['id']}", {"status": "draft"}, expect=409)  # number taken again
    a.call("PATCH", f"/crm/documents/{inv['id']}", {"status": "sent"})
    a.call("PATCH", f"/crm/documents/{inv['id']}", {"status": "paid"}, expect=400)
    nxt = a.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"]})["document"]
    assert nxt["number"] == "3"
    step("Manual numbers, duplicates per kind and year, cancellation frees a number, statuses")

    # ------------------------------------------------------------ lists, history, folders and access
    docs = a.call("GET", f"/crm/documents?deal_id={deal['id']}")["documents"]
    assert len(docs) == 7 and {d["kind"] for d in docs} == {"quote", "contract", "invoice", "act"}
    assert len(a.call("GET", f"/crm/documents?company_id={co}&kind=invoice")["documents"]) == 4
    hist = [x["body"] for x in a.call("GET", f"/crm/deals/{deal['id']}")["activities"]]
    assert any(h.startswith("Сформирован документ: Счёт № 1 от") for h in hist), hist
    # B does not see the company: no documents, no folder, no files.
    assert b.call("GET", "/crm/documents")["documents"] == []
    b.call("POST", f"/crm/companies/{co}/folder", expect=404)
    folder = a.call("POST", f"/crm/companies/{co}/folder")["folder_id"]
    listing = a.call("GET", f"/folders/{folder}")
    names = sorted(f["name"] for f in listing["files"])
    assert len(names) == 7 and any(x.startswith("Счёт № 1 от") for x in names), names
    for c in (b, n):
        c.call("GET", f"/folders/{folder}", expect=NO_ACCESS)
        c.call("GET", f"/files/{inv['file_id']}/download", expect=NO_ACCESS, raw=True)
    # Opening the company to everyone gives B edit access to the folder.
    a.call("PUT", f"/crm/companies/{co}", {"name": "АО «Заказчик»", "full_name": "Акционерное общество «Заказчик»", "inn": "7701885820",
                                            "legal_address": "г. Москва, ул. Строителей, 1", "director": "Петров П. П.",
                                            "visibility": "all", "responsible_id": ids["a"], "tags": []})
    assert len(b.call("GET", f"/folders/{folder}")["files"]) == 7
    assert len(b.call("GET", "/crm/documents")["documents"]) == 7
    b.call("PATCH", f"/crm/documents/{inv['id']}", {"status": "signed"}, expect=403)  # not responsible
    n.call("GET", "/crm/documents", expect=403)
    step("Documents list by deal/company/kind; history entry; company folder follows CRM visibility")
    print("\nALL SALES DOCUMENT TESTS PASSED")


if __name__ == "__main__":
    main()
