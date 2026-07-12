#!/usr/bin/env bash
# Fail if from_pools is called outside the 19 allowlisted sites
# (2 definition files, 9 production wiring files, 8 test sites).
set -e
# from_pools may only be called from the sites below.
# 2 definition files: wyrd-sql/src/postgres.rs, vala-sql/src/postgres.rs
# 9 production wiring files: wyrd-server/src/{state,boot/mod,http/middleware/authenticate,components/{auth/{principal_extractor,caller_extractor,policy_hook,routes},health},postgres}.rs
# 3 in-src #[cfg(test)] sites: wyrd-server/src/{bifrost/service,query/service,app/server}.rs
# 5 integration test files: wyrd-server/tests/{pg_authz_check_route,pg_grpc_smoke,pg_router_smoke,pg_grpc_ingest_smoke,pg_merge_http_protected}.rs
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
    crates/ python/
