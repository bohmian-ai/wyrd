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
        let code = self.invitation_code().await;
        begin_google(
            &self.srv,
            json!({ "tenant": self.slug, "provider": "google", "invitation_code": code }),
        )
        .await
    }

    /// Invite one person as a `viewer`; returns the link's code.
    ///
    /// # Panics
    /// Panics when the invitation is refused.
    async fn invitation_code(&self) -> String {
        let (status, issued) = self
            .bearer(
                Method::POST,
                "/v1/principals/users/invitations",
                Some(json!({
                    "recipients": ["invitee@example.test"],
                    "assignments": [{ "role": "viewer", "scope": "all" }],
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "invitation: {issued}");
        issued["invitations"][0]["invitation_link"]
            .as_str()
            .and_then(|link| link.split_once("#invitation="))
            .map(|(_, code)| code.to_owned())
            .expect("the link carries its code")
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

impl Workspace {
    /// Read the login policy as the browser holding `cookie`.
    ///
    /// # Panics
    /// Panics when the read is refused.
    async fn policy(&self, cookie: &str) -> Value {
        let (status, policy) = self.browser(Method::GET, POLICY, cookie, None).await;
        assert_eq!(status, StatusCode::OK, "policy reads: {policy}");
        policy
    }

    /// Submit `require` against the revisions in `read`, as the browser
    /// holding `cookie`.
    async fn change(&self, cookie: &str, require: bool, read: &Value) -> (StatusCode, Value) {
        self.browser(
            Method::PUT,
            POLICY,
            cookie,
            Some(change_body(require, read)),
        )
        .await
    }

    /// Whether the browser session `cookie` is still live.
    async fn live(&self, cookie: &str) -> bool {
        let uri = format!("/auth/browser/session?tenant={}", self.slug);
        let (status, _) = self.browser(Method::GET, &uri, cookie, None).await;
        status == StatusCode::OK
    }

    /// A browser session recovered with the headless administrator's API key.
    ///
    /// # Panics
    /// Panics when recovery is refused.
    async fn recovery_session(&self) -> String {
        let resp = self
            .srv
            .oneshot(browser_post(
                "/auth/browser/api-key",
                None,
                json!({ "tenant": self.slug, "api_key": self.recovery_key.expose_secret() }),
            ))
            .await
            .expect("recovery responds");
        assert_eq!(resp.status(), StatusCode::OK, "recovery sign-in");
        session_cookie(&resp, &self.slug).expect("recovery sets a session")
    }

    /// Move every browser session of the tenant past the five-minute
    /// recent-sign-in window.
    async fn age_sessions(&self) {
        sqlx::query(
            "UPDATE wyrd.auth_browser_sessions \
             SET created_at = created_at - interval '6 minutes', \
                 expires_at = expires_at - interval '6 minutes'",
        )
        .execute(&self.superuser)
        .await
        .expect("sessions age");
    }
}

/// A policy change body requiring `require` at the revisions `read` names.
fn change_body(require: bool, read: &Value) -> Value {
    json!({
        "require_company_sso": require,
        "expected_revision": read["revision"],
        "expected_connection": read["connection"],
    })
}

/// A company administrator enables and disables required company SSO:
///   1. a company member, the headless bearer, an API-key recovery session,
///      a password administrator, a stale company proof, a stale policy
///      revision and a changed connection revision are each refused, and
///      none changes the policy or ends a credential;
///   2. a fresh company sign-in of a group-mapped administrator enables it
///      although Users without a company identity remain; a second replica
///      reads it and the methods report offers company SSO only;
///   3. once the proof is stale, Disable is refused; signing in again does
///      not change the policy until the administrator confirms, after which
///      password sign-in works again.
///
/// # Panics
/// Panics when any step differs.
#[tokio::test(flavor = "multi_thread")]
async fn a_company_administrator_enables_and_disables_required_sso() {
    let workspace = Workspace::start().await;
    let admin = workspace.company_session("corp-admin", &[ADMINS]).await;
    let member = workspace.company_session("corp-member", &[]).await;
    let password_admin = workspace.password_session("pat-admin", "admin").await;
    let recovery = workspace.recovery_session().await;
    let read = workspace.policy(&admin).await;
    assert_eq!(read["users_without_company_identity"], 2, "{read}");

    // 1. Refusals that change nothing.
    let (status, refused) = workspace.change(&member, true, &read).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member: {refused}");
    assert_eq!(refused["code"], "WYRD_PERMISSION_403_DENIED_RBAC");
    let (status, refused) = workspace
        .bearer(Method::PUT, POLICY, Some(change_body(true, &read)))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "bearer: {refused}");
    assert_eq!(refused["code"], "WYRD_AUTH_403_RECENT_LOGIN_REQUIRED");
    for (case, cookie) in [("recovery", &recovery), ("password", &password_admin)] {
        let (status, refused) = workspace.change(cookie, true, &read).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{case}: {refused}");
        assert_eq!(
            refused["code"], "WYRD_AUTH_403_RECENT_LOGIN_REQUIRED",
            "{case}"
        );
    }
    let mut stale = read.clone();
    stale["revision"] = json!(7);
    let (status, refused) = workspace.change(&admin, true, &stale).await;
    assert_eq!(status, StatusCode::CONFLICT, "stale policy: {refused}");
    assert_eq!(refused["details"]["reason"], "policy_changed");
    let mut moved = read.clone();
    moved["connection"]["revision"] = json!(99);
    let (status, refused) = workspace.change(&admin, true, &moved).await;
    assert_eq!(status, StatusCode::CONFLICT, "moved connection: {refused}");
    assert_eq!(refused["details"]["reason"], "connection_changed");
    workspace.age_sessions().await;
    let (status, refused) = workspace.change(&admin, true, &read).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "stale proof: {refused}");
    assert_eq!(refused["code"], "WYRD_AUTH_403_RECENT_LOGIN_REQUIRED");
    assert_eq!(workspace.policy(&admin).await, read, "nothing changed");
    assert!(
        workspace.live(&password_admin).await,
        "password session kept"
    );

    // 2. A fresh company proof enables; a replica and the methods report agree.
    let admin = workspace.company_session("corp-admin", &[ADMINS]).await;
    let (status, enabled) = workspace.change(&admin, true, &read).await;
    assert_eq!(status, StatusCode::OK, "enable: {enabled}");
    assert_eq!(enabled["require_company_sso"], true);
    assert_eq!(enabled["revision"], 1);
    let replica = workspace
        .srv
        .start_replica(wyrd_testing::WyrdTestServerBuilder::default())
        .await
        .expect("replica boots");
    let (status, replicated) = social_call(
        &replica,
        social_request(Method::GET, POLICY, Some(&workspace.admin), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "replica read: {replicated}");
    assert_eq!(replicated, enabled, "the replica reads the same policy");
    let methods = body_json(
        workspace
            .srv
            .oneshot(browser_request(
                Method::GET,
                &format!("/auth/browser/methods?tenant={}", workspace.slug),
                None,
                None,
            ))
            .await
            .expect("methods respond"),
    )
    .await;
    assert_eq!(methods["company_sso"], true, "{methods}");
    assert_eq!(methods["password"], false, "{methods}");
    assert_eq!(methods["providers"], json!([]), "{methods}");

    // 3. Disable needs fresh proof; signing in alone changes nothing.
    workspace.age_sessions().await;
    let (status, refused) = workspace.change(&admin, false, &enabled).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "stale disable: {refused}");
    let admin = workspace.company_session("corp-admin", &[ADMINS]).await;
    assert_eq!(
        workspace.policy(&admin).await,
        enabled,
        "login alone mutates nothing"
    );
    let (status, disabled) = workspace.change(&admin, false, &enabled).await;
    assert_eq!(status, StatusCode::OK, "disable: {disabled}");
    assert_eq!(disabled["require_company_sso"], false);
    assert_eq!(disabled["revision"], 2);
    let resp = workspace.password_login("pat-admin").await;
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "password sign-in works again"
    );
}

impl Workspace {
    /// Require company SSO through a fresh company administrator sign-in;
    /// returns that administrator's session and the enabled view.
    ///
    /// # Panics
    /// Panics when the change is refused.
    async fn enable(&self) -> (String, Value) {
        let admin = self.company_session("corp-admin", &[ADMINS]).await;
        let read = self.policy(&admin).await;
        let (status, enabled) = self.change(&admin, true, &read).await;
        assert_eq!(status, StatusCode::OK, "enable: {enabled}");
        (admin, enabled)
    }

    /// Start a CLI device authorization; returns its device and user codes.
    ///
    /// # Panics
    /// Panics when the authorization is refused.
    async fn device(&self) -> (String, String) {
        let resp = self
            .srv
            .oneshot(form_post(
                "/auth/device_authorization",
                None,
                &[("client_id", "wyrd-cli"), ("tenant", &self.slug)],
            ))
            .await
            .expect("device authorization responds");
        assert_eq!(resp.status(), StatusCode::OK, "device authorization");
        let device = body_json(resp).await;
        (
            device["device_code"]
                .as_str()
                .expect("device code")
                .to_owned(),
            device["user_code"].as_str().expect("user code").to_owned(),
        )
    }

    /// Approve `user_code` from the browser holding `cookie`.
    async fn approve(&self, cookie: &str, user_code: &str) -> StatusCode {
        self.srv
            .oneshot(form_post(
                "/auth/device",
                Some(cookie),
                &[
                    ("tenant", &self.slug),
                    ("user_code", user_code),
                    ("decision", "approve"),
                ],
            ))
            .await
            .expect("approval responds")
            .status()
    }

    /// Poll the token endpoint for `device_code`.
    async fn redeem(&self, device_code: &str) -> (StatusCode, Value) {
        let resp = self
            .srv
            .oneshot(form_post(
                "/auth/token",
                None,
                &[
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device_code),
                    ("client_id", "wyrd-cli"),
                ],
            ))
            .await
            .expect("token responds");
        let status = resp.status();
        (status, body_json(resp).await)
    }

    /// Finish the Google login `begun` against the reply already mounted;
    /// returns the status and any session set.
    async fn finish(&self, begun: &BegunSocialLogin) -> (StatusCode, Option<String>) {
        let (status, _, cookie) =
            finish_google_session(&self.srv, begun, &self.google.uri(), &self.slug).await;
        (status, cookie)
    }

    /// How many Users the fixture tenant holds.
    async fn users(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM wyrd.auth_users WHERE data_tenant_id = $1")
            .bind(self.srv.data_tenant_id().as_uuid())
            .fetch_one(&self.superuser)
            .await
            .expect("Users count")
    }
}

