#!/usr/bin/env bash
# Assert mock helpers stay out of production source.
if rg -n 'mockall|wiremock|mockito' crates \
  --glob '!crates/shared/wyrd-testing/**' \
  --glob '!crates/skald/skald-providers/Cargo.toml' \
  --glob '!crates/skald/skald-providers/tests/**' \
  --glob '!crates/wyrd/wyrd-cli/Cargo.toml' \
  --glob '!crates/wyrd/wyrd-cli/tests/**' \
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
