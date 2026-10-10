#!/usr/bin/env bash
set -Eeuo pipefail

readonly root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly wrapper="$root/scripts/postgres/with-test-postgres.sh"
command -v docker >/dev/null 2>&1 || { echo "docker is required for the role audit" >&2; exit 1; }
docker info >/dev/null 2>&1 || { echo "Docker daemon is unavailable" >&2; exit 1; }

"$wrapper" -- bash -c '
  set -Eeuo pipefail
  # The owner DSN is used here only to exercise drift repair; fixture
  # production code is the sole runtime owner of ephemeral database lifecycle.
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_app'\''")" = "t|f|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_platform_admin'\''")" = "t|t|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CONNECT'\''), has_database_privilege('\''wyrd_platform_admin'\'', '\''wyrd'\'', '\''CONNECT'\'')")" = "t|t"

  admin_password="${WYRD_TEST_POSTGRES_ADMIN_PASSWORD:-wyrd_test_admin_pw}"
  app_password="${WYRD_TEST_POSTGRES_APP_PASSWORD:-wyrd_app_pw}"
  platform_admin_password="${WYRD_TEST_POSTGRES_PLATFORM_ADMIN_PASSWORD:-wyrd_platform_admin_pw}"
  role_sql="${PWD}/crates/wyrd/wyrd-sql/bootstrap/roles.sql"
  PGPASSWORD="$admin_password" psql "$WYRD_TEST_DATABASE_ADMIN_URL" -v ON_ERROR_STOP=1 -c "CREATE ROLE wyrd_operator_fixture LOGIN BYPASSRLS PASSWORD '\''operator_fixture_pw'\''; GRANT wyrd_operator_fixture TO wyrd_app; GRANT CONNECT ON DATABASE wyrd TO wyrd_operator_fixture; ALTER ROLE wyrd_app CREATEDB BYPASSRLS; GRANT wyrd_app TO wyrd_platform_admin; REVOKE CONNECT ON DATABASE wyrd FROM wyrd_app; GRANT CREATE ON DATABASE wyrd TO wyrd_app;"
  # The production channel: both serving passwords through the environment,
  # run by the container psql so `\getenv` never depends on the host version.
  port="${DATABASE_URL##*:}"; port="${port%%/*}"
  container="$(docker ps --quiet --filter "publish=$port")"
  test -n "$container"
  WYRD_APP_PASSWORD="$app_password" WYRD_PLATFORM_ADMIN_PASSWORD="$platform_admin_password" \
    docker exec -i -e WYRD_APP_PASSWORD -e WYRD_PLATFORM_ADMIN_PASSWORD "$container" \
    psql --username=wyrd_test_admin --dbname=wyrd -v ON_ERROR_STOP=1 <"$role_sql"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_app'\''")" = "t|f|f|f"
  managed_memberships="$(psql "$DATABASE_URL" -Atqc "SELECT count(*) FROM pg_auth_members edge JOIN pg_roles granted ON granted.oid=edge.roleid JOIN pg_roles member ON member.oid=edge.member WHERE granted.rolname IN ('\''wyrd_app'\'','\''wyrd_platform_admin'\'') AND member.rolname IN ('\''wyrd_app'\'','\''wyrd_platform_admin'\'')")"
  test "$managed_memberships" = "0"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT pg_has_role('\''wyrd_app'\'', '\''wyrd_operator_fixture'\'', '\''member'\'')")" = "t"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT rolcanlogin, rolbypassrls, rolsuper, rolcreatedb FROM pg_roles WHERE rolname = '\''wyrd_operator_fixture'\''")" = "t|t|f|f"
  test "$(psql "$DATABASE_URL" -Atqc "SELECT has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CONNECT'\''), has_database_privilege('\''wyrd_app'\'', '\''wyrd'\'', '\''CREATE'\''), has_database_privilege('\''wyrd_operator_fixture'\'', '\''wyrd'\'', '\''CONNECT'\'')")" = "t|f|t"
  database_acl="$(psql "$DATABASE_URL" -Atqc "SELECT string_agg(role.rolname || '\'':'\'' || acl.privilege_type, '\'','\'' ORDER BY role.rolname, acl.privilege_type) FROM pg_database database CROSS JOIN LATERAL aclexplode(database.datacl) acl JOIN pg_roles role ON role.oid=acl.grantee WHERE database.datname='\''wyrd'\'' AND role.rolname IN ('\''wyrd_app'\'','\''wyrd_platform_admin'\'')")"
  test "$database_acl" = "wyrd_app:CONNECT,wyrd_platform_admin:CONNECT"
  PGPASSWORD="$app_password" psql "$WYRD_DATABASE_URL" -Atqc "SELECT 1" >/dev/null
  PGPASSWORD="$platform_admin_password" psql "$WYRD_PLATFORM_DATABASE_URL" -Atqc "SELECT 1" >/dev/null
  PGPASSWORD="$admin_password" psql "$WYRD_TEST_DATABASE_ADMIN_URL" -v ON_ERROR_STOP=1 -c "REVOKE wyrd_operator_fixture FROM wyrd_app; REVOKE ALL PRIVILEGES ON DATABASE wyrd FROM wyrd_operator_fixture; DROP ROLE wyrd_operator_fixture;"
'
echo "postgres role audit: PASS"
