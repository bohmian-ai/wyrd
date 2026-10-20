#!/usr/bin/env bash
# `test:server:startup:dev`: the development setup from the Get started guide,
# run against the official image. One ordinary login named `wyrd` owns its
# database; migrate, serve, and setup run with only WYRD_DATABASE_URL, and
# storage defaults to `.wyrd/storage` under the working directory. The login shares its name with Wyrd's `wyrd` schema, which
# puts that schema on its default search path. The tenant key then signs in
# to the production UI through nginx, and killing the UI stops the container.
startup_journey=dev
source "$(dirname "${BASH_SOURCE[0]}")/startup-common.sh"
readonly dev_pw=dev_pw_startup

psql_owner -c "CREATE ROLE wyrd LOGIN PASSWORD '$dev_pw'" -c "CREATE DATABASE wyrd_dev OWNER wyrd"
serving_env=(
  -e "WYRD_DATABASE_URL=postgres://wyrd:${dev_pw}@${pg}:5432/wyrd_dev"
  ${WYRD_LOG:+-e "WYRD_LOG=$WYRD_LOG"}
)

echo "== dev journey: migrate, serve, setup"
docker run --rm --network "$net" "${serving_env[@]:0:2}" "$image" wyrd-server migrate
start_app development
api_key="$(docker exec "$app" wyrd-server setup --tenant acme | sed -n 's/^admin_credential: //p')"
[[ -n $api_key ]] || fail "setup disclosed no tenant credential"

echo "== dev journey: sign in to the UI, then lose the UI"
ui_sign_in "$api_key"
ui_exit_stops_app
echo "server startup dev lane: PASS"
