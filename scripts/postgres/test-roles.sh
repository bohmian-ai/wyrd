#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly wrapper="$root/scripts/postgres/with-test-postgres.sh"
command -v docker >/dev/null 2>&1 || { echo "docker is required for the role audit" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "Docker daemon is unavailable" >&2; exit 1; }

"$wrapper" -- bash -c '
  set -Eeuo pipefail
  # The neutral admin DSN is used here only to exercise drift repair; fixture
  # production code is the sole runtime owner of ephemeral database lifecycle.
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_migrator'\''")" = "t|t|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_app'\''")" = "t|f|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_platform_admin'\''")" = "t|t|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_catalog'\''")" = "f|f|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT pg_has_role('\''wyrd_catalog_app'\'', '\''wyrd_catalog'\'', '\''member'\'')")" = "t"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CONNECT'\''), has_database_privilege('\''wyrd_migrator'\'', '\''wyrd'\'', '\''CREATE'\'' )")" = "t|t"
  catalog_url="postgres://wyrd_catalog_app:${WYRD_DATABASE_CATALOG_APP_PASSWORD}@${DATABASE_URL#*@}"
  PGPASSWORD="$WYRD_DATABASE_CATALOG_APP_PASSWORD" psql "$catalog_url" -Atqc "SELECT 1" >/dev/null

  admin_password="${WYRD_TEST_POSTGRES_ADMIN_PASSWORD:-wyrd_test_admin_pw}"
  role_sql="${PWD}/crates/wyrd/wyrd-sql/bootstrap/roles.sql"
  PGPASSWORD="$admin_password" psql "$WYRD_TEST_DATABASE_ADMIN_URL" -v ON_ERROR_STOP=1 -c "CREATE ROLE wyrd_operator_fixture LOGIN PASSWORD '\''operator_fixture_pw'\''; GRANT wyrd_operator_fixture TO wyrd_app; GRANT CONNECT ON DATABASE wyrd TO wyrd_operator_fixture; ALTER ROLE wyrd_app CREATEDB BYPASSRLS; REVOKE wyrd_catalog FROM wyrd_migrator; GRANT wyrd_app TO wyrd_catalog; REVOKE CONNECT ON DATABASE wyrd FROM wyrd_app; GRANT CREATE ON DATABASE wyrd TO wyrd_app;"
  PGPASSWORD="$admin_password" psql "$WYRD_TEST_DATABASE_ADMIN_URL" \
    --set=migrator_password="$WYRD_DATABASE_MIGRATOR_PASSWORD" \
    --set=app_password="${WYRD_TEST_POSTGRES_APP_PASSWORD:-wyrd_app_pw}" \
    --set=platform_admin_password="$WYRD_DATABASE_PLATFORM_ADMIN_PASSWORD" \
    --set=catalog_app_password="$WYRD_DATABASE_CATALOG_APP_PASSWORD" \
    --file="$role_sql"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_app'\''")" = "t|f|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT pg_has_role('\''wyrd_migrator'\'', '\''wyrd_catalog'\'', '\''member'\''), pg_has_role('\''wyrd_app'\'', '\''wyrd_catalog'\'', '\''member'\''), pg_has_role('\''wyrd_app'\'', '\''wyrd_operator_fixture'\'', '\''member'\'')")" = "t|f|t"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CONNECT'\''), has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CREATE'\''), has_database_privilege('\''wyrd_operator_fixture'\'', '\''wyrd'\'', '\''CONNECT'\'')")" = "t|f|t"
  PGPASSWORD="$admin_password" psql "$WYRD_TEST_DATABASE_ADMIN_URL" -v ON_ERROR_STOP=1 -c "REVOKE wyrd_operator_fixture FROM wyrd_app; REVOKE ALL PRIVILEGES ON DATABASE wyrd FROM wyrd_operator_fixture; DROP ROLE wyrd_operator_fixture;"
'
echo "postgres role audit: PASS"
