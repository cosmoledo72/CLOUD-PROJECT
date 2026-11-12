#!/usr/bin/env python3
"""End-to-end test of CRM reports (B11): key figures, funnel by the furthest
stage reached, won and lost by month, managers with money received and owed
(only for those who see finance), reasons for lost deals, drill-down to deals,
scope (managers see all, others only their deals), Excel export and lost
reason suggestions.

Usage:
    API=http://localhost:8080 python3 scripts/crm_reports_test.py
Requires: openpyxl.
"""

import datetime as dt
import io
import os
import sys
import uuid
from urllib.parse import urlencode

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def login(slug, key):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": f"{key}-password-123"})
    return c


def main():
    slug = "rep" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга", "password": "owner-password-1"})
    ids = {}
    for key, name in [("a", "Анна"), ("b", "Борис"), ("m", "Менеджер"), ("n", "Без CRM")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": f"{key}-password-123"})["id"]
    owner.call("PATCH", f"/users/{ids['a']}", {"crm_role": "user"})
    owner.call("PATCH", f"/users/{ids['b']}", {"crm_role": "user"})
    owner.call("PATCH", f"/users/{ids['m']}", {"crm_role": "manager"})
    owner.call("PATCH", f"/users/{ids['n']}", {"crm_role": "none"})
    a, b, m, n = (login(slug, k) for k in "abmn")
    stages = owner.call("GET", "/crm/stages")["stages"]
    st = {s["name"]: s["id"] for s in stages}
    req, kp, tender, agree, won, lost = (st["Запрос"], st["Коммерческое предложение"], st["Тендер"],
                                         st["Согласование договора"], st["Выиграна"], st["Проиграна"])

    co = owner.call("POST", "/crm/companies", {"name": "АО «Заказчик»", "inn": "7701885820"})["id"]

    def deal(c, title, amount):
        return c.call("POST", "/crm/deals", {"title": title, "amount": amount, "company_id": co})["deal"]["id"]

    def move(c, d, stage, reason=None):
        c.call("PATCH", f"/crm/deals/{d}", {"stage_id": stage, **({"lost_reason": reason} if reason else {})})

    d1 = deal(a, "Обследование моста", 100000)
    for s in (kp, tender, won):
        move(a, d1, s)
    d2 = deal(a, "Проект усиления", 200000)
    move(a, d2, kp)
    move(a, d2, lost, "Цена выше конкурентов")
    deal(a, "Экспертиза", 50000)
    d4 = deal(b, "Обмерные работы", 300000)
    move(b, d4, lost, "  цена выше конкурентов ")
    d5 = deal(b, "Изыскания", 400000)
    move(b, d5, kp)
    step("Deals of two people moved through the pipeline (won, lost with reasons, open)")

    # Money: an invoice on the won deal, partly paid.
    owner.call("PUT", "/admin/organization", {"full_name": "ООО «Бюро»", "inn": "7707083893", "vat_rate": "20",
                                               "bik": "044525225", "account": "40702810938000000001", "corr_account": "30101810400000000225"})
    owner.call("POST", "/admin/templates/sales-samples")
    tpl = next(t for t in owner.call("GET", "/templates")["templates"] if t["scope"] == "sales" and t["doc_kind"] == "invoice")
    today = dt.date.fromisoformat(owner.call("GET", "/crm/tasks")["today"])
    inv = owner.call("POST", f"/crm/deals/{d1}/documents", {"kind": "invoice", "template_id": tpl["id"], "date": today.isoformat(),
                                                            "amount": 100000})["document"]
    owner.call("POST", "/finance/payments", {"kind": "income", "pay_date": today.isoformat(), "amount": 60000, "invoice_id": inv["id"]})
    step("Invoice on the won deal, 60 000 paid")

    frm, to = (today - dt.timedelta(days=30)).isoformat(), today.isoformat()
    q = lambda **kw: urlencode({"from": frm, "to": to, **kw})  # noqa: E731
    r = owner.call("GET", "/crm/reports?" + q())
    k = r["kpi"]
    assert (k["new"], k["won"], k["won_amount"], k["lost"], k["lost_amount"]) == (5, 1, 100000.0, 2, 500000.0), k
    assert k["win_rate"] == 33.3 and k["avg_check"] == 100000.0 and k["new_prev"] == 0 and k["win_rate_prev"] is None
    assert (k["open"], k["open_amount"], k["weighted"]) == (2, 450000.0, 125000.0), k  # 50 000 × 10 % + 400 000 × 30 %
    assert k["cycle_days"] == 0
    step("Key figures: new, won, lost, conversion, average check, cycle, weighted pipeline")

    funnel = [(s["name"], s["count"], s["amount"]) for s in r["funnel"]]
    assert funnel == [("Запрос", 5, 1050000.0), ("Коммерческое предложение", 3, 700000.0), ("Тендер", 1, 100000.0),
                      ("Согласование договора", 1, 100000.0), ("Выиграна", 1, 100000.0)], funnel
    assert r["funnel"][-1]["kind"] == "won"
    step("Funnel: a deal counts up to the furthest stage it reached; a won deal counts on every stage")

    month = today.strftime("%Y-%m")
    mon = {x["month"]: x for x in r["months"]}
    assert len(r["months"]) >= 6 and r["months"][-1]["month"] == month
    assert (mon[month]["won_amount"], mon[month]["lost"], mon[month]["lost_amount"]) == (100000.0, 2, 500000.0)
    step("Won and lost by month (at least six months, ending with the period)")

    mg = {x["name"]: x for x in r["managers"]}
    assert (mg["Анна"]["new"], mg["Анна"]["won"], mg["Анна"]["lost"], mg["Анна"]["win_rate"], mg["Анна"]["open"]) == (3, 1, 1, 50.0, 1)
    assert (mg["Борис"]["new"], mg["Борис"]["won"], mg["Борис"]["win_rate"], mg["Борис"]["open"]) == (2, 0, 0.0, 1)
    assert r["money"] is True and mg["Анна"]["received"] == 60000.0 and mg["Анна"]["debt"] == 40000.0 and mg["Борис"]["debt"] == 0.0
    assert r["totals"]["received"] == 60000.0 and r["totals"]["new"] == 5
    reasons = r["reasons"]
    assert reasons[0]["count"] == 2 and reasons[0]["reason"] == "Цена выше конкурентов" and reasons[0]["amount"] == 500000.0, reasons
    step("Managers with money received and owed; reasons grouped regardless of case and spaces")

    # A CRM manager without finance rights sees no money.
    rm = m.call("GET", "/crm/reports?" + q())
    assert rm["money"] is False and "received" not in rm["managers"][0] and rm["can_filter"] is True
    rb = m.call("GET", "/crm/reports?" + q(responsible_id=ids["b"]))
    assert rb["kpi"]["new"] == 2 and [x["name"] for x in rb["managers"]] == ["Борис"]
    assert m.call("GET", "/crm/reports?" + q(responsible_id="none"))["kpi"]["new"] == 0
    # A CRM user sees only own deals; the filter is ignored.
    ra = a.call("GET", "/crm/reports?" + q(responsible_id=ids["b"]))
    assert ra["kpi"]["new"] == 3 and ra["can_filter"] is False and [x["name"] for x in ra["managers"]] == ["Анна"]
    n.call("GET", "/crm/reports?" + q(), expect=403)
    owner.call("GET", "/crm/reports?" + urlencode({"from": to, "to": frm}), expect=400)
    step("Scope: managers see all and filter by person; users see their own deals; no CRM — 403")

    dd = owner.call("GET", "/crm/reports/deals?" + q(set="funnel", stage_id=kp))
    assert dd["count"] == 3 and {x["title"] for x in dd["deals"]} == {"Обследование моста", "Проект усиления", "Изыскания"}
    assert dd["deals"][0]["title"] == "Изыскания"  # largest first
    assert owner.call("GET", "/crm/reports/deals?" + q(set="funnel", stage_id=won))["count"] == 1
    dr = owner.call("GET", "/crm/reports/deals?" + q(set="reason", reason="цена выше конкурентов"))
    assert dr["count"] == 2 and dr["amount"] == 500000.0
    assert owner.call("GET", "/crm/reports/deals?" + q(set="new", manager=ids["a"]))["count"] == 3
    assert owner.call("GET", "/crm/reports/deals?" + q(set="open"))["count"] == 2
    assert a.call("GET", "/crm/reports/deals?" + q(set="lost"))["count"] == 1
    owner.call("GET", "/crm/reports/deals?" + q(set="bogus"), expect=400)
    step("Drill-down: the deals behind funnel steps, reasons, a manager, open deals")

    sugg = b.call("GET", "/crm/lost-reasons")["reasons"]
    assert len(sugg) == 1 and sugg[0].lower() == "цена выше конкурентов"
    step("Lost reason suggestions")

    blob = owner.call("GET", "/crm/reports.xlsx?" + q(), raw=True)
    import openpyxl
    wb = openpyxl.load_workbook(io.BytesIO(blob))
    assert wb.sheetnames == ["Сводка", "Воронка", "По месяцам", "Менеджеры", "Причины отказов", "Сделки"], wb.sheetnames
    assert wb["Сводка"]["B3"].value == 5 and wb["Воронка"]["B2"].value == 5 and wb["Воронка"]["C3"].value == 60
    assert wb["Менеджеры"]["H1"].value == "Поступило" and wb["Сделки"].max_row == 6
    assert [c.value for c in wb["Менеджеры"][wb["Менеджеры"].max_row]][0] == "Итого"
    blob = m.call("GET", "/crm/reports.xlsx?" + q(), raw=True)
    assert openpyxl.load_workbook(io.BytesIO(blob))["Менеджеры"]["H1"].value is None
    step("Excel export: summary, funnel, months, managers (money only with finance rights), reasons, deals")

    print("ALL CRM REPORT TESTS PASSED")


if __name__ == "__main__":
    main()
