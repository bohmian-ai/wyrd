#!/usr/bin/env bash
# `test:server:startup:prod`: the team deployment from the Operate guide, run
# against the official image: a DBA-created platform and tenant login, driven
# the way an operator and a client use it.
#
# Cases (each is an assertion below, and the lane fails if any is skipped):
#   migration_refusal_and_retry  serve refuses an unmigrated, checksum-altered,
#                                RLS-deficient, or policy-less schema and an
#                                over-granted tenant login (migrate refuses the
#                                policy-less schema); a failed migration
#                                exits nonzero, serve stays unready, and the
#                                retry succeeds without losing tenant data.
#   startup_image_journey        migrate, serve, idempotent setup, nginx routes
#                                (API, BFF, health, MCP stream, no authz check),
#                                public Rust SDK over HTTP and derived gRPC,
#                                restart into APP_ENV=production, persistence.
#   authenticated_analytical_bff the image's production Node BFF, signed in
#                                with an API key, renders a Service page from
#                                evidence two Services emitted through the
#                                public SDK, read by its installed native SDK
#                                over the default HTTP and gRPC endpoints; an
#                                unreachable analytical endpoint renders a
#                                truthful failure with no fixture fallback.
#
# `WYRD_STARTUP_FILTER=<case>` runs exactly that case and the setup it needs;
# an unknown selector fails before the build.
#
# Public gRPC is published on the fixed loopback port 50051 because the SDK
# derives it from the server URL; HTTP uses an ephemeral loopback port.
readonly cases=(migration_refusal_and_retry startup_image_journey authenticated_analytical_bff)
readonly filter="${WYRD_STARTUP_FILTER:-}"
if [[ -n $filter ]] && ! printf '%s\n' "${cases[@]}" | grep -qxF -- "$filter"; then
  echo "FAIL: WYRD_STARTUP_FILTER=$filter is not one of: ${cases[*]}" >&2
  exit 1
fi
selected() { [[ -z $filter || $filter == "$1" ]]; }
startup_journey=prod
source "$(dirname "${BASH_SOURCE[0]}")/startup-common.sh"
readonly app_pw=app_pw_startup platform_pw=platform_pw_startup
readonly ui_dir="$root/crates/wyrd/wyrd-server/wyrd-ui"

serving_env=(
  -e "WYRD_DATABASE_URL=postgres://wyrd_tenant:${app_pw}@${pg}:5432/wyrd"
  -e "WYRD_PLATFORM_DATABASE_URL=postgres://wyrd_platform:${platform_pw}@${pg}:5432/wyrd"
  -e WYRD_STORAGE_URL=file:///var/lib/wyrd/storage
  -e WYRD_SIGNING_KEY_FILE=/run/wyrd/signing.pem
  -e WYRD_SERVER_TENANT_SLUG=acme
  -e WYRD_OPERATOR_KEK_SOURCE=file
  -e WYRD_OPERATOR_KEK_DIR=/run/wyrd/kek
  ${WYRD_LOG:+-e "WYRD_LOG=$WYRD_LOG"}
)

# One-off migration as the platform login, which then owns Wyrd's objects.
migrate() { docker run --rm --network "$net" "${serving_env[@]:0:4}" "$image" wyrd-server migrate; }

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

sdk_phase() {
  WYRD_SERVER_URL="$http" WYRD_API_KEY="$api_key" WYRD_STARTUP_STATE_DIR="$work" \
    cargo nextest run --locked -p wyrd-client --test integration \
    --run-ignored=only -E "test(=startup_image_journey::$1)"
}

echo "== a DBA-created platform and tenant login"
{
  printf "CREATE ROLE wyrd_platform LOGIN PASSWORD '%s';\n" "$platform_pw"
  printf "CREATE ROLE wyrd_tenant LOGIN PASSWORD '%s';\n" "$app_pw"
  printf 'ALTER DATABASE wyrd OWNER TO wyrd_platform;\n'
  cat "$root/scripts/postgres/test-database-setup.sql"
} | psql_owner >/dev/null

if selected migration_refusal_and_retry; then
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
psql_owner -c "GRANT TRUNCATE ON wyrd.$rls_table TO wyrd_tenant"
expect_serve_refusal "holds TRUNCATE"
psql_owner -c "REVOKE TRUNCATE ON wyrd.$rls_table FROM wyrd_tenant"
else
  migrate
fi

# Set the tenant up, keeping the administrator key it discloses the first time.
setup_tenant() {
  setup_out="$(docker exec "$app" wyrd-server setup --tenant acme)"
  api_key="$(sed -n 's/^admin_credential: //p' <<<"$setup_out")"
  [[ -n $api_key ]] || fail "setup disclosed no tenant credential: $setup_out"
}

if selected startup_image_journey; then
echo "== startup_image_journey: serve, setup, routes"
start_app development
docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' "$app" | grep -q "$owner_pw" \
  && fail "the DBA credential reached the serving container"
setup_tenant
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
fi

if selected migration_refusal_and_retry; then
echo "== migration_refusal_and_retry: failed release migration on a live deployment"
docker rm -f "$app" >/dev/null 2>&1 || true
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
fi

if selected startup_image_journey; then
echo "== startup_image_journey: restart in the production profile"
docker rm -f "$app" >/dev/null 2>&1 || true
start_app production
sdk_phase startup_image_verify
passed+=(startup_image_journey:verify)
fi

