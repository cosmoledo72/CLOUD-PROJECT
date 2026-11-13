# CLOUD-PROJECT — передача работы (на 13.11.2026)

Документ для Claude на новом аккаунте: всё, что нужно, чтобы продолжить разработку с того же места.

## 1. Где что лежит

| Что | Где |
|---|---|
| Рабочая копия кода | `D:\PROGRAMS\CLOUD-PROJECT` на компьютере заказчика (git там нет) |
| История git (итерации 1–28в и пакет переноса) | `transfer/cloud-project.bundle` — восстановить: `git clone cloud-project.bundle cp` |
| Техническое задание | `docs/SPEC.md` |
| Дорожная карта | `docs/PLAN.md` |
| История решений по итерациям | `docs/CHAT_LOG.md` |
| Секреты рабочего портала | `.env` в корне — **не изменять** |
| Скрипты среды разработки в облаке | `transfer/dev-env/` (up.sh, server.sh, seed/*.py, примеры Playwright) |

Структура кода:
- `backend/` — Rust (axum + sqlx + PostgreSQL). Модули в `src/` по разделам (projects, files, crm/, finance, kb, chat, templates, profiles, ...). Миграции — `backend/migrations/0001…0029`, применяются автоматически при старте.
- `frontend/` — SvelteKit 2 / Svelte 5, adapter-static (SPA, fallback index.html). Страницы — `src/routes/*`, общие компоненты — `src/lib/`, `src/lib/ui/` (StatTile, PickFilter, EmptyState и др.).
- `scripts/` — сквозные тесты `*_test.py` (Python, без зависимостей; общий клиент в `smoke_test.py`), `start.cmd` (запуск на Windows), резервные копии.
- `deploy/` — Caddy, сертификаты локального HTTPS; `docker-compose*.yml` — сборка.

## 2. Состояние на момент переноса

Сделаны итерации 1–28в. Последние:
- **28** — отчёты CRM (B11); **28а** — единый стиль страниц, «Финансы» по макету; **28б** — карточка сотрудника (`/team/{id}`, миграция 0029, фото, подпись, реквизиты для документов, свои поля администратора); **28в** — метки шаблонов: `{{ключ}}` и `{{project_ключ}}` = `{{project.ключ}}`, подметки сотрудника `{{project.<ключ>.short_name}}` и др., понятное предупреждение о ненайденных метках.
- Заказчик пересобрал портал с 28в (13.11.2026), сборка успешна.

Открыто:
- Заказчик сообщил, что «карточка сотрудника не отображается на сайте». Файлы на диске проверены — совпадают с git. Вероятная причина — вкладка браузера со старой версией SPA (index.html отдаётся nginx без `Cache-Control: no-cache`). Предложено (ответа ещё нет): `Cache-Control: no-cache` для index.html в `frontend/nginx.conf` + `kit.version.pollInterval` в `svelte.config.js` и плашка «Портал обновлён — обновить страницу» через `updated` из `$app/state`. Сначала уточнить у заказчика результат Ctrl+F5.

Следующие шаги по плану (после подтверждения заказчика):
- волна 6 «Готовность к SaaS» или запуск на сервере офиса (A1–A4, SPEC раздел 12);
- выгрузка платежей и документов в XLSX (сейчас CSV);
- волна 9: сборка тома C9 и «Инструменты»; Garage — отложен.

## 3. Порядок работы (правила заказчика)

1. Ответы — на русском; комментарии в коде — на английском.
2. `.env` в корне не трогать.
3. Каждая итерация: короткий план → подтверждение → разработка → тесты → проверка в браузере (светлая, тёмная тема, телефон) → обновление SPEC/PLAN/CHAT_LOG (в `docs/` и в проекте Claude) → выкладка на диск.
4. Выкладка на диск: перед записью сравнить md5 файла на диске с `git show HEAD:<файл>`; если отличается — заказчик правил вручную, сначала слить. После записи сверить md5 всех файлов.
5. Коммиты — по одному на итерацию, сообщение на английском: `Iteration NN: ...`.

## 4. Среда разработки в облачном контейнере Claude

Контейнер каждой новой сессии пустой. Восстановление:

```bash
# 1. Code with history (bundle staged from the user's folder transfer/)
git clone /path/to/cloud-project.bundle /home/claude/cp
# Bring in any manual edits from D:\PROGRAMS\CLOUD-PROJECT (compare md5, copy the newer files).

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
