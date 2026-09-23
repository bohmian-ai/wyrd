#!/usr/bin/env bash
# Own one complete, isolated Postgres lifecycle for the invoking command.
set -Eeuo pipefail

if [[ $# -lt 2 || $1 != "--" ]]; then
  echo "usage: $0 -- command [args ...]" >&2
  exit 64
fi
shift

readonly repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly compose_file="${WYRD_POSTGRES_COMPOSE_FILE:-$repo_root/docker-compose.yml}"
readonly compose_project="$(printf 'wyrd-test-%s-%s-%s' "$(basename "$repo_root")" "${PPID:-0}" "${BASHPID:-$$}" | tr '[:upper:]_./ ' '[:lower:]----' | tr -cd 'a-z0-9-' | cut -c1-48)"
readonly admin_password="${WYRD_TEST_POSTGRES_ADMIN_PASSWORD:-wyrd_test_admin_pw}"
readonly migrator_password="${WYRD_TEST_POSTGRES_MIGRATOR_PASSWORD:-wyrd_migrator_pw}"
readonly app_password="${WYRD_TEST_POSTGRES_APP_PASSWORD:-wyrd_app_pw}"
readonly platform_admin_password="${WYRD_TEST_POSTGRES_PLATFORM_ADMIN_PASSWORD:-wyrd_platform_admin_pw}"
readonly catalog_app_password="${WYRD_TEST_POSTGRES_CATALOG_APP_PASSWORD:-wyrd_catalog_app_pw}"
readonly compose=(docker compose --project-name "$compose_project" --file "$compose_file")

cleanup() {
  local status=$?
  trap - EXIT INT TERM
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || {
    echo "failed to tear down Postgres Compose project '$compose_project'" >&2
    [[ $status -eq 0 ]] && status=1
  }
  return "$status"
}

handle_signal() {
  local signal=$1
  trap - EXIT INT TERM
  if ! "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1; then
    echo "failed to tear down Postgres Compose project '$compose_project' after $signal" >&2
  fi
  kill -"$signal" "$$"
}

trap cleanup EXIT
trap 'handle_signal INT' INT
trap 'handle_signal TERM' TERM

# Bring the project up and return the host endpoint that actually accepts.
#
# `--wait` proves the server accepts TCP inside the container. It does not prove
# the published port is reachable from this host: under a VM-backed Docker
# (Colima/Lima, and Docker Desktop) the host side is an SSH forward that the VM
# manager creates asynchronously after the guest starts listening, so a healthy
# container is routinely reachable only seconds later -- and occasionally not at
# all, when that forwarder misses the guest port under churn. Waiting alone
# cannot fix the second case, so a project whose endpoint never opens is rebuilt
# once, which makes the forwarder observe a fresh guest port.
start_postgres() {
  # This function is called from an `if !` condition, which disables errexit for
  # everything it runs. Both commands must therefore report their own failure,
  # or a Compose that never came up would fall through to the endpoint probe.
  "${compose[@]}" up --detach --wait postgres || return 1
  endpoint="$("${compose[@]}" port --protocol tcp postgres 5432)" || return 1
  if [[ ! $endpoint =~ ^(127\.0\.0\.1|localhost|0\.0\.0\.0):([1-9][0-9]*)$ ]]; then
    echo "Docker returned an invalid loopback Postgres endpoint: '$endpoint'" >&2
    return 1
  fi
  host="${BASH_REMATCH[1]}"
  port="${BASH_REMATCH[2]}"
  if [[ $host == "0.0.0.0" ]]; then
    host=127.0.0.1
  fi
  for _ in $(seq 1 60); do
    if (exec 3<>"/dev/tcp/${host}/${port}") 2>/dev/null; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if ! start_postgres; then
  echo "Postgres endpoint ${host-unknown}:${port-unknown} never accepted a connection; rebuilding the project once" >&2
  "${compose[@]}" down --volumes --remove-orphans >/dev/null 2>&1 || true
  if ! start_postgres; then
    # An environment fault, not a test result, and it aborts the whole lane.
    # Print what the container and the host socket table say at the moment of
    # refusal so the next occurrence is diagnosable from the lane log.
    {
      echo "Postgres endpoint ${host-unknown}:${port-unknown} never accepted a connection"
      echo "--- compose ps"
      "${compose[@]}" ps --all
      echo "--- published ports"
      "${compose[@]}" port --protocol tcp postgres 5432 || true
      echo "--- host listeners on ${port-0}"
      (ss -lntp "sport = :${port-0}" || true) 2>/dev/null
      echo "--- postgres log (last 40)"
      "${compose[@]}" logs --tail 40 postgres || true
    } >&2
    exit 1
  fi
fi

admin_dsn="postgres://wyrd_test_admin:${admin_password}@${host}:${port}/wyrd"
export DATABASE_URL="postgres://wyrd_migrator:${migrator_password}@${host}:${port}/wyrd"
export WYRD_DATABASE_URL="postgres://wyrd_app:${app_password}@${host}:${port}/wyrd"
export WYRD_TEST_DATABASE_ADMIN_URL="$admin_dsn"
export WYRD_DATABASE_MIGRATOR_PASSWORD="$migrator_password"
export WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD="$platform_admin_password"
export WYRD_DATABASE_CATALOG_APP_PASSWORD="$catalog_app_password"
export WYRD_MIGRATOR_DSN="$DATABASE_URL"

# Journey and e2e lanes boot one WyrdTestServer per test, each retaining an
# app pool sized for a production server (PoolConfig::app_defaults, 32). A test
# fixture never needs that, and the per-server cost is what caps how many tests
# can run at once against one Postgres. Cap the app pool here; the migrator and
# platform-admin pools are already 2 and read their own suffixed vars.
export WYRD_DB_MAX_CONNECTIONS="${WYRD_DB_MAX_CONNECTIONS:-8}"

PGPASSWORD="$admin_password" psql "$admin_dsn" \
  --set=migrator_password="$migrator_password" \
  --set=app_password="$app_password" \
  --set=platform_admin_password="$platform_admin_password" \
  --set=catalog_app_password="$catalog_app_password" \
  --file="$repo_root/crates/wyrd/wyrd-sql/bootstrap/roles.sql"

set +e
"$@"
status=$?
set -e
exit "$status"
