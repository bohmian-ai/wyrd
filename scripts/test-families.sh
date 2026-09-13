#!/usr/bin/env bash
# Single source of truth for the four workspace test-family package arrays.
#
# Source this file; do not execute it directly.
# Used by scripts/run-family-tests.sh and scripts/checks/test-coverage.sh.

FAMILY_WYRD=(
  wyrd
  wyrd-auth
  wyrd-cards
  wyrd-cli
  wyrd-config
  wyrd-interfaces
  wyrd-mcp
  wyrd-server
  wyrd-sql
  wyrd-storage
  wyrd-testing
  wyrd-tonic
  wyrd-spec
)

FAMILY_SKALD=(
  skald-agent
  skald-cache
  skald-observer
  skald-prompt
  skald-providers
  skald-observer
  skald-runtime
  skald-spec
  skald-tool
  skald-workflow
)

FAMILY_VALA=(
  vala-bifrost-redux
  vala-core
  vala-drift
  vala-eval
  vala-ingest
  vala-sql
)

FAMILY_SHARED=(
  wyrd-auth-check
  wyrd-auth-issue
  wyrd-auth-oidc
  wyrd-auth-verify
  wyrd-client
  wyrd-crypt
  wyrd-dev-fixtures
  wyrd-error-derive
  wyrd-queue
  wyrd-runtime
  wyrd-loader
  wyrd-semver
  wyrd-telemetry
  wyrd-test-contract-macros
  wyrd-tls
  wyrd-bench
  wyrd-utils
  wyrd-version
)
