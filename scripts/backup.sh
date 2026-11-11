#!/bin/sh
# Backups for CLOUD-PROJECT: PostgreSQL dump + portal files.
# Runs inside the "backup" service (postgres:16-alpine), see docker-compose.yml.
#
#   backup.sh loop             nightly at BACKUP_HOUR, keep BACKUP_KEEP_DAYS days (service default)
#   backup.sh now              make a backup right away
#   backup.sh once             one turn of the loop: heartbeat, serve a REQUEST (tests)
#   backup.sh list             list backups
#   backup.sh restore NAME     restore a backup (stop the backend first!)
#
# Each backup is a folder BACKUP_DIR/cloud-project-YYYY-MM-DD_HHMM with:
#   db.dump      pg_dump custom format
#   files.tar.gz portal files (document versions, logos, ...)
#   COMPLETE     written last; folders without it are unfinished and ignored
#
# Files shared with the portal («Администрирование → Резервные копии»):
#   STATUS       heartbeat of the loop, rewritten every minute (alive=<unix time>)
#   LAST_RESULT  outcome of the last attempt (result=ok|error, time, name, error)
#   REQUEST      written by the portal: «make a backup now»; taken within a minute
#
# Connection: PGHOST, PGUSER, PGDATABASE, PGPASSWORD. Paths: BLOBS_DIR, BACKUP_DIR.
# Times are taken from PostgreSQL in the TZ time zone (default Asia/Sakhalin).

set -eu

BACKUP_DIR="${BACKUP_DIR:-/backups}"
BLOBS_DIR="${BLOBS_DIR:-/data/blobs}"
BACKUP_HOUR="$(printf '%02d' "$(echo "${BACKUP_HOUR:-03}" | sed 's/^0*//; s/^$/0/')")"
BACKUP_KEEP_DAYS="${BACKUP_KEEP_DAYS:-14}"
TZ_NAME="${TZ:-Asia/Sakhalin}"
PREFIX="cloud-project-"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') backup: $*"; }

# Local time from the database (the alpine image may lack zone files).
db_now() { psql -XAtc "SELECT to_char(now() AT TIME ZONE '$TZ_NAME', '$1')"; }

wait_db() {
    i=0
    until pg_isready -q; do
        i=$((i + 1))
        if [ "$i" -ge 60 ]; then log "database is not reachable"; return 1; fi
        sleep 2
    done
}

# Write a small file atomically (the portal may read it at any moment).
write_file() {
    tmp="$1.tmp.$$"
    cat > "$tmp" && mv -f "$tmp" "$1"
}

heartbeat() {
    printf 'alive=%s\n' "$(date +%s)" | write_file "$BACKUP_DIR/STATUS" 2>/dev/null || true
}

# result ok|error, folder name, error text
record() {
    printf 'result=%s\ntime=%s\nname=%s\nerror=%s\n' "$1" "$(db_now 'YYYY-MM-DD HH24:MI' 2>/dev/null || date '+%Y-%m-%d %H:%M')" "$2" "$3" \
        | write_file "$BACKUP_DIR/LAST_RESULT" 2>/dev/null || true
}

# Each step is checked explicitly: "set -e" does not apply inside "make_backup || ...".
fail() {
    log "backup FAILED: $1"
    record error "$(basename "${dest:-}")" "$1"
    return 1
}

make_backup() {
    wait_db || { dest=""; fail "база данных недоступна"; return 1; }
    stamp="$(db_now 'YYYY-MM-DD_HH24MI')" || { dest=""; fail "нет подключения к базе данных"; return 1; }
    dest="$BACKUP_DIR/$PREFIX$stamp"
    if [ -e "$dest" ]; then dest="$dest-$(date +%S)"; fi
    mkdir -p "$dest" || { fail "не удалось создать папку $dest"; return 1; }
    log "start -> $dest"
    pg_dump -Fc -f "$dest/db.dump" || { fail "ошибка выгрузки базы (pg_dump)"; return 1; }
    if [ -d "$BLOBS_DIR" ]; then
        tar -czf "$dest/files.tar.gz" -C "$BLOBS_DIR" . || { fail "ошибка архивации файлов (tar)"; return 1; }
    else
        log "no files folder $BLOBS_DIR, skipping files"
    fi
    # Sanity check: the dump must be readable.
    pg_restore -l "$dest/db.dump" > /dev/null || { fail "дамп базы не читается (pg_restore -l)"; return 1; }
    {
        echo "created=$(db_now 'YYYY-MM-DD HH24:MI:SS') $TZ_NAME"
        echo "database=$(du -k "$dest/db.dump" | cut -f1) KiB"
        if [ -f "$dest/files.tar.gz" ]; then echo "files=$(du -k "$dest/files.tar.gz" | cut -f1) KiB"; fi
    } > "$dest/COMPLETE" || { fail "не удалось записать COMPLETE"; return 1; }
    record ok "$(basename "$dest")" ""
    log "done: $(tr '\n' ' ' < "$dest/COMPLETE")"
}

