#!/usr/bin/env bash
# `test:identity:ui:container`: real provider sign-in through the official
# Debian image's nginx public origin. Run inside
# `scripts/identity/with-identity-providers.sh`, which owns Keycloak and Dex.
#
# Builds the official image and a fresh external Postgres (startup-common.sh),
# serves the team deployment, sets up tenant `acme`, and joins the app to the
# providers' compose network. A forwarder from the same image shares the app's
# network namespace and serves the providers at the very URLs the host uses
# (`localhost:8180`, `localhost:5556`), so the server and the browser see one
# issuer. Then `identity_ui_e2e::production_ui_nginx_journey` drives the
# browser flow at nginx's origin.
startup_journey=identity
source "$(dirname "${BASH_SOURCE[0]}")/startup-common.sh"
readonly app_pw=app_pw_startup platform_pw=platform_pw_startup forwarder="$app-idp"
# Forwarder removal is best effort; the shared cleanup always runs with the original status.
trap 'status=$?; docker rm -f "$forwarder" >/dev/null 2>&1 || true; cleanup "$status"' EXIT

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

# Pipes each connection on a loopback port to `host:port` on the compose network.
readonly forward_js="\
const net = require('node:net');\
for (const [listen, host, port] of [[8180, 'keycloak', 8080], [5556, 'dex', 5556]])\
  net.createServer((client) => {\
    const upstream = net.connect(port, host);\
    client.on('error', () => upstream.destroy());\
    upstream.on('error', () => client.destroy());\
    client.pipe(upstream).pipe(client);\
  }).listen(listen, '127.0.0.1');"

echo "== a DBA-created platform and tenant login, migrated"
{
  printf "CREATE ROLE wyrd_platform LOGIN PASSWORD '%s';\n" "$platform_pw"
  printf "CREATE ROLE wyrd_tenant LOGIN PASSWORD '%s';\n" "$app_pw"
  printf 'ALTER DATABASE wyrd OWNER TO wyrd_platform;\n'
  cat "$root/scripts/postgres/test-database-setup.sql"
} | psql_owner >/dev/null
docker run --rm --network "$net" "${serving_env[@]:0:4}" "$image" wyrd-server migrate

echo "== serve, set up acme, reach the providers"
start_app development
api_key="$(docker exec "$app" wyrd-server setup --tenant acme | sed -n 's/^admin_credential: //p')"
[[ -n $api_key ]] || fail "setup disclosed no tenant credential"
providers="$(docker inspect -f '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}}{{end}}' \
  "$(cd "$root" && docker compose ps -q keycloak)")"
docker network connect "$providers" "$app"
docker run -d --name "$forwarder" --network "container:$app" --entrypoint node "$image" \
  -e "$forward_js" >/dev/null

echo "== production_ui_nginx_journey at $http"
cd "$root"
WYRD_UI_JOURNEY="$(printf '{"origin":"%s","admin_key":"%s","app":"%s","forwarder":"%s"}' \
  "$http" "$api_key" "$app" "$forwarder")" \
  cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture \
  -E 'test(=identity_ui_e2e::production_ui_nginx_journey)'
echo "identity UI container lane: PASS (image $image)"
