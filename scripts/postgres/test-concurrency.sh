#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly wrapper="$root/scripts/postgres/with-test-postgres.sh"
command -v docker >/dev/null 2>&1 || { echo "docker is required for the concurrency journey" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "Docker daemon is unavailable" >&2; exit 1; }
readonly temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/wyrd-pg-concurrency.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT

run_one() {
  local output=$1
  "$wrapper" -- bash -c 'printf "%s\n" "$DATABASE_URL" >"$1"; psql "$DATABASE_URL" -Atqc "SELECT 1"; sleep 30' _ "$output"
}

run_one "$temp_dir/one" & one_pid=$!
run_one "$temp_dir/two" & two_pid=$!
for _ in $(seq 1 120); do
  [[ -s "$temp_dir/one" && -s "$temp_dir/two" ]] && break
  sleep 0.25
done
test -s "$temp_dir/one" && test -s "$temp_dir/two"
one_url=$(cat "$temp_dir/one")
two_url=$(cat "$temp_dir/two")
test "$one_url" != "$two_url"
kill "$one_pid"
set +e
wait "$one_pid"
one_status=$?
set -e
test "$one_status" -ne 0
PGPASSWORD=wyrd_migrator_pw psql "$two_url" -Atqc 'SELECT 1' | grep -qx 1
kill "$two_pid"
set +e
wait "$two_pid"
two_status=$?
set -e
test "$two_status" -ne 0
echo "postgres wrapper concurrency: PASS ($one_url, $two_url)"
