#!/usr/bin/env bash
# WHY THIS FILE EXISTS: mock libraries (mockall, wiremock, mockito) are
# test-only tools. If they appear in production source — even behind a
# #[cfg(test)] block — they become non-optional transitive deps for every
# downstream consumer, inflating compile times and risking test-only behavior
# being activated in production builds by a misconfigured feature flag.
# The allowlisted files are production code that instantiates a mock server
# for live provider integration or uses wiremock for test helpers exposed
# only through wyrd-testing.
# `wyrd-auth/src/callback.rs` carries the provider-screening tests in a
# `#[cfg(test)]` module over a `[dev-dependencies]` wiremock, exactly as the
# `wyrd-server/src/auth/callback.rs` entry above it does; the callback
# screening path gained a second home in `wyrd-auth` without the allowlist
# following it.
# `wyrd-client/tests/storage_dispatch.rs` (presigned transfer headers) and the
# `#[cfg(test)]` module of `wyrd-client/src/cards/handle.rs` are test-only
# seams; their wiremock dev-dependency is not part of the published client
# dependency graph. The `#[cfg(test)]` `screening_tests` module of
# `wyrd-auth/src/callback.rs` is the same shape: a dev-dependency only.
# Gateway's mock provider and Postgres proofs, plus the provider client tests,
# are also compiled only under `#[cfg(test)]` with dev-only wiremock.
# The shared `wyrd-vault` KV v2 reader carries the Vault tests that moved with
# it out of Gateway, in the same `#[cfg(test)]` module over dev-only wiremock.
# Operator key reads and delivery pin their Vault and provider behavior in
# `#[cfg(test)]` modules, and the Operator Postgres journeys stand in for
# providers, all over the server's dev-only wiremock.
# Workflow execution pins its route dispatch the same way: the
# `#[cfg(test)]` modules of `skald-workflow/src/workflow.rs`,
# `skald-providers/src/endpoint.rs`, and `wyrd-client/src/workflow/mod.rs`,
# and the `wyrd-client/tests/workflow_transport.rs` target, all stand in for
# gateways and providers over dev-only wiremock.
# OIDC login, authorization, platform administration, and provider screening
# pin their provider behavior the same way: the `#[cfg(test)]` modules of
# `wyrd-auth-oidc` (relying party, screening), `wyrd-auth` (CLI and platform
# logins), and `wyrd-server` (authorize, admin identity), plus the
# `identity_e2e` and `platform_admin_e2e` journeys, stand in for identity
# providers over dev-only wiremock. The `#[cfg(test)]` modules of the
# `wyrd-client` Bifrost gRPC transport and storage upload, and the
# `pg_verification_routes` target, stand in for auth and storage endpoints
# the same way.
#
# WHAT IT CHECKS: mockall, wiremock, and mockito references do not appear
# outside wyrd-testing and the explicitly allowlisted test-only seams.
if rg -n 'mockall|wiremock|mockito' crates \
  --glob '!crates/wyrd/wyrd-testing/**' \
  --glob '!crates/skald/skald-providers/Cargo.toml' \
  --glob '!crates/skald/skald-providers/src/lib.rs' \
  --glob '!crates/skald/skald-providers/src/raw.rs' \
  --glob '!crates/skald/skald-providers/src/auth/google_oauth.rs' \
  --glob '!crates/skald/skald-providers/src/clients/anthropic.rs' \
  --glob '!crates/skald/skald-providers/src/clients/google.rs' \
  --glob '!crates/skald/skald-providers/src/clients/openai.rs' \
  --glob '!crates/skald/skald-providers/src/clients/vertex.rs' \
  --glob '!crates/skald/skald-providers/src/clients/mod.rs' \
  --glob '!crates/skald/skald-providers/src/endpoint.rs' \
  --glob '!crates/skald/skald-workflow/Cargo.toml' \
  --glob '!crates/skald/skald-workflow/src/workflow.rs' \
  --glob '!crates/shared/wyrd-vault/Cargo.toml' \
  --glob '!crates/shared/wyrd-vault/src/lib.rs' \
  --glob '!crates/wyrd/wyrd-gateway/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-gateway/src/vault.rs' \
  --glob '!crates/wyrd/wyrd-gateway/src/credential.rs' \
  --glob '!crates/wyrd/wyrd-gateway/src/endpoint.rs' \
  --glob '!crates/wyrd/wyrd-gateway/src/adapter/tests.rs' \
  --glob '!crates/wyrd/wyrd-cli/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-cli/tests/**' \
  --glob '!crates/shared/wyrd-client/Cargo.toml' \
  --glob '!crates/shared/wyrd-client/tests/storage_dispatch.rs' \
  --glob '!crates/shared/wyrd-client/src/cards/handle.rs' \
  --glob '!crates/shared/wyrd-client/src/workflow/mod.rs' \
  --glob '!crates/shared/wyrd-client/tests/workflow_transport.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/Cargo.toml' \
  --glob '!crates/shared/wyrd-auth-oidc/src/jwks.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/src/provider.rs' \
  --glob '!crates/shared/wyrd-auth-verify/Cargo.toml' \
  --glob '!crates/shared/wyrd-auth-verify/src/lib.rs' \
  --glob '!crates/wyrd/wyrd-server/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs' \
  --glob '!crates/wyrd/wyrd-server/src/auth/callback.rs' \
  --glob '!crates/wyrd/wyrd-server/src/components/admin/routes.rs' \
  --glob '!crates/wyrd/wyrd-server/src/boot/issuer.rs' \
  --glob '!crates/wyrd/wyrd-server/src/components/gateway/pg_invocation_tests.rs' \
  --glob '!crates/wyrd/wyrd-server/src/components/operators/keys.rs' \
  --glob '!crates/wyrd/wyrd-server/src/verification/operators.rs' \
  --glob '!crates/wyrd/wyrd-server/tests/pg_operator_delivery.rs' \
  --glob '!crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs' \
  --glob '!crates/wyrd/wyrd-auth/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-auth/src/callback.rs' \
  --glob '!crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/src/relying_party.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/src/screening.rs' \
  --glob '!crates/shared/wyrd-client/src/bifrost/grpc.rs' \
  --glob '!crates/shared/wyrd-client/src/storage/upload/tests.rs' \
  --glob '!crates/wyrd/wyrd-auth/src/cli_logins.rs' \
  --glob '!crates/wyrd/wyrd-auth/src/platform_login.rs' \
  --glob '!crates/wyrd/wyrd-server/src/auth/authorize.rs' \
  --glob '!crates/wyrd/wyrd-server/src/components/admin/identity.rs' \
  --glob '!crates/wyrd/wyrd-server/tests/identity_e2e.rs' \
  --glob '!crates/wyrd/wyrd-server/tests/platform_admin_e2e.rs' \
  --glob '!crates/wyrd/wyrd-server/tests/pg_verification_routes.rs'; then
  echo 'mock dependency leaked outside wyrd-testing'
  exit 1
fi
