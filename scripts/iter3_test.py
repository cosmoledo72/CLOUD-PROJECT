#!/usr/bin/env python3
"""End-to-end test of document templates, trash and login rate limiting.

Usage:
    API=http://localhost:8080 python3 scripts/iter3_test.py
"""

import io
import json
import os
import sys
import urllib.error
import urllib.request
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import API, Client, step  # noqa: E402


def multipart(fields, file_name, content):
    boundary = uuid.uuid4().hex
    parts = []
    for k, v in fields.items():
        parts.append(f"--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n".encode())
    parts.append(
        f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n"
        f"Content-Type: application/octet-stream\r\n\r\n".encode() + content + b"\r\n"
    )
    parts.append(f"--{boundary}--\r\n".encode())
    return b"".join(parts), {"Content-Type": f"multipart/form-data; boundary={boundary}"}


def make_docx(body_xml, footer_xml=None):
    """Minimal DOCX whose document.xml body is `body_xml`."""
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", "<Types/>")
        z.writestr("word/document.xml", f'<w:document xmlns:w="w"><w:body>{body_xml}</w:body></w:document>')
        if footer_xml:
            z.writestr("word/footer1.xml", f'<w:ftr xmlns:w="w">{footer_xml}</w:ftr>')
    return buf.getvalue()


def make_xlsx(shared_strings_xml):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", "<Types/>")
        z.writestr("xl/workbook.xml", "<workbook/>")
        z.writestr("xl/sharedStrings.xml", f"<sst>{shared_strings_xml}</sst>")
    return buf.getvalue()


def part(doc_bytes, name):
    with zipfile.ZipFile(io.BytesIO(doc_bytes)) as z:
        return z.read(name).decode()


