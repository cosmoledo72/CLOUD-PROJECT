#!/usr/bin/env python3
"""End-to-end test of the admin panel API: overview, audit log, portal
settings, account recovery, groups, branding (including image validation)
and project requisites.

Usage:
    API=http://localhost:8080 python3 scripts/admin_test.py
"""

import os
import sys
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import API, Client, step  # noqa: E402

PNG_1PX = bytes.fromhex(
    "89504e470d0a1a0a0000000d4948445200000001000000010806000000"
    "1f15c4890000000d49444154789c6360f8cfc0f01f0005000201e5273de40000000049454e44ae426082"
)


def upload_asset(c, kind, name, content, expect=200):
    boundary = uuid.uuid4().hex
    body = (
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\n"
        f"Content-Type: application/octet-stream\r\n\r\n"
    ).encode() + content + f"\r\n--{boundary}--\r\n".encode()
    return c.call("POST", f"/admin/branding/asset?kind={kind}", body, expect=expect,
                  headers={"Content-Type": f"multipart/form-data; boundary={boundary}"})


def main():
    slug = "a" + uuid.uuid4().hex[:8]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "Admin Org", "email": "owner@example.com",
                                        "display_name": "Owner", "password": "owner-password-1"})
    owner.call("POST", "/users", {"email": "admin@example.com", "display_name": "Admin", "password": "admin-password-1", "role": "admin"})
    mid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Member", "password": "member-password-1"})["id"]
    admin, member = Client(), Client()
    admin.call("POST", "/auth/login", {"tenant": slug, "email": "admin@example.com", "password": "admin-password-1"})
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "member-password-1"})

    # --- Access control -----------------------------------------------------
    for path in ["/admin/overview", "/admin/audit", "/admin/portal"]:
        member.call("GET", path, expect=403)
    member.call("PUT", "/admin/branding", {"display_name": "", "accent_color": "#000000", "sidebar_style": "light",
                                           "color_scheme": "auto", "corner_style": "default", "login_title": "",
                                           "login_subtitle": ""}, expect=403)
    member.call("POST", "/admin/project-fields/preset", expect=403)
    step("admin endpoints are forbidden for members")

    # --- Overview & audit -----------------------------------------------------
    ov = admin.call("GET", "/admin/overview")
    assert ov["stats"]["users_total"] == 3 and ov["stats"]["users_active"] == 3
    assert ov["stats"]["storage_quota"] > 0 and isinstance(ov["recent"], list)
    log = admin.call("GET", "/admin/audit?limit=2")
    assert len(log["entries"]) == 2 and log["next_before"]
    page2 = admin.call("GET", f"/admin/audit?limit=2&before={log['next_before']}")
    assert all(e["id"] < log["next_before"] for e in page2["entries"])
    created = admin.call("GET", "/admin/audit?action=user.created")
    assert len(created["entries"]) == 2 and all(e["action"] == "user.created" for e in created["entries"])
    assert created["entries"][0]["target_name"] in ("Admin", "Member")
    step("overview stats, audit pagination and action filter")

    # --- Portal settings ------------------------------------------------------
    admin.call("PATCH", "/admin/portal", {"name": "Партнер-Гарант Инжиниринг"})
    assert admin.call("GET", "/admin/portal")["name"] == "Партнер-Гарант Инжиниринг"
    admin.call("PATCH", "/admin/portal", {"name": "  "}, expect=400)
    step("portal rename")

    # --- Account recovery -------------------------------------------------------
    admin.call("POST", f"/admin/users/{mid}/reset-password", {"password": "new-member-pass-1"})
    member.call("GET", "/auth/me", expect=401)  # sessions revoked
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "new-member-pass-1"})
    owner_id = owner.call("GET", "/auth/me")["id"]
    admin.call("POST", f"/admin/users/{owner_id}/reset-password", {"password": "hijack-pass-123"}, expect=403)
    admin_id = admin.call("GET", "/auth/me")["id"]
    admin.call("POST", f"/admin/users/{admin_id}/reset-mfa", expect=400)
    admin.call("POST", f"/admin/users/{mid}/reset-mfa")
    member.call("GET", "/auth/me", expect=401)  # MFA reset also ends sessions
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "new-member-pass-1"})
    r = admin.call("POST", f"/admin/users/{mid}/revoke-sessions")
    assert r["revoked"] >= 1
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "new-member-pass-1"})
    # Role rules: only the owner appoints or manages administrators; nobody edits their own role.
    admin.call("POST", "/users", {"email": "a2@example.com", "display_name": "A2", "password": "admin2-password", "role": "admin"}, expect=403)
    admin.call("PATCH", f"/users/{mid}", {"role": "admin"}, expect=403)
    admin.call("PATCH", f"/users/{admin_id}", {"role": "member"}, expect=400)
    owner.call("PATCH", f"/users/{admin_id}", {"role": "admin"})
    step("password reset revokes sessions, owner protected, MFA reset, revoke sessions, role rules")

    # --- Groups -----------------------------------------------------------------
    gid = admin.call("POST", "/groups", {"name": "Отдел обследований"})["id"]
    admin.call("PUT", f"/groups/{gid}/members", {"user_ids": [mid]})
    admin.call("PATCH", f"/groups/{gid}", {"name": "Отдел обследований и испытаний"})
    groups = admin.call("GET", "/groups")["groups"]
    assert groups[0]["name"] == "Отдел обследований и испытаний" and groups[0]["member_ids"] == [mid]
    admin.call("DELETE", f"/groups/{gid}")
    assert admin.call("GET", "/groups")["groups"] == []
    step("group rename, members, delete")

    # --- Branding ---------------------------------------------------------------
    anon = Client()
    b = anon.call("GET", f"/public/branding/{slug}")
    assert b["accent_color"] == "#1f4e79" and b["logo_url"] is None
    anon.call("GET", "/public/branding/no-such-portal", expect=404)
    payload = {"display_name": "ПАРТНЕР-ГАРАНТ", "accent_color": "#8B1E2D", "sidebar_style": "dark",
               "color_scheme": "auto", "corner_style": "round", "login_title": "Рабочее пространство",
               "login_subtitle": "Обследование и проектирование", "logo_plate": True}
    b = admin.call("PUT", "/admin/branding", payload)
    assert b["accent_color"] == "#8b1e2d" and b["display_name"] == "ПАРТНЕР-ГАРАНТ" and b["logo_plate"] is True
    admin.call("PUT", "/admin/branding", {**payload, "accent_color": "red"}, expect=400)
    admin.call("PUT", "/admin/branding", {**payload, "sidebar_style": "neon"}, expect=400)

    b = upload_asset(admin, "logo", "logo.png", PNG_1PX)
    assert b["has_logo"] and b["logo_url"].endswith(f"v={b['asset_version']}")
    raw = anon.call("GET", None, url=API.replace("/api", "") + b["logo_url"], raw=True)
    assert raw == PNG_1PX
    clean_svg = b'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 8 8"><rect width="8" height="8" fill="#8b1e2d"/></svg>'
    b = upload_asset(admin, "icon", "icon.svg", clean_svg)
    assert b["has_icon"]
    upload_asset(admin, "icon", "evil.svg", b'<svg onload="alert(1)"></svg>', expect=400)
    upload_asset(admin, "logo", "fake.png", b"GIF89a....", expect=400)
    upload_asset(admin, "logo", "huge.png", PNG_1PX + b"0" * (600 * 1024), expect=413)
    upload_asset(member, "logo", "logo.png", PNG_1PX, expect=403)
    b = admin.call("DELETE", "/admin/branding/asset?kind=icon")
    assert not b["has_icon"]
    anon.call("GET", None, url=API.replace("/api", "") + f"/api/public/branding/{slug}/icon", expect=404)
    assert member.call("GET", "/branding")["sidebar_style"] == "dark"
    step("branding: settings, validation, PNG/SVG upload, malicious SVG rejected, public read, delete")

    # Login page plate and background, portal font.
    pub = anon.call("GET", f"/public/branding/{slug}")
    assert pub["login_logo_plate"] is True and pub["login_bg_overlay"] == 45 and pub["login_bg_url"] is None
    assert pub["font_family"] == "inter" and pub["font_size"] == "default"
    b = admin.call("PUT", "/admin/branding", {**payload, "login_logo_plate": False, "login_bg_overlay": 60,
                                              "font_family": "golos", "font_size": "large"})
    assert b["login_logo_plate"] is False and b["login_bg_overlay"] == 60 and b["font_family"] == "golos" and b["font_size"] == "large"
    admin.call("PUT", "/admin/branding", {**payload, "font_family": "comic_sans"}, expect=400)
    admin.call("PUT", "/admin/branding", {**payload, "font_size": "huge"}, expect=400)
    admin.call("PUT", "/admin/branding", {**payload, "login_bg_overlay": 95}, expect=400)
    # Login panel tone and base colour under a transparent background.
    assert pub["login_bg_tone"] == "dark" and pub["login_bg_base"] == "brand"
    b = admin.call("PUT", "/admin/branding", {**payload, "login_bg_tone": "light", "login_bg_base": "white"})
    assert (b["login_bg_tone"], b["login_bg_base"]) == ("light", "white")
    b = admin.call("PUT", "/admin/branding", {**payload, "login_bg_tone": "light", "login_bg_base": "#E8EEF6"})
    assert b["login_bg_base"] == "#e8eef6"
    assert anon.call("GET", f"/public/branding/{slug}")["login_bg_tone"] == "light"
    admin.call("PUT", "/admin/branding", {**payload, "login_bg_tone": "grey"}, expect=400)
    admin.call("PUT", "/admin/branding", {**payload, "login_bg_base": "blue"}, expect=400)
    b = admin.call("PUT", "/admin/branding", {**payload, "login_logo_plate": False, "login_bg_overlay": 60,
                                              "font_family": "golos", "font_size": "large"})
    assert (b["login_bg_tone"], b["login_bg_base"]) == ("dark", "brand"), "omitted fields fall back to the defaults"
    # Background photos may be up to 3 MB, logos stay at 512 KB.
    big = PNG_1PX + b"0" * (2 * 1024 * 1024)
    b = upload_asset(admin, "login_bg", "bg.png", big)
    assert b["has_login_bg"] and b["login_bg_url"].endswith(f"v={b['asset_version']}")
    assert anon.call("GET", None, url=API.replace("/api", "") + b["login_bg_url"], raw=True) == big
    upload_asset(admin, "login_bg", "huge.png", PNG_1PX + b"0" * (3 * 1024 * 1024 + 1), expect=413)
    pub = anon.call("GET", f"/public/branding/{slug}")
    assert pub["login_bg_url"] and pub["login_logo_plate"] is False and pub["font_family"] == "golos"
    assert not admin.call("DELETE", "/admin/branding/asset?kind=login_bg")["has_login_bg"]
    # Settings saved without the new fields keep working (older clients).
    assert admin.call("PUT", "/admin/branding", payload)["login_logo_plate"] is True
    step("branding: login logo plate, login background (3 MB), overlay, portal font and size")

    # --- Project requisites --------------------------------------------------------
    r = admin.call("POST", "/admin/project-fields/preset")
    assert r["created"] == 15
    assert admin.call("POST", "/admin/project-fields/preset")["created"] == 0  # idempotent
    defs = member.call("GET", "/project-fields")["fields"]
    keys = [d["key"] for d in defs]
    assert keys[0] == "code" and "chief_engineer" in keys
    admin.call("POST", "/admin/project-fields", {"key": "code", "label": "Dup", "field_type": "text"}, expect=409)
    admin.call("POST", "/admin/project-fields", {"key": "Bad Key", "label": "X", "field_type": "text"}, expect=400)
    admin.call("POST", "/admin/project-fields", {"key": "sel", "label": "X", "field_type": "select"}, expect=400)
    fid = admin.call("POST", "/admin/project-fields", {"key": "risk", "label": "Категория риска", "field_type": "select",
                                                       "options": ["Низкая", "Средняя", "Высокая"], "section": "Прочее"})["id"]

    # Required: code, object_name, client.
    admin.call("POST", "/projects", {"name": "Без реквизитов"}, expect=400)
    p = admin.call("POST", "/projects", {"name": "Корпус 12", "fields": {
        "code": "125-24-ТО", "object_name": "Административное здание", "client": "ООО «Заказчик»",
        "contract_date": "2026-10-01", "contract_amount": "1 250 000,50", "stage": "Обследование",
        "chief_engineer": mid, "expertise_required": True, "risk": "Средняя"}})
    pid = p["id"]
    proj = admin.call("GET", f"/projects/{pid}")["project"]
    f = proj["fields"]
    assert f["code"] == "125-24-ТО" and f["contract_amount"] == 1250000.5 and f["chief_engineer"] == mid
    assert f["expertise_required"] is True
    listed = admin.call("GET", "/projects")["projects"][0]
    assert listed["fields"]["code"] == "125-24-ТО"

    admin.call("PUT", f"/projects/{pid}/fields", {"values": {"stage": "Неизвестная"}}, expect=400)
    admin.call("PUT", f"/projects/{pid}/fields", {"values": {"contract_date": "01.10.2026"}}, expect=400)
    admin.call("PUT", f"/projects/{pid}/fields", {"values": {"chief_engineer": str(uuid.uuid4())}}, expect=400)
    admin.call("PUT", f"/projects/{pid}/fields", {"values": {"nope": 1}}, expect=400)
    admin.call("PUT", f"/projects/{pid}/fields", {"values": {"client": ""}}, expect=400)  # required
    v = admin.call("PUT", f"/projects/{pid}/fields", {"values": {"stage": "П", "risk": None}})["values"]
    assert v["stage"] == "П" and "risk" not in v

    # Project members (non-managers) cannot edit requisites.
    admin.call("POST", f"/projects/{pid}/members", {"user_id": mid, "role": "member"})
    member.call("PUT", f"/projects/{pid}/fields", {"values": {"stage": "РД"}}, expect=403)

    # Type change is blocked once values exist; archive keeps values but hides the field.
    admin.call("PUT", f"/admin/project-fields/{fid}", {"key": "risk", "label": "Риск", "field_type": "text"})  # no values now
    stage = next(d for d in defs if d["key"] == "stage")
    admin.call("PUT", f"/admin/project-fields/{stage['id']}", {**stage, "field_type": "text", "options": []}, expect=400)
    admin.call("PUT", f"/admin/project-fields/{stage['id']}", {**stage, "options": stage["options"], "is_archived": True})
    assert "stage" not in [d["key"] for d in member.call("GET", "/project-fields")["fields"]]
    assert admin.call("GET", f"/projects/{pid}")["project"]["fields"]["stage"] == "П"

    ids = [d["id"] for d in admin.call("GET", "/project-fields")["fields"]]
    admin.call("POST", "/admin/project-fields/reorder", {"ids": list(reversed(ids))})
    assert admin.call("GET", "/project-fields")["fields"][0]["id"] == ids[-1]
    admin.call("DELETE", f"/admin/project-fields/{fid}")
    step("requisites: preset, validation by type, required check, list values, permissions, archive, reorder")

    # Tenant isolation for branding assets and fields.
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": slug + "z", "tenant_name": "Other", "email": "o@example.com",
                                        "display_name": "O", "password": "other-password-1"})
    assert other.call("GET", "/project-fields")["fields"] == []
    other.call("PUT", f"/admin/project-fields/{stage['id']}", {**stage, "options": stage["options"]}, expect=404)
    other.call("PUT", f"/projects/{pid}/fields", {"values": {}}, expect=404)
    step("tenant isolation of requisites")
    print("ALL ADMIN TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
