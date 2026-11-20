#!/usr/bin/env bash
# Run every end-to-end test in scripts/*_test.py against a running backend and
# fail if any of them fails. Used by CI (.github/workflows/ci.yml) and locally.
#
#   API=http://localhost:8080 WEB=http://localhost:8090 scripts/run_e2e.sh [test ...]
#
# The backend must be started with the same test settings as in CI
# (DOCS_JWT_SECRET, BACKUP_DIR, BACKUP_PORTAL, BACKUP_ALERT_SECS, ...);
# onlyoffice_live_test.py needs a real Document Server and is skipped.
set -u
cd "$(dirname "$0")/.."
tests=("$@")
[ ${#tests[@]} -eq 0 ] && tests=(scripts/*_test.py)
failed=()
for t in "${tests[@]}"; do
  [ "$t" = scripts/onlyoffice_live_test.py ] && continue
  start=$(date +%s)
  if out=$(timeout 600 python3 "$t" 2>&1); then
    echo "PASS $t ($(( $(date +%s) - start )) s): $(echo "$out" | tail -1)"
  else
    echo "FAIL $t"
    echo "$out" | tail -25 | sed 's/^/    /'
    failed+=("$t")
  fi
done
if [ ${#failed[@]} -gt 0 ]; then
  echo "Failed: ${failed[*]}"
  exit 1
fi
echo "All end-to-end tests passed"
