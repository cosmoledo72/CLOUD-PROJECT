#!/usr/bin/env python3
"""End-to-end test of the employee card (iteration 28б): who sees and edits
what, validation (phones, Telegram, dates, manager), requisites for documents
and custom fields, groups, login email, photo and signature, ending sessions,
last visit and author placeholders in templates.

Usage:
    API=http://localhost:8080 python3 scripts/profile_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

PNG = b"\x89PNG\r\n\x1a\n" + b"\x00" * 64
JPG = b"\xff\xd8\xff\xe0" + b"\x00" * 64


def upload(c, path, name, data, expect=200):
    boundary = "----b" + uuid.uuid4().hex
    body = (f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
            f"Content-Type: application/octet-stream\r\n\r\n").encode() + data + f"\r\n--{boundary}--\r\n".encode()
    return c.call("POST", path, body, expect=expect, headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})


def login(slug, email, password):
    c = Client()
    c.call("POST", "/auth/login", {"tenant": slug, "email": email, "password": password})
    return c


def main():
    slug = "prof" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Бюро", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    ids = {}
    for key, name in [("a", "Анна Петрова"), ("b", "Борис Ковалёв"), ("g", "Гость")]:
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": f"{key}-password-123",
                                                 **({"role": "guest"} if key == "g" else {})})["id"]
    a, b, g = (login(slug, f"{k}@example.com", f"{k}-password-123") for k in "abg")
    me = owner.call("GET", "/auth/me")
    oid = me["id"] if "id" in me else me["user"]["id"]

    # --- Own card ---------------------------------------------------------------
    card = a.call("GET", f"/users/{ids['a']}/profile")
    assert card["can_edit"] and not card["can_admin"]
    assert (card["user"]["first_name"], card["user"]["last_name"]) == ("Анна", "Петрова"), "names split by the migration rule"
    r = a.call("PUT", f"/users/{ids['a']}/profile", {
        "middle_name": "Сергеевна", "birth_date": "1988-03-14", "position": "Главный инженер проекта", "department": "Отдел обследований",
        "manager_id": oid, "hired_on": "2019-09-01", "phone": "+7 (4242) 49-00-12, доб. 214", "phone_mobile": "+7 914 123-45-67",
        "telegram": "apetrova", "doc_short_name": "Петрова А. С.", "doc_position": "ГИП", "name_genitive": "Петровой Анны Сергеевны",
        "nrs_number": "П-123456", "qual_cert": "МС-Э-12-2-12345", "power_of_attorney": "№ 14 от 10.01.2026"})
    assert r["telegram"] == "@apetrova" and r["display_name"] == "Анна Петрова"
    step("An employee fills in their own card and requisites")

    for body, msg in [({"phone": "a@example.com"}, "Рабочий телефон"), ({"phone_mobile": "доб. 12"}, "Мобильный"),
                      ({"telegram": "@ab"}, "Telegram"), ({"birth_date": "14.03.1988"}, "Дата рождения"),
                      ({"birth_date": "2999-01-01"}, "Дата рождения"), ({"manager_id": ids["a"]}, "Руководитель"),
                      ({"manager_id": str(uuid.uuid4())}, "Руководитель"), ({"position": "x" * 101}, "Должность")]:
        err = a.call("PUT", f"/users/{ids['a']}/profile", body, expect=400)
        assert msg in str(err), (body, err)
    # The old settings form gives a clear message for an email in the phone field.
    err = a.call("PUT", "/auth/profile", {"display_name": "Анна Петрова", "phone": "a@example.com"}, expect=400)
    assert "не email" in str(err)
    step("Validation: phones (no email), extension, Telegram, dates, manager, lengths")

    # --- Colleague's view -------------------------------------------------------
    cb = b.call("GET", f"/users/{ids['a']}/profile")
    u = cb["user"]
    assert not cb["can_edit"] and u["birthday"] == "14.03" and "birth_date" not in u and "nrs_number" not in u
    assert u["phone_mobile"] == "+7 914 123-45-67" and u["manager_name"] == "Ольга Смирнова" and cb["custom"] == []
    assert "hour_rate" not in u
    b.call("PUT", f"/users/{ids['a']}/profile", {"position": "x"}, expect=403)
    g.call("GET", f"/users/{ids['a']}/profile", expect=403)
    assert g.call("GET", f"/users/{ids['g']}/profile")["can_edit"]
    step("Colleagues see contacts and the birthday (day and month), not requisites; guests see only themselves")

    # --- Custom requisites --------------------------------------------------------
    a.call("PUT", "/admin/user-fields", {"fields": []}, expect=403)
    f = owner.call("PUT", "/admin/user-fields", {"fields": [{"label": "СНИЛС для пропуска", "field_type": "text"},
                                                           {"label": "Удостоверение по охране труда до", "field_type": "date"}]})["fields"]
    owner.call("PUT", "/admin/user-fields", {"fields": [{"label": "А", "field_type": "text"}, {"label": "а", "field_type": "text"}]}, expect=400)
    owner.call("PUT", "/admin/user-fields", {"fields": [{"label": "А", "field_type": "select"}]}, expect=400)
    a.call("PUT", f"/users/{ids['a']}/profile", {"custom": {f[1]["id"]: "31.12.2027"}}, expect=400)
    a.call("PUT", f"/users/{ids['a']}/profile", {"custom": {f[1]["id"]: "2027-12-31", f[0]["id"]: "Пропуск 12"}})
    a.call("PUT", f"/users/{ids['a']}/profile", {"custom": {str(uuid.uuid4()): "x"}}, expect=400)
    custom = {c["label"]: c["value"] for c in owner.call("GET", f"/users/{ids['a']}/profile")["custom"]}
    assert custom == {"СНИЛС для пропуска": "Пропуск 12", "Удостоверение по охране труда до": "2027-12-31"}, custom
    # Rename one, drop the other: its values go too.
    owner.call("PUT", "/admin/user-fields", {"fields": [{"id": f[1]["id"], "label": "Охрана труда до", "field_type": "date"}]})
    custom = a.call("GET", f"/users/{ids['a']}/profile")["custom"]
    assert [(c["label"], c["value"]) for c in custom] == [("Охрана труда до", "2027-12-31")]
    step("Administrator defines custom requisites; values validated, renamed fields keep values, removed fields drop them")

    # --- Administrator: groups, email, sessions -----------------------------------
    gid = owner.call("POST", "/groups", {"name": "ГИПы"})["id"]
    a.call("PUT", f"/users/{ids['a']}/groups", {"group_ids": [gid]}, expect=403)
    owner.call("PUT", f"/users/{ids['a']}/groups", {"group_ids": [gid, gid]})
    assert [x["name"] for x in b.call("GET", f"/users/{ids['a']}/profile")["groups"]] == ["ГИПы"]
    owner.call("PUT", f"/users/{ids['a']}/groups", {"group_ids": [str(uuid.uuid4())]}, expect=400)
    owner.call("PATCH", f"/users/{ids['b']}", {"email": "a@example.com"}, expect=409)
    owner.call("PATCH", f"/users/{ids['b']}", {"email": "not-an-email"}, expect=400)
    owner.call("PATCH", f"/users/{ids['b']}", {"email": "boris@example.com"})
    b2 = login(slug, "boris@example.com", "b-password-123")
    owner.call("POST", f"/admin/users/{ids['b']}/revoke-sessions")
    b2.call("GET", "/auth/me", expect=401)
    b.call("GET", "/auth/me", expect=401)
    b = login(slug, "boris@example.com", "b-password-123")
    a.call("POST", f"/admin/users/{ids['b']}/revoke-sessions", expect=403)
    owner_card = owner.call("GET", f"/users/{ids['a']}/profile")
    assert owner_card["can_admin"] and owner_card["user"]["nrs_number"] == "П-123456" and "hour_rate" in owner_card["user"]
    step("Administrator: groups, login email (unique), ending all sessions; sees requisites and the rate")

    # --- Photo and signature --------------------------------------------------------
    assert upload(a, f"/users/{ids['a']}/avatar", "me.jpg", JPG)["at"]
    upload(a, f"/users/{ids['a']}/avatar", "x.gif", b"GIF89a....", expect=400)
    upload(a, f"/users/{ids['a']}/avatar", "big.png", PNG + b"\x00" * (2 * 1024 * 1024), expect=413)
    upload(b, f"/users/{ids['a']}/avatar", "me.png", PNG, expect=403)
    raw = b.call("GET", f"/users/{ids['a']}/avatar", raw=True)
    assert raw.startswith(b"\xff\xd8")
    av = b.call("GET", "/users/avatars")["avatars"]
    assert list(av) == [ids["a"]]
    upload(a, f"/users/{ids['a']}/signature", "s.jpg", JPG, expect=400)
    upload(a, f"/users/{ids['a']}/signature", "s.png", PNG)
    b.call("GET", f"/users/{ids['a']}/signature", expect=404)
    assert owner.call("GET", f"/users/{ids['a']}/signature", raw=True).startswith(b"\x89PNG")
    a.call("DELETE", f"/users/{ids['a']}/avatar")
    b.call("GET", f"/users/{ids['a']}/avatar", expect=404)
    assert b.call("GET", "/users/avatars")["avatars"] == {}
    step("Photo (PNG/JPEG/WEBP up to 2 MB) for everyone to see; signature PNG only, seen by the person and administrators")

    # --- Team list, last visit, audit, placeholders ---------------------------------
    team = {m["display_name"]: m for m in b.call("GET", "/team")["members"]}
    assert team["Анна Петрова"]["department"] == "Отдел обследований" and team["Анна Петрова"]["last_seen_at"]
    acts = {e["action"] for e in owner.call("GET", "/admin/audit?limit=200")["entries"]}
    assert {"user.groups_changed", "user.sessions_revoked", "user.avatar_changed", "user.signature_changed", "user_fields.saved"} <= acts, acts
    keys = {p["key"] for p in a.call("GET", "/templates/placeholders")["placeholders"]}
    assert {"user.short_name", "user.name_genitive", "user.nrs", "user.position"} <= keys
    step("Team list shows department and last visit; actions audited; author placeholders in templates")

    print("ALL PROFILE TESTS PASSED")


if __name__ == "__main__":
    main()
