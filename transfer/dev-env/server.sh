#!/bin/bash
# usage: server.sh  (restarts backend with test env)
pkill -f target/debug/workhub-server 2>/dev/null; sleep 1
cd /home/claude/cp/backend
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/workhub
export APP_SECRET=dev-app-secret-0123456789abcdef0123456789
export DOCS_JWT_SECRET=dev-docs-secret-123
export DOCS_PUBLIC_URL=http://localhost:9999 DOCS_INTERNAL_URL=http://localhost:9999 INTERNAL_URL=http://localhost:8080
export SMTP_ALLOW_PRIVATE=true NOTIFY_EMAIL_DELAY_SECS=2 DUE_SCAN_SECS=2 DUE_REMINDER_HOUR=0
export SIGNUP_IP_LIMIT=100000 STORAGE_DIR=/home/claude/run/data BACKUP_DIR=/home/claude/run/backups BACKUP_PORTAL=bkptest BACKUP_ALERT_SECS=2
nohup ./target/debug/$(ls target/debug | grep -E '^workhub-server$' | head -1) > /home/claude/run/server.log 2>&1 &
for i in $(seq 1 40); do curl -s localhost:8080/api/auth/me >/dev/null && break; sleep 0.5; done
tail -3 /home/claude/run/server.log
