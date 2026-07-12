#!/usr/bin/env bash
# WHY THIS FILE EXISTS: wyrd-dev-fixtures is used by integration tests across
# many crates. If it imports wyrd-server, every consumer transitively compiles
# the full server stack — including axum, tonic, and all server business logic —
# just to run a database fixture. That inflates test compile times and risks
# server-only side effects leaking into test setup. ServerPostgres::from_parts
# is a low-level server internal; calling it outside wyrd-server/wyrd-testing
# bypasses the intended lifecycle and tenancy guarantees.
#
# WHAT IT CHECKS:
#   1. wyrd-dev-fixtures source does not import wyrd_server
#   2. ServerPostgres::from_parts appears only in wyrd-server and wyrd-testing
#   3. wyrd-dev-fixtures Cargo.toml does not declare wyrd-server as a dep
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
