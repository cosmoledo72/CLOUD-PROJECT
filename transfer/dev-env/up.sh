#!/bin/bash
# Bring the dev stack up: postgres, backend (server.sh), vite.
service postgresql start >/dev/null 2>&1; sleep 1
curl -s -o /dev/null --noproxy '*' localhost:8080/api/health || /home/claude/run/server.sh >/dev/null 2>&1
if ! curl -s -o /dev/null --noproxy '*' localhost:5173/login; then
  cd /home/claude/cp/frontend && (nohup npx vite dev --port 5173 > /home/claude/run/vite.log 2>&1 &)
  for i in $(seq 1 30); do curl -s -o /dev/null --noproxy '*' localhost:5173/login && break; sleep 1; done
fi
curl -s -o /dev/null -w "api %{http_code} " --noproxy '*' localhost:8080/api/health; curl -s -o /dev/null -w "web %{http_code}\n" --noproxy '*' localhost:5173/login
