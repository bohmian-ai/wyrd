#!/usr/bin/env bash
# `test:server:startup`: the official application image against a fresh
# external Postgres, driven the way an operator and a client use it.
#
# Cases (each is an assertion below, and the lane fails if any is skipped):
#   migration_refusal_and_retry  serve refuses an unmigrated, checksum-altered,
#                                RLS-deficient, policy-less, or over-granted
#                                schema (migrate refuses the last two); a failed migration
#                                exits nonzero, serve stays unready, and the
#                                retry succeeds without losing tenant data.
#   startup_image_journey        migrate, serve, idempotent setup, nginx routes
#                                (API, BFF, health, MCP stream, no authz check),
#                                public Rust SDK over HTTP and derived gRPC,
#                                restart into APP_ENV=production, persistence.
#
# Public gRPC is published on the fixed loopback port 50051 because the SDK
# derives it from the server URL; HTTP uses an ephemeral loopback port.
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly run_id="wyrd-startup-$$"
readonly tag="$run_id:test"
# Replaced by the immutable image ID the build produces; every container runs by ID.
image="$tag"
readonly net="$run_id" pg="$run_id-pg" app="$run_id-app" volume="$run_id-data" keys="$run_id-keys"
readonly owner_pw=owner_pw_startup app_pw=app_pw_startup platform_pw=platform_pw_startup
readonly owner_url="postgres://wyrd_owner:${owner_pw}@${pg}:5432/wyrd"
# Under the repository, not /tmp: VM-backed Docker (Colima, Docker Desktop)
# shares only the user's home into the VM, so /tmp bind mounts arrive empty.
mkdir -p "$root/target"
readonly work="$(mktemp -d "$root/target/$run_id.XXXXXX")"
passed=()

