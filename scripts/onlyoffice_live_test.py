#!/usr/bin/env python3
"""Live check of the ONLYOFFICE Docs integration (task A2) against a real
Document Server started with docker compose.

Automatic part:
  1. the Document Server is up (/healthcheck) and answers its command service
     with our JWT secret (a wrong secret is reported clearly);
  2. the Document Server can download a document from the backend: a new
     .docx is created in "Мои документы" and converted to PDF by the server
     (this uses the same signed download link as the editor);
Guided part (needs a person, skipped with --no-wait):
  3. forced save (Ctrl+S in the editor) creates version 2;
  4. co-editing in two browsers and closing the editor creates version 3.

Usage (from the project folder, portal started):
    python scripts/onlyoffice_live_test.py --tenant <portal> --email <e-mail>
    python scripts/onlyoffice_live_test.py --portal https://cloud-project.online \\
        --docs https://docs.cloud-project.online --tenant ... --email ...

The password is asked interactively (or WH_PASSWORD). DOCS_JWT_SECRET is read
from the environment or, read-only, from .env in the project folder.
Only the Python standard library is used.
"""

import argparse
import base64
import getpass
import hashlib
import hmac
import http.cookiejar
import json
import os
import ssl
import sys
import time
import urllib.error
import urllib.request
import uuid

ERRORS = {
    -1: "неизвестная ошибка",
    -2: "тайм-аут конвертации",
    -3: "ошибка конвертации",
    -4: "сервер документов не смог скачать файл с бэкенда (проверьте INTERNAL_URL и сеть Docker)",
    -5: "неверный пароль документа",
    -6: "ошибка базы данных сервера документов",
    -7: "ошибка входных данных",
    -8: "неверный токен: DOCS_JWT_SECRET бэкенда и JWT_SECRET сервера документов не совпадают",
}

OK, FAIL = "  ✓", "  ✗"


def jwt(payload: dict, secret: str) -> str:
    """HS256 JWT, as the Document Server expects in the request body."""
    def b64(b: bytes) -> str:
        return base64.urlsafe_b64encode(b).rstrip(b"=").decode()
    head = b64(json.dumps({"alg": "HS256", "typ": "JWT"}).encode())
    body = b64(json.dumps(payload, ensure_ascii=False).encode())
    sig = hmac.new(secret.encode(), f"{head}.{body}".encode(), hashlib.sha256).digest()
    return f"{head}.{body}.{b64(sig)}"


def secret_from_env_file(path: str) -> str | None:
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.strip()
                if line.startswith("DOCS_JWT_SECRET="):
                    return line.split("=", 1)[1].strip().strip('"').strip("'")
    except OSError:
        return None
    return None


