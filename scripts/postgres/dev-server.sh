#!/usr/bin/env bash
# Serve a built wyrd-server against an ephemeral local Postgres.
#
# The wrapper bootstraps the two serving roles; the database owner then runs
# `wyrd-server migrate` exactly as a deployment does, and the server serves with
# only the wyrd_app and wyrd_platform_admin URLs. The database is discarded on exit.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$root/scripts/postgres/with-test-postgres.sh" -- bash -c '
  set -Eeuo pipefail
  WYRD_DATABASE_URL="$WYRD_TEST_DATABASE_ADMIN_URL" ./target/debug/wyrd-server migrate
  ./target/debug/wyrd-server "$@"
' _ "$@"
