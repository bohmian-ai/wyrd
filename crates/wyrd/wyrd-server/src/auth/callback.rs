//! Human OIDC callback adapter: the common `GET /auth/callback` exchange.

use wyrd_auth::callback::LoginCompletion;
use wyrd_spec::auth::ProviderResponse;

use crate::auth::auth_not_configured;
use crate::http::error::WyrdErrorResponse;
use crate::state::AppState;

/// Complete a login from the provider callback's `response` (its code or
/// error), `state`, and optional RFC 9207 `iss`.
///
/// Builds the authorization-code exchange from the server's auth
/// configuration and runs it. The tenant is recovered from the state alone;
/// no request header is consulted. No token is issued here: the returned
/// completion names the authorization code, device approval, or tested
/// candidate the callback recorded.
///
/// # Errors
/// Returns [`WyrdErrorResponse`] when auth is not configured, and every
/// refusal of [`wyrd_auth::callback::AuthorizationCodeExchange::execute`].
pub async fn exchange_authorization_code(
    state: &AppState,
    response: ProviderResponse,
    state_key: &str,
    response_issuer: Option<&str>,
    request_id: &str,
) -> Result<LoginCompletion, WyrdErrorResponse> {
    let service = wyrd_auth::callback::AuthorizationCodeExchange {
        issuer: state.auth.tenant_issuer().ok_or_else(auth_not_configured)?,
        connections: state
            .auth
            .human_connections
            .clone()
            .ok_or_else(auth_not_configured)?,
    };
    service
        .execute(response, state_key, response_issuer, request_id)
        .await
        .map_err(WyrdErrorResponse::from)
}