class Http:
    def __init__(self, insecure: bool):
        ctx = ssl.create_default_context()
        if insecure:
            ctx.check_hostname = False
            ctx.verify_mode = ssl.CERT_NONE
        self.opener = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()), urllib.request.HTTPSHandler(context=ctx)
        )

    def request(self, method: str, url: str, body=None, timeout=60) -> tuple[int, bytes]:
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(url, data=data, method=method)
        req.add_header("Accept", "application/json")
        if data is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with self.opener.open(req, timeout=timeout) as r:
                return r.status, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.read()

    def json(self, method: str, url: str, body=None, timeout=60) -> dict:
        status, raw = self.request(method, url, body, timeout)
        if status >= 400:
            raise RuntimeError(f"{method} {url}: HTTP {status} {raw[:300].decode(errors='replace')}")
        return json.loads(raw or b"{}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Live check of ONLYOFFICE Docs integration")
    ap.add_argument("--portal", default="http://localhost:8080", help="portal address")
    ap.add_argument("--docs", default="http://localhost:8081", help="Document Server address as seen from this computer")
    ap.add_argument("--tenant", required=True, help="portal (tenant) short name")
    ap.add_argument("--email", required=True)
    ap.add_argument("--secret", help="DOCS_JWT_SECRET (default: environment or .env)")
    ap.add_argument("--no-wait", action="store_true", help="only the automatic part")
    ap.add_argument("--insecure", action="store_true", help="do not verify HTTPS certificates")
    ap.add_argument("--timeout", type=int, default=180, help="seconds to wait for each new version")
    a = ap.parse_args()

    portal, docs = a.portal.rstrip("/"), a.docs.rstrip("/")
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    secret = a.secret or os.environ.get("DOCS_JWT_SECRET") or secret_from_env_file(os.path.join(root, ".env"))
    if not secret:
        print("Не найден DOCS_JWT_SECRET: передайте --secret или запустите из папки проекта с .env")
        return 2
    password = os.environ.get("WH_PASSWORD") or getpass.getpass(f"Пароль {a.email}: ")
    h = Http(a.insecure)
    failed = 0

    print(f"Сервер документов: {docs}")
    # 1. Health and the command service (checks the shared JWT secret).
    try:
        status, raw = h.request("GET", f"{docs}/healthcheck", timeout=15)
        if status == 200 and raw.strip() == b"true":
            print(f"{OK} сервер документов запущен")
        else:
            print(f"{FAIL} /healthcheck ответил {status} {raw[:100]!r} — сервер ещё запускается или неисправен")
            return 1
    except OSError as e:
        print(f"{FAIL} сервер документов недоступен: {e}")
        return 1

    cmd = {"c": "version"}
    r = h.json("POST", f"{docs}/coauthoring/CommandService.ashx", {**cmd, "token": jwt(cmd, secret)})
    if r.get("error") == 0:
        print(f"{OK} версия ONLYOFFICE Docs {r.get('version')}, секрет JWT совпадает")
    else:
        print(f"{FAIL} служба команд вернула ошибку {r.get('error')}: "
              + ("секрет JWT не совпадает с JWT_SECRET сервера документов" if r.get("error") == 6 else str(r)))
        failed += 1

    # 2. Log in, create a document and let the Document Server download it.
    print(f"Портал: {portal}")
    h.json("POST", f"{portal}/api/auth/login", {"tenant": a.tenant, "email": a.email, "password": password})
    me = h.json("GET", f"{portal}/api/auth/me")
    if me.get("mfa_required") or me.get("user", me).get("id") is None:
        print(f"{FAIL} вход не завершён (двухфакторная защита?) — используйте учётную запись без 2FA")
        return 1
    print(f"{OK} вход выполнен: {me.get('user', me).get('display_name')}")
    roots = h.json("GET", f"{portal}/api/folders/roots")
    name = f"Проверка ONLYOFFICE {time.strftime('%Y-%m-%d %H-%M-%S')}"
    file_id = h.json("POST", f"{portal}/api/folders/{roots['my']}/new", {"kind": "docx", "name": name})["id"]
    cfg = h.json("GET", f"{portal}/api/files/{file_id}/editor")
    doc = cfg["config"]["document"]
    print(f"{OK} создан документ «{name}.docx»; редактор: {cfg['config']['editorConfig']['mode']}, "
          f"адрес сервера документов в портале: {cfg['docs_url']}")
    if cfg["docs_url"].rstrip("/") != docs and "localhost" not in docs:
        print(f"    ! портал отдаёт браузеру {cfg['docs_url']}, а проверяется {docs} — проверьте DOCS_PUBLIC_URL")

    conv = {"async": False, "filetype": "docx", "outputtype": "pdf", "key": uuid.uuid4().hex,
            "title": f"{name}.docx", "url": doc["url"]}
    r = h.json("POST", f"{docs}/ConvertService.ashx", {**conv, "token": jwt(conv, secret)}, timeout=120)
    if r.get("endConvert") and r.get("fileUrl"):
        status, pdf = h.request("GET", r["fileUrl"], timeout=60)
        if status == 200 and pdf.startswith(b"%PDF"):
            print(f"{OK} сервер документов скачал файл с бэкенда и сконвертировал его в PDF ({len(pdf)} байт)")
        else:
            print(f"{FAIL} результат конвертации не скачивается по {r['fileUrl']} (HTTP {status})")
            failed += 1
    else:
        code = r.get("error")
        print(f"{FAIL} конвертация: ошибка {code} — {ERRORS.get(code, r)}")
        failed += 1

    if a.no_wait or failed:
        print("\nИтог:", "ошибок нет" if not failed else f"ошибок: {failed}")
        return 1 if failed else 0

    # 3–4. Guided: forced save and co-editing create new versions.
    def versions() -> int:
        return len(h.json("GET", f"{portal}/api/files/{file_id}/versions")["versions"])

    def wait_for(n: int, what: str) -> bool:
        deadline = time.time() + a.timeout
        while time.time() < deadline:
            if versions() >= n:
                print(f"{OK} {what}: появилась версия {n}")
                return True
            time.sleep(3)
        print(f"{FAIL} {what}: версия {n} не появилась за {a.timeout} с (смотрите журнал бэкенда: docker compose logs backend)")
        return False

    link = f"{portal}/editor/{file_id}"
    print("\nРучная часть. Откройте документ:")
    print(f"  {link}")
    print("  1) Наберите строку текста и нажмите Ctrl+S (принудительное сохранение).")
    input("     Затем нажмите Enter здесь… ")
    if not wait_for(2, "принудительное сохранение"):
        failed += 1
    print("  2) Откройте ту же ссылку во втором браузере (или от другого сотрудника с доступом к документу).")
    print("     Убедитесь, что видны правки друг друга и курсор второго участника.")
    print("     Наберите ещё по строке в обоих окнах и закройте обе вкладки редактора.")
    input("     Затем нажмите Enter здесь… ")
    if not wait_for(3, "совместное редактирование и закрытие"):
        failed += 1
    print("\nИтог:", "ошибок нет" if not failed else f"ошибок: {failed}")
    print("Проверочный документ можно удалить из «Мои документы».")
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except RuntimeError as e:
        print(f"{FAIL} {e}")
        sys.exit(1)
