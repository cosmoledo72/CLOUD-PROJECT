# CLOUD-PROJECT (WorkHub)

Multi-tenant collaboration platform: document management with real-time co-editing in **ONLYOFFICE Docs**, projects and tasks with a **Gantt chart**, and an **admin panel** with configurable project requisites and portal branding.

- **Backend:** Rust (axum, sqlx, PostgreSQL)
- **Frontend:** SvelteKit + Svelte 5 SPA
- **Editor:** ONLYOFFICE Document Server
- **Deployment:** Docker Compose

Architecture, security model and roadmap are in [`docs/SPEC.md`](docs/SPEC.md) (in Russian).

## Quick start (Docker Desktop / Linux server)

```bash
cp .env.example .env        # replace every secret
docker compose up -d --build
```

| Service | URL |
|---|---|
| Web UI | http://localhost:8080 — create a portal on the login page |
| ONLYOFFICE Docs | http://localhost:8081 (first start takes 1–2 minutes) |

### Office server: HTTPS on cloud-project.online

```bash
# .env: real secrets + ACME_EMAIL=you@example.com (optional PORTAL_DOMAIN, DOCS_DOMAIN)
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d --build
# Windows: scripts\start.cmd prod
```

Caddy (`deploy/Caddyfile`) obtains Let's Encrypt certificates for `cloud-project.online` (portal) and
`docs.cloud-project.online` (ONLYOFFICE). Both DNS names must point to the office's public IP and the router
must forward TCP 80/443 (and UDP 443) to the server. The overlay sets `COOKIE_SECURE=true`, the public URLs,
`SIGNUP_ENABLED=false` (set `true` once to create the first portal), and binds ports 8080/8081 to
`127.0.0.1` only. Step-by-step guide (in Russian): `docs/SPEC.md`, section 12.

### Backups

The `backup` service dumps the database and archives the portal files into `./backups` every night at
`BACKUP_HOUR` (default 03, time zone `TZ`, default Asia/Sakhalin) and keeps `BACKUP_KEEP_DAYS` (14) days.

```bash
docker compose exec backup sh /usr/local/bin/backup.sh now        # backup now   (scripts\backup-now.cmd)
docker compose exec backup sh /usr/local/bin/backup.sh list       # list
docker compose stop web backend                                   # restore      (scripts\restore.cmd NAME)
docker compose exec backup sh /usr/local/bin/backup.sh restore cloud-project-2026-11-12_0300
docker compose start backend web
```

Keep a copy of `.env` separately: `APP_SECRET` decrypts stored SMTP passwords. Copy `./backups` to another disk.

### Live ONLYOFFICE check (task A2)

```bash
python scripts/onlyoffice_live_test.py --tenant <portal> --email <e-mail>
```

Checks the Document Server health and JWT secret, makes it download and convert a new document from the
backend, then guides through forced save and co-editing in two browsers and verifies the new versions.

## Local development

Requirements: Rust 1.80+, Node 20+, PostgreSQL 16, and optionally a Document Server container.

```bash
# 1. Database
createdb workhub

# 2. Document Server (optional, needed for the editor)
docker run -d -p 8081:80 -e JWT_SECRET=dev-docs-secret-123 \
  -e ALLOW_PRIVATE_IP_ADDRESS=true onlyoffice/documentserver

# 3. Backend (http://localhost:8080)
cd backend
export DATABASE_URL=postgres://postgres@localhost:5432/workhub
export APP_SECRET=dev-app-secret-0123456789abcdef0123456789
export DOCS_JWT_SECRET=dev-docs-secret-123
export DOCS_PUBLIC_URL=http://localhost:8081 DOCS_INTERNAL_URL=http://localhost:8081
# The Document Server container must reach the backend on the host:
export INTERNAL_URL=http://host.docker.internal:8080
cargo run

# 4. Frontend (http://localhost:5173; /api is proxied to :8080)
cd frontend
npm install
npm run dev
```

On Windows PowerShell, set variables with `$env:NAME = "value"` instead of `export`.

## Tests

The end-to-end API smoke test needs a running backend and uses only the Python standard library. It covers:

- sign-up and TOTP login;
- ACL and tenant isolation;
- uploads and file versions;
- the ONLYOFFICE download → callback → new version flow, with a simulated Document Server;
- projects, tasks, dependencies, cycle rejection and time tracking.

```bash
API=http://localhost:8080 DOCS_JWT_SECRET=<same as backend> python3 scripts/smoke_test.py
API=http://localhost:8080 python3 scripts/admin_test.py   # admin panel, branding, requisites
API=http://localhost:8080 python3 scripts/iter3_test.py   # templates, trash, login rate limit
API=http://localhost:8080 python3 scripts/iter4_test.py   # Gantt auto-shift, critical path, baseline
API=http://localhost:8080 python3 scripts/crm_test.py      # CRM: companies, import, visibility, deals, projects
API=http://localhost:8080 python3 scripts/mail_test.py     # SMTP settings, outbox, invitations, password reset
API=http://localhost:8080 python3 scripts/notify_test.py   # notifications, comments, mentions, live stream, digests, due reminders
API=http://localhost:8080 python3 scripts/crm_tasks_test.py # CRM to-dos: links, rights, reminders, completion into history
API=http://localhost:8080 python3 scripts/sales_docs_test.py # sales documents: numbering, templates, VAT, amounts in words, folders
API=http://localhost:8080 python3 scripts/kb_chat_test.py   # knowledge base, chats, team directory, finance filters
API=http://localhost:8080 python3 scripts/tasks_test.py     # "All tasks" board, task numbers, labels, checklists
API=http://localhost:8080 python3 scripts/project_edit_test.py  # project rename, folder rename, validation
cd backend && cargo test                                   # unit tests (SVG filter, OOXML, limiter, scheduling, requisites validation, import)
```

