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
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_oidc::{JwksCache, ScreenedHttp};
use wyrd_auth_verify::{
    ExternalVerifier, Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
};

use crate::auth::pg_resolvers::PgIssuerResolver;
use crate::boot::ServerBootError;

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

/// The auth handles production boot installs on [`crate::state::AppState`].
pub struct AuthHandles {
    /// Signs every Wyrd access, refresh, and platform token.
    pub issuing_key: Arc<IssuingKey>,
    /// Verifies Wyrd tenant access tokens on every request, locally.
    pub token_verifier: Arc<TokenVerifier>,
    /// Verifies foreign OIDC ID tokens and workload assertions at issuance.
    pub external_verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
}

/// Build Wyrd's issuing key, request verifier, and issuance-side external
/// verifier from the configured signing key.
///
/// The public verification key is derived from the private signing key (see
/// [`IssuingKey::verifying_key_pem`]), so a single environment-provided signing
/// key is sufficient. The request verifier holds only that key, the issuer,
/// the `wyrd` audience, and clock skew; it reads no database. The external
/// verifier resolves the requesting tenant's trusted issuers from Postgres
/// through the supplied [`PgIssuerResolver`] and is used only where a foreign
/// token is exchanged for a Wyrd one.
///
/// `http` is the deployment's outbound address screening. The JWKS cache is
/// built with it rather than a bare client, so a refresh long after the issuer
/// was configured is screened again at the moment it is made.
///
/// # Errors
/// Returns [`ServerBootError::OraclePeer`] when another Rustls provider already
/// owns the process. Returns [`ServerBootError::SigningKey`] when the PEM cannot
/// be loaded as an Ed25519 signing key or its public key cannot be derived.
///
/// # Panics
/// Panics only if the static signing `kid` is invalid, which is a compile-time
/// constant invariant.
pub fn build_auth_handles(
    signing_key: &SecretString,
    issuer_resolver: Arc<PgIssuerResolver>,
    http: ScreenedHttp,
) -> Result<AuthHandles, ServerBootError> {
    wyrd_tls::install_crypto_provider()
        .map_err(|error| ServerBootError::OraclePeer(error.to_string()))?;
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
    let settings = WyrdAuthVerifySettings::default();
    let token_verifier = TokenVerifier::new(decoding_keys, WYRD_ISSUER, settings.clone());
    let external_verifier = ExternalVerifier::new(
        Arc::new(JwksCache::new(
            http,
            Duration::from_secs(JWKS_CACHE_TTL_SECS),
            Duration::from_secs(JWKS_FETCH_TIMEOUT_SECS),
        )),
        issuer_resolver,
        settings,
    );

    Ok(AuthHandles {
        issuing_key: Arc::new(issuing_key),
        token_verifier: Arc::new(token_verifier),
        external_verifier: Arc::new(external_verifier),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;
    use sqlx::PgPool;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use wyrd_auth_issue::AccessGrant;
    use wyrd_auth_verify::{TokenPrincipalRef, decode_kid};
    use wyrd_runtime::{Permission, PrincipalId};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalKindTag;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";

    /// A pool that never connects; the assembler must not touch the database.
    fn lazy_pool() -> PgPool {
        PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new())
    }

    /// The tenant the fixture token is minted for.
    fn tenant() -> DataTenantId {
        "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b01"
            .parse::<DataTenantId>()
            .expect("static tenant id is valid")
    }

    /// A human principal in [`tenant`].
    fn user_principal() -> TokenPrincipalRef {
        TokenPrincipalRef {
            id: "01890f28-7c4a-7cc3-98e7-4f4a3c2d1b00"
                .parse::<PrincipalId>()
                .expect("static principal id is valid"),
            kind: PrincipalKindTag::User,
            tenant_id: tenant(),
            card_ref: None,
            card_ref_scope: Default::default(),
        }
    }

    /// Drives the production auth-handle assembler directly: the issuing key it
    /// returns must mint a token that the assembled request verifier accepts,
    /// under the same kid, issuer, and audience, with the signed permissions as
    /// the principal's authority.
    #[tokio::test(flavor = "current_thread")]
    async fn build_auth_handles_mints_tokens_the_request_verifier_accepts() {
        let handles = build_auth_handles(
            &SecretString::from(PRIVATE_KEY_PEM),
            Arc::new(PgIssuerResolver::new(Arc::new(lazy_pool()), None)),
            ScreenedHttp::allowing_internal(),
        )
        .expect("auth handles assemble from the signing key");

        let token = handles
            .issuing_key
            .issue_access_token(
                AccessGrant {
                    principal: user_principal(),
                    roles: Vec::new(),
                    permissions: std::iter::once(Permission::card_read()).collect(),
                    credential_id: None,
                    delegated_by: None,
                },
                ChronoDuration::minutes(5),
            )
            .expect("issuing key mints a token");

        assert_eq!(
            decode_kid(&token).expect("kid decodes"),
            Some(WYRD_SIGNING_KID.to_owned())
        );
        let verified = handles
            .token_verifier
            .verify(&SecretString::from(token), &tenant())
            .expect("minted token verifies against the assembled request verifier");
        assert_eq!(verified.principal.kind.tag(), PrincipalKindTag::User);
        assert!(
            verified
                .principal
                .effective_permissions
                .contains(&Permission::card_read())
        );
    }

    /// An unparseable signing key fails boot rather than serving unsigned.
    #[tokio::test(flavor = "current_thread")]
    async fn build_auth_handles_rejects_an_invalid_signing_key() {
        let result = build_auth_handles(
            &SecretString::from("not a pem"),
            Arc::new(PgIssuerResolver::new(Arc::new(lazy_pool()), None)),
            ScreenedHttp::allowing_internal(),
        );
        assert!(matches!(result, Err(ServerBootError::SigningKey(_))));
    }
}
