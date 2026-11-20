#!/usr/bin/env python3
"""End-to-end test of the web server (frontend/nginx.conf): caching and security headers.

Checks what an open browser tab depends on after a rebuild of the portal:
index.html and /_app/version.json are never cached, hashed bundles are cached
for a year; pages carry the anti-framing and content-type headers.

Needs the nginx of the "web" service (or the same config) in front of the API:
    WEB=http://localhost:8080 python3 scripts/web_headers_test.py
Without WEB the test is skipped (the API-only dev setup has no nginx).
"""

import os
import re
import sys
import urllib.request

sys.path.insert(0, os.path.dirname(__file__))
from smoke_test import step  # noqa: E402

WEB = os.environ.get("WEB", "").rstrip("/")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect)


def get(path):
    try:
        with opener.open(WEB + path, timeout=20) as r:
            return r.status, r.headers, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.headers, e.read()


def cache(h):
    """All Cache-Control values ("expires" adds one of its own)."""
    return ", ".join(h.get_all("Cache-Control") or [])


def main():
    if not WEB:
        print("SKIPPED: set WEB to the address of the web service (nginx)")
        return

    for path in ("/", "/projects", "/team/me", "/index.html", "/s/some-public-link"):
        status, h, body = get(path)
        assert status == 200 and b"<html" in body.lower(), (path, status)
        assert "no-cache" in cache(h), (path, cache(h))
        assert h.get("X-Frame-Options") == "SAMEORIGIN", (path, h.get("X-Frame-Options"))
        csp = h.get("Content-Security-Policy") or ""
        assert "frame-ancestors 'self'" in csp and "object-src 'none'" in csp, (path, csp)
        assert h.get("X-Content-Type-Options") == "nosniff", path
        assert "camera=()" in (h.get("Permissions-Policy") or ""), path
    step("Every page (index.html and the SPA fallback) is not cached and refuses framing by other sites")

    status, h, body = get("/_app/version.json")
    assert status == 200 and b"version" in body, status
    assert "no-cache" in cache(h), cache(h)
    step("/_app/version.json is not cached: open tabs notice a rebuilt portal")

    _, _, index = get("/")
    m = re.search(rb'/_app/immutable/[^"\']+\.js', index)
    assert m, "no hashed bundle referenced by index.html"
    status, h, _ = get(m.group(0).decode())
    assert status == 200 and "immutable" in cache(h) and "no-cache" not in cache(h), (m.group(0), cache(h))
    step("Hashed bundles under /_app/immutable/ are cached for a year")

    status, _, _ = get("/api/health")
    assert status == 200, status
    step("/api is proxied to the backend")
    print("ALL WEB HEADER TESTS PASSED")


if __name__ == "__main__":
    try:
        main()
    except AssertionError as e:
        print("FAILED:", e)
        raise
