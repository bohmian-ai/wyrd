//! Required company SSO: a company-authenticated administrator reads the
//! tenant login policy and its account warning, then requires company SSO
//! through tenant Authentication settings; Rust enforces the policy at every
//! password and social start, completion, session and renewal.
//!
//! The company identity provider is a controlled mock: each sign-in answers
//! with an ID token for the nonce Wyrd sent, so a journey chooses who signs in
//! and which groups they carry. The connection still goes through the real
//! stage, test-sign-in and recovery-key activation lifecycle.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::{Value, json};
use sqlx::PgPool;
use wyrd_testing::WyrdTestServer;

use super::{
    BegunSocialLogin, PLATFORM_SIGNING_KEY, SOCIAL_HOST, authorization_query, begin_google,
    begin_social, body_json, enable_google, finish_google_session, identity_token_reply,
    initialize_platform_root, mount_platform_provider, platform_key_set, platform_session,
    session_cookie, social_call, social_request,
};

/// The tenant login-policy route.
const POLICY: &str = "/v1/identity/login-policy";

/// The client id Wyrd holds at the company provider.
const CORP_CLIENT: &str = "wyrd-corp";

/// The company group the connection maps to the tenant `admin` role.
const ADMINS: &str = "wyrd-admins";

/// One workspace with a tested Active company connection, Google enabled by
/// the operator, and the tenant's headless administrator.
struct Workspace {
    /// The in-process server.
    srv: WyrdTestServer,
    /// The controlled company identity provider.
    corp: wiremock::MockServer,
    /// The controlled Google provider.
    google: wiremock::MockServer,
    /// The fixture tenant's route key.
    slug: String,
    /// The headless administrator's bearer token.
    admin: String,
    /// The headless administrator's API key, the activation recovery key.
    recovery_key: SecretString,
    /// A superuser pool for counting stored records.
    superuser: PgPool,
}

impl Workspace {
    /// Start the server, enable Google and activate a company connection
    /// mapping [`ADMINS`] to `admin` through the real lifecycle.
    ///
    /// # Panics
    /// Panics when any setup step is refused.
    async fn start() -> Self {
        let google = wiremock::MockServer::start().await;
        let corp = wiremock::MockServer::start().await;
        let srv = WyrdTestServer::builder()
            .with_social_endpoints_for_test(wyrd_server::config::SocialConfig {
                google_issuer: Some(google.uri()),
                ..Default::default()
            })
            .start_in_process()
            .await
            .expect("server starts");
        let root = initialize_platform_root(&srv)
            .await
            .expect("deployment initializes");
        let session = platform_session(&srv, root.expose_secret()).await;
        mount_platform_provider(
            &google,
            platform_key_set("google-key"),
            wiremock::ResponseTemplate::new(500),
        )
        .await;
        enable_google(&srv, &session, &google).await;
        let tenant = srv.data_tenant_id();
        let bootstrap = srv
            .bootstrap_service_in_tenant(tenant, "headless-admin", &["admin"])
            .await
            .expect("tenant admin bootstraps");
        let recovery_key = bootstrap.api_key().expect("admin has an api key").clone();
        let admin = srv
            .exchange_api_key(&recovery_key)
            .await
            .expect("admin key exchanges");
        let workspace = Self {
            slug: srv.pg_fixture().tenant_slug().to_owned(),
            superuser: srv
                .pg_fixture()
                .superuser_pool()
                .expect("superuser pool opens"),
            srv,
            corp,
            google,
            admin,
            recovery_key,
        };
        workspace.activate_corp().await;
        workspace
    }

