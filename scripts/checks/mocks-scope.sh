#!/usr/bin/env bash
# WHY THIS FILE EXISTS: mock libraries (mockall, wiremock, mockito) are
# test-only tools. If they appear in production source — even behind a
# #[cfg(test)] block — they become non-optional transitive deps for every
# downstream consumer, inflating compile times and risking test-only behavior
# being activated in production builds by a misconfigured feature flag.
# The allowlisted files are production code that instantiates a mock server
# for live provider integration or uses wiremock for test helpers exposed
# only through wyrd-testing.
# `wyrd-client/tests/storage_dispatch.rs` (presigned transfer headers) and the
# `#[cfg(test)]` module of `wyrd-client/src/cards/handle.rs` are test-only
# seams; their wiremock dev-dependency is not part of the published client
# dependency graph.
#
# WHAT IT CHECKS: mockall, wiremock, and mockito references do not appear
# outside wyrd-testing and the explicitly allowlisted provider/auth files.
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
  --glob '!crates/wyrd/wyrd-cli/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-cli/tests/**' \
  --glob '!crates/shared/wyrd-client/Cargo.toml' \
  --glob '!crates/shared/wyrd-client/tests/storage_dispatch.rs' \
  --glob '!crates/shared/wyrd-client/src/cards/handle.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/Cargo.toml' \
  --glob '!crates/shared/wyrd-auth-oidc/src/jwks.rs' \
  --glob '!crates/shared/wyrd-auth-oidc/src/provider.rs' \
  --glob '!crates/shared/wyrd-auth-verify/Cargo.toml' \
  --glob '!crates/shared/wyrd-auth-verify/src/lib.rs' \
  --glob '!crates/wyrd/wyrd-server/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-server/src/auth/jwt_bearer.rs' \
  --glob '!crates/wyrd/wyrd-server/src/auth/callback.rs' \
  --glob '!crates/wyrd/wyrd-server/src/auth/admin.rs' \
  --glob '!crates/wyrd/wyrd-server/src/components/admin/routes.rs' \
  --glob '!crates/wyrd/wyrd-server/src/issuer_boot.rs' \
  --glob '!crates/wyrd/wyrd-server/src/boot/issuer.rs' \
  --glob '!crates/wyrd/wyrd-cli/src/auth/trusted_issuer.rs'; then
  echo 'mock dependency leaked outside wyrd-testing'
  exit 1
fi
