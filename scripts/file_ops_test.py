#!/usr/bin/env python3
"""End-to-end test of the file list operations: storage usage, folder item
counts and owners, copying files and folders («(копия)» names, nested
structure, rights, no copying a folder into itself) and ZIP download of a
selection (structure, repeated names, rights, audit).

Usage:
    API=http://localhost:8080 python3 scripts/file_ops_test.py
"""

import io
import os
import sys
import uuid
import zipfile

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402


def main():
    slug = "fo" + uuid.uuid4().hex[:6]
    owner = Client()
    owner.call("POST", "/auth/signup", {"tenant_slug": slug, "tenant_name": "ПАРТНЕР-ГАРАНТ", "email": "o@example.com",
                                        "display_name": "Ольга Смирнова", "password": "owner-password-1"})
    ids, c = {}, {}
    for key, name in (("eng", "Егор Лаптев"), ("out", "Олег Чужой")):
        ids[key] = owner.call("POST", "/users", {"email": f"{key}@example.com", "display_name": name, "password": "user-password-123", "role": "member"})["id"]
        c[key] = Client()
        c[key].call("POST", "/auth/login", {"tenant": slug, "email": f"{key}@example.com", "password": "user-password-123"})
    eng, out = c["eng"], c["out"]

    # --- Usage and folder listing ------------------------------------------------------
    u0 = eng.call("GET", "/storage/usage")
    assert u0["used"] == 0 and u0["quota"] > 0, u0
    my = eng.call("GET", "/folders/roots")["my"]
    docs = eng.call("POST", "/folders", {"parent_id": my, "name": "Обследование"})["id"]
    photos = eng.call("POST", "/folders", {"parent_id": docs, "name": "Фото"})["id"]
    eng.upload(docs, "Акт.txt", "Акт осмотра".encode())
    eng.upload(docs, "Отчёт.txt", b"x" * 1000)
    eng.upload(photos, "Акт.txt", "Фото к акту".encode())
    eng.upload(photos, "IMG_01.jpg", b"\xff\xd8\xff" + b"0" * 500)
    assert eng.call("GET", "/storage/usage")["used"] == len("Акт осмотра".encode()) + 1000 + len("Фото к акту".encode()) + 503
    sub = eng.call("GET", f"/folders/{my}")["folders"]
    d = next(f for f in sub if f["id"] == docs)
    assert d["items"] == 3 and d["owner_name"] == "Егор Лаптев", d
    step("Storage usage for any employee; subfolders carry owner and item count")

    # --- Copy -----------------------------------------------------------------------------
    files = {f["name"]: f["id"] for f in eng.call("GET", f"/folders/{docs}")["files"]}
    r = eng.call("POST", "/files/copy", {"files": [files["Акт.txt"]], "target": docs})
    names = sorted(f["name"] for f in eng.call("GET", f"/folders/{docs}")["files"])
    assert names == ["Акт (копия).txt", "Акт.txt", "Отчёт.txt"], names
    eng.call("POST", "/files/copy", {"files": [files["Акт.txt"]], "target": docs})
    assert "Акт (копия 2).txt" in [f["name"] for f in eng.call("GET", f"/folders/{docs}")["files"]]
    copy_id = r["ids"][0]
    assert eng.call("GET", f"/files/{copy_id}/download", raw=True) == "Акт осмотра".encode()
    # A folder with everything inside.
    archive = eng.call("POST", "/folders", {"parent_id": my, "name": "Архив"})["id"]
    eng.call("POST", "/files/copy", {"folders": [docs], "target": archive})
    a = eng.call("GET", f"/folders/{archive}")["folders"]
    assert [x["name"] for x in a] == ["Обследование"] and a[0]["items"] == 5, a
    inner = eng.call("GET", f"/folders/{a[0]['id']}")
    assert [x["name"] for x in inner["folders"]] == ["Фото"]
    assert sorted(x["name"] for x in eng.call("GET", f"/folders/{inner['folders'][0]['id']}")["files"]) == ["IMG_01.jpg", "Акт.txt"]
    # Not into itself or below itself; nothing selected; no rights.
    eng.call("POST", "/files/copy", {"folders": [docs], "target": photos}, expect=400)
    eng.call("POST", "/files/copy", {"folders": [docs], "target": docs}, expect=400)
    eng.call("POST", "/files/copy", {"target": docs}, expect=400)
    out_my = out.call("GET", "/folders/roots")["my"]
    out.call("POST", "/files/copy", {"files": [files["Отчёт.txt"]], "target": out_my}, expect=404)
    eng.call("POST", "/files/copy", {"files": [files["Отчёт.txt"]], "target": out_my}, expect=404)
    # Read access through a share is enough to copy into one's own folder.
    eng.call("POST", "/shares", {"resource_type": "file", "resource_id": files["Отчёт.txt"], "user_id": ids["out"], "access": 1})
    out.call("POST", "/files/copy", {"files": [files["Отчёт.txt"]], "target": out_my})
    got = out.call("GET", f"/folders/{out_my}")["files"]
    assert [f["name"] for f in got] == ["Отчёт.txt"] and got[0]["owner_name"] == "Олег Чужой"
    step("Copy: «(копия)», «(копия 2)» names, folders with contents, not into itself, needs read + edit on the target")

    # --- ZIP ------------------------------------------------------------------------------
    z = zipfile.ZipFile(io.BytesIO(eng.call("POST", "/files/zip", {"folders": [docs]}, raw=True)))
    names = sorted(z.namelist())
    assert "Обследование/Фото/IMG_01.jpg" in names and "Обследование/Акт.txt" in names and "Обследование/Фото/" in names, names
    assert z.read("Обследование/Фото/Акт.txt") == "Фото к акту".encode()
    # Files with the same name from different places get a number.
    photo_akt = next(f["id"] for f in eng.call("GET", f"/folders/{photos}")["files"] if f["name"] == "Акт.txt")
    z = zipfile.ZipFile(io.BytesIO(eng.call("POST", "/files/zip", {"files": [files["Акт.txt"], photo_akt]}, raw=True)))
    assert sorted(z.namelist()) == ["Акт (2).txt", "Акт.txt"], z.namelist()
    eng.call("POST", "/files/zip", {}, expect=400)
    out.call("POST", "/files/zip", {"folders": [docs]}, expect=404)
    out.call("POST", "/files/zip", {"files": [files["Акт.txt"]]}, expect=404)
    assert out.call("POST", "/files/zip", {"files": [files["Отчёт.txt"]]}, raw=True)[:2] == b"PK", "shared file"
    audit = owner.call("GET", "/admin/audit?action=file.zip_downloaded")["entries"]
    assert len(audit) == 3, len(audit)
    assert owner.call("GET", "/admin/audit?action=file.copied")["entries"]
    step("ZIP: folder structure, numbered repeated names, rights, audit")

    print("ALL FILE OPERATION TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
