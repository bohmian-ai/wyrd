#!/usr/bin/env bash
# Serve a built wyrd-server against an ephemeral local Postgres.
#
# The wrapper creates a platform login that owns the database and a separate
# tenant login, as a DBA would; `wyrd-server migrate` then runs as the platform
# login exactly as a deployment does, and the server serves with both URLs.
# Once it is ready, `wyrd-server setup` creates the `dev` tenant and its admin
# key replaces the Wyrd config directory's `credentials.toml`, where every SDK
# and CLI process finds it; the client's default URLs reach this server.
# The database is discarded on exit, and the server's `.wyrd` data is removed
# before each start so a fresh database never replays an old Bifrost log.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$root/scripts/postgres/with-test-postgres.sh" -- bash -c '
  set -Eeuo pipefail
  rm -rf .wyrd
  ./target/debug/wyrd-server migrate
  ./target/debug/wyrd-server "$@" &
  server=$!
  trap "kill -INT $server 2>/dev/null; wait $server" EXIT INT TERM
  until curl -sf http://127.0.0.1:8080/readyz >/dev/null; do
    kill -0 "$server" 2>/dev/null || exit 1
    sleep 1
  done
  key=$(./target/debug/wyrd-server setup --tenant dev | sed -n "s/^admin_credential: //p")
  config=${WYRD_CONFIG_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}/wyrd}
  mkdir -p "$config"
  printf "[default]\napi_key = \"%s\"\n" "$key" > "$config/credentials.toml"
  chmod 600 "$config/credentials.toml"
  printf "\nwyrd-server ready on http://127.0.0.1:8080; tenant dev key saved to %s\n\n" "$config/credentials.toml"
  wait "$server"
' _ "$@"
