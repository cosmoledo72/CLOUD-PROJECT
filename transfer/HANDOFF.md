# CLOUD-PROJECT — передача работы (на 20.11.2026)

Документ для Claude на новом аккаунте: всё, что нужно, чтобы продолжить разработку с того же места.

## 1. Где что лежит

| Что | Где |
|---|---|
| Рабочая копия кода | `D:\PROGRAMS\CLOUD-PROJECT` на компьютере заказчика — с git (с итерации 29) |
| Удалённый репозиторий | `https://github.com/cosmoledo72/cloud-project` (приватный), ветка `main` |
| История git | репозиторий GitHub выше; `transfer/cloud-project.bundle` — запасная копия истории до 13.11 |
| Техническое задание | `docs/SPEC.md` |
| Дорожная карта | `docs/PLAN.md` |
| История решений по итерациям | `docs/CHAT_LOG.md` |
| Секреты рабочего портала | `.env` в корне — **не изменять** |
| Скрипты среды разработки в облаке | `transfer/dev-env/` (up.sh, server.sh, seed/*.py, примеры Playwright) |

Структура кода:
- `backend/` — Rust (axum + sqlx + PostgreSQL). Модули в `src/` по разделам (projects, files, crm/, finance, kb, chat, templates, profiles, ...). Миграции — `backend/migrations/0001…0029`, применяются автоматически при старте.
- `frontend/` — SvelteKit 3 / Svelte 5 / Vite 8, adapter-static (SPA, fallback index.html); настройки SvelteKit — в `vite.config.ts`, импорт общих модулей — `#lib/<путь с расширением>` (не `$lib`). Страницы — `src/routes/*`, общие компоненты — `src/lib/`, `src/lib/ui/` (StatTile, PickFilter, EmptyState и др.).
- `scripts/` — сквозные тесты `*_test.py` (Python, без зависимостей; общий клиент в `smoke_test.py`), `start.cmd` (запуск на Windows), резервные копии.
- `deploy/` — Caddy, сертификаты локального HTTPS; `docker-compose*.yml` — сборка.

## 2. Состояние

Сделаны итерации 1–29 и 33 (SvelteKit 3, 20.11.2026); 30 и 31 отложены заказчиком. Последняя — **29 «Гигиена и надёжность»** (20.11.2026, по ревизии — документ проекта AUDIT_2026-11-20.md): расписание резервных копий в портале (час — в «Резервных копиях», пояс — пояс системного портала, миграция 0030), догоняющая копия, плашка «Портал обновлён» и запрет кэширования `index.html`, защитные заголовки nginx, обновление зависимостей, clippy, git в рабочей папке и GitHub.

Следующее по плану (PLAN.md, пересчитан 20.11.2026): итерация 30 «Запуск в офисе» (A1–A4, нужен заказчик); пилот A5–A7; итерация 31 — XLSX и расширенные шаблоны (C8); 32 — D3, оповещения, CI (D6), Playwright (D7); 33 — SvelteKit 3.

## 3. Порядок работы (правила заказчика)

1. Ответы — на русском; комментарии в коде — на английском.
2. `.env` в корне не трогать.
3. Каждая итерация: короткий план → подтверждение → разработка → тесты → проверка в браузере (светлая, тёмная тема, телефон) → обновление SPEC/PLAN/CHAT_LOG (в `docs/` и в проекте Claude) → выкладка на диск.
4. Выкладка на диск: перед записью сравнить md5 файла на диске с `git show HEAD:<файл>`; если отличается — заказчик правил вручную, сначала слить. После записи сверить md5 всех файлов.
5. Коммиты — по одному на итерацию, сообщение на английском: `Iteration NN: ...`; отправка в `origin main`. Git в папке заказчика ведётся тоже: после выкладки там делается тот же коммит (или `git pull`), чтобы `git status` был чистым и ручные правки заказчика были видны.

## 4. Среда разработки в облачном контейнере Claude

Контейнер каждой новой сессии пустой. Восстановление:

```bash
# 1. Code with history: from GitHub (attach the repo to the session first)
git clone https://github.com/cosmoledo72/cloud-project /home/claude/cp
# Manual edits on the user's PC show up in `git status` there (device shell).

# 2. Tools (versions used: cargo 1.97, node 22, PostgreSQL 16, Python 3.13)
apt-get install -y postgresql   # if missing
service postgresql start
sudo -u postgres psql -c "ALTER USER postgres PASSWORD 'postgres'" -c "CREATE DATABASE workhub"
cd /home/claude/cp/backend && cargo build
cd /home/claude/cp/frontend && npm ci

# 3. Run: copy transfer/dev-env/*.sh to /home/claude/run/ and start
mkdir -p /home/claude/run/{data,backups,shots} && cp transfer/dev-env/*.sh /home/claude/run/ && chmod +x /home/claude/run/*.sh
/home/claude/run/up.sh          # postgres + backend :8080 + vite :5173
# After backend changes:
cargo build && pkill -f target/debug/workhub-server; sleep 1; /home/claude/run/up.sh

# 4. Demo portal for browser checks
python3 transfer/dev-env/seed/seed.py   # portal "demo"
```

Демо-портал: `demo`, владелец o@example.com / owner-password-1; Борис Ковалёв — b@example.com / boris-password-1 (или b-password-123 по seed.py); Анна Петрова — a@example.com. Остальные `seed*.py` дозаполняют проекты, финансы, CRM (пути внутри скриптов — `/home/claude/cp/...`).

Сквозные тесты (28 файлов, onlyoffice_live_test — только с настоящим ONLYOFFICE):

```bash
export BACKUP_DIR=/home/claude/run/backups BACKUP_PORTAL=bkptest PGHOST=localhost PGUSER=postgres PGPASSWORD=postgres PGDATABASE=workhub BLOBS_DIR=/home/claude/run/data
for t in scripts/*_test.py; do [ "$t" = scripts/onlyoffice_live_test.py ] && continue
  r=$(API=http://localhost:8080 DOCS_JWT_SECRET=dev-docs-secret-123 TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/workhub timeout 300 python3 $t 2>&1 | tail -1); echo "$t: $r"; done
```

Длительный прогон запускать в фоне с записью в лог. Если несколько тестов подряд падают с 429 — сработала защита от подбора пароля (iter3_test её проверяет): перезапустить backend и повторить.

Также: `cargo test` (73 теста) в `backend/`, `npx svelte-check --threshold warning` в `frontend/`. Скриншоты — Playwright (`require('playwright')`, примеры в `transfer/dev-env/*.js`).

## 5. Важные технические заметки

- **Шаблоны документов** (`backend/src/ooxml.rs`, `templates.rs`): метки `{{key}}`, только ASCII, регистр не важен; `values_for` строит словарь (используется в генерации, проверке и CRM-документах); алиасы — `resolve_key`/`add_aliases`; данные человека — `profiles::person_values`.
- **Реквизиты проектов** — `backend/src/fields.rs` (пресеты, ключ ГИПа — `chief_engineer`, тип `user`).
- **Права**: роли owner/admin/member/guest; коллеги видят контакты сотрудника, реквизиты — сам сотрудник и администраторы.
- **Защита входа**: ограничение попыток в `ratelimit.rs` (по IP из X-Real-IP).
- **Тесты, зависящие от даты**: учитывать часовой пояс Сахалина (UTC+11) и рабочие дни проекта.
- **Docker на Windows**: первая сборка backend ~9 минут, повторные ~4 минуты; при ошибке «read-only file system» — перезапуск Docker Desktop / очистка `docker builder prune`.