cleanup() {
  local status=$?
  if [[ $status -ne 0 ]]; then
    docker logs "$app" >"$root/target/$run_id-app.log" 2>&1 || true
    echo "--- app logs (full log: target/$run_id-app.log)"; tail -n 80 "$root/target/$run_id-app.log"
  fi
  docker rm -f "$app" "$pg" >/dev/null 2>&1 || true
  docker volume rm -f "$volume" "$keys" >/dev/null 2>&1 || true
  docker network rm "$net" >/dev/null 2>&1 || true
  docker image rm -f "$tag" >/dev/null 2>&1 || true
  rm -rf "$work" "$root/binary"
  exit "$status"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
psql_owner() { docker exec -i "$pg" psql -v ON_ERROR_STOP=1 -qAt -U wyrd_owner -d wyrd "$@"; }

serving_env=(
  -e "WYRD_DATABASE_URL=postgres://wyrd_app:${app_pw}@${pg}:5432/wyrd"
  -e "WYRD_PLATFORM_DATABASE_URL=postgres://wyrd_platform_admin:${platform_pw}@${pg}:5432/wyrd"
  -e WYRD_STORAGE_URL=file:///var/lib/wyrd/storage
  -e WYRD_SIGNING_KEY_FILE=/run/wyrd/signing.pem
  -e WYRD_SERVER_TENANT_SLUG=acme
  -e WYRD_OPERATOR_KEK_SOURCE=file
  -e WYRD_OPERATOR_KEK_DIR=/run/wyrd/kek
  ${WYRD_LOG:+-e "WYRD_LOG=$WYRD_LOG"}
)
serving_mounts=(-v "$volume:/var/lib/wyrd" -v "$keys:/run/wyrd:ro")

# One-off migration: the only process that ever receives the owner URL.
migrate() { docker run --rm --network "$net" -e "WYRD_DATABASE_URL=$owner_url" "$image" wyrd-server migrate; }

# Serve directly (no entrypoint) and require a refusal naming `$1`.
expect_serve_refusal() {
  local output
  if output="$(timeout 120 docker run --rm --network "$net" "${serving_env[@]}" "${serving_mounts[@]}" \
      "$image" wyrd-server 2>&1)"; then
    fail "serve started against a schema that should be refused ($1)"
  fi
  grep -q "$1" <<<"$output" || fail "serve refusal did not mention '$1': $output"
}

# Re-run the one-off migration and require its post-validation refusal naming `$1`.
expect_migrate_refusal() {
  local output
  if output="$(migrate 2>&1)"; then
    fail "migrate validated a schema that should be refused ($1)"
  fi
  grep -q "$1" <<<"$output" || fail "migrate refusal did not mention '$1': $output"
}

start_app() {
  docker run -d --name "$app" --network "$net" "${serving_env[@]}" "${serving_mounts[@]}" \
    -e "APP_ENV=$1" -p 127.0.0.1::8080 -p 127.0.0.1:50051:50051 "$image" >/dev/null
  [[ "$(docker inspect -f '{{.Image}}' "$app")" == "$image" ]] || fail "app is not running $image"
  http="http://$(docker port "$app" 8080/tcp | head -1)"
  for _ in $(seq 1 180); do
    curl -fsS "$http/readyz" >/dev/null 2>&1 && return 0
    docker inspect -f '{{.State.Running}}' "$app" | grep -q true || fail "app exited during start"
    sleep 1
  done
  fail "app never became ready through nginx: $(curl -sS "$http/readyz" 2>&1)"
}

sdk_phase() {
  WYRD_SERVER_URL="$http" WYRD_API_KEY="$api_key" WYRD_STARTUP_STATE_DIR="$work" \
    cargo nextest run --locked -p wyrd-client --test startup_image_journey \
    --run-ignored=only -E "test(=$1)"
}

echo "== build: server binary and the official image"
cargo build --locked -p wyrd-server --features cloud
mkdir -p "$root/binary"
cp "$root/target/debug/wyrd-server" "$root/binary/"
source_commit="$(git -C "$root" rev-parse HEAD)"
[[ -z "$(git -C "$root" status --porcelain --untracked-files=no)" ]] || source_commit+="+dirty"
image="$(docker build -q -f "$root/docker/official/Dockerfile" \
  --label "org.opencontainers.image.revision=$source_commit" -t "$tag" "$root")"
[[ $image == sha256:* ]] || fail "the official build reported no immutable image ID: $image"
echo "image: $image source: $source_commit" | tee "$root/target/startup-image-provenance.txt"
# The server reads only owner-only key files, so the signing key and the
# single-tenant deployment's file operator KEK are written into a volume as
# the image's serving user, the way mounted secrets are delivered.
docker volume create "$keys" >/dev/null
put_key() {
  docker run --rm -i -u root -v "$keys:/run/wyrd" "$image" sh -c \
    "mkdir -p /run/wyrd/kek && cat >/run/wyrd/$1 && chown -R wyrd:wyrd /run/wyrd && chmod 0600 /run/wyrd/$1"
}
openssl genpkey -algorithm ed25519 2>/dev/null | put_key signing.pem
openssl rand -base64 32 | tr -d '\n' | put_key kek/v1

echo "== fresh external Postgres with only the two serving roles"
docker network create "$net" >/dev/null
docker volume create "$volume" >/dev/null
docker run -d --name "$pg" --network "$net" -e POSTGRES_DB=wyrd -e POSTGRES_USER=wyrd_owner \
  -e "POSTGRES_PASSWORD=$owner_pw" --tmpfs /var/lib/postgresql/data postgres:16 >/dev/null
for _ in $(seq 1 60); do
  docker exec "$pg" pg_isready -h 127.0.0.1 -U wyrd_owner -d wyrd >/dev/null 2>&1 && break
  sleep 1
done
psql_owner --set "app_password=$app_pw" --set "platform_admin_password=$platform_pw" \
  <"$root/crates/wyrd/wyrd-sql/bootstrap/roles.sql" >/dev/null

echo "== migration_refusal_and_retry: refusals before and after migration"
expect_serve_refusal WYRD_SQL_503_SCHEMA_NOT_READY
migrate
checksum="$(psql_owner -c "SELECT encode(checksum, 'hex') FROM wyrd._sqlx_migrations ORDER BY version DESC LIMIT 1")"
psql_owner -c "UPDATE wyrd._sqlx_migrations SET checksum = '\\x00' WHERE version = (SELECT max(version) FROM wyrd._sqlx_migrations)"
expect_serve_refusal checksum
psql_owner -c "UPDATE wyrd._sqlx_migrations SET checksum = decode('$checksum', 'hex') WHERE version = (SELECT max(version) FROM wyrd._sqlx_migrations)"
rls_table="$(psql_owner -c "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'data_tenant_id' WHERE n.nspname = 'wyrd' AND c.relkind = 'r' ORDER BY 1 LIMIT 1")"
psql_owner -c "ALTER TABLE wyrd.$rls_table NO FORCE ROW LEVEL SECURITY"
expect_serve_refusal "does not force row-level security"
psql_owner -c "ALTER TABLE wyrd.$rls_table FORCE ROW LEVEL SECURITY"
psql_owner -c "ALTER POLICY tenant_isolation ON wyrd.$rls_table RENAME TO tenant_isolation_off"
expect_serve_refusal "does not force row-level security"
expect_migrate_refusal "does not force row-level security"
psql_owner -c "ALTER POLICY tenant_isolation_off ON wyrd.$rls_table RENAME TO tenant_isolation"
psql_owner -c "GRANT TRUNCATE ON wyrd.$rls_table TO wyrd_app"
expect_serve_refusal "holds TRUNCATE"
expect_migrate_refusal "holds TRUNCATE"
psql_owner -c "REVOKE TRUNCATE ON wyrd.$rls_table FROM wyrd_app"

echo "== startup_image_journey: serve, setup, routes"
start_app development
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$app" | grep -q "$owner_pw" \
  && fail "the owner credential reached the serving container"
setup_out="$(docker exec "$app" wyrd-server setup --tenant acme)"
api_key="$(sed -n 's/^admin_credential: //p' <<<"$setup_out")"
[[ -n $api_key ]] || fail "setup disclosed no tenant credential: $setup_out"
docker exec "$app" wyrd-server setup --tenant acme | grep -q "already set up" \
  || fail "second setup was not a no-op"

[[ "$(curl -fsS "$http/healthz")" == ok ]] || fail "healthz through nginx"
curl -fsS "$http/openapi.json" | grep -q '"/v1/cards"' || fail "openapi through nginx"
curl -fsS "$http/openapi.json" | grep -q 'authz/check' && fail "authz check still in OpenAPI"
bff="$(curl -fsS "$http/")"
grep -qi '<html' <<<"$bff" || fail "BFF did not serve the UI"
grep -q WYRD_SPEC_502_UPSTREAM_FAILURE <<<"$bff" && fail "BFF could not reach the Rust server"
token="$(curl -fsS -d grant_type=urn:ietf:params:oauth:grant-type:token-exchange \
  --data-urlencode "subject_token=$api_key" \
  -d subject_token_type=urn:wyrd:oauth:token-type:api_key "$http/auth/token" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["access_token"])')"
# `/v1` authenticates before routing, so only an authenticated probe can see 404.
authz_check="$(curl -s -w '\n%{http_code}' -X POST -H "x-wyrd-access-token: Bearer $token" \
  -H 'content-type: application/json' -d '{}' "$http/v1/authz/check")"
[[ "${authz_check##*$'\n'}" == 404 ]] || fail "/v1/authz/check is still routed: $authz_check"
mcp="$(curl -sS -N --max-time 30 -D - -H "x-wyrd-access-token: Bearer $token" \
  -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  -H 'mcp-method: server/discover' -H 'mcp-protocol-version: 2026-07-28' \
  -d '{"jsonrpc":"2.0","id":1,"method":"server/discover","params":{"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}}}' \
  "$http/mcp")"
