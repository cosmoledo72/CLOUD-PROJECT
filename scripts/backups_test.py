#!/usr/bin/env python3
"""End-to-end test of «Администрирование → Резервные копии».

The backup service is simulated with files in the backup folder, so the test
needs the folder the server reads (BACKUP_DIR) on this machine and the server
started with BACKUP_PORTAL=<portal>. With pg_dump available and PG* variables
set, the request is also served by the real scripts/backup.sh.

Usage:
    API=http://localhost:8080 BACKUP_DIR=/path/to/backups BACKUP_PORTAL=bkptest python3 scripts/backups_test.py
"""

import os
import shutil
import subprocess
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import Client, step  # noqa: E402

DIR = os.environ.get("BACKUP_DIR", "")
PORTAL = os.environ.get("BACKUP_PORTAL", "")
PASSWORD = "owner-password-1"


def portal_admin():
    """Sign in to the backup portal (created by the first run of this test)."""
    c = Client()
    status = None
    try:
        c.call("POST", "/auth/signup", {"tenant_slug": PORTAL, "tenant_name": "Системный портал", "email": "o@example.com",
                                        "display_name": "Ольга", "password": PASSWORD})
        status = 200
    except AssertionError:
        c.call("POST", "/auth/login", {"tenant": PORTAL, "email": "o@example.com", "password": PASSWORD})
    return c, status


def make_folder(name, complete=True, db=b"PGDMP-dump", files=b"tar", age=0):
    d = os.path.join(DIR, name)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "db.dump"), "wb") as f:
        f.write(db)
    with open(os.path.join(d, "files.tar.gz"), "wb") as f:
        f.write(files)
    if complete:
        with open(os.path.join(d, "COMPLETE"), "w") as f:
            f.write("created=2026-11-12 03:00:41 Asia/Sakhalin\ndatabase=1 KiB\nfiles=1 KiB\n")
    if age:
        t = time.time() - age
        for p in [d] + [os.path.join(d, x) for x in os.listdir(d)]:
            os.utime(p, (t, t))
    return d