    /// Stage the company connection (replacing any candidate), test it
    /// through one company sign-in and activate it with the recovery key;
    /// returns the Active view.
    ///
    /// # Panics
    /// Panics when any step is refused.
    async fn activate_corp(&self) -> Value {
        let (_, listed) = self
            .bearer(Method::GET, "/v1/identity/oidc/connections", None)
            .await;
        let (status, candidate) = self
            .bearer(
                Method::PUT,
                "/v1/identity/oidc/candidate",
                Some(json!({
                    "issuer": self.corp.uri(),
                    "client_id": CORP_CLIENT,
                    "client_auth": "Public",
                    "claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
                    "group_role_map": { ADMINS: ["admin"] },
                    "expected_revision": listed["candidate"]["revision"],
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "candidate stages: {candidate}");
        let revision = candidate["revision"].as_u64().expect("candidate revision");
        mount_platform_provider(
            &self.corp,
            platform_key_set("corp-key"),
            wiremock::ResponseTemplate::new(500),
        )
        .await;
        let (status, begun) = self
            .bearer(
                Method::POST,
                "/v1/identity/oidc/candidate/test",
                Some(json!({ "expected_revision": revision })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "test begins: {begun}");
        let query = authorization_query(begun["authorization_url"].as_str().expect("url"));
        self.answer_corp(&query["nonce"], "corp-tester", &[]).await;
        let resp = self
            .srv
            .oneshot(self.callback_request(&query["state"], None))
            .await
            .expect("callback responds");
        assert_eq!(resp.status(), StatusCode::OK, "the test sign-in completes");
        let (status, active) = self
            .bearer(
                Method::POST,
                "/v1/identity/oidc/candidate/activate",
                Some(json!({
                    "expected_revision": revision,
                    "recovery_api_key": self.recovery_key.expose_secret(),
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "candidate activates: {active}");
        active
    }

    /// Answer the next company token request with an ID token for `nonce`,
    /// `subject` and `groups`.
    async fn answer_corp(&self, nonce: &str, subject: &str, groups: &[&str]) {
        mount_platform_provider(
            &self.corp,
            platform_key_set("corp-key"),
            corp_token(&self.corp.uri(), nonce, subject, groups),
        )
        .await;
    }

    /// The common company callback for `state`, from a browser holding
    /// `cookie` when given.
    fn callback_request(&self, state: &str, cookie: Option<&str>) -> Request<Body> {
        let issuer: String =
            url::form_urlencoded::byte_serialize(self.corp.uri().as_bytes()).collect();
        let mut builder = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/auth/callback?code=corp-code&state={state}&iss={issuer}"
            ))
            .header(header::HOST, SOCIAL_HOST);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        builder.body(Body::empty()).expect("request builds")
    }

    /// Sign `subject` carrying `groups` in through company SSO from a fresh
    /// browser; returns the tenant session cookie pair.
    ///
    /// # Panics
    /// Panics when the sign-in does not set a session.
    async fn company_session(&self, subject: &str, groups: &[&str]) -> String {
        let begun = begin_social(&self.srv, json!({ "tenant": self.slug })).await;
        self.answer_corp(&begun.query["nonce"], subject, groups)
            .await;
        let resp = self
            .srv
            .oneshot(self.callback_request(&begun.query["state"], Some(&begun.cookie)))
            .await
            .expect("callback responds");
        assert_eq!(resp.status(), StatusCode::SEE_OTHER, "company sign-in");
        session_cookie(&resp, &self.slug).expect("company sign-in sets a session")
    }

    /// Create a local password User holding `role` and sign them in;
    /// returns the session cookie pair.
    ///
    /// # Panics
    /// Panics when creation or sign-in is refused.
    async fn password_session(&self, username: &str, role: &str) -> String {
        let (status, created) = self
            .bearer(
                Method::POST,
                "/v1/principals/users",
                Some(json!({
                    "username": username,
                    "password": password_of(username),
                    "assignments": [{ "role": role, "scope": "all" }],
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "User creates: {created}");
        let resp = self.password_login(username).await;
        assert_eq!(resp.status(), StatusCode::OK, "password sign-in");
        session_cookie(&resp, &self.slug).expect("password sign-in sets a session")
    }

    /// `POST /auth/browser/password/login` for `username`.
    async fn password_login(&self, username: &str) -> axum::http::Response<Body> {
        self.srv
            .oneshot(browser_post(
                "/auth/browser/password/login",
                None,
                json!({
                    "tenant": self.slug,
                    "username": username,
                    "password": password_of(username),
                }),
            ))
            .await
            .expect("password login responds")
    }

    /// Invite one person as a `viewer` and enroll them with Google as
    /// `subject`; returns the session cookie pair.
    ///
    /// # Panics
    /// Panics when the invitation or enrollment is refused.
    async fn google_session(&self, subject: &str) -> String {
        let begun = self.begin_google_invitation().await;
        self.answer_google(&begun, subject).await;
        let (status, _, cookie) =
            finish_google_session(&self.srv, &begun, &self.google.uri(), &self.slug).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "Google enrollment");
        cookie.expect("Google enrollment sets a session")
    }

    /// Invite one person as a `viewer` and begin their Google enrollment.
    async fn begin_google_invitation(&self) -> BegunSocialLogin {
        let (status, issued) = self
            .bearer(
                Method::POST,
                "/v1/principals/users/invitations",
                Some(json!({
                    "recipients": ["social@example.test"],
                    "assignments": [{ "role": "viewer", "scope": "all" }],
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "invitation: {issued}");
        let code = issued["invitations"][0]["invitation_link"]
            .as_str()
            .and_then(|link| link.split_once("#invitation="))
            .map(|(_, code)| code.to_owned())
            .expect("the link carries its code");
        begin_google(
            &self.srv,
            json!({ "tenant": self.slug, "provider": "google", "invitation_code": code }),
        )
        .await
    }

    /// Answer the next Google token request for `begun` as `subject`.
    async fn answer_google(&self, begun: &BegunSocialLogin, subject: &str) {
        mount_platform_provider(
            &self.google,
            platform_key_set("google-key"),
            identity_token_reply(
                &self.google.uri(),
                &begun.query["nonce"],
                "wyrd-google",
                "google-key",
                subject,
                &format!("{subject}@gmail.test"),
            ),
        )
        .await;
    }

    /// Call `uri` with the headless administrator's bearer token.
    async fn bearer(&self, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        social_call(
            &self.srv,
            social_request(method, uri, Some(&self.admin), body),
        )
        .await
    }

    /// Call `uri` as a browser holding the session `cookie`, from the
    /// deployment's own origin.
    async fn browser(
        &self,
        method: Method,
        uri: &str,
        cookie: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let resp = self
            .srv
            .oneshot(browser_request(method, uri, Some(cookie), body))
            .await
            .expect("route responds");
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .expect("read body");
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
}

/// A browser `method` request to `uri` from the deployment origin, holding
/// `cookie` when given.
fn browser_request(
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"));
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    match body {
        Some(body) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .expect("request builds"),
        None => builder.body(Body::empty()).expect("request builds"),
    }
}

/// A browser JSON `POST` to `uri`.
fn browser_post(uri: &str, cookie: Option<&str>, body: Value) -> Request<Body> {
    browser_request(Method::POST, uri, cookie, Some(body))
}

/// The fixed password a journey User is created with.
fn password_of(username: &str) -> String {
    format!("Quiet harbor lantern for {username}")
}

/// A company token-endpoint reply: an ID token from `issuer` for `nonce`
/// naming `subject` and carrying `groups`.
///
/// # Panics
/// Panics when the test key does not load or the token does not sign.
fn corp_token(
    issuer: &str,
    nonce: &str,
    subject: &str,
    groups: &[&str],
) -> wiremock::ResponseTemplate {
    let now = chrono::Utc::now().timestamp();
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
    header.kid = Some("corp-key".to_owned());
    let key = jsonwebtoken::EncodingKey::from_ed_pem(PLATFORM_SIGNING_KEY.as_bytes())
        .expect("test key loads");
    let id_token = jsonwebtoken::encode(
        &header,
        &json!({
            "iss": issuer,
            "sub": subject,
            "aud": CORP_CLIENT,
            "exp": now + 3600,
            "iat": now,
            "nonce": nonce,
            "email": format!("{subject}@corp.test"),
            "groups": groups,
        }),
        &key,
    )
    .expect("token signs");
    wiremock::ResponseTemplate::new(200).set_body_json(json!({
        "access_token": "provider-access",
        "token_type": "Bearer",
        "id_token": id_token,
    }))
}

/// A company-authenticated administrator reads the tenant login policy and
/// its account warning from Rust:
///   1. the read names the policy (not required, revision 0), the current
///      Active company connection and its revision, and that it is ready;
///   2. the warning counts exactly the active Users without a company
///      identity: the fixture's seeded admin and the password and Google
///      Users, not the company Users, a suspended User, a Service or another
///      tenant's User;
///   3. a non-administrator's read is refused, and the anonymous methods
///      report carries no count or account list.
///
/// # Panics
/// Panics when any read differs.
#[tokio::test(flavor = "multi_thread")]
async fn a_company_administrator_reads_the_policy_and_its_account_warning() {
    let workspace = Workspace::start().await;
    let (_, connections) = workspace
        .bearer(Method::GET, "/v1/identity/oidc/connections", None)
        .await;
    let active = &connections["active"];
    let admin = workspace.company_session("corp-admin", &[ADMINS]).await;
    workspace.company_session("corp-member", &[]).await;
    let viewer = workspace.password_session("pat-viewer", "viewer").await;
    workspace.google_session("google-sam").await;
    workspace.password_session("sue-suspended", "viewer").await;
    sqlx::query("UPDATE wyrd.auth_users SET status = 'suspended' WHERE username = $1")
        .bind("sue-suspended")
        .execute(&workspace.superuser)
        .await
        .expect("User suspends");
    let other = workspace
        .srv
        .pg_fixture()
        .seed_additional_tenant("company-sso-other")
        .await
        .expect("other tenant seeds");
    workspace
        .srv
        .bootstrap_user_in_tenant(other, "other-tenant-user", &[])
        .await
        .expect("other tenant User bootstraps");

    // 1-2. The policy, its connection and the account warning.
    let (status, policy) = workspace.browser(Method::GET, POLICY, &admin, None).await;
    assert_eq!(status, StatusCode::OK, "policy reads: {policy}");
    assert_eq!(
        policy,
        json!({
            "require_company_sso": false,
            "revision": 0,
            "connection": { "id": active["id"], "revision": active["revision"] },
            "ready": true,
            "users_without_company_identity": 3,
        })
    );

    // 3. A non-administrator and an anonymous visitor learn nothing.
    let (status, refused) = workspace.browser(Method::GET, POLICY, &viewer, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{refused}");
    assert_eq!(refused["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    let resp = workspace
        .srv
        .oneshot(browser_request(
            Method::GET,
            &format!("/auth/browser/methods?tenant={}", workspace.slug),
            None,
            None,
        ))
        .await
        .expect("methods respond");
    let methods = body_json(resp).await;
    for leaked in ["users_without_company_identity", "users", "revision"] {
        assert!(methods.get(leaked).is_none(), "{leaked} in {methods}");
    }
}
