#!/usr/bin/env python3
"""End-to-end test of self-registration in a portal and of invitations
(several addresses, groups, projects, personal message, revoke).

Uses the in-process SMTP sink from mail_test.py, so the backend must run with
SMTP_ALLOW_PRIVATE=true and PUBLIC_URL set.

Usage:
    API=http://localhost:8080 python3 scripts/registration_test.py
"""

import os
import re
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402
from mail_test import INBOX, link, start_sink, text_of, wait_mail  # noqa: E402


def confirm_token(msg):
    m = re.search(r"https?://\S+/register/confirm/([0-9a-f]{64})", text_of(msg))
    assert m, text_of(msg)
    return m.group(1)


def no_mail_to(addr, wait=1.5):
    time.sleep(wait)
    assert not [m for r, m in INBOX if addr in r], [m["subject"] for r, m in INBOX if addr in r]


def main():
    slug = "reg" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро «Регистрация»", "email": "o@example.com",
                                        "display_name": "Ольга Владелец", "password": "owner-password-1"})
    admin_id = owner.call("POST", "/users", {"email": "adm@example.com", "display_name": "Антон Админ",
                                             "password": "admin-password-1", "role": "admin"})["id"]
    adm = Client()
    adm.call("POST", "/auth/login", {"tenant": slug, "email": "adm@example.com", "password": "admin-password-1"})
    member_id = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Марк",
                                              "password": "m-password-123", "role": "member"})["id"]
    m = Client()
    m.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "m-password-123"})
    anon = Client()

    # ------------------------------------------------------------ settings
    s = owner.call("GET", "/admin/registration")
    assert s["mode"] == "request" and s["role"] == "member" and s["domains"] == [] and s["pending"] == 0
    assert anon.call("GET", f"/public/branding/{slug}")["registration_open"] is True
    m.call("GET", "/admin/registration", expect=403)
    m.call("PUT", "/admin/registration", {"mode": "off"}, expect=403)
    owner.call("PUT", "/admin/registration", {"mode": "domain", "domains": []}, expect=400)
    owner.call("PUT", "/admin/registration", {"mode": "domain", "domains": ["localhost"]}, expect=400)
    owner.call("PUT", "/admin/registration", {"mode": "nope"}, expect=400)
    owner.call("PUT", "/admin/registration", {"mode": "request", "role": "admin"}, expect=400)
    owner.call("PUT", "/admin/registration", {"mode": "request", "group_ids": [str(uuid.uuid4())]}, expect=400)
    owner.call("PUT", "/admin/registration", {"mode": "off"})
    assert anon.call("GET", f"/public/branding/{slug}")["registration_open"] is False
    anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Кто-то", "email": "x@example.com",
                                           "password": "x-password-123"}, expect=400)
    step("Registration settings: request mode by default, admins only, validated; closed portal refuses")

    # ------------------------------------------------------------ requests without mail
    owner.call("PUT", "/admin/registration", {"mode": "request"})
    r = anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Пётр  Без Почты", "email": "petr@example.com",
                                               "password": "petr-password-1", "comment": "Новый инженер отдела"})
    mail_configured = r["next"] == "confirm_email"
    if not mail_configured:
        reqs = owner.call("GET", "/admin/registration/requests")["requests"]
        assert [(x["email"], x["display_name"], x["status"], x["comment"]) for x in reqs] == \
            [("petr@example.com", "Пётр Без Почты", "pending", "Новый инженер отдела")]
        anon.call("POST", "/auth/login", {"tenant": slug, "email": "petr@example.com", "password": "petr-password-1"}, expect=401)
        owner.call("POST", f"/admin/registration/requests/{reqs[0]['id']}/reject", {"reason": "Дубль"})
        step("Without mail an application goes straight to the administrators")

    # ------------------------------------------------------------ mail on
    port = start_sink()
    cfg = {"enabled": True, "host": "127.0.0.1", "port": port, "security": "none", "username": "",
           "from_email": "portal@example.com", "from_name": "Бюро", "reply_to": ""}
    owner.call("PUT", "/admin/mail", cfg)
    group = owner.call("POST", "/groups", {"name": "Инженеры"})
    gid = group.get("id") or group["group"]["id"]
    owner.call("PUT", "/admin/registration", {"mode": "request", "role": "guest", "group_ids": [gid]})

    # Validation and the honeypot.
    bad = {"tenant": slug, "display_name": "Ира", "email": "ira@example.com", "password": "ira-password-1"}
    anon.call("POST", "/public/register", {**bad, "email": "not-an-email"}, expect=400)
    anon.call("POST", "/public/register", {**bad, "password": "short"}, expect=400)
    anon.call("POST", "/public/register", {**bad, "display_name": "  "}, expect=400)
    anon.call("POST", "/public/register", {**bad, "tenant": "no-such-portal-xyz"}, expect=400)
    r = anon.call("POST", "/public/register", {**bad, "email": "bot@example.com", "website": "http://spam"})
    assert r == {"ok": True, "next": "confirm_email"}
    no_mail_to("bot@example.com", 0.5)
    step("Form validated; honeypot answered like a real application but ignored")

    # A real application: confirm the address, then the admins are told.
    r = anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Ирина Котова", "email": "ira@example.com",
                                               "password": "ira-password-1", "comment": "ГИП, отдел обследований"})
    assert r == {"ok": True, "next": "confirm_email"}
    msg = wait_mail("ira@example.com", "Подтвердите адрес")
    token = confirm_token(msg)
    reqs = owner.call("GET", "/admin/registration/requests")["requests"]
    assert reqs == [], "an unconfirmed application is not shown as pending"
    all_reqs = owner.call("GET", "/admin/registration/requests?status=all")["requests"]
    assert any(x["email"] == "ira@example.com" and x["status"] == "unconfirmed" for x in all_reqs)
    assert not [n for n in owner.call("GET", "/notifications")["items"]
                if n["kind"] == "registration_request" and "Ирина" in n["title"]]

    # Applying again replaces the open application (and the old link).
    anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Ирина Котова", "email": "IRA@example.com",
                                           "password": "ira-password-2"})
    token2 = confirm_token(wait_mail("IRA@example.com", "Подтвердите адрес"))
    anon.call("POST", "/public/register/confirm", {"token": token}, expect=400)
    c = anon.call("POST", "/public/register/confirm", {"token": token2})
    assert c["status"] == "pending" and c["tenant_slug"] == slug
    anon.call("POST", "/public/register/confirm", {"token": token2}, expect=400)
    anon.call("POST", "/auth/login", {"tenant": slug, "email": "ira@example.com", "password": "ira-password-2"}, expect=401)
    reqs = owner.call("GET", "/admin/registration/requests")["requests"]
    assert len(reqs) == 1 and reqs[0]["email_confirmed"] and reqs[0]["status"] == "pending"
    rid = reqs[0]["id"]
    assert owner.call("GET", "/admin/registration")["pending"] == 1
    for c_ in (owner, adm):
        notes = [n for n in c_.call("GET", "/notifications")["items"]
                 if n["kind"] == "registration_request" and "Ирина Котова" in n["title"]]
        assert len(notes) == 1, notes
    assert not [n for n in m.call("GET", "/notifications")["items"] if n["kind"] == "registration_request"]
    assert "registration_request" not in {p["kind"] for p in m.call("GET", "/notifications/prefs")["prefs"]}
    assert "registration_request" in {p["kind"] for p in adm.call("GET", "/notifications/prefs")["prefs"]}
    wait_mail("o@example.com", "Заявка на регистрацию")
    step("Address confirmed by link; re-applying replaces the link; administrators notified in the portal and by mail")

    # Approve with the defaults (guest + group).
    m.call("POST", f"/admin/registration/requests/{rid}/approve", {}, expect=403)
    a = adm.call("POST", f"/admin/registration/requests/{rid}/approve", {})
    assert a["emailed"]
    adm.call("POST", f"/admin/registration/requests/{rid}/approve", {}, expect=400)
    msg = wait_mail("IRA@example.com", "Заявка одобрена")
    assert f"/login?portal={slug}" in text_of(msg)
    ira = Client()
    me = ira.call("POST", "/auth/login", {"tenant": slug, "email": "ira@example.com", "password": "ira-password-2"})["user"]
    assert me["role"] == "guest" and me["display_name"] == "Ирина Котова"
    members = owner.call("GET", "/groups")
    groups = members.get("groups", members)
    g = next(x for x in groups if x["id"] == gid)
    assert a["user_id"] in [str(u) if isinstance(u, str) else u["id"] for u in g.get("member_ids", g.get("members", []))], g
    assert owner.call("GET", "/admin/registration")["pending"] == 0
    step("Approved: the account works with the applicant's password, default role and groups; applicant emailed")

    # An existing account: same answer, a hint by mail instead of a new application.
    r = anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Ирина", "email": "ira@example.com",
                                               "password": "whatever-123"})
    assert r == {"ok": True, "next": "confirm_email"}
    assert "уже есть учётная запись" in text_of(wait_mail("ira@example.com", "уже зарегистрированы"))
    step("Registering an existing address answers the same and only emails a hint")

    # Reject with a reason.
    anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Спамер", "email": "spam@example.com",
                                           "password": "spam-password-1"})
    anon.call("POST", "/public/register/confirm", {"token": confirm_token(wait_mail("spam@example.com", "Подтвердите"))})
    rid = owner.call("GET", "/admin/registration/requests")["requests"][0]["id"]
    owner.call("POST", f"/admin/registration/requests/{rid}/reject", {"reason": "Не сотрудник бюро"})
    assert "Не сотрудник бюро" in text_of(wait_mail("spam@example.com", "Заявка на регистрацию"))
    hist = owner.call("GET", "/admin/registration/requests?status=all")["requests"]
    rej = next(x for x in hist if x["email"] == "spam@example.com")
    assert rej["status"] == "rejected" and rej["decided_by_name"] == "Ольга Владелец"
    anon.call("POST", "/auth/login", {"tenant": slug, "email": "spam@example.com", "password": "spam-password-1"}, expect=401)
    step("Rejected: the applicant is told the reason; the decision stays in the history")

    # ------------------------------------------------------------ domain mode
    owner.call("PUT", "/admin/registration", {"mode": "domain", "domains": ["@Partner-Garant.RU"], "role": "member", "group_ids": []})
    assert owner.call("GET", "/admin/registration")["domains"] == ["partner-garant.ru"]
    anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Сергей Свой", "email": "sergey@partner-garant.ru",
                                           "password": "sergey-password-1"})
    c = anon.call("POST", "/public/register/confirm", {"token": confirm_token(wait_mail("sergey@partner-garant.ru", "Подтвердите"))})
    assert c["status"] == "active" and c["email"] == "sergey@partner-garant.ru"
    Client().call("POST", "/auth/login", {"tenant": slug, "email": "sergey@partner-garant.ru", "password": "sergey-password-1"})
    anon.call("POST", "/public/register", {"tenant": slug, "display_name": "Чужой", "email": "other@gmail.example",
                                           "password": "other-password-1"})
    c = anon.call("POST", "/public/register/confirm", {"token": confirm_token(wait_mail("other@gmail.example", "Подтвердите"))})
    assert c["status"] == "pending"
    reqs = owner.call("GET", "/admin/registration/requests")["requests"]
    assert [x["email"] for x in reqs] == ["other@gmail.example"] and not reqs[0]["domain_match"]
    rows = owner.call("GET", "/admin/audit?action=registration.joined")["entries"]
    assert rows, "domain join audited"
    step("Domain mode: a confirmed address of the domain joins at once, others become requests")

    # ------------------------------------------------------------ invitations
    p1 = owner.call("POST", "/projects", {"name": "Корпус 7"})["id"]
    p2 = owner.call("POST", "/projects", {"name": "Причал 3"})["id"]
    m.call("POST", "/admin/invitations", {"emails": "a@example.com"}, expect=403)
    adm.call("POST", "/admin/invitations", {"emails": "a@example.com", "role": "admin"}, expect=403)
    owner.call("POST", "/admin/invitations", {"emails": "a@example.com", "projects": [{"project_id": str(uuid.uuid4())}]}, expect=400)
    owner.call("POST", "/admin/invitations", {"emails": "a@example.com", "projects": [{"project_id": p1, "role": "boss"}]}, expect=400)
    owner.call("POST", "/admin/invitations", {"emails": "  "}, expect=400)
    r = owner.call("POST", "/admin/invitations", {
        "emails": "Анна Смирнова <anna@example.com>\nboris.ivanov@example.com, m@example.com; ANNA@example.com\nнеадрес",
        "role": "member", "group_ids": [gid],
        "projects": [{"project_id": p1, "role": "member"}, {"project_id": p2, "role": "viewer"}],
        "message": "Подключайтесь к проектам по Холмску",
    })["results"]
    st = {x.get("email", x.get("input")): x["status"] for x in r}
    assert st == {"anna@example.com": "invited", "boris.ivanov@example.com": "invited", "m@example.com": "exists",
                  "неадрес": "invalid"}, st
    names = {x["email"]: x["display_name"] for x in r if x["status"] == "invited"}
    assert names == {"anna@example.com": "Анна Смирнова", "boris.ivanov@example.com": "Boris Ivanov"}
    msg = wait_mail("anna@example.com", "Приглашение")
    body = text_of(msg)
    assert "Подключайтесь к проектам по Холмску" in body and "«Корпус 7»" in body and "«Причал 3»" in body, body
    anna_token = link(msg, "invite")
    wait_mail("boris.ivanov@example.com", "Приглашение")
    again = owner.call("POST", "/admin/invitations", {"emails": "anna@example.com"})["results"]
    assert again[0]["status"] == "already_invited"
    step("Invitations: several addresses with names, existing and invalid ones reported, message and projects in the letter")

    inv = owner.call("GET", "/admin/invitations")["invitations"]
    anna = next(x for x in inv if x["email"] == "anna@example.com")
    assert anna["invited_by_name"] == "Ольга Владелец" and anna["message"] == "Подключайтесь к проектам по Холмску"
    assert {p["name"]: p["role"] for p in anna["projects"]} == {"Корпус 7": "member", "Причал 3": "viewer"}
    assert anna["groups"] == ["Инженеры"] and not anna["expired"] and anna["expires_at"]
    m.call("GET", "/admin/invitations", expect=403)
    info = anon.call("POST", "/public/invite/check", {"token": anna_token})
    assert info["invited_by_name"] == "Ольга Владелец" and sorted(info["projects"]) == ["Корпус 7", "Причал 3"]
    assert info["message"] == "Подключайтесь к проектам по Холмску"
    anon.call("POST", "/public/invite/accept", {"token": anna_token, "password": "anna-password-1"})
    anna_c = Client()
    anna_c.call("POST", "/auth/login", {"tenant": slug, "email": "anna@example.com", "password": "anna-password-1"})
    mine = {p["name"] for p in anna_c.call("GET", "/projects")["projects"]}
    assert mine == {"Корпус 7", "Причал 3"}, mine
    assert "anna@example.com" not in [x["email"] for x in owner.call("GET", "/admin/invitations")["invitations"]]
    step("Invitation list shows who, when, groups and projects; accepted invitee lands in the projects")

    # Revoke.
    boris = next(x for x in owner.call("GET", "/admin/invitations")["invitations"] if x["email"] == "boris.ivanov@example.com")
    r2 = owner.call("POST", f"/admin/users/{boris['id']}/invite")
    boris_token = link(wait_mail("boris.ivanov@example.com", "Приглашение"), "invite")
    assert boris_token in r2["invite_url"]
    m.call("DELETE", f"/admin/invitations/{boris['id']}", expect=403)
    rv = owner.call("DELETE", f"/admin/invitations/{boris['id']}")
    assert rv["removed"] is True
    owner.call("DELETE", f"/admin/invitations/{boris['id']}", expect=400)
    anon.call("POST", "/public/invite/check", {"token": boris_token}, expect=400)
    assert "boris.ivanov@example.com" not in [u["email"] for u in owner.call("GET", "/users")["users"]]
    assert not [x for x in owner.call("GET", "/admin/invitations")["invitations"] if x["email"] == "boris.ivanov@example.com"]
    # The address can be invited again afterwards.
    again = owner.call("POST", "/admin/invitations", {"emails": "boris.ivanov@example.com"})["results"]
    assert again[0]["status"] == "invited"
    step("Revoking an invitation removes the pending account and kills the link; the address can be invited again")

    # Single-user invitation from the old form still records who invited.
    one = owner.call("POST", "/users", {"email": "solo@example.com", "display_name": "Соло"})
    solo = next(x for x in owner.call("GET", "/admin/invitations")["invitations"] if x["id"] == one["id"])
    assert solo["invited_by_name"] == "Ольга Владелец"
    rows = owner.call("GET", "/admin/audit?action=user.invite_revoked")["entries"]
    assert rows
    step("Old single invitation keeps working and appears in the list; revocations audited")

    print("ALL REGISTRATION TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