The simulated Document Server listens on port 9999, so run the backend with `DOCS_PUBLIC_URL=DOCS_INTERNAL_URL=http://localhost:9999` for this test.

`mail_test.py` and `notify_test.py` start their own SMTP server on 127.0.0.1, so run the backend with `SMTP_ALLOW_PRIVATE=true` for them; `notify_test.py` and `crm_tasks_test.py` also need short timers: `NOTIFY_EMAIL_DELAY_SECS=2 DUE_SCAN_SECS=2 DUE_REMINDER_HOUR=0`.

## Configuration (backend)

| Variable | Required | Description |
|---|---|---|
| `DATABASE_URL` | yes | Postgres connection string |
| `APP_SECRET` | yes | ≥ 32 chars; signs URLs handed to the Document Server |
| `DOCS_JWT_SECRET` | yes | Same value as `JWT_SECRET` of the Document Server |
| `INTERNAL_URL` | | API base URL as seen by the Document Server |
| `DOCS_PUBLIC_URL` | | Document Server URL as seen by the browser |
| `DOCS_INTERNAL_URL` | | Document Server URL as seen by the backend |
| `STORAGE_DIR` | | Blob directory (default `./data/blobs`) |
| `COOKIE_SECURE` | | `true` behind HTTPS |
| `SESSION_TTL_HOURS` | | Default `168` |
| `SIGNUP_ENABLED` | | Allow public portal registration (default `true`) |
| `MAX_UPLOAD_MB` | | Default `512` |
| `CORS_ORIGIN` | | Only if the SPA is served from another origin |
| `TRASH_RETENTION_DAYS` | | Auto-purge trash after N days (default `30`) |
| `LOGIN_ACCOUNT_LIMIT` | | Failed logins per account per 15 min (default `8`) |
| `LOGIN_IP_LIMIT` | | Failed logins per IP per 15 min (default `30`) |
| `SIGNUP_IP_LIMIT` | | Portal sign-ups per IP per hour (default `10`) |

Running the test scripts repeatedly from one machine hits the IP limits; for test runs set `LOGIN_IP_LIMIT` and `SIGNUP_IP_LIMIT` high.

## Layout

```
backend/            Rust API
  migrations/       SQL schema (embedded into the binary)
  assets/templates/ Blank docx/xlsx/pptx for "New document"
  src/
    auth/           Sessions, Argon2, TOTP, login flow
    accounts.rs     Invitations and password reset by one-time links
    mail.rs         SMTP settings (encrypted password), outbox worker with retries
    notify.rs       Notifications: preferences, live stream (SSE), email digests, due-date reminders
    comments.rs     Task comments with @mentions
    crm/            CRM: companies, contacts, deals pipeline, activities, to-dos, sales documents, import, requisites validation
    money.rs        Amounts in words (Russian), VAT included in a price
    kb.rs           Knowledge base: Markdown page tree, Russian full-text search
    chat.rs         Chats: project, group and direct channels; live delivery; unread counters
    team.rs         Team directory and self-service profile
    acl.rs          Effective-access resolution for folders/files
    admin.rs        Overview, audit log, portal settings, account recovery, groups
    branding.rs     Portal branding, logo/icon upload with image validation
    fields.rs       Project requisites: typed field definitions and values
    ooxml.rs        {{placeholder}} substitution in DOCX/XLSX (handles split runs)
    templates.rs    Document templates and generation into project folders
    trash.rs        Trash: restore, purge, hourly auto-purge
    ratelimit.rs    Login/sign-up throttling
    files.rs        Folders, files, versions, shares
    onlyoffice.rs   Editor config, Document Server download/callback
    projects.rs     Projects, tasks, dependencies, time tracking, baseline
    scheduling.rs   Dependency-driven auto-shift and critical path (CPM)
    users.rs        Users and groups
frontend/           SvelteKit SPA
  src/lib/Gantt.svelte  SVG Gantt chart (drag/resize, dependencies, critical path, baseline)
  src/lib/RequisitesForm.svelte  Typed requisites form (view/edit)
  src/lib/NotificationBell.svelte  Notification center; live updates via notifications.svelte.ts
  src/lib/MentionInput.svelte    Textarea with @mention suggestions
  src/lib/branding.svelte.ts     Runtime theming from portal branding
  src/routes/admin/     Admin panel
  src/routes/crm/       CRM: deals kanban, companies, contacts, pipeline stages
  src/lib/crm/          Company form, activity feed, requisites validation
  src/lib/TaskRows.svelte  Project tasks as rows (grouping, sorting, filters)
  src/lib/chat.svelte.ts   Chat unread counter and live events
  src/routes/kb/ chats/ team/ finance/  Knowledge base, chats, team, finance
docs/SPEC.md        Specification
scripts/            End-to-end API tests
```

## Licensing note

ONLYOFFICE Docs Community Edition is AGPL-3.0 and limited to 20 simultaneous connections. A commercial SaaS offering needs the Enterprise or Developer edition. The integration API is identical across editions.
