#!/usr/bin/env python3
"""End-to-end test of finance (F4, F5): finance access role, categories,
payments (planned and actual) with links to a project, counterparty and
invoice, invoice payment state, analytics, receivables, the money of a
project for its manager, the money columns of the project list and the
module switch.

Usage:
    API=http://localhost:8080 python3 scripts/finance_test.py
"""

import datetime as dt
import os
import sys
import urllib.parse
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "fin" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    clients = {}
    for key, email, name in [("acc", "acc@example.com", "Бухгалтер Вера"), ("view", "v@example.com", "Директор Виктор"),
                             ("gip", "gip@example.com", "ГИП Глеб"), ("m", "m@example.com", "Марк")]:
        ids[key] = owner.call("POST", "/users", {"email": email, "display_name": name, "password": "password-12345", "role": "member"})["id"]
    owner.call("PATCH", f"/users/{ids['acc']}", {"finance_role": "edit", "crm_role": "user"})
    owner.call("PATCH", f"/users/{ids['view']}", {"finance_role": "view"})
    owner.call("PATCH", f"/users/{ids['m']}", {"finance_role": "boss"}, expect=400)
    for key, email in [("acc", "acc@example.com"), ("view", "v@example.com"), ("gip", "gip@example.com"), ("m", "m@example.com")]:
        c = Client()
        c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": "password-12345"})
        clients[key] = c
    acc, view, gip, m = clients["acc"], clients["view"], clients["gip"], clients["m"]
    assert owner.call("GET", "/auth/me")["finance_role"] == "edit"
    assert acc.call("GET", "/auth/me")["finance_role"] == "edit" and view.call("GET", "/auth/me")["finance_role"] == "view"
    assert m.call("GET", "/auth/me")["finance_role"] == "none" and "finance" in m.call("GET", "/auth/me")["modules"]
    step("Finance role: none / view / edit per user, administrators always edit")

    # ------------------------------------------------------------ categories
    cats = acc.call("GET", "/finance/categories")["categories"]
    names = {(c["kind"], c["name"]) for c in cats}
    assert ("expense", "Субподряд") in names and ("income", "Оплата по договору") in names and len(cats) == 9
    cat = {c["name"]: c["id"] for c in cats}
    view.call("POST", "/finance/categories", {"kind": "expense", "name": "Геодезия"}, expect=403)
    geo = acc.call("POST", "/finance/categories", {"kind": "expense", "name": "Геодезия"})["id"]
    acc.call("POST", "/finance/categories", {"kind": "expense", "name": "геодезия"}, expect=409)
    acc.call("PATCH", f"/finance/categories/{cat['Налоги']}", {"is_archived": True})
    assert next(c for c in acc.call("GET", "/finance/categories")["categories"] if c["id"] == cat["Налоги"])["is_archived"]
    step("Categories: default set created on first use, added and archived by finance editors")

    # ------------------------------------------------------------ project with a CRM deal and an invoice
    owner.call("PUT", "/admin/organization", {"full_name": "ООО «Бюро»", "inn": "7707083893", "vat_rate": "20",
                                               "bik": "044525225", "account": "40702810938000000001", "corr_account": "30101810400000000225"})
    owner.call("POST", "/admin/templates/sales-samples")
    tpls = {t["doc_kind"]: t for t in owner.call("GET", "/templates")["templates"] if t["scope"] == "sales"}
    co = owner.call("POST", "/crm/companies", {"name": "АО «Заказчик»", "inn": "7701885820"})["id"]
    deal = owner.call("POST", "/crm/deals", {"title": "Обследование моста", "company_id": co, "amount": 1000000})["deal"]
    pid = owner.call("POST", f"/crm/deals/{deal['id']}/project", {})["project_id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["gip"], "role": "manager"})
    owner.call("POST", f"/projects/{pid}/members", {"user_id": ids["m"], "role": "member"})
    other = owner.call("POST", "/projects", {"name": "Чужой проект"})["id"]
    # The portal's own date (its time zone), not this machine's: they differ around midnight.
    today = dt.date.fromisoformat(owner.call("GET", "/crm/tasks")["today"])
    old = (today - dt.timedelta(days=30)).isoformat()
    inv = owner.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"],
                                                                   "date": old, "amount": 300000, "subject": "Аванс 30%"})["document"]
    inv2 = owner.call("POST", f"/crm/deals/{deal['id']}/documents", {"kind": "invoice", "template_id": tpls["invoice"]["id"],
                                                                    "date": today.isoformat(), "amount": 700000})["document"]
    step("Project from a deal; two invoices (one 30 days old)")

    # ------------------------------------------------------------ payments
    base = {"kind": "income", "pay_date": today.isoformat(), "amount": 100000}
    m.call("POST", "/finance/payments", base, expect=403)
    view.call("POST", "/finance/payments", base, expect=403)
    acc.call("POST", "/finance/payments", {**base, "amount": 0}, expect=400)
    acc.call("POST", "/finance/payments", {**base, "kind": "gift"}, expect=400)
    acc.call("POST", "/finance/payments", {**base, "category_id": cat["Субподряд"]}, expect=400)  # expense category on income
    acc.call("POST", "/finance/payments", {**base, "project_id": str(uuid.uuid4())}, expect=400)
    acc.call("POST", "/finance/payments", {**base, "kind": "expense", "invoice_id": inv["id"]}, expect=400)
    # Paying the invoice fills in the counterparty and the project.
    p1 = acc.call("POST", "/finance/payments", {**base, "amount": 120000, "invoice_id": inv["id"], "category_id": cat["Оплата по договору"],
                                                 "vat_rate": "20", "description": "Аванс, п/п 15"})["payment"]
    assert p1["company_id"] == co and p1["project_id"] == pid and p1["invoice_number"] == inv["number"] and p1["vat_amount"] == 20000.0
    # Payment schedule: the rest of the advance is planned (overdue) and the final payment is planned ahead.
    plan_late = acc.call("POST", "/finance/payments", {"kind": "income", "status": "plan", "pay_date": (today - dt.timedelta(days=3)).isoformat(),
                                                       "amount": 180000, "project_id": pid, "description": "Остаток аванса"})["payment"]
    acc.call("POST", "/finance/payments", {"kind": "income", "status": "plan", "pay_date": (today + dt.timedelta(days=20)).isoformat(),
                                           "amount": 700000, "project_id": pid, "invoice_id": inv2["id"]})
    acc.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 250000, "project_id": pid,
                                           "category_id": cat["Субподряд"], "company_name": "ООО «Геолаб»"})
    acc.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 50000, "category_id": cat["Офис"]})
    step("Payments validated; an invoice payment takes the counterparty and project; planned schedule stages")

    # The project manager adds an expense of their own project, but nothing else.
    e = gip.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 30000, "project_id": pid,
                                                "category_id": geo, "description": "Геодезическая съёмка"})["payment"]
    gip.call("POST", "/finance/payments", {"kind": "income", "pay_date": today.isoformat(), "amount": 1, "project_id": pid}, expect=403)
    gip.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 1, "project_id": other}, expect=403)
    gip.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 1}, expect=403)
    gip.call("PUT", f"/finance/payments/{e['id']}", {"kind": "expense", "pay_date": today.isoformat(), "amount": 35000, "project_id": pid,
                                                     "category_id": geo, "description": "Геодезическая съёмка"})
    gip.call("PUT", f"/finance/payments/{e['id']}", {"kind": "expense", "pay_date": today.isoformat(), "amount": 35000, "project_id": other}, expect=403)
    gip.call("DELETE", f"/finance/payments/{p1['id']}", expect=403)
    m.call("POST", "/finance/payments", {"kind": "expense", "pay_date": today.isoformat(), "amount": 1, "project_id": pid}, expect=403)
    gip.call("GET", "/finance/payments", expect=403)
    assert len(gip.call("GET", f"/finance/payments?project_id={pid}")["payments"]) == 5
    m.call("GET", f"/finance/payments?project_id={pid}", expect=403)
    step("Project manager records and edits expenses of their project only; sees its payments, not the rest")

    # ------------------------------------------------------------ lists and totals
    r = view.call("GET", f"/finance/payments?from={old}&to={(today + dt.timedelta(days=60)).isoformat()}")
    t = r["totals"]
    assert t == {"income": 120000.0, "expense": 335000.0, "income_plan": 880000.0, "expense_plan": 0.0}, t
    assert len(view.call("GET", "/finance/payments?kind=expense&status=fact")["payments"]) == 3
    assert [p["description"] for p in view.call("GET", "/finance/payments?q=" + urllib.parse.quote("Геодез"))["payments"]] == ["Геодезическая съёмка"]
    assert len(view.call("GET", f"/finance/payments?category_id={cat['Субподряд']}")["payments"]) == 1
    step("Payment list with filters (kind, status, category, text) and totals")

    # ------------------------------------------------------------ invoice state in the documents registry
    docs = {d["id"]: d for d in owner.call("GET", "/crm/documents?kind=invoice")["documents"]}
    assert docs[inv["id"]]["paid_amount"] == 120000.0 and docs[inv2["id"]]["paid_amount"] == 0.0
    step("Documents registry shows how much of each invoice is paid")

    # ------------------------------------------------------------ receivables
    rec = view.call("GET", "/finance/receivables")
    invs = {i["id"]: i for i in rec["invoices"]}
    assert invs[inv["id"]]["state"] == "overdue" and invs[inv["id"]]["debt"] == 180000.0 and invs[inv["id"]]["days_overdue"] == 20
    assert invs[inv2["id"]]["state"] == "issued" and invs[inv2["id"]]["project_id"] == pid
    assert rec["totals"] == {"debt": 880000.0, "overdue": 180000.0, "plan_overdue": 180000.0}, rec["totals"]
    assert rec["companies"][0]["company_name"] == "АО «Заказчик»" and rec["companies"][0]["invoices"] == 2
    assert [p["id"] for p in rec["plan"]][0] == plan_late["id"]
    m.call("GET", "/finance/receivables", expect=403)
    # Paying the rest closes the invoice.
    acc.call("POST", "/finance/payments", {"kind": "income", "pay_date": today.isoformat(), "amount": 180000, "invoice_id": inv["id"]})
    acc.call("DELETE", f"/finance/payments/{plan_late['id']}")
    rec = view.call("GET", "/finance/receivables")
    assert inv["id"] not in {i["id"] for i in rec["invoices"]} and rec["totals"]["overdue"] == 0.0
    owner.call("PUT", "/finance/settings", {"invoice_due_days": 5})
    view.call("PUT", "/finance/settings", {"invoice_due_days": 5}, expect=403)
    owner.call("PUT", "/finance/settings", {"invoice_due_days": 400}, expect=400)
    assert view.call("GET", "/finance/settings")["invoice_due_days"] == 5
    step("Receivables: debt per invoice and counterparty, overdue by the payment term; paid invoices drop out")

    # ------------------------------------------------------------ analytics
    s = view.call("GET", f"/finance/summary?from={today.replace(day=1).isoformat()}&to={today.isoformat()}")
    assert s["totals"]["income"] == 300000.0 and s["totals"]["expense"] == 335000.0 and s["totals"]["profit"] == -35000.0
    assert s["series"][-1]["income"] == 300000.0
    cats_sum = {c["name"]: c["amount"] for c in s["categories"] if c["kind"] == "expense"}
    assert cats_sum == {"Субподряд": 250000.0, "Офис": 50000.0, "Геодезия": 35000.0}, cats_sum
    assert s["projects"][0]["id"] == pid and s["projects"][0]["profit"] == 15000.0
    assert s["plan"]["next_30_days"] == 700000.0
    w = view.call("GET", f"/finance/summary?from={(today - dt.timedelta(days=13)).isoformat()}&to={today.isoformat()}&period=week")
    assert len(w["series"]) in (2, 3)
    gip.call("GET", f"/finance/summary?from={today.isoformat()}&to={today.isoformat()}", expect=403)
    step("Analytics: income, expense, profit; series by month and week; by category; projects by profit; planned income")

    # ------------------------------------------------------------ project money (F5)
    f = gip.call("GET", f"/projects/{pid}/finance")
    assert f["budget"] == 1000000.0 and f["budget_source"] == "deal" and f["paid"] == 300000.0 and f["due"] == 700000.0
    assert f["spent"] == 285000.0 and f["profit"] == 715000.0 and f["can_add_expense"] and not f["can_edit"]
    assert [i["state"] for i in f["invoices"]] == ["paid", "issued"]
    m.call("GET", f"/projects/{pid}/finance", expect=403)
    gip.call("GET", f"/projects/{other}/finance", expect=403)
    # A contract amount requisite wins over the deal amount.
    owner.call("POST", "/admin/project-fields", {"label": "Стоимость работ, руб.", "key": "contract_amount", "field_type": "number"})
    owner.call("PUT", f"/projects/{pid}/fields", {"values": {"contract_amount": 1200000}})
    f = view.call("GET", f"/projects/{pid}/finance")
    assert f["budget"] == 1200000.0 and f["budget_source"] == "field" and f["budget_label"] == "Стоимость работ, руб."
    money = {p["project_id"]: p for p in view.call("GET", "/finance/projects")["projects"]}
    assert money[pid]["budget"] == 1200000.0 and money[pid]["paid"] == 300000.0 and money[pid]["due"] == 900000.0
    assert money[other]["budget"] is None
    assert [p["project_id"] for p in gip.call("GET", "/finance/projects")["projects"]] == [pid]
    assert m.call("GET", "/finance/projects")["projects"] == []
    step("Project money: budget from the contract requisite or the deal, paid, due, spent, profit; list columns by access")

    # ------------------------------------------------------------ module switch
    owner.call("PUT", "/admin/modules", {"disabled": ["finance"]})
    assert acc.call("GET", "/auth/me")["finance_role"] == "none" and "finance" not in acc.call("GET", "/auth/me")["modules"]
    r = acc.call("GET", "/finance/payments", expect=403)
    assert r["error"] == "module_disabled"
    gip.call("GET", f"/projects/{pid}/finance", expect=403)
    owner.call("PUT", "/admin/modules", {"disabled": []})
    assert len(acc.call("GET", "/finance/payments")["payments"]) == 6
    rows = owner.call("GET", "/admin/audit?action=finance.payment_deleted")["entries"]
    assert rows
    step("Finance module switches off and on; data kept; payment changes audited")

    print("ALL FINANCE TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
