#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly wrapper="$root/scripts/postgres/with-test-postgres.sh"
readonly temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/wyrd-pg-contract.XXXXXX")"
mkdir -p "$temp_dir/bin"

# The wrapper TCP-probes the published endpoint before trusting it, so the fake
# publishes a real loopback listener that accepts and drops every connection.
python3 -c 'import socket, sys
s = socket.create_server(("127.0.0.1", 0))
open(sys.argv[1], "w").write(str(s.getsockname()[1]))
while True:
    s.accept()[0].close()' "$temp_dir/port.tmp" &
listener_pid=$!
trap 'kill "$listener_pid" 2>/dev/null; wait "$listener_pid" 2>/dev/null || true; rm -rf "$temp_dir"' EXIT
for _ in $(seq 1 100); do [[ -s $temp_dir/port.tmp ]] && break; sleep 0.05; done
FAKE_POSTGRES_PORT="$(cat "$temp_dir/port.tmp")"
export FAKE_POSTGRES_PORT

cat >"$temp_dir/bin/docker" <<'DOCKER'
#!/usr/bin/env bash
set -Eeuo pipefail
log="${FAKE_DOCKER_LOG:?}"
printf '%s\n' "$*" >>"$log"
if [[ " $* " == *" exec "* ]]; then
  cat >"${FAKE_PSQL_STDIN:?}"
  if [[ ${FAKE_PSQL_FAIL:-0} == 1 ]]; then exit 19; fi
  exit 0
fi
case "${*: -1}" in
  5432)
    if [[ ${FAKE_DOCKER_BAD_ENDPOINT:-0} == 1 ]]; then
      printf 'not-a-loopback-endpoint\n'
    else
      printf '127.0.0.1:%s\n' "${FAKE_POSTGRES_PORT:?}"
    fi
    ;;
esac
DOCKER
chmod +x "$temp_dir/bin/docker"
# A host psql that predates `\getenv`: bootstrap must never reach it.
cat >"$temp_dir/bin/psql" <<'PSQL'
#!/usr/bin/env bash
echo "host psql 14 must not run role bootstrap" >&2
exit 42
PSQL
chmod +x "$temp_dir/bin/psql"

export PATH="$temp_dir/bin:$PATH"
export FAKE_DOCKER_LOG="$temp_dir/docker.log"
export FAKE_PSQL_STDIN="$temp_dir/psql.stdin"

assert_contains() { grep -Fq -- "$2" "$1" || { echo "missing '$2' in $1" >&2; exit 1; }; }
assert_not_contains() { ! grep -Fq -- "$2" "$1" || { echo "unexpected '$2' in $1" >&2; exit 1; }; }

rm -f "$FAKE_DOCKER_LOG" "$FAKE_PSQL_STDIN"
env_capture="$temp_dir/env"
"$wrapper" -- bash -c 'printf "%s\n%s\n" "$DATABASE_URL" "$WYRD_DATABASE_URL" >"$1"' _ "$env_capture"
test "$(wc -l <"$env_capture" | tr -d ' ')" -eq 2
grep -Eq '^postgres://wyrd_test_admin:.*@127\.0\.0\.1:[1-9][0-9]*/wyrd$' "$env_capture"
grep -Eq '^postgres://wyrd_app:.*@127\.0\.0\.1:[1-9][0-9]*/wyrd$' "$env_capture"
up_project="$(awk '/ up / {for(i=1;i<=NF;i++) if($i=="--project-name") print $(i+1)}' "$FAKE_DOCKER_LOG")"
down_project="$(awk '/ down / {for(i=1;i<=NF;i++) if($i=="--project-name") print $(i+1)}' "$FAKE_DOCKER_LOG")"
test -n "$up_project" && test "$up_project" = "$down_project"
assert_contains "$FAKE_DOCKER_LOG" "exec -T postgres psql --username=wyrd_test_admin --dbname=wyrd --set=ON_ERROR_STOP=1 --set=app_password=wyrd_app_pw --set=platform_admin_password=wyrd_platform_admin_pw"
cmp -s "$FAKE_PSQL_STDIN" "$root/crates/wyrd/wyrd-sql/bootstrap/roles.sql" || { echo "container psql did not receive roles.sql on stdin" >&2; exit 1; }
assert_not_contains "$FAKE_DOCKER_LOG" "docker ps"
assert_not_contains "$FAKE_DOCKER_LOG" "docker rm"

set +e
"$wrapper" -- bash -c 'exit 37'
status=$?
set -e
test "$status" -eq 37

caller_marker="$temp_dir/caller-ran"
downs_before="$(grep -c ' down ' "$FAKE_DOCKER_LOG")"
set +e
FAKE_PSQL_FAIL=1 "$wrapper" -- touch "$caller_marker"
status=$?
set -e
test "$status" -eq 19
test ! -e "$caller_marker"
test "$(grep -c ' down ' "$FAKE_DOCKER_LOG")" -eq $((downs_before + 1))

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