def main():
    slug = "t3" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ООО «Инж-Групп»", "email": "o@example.com",
                                        "display_name": "Дмитрий", "password": "owner-password-1"})
    mid = owner.call("POST", "/users", {"email": "m@example.com", "display_name": "Игорь Петров", "password": "member-password-1"})["id"]
    member = Client()
    member.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "member-password-1"})

    # ------------------------------------------------------------------ templates
    owner.call("POST", "/admin/project-fields/preset")
    sample = owner.call("POST", "/admin/templates/sample")["id"]
    tpls = owner.call("GET", "/templates")["templates"]
    assert tpls[0]["id"] == sample and "project.code" in tpls[0]["placeholders"] and "year" in tpls[0]["placeholders"]
    owner.call("POST", "/admin/templates/sample", expect=409)  # same name

    # Custom DOCX with a placeholder split across three runs, a multi-line value and an unknown key.
    docx = make_docx(
        '<w:p><w:r><w:t>Шифр {{</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>project.</w:t></w:r><w:r><w:t>code}}</w:t></w:r></w:p>'
        '<w:p><w:r><w:t>{{project.object_address}}</w:t></w:r></w:p>'
        '<w:p><w:r><w:t>{{date_long}}, {{project.contract_amount}} руб., ГИП {{project.chief_engineer}}, {{project.unknown_key}}</w:t></w:r></w:p>',
        '<w:p><w:r><w:t>{{portal.name}} © {{year}}</w:t></w:r></w:p>',
    )
    body, hdr = multipart({"name": "Акт", "category": "Акты", "description": "Тест"}, "act.docx", docx)
    act = owner.call("POST", "/admin/templates", body, headers=hdr)["id"]
    body, hdr = multipart({}, "x.docx", docx)
    member.call("POST", "/admin/templates", body, headers=hdr, expect=403)
    body, hdr = multipart({}, "bad.docx", b"not a zip")
    owner.call("POST", "/admin/templates", body, headers=hdr, expect=400)
    body, hdr = multipart({}, "notes.txt", b"{{project.code}}")
    owner.call("POST", "/admin/templates", body, headers=hdr, expect=400)
    xlsx = make_xlsx('<si><t>{{project.code}}</t></si><si><r><t>Заказчик: {{project.cl</t></r><r><t>ient}}</t></r></si>')
    body, hdr = multipart({"name": "Смета"}, "smeta.xlsx", xlsx)
    smeta = owner.call("POST", "/admin/templates", body, headers=hdr)["id"]
    cat = owner.call("GET", "/templates/placeholders")["placeholders"]
    assert any(p["key"] == "project.chief_engineer" and p["label"] == "ГИП" for p in cat)
    step("templates: sample install, upload DOCX/XLSX, placeholder scan, validation, admin-only")

    p = owner.call("POST", "/projects", {"name": "Корпус 12", "fields": {
        "code": "125-24-ТО", "object_name": "Здание", "client": "ООО «Заказчик» & партнёры",
        "object_address": "г. Москва\nул. Ленина, 1", "contract_amount": 1250000.5, "chief_engineer": mid}})
    pid = p["id"]
    owner.call("POST", f"/projects/{pid}/members", {"user_id": mid, "role": "member"})

    chk = member.call("GET", f"/projects/{pid}/templates/{act}/check?today=2026-11-10")["missing"]
    keys = {m["key"]: m for m in chk}
    assert set(keys) == {"project.unknown_key"} and keys["project.unknown_key"]["known"] is False

    g = member.call("POST", f"/projects/{pid}/generate", {"template_id": act, "today": "2026-11-10"})
    assert g["name"] == "Акт 125-24-ТО.docx" and g["missing"] == ["project.unknown_key"]
    out = member.call("GET", f"/files/{g['file_id']}/download", raw=True)
    doc = part(out, "word/document.xml")
    assert "Шифр 125-24-ТО" in doc, doc
    assert "<w:b/></w:rPr><w:t xml:space=\"preserve\"></w:t>" in doc  # middle run emptied, formatting kept
    assert "г. Москва</w:t><w:br/>" in doc
    assert "10 ноября 2026 г., 1 250 000,5 руб., ГИП Игорь Петров, " in doc, doc
    assert "{{" not in doc
    assert "ООО «Инж-Групп» © 2026" in part(out, "word/footer1.xml")
    # Markers without the "project." prefix and requisites of the chosen person.
    member.call("PUT", f"/users/{mid}/profile", {"last_name": "Петров", "first_name": "Игорь", "middle_name": "Олегович",
                                                "doc_position": "Главный инженер проекта", "nrs_number": "П-777"})
    alias_docx = make_docx('<w:p><w:r><w:t>[{{code}}|{{project_code}}|{{chief_engineer}}|{{project_chief_engineer}}|'
                           '{{project.chief_engineer.short_name}}|{{chief_engineer.position}}|{{project_chief_engineer.nrs}}|{{date}}|{{gip}}]</w:t></w:r></w:p>')
    body, hdr = multipart({"name": "Алиасы"}, "alias.docx", alias_docx)
    alias = owner.call("POST", "/admin/templates", body, headers=hdr)["id"]
    cat_keys = {p["key"] for p in owner.call("GET", "/templates/placeholders")["placeholders"]}
    assert {"project.chief_engineer.short_name", "project.chief_engineer.qual_cert"} <= cat_keys
    chk = member.call("GET", f"/projects/{pid}/templates/{alias}/check?today=2026-11-10")["missing"]
    assert [(m["key"], m["known"]) for m in chk] == [("gip", False)], chk
    ga = member.call("POST", f"/projects/{pid}/generate", {"template_id": alias, "today": "2026-11-10"})
    doc = part(member.call("GET", f"/files/{ga['file_id']}/download", raw=True), "word/document.xml")
    assert "[125-24-ТО|125-24-ТО|Игорь Петров|Игорь Петров|Петров И. О.|Главный инженер проекта|П-777|10.11.2026|]" in doc, doc
    owner.call("DELETE", f"/admin/templates/{alias}")
    step("templates: {{key}} and {{project_key}} mean {{project.key}}; person requisites {{project.<key>.short_name}} etc.")

    g2 = member.call("POST", f"/projects/{pid}/generate", {"template_id": act})
    assert g2["name"] == "Акт 125-24-ТО (2).docx"
    gx = owner.call("POST", f"/projects/{pid}/generate", {"template_id": smeta, "name": "Смета/v1"})
    sst = part(owner.call("GET", f"/files/{gx['file_id']}/download", raw=True), "xl/sharedStrings.xml")
    assert ">125-24-ТО<" in sst and "Заказчик: ООО «Заказчик» &amp; партнёры" in sst, sst
    assert gx["name"] == "Смета-v1.xlsx"
    gs = owner.call("POST", f"/projects/{pid}/generate", {"template_id": sample})
    title = part(owner.call("GET", f"/files/{gs['file_id']}/download", raw=True), "word/document.xml")
    assert "125-24-ТО" in title and "ООО «Инж-Групп»" in title and "{{" not in title

    outsider = Client()
    outsider.call("POST", "/auth/signup", {"tenant_slug": slug + "x", "tenant_name": "X", "email": "x@example.com",
                                           "display_name": "X", "password": "other-password-1"})
    outsider.call("POST", f"/projects/{pid}/generate", {"template_id": act}, expect=404)
    outsider.call("GET", f"/templates/{act}/download", expect=404)

    body, hdr = multipart({}, "act2.docx", make_docx('<w:p><w:r><w:t>{{project.stage}}</w:t></w:r></w:p>'))
    r = owner.call("POST", f"/admin/templates/{act}/file", body, headers=hdr)
    assert r["placeholders"] == ["project.stage"]
    owner.call("PUT", f"/admin/templates/{act}", {"name": "Акт осмотра", "category": "Акты", "description": ""})
    owner.call("DELETE", f"/admin/templates/{smeta}")
    assert [t["name"] for t in owner.call("GET", "/templates")["templates"]] == ["Акт осмотра", "Титульный лист отчёта (пример)"]
    step("generation: split runs, line breaks, ru number/date, footer, unique names, XLSX, isolation, replace/rename/delete")

    # ---------------------------------------------------------------------- trash
    folder_id = p["folder_id"]
    member.call("DELETE", f"/files/{g2['file_id']}")
    owner.call("DELETE", f"/files/{gx['file_id']}")
    mine = member.call("GET", "/trash")
    assert [i["id"] for i in mine["items"]] == [g2["file_id"]] and mine["retention_days"] == 30
    assert len(owner.call("GET", "/trash")["items"]) == 2  # admin sees everything
    member.call("POST", f"/trash/file/{gx['file_id']}/restore", expect=404)  # not theirs

    # Restore with a name clash.
    member.call("POST", f"/projects/{pid}/generate", {"template_id": act, "name": "Акт 125-24-ТО (2)"})
    r = member.call("POST", f"/trash/file/{g2['file_id']}/restore")
    assert r["name"] == "Акт 125-24-ТО (2) (восстановлено).docx", r
    member.call("GET", f"/files/{g2['file_id']}")

    # Folder with content: delete, file inside is hidden, purge removes everything.
    sub = owner.call("POST", "/folders", {"parent_id": folder_id, "name": "Фото"})["id"]
    body, hdr = multipart({}, "img.txt", b"data")
    inner = owner.call("POST", f"/folders/{sub}/upload", body, headers=hdr)["ids"][0]
    owner.call("POST", "/shares", {"resource_type": "folder", "resource_id": sub, "user_id": mid, "access": 1})
    owner.call("DELETE", f"/folders/{sub}")
    owner.call("GET", f"/files/{inner}", expect=404)
    items = owner.call("GET", "/trash")["items"]
    assert any(i["id"] == sub for i in items) and not any(i["id"] == inner for i in items)
    owner.call("DELETE", f"/trash/folder/{sub}")
    assert not any(i["id"] == sub for i in owner.call("GET", "/trash")["items"])
    owner.call("POST", f"/trash/folder/{sub}/restore", expect=404)

    # Deleted project folder is listed but not restorable.
    p2 = owner.call("POST", "/projects", {"name": "Временный", "fields": {"code": "X", "object_name": "Y", "client": "Z"}})
    owner.call("DELETE", f"/projects/{p2['id']}")
    pf = next(i for i in owner.call("GET", "/trash")["items"] if i["id"] == p2["folder_id"])
    assert pf["restorable"] is False
    owner.call("POST", f"/trash/folder/{p2['folder_id']}/restore", expect=400)
    r = owner.call("POST", "/trash/empty")
    assert r["purged"] >= 2 and owner.call("GET", "/trash")["items"] == []
    step("trash: per-user visibility, restore with rename, folder purge with contents, project folders, empty")

    # --------------------------------------------------------------- rate limit
    rl = Client()
    for _ in range(8):
        rl.call("POST", "/auth/login", {"tenant": slug, "email": "m@example.com", "password": "wrong-password!!"}, expect=401)
    req = urllib.request.Request(API + "/auth/login", method="POST", headers={"Content-Type": "application/json"},
                                 data=json.dumps({"tenant": slug, "email": "m@example.com", "password": "member-password-1"}).encode())
    try:
        urllib.request.urlopen(req)
        raise AssertionError("expected 429")
    except urllib.error.HTTPError as e:
        assert e.code == 429 and int(e.headers["Retry-After"]) > 0, e.code
    # Other accounts are unaffected.
    rl.call("POST", "/auth/login", {"tenant": slug, "email": "o@example.com", "password": "owner-password-1"})
    step("rate limit: account locked after 8 failures (429 + Retry-After), others unaffected")

    # ------------------------------------------------------------------ moving files and folders
    common = owner.call("GET", "/folders/roots")["common"]
    a = owner.call("POST", "/folders", {"parent_id": common, "name": "Объект А"})["id"]
    b = owner.call("POST", "/folders", {"parent_id": a, "name": "Фото"})["id"]
    c = owner.call("POST", "/folders", {"parent_id": common, "name": "Архив"})["id"]
    fid = owner.upload(a, "акт.txt", b"1")["ids"][0]
    owner.call("PATCH", f"/files/{fid}", {"folder_id": b})
    assert [f["id"] for f in owner.call("GET", f"/folders/{b}")["files"]] == [fid]
    owner.upload(a, "акт.txt", b"2")
    owner.call("PATCH", f"/files/{fid}", {"folder_id": a}, expect=409)  # same name already there
    owner.call("PATCH", f"/folders/{b}", {"parent_id": c})
    assert owner.call("GET", f"/folders/{b}")["breadcrumbs"][-2]["id"] == c
    owner.call("PATCH", f"/folders/{c}", {"parent_id": b}, expect=400)  # into its own subfolder
    owner.call("PATCH", f"/folders/{c}", {"parent_id": c}, expect=400)
    owner.call("PATCH", f"/folders/{common}", {"parent_id": a}, expect=400)  # roots stay put
    owner.call("PATCH", f"/folders/{b}", {"name": "Фотофиксация"})
    assert owner.call("GET", f"/folders/{b}")["folder"]["name"] == "Фотофиксация"
    # A member without rights on the folder cannot move it.
    member.call("PATCH", f"/folders/{b}", {"parent_id": a}, expect=403)
    step("move: files and folders between folders, name conflicts, cycles and roots rejected, rights checked")
    print("ALL ITERATION-3 TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print(f"FAILED: {e}")
        sys.exit(1)
