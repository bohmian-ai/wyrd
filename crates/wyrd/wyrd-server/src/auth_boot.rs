//! Assembly of Wyrd's own auth handles (issuing key + token verifier).
//!
//! This is the single place production boot builds the `issuing_key` /
//! `token_verifier` pair that [`crate::state::AppState`] needs to mint and
//! verify Wyrd JWTs. It is deliberately synchronous and pool-/registry-injected
//! so it can be exercised directly in tests, closing the gap where the
//! config→verifier path had no coverage outside the test harness.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use sqlx::PgPool;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_oidc::JwksCache;
use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};

use crate::auth::permission_resolver::SqlPermissionResolver;
use crate::auth::pg_resolvers::PgIssuerResolver;
use crate::auth::revocation_resolver::SqlRevocationCheck;
use crate::boot::ServerBootError;
use crate::state::WyrdTokenVerifier;

/// Wyrd's own issuer identity, stamped into minted tokens (`iss`) and checked by
/// the verifier. A single self-hosted deployment has one issuer.
const WYRD_ISSUER: &str = "wyrd";

/// Key id for Wyrd's single self-hosted signing key. The minted token header
/// carries this `kid`; the verifier's decoding-key map is keyed by it.
const WYRD_SIGNING_KID: &str = "wyrd-signing-key";

/// JWKS refresh TTL for foreign-issuer public keys (matches the test harness).
const JWKS_CACHE_TTL_SECS: u64 = 300;

/// Per-fetch timeout when refreshing a foreign issuer's JWKS.
const JWKS_FETCH_TIMEOUT_SECS: u64 = 5;

/// Build Wyrd's issuing key and token verifier from the configured signing key.
///
/// The public verification key is derived from the private signing key (see
/// [`IssuingKey::verifying_key_pem`]), so a single environment-provided signing
/// key is sufficient. The verifier's external (foreign-OIDC) path is always
/// wired to the supplied [`PgIssuerResolver`]; it resolves the requesting
/// tenant's trusted issuers per-request from Postgres (an empty result simply
/// means the tenant federates no issuers), so federated tokens can be exchanged.
///
/// # Errors
/// Returns [`ServerBootError::SigningKey`] when the PEM cannot be loaded as an
/// Ed25519 signing key or its public key cannot be derived.
pub fn build_auth_handles(
    signing_key: &SecretString,
    pool: &PgPool,
    issuer_resolver: Arc<PgIssuerResolver>,
) -> Result<(Arc<IssuingKey>, Arc<WyrdTokenVerifier>), ServerBootError> {
    let kid = Kid::new(WYRD_SIGNING_KID).expect("static signing kid is valid");
    let issuing_key = IssuingKey::from_ed_pem(signing_key.clone(), kid.clone(), WYRD_ISSUER)
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;

    let verifying_pem = issuing_key
        .verifying_key_pem()
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;
    let decoding = public_key_from_pem(verifying_pem.as_bytes())
        .map_err(|error| ServerBootError::SigningKey(error.to_string()))?;

    let mut decoding_keys = HashMap::new();
    decoding_keys.insert(kid, Arc::new(decoding));

    let resolver = Arc::new(SqlPermissionResolver::new(Arc::new(pool.clone())));
    let verifier_base = TokenVerifier::new(
        decoding_keys,
        WYRD_ISSUER,
        resolver,
        WyrdAuthVerifySettings::default(),
    )
    .with_revocation(Arc::new(SqlRevocationCheck::new(Arc::new(pool.clone()))));

    let verifier = verifier_base.with_external(
        Arc::new(JwksCache::new(
            reqwest::Client::new(),
            Duration::from_secs(JWKS_CACHE_TTL_SECS),
            Duration::from_secs(JWKS_FETCH_TIMEOUT_SECS),
        )),
        issuer_resolver,
    );

    Ok((Arc::new(issuing_key), Arc::new(verifier)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use wyrd_auth_verify::{
        AccessTokenClaims, PrincipalKind, TokenPrincipalRef, decode_kid, verify_eddsa,
    };
    use wyrd_runtime::{PrincipalId, RoleRef};
    use wyrd_spec::DataTenantId;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    fn lazy_pool() -> PgPool {
        PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new())
    }

    fn user_principal() -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
                .parse::<PrincipalId>()
                .expect("static principal id is valid"),
            kind: PrincipalKind::User,
            tenant_id: "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
                .parse::<DataTenantId>()
                .expect("static tenant id is valid"),
        }
    }

    // Drives the production auth-handle assembler directly: the issuing key it
    // returns must mint a token that the verifier's derived public key accepts,
    // under the same kid/issuer the assembler installs. This is the config→
    // verifier coverage the test harness previously held exclusively.
    #[tokio::test(flavor = "current_thread")]
    async fn build_auth_handles_mints_tokens_verifiable_by_the_derived_key() {
        let (issuing_key, _verifier) = build_auth_handles(
            &SecretString::from(PRIVATE_KEY_PEM),
            &lazy_pool(),
            Arc::new(PgIssuerResolver::new(Arc::new(lazy_pool()), None)),
        )
        .expect("auth handles assemble from the signing key");

        let token = issuing_key
            .issue_user_access_token(
                user_principal(),
                vec![RoleRef::new("runtime_admin").expect("static role is valid")],
                ChronoDuration::minutes(5),
            )
            .expect("issuing key mints a token");

        assert_eq!(
            decode_kid(&token).expect("kid decodes"),
            Some(WYRD_SIGNING_KID.to_owned())
        );

        let derived_pem = issuing_key
            .verifying_key_pem()
            .expect("public key derives from the signing key");
        let decoding = public_key_from_pem(derived_pem.as_bytes()).expect("derived key loads");
        let claims = verify_eddsa::<AccessTokenClaims>(&token, &decoding, Some(WYRD_ISSUER))
            .expect("minted token verifies against the assembler's derived key and issuer");

        assert_eq!(claims.principal.kind, PrincipalKind::User);
        assert_eq!(claims.iss, WYRD_ISSUER);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn build_auth_handles_rejects_an_invalid_signing_key() {
        let result = build_auth_handles(
            &SecretString::from("not a pem"),
            &lazy_pool(),
            Arc::new(PgIssuerResolver::new(Arc::new(lazy_pool()), None)),
        );
        assert!(matches!(result, Err(ServerBootError::SigningKey(_))));
    }
}
