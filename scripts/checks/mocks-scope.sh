#!/usr/bin/env bash
# WHY THIS FILE EXISTS: mock crates (mockall, wiremock, mockito) must never
# reach a production build. Test-only use needs no listing: a
# [dev-dependencies] crate cannot compile into non-test code.
#
# WHAT IT CHECKS: no mock crate appears in the normal/build dependency graph
# of any workspace package except the test harnesses wyrd-testing and
# wyrd-sdk-ts-testing.
set -euo pipefail
graph=$(cargo tree --locked --workspace \
  --exclude wyrd-testing --exclude wyrd-sdk-ts-testing \
  -e normal,build --prefix none)
! grep -E '^(mockall|wiremock|mockito) ' <<<"$graph"