grep -qi '^content-type: text/event-stream' <<<"$mcp" || fail "MCP did not stream: $mcp"
grep -q '"supportedVersions":\["2026-07-28"\]' <<<"$mcp" || fail "MCP discovery returned no supported version: $mcp"
sdk_phase startup_image_write
passed+=(startup_image_journey:write)

echo "== migration_refusal_and_retry: failed release migration on a live deployment"
docker rm -f "$app" >/dev/null
iceberg_version="$(psql_owner -c "SELECT version FROM vala._sqlx_migrations WHERE description = 'iceberg catalog'")"
[[ -n $iceberg_version ]] || fail "iceberg catalog migration not found in the ledger"
psql_owner -c "DELETE FROM vala._sqlx_migrations WHERE version = $iceberg_version"
psql_owner <<'SQL'
CREATE FUNCTION public.startup_fail_migration() RETURNS event_trigger LANGUAGE plpgsql AS $$
BEGIN
  IF current_query() ILIKE '%iceberg_catalog%' THEN
    RAISE EXCEPTION 'injected migration failure';
  END IF;
END $$;
CREATE EVENT TRIGGER startup_fail_migration ON ddl_command_start
  EXECUTE FUNCTION public.startup_fail_migration();
SQL
migrate && fail "the injected migration failure exited zero"
expect_serve_refusal WYRD_SQL_503_SCHEMA_NOT_READY
psql_owner -c "DROP EVENT TRIGGER startup_fail_migration; DROP FUNCTION public.startup_fail_migration()"
migrate
passed+=(migration_refusal_and_retry)

echo "== startup_image_journey: restart in the production profile"
start_app production
sdk_phase startup_image_verify
passed+=(startup_image_journey:verify)

[[ ${#passed[@]} -eq 3 ]] || fail "only ${passed[*]} ran"
echo "server startup lane: PASS (${passed[*]})"
