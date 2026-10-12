#!/usr/bin/env bash
# Serve a built wyrd-server against an ephemeral local Postgres.
#
# The wrapper creates a platform login that owns the database and a separate
# tenant login, as a DBA would; `wyrd-server migrate` then runs as the platform
# login exactly as a deployment does, and the server serves with both URLs.
# The database is discarded on exit.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$root/scripts/postgres/with-test-postgres.sh" -- bash -c '
  set -Eeuo pipefail
  ./target/debug/wyrd-server migrate
  ./target/debug/wyrd-server "$@"
' _ "$@"
