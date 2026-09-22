#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly wrapper="$root/scripts/postgres/with-test-postgres.sh"
readonly temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/wyrd-pg-contract.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT
mkdir -p "$temp_dir/bin"

cat >"$temp_dir/bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -Eeuo pipefail
log="${FAKE_DOCKER_LOG:?}"
printf '%s\n' "$*" >>"$log"
project=default
for ((i=1; i<=$#; i++)); do
  if [[ ${!i} == --project-name ]]; then
    j=$((i + 1)); project=${!j}
  fi
done
case "${*: -1}" in
  5432)
    if [[ ${FAKE_DOCKER_BAD_ENDPOINT:-0} == 1 ]]; then
      printf 'not-a-loopback-endpoint\n'
    else
      printf '127.0.0.1:%s\n' "$((30000 + $(printf '%s' "$project" | cksum | cut -d' ' -f1) % 20000))"
    fi
    ;;
esac
DOCKER
chmod +x "$temp_dir/bin/docker"
cat >"$temp_dir/bin/psql" <<'PSQL'
#!/usr/bin/env bash
set -Eeuo pipefail
printf '%s\n' "$*" >>"${FAKE_PSQL_LOG:?}"
if [[ ${FAKE_PSQL_FAIL:-0} == 1 ]]; then exit 19; fi
PSQL
chmod +x "$temp_dir/bin/psql"

export PATH="$temp_dir/bin:$PATH"
export FAKE_DOCKER_LOG="$temp_dir/docker.log"
export FAKE_PSQL_LOG="$temp_dir/psql.log"

assert_contains() { grep -Fq -- "$2" "$1" || { echo "missing '$2' in $1" >&2; exit 1; }; }
assert_not_contains() { ! grep -Fq -- "$2" "$1" || { echo "unexpected '$2' in $1" >&2; exit 1; }; }

rm -f "$FAKE_DOCKER_LOG" "$FAKE_PSQL_LOG"
env_capture="$temp_dir/env"
"$wrapper" -- bash -c 'printf "%s\n%s\n" "$DATABASE_URL" "$WYRD_DATABASE_URL" >"$1"' _ "$env_capture"
test "$(wc -l <"$env_capture" | tr -d ' ')" -eq 2
grep -Eq '^postgres://wyrd_migrator:.*@127\.0\.0\.1:[1-9][0-9]*/wyrd$' "$env_capture"
grep -Eq '^postgres://wyrd_app:.*@127\.0\.0\.1:[1-9][0-9]*/wyrd$' "$env_capture"
up_project="$(awk '/ up / {for(i=1;i<=NF;i++) if($i=="--project-name") print $(i+1)}' "$FAKE_DOCKER_LOG")"
down_project="$(awk '/ down / {for(i=1;i<=NF;i++) if($i=="--project-name") print $(i+1)}' "$FAKE_DOCKER_LOG")"
test -n "$up_project" && test "$up_project" = "$down_project"
assert_contains "$FAKE_PSQL_LOG" "--file=$root/crates/wyrd/wyrd-sql/bootstrap/roles.sql"
assert_not_contains "$FAKE_DOCKER_LOG" "docker ps"
assert_not_contains "$FAKE_DOCKER_LOG" "docker rm"

set +e
"$wrapper" -- bash -c 'exit 37'
status=$?
set -e
test "$status" -eq 37

caller_marker="$temp_dir/caller-ran"
set +e
FAKE_PSQL_FAIL=1 "$wrapper" -- touch "$caller_marker"
status=$?
set -e
test "$status" -eq 19
test ! -e "$caller_marker"

set +e
FAKE_DOCKER_BAD_ENDPOINT=1 "$wrapper" -- touch "$caller_marker"
status=$?
set -e
test "$status" -ne 0
test ! -e "$caller_marker"

set +e
"$wrapper" -- sleep 20 &
wrapper_pid=$!
for _ in $(seq 1 50); do grep -q ' up ' "$FAKE_DOCKER_LOG" 2>/dev/null && break; sleep 0.02; done
kill -TERM "$wrapper_pid"
wait "$wrapper_pid"
status=$?
set -e
test "$status" -eq 143
test "$(grep -c ' down ' "$FAKE_DOCKER_LOG")" -ge 1

echo "postgres wrapper contract: PASS"