def main():
    if not DIR or not PORTAL or not os.path.isdir(DIR):
        print("SKIPPED: set BACKUP_DIR (an existing folder the server reads) and BACKUP_PORTAL")
        return
    # Clean slate: only test folders and status files.
    for x in os.listdir(DIR):
        p = os.path.join(DIR, x)
        if x.startswith("cloud-project-"):
            shutil.rmtree(p)
        elif x in ("STATUS", "LAST_RESULT", "REQUEST"):
            os.remove(p)

    admin, _ = portal_admin()
    assert admin.call("GET", "/auth/me")["backups"] is True

    # --- Who sees the section -------------------------------------------------
    admin.call("POST", "/users", {"email": "m" + uuid.uuid4().hex[:6] + "@example.com", "display_name": "Марк",
                                  "password": "member-password-1", "role": "member"})
    users = admin.call("GET", "/users")["users"]
    member_email = next(u["email"] for u in users if u["role"] == "member")
    member = Client()
    member.call("POST", "/auth/login", {"tenant": PORTAL, "email": member_email, "password": "member-password-1"})
    assert member.call("GET", "/auth/me")["backups"] is False
    member.call("GET", "/admin/backups", expect=404)
    other = Client()
    other.call("POST", "/auth/signup", {"tenant_slug": "bk" + uuid.uuid4().hex[:6], "tenant_name": "Другой", "email": "x@example.com",
                                        "display_name": "Икс", "password": PASSWORD})
    assert other.call("GET", "/auth/me")["backups"] is False
    other.call("GET", "/admin/backups", expect=404)
    other.call("POST", "/admin/backups/now", expect=404)
    step("Only administrators of the BACKUP_PORTAL portal see the section; others get 404")

    # --- Empty folder, service silent -----------------------------------------
    r = admin.call("GET", "/admin/backups")
    assert r["available"] and not r["alive"] and r["backups"] == [] and r["requested_at"] is None
    assert any("не отвечает" in w for w in r["warnings"]) and any("Готовых резервных копий нет" in w for w in r["warnings"])

    # --- Service alive, backups of different states ---------------------------
    with open(os.path.join(DIR, "STATUS"), "w") as f:
        f.write(f"alive={int(time.time())}\n")
    make_folder("cloud-project-2026-11-10_0300", age=2 * 86400)
    make_folder("cloud-project-2026-11-11_0300", db=b"D" * 2048, files=b"F" * 4096, age=3600)
    make_folder("cloud-project-2026-11-11_1200", complete=False)                      # being written
    make_folder("cloud-project-2026-11-09_0300", complete=False, age=2 * 86400)       # abandoned
    os.makedirs(os.path.join(DIR, "not-a-backup"), exist_ok=True)
    r = admin.call("GET", "/admin/backups")
    assert r["alive"] and r["hour"] == 3 and r["keep_days"] == 14
    states = {b["name"]: b["state"] for b in r["backups"]}
    assert states == {"cloud-project-2026-11-11_1200": "running", "cloud-project-2026-11-11_0300": "ready",
                      "cloud-project-2026-11-10_0300": "ready", "cloud-project-2026-11-09_0300": "unfinished"}, states
    assert [b["name"] for b in r["backups"]][0] == "cloud-project-2026-11-11_1200", "newest first"
    b = next(b for b in r["backups"] if b["name"] == "cloud-project-2026-11-11_0300")
    assert (b["db_bytes"], b["files_bytes"], b["created"]) == (2048, 4096, "2026-11-12 03:00:41 Asia/Sakhalin")
    assert r["running"] is True and r["warnings"] == [], r["warnings"]
    step("Backups listed newest first with sizes and states (ready / running / unfinished); a fresh one — no warnings")

    # --- Warnings: old backups, failed attempt --------------------------------
    shutil.rmtree(os.path.join(DIR, "cloud-project-2026-11-11_0300"))
    with open(os.path.join(DIR, "LAST_RESULT"), "w") as f:
        f.write("result=error\ntime=2026-11-12 03:00\nname=\nerror=ошибка выгрузки базы (pg_dump)\n")
    w = admin.call("GET", "/admin/backups")["warnings"]
    assert any("более 4" in x or "более" in x for x in w) and any("pg_dump" in x for x in w), w
    step("Warnings when the last ready backup is older than 26 h and when the last attempt failed")

    # --- Downloads --------------------------------------------------------------
    audit = lambda action: len(admin.call("GET", f"/admin/audit?action={action}")["entries"])  # noqa: E731
    downloads0, requests0 = audit("backup.downloaded"), audit("backup.requested")
    data = admin.call("GET", "/admin/backups/cloud-project-2026-11-10_0300/db.dump", raw=True)
    assert data == b"PGDMP-dump"
    admin.call("GET", "/admin/backups/cloud-project-2026-11-10_0300/files.tar.gz", raw=True)
    admin.call("GET", "/admin/backups/cloud-project-2026-11-10_0300/COMPLETE", expect=404)
    admin.call("GET", "/admin/backups/cloud-project-2026-11-11_1200/db.dump", expect=404)  # not finished
    admin.call("GET", "/admin/backups/..%2F..%2Fetc/db.dump", expect=404)
    admin.call("GET", "/admin/backups/not-a-backup/db.dump", expect=404)
    member.call("GET", "/admin/backups/cloud-project-2026-11-10_0300/db.dump", expect=404)
    assert audit("backup.downloaded") == downloads0 + 2
    step("Ready backups download (db.dump, files.tar.gz) and are audited; other files, unfinished and bad names refused")

    # --- «Make a backup now» ----------------------------------------------------
    r = admin.call("POST", "/admin/backups/now")
    assert r["requested_at"] and os.path.isfile(os.path.join(DIR, "REQUEST"))
    body = open(os.path.join(DIR, "REQUEST")).read()
    assert "by=o@example.com" in body
    admin.call("POST", "/admin/backups/now")  # already requested: no second request
    assert audit("backup.requested") == requests0 + 1
    step("«Сделать копию сейчас» leaves one REQUEST for the service")

    # --- The real service picks the request up --------------------------------
    script = os.path.join(os.path.dirname(__file__), "backup.sh")
    if shutil.which("pg_dump") and os.environ.get("PGHOST"):
        env = dict(os.environ, BACKUP_DIR=DIR, BLOBS_DIR=os.environ.get("BLOBS_DIR", DIR + "/../data"))
        subprocess.run(["sh", script, "once"], env=env, check=True, capture_output=True, timeout=300)
        r = admin.call("GET", "/admin/backups")
        assert r["requested_at"] is None and r["alive"]
        assert r["last_result"]["result"] == "ok"
        new = [b for b in r["backups"] if b["state"] == "ready" and b["name"].startswith("cloud-project-20") and b["db_bytes"] > 1000]
        assert new, r["backups"]
        assert admin.call("GET", f"/admin/backups/{new[0]['name']}/db.dump", raw=True)[:5] == b"PGDMP"
        step("backup.sh takes the request, makes a real backup and records the result")
    else:
        print("  (backup.sh not run: no pg_dump / PGHOST)")

    # Leave the folder clean.
    for x in os.listdir(DIR):
        p = os.path.join(DIR, x)
        if x.startswith("cloud-project-") or x == "not-a-backup":
            shutil.rmtree(p)
        elif x in ("STATUS", "LAST_RESULT", "REQUEST"):
            os.remove(p)
    print("ALL BACKUP TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