prune() {
    # Complete backups older than BACKUP_KEEP_DAYS; unfinished ones after a day.
    find "$BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d -name "$PREFIX*" | while read -r d; do
        if [ -f "$d/COMPLETE" ]; then
            if [ -n "$(find "$d/COMPLETE" -mtime +"$((BACKUP_KEEP_DAYS - 1))")" ]; then
                log "remove old $d"
                rm -rf "$d"
            fi
        elif [ -n "$(find "$d" -maxdepth 0 -mmin +1440)" ]; then
            log "remove unfinished $d"
            rm -rf "$d"
        fi
    done
    return 0
}

list() {
    found=0
    for d in "$BACKUP_DIR"/"$PREFIX"*; do
        [ -f "$d/COMPLETE" ] || continue
        found=1
        echo "$(basename "$d")  $(tr '\n' ' ' < "$d/COMPLETE")"
    done
    [ "$found" = 1 ] || echo "no backups in $BACKUP_DIR"
}

restore() {
    name="${1:-}"
    [ -n "$name" ] || { echo "usage: backup.sh restore NAME   (see: backup.sh list)"; exit 2; }
    src="$BACKUP_DIR/$name"
    [ -f "$src/COMPLETE" ] || { echo "backup $name not found or unfinished"; exit 2; }
    wait_db
    others="$(psql -XAtc "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND pid <> pg_backend_pid()")"
    if [ "$others" != "0" ]; then
        echo "The database is in use ($others connections). Stop the backend first:"
        echo "  docker compose stop backend"
        exit 3
    fi
    log "restoring database from $name"
    psql -X -v ON_ERROR_STOP=1 -qc "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
    pg_restore --no-owner --exit-on-error -d "$PGDATABASE" "$src/db.dump"
    if [ -f "$src/files.tar.gz" ]; then
        log "restoring files into $BLOBS_DIR"
        mkdir -p "$BLOBS_DIR"
        find "$BLOBS_DIR" -mindepth 1 -delete
        tar -xzf "$src/files.tar.gz" -C "$BLOBS_DIR"
        # The backend runs as uid 10001 (backend/Dockerfile).
        chown -R 10001 "$BLOBS_DIR" 2>/dev/null || true
    fi
    log "restore finished; start the backend: docker compose start backend"
}

loop() {
    log "nightly backups at ${BACKUP_HOUR}:00 ($TZ_NAME), keep $BACKUP_KEEP_DAYS days, into $BACKUP_DIR"
    mkdir -p "$BACKUP_DIR"
    # The backend (uid 10001, backend/Dockerfile) leaves REQUEST files here.
    chown 10001 "$BACKUP_DIR" 2>/dev/null || true
    while true; do
        heartbeat
        # A request from the portal («Сделать копию сейчас»).
        if [ -f "$BACKUP_DIR/REQUEST" ]; then
            log "backup requested from the portal: $(tr '\n' ' ' < "$BACKUP_DIR/REQUEST")"
            rm -f "$BACKUP_DIR/REQUEST"
            make_backup || true
            prune
            heartbeat
        fi
        if wait_db; then
            today="$(db_now 'YYYY-MM-DD')"
            hour="$(db_now 'HH24')"
            if [ "$hour" = "$BACKUP_HOUR" ] && ! ls -d "$BACKUP_DIR/$PREFIX$today"_* > /dev/null 2>&1; then
                make_backup || true
                prune
            fi
        fi
        sleep "${BACKUP_POLL_SECS:-60}"
    done
}

cmd="${1:-loop}"
if [ $# -gt 0 ]; then shift; fi
case "$cmd" in
    loop) loop ;;
    now) mkdir -p "$BACKUP_DIR"; make_backup; prune ;;
    once) mkdir -p "$BACKUP_DIR"; heartbeat; if [ -f "$BACKUP_DIR/REQUEST" ]; then rm -f "$BACKUP_DIR/REQUEST"; make_backup; prune; fi ;;
    list) list ;;
    restore) restore "$@" ;;
    *) echo "usage: backup.sh loop|now|once|list|restore NAME"; exit 2 ;;
esac
