//! Human OIDC callback adapter: the common `GET /auth/callback` exchange.

use secrecy::SecretString;
use wyrd_spec::auth::LoginInitiation;

use crate::auth::auth_not_configured;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Complete a login from the provider callback's `code` and `state`.
///
/// Builds the authorization-code exchange from the server's auth
/// configuration and runs it. The tenant is recovered from the state alone;
/// no request header is consulted. The issued session is stored sealed for
/// redemption and never returned here.
///
/// # Errors
/// Returns [`WyrdErrorResponse`] when auth is not configured, and every
/// refusal of [`wyrd_auth::callback::AuthorizationCodeExchange::execute`].
pub async fn exchange_authorization_code(
    state: &AppState,
    code: SecretString,
    state_key: &str,
    request_id: &str,
) -> Result<LoginInitiation, WyrdErrorResponse> {
    let service = wyrd_auth::callback::AuthorizationCodeExchange {
        issuer: state.auth.tenant_issuer().ok_or_else(auth_not_configured)?,
        verifier: state
            .auth
            .external_verifier
            .clone()
            .ok_or_else(auth_not_configured)?,
        connections: state
            .auth
            .human_connections
            .clone()
            .ok_or_else(auth_not_configured)?,
    };
    service
        .execute(code, state_key, request_id)
        .await
        .map_err(WyrdErrorResponse::from)
}