#[cfg(test)]
mod pg_tests {
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use crate::http::error::WyrdErrorResponse;
    use crate::state::AppState;
    use secrecy::{ExposeSecret as _, SecretString};
    use uuid::Uuid;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wyrd_auth::callback::{
        AuthorizationCodeExchange, LoginCompletion, audit_authorization_code_failure,
        ensure_user_identity, role_names_to_refs,
    };
    use wyrd_auth::connections::HumanConnections;
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_oidc::{
        ClaimMapping, ClaimPath, ClientAuth, MappedClaims, ScreenedHttp, TrustedIssuer,
    };
    use wyrd_auth_verify::{Kid, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem};
    use wyrd_crypt::{SealingKeyring, SecretKey};
    use wyrd_dev_fixtures::pg::{PgFixture, seed_active_human_connection};
    use wyrd_runtime::Permission;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::IssuerTokenPolicy;
    use wyrd_spec::auth::{
        ClientAuthorization, ConnectionTester, IssuerUrl, LoginInitiation, OAuthClientId,
        OAuthErrorCode, PrincipalId, PrincipalKindTag, ProviderResponse, SecretBearer, Sha256Hex,
        TokenType,
    };
    use wyrd_sql::queries::auth::{
        HumanConnectionWrite, LoginState, consume_login_state, human_connection_in_state,
        insert_human_candidate, insert_login_state, insert_role, insert_user, list_user_roles,
        lock_refresh_family, replace_user_roles, user_id_by_identity,
    };
    use wyrd_sql::row_types::auth::HumanConnectionBinding;
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};

    use super::exchange_authorization_code;
    use crate::auth::oauth::OAuthError;

    const PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";
    const EXTERNAL_ISSUER: &str = "https://idp.example.com/realms/acme";
    const EXTERNAL_AUDIENCE: &str = "wyrd-client-id";
    const EXTERNAL_KID: &str = "ext-key-1";
    const ED_X: &str = "WhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ-DZ8Vw";
    const EXTERNAL_SUBJECT: &str = "ext-user@idp.example.com";
    /// The `wyrd-ui` redirect URI the test logins name.
    const UI_REDIRECT: &str = "https://wyrd.example.com/login/callback";
    /// The RFC 7636 Appendix B code verifier the test logins redeem with.
    const CODE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
    /// The S256 challenge of [`CODE_VERIFIER`].
    const CODE_CHALLENGE: &str = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

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

    /// A state naming no pending login is refused before any tenant is known,
    /// so no tenant audit event is written for it.
    ///
    /// # Panics
    /// Panics when the state is accepted or an audit event is staged.
    #[tokio::test]
    async fn unknown_state_is_refused_without_a_tenant_audit() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let state = test_state_with_human_connections(&fixture).await;

        let error = exchange_authorization_code(
            &state,
            ProviderResponse::Code(SecretBearer::new("code".to_owned())),
            "missing-state",
            None,
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
        let state = test_state_with_human_connections(&fixture).await;
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
            ProviderResponse::Code(SecretBearer::new("code".to_owned())),
            raw,
            None,
            "req-consumed",
        )
        .await
        .expect_err("a consumed state is refused");

        assert_eq!(error.0.code(), "WYRD_AUTH_400_INVALID_STATE");
    }

    /// A provider's `error` consumes the login state once and resolves to
    /// the client's redirect with `access_denied` and its exact `state`,
    /// issuing nothing; `server_error` and `temporarily_unavailable` keep
    /// their codes, and a present `iss` must still name the login's issuer.
    ///
    /// # Panics
    /// Panics when a provider error is not resolved to the client's refusal,
    /// the state survives, or anything is issued.
    #[tokio::test]
    async fn a_provider_error_consumes_state_and_refuses_to_the_client() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let state = test_state_with_human_connections(&fixture).await;
        let binding = committed_active_binding(&fixture).await;
        let cases = [
            ("access_denied", None, OAuthErrorCode::AccessDenied),
            ("login_required", None, OAuthErrorCode::AccessDenied),
            ("server_error", None, OAuthErrorCode::ServerError),
            (
                "temporarily_unavailable",
                Some("https://idp.fixture.test"),
                OAuthErrorCode::TemporarilyUnavailable,
            ),
            (
                "temporarily_unavailable",
                Some("https://other.example.com"),
                OAuthErrorCode::AccessDenied,
            ),
        ];
        for (index, (provider_error, iss, expected)) in cases.into_iter().enumerate() {
            let raw = format!("provider-error-{index}");
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            let inserted = insert_login_state(
                &mut conn,
                &Sha256Hex::digest(raw.as_bytes()),
                &LoginState {
                    connection: binding,
                    issuer: "https://idp.fixture.test".to_owned(),
                    client_id: "wyrd-fixture".to_owned(),
                    redirect_uri: "https://test-tenant-1.example.com/auth/callback".to_owned(),
                    code_verifier: SecretString::from("verifier"),
                    nonce: "nonce".to_owned(),
                    initiation: LoginInitiation::Authorize(ClientAuthorization {
                        client: OAuthClientId::WyrdUi,
                        redirect_uri: UI_REDIRECT.to_owned(),
                        code_challenge: CODE_CHALLENGE.to_owned(),
                        state: Some("client-state".to_owned()),
                    }),
                },
                StdDuration::from_mins(5),
            )
            .await
            .expect("state inserts");
            assert!(inserted, "the state is recorded");
            conn.commit().await.expect("state commits");

            let completed = exchange_authorization_code(
                &state,
                ProviderResponse::Error(provider_error.to_owned()),
                &raw,
                iss,
                "req-provider-error",
            )
            .await
            .expect("a provider error resolves to the client");
            let LoginCompletion::Refused {
                authorization,
                error,
            } = completed
            else {
                panic!("{provider_error}: expected a client refusal, got {completed:?}");
            };
            assert_eq!(authorization.redirect_uri, UI_REDIRECT);
            assert_eq!(authorization.state.as_deref(), Some("client-state"));
            assert_eq!(
                OAuthError::authorization(error),
                expected,
                "{provider_error} {iss:?}"
            );

            let replay = exchange_authorization_code(
                &state,
                ProviderResponse::Error(provider_error.to_owned()),
                &raw,
                iss,
                "req-provider-error-replay",
            )
            .await
            .expect_err("the state was consumed once");
            assert_eq!(replay.0.code(), "WYRD_AUTH_400_INVALID_STATE");
        }
        assert_nothing_persisted(&fixture).await;
    }

    /// A verified token for a consumed login issues only an authorization
    /// code; that code redeems once to a usable session, which is minted and
    /// audited at redemption.
    ///
    /// # Panics
    /// Panics when completion, redemption, or the audit differ.
    #[tokio::test]
    async fn finish_issues_seals_and_audits_the_session() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted =
            trusted_issuer_with_jwks(tenant, jwks_uri(&server), HashMap::new(), Vec::new());
        let binding = committed_active_binding(&fixture).await;
        let (hash, login) = pending_login(&fixture, state_hash(1), binding, "nonce-ok").await;

        let completed = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &identity(EXTERNAL_SUBJECT, Some("ext@example.com"), &[]),
                "req-success",
            )
            .await
            .expect("callback completion succeeds");

        let principal_id = user_for(&fixture, EXTERNAL_SUBJECT)
            .await
            .expect("the user was created");
        assert_eq!(issued_codes(&fixture).await, 1);
        assert_eq!(
            refresh_token_count(&fixture, principal_id).await,
            0,
            "the callback mints nothing"
        );
        assert!(audit_rows(&fixture).await.is_empty());
        let redeemed = redeem(&state, &completed).await.expect("the code redeems");
        assert_eq!(refresh_token_count(&fixture, principal_id).await, 1);
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, principal_id);
        assert_eq!(audit[0].1, "allowed");
        let user = principal_id.to_string();
        assert_eq!(audit[0].2["subject_principal_id"], user.as_str());
        assert_eq!(audit[0].2["actor_principal_id"], user.as_str());
        assert_eq!(audit[0].2["delegation_chain"], serde_json::json!([]));
        assert_eq!(redeemed.token_type, TokenType::Bearer);
        assert!(!redeemed.access_token.expose().is_empty());
        assert!(redeemed.refresh_token.is_some());
        redeem(&state, &completed)
            .await
            .expect_err("a code redeems once");
    }

    /// A login whose bound connection is no longer the tenant's Active one is
    /// refused after verification, audited with no principal, and leaves no
    /// completion or session behind.
    ///
    /// # Panics
    /// Panics when the login is accepted or a completion is stored.
    #[tokio::test]
    async fn a_changed_connection_is_refused_and_leaves_no_completion() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted =
            trusted_issuer_with_jwks(tenant, jwks_uri(&server), HashMap::new(), Vec::new());
        let mut binding = committed_active_binding(&fixture).await;
        binding.connection_revision += 1;
        let (hash, login) = pending_login(&fixture, state_hash(2), binding, "nonce-ok").await;

        let error = finish_with_failure_audit(
            &state,
            &hash,
            &trusted,
            &login,
            &identity(EXTERNAL_SUBJECT, Some("ext@example.com"), &[]),
        )
        .await
        .expect_err("a changed connection rejects");

        assert_eq!(error.0.code(), "WYRD_AUTH_401_INVALID_TOKEN");
        let audit = audit_rows(&fixture).await;
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].0, Uuid::nil());
        assert_eq!(audit[0].1, "denied");
        assert_eq!(audit[0].2, auth_failure_detail("INVALID_TOKEN"));
        assert_eq!(issued_codes(&fixture).await, 0, "no code was issued");
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

    /// Two signed subjects at one issuer sharing an email are two Users, and
    /// the second gains none of the first's authority.
    ///
    /// # Panics
    /// Panics when the subjects merge or authority transfers.
    #[tokio::test]
    async fn same_email_different_subjects_are_distinct_users() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let binding = committed_active_binding(&fixture).await;
        let service = authorization_exchange_service(&state);
        for (n, subject, groups) in [(6, "subject-a", &["admins"][..]), (7, "subject-b", &[])] {
            let (hash, login) = pending_login(&fixture, state_hash(n), binding, "nonce").await;
            service
                .finish_id_token_exchange(
                    &hash,
                    &trusted,
                    &login,
                    &identity(subject, Some("shared@example.com"), groups),
                    "req-subject",
                )
                .await
                .expect("login completes");
        }

        let first = user_for(&fixture, "subject-a").await.expect("first user");
        let second = user_for(&fixture, "subject-b").await.expect("second user");
        assert_ne!(first, second);
        assert_eq!(
            user_roles(&fixture, first).await,
            vec![SYNC_ROLE.to_owned()]
        );
        assert!(user_roles(&fixture, second).await.is_empty());
    }

    /// A login that changes the User's durable roles stages exactly one
    /// `auth.user.roles.sync` event, and its code one token exchange at
    /// redemption; a repeat login with the same groups stages no sync.
    ///
    /// # Panics
    /// Panics when the role-sync evidence differs.
    #[tokio::test]
    async fn changed_roles_are_audited_once_and_unchanged_roles_never() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let binding = committed_active_binding(&fixture).await;
        let service = authorization_exchange_service(&state);
        for n in [8, 9] {
            let (hash, login) = pending_login(&fixture, state_hash(n), binding, "nonce").await;
            let completed = service
                .finish_id_token_exchange(
                    &hash,
                    &trusted,
                    &login,
                    &identity(EXTERNAL_SUBJECT, None, &["admins"]),
                    "req-sync",
                )
                .await
                .expect("login completes");
            redeem(&state, &completed).await.expect("the code redeems");
        }

        let user = user_for(&fixture, EXTERNAL_SUBJECT).await.expect("user");
        assert_eq!(user_roles(&fixture, user).await, vec![SYNC_ROLE.to_owned()]);
        let sync = operation_rows(&fixture, "auth.user.roles.sync").await;
        assert_eq!(
            sync,
            vec![(user, "allowed".to_owned(), format!("principal:{user}"))]
        );
        assert_eq!(
            audit_rows(&fixture).await.len(),
            2,
            "one exchange per redeemed login"
        );
    }

    /// Two concurrent callbacks for one existing User with disjoint mapped
    /// roles serialize before role replacement, so neither issues the union.
    ///
    /// A test transaction holds the User's refresh-family lock while callback
    /// `A` (group `alpha`) and then callback `B` (group `beta`) park on it, the
    /// order observed through `pg_locks` rather than timing. Once released,
    /// the durable set is `B`'s, each login stages one role-sync event, and
    /// each code redeems to a session carrying exactly that durable set.
    ///
    /// # Panics
    /// Panics when a callback fails, a waiter is never observed, or any token
    /// or the durable set carries the union.
    #[tokio::test]
    async fn concurrent_callbacks_replace_roles_without_union() {
        const ALPHA: &str = "login_alpha_probe";
        const BETA: &str = "login_beta_probe";
        let fixture = PgFixture::start().await.expect("fixture starts");
        let tenant = fixture.data_tenant_id();
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        seed_role(&fixture, ALPHA).await;
        seed_role(&fixture, BETA).await;
        let trusted = trusted_issuer_with_jwks(
            tenant,
            jwks_uri(&server),
            HashMap::from([
                ("alpha".to_owned(), vec![ALPHA.to_owned()]),
                ("beta".to_owned(), vec![BETA.to_owned()]),
            ]),
            Vec::new(),
        );
        let binding = committed_active_binding(&fixture).await;
        let service = authorization_exchange_service(&state);
        let login = |hash: Sha256Hex, login: LoginState, group: &'static str| {
            let service = &service;
            let trusted = &trusted;
            async move {
                service
                    .finish_id_token_exchange(
                        &hash,
                        trusted,
                        &login,
                        &identity(EXTERNAL_SUBJECT, None, &[group]),
                        "req-race",
                    )
                    .await
                    .expect("login completes")
            }
        };
        let (first, first_login) = pending_login(&fixture, state_hash(30), binding, "nonce").await;
        let _first = login(first, first_login, "none").await;
        let user = user_for(&fixture, EXTERNAL_SUBJECT).await.expect("user");
        let (hash_a, login_a) = pending_login(&fixture, state_hash(31), binding, "nonce").await;
        let (hash_b, login_b) = pending_login(&fixture, state_hash(32), binding, "nonce").await;

        let mut gate = fixture.tenant_conn().await.expect("gate conn opens");
        lock_refresh_family(&mut gate, "user", user)
            .await
            .expect("the gate holds the family lock");
        let (completion_a, completion_b, ()) = tokio::join!(
            login(hash_a, login_a, "alpha"),
            async {
                wait_for_lock_waiters(&fixture, 1).await;
                login(hash_b, login_b, "beta").await
            },
            async {
                wait_for_lock_waiters(&fixture, 2).await;
                gate.commit().await.expect("the gate releases");
            }
        );

        let verifier = state.auth.token_verifier.clone().expect("token verifier");
        for completion in [completion_a, completion_b] {
            let session = redeem(&state, &completion).await.expect("redeems");
            let access = SecretString::from(session.access_token.expose().to_owned());
            let roles = verifier
                .verify(&access, &tenant)
                .expect("the session verifies")
                .principal
                .roles;
            assert_eq!(role_set(roles), BTreeSet::from([BETA.to_owned()]));
        }
        assert_eq!(user_roles(&fixture, user).await, vec![BETA.to_owned()]);
        assert_eq!(
            operation_rows(&fixture, "auth.user.roles.sync").await.len(),
            2,
            "each login changed the durable set once"
        );
    }

    /// Wait until `count` backends of this test database wait on a lock.
    ///
    /// The gate's family lock parks each callback: before role replacement
    /// once the callback serializes, or — without that — the first at the
    /// family lock and the second behind the first's audit chain head, so the
    /// count observes either ordering without timing.
    ///
    /// # Panics
    /// Panics when `pg_stat_activity` cannot be read or the waiters never
    /// appear.
    async fn wait_for_lock_waiters(fixture: &PgFixture, count: i64) {
        tokio::time::timeout(StdDuration::from_secs(30), async {
            loop {
                let waiting: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity
                      WHERE datname = current_database() AND wait_event_type = 'Lock'",
                )
                .fetch_one(fixture.app_pool())
                .await
                .expect("lock state reads");
                if waiting >= count {
                    break;
                }
                tokio::time::sleep(StdDuration::from_millis(5)).await;
            }
        })
        .await
        .expect("the callbacks park behind the gate");
    }

    /// When the role-sync event cannot be staged, the role change, session,
    /// refresh row, and completion all roll back together.
    ///
    /// # Panics
    /// Panics when the login succeeds or leaves anything behind.
    #[tokio::test]
    async fn a_failed_role_sync_audit_rolls_back_the_whole_login() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let binding = committed_active_binding(&fixture).await;
        let (hash, login) = pending_login(&fixture, state_hash(10), binding, "nonce").await;
        let superuser = fixture
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION vala.test_fail_roles_sync_audit()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 IF NEW.operation = 'auth.user.roles.sync' THEN
                   RAISE EXCEPTION 'injected roles sync audit failure';
                 END IF;
                 RETURN NEW;
               END;
               $$;"#,
        )
        .execute(&superuser)
        .await
        .expect("failure function installs");
        sqlx::query(
            r#"CREATE TRIGGER test_fail_roles_sync_audit
               BEFORE INSERT ON vala.audit_staging
               FOR EACH ROW EXECUTE FUNCTION vala.test_fail_roles_sync_audit()"#,
        )
        .execute(&superuser)
        .await
        .expect("failure trigger installs");

        let error = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &identity(EXTERNAL_SUBJECT, None, &["admins"]),
                "req-sync-fail",
            )
            .await
            .expect_err("an unrecordable role change refuses the login");

        assert_eq!(error.code(), "WYRD_AUDIT_503_UNAVAILABLE");
        assert_nothing_persisted(&fixture).await;
        let role_rows: i64 = {
            let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
            sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_user_roles")
                .fetch_one(&mut **conn.transaction())
                .await
                .expect("role count runs")
        };
        assert_eq!(role_rows, 0, "the role assignment rolled back");
    }

    /// A verified test sign-in marks only its bound candidate revision tested,
    /// with the JWKS URI its token was verified against, records one allowed
    /// `identity.oidc.candidate.tested` decision for its tester, and issues
    /// nothing: no User, refresh row, completion, or role sync.
    ///
    /// # Panics
    /// Panics when the test is refused, the stamp or decision differs, or
    /// anything is issued.
    #[tokio::test]
    async fn a_test_sign_in_marks_only_its_candidate_tested_and_issues_nothing() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let tester = seed_tester(&fixture, true).await;
        let binding = committed_candidate_binding(&fixture).await;
        let (hash, login) = pending_login_with(
            &fixture,
            state_hash(20),
            binding,
            "nonce",
            LoginInitiation::ConnectionTest(tester),
        )
        .await;

        let completed = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &identity(EXTERNAL_SUBJECT, Some("ext@example.com"), &["admins"]),
                "req-test",
            )
            .await
            .expect("the test sign-in completes");

        assert!(matches!(completed, LoginCompletion::ConnectionTested));
        assert_eq!(
            candidate_stamp(&fixture).await,
            (
                Some(binding.connection_revision),
                Some(jwks_uri(&server).to_string())
            )
        );
        assert_eq!(
            operation_rows(&fixture, "identity.oidc.candidate.tested").await,
            vec![(
                tester.principal_id.as_uuid(),
                "allowed".to_owned(),
                "identity:oidc_connection".to_owned()
            )]
        );
        assert_nothing_persisted(&fixture).await;
    }

    /// A tester whose stored roles no longer grant
    /// `identity_connections:write` is refused at the stamp: the denial is
    /// audited and the candidate stays untested.
    ///
    /// # Panics
    /// Panics when the test is accepted, the denial is missing, or the
    /// candidate is stamped.
    #[tokio::test]
    async fn an_unauthorized_tester_leaves_the_candidate_untested() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let tester = seed_tester(&fixture, false).await;
        let binding = committed_candidate_binding(&fixture).await;
        let (hash, login) = pending_login_with(
            &fixture,
            state_hash(21),
            binding,
            "nonce",
            LoginInitiation::ConnectionTest(tester),
        )
        .await;

        let error = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &identity(EXTERNAL_SUBJECT, None, &[]),
                "req-test-denied",
            )
            .await
            .expect_err("an unauthorized tester is refused");

        assert_eq!(error.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(
            operation_rows(&fixture, "identity.oidc.candidate.tested").await,
            vec![(
                tester.principal_id.as_uuid(),
                "denied".to_owned(),
                "identity:oidc_connection".to_owned()
            )]
        );
        assert_eq!(candidate_stamp(&fixture).await, (None, None));
        assert_nothing_persisted(&fixture).await;
    }

    /// A tested decision that cannot be recorded fails closed: the stamp
    /// shares its transaction, so the candidate stays untested.
    ///
    /// # Panics
    /// Panics when the trigger cannot be installed, the test succeeds, or the
    /// candidate is stamped.
    #[tokio::test]
    async fn a_failed_tested_audit_leaves_the_candidate_untested() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let server = jwks_server().await;
        let state = test_state_with_human_connections(&fixture).await;
        let trusted = sync_trusted(&fixture, &server).await;
        let tester = seed_tester(&fixture, true).await;
        let binding = committed_candidate_binding(&fixture).await;
        let (hash, login) = pending_login_with(
            &fixture,
            state_hash(22),
            binding,
            "nonce",
            LoginInitiation::ConnectionTest(tester),
        )
        .await;
        let superuser = fixture
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION vala.test_fail_candidate_tested_audit()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 IF NEW.operation = 'identity.oidc.candidate.tested' THEN
                   RAISE EXCEPTION 'injected candidate tested audit failure';
                 END IF;
                 RETURN NEW;
               END;
               $$;"#,
        )
        .execute(&superuser)
        .await
        .expect("failure function installs");
        sqlx::query(
            r#"CREATE TRIGGER test_fail_candidate_tested_audit
               BEFORE INSERT ON vala.audit_staging
               FOR EACH ROW EXECUTE FUNCTION vala.test_fail_candidate_tested_audit()"#,
        )
        .execute(&superuser)
        .await
        .expect("failure trigger installs");

        let error = authorization_exchange_service(&state)
            .finish_id_token_exchange(
                &hash,
                &trusted,
                &login,
                &identity(EXTERNAL_SUBJECT, None, &[]),
                "req-test-audit-fail",
            )
            .await
            .expect_err("an unrecordable tested decision refuses the test");

        assert_eq!(error.code(), "WYRD_AUDIT_503_UNAVAILABLE");
        assert_eq!(candidate_stamp(&fixture).await, (None, None));
    }

    /// Seed and commit a password User of the fixture tenant to begin a
    /// connection test as; when `authorized`, it holds a role granting
    /// `identity_connections:write`.
    ///
    /// # Panics
    /// Panics when the user, role, or grant cannot be written.
    async fn seed_tester(fixture: &PgFixture, authorized: bool) -> ConnectionTester {
        let user = Uuid::now_v7();
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        insert_user(&mut conn, user, Some("admin@example.com"), "password", None)
            .await
            .expect("tester inserts");
        if authorized {
            insert_role(
                &mut conn,
                Uuid::now_v7(),
                "connection_tester",
                &serde_json::to_value([Permission::identity_connections_write()])
                    .expect("permission encodes"),
                false,
            )
            .await
            .expect("role inserts");
            replace_user_roles(&mut conn, user, &["connection_tester"])
                .await
                .expect("role grants");
        }
        conn.commit().await.expect("tester commits");
        ConnectionTester {
            principal_id: PrincipalId::new(user),
            principal_kind: PrincipalKindTag::User,
        }
    }

    /// Stage and commit an untested public candidate for the test issuer,
    /// returning the binding a test sign-in of it records.
    ///
    /// # Panics
    /// Panics when the candidate cannot be written.
    async fn committed_candidate_binding(fixture: &PgFixture) -> HumanConnectionBinding {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = insert_human_candidate(
            &mut conn,
            &HumanConnectionWrite {
                issuer_url: EXTERNAL_ISSUER.to_owned(),
                client_id: EXTERNAL_AUDIENCE.to_owned(),
                client_auth: "Public".to_owned(),
                client_secret_enc: None,
                claim_mapping: serde_json::json!({ "subject": "sub" }),
                group_role_map: serde_json::json!({}),
                jwks_ttl_secs: 300,
            },
        )
        .await
        .expect("candidate inserts");
        conn.commit().await.expect("candidate commits");
        HumanConnectionBinding {
            connection_id: row.connection_id,
            connection_revision: row.revision,
        }
    }

    /// The candidate's `(tested_revision, jwks_uri)`.
    ///
    /// # Panics
    /// Panics when the candidate cannot be read or is missing.
    async fn candidate_stamp(fixture: &PgFixture) -> (Option<i64>, Option<String>) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let row = human_connection_in_state(&mut conn, "Candidate")
            .await
            .expect("candidate reads")
            .expect("candidate exists");
        (row.tested_revision, row.jwks_uri)
    }

    /// The tenant role the role-sync cases map a provider group to.
    const SYNC_ROLE: &str = "login_sync_probe";

    /// Create and commit tenant role `name`.
    ///
    /// # Panics
    /// Panics when the role cannot be written.
    async fn seed_role(fixture: &PgFixture, name: &str) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        insert_role(
            &mut conn,
            Uuid::now_v7(),
            name,
            &serde_json::json!([]),
            false,
        )
        .await
        .expect("role inserts");
        conn.commit().await.expect("role commits");
    }

    /// Seed [`SYNC_ROLE`] and trust the external issuer at `server` with
    /// group `admins` mapped to it and no default roles, the setup every
    /// role-sync test shares.
    ///
    /// # Panics
    /// Panics when the role cannot be written.
    async fn sync_trusted(fixture: &PgFixture, server: &MockServer) -> TrustedIssuer {
        seed_role(fixture, SYNC_ROLE).await;
        trusted_issuer_with_jwks(
            fixture.data_tenant_id(),
            jwks_uri(server),
            HashMap::from([("admins".to_owned(), vec![SYNC_ROLE.to_owned()])]),
            Vec::new(),
        )
    }

    /// Role names durably granted to `user`.
    ///
    /// # Panics
    /// Panics when the query fails.
    async fn user_roles(fixture: &PgFixture, user: Uuid) -> Vec<String> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        list_user_roles(&mut conn, user)
            .await
            .expect("role list runs")
    }

    /// A mock JWKS endpoint serving the test Ed25519 key.
    async fn jwks_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ed_jwks_json(EXTERNAL_KID)))
            .mount(&server)
            .await;
        server
    }

    /// Assert a refused login left no User, refresh row, authorization code,
    /// or role-sync event behind.
    ///
    /// # Panics
    /// Panics when anything persisted.
    async fn assert_nothing_persisted(fixture: &PgFixture) {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        let (users, refresh): (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM wyrd.auth_users WHERE auth_type = 'oidc'),
                    (SELECT COUNT(*) FROM wyrd.auth_refresh_tokens)",
        )
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("persistence counts run");
        assert_eq!((users, refresh), (0, 0));
        assert!(
            operation_rows(fixture, "auth.user.roles.sync")
                .await
                .is_empty()
        );
        assert_eq!(issued_codes(fixture).await, 0, "no code was issued");
    }

    /// How many authorization codes the tenant's login states hold.
    ///
    /// # Panics
    /// Panics when the count fails.
    async fn issued_codes(fixture: &PgFixture) -> i64 {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_scalar("SELECT COUNT(*) FROM wyrd.auth_login_state WHERE code_hash IS NOT NULL")
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("code count runs")
    }

    /// Staged events of `operation` as `(principal_id, outcome, resource)`,
    /// oldest first.
    ///
    /// # Panics
    /// Panics when the query fails.
    async fn operation_rows(fixture: &PgFixture, operation: &str) -> Vec<(Uuid, String, String)> {
        let mut conn = fixture.tenant_conn().await.expect("tenant conn opens");
        sqlx::query_as(
            "SELECT principal_id, outcome, resource
               FROM vala.audit_staging
              WHERE operation = $1
              ORDER BY seq ASC",
        )
        .bind(operation)
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("audit query runs")
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

    /// The identity a verified ID token for `subject` maps to.
    fn identity(subject: &str, email: Option<&str>, groups: &[&str]) -> MappedClaims {
        MappedClaims {
            subject: subject.to_owned(),
            email: email.map(ToOwned::to_owned),
            groups: groups.iter().map(|group| (*group).to_owned()).collect(),
        }
    }

    /// Redeem the authorization code of an authorized `completion` as the
    /// `wyrd-ui` client that requested it, with the matching verifier.
    ///
    /// # Errors
    /// Returns the redemption refusal.
    ///
    /// # Panics
    /// Panics when `completion` issued no code.
    async fn redeem(
        state: &AppState,
        completion: &LoginCompletion,
    ) -> Result<wyrd_spec::auth::TokenResponse, wyrd_spec::error::WyrdError> {
        let LoginCompletion::Authorized { code, .. } = completion else {
            panic!("the login issued no authorization code");
        };
        authorization_exchange_service(state)
            .redeem_code(
                &SecretBearer::new(code.expose_secret().to_owned()),
                OAuthClientId::WyrdUi,
                UI_REDIRECT,
                &SecretBearer::new(CODE_VERIFIER.to_owned()),
                "req-redeem",
            )
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

    /// Record and consume a `wyrd-ui` authorization-request login bound to
    /// `connection`, as the callback does before provider IO, and return its
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
        let initiation = LoginInitiation::Authorize(ClientAuthorization {
            client: OAuthClientId::WyrdUi,
            redirect_uri: UI_REDIRECT.to_owned(),
            code_challenge: CODE_CHALLENGE.to_owned(),
            state: None,
        });
        pending_login_with(fixture, state_hash, connection, nonce, initiation).await
    }

    /// Record and consume a login bound to `connection` with `initiation`, as
    /// the callback does before provider IO, and return its state hash and
    /// consumed row.
    ///
    /// # Panics
    /// Panics when the row cannot be written or consumed.
    async fn pending_login_with(
        fixture: &PgFixture,
        state_hash: Sha256Hex,
        connection: HumanConnectionBinding,
        nonce: &str,
        initiation: LoginInitiation,
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
                initiation,
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
        identity: &MappedClaims,
    ) -> Result<LoginCompletion, WyrdErrorResponse> {
        let result = authorization_exchange_service(state)
            .finish_id_token_exchange(state_hash, trusted, login, identity, "req")
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
    /// Panics when the test state lacks issuing or connections.
    fn authorization_exchange_service(state: &AppState) -> AuthorizationCodeExchange {
        AuthorizationCodeExchange {
            issuer: state
                .auth
                .tenant_issuer()
                .expect("test state has issuing key"),
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

    /// Build callback test state with a real issuing key, the matching
    /// access-token verifier, and the human-connection owner.
    ///
    /// Human trust is passed to `finish_id_token_exchange` explicitly; a test
    /// that reaches issuance seeds the Active connection its login state is
    /// bound to.
    async fn test_state_with_human_connections(fixture: &PgFixture) -> AppState {
        let sealing_key = Arc::new(SealingKeyring::new(SecretKey::from_bytes([7_u8; 32])));

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
        test_state(fixture)
            .await
            .with_auth(crate::components::auth::ServerAuth {
                issuing_key: Some(issuing_key),
                token_verifier: Some(Arc::new(verifier)),
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
