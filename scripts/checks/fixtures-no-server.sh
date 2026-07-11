#!/usr/bin/env bash
# Fail if wyrd-dev-fixtures imports wyrd-server or ServerPostgres::from_parts
# appears outside wyrd-server/wyrd-testing.
set -e
# 1. wyrd-dev-fixtures must never import wyrd-server.
! rg -n --no-heading -e '\buse\s+wyrd_server\b|\bwyrd_server::' \
    crates/shared/wyrd-dev-fixtures/
# 2. ServerPostgres composition (from_parts) stays in server + test harness only.
! rg -n --no-heading -e 'ServerPostgres::from_parts' \
    --glob '!crates/wyrd/wyrd-server/**' \
    --glob '!crates/wyrd/wyrd-testing/**' \
    crates/ python/
# 3. wyrd-dev-fixtures Cargo.toml must not declare wyrd-server as a dependency.
! rg -n --no-heading -e '^[[:space:]]*wyrd-server[[:space:]]*=' \
    crates/shared/wyrd-dev-fixtures/Cargo.toml