/// A browser form `POST` to `uri` from the deployment origin.
fn form_post(uri: &str, cookie: Option<&str>, pairs: &[(&str, &str)]) -> Request<Body> {
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish();
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(body)).expect("request builds")
}

/// Required company SSO cannot be bypassed at a later login step:
///   1. a Google sign-in and a Google invitation enrollment begun while
///      optional are refused at their callbacks, setting no session and
///      creating no User;
///   2. a password invitation signup, a password sign-in and a new Google
///      start are refused directly over HTTP;
///   3. a device code a password session approved before the change redeems
///      nothing, and a password session cannot approve a new one;
///   4. a company sign-in and a company-approved device code still work.
///
/// # Panics
/// Panics when a disallowed path acts or a company path is refused.
#[tokio::test(flavor = "multi_thread")]
async fn required_sso_refuses_every_later_password_and_social_step() {
    let workspace = Workspace::start().await;
    workspace.google_session("google-sam").await;
    let password = workspace.password_session("pat-admin", "admin").await;
    let returning = begin_google(
        &workspace.srv,
        json!({ "tenant": workspace.slug, "provider": "google" }),
    )
    .await;
    workspace.answer_google(&returning, "google-sam").await;
    let enrolling = workspace.begin_google_invitation().await;
    let pending_code = workspace.invitation_code().await;
    let (approved_device, approved_user_code) = workspace.device().await;
    assert_eq!(
        workspace.approve(&password, &approved_user_code).await,
        StatusCode::OK,
        "the password session approves while optional"
    );
    let (open_device, open_user_code) = workspace.device().await;
    let (admin, _) = workspace.enable().await;
    let users = workspace.users().await;

    // 1. Completions begun while optional.
    let (status, session) = workspace.finish(&returning).await;
    assert!(session.is_none(), "returning Google sign-in: {status}");
    workspace.answer_google(&enrolling, "google-new").await;
    let (status, session) = workspace.finish(&enrolling).await;
    assert!(session.is_none(), "Google enrollment: {status}");
    assert_eq!(workspace.users().await, users, "no User is created");

    // 2. Direct HTTP starts and one-step completions.
    let resp = workspace
        .srv
        .oneshot(browser_post(
            "/auth/browser/password/signup",
            None,
            json!({
                "tenant": workspace.slug,
                "invitation_code": pending_code,
                "username": "late-invitee",
                "password": password_of("late-invitee"),
            }),
        ))
        .await
        .expect("signup responds");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "password invitation");
    assert!(session_cookie(&resp, &workspace.slug).is_none());
    let resp = workspace.password_login("pat-admin").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "password sign-in");
    let refused = body_json(resp).await;
    assert_eq!(
        refused["details"]["reason"], "method_unavailable",
        "{refused}"
    );
    let resp = workspace
        .srv
        .oneshot(browser_post(
            "/auth/browser/login",
            None,
            json!({ "tenant": workspace.slug, "provider": "google" }),
        ))
        .await
        .expect("login responds");
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "Google start");
    assert_eq!(workspace.users().await, users, "no User is created");

    // 3. Password device authority.
    let (status, token) = workspace.redeem(&approved_device).await;
    assert_ne!(status, StatusCode::OK, "password-approved device: {token}");
    assert!(token.get("access_token").is_none(), "{token}");
    assert_ne!(
        workspace.approve(&password, &open_user_code).await,
        StatusCode::OK,
        "a password session cannot approve"
    );

    // 4. Company paths still work.
    assert_eq!(
        workspace.approve(&admin, &open_user_code).await,
        StatusCode::OK
    );
    let (status, token) = workspace.redeem(&open_device).await;
    assert_eq!(status, StatusCode::OK, "company-approved device: {token}");
    assert!(token["access_token"].is_string());
    workspace.company_session("corp-member", &[]).await;
}
