#!/usr/bin/env bash
# `test:server:startup:dev`: the development setup from the Get started guide,
# run against the official image. One ordinary login named `wyrd` owns its
# database; migrate, serve, and setup run with only WYRD_DATABASE_URL, and
# storage defaults to `.wyrd/storage` under the working directory. The login shares its name with Wyrd's `wyrd` schema, which
# puts that schema on its default search path.
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
docker exec "$app" wyrd-server setup --tenant acme | grep -q '^admin_credential: ' \
  || fail "setup disclosed no tenant credential"
echo "server startup dev lane: PASS"