# Sign in through the BFF's API-key recovery form into the cookie jar `$jar`.
bff_signin() {
  public="${http/127.0.0.1/localhost}"
  jar="$work/bff-cookies"
  local signin
  signin="$(curl -sS -o "$work/signin.html" -w '%{http_code} %{redirect_url}' -c "$jar" \
    -H "origin: $public" -H 'accept: text/html' \
    --data-urlencode "apiKey=$api_key" "$public/t/acme/login/api-key")"
  [[ $signin == "303 $public/t/acme" ]] \
    || fail "API-key sign-in through the BFF did not redirect home: $signin $(grep -oE 'WYRD_[A-Z0-9_]+' "$work/signin.html" | sort -u | tr '\n' ' ')"
  grep -q wyrd_session_acme "$jar" || fail "the BFF issued no session cookie"
}
# The BFF's Service page for `$1` (a UID plus query), as the signed-in browser in `$jar`.
bff_page() { curl -fsS -b "$jar" "$public/t/acme/cards/$1"; }
# Field `$2` of workload Service `$1`.
workload_field() { python3 -c 'import json,sys; print(json.loads(sys.argv[1])[int(sys.argv[2])][sys.argv[3]])' "$workload" "$1" "$2"; }

if selected authenticated_analytical_bff; then
echo "== authenticated_analytical_bff: the production BFF's live analytical read"
docker rm -f "$app" >/dev/null 2>&1 || true
start_app production
[[ -n ${api_key:-} ]] || setup_tenant
# The platform package is not hoisted: it resolves from the SDK's own install, as the SDK loads it.
docker exec "$app" sh -c 'cd /app/ui && node --version && node --input-type=module -e "
  import { createRequire } from \"node:module\";
  const sdk = import.meta.resolve(\"@wyrd/sdk\");
  console.log(\"native:\", createRequire(sdk).resolve(\"@wyrd/sdk-linux-x64-gnu\"));
  const m = await import(\"@wyrd/sdk\");
  console.log(\"@wyrd/sdk loaded:\", typeof m.Bifrost);"' \
  | tee "$root/target/startup-prod-bff-layout.txt" || fail "the image BFF cannot load its native SDK"
grep -q '@wyrd/sdk loaded: function' "$root/target/startup-prod-bff-layout.txt" || fail "the image BFF SDK has no Bifrost"
# Two Services' evidence, written only through the public SDK and OTLP, with bounded readiness.
pnpm --dir "$ui_dir" install --frozen-lockfile >/dev/null
workload="$(cd "$ui_dir" && WYRD_SERVER_URL="$http" WYRD_API_KEY="$api_key" node --input-type=module -e '
  import { WyrdClient } from "@wyrd/sdk";
  import { serviceWorkload } from "./src/lib/server/auth/service-workload.ts";
  const { WYRD_SERVER_URL: serverUrl, WYRD_API_KEY: credential } = process.env;
  const admin = WyrdClient.connect({ serverUrl, credential, grpcUrl: "http://127.0.0.1:50051" });
  const services = await serviceWorkload(admin);
  console.log(JSON.stringify(services.map(({ name, uid, recentSpans }) => ({ name, uid, recentSpans }))));
  process.exit(0);
' | tail -n 1)"
spans="$(workload_field 0 recentSpans)"
bff_signin
html="$(bff_page "$(workload_field 0 uid)?range=1h")"
for text in '✓ VERIFICATION PASSING' "$spans root spans · 1h" 'WL-ANSWER 1.0.0 → agent'; do
  grep -qF "$text" <<<"$html" || fail "the Service page lacks '$text'"
done
for text in "$(workload_field 1 name)" '▲ VERIFICATION FAILING' checkout-api WYRD_UI_ANALYTICAL; do
  grep -qF "$text" <<<"$html" && fail "the Service page shows '$text' outside its scope"
done
grep -qF "$((spans + 1)) root spans · 6h" <<<"$(bff_page "$(workload_field 0 uid)?range=6h")" \
  || fail "the 6h range does not include the older root span"
grep -qF '▲ VERIFICATION FAILING' <<<"$(bff_page "$(workload_field 1 uid)?range=1h")" \
  || fail "the failing Service is not shown failing"
echo "== authenticated_analytical_bff: unreachable analytical endpoint"
docker rm -f "$app" >/dev/null
serving_env+=(-e WYRD_GRPC_URL=http://127.0.0.1:1)
start_app production
bff_signin
html="$(bff_page "$(workload_field 0 uid)?range=1h")"
for text in 'ASSESSMENT INCOMPLETE' 'unavailable · WYRD_' 'trace read failed — not zero'; do
  grep -qF "$text" <<<"$html" || fail "the unavailable read is not truthful: lacks '$text'"
done
for text in '✓ VERIFICATION PASSING' 'root spans · 1h' checkout-api; do
  grep -qF "$text" <<<"$html" && fail "the unavailable read shows '$text'"
done
passed+=(authenticated_analytical_bff)
fi

case $filter in
  '') expected=4 ;;
  startup_image_journey) expected=2 ;;
  *) expected=1 ;;
esac
[[ ${#passed[@]} -eq $expected ]] || fail "only ${passed[*]:-nothing} ran"
echo "server startup prod lane: PASS (${passed[*]})"