#[cfg(test)]
mod pg_tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use crate::auth::pg_resolvers::PgIssuerResolver;
    use crate::http::error::WyrdErrorResponse;
    use crate::state::AppState;
    use chrono::{Duration as ChronoDuration, Utc};
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use secrecy::SecretString;
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::callback::{
        AuthorizationCodeExchange, audit_authorization_code_failure, ensure_user_identity,
        role_names_to_refs, verify_authorized_party, verify_nonce,
    };
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, JwksCache, ScreenedHttp, TrustedIssuer,
    };
    use wyrd_auth_verify::{
        ExternalVerifier, Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::{IssuerUrl, LoginInitiation, Sha256Hex, TokenType};
    use wyrd_sql::queries::auth::{
        LoginState, consume_login_state, insert_login_state, insert_role, list_user_roles,
        user_id_by_identity,
    };
    use wyrd_sql::row_types::auth::HumanConnectionBinding;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::exchange_authorization_code;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";
    const EXTERNAL_ISSUER: &str = "https://idp.example.com/realms/acme";
    const EXTERNAL_AUDIENCE: &str = "wyrd-client-id";
    const EXTERNAL_KID: &str = "ext-key-1";
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
    const EXTERNAL_SUBJECT: &str = "ext-user@idp.example.com";

    /// Only mapped groups grant roles; connection default roles never apply
    /// to a human login.
    #[test]
    fn role_names_to_refs_maps_known_groups_only_and_never_defaults() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([("data-science".to_owned(), vec!["data_science".to_owned()])]),
            vec!["viewer".to_owned()],
        );

        let roles = role_set(
            role_names_to_refs(
                &trusted,
                &["data-science".to_owned(), "marketing".to_owned()],
            )
            .expect("roles map"),
        );

        assert_eq!(roles, BTreeSet::from(["data_science".to_owned()]));
    }

    /// A person in no group gets no roles, even when the connection lists
    /// default roles.
    #[test]
    fn role_names_to_refs_grants_nothing_without_groups() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([("data-science".to_owned(), vec!["data_science".to_owned()])]),
            vec!["viewer".to_owned()],
        );

        let roles = role_names_to_refs(&trusted, &[]).expect("roles map");

        assert!(roles.is_empty());
    }

    /// An unmapped group grants nothing.
    #[test]
    fn role_names_to_refs_default_denies_unmapped_groups() {
        let trusted = trusted_issuer(DataTenantId::new_v7(), HashMap::new(), Vec::new());

        let roles = role_names_to_refs(&trusted, &["anything".to_owned()]).expect("roles map");

        assert!(roles.is_empty());
    }

    /// Two groups mapping to the same role yield it once.
    #[test]
    fn role_names_to_refs_deduplicates_group_roles() {
        let trusted = trusted_issuer(
            DataTenantId::new_v7(),
            HashMap::from([
                ("data-science".to_owned(), vec!["viewer".to_owned()]),
                ("analysts".to_owned(), vec!["viewer".to_owned()]),
            ]),
            Vec::new(),
        );

        let roles = role_set(
            role_names_to_refs(
                &trusted,
                &["data-science".to_owned(), "analysts".to_owned()],
            )
            .expect("roles map"),
        );

        assert_eq!(roles, BTreeSet::from(["viewer".to_owned()]));
    }

    /// A token echoing another login's nonce is refused.
    #[test]
    fn verify_nonce_rejects_mismatched_id_token_nonce() {
        let error = verify_nonce("nonce-a", &serde_json::json!({ "nonce": "nonce-b" }))
            .expect_err("nonce mismatch rejects");

        assert_eq!(error.code(), "WYRD_AUTH_400_INVALID_NONCE");
    }

    /// A state naming no pending login is refused before any tenant is known,
    /// so no tenant audit event is written for it.
    ///
    /// # Panics
    /// Panics when the state is accepted or an audit event is staged.
    #[tokio::test]
    async fn unknown_state_is_refused_without_a_tenant_audit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let state = test_state_with_external(&fixture).await;

        let error = exchange_authorization_code(
            &state,
            SecretString::from("code".to_owned()),
            "missing-state",
            "req-missing-state",
        )
        .await
        .expect_err("missing state fails");

        assert_eq!(error.0.code(), "WYRD_AUTH_400_INVALID_STATE");
        assert!(audit_rows(&fixture).await.is_empty());
    }

    /// A state that was already consumed names no pending login, so its
    /// replay is refused before any provider IO.
    ///
    /// # Panics
    /// Panics when the replay is accepted.
    #[tokio::test]
    async fn a_consumed_state_is_refused() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let state = test_state_with_external(&fixture).await;
        let binding = committed_active_binding(&fixture).await;
        let raw = "consumed-state";
        pending_login(
            &fixture,
            Sha256Hex::digest(raw.as_bytes()),
            binding,
            "nonce",
        )
        .await;

        let error = exchange_authorization_code(
            &state,
            SecretString::from("code".to_owned()),
            raw,
            "req-consumed",
        )
        .await
        .expect_err("a consumed state is refused");

        assert_eq!(error.0.code(), "WYRD_AUTH_400_INVALID_STATE");
    }

    /// A verified token for a consumed login issues the session, stores it
    /// sealed on the state row for its browser binding, and audits success;
    /// the completion redeems once to a usable session.
    ///
    /// # Panics
    /// Panics when completion, redemption, or the audit differ.
    #[tokio::test]
    async fn finish_issues_seals_and_audits_the_session() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let state = test_state_with_external(&fixture).await;
        let trusted =
            trusted_issuer_with_jwks(tenant, jwks_uri(&server), HashMap::new(), Vec::new());
        let binding = committed_active_binding(&fixture).await;
        let (hash, login) = pending_login(&fixture, state_hash(1), binding, "nonce-ok").await;
        let id_token = encode_external_token(&external_claims(
            EXTERNAL_AUDIENCE,
            "nonce-ok",
            Some("ext@example.com"),
            &[],
        ));

        let completed = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &advertised(),
                &id_token,
                "req-success",
            )
            .await
            .expect("callback completion succeeds");

        assert_eq!(completed, LoginInitiation::Browser(flow_for(&hash)));
        let principal_id = user_for(&fixture, EXTERNAL_SUBJECT)
            .await
            .expect("the user was created");
        assert_eq!(refresh_token_count(&fixture, principal_id).await, 1);
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, principal_id);
        assert_eq!(audit[0].1, "allowed");
        let user = principal_id.to_string();
        assert_eq!(audit[0].2["subject_principal_id"], user.as_str());
        assert_eq!(audit[0].2["actor_principal_id"], user.as_str());
        assert_eq!(audit[0].2["delegation_chain"], serde_json::json!([]));
        let redeemed = redeem(&state, tenant, &hash)
            .await
            .expect("the completion redeems");
        assert_eq!(redeemed.token_type, TokenType::Bearer);
        assert!(!redeemed.access_token.expose().is_empty());
        assert!(redeemed.refresh_token.is_some());
        redeem(&state, tenant, &hash)
            .await
            .expect_err("a completion redeems once");
    }

    /// A token for another audience is refused, audited with no principal,
    /// and leaves no completion or session behind.
    ///
    /// # Panics
    /// Panics when the token is accepted or a completion is stored.
    #[tokio::test]
    async fn a_wrong_audience_is_refused_and_leaves_no_completion() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        let state = test_state_with_external(&fixture).await;
        let trusted =
            trusted_issuer_with_jwks(tenant, jwks_uri(&server), HashMap::new(), Vec::new());
        let binding = committed_active_binding(&fixture).await;
        let (hash, login) = pending_login(&fixture, state_hash(2), binding, "nonce-ok").await;
        let id_token = encode_external_token(&external_claims(
            "wrong-audience",
            "nonce-ok",
            Some("ext@example.com"),
            &[],
        ));

        let error = finish_with_failure_audit(&state, &hash, &trusted, &login, &id_token)
            .await
            .expect_err("wrong audience rejects");

        assert_eq!(error.0.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, Uuid::nil());
        assert_eq!(audit[0].1, "denied");
        assert_eq!(audit[0].2, auth_failure_detail("INVALID_TOKEN"));
        redeem(&state, tenant, &hash)
            .await
            .expect_err("no completion was stored");
    }

    #[tokio::test]
    async fn ensure_user_identity_keys_on_issuer_and_subject_not_email() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let trusted = trusted_issuer(tenant, HashMap::new(), Vec::new());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let first = ensure_user_identity(&mut conn, &trusted, "subject-1", Some("a@example.com"))
            .await
            .expect("first identity upserts");
        let second = ensure_user_identity(&mut conn, &trusted, "subject-1", Some("b@example.com"))
            .await
            .expect("second identity reuses subject");

        assert_eq!(first, second);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_users WHERE auth_type = 'oidc'")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("user count query runs");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn ensure_user_identity_allows_missing_email() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let trusted = trusted_issuer(tenant, HashMap::new(), Vec::new());
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");

        let user_id = ensure_user_identity(&mut conn, &trusted, "subject-no-email", None)
            .await
            .expect("identity with omitted email inserts");
        let email: Option<String> =
            sqlx::query_scalar("SELECT email FROM wyrd.auth_users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("email query runs");

        assert!(email.is_none());
    }

    /// A nonce mismatch is audited as a credential refusal.
    #[test]
    fn callback_nonce_failure_maps_to_invalid_token_code() {
        assert_eq!(
            wyrd_auth::audit::auth_failure_code(&wyrd_spec::error::WyrdError::InvalidNonce {
                message: "nonce".to_owned(),
                details: serde_json::json!({}),
            }),
            wyrd_spec::vala::audit_detail::AuditErrorCode::InvalidToken
        );
    }

    fn trusted_issuer(
        tenant_id: DataTenantId,
        group_role_map: HashMap<String, Vec<String>>,
        default_roles: Vec<String>,
    ) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: IssuerUrl::new("https://idp.example.com/realms/acme")
                .expect("issuer URL is valid"),
            jwks_uri: "https://idp.example.com/realms/acme/jwks"
                .parse()
                .expect("jwks URL is valid"),
            expected_audience: "wyrd-client-id".to_owned(),
            client_id: "wyrd-client-id".to_owned(),
            client_auth: ClientAuth::SecretPost(SecretString::from("client-secret".to_owned())),
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("groups")),
            },
            group_role_map,
            default_roles,
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: StdDuration::from_secs(300),
        }
    }

    fn trusted_issuer_with_jwks(
        tenant_id: DataTenantId,
        jwks_uri: url::Url,
        group_role_map: HashMap<String, Vec<String>>,
        default_roles: Vec<String>,
    ) -> TrustedIssuer {
        TrustedIssuer {
            tenant_id,
            issuer: IssuerUrl::new(EXTERNAL_ISSUER).expect("issuer URL is valid"),
            jwks_uri,
            expected_audience: EXTERNAL_AUDIENCE.to_owned(),
            client_id: EXTERNAL_AUDIENCE.to_owned(),
            client_auth: ClientAuth::SecretPost(SecretString::from("client-secret".to_owned())),
            claim_mapping: ClaimMapping {
                subject: ClaimPath::new("sub"),
                email: Some(ClaimPath::new("email")),
                groups: Some(ClaimPath::new("groups")),
            },
            group_role_map,
            default_roles,
            principal_kind: IssuerTokenPolicy::Human,
            jwks_ttl: StdDuration::from_secs(300),
        }
    }

    /// A distinct state hash for the `n`th login of a test.
    fn state_hash(n: u8) -> Sha256Hex {
        Sha256Hex::digest(&[n])
    }

    /// The browser flow binding the pending login under `state_hash` records;
    /// distinct per login so one test can complete several.
    fn flow_for(state_hash: &Sha256Hex) -> Sha256Hex {
        Sha256Hex::digest(state_hash.as_bytes())
    }

    /// The ID-token algorithms the test provider advertises: the one its
    /// Ed25519 key signs with.
    fn advertised() -> Vec<String> {
        vec!["EdDSA".to_owned()]
    }

    /// Redeem the completion of the login under `state_hash` through the
    /// connection owner.
    ///
    /// # Errors
    /// Returns the redemption refusal.
    ///
    /// # Panics
    /// Panics when the test state has no connection owner.
    async fn redeem(
        state: &AppState,
        tenant: DataTenantId,
        state_hash: &Sha256Hex,
    ) -> Result<wyrd_spec::auth::TokenResponse, wyrd_spec::error::WyrdError> {
        state
            .auth
            .human_connections
            .as_ref()
            .expect("connection owner")
            .redeem_completion(tenant, LoginInitiation::Browser(flow_for(state_hash)))
            .await
    }

    /// The User the test issuer's `subject` resolved to, if any.
    ///
    /// # Panics
    /// Panics when the lookup fails.
    async fn user_for(fixture: &PgFixture, subject: &str) -> Option<Uuid> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        user_id_by_identity(&mut conn, EXTERNAL_ISSUER, subject)
            .await
            .expect("identity lookup runs")
    }

    /// Record and consume a browser login bound to the fixture's seeded Active
    /// connection, as the callback does before provider IO, and return its
    /// state hash and consumed row.
    ///
    /// # Panics
    /// Panics when the row cannot be written or consumed.
    async fn pending_login(
        fixture: &PgFixture,
        state_hash: Sha256Hex,
        connection: HumanConnectionBinding,
        nonce: &str,
    ) -> (Sha256Hex, LoginState) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let inserted = insert_login_state(
            &mut conn,
            &state_hash,
            &LoginState {
                connection,
                issuer: "https://idp.fixture.test".to_owned(),
                client_id: "wyrd-fixture".to_owned(),
                redirect_uri: "https://test-tenant-1.example.com/auth/callback".to_owned(),
                code_verifier: SecretString::from("verifier"),
                nonce: nonce.to_owned(),
                initiation: LoginInitiation::Browser(flow_for(&state_hash)),
            },
            StdDuration::from_mins(5),
        )
        .await
        .expect("state inserts");
        assert!(inserted, "the state is recorded");
        let consumed = consume_login_state(&mut conn, &state_hash)
            .await
            .expect("state consumes")
            .expect("state is pending");
        conn.commit().await.expect("state commits");
        (state_hash, consumed)
    }

    /// Seed and commit the tenant's Active human connection, returning the
    /// binding a login through it records.
    async fn committed_active_binding(fixture: &PgFixture) -> HumanConnectionBinding {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let binding = seed_active_human_connection(&mut conn)
            .await
            .expect("connection seeds");
        conn.commit().await.expect("connection commits");
        binding
    }

    /// Run the post-exchange half of the callback and, on refusal, stage the
    /// denied audit exactly as `execute` does.
    ///
    /// # Errors
    /// Returns the refusal of `finish_id_token_exchange`.
    async fn finish_with_failure_audit(
        state: &AppState,
        state_hash: &Sha256Hex,
        trusted: &TrustedIssuer,
        login: &LoginState,
        id_token: &str,
    ) -> Result<LoginInitiation, WyrdErrorResponse> {
        let result = authorization_exchange_service(state)
            .finish_id_token_exchange(state_hash, trusted, login, &advertised(), id_token, "req")
            .await
            .map_err(WyrdErrorResponse::from);
        if let Err(error) = &result {
            audit_authorization_code_failure(
                state.postgres.wyrd(),
                trusted.tenant_id,
                "req",
                &error.0,
            )
            .await;
        }
        result
    }

    /// The exchange service over the test state's auth configuration.
    ///
    /// # Panics
    /// Panics when the test state lacks issuing, verification, or connections.
    fn authorization_exchange_service(state: &AppState) -> AuthorizationCodeExchange {
        AuthorizationCodeExchange {
            issuer: state
                .auth
                .tenant_issuer()
                .expect("test state has issuing key"),
            verifier: state
                .auth
                .external_verifier
                .clone()
                .expect("test state has external verifier"),
            connections: state
                .auth
                .human_connections
                .clone()
                .expect("test state has a connection owner"),
        }
    }

    fn jwks_uri(server: &MockServer) -> url::Url {
        format!("{}/jwks", server.uri())
            .parse()
            .expect("wiremock URI is valid")
    }

    fn ed_jwks_json(kid: &str) -> serde_json::Value {
        serde_json::json!({
            "keys": [{
                "kty": "OKP",
                "crv": "Ed25519",
                "kid": kid,
                "x": ED_X
            }]
        })
    }

    fn external_claims(
        audience: &str,
        nonce: &str,
        email: Option<&str>,
        groups: &[&str],
    ) -> serde_json::Value {
        let now = Utc::now();
        let mut claims = serde_json::json!({
            "sub": EXTERNAL_SUBJECT,
            "iss": EXTERNAL_ISSUER,
            "aud": audience,
            "exp": (now + ChronoDuration::hours(1)).timestamp(),
            "iat": now.timestamp(),
            "nonce": nonce,
            "groups": groups,
        });
        if let Some(email) = email {
            claims["email"] = serde_json::json!(email);
        }
        claims
    }

    fn encode_external_token(claims: &serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::EdDSA);
        header.kid = Some(EXTERNAL_KID.to_owned());
        let key = EncodingKey::from_ed_pem(PRIVATE_KEY_PEM.as_bytes())
            .expect("external private key parses");
        encode(&header, claims, &key).expect("external token signs")
    }

    fn role_set(roles: Vec<wyrd_runtime::RoleRef>) -> BTreeSet<String> {
        roles
            .iter()
            .map(wyrd_runtime::RoleRef::as_str)
            .map(ToOwned::to_owned)
            .collect()
    }

    async fn test_state(fixture: &PgFixture) -> AppState {
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(
            fixture.wyrd_postgres().clone(),
            fixture.vala_postgres().clone(),
        ));
        let dir = tempfile::tempdir().expect("callback storage tempdir");
        let storage_root = dir.keep().join("callback-storage");
        std::fs::create_dir_all(&storage_root).expect("storage root creates");
        let signer = LocalSigner::new(storage_root).expect("local signer creates");
        crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
    }

    /// Build callback test state with a real issuing key, external verifier,
    /// and human-connection owner.
    ///
    /// Human trust is passed to `finish_id_token_exchange` explicitly; a test
    /// that reaches issuance seeds the Active connection its login state is
    /// bound to. The Pg issuer resolver backs the verifier's workload path.
    async fn test_state_with_external(fixture: &PgFixture) -> AppState {
        let sealing_key = Arc::new(SealingKeyring::new(SecretKey::from_bytes([7_u8; 32])));
        let issuer_resolver = Arc::new(PgIssuerResolver::new(
            fixture.wyrd_postgres().clone(),
            Some(Arc::clone(&sealing_key)),
        ));

        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(PRIVATE_KEY_PEM.to_owned()),
                Kid::new("k1").expect("kid is valid"),
                "wyrd",
            )
            .expect("issuing key parses"),
        );
        let mut local_keys = HashMap::new();
        local_keys.insert(
            Kid::new("k1").expect("kid is valid"),
            Arc::new(public_key_from_pem(PUBLIC_KEY_PEM).expect("public key parses")),
        );
        let verifier = TokenVerifier::new(local_keys, "wyrd", WyrdAuthVerifySettings::default());
        let external_verifier = ExternalVerifier::new(
            Arc::new(JwksCache::new(
                ScreenedHttp::allowing_internal(),
                StdDuration::from_secs(300),
                StdDuration::from_secs(5),
            )),
            Arc::clone(&issuer_resolver),
            WyrdAuthVerifySettings::default(),
        );
        test_state(fixture)
            .await
            .with_auth(crate::components::auth::ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(Arc::new(verifier)),
                external_verifier: Some(Arc::new(external_verifier)),
                trusted_issuer_resolver: Some(issuer_resolver),
                human_connections: Some(HumanConnections::new(
                    fixture.wyrd_postgres().clone(),
                    Some(Arc::clone(&sealing_key)),
                    ScreenedHttp::allowing_internal(),
                    None,
                )),
                sealing_key: Some(sealing_key),
                ..crate::components::auth::ServerAuth::default()
            })
    }

    /// Staged `auth.token.exchange` events as `(principal_id, outcome, detail)`,
    /// oldest first.
    ///
    /// # Panics
    /// Panics when the query fails or a detail is not JSON.
    async fn audit_rows(fixture: &PgFixture) -> Vec<(Uuid, String, serde_json::Value)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
            "SELECT principal_id, outcome, detail
               FROM vala.audit_staging
              WHERE operation = 'auth.token.exchange'
              ORDER BY seq ASC",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit query runs");
        rows.into_iter()
            .map(|(principal_id, outcome, detail)| {
                let detail = serde_json::from_str(&detail).expect("audit detail is json");
                (principal_id, outcome, detail)
            })
            .collect()
    }

    /// Canonical detail of a refused exchange with `error_code`.
    fn auth_failure_detail(error_code: &str) -> serde_json::Value {
        serde_json::json!({ "kind": "auth_failure", "error_code": error_code })
    }

    async fn refresh_token_count(fixture: &PgFixture, principal_id: Uuid) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar(
            "SELECT COUNT(*)
               FROM wyrd.auth_refresh_tokens
              WHERE principal_kind = 'user'
                AND principal_id = $1",
        )
        .bind(principal_id)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("refresh token count query runs")
    }
}
