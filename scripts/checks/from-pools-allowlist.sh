#!/usr/bin/env bash
# WHY THIS FILE EXISTS: from_pools hands out a raw PgPool that bypasses
# per-tenant connection isolation. Every authorized call site has been audited
# to ensure it holds a validated tenant context before touching the pool. An
# unauthorized call site — even a well-intentioned helper — can silently
# process one tenant's data under another's context, which is a data-isolation
# violation. The allowlist is the enforcement boundary; adding a site requires
# an explicit audit, not just a passing test.
#
# WHAT IT CHECKS: from_pools (both method and associated-function call forms)
# does not appear outside the 2 definition files, 9 production wiring sites,
# and 7 test sites listed in the glob exclusions below.
set -e
# from_pools may only be called from the sites below.
# 2 definition files: wyrd-sql/src/postgres.rs, vala-sql/src/postgres.rs
# 9 production wiring files: wyrd-server/src/{state,boot/mod,http/middleware/authenticate,components/{auth/{principal_extractor,caller_extractor,policy_hook,routes},health},postgres}.rs
# 3 in-src #[cfg(test)] sites: wyrd-server/src/{bifrost/service,query/service,app/server}.rs
# 5 integration test files: wyrd-server/tests/{pg_authz_check_route,pg_grpc_smoke,pg_router_smoke,pg_grpc_ingest_smoke,pg_merge_http_protected}.rs
# 1 Vala test file: vala-bifrost-redux/tests/pg_scribe_seal.rs
# Harness audited FD-009: constructs ValaPostgres for test fixture only.
! rg -n --no-heading -e '\.from_pools\(|::from_pools\(' \
    --glob '!crates/wyrd/wyrd-sql/src/postgres.rs' \
    --glob '!crates/vala/vala-sql/src/postgres.rs' \
    --glob '!crates/wyrd/wyrd-server/src/state.rs' \
    --glob '!crates/wyrd/wyrd-server/src/boot/mod.rs' \
    --glob '!crates/wyrd/wyrd-server/src/http/middleware/authenticate.rs' \
    --glob '!crates/wyrd/wyrd-server/src/components/auth/principal_extractor.rs' \
    --glob '!crates/wyrd/wyrd-server/src/components/auth/caller_extractor.rs' \
    --glob '!crates/wyrd/wyrd-server/src/components/auth/policy_hook.rs' \
    --glob '!crates/wyrd/wyrd-server/src/components/auth/routes.rs' \
    --glob '!crates/wyrd/wyrd-server/src/components/health/mod.rs' \
    --glob '!crates/wyrd/wyrd-server/src/postgres.rs' \
    --glob '!crates/wyrd/wyrd-server/src/bifrost/service.rs' \
    --glob '!crates/wyrd/wyrd-server/src/query/service.rs' \
    --glob '!crates/wyrd/wyrd-server/src/app/server.rs' \
    --glob '!crates/wyrd/wyrd-server/tests/pg_authz_check_route.rs' \
    --glob '!crates/wyrd/wyrd-server/tests/pg_grpc_smoke.rs' \
    --glob '!crates/wyrd/wyrd-server/tests/pg_router_smoke.rs' \
    --glob '!crates/wyrd/wyrd-server/tests/pg_grpc_ingest_smoke.rs' \
    --glob '!crates/wyrd/wyrd-server/tests/pg_merge_http_protected.rs' \
    --glob '!crates/vala/vala-bifrost-redux/tests/pg_scribe_seal.rs' \
    crates/ python/
