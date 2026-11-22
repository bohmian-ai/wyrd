//! The advertised sign-in methods: operator and tenant settings in, the one
//! capability report out, and every existing consumer refusing a method the
//! report no longer offers.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, body_json, enable_google, initialize_platform_root, mount_platform_provider,
    platform_key_set, platform_session, session_cookie, social_call, social_request,
};

/// A server whose Google endpoints are `google`, with its operator session.
///
/// # Panics
/// Panics when the server or operator session fails.
async fn methods_server(google: &wiremock::MockServer) -> (WyrdTestServer, String) {
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
    let session = platform_session(&srv, secrecy::ExposeSecret::expose_secret(&root)).await;
    (srv, session)
}

/// The methods report for `tenant`, or the root report.
///
/// # Panics
/// Panics when the report is refused.
async fn methods(srv: &WyrdTestServer, tenant: Option<&str>) -> Value {
    let uri = tenant.map_or_else(
        || "/auth/browser/methods".to_owned(),
        |slug| format!("/auth/browser/methods?tenant={slug}"),
    );
    let (status, body) = social_call(srv, social_request(Method::GET, &uri, None, None)).await;
    assert_eq!(status, StatusCode::OK, "methods: {body}");
    body
}

/// Apply operator login `settings`.
///
/// # Panics
/// Panics when the change is refused.
async fn login_settings(srv: &WyrdTestServer, session: &str, password: bool, signup: bool) {
    let (status, body) = social_call(
        srv,
        social_request(
            Method::PUT,
            "/platform/login-settings",
            Some(session),
            Some(json!({ "password_enabled": password, "signup_enabled": signup })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "settings change: {body}");
}

/// The full report with every flag `false` and no providers, as a base to
/// override.
fn nothing() -> Value {
    json!({
        "password": false,
        "company_sso": false,
        "providers": [],
        "signup": false,
        "local_signup": false,
        "email_discovery": false,
        "password_reset": false,
        "email_verification_required": true,
        "signup_providers": [],
    })
}

/// `base` with `changes` applied.
fn with(mut base: Value, changes: Value) -> Value {
    for (key, value) in changes.as_object().expect("an object") {
        base[key] = value.clone();
    }
    base
}

/// The methods report states exactly what is usable now:
///   1. defaults offer tenant password only, and no creation;
///   2. signup switched on without SMTP or a provider still offers no creation;
///   3. an SMTP relay makes local signup, discovery and reset usable;
///   4. an enabled provider is both returning and creation method;
///   5. disabling password removes password, local signup and reset;
///   6. required company SSO with an Active connection leaves only company
///      SSO; without a usable connection it leaves no tenant method;
///   7. root never promises password or company SSO.
///
/// # Panics
/// Panics when a report states a capability other than the current one.
#[tokio::test(flavor = "multi_thread")]
async fn methods_report_exactly_the_usable_capabilities() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = methods_server(&google).await;
    let tenant = srv.pg_fixture().tenant_slug().to_owned();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");

    // 1. Defaults.
    let password_only = with(nothing(), json!({ "password": true }));
    assert_eq!(methods(&srv, Some(&tenant)).await, password_only);

    // 2. A switch alone is not a method.
    login_settings(&srv, &session, true, true).await;
    assert_eq!(methods(&srv, Some(&tenant)).await, password_only);

    // 3. Mail makes local creation, discovery and reset usable.
    let (status, body) = social_call(
        &srv,
        social_request(
            Method::PUT,
            "/platform/mail",
            Some(&session),
            Some(json!({ "host": "127.0.0.1", "port": 2525, "tls": "none", "sender": "wyrd@wyrd.test" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "relay stores: {body}");
    let mailed = json!({
        "password": true, "signup": true, "local_signup": true,
        "email_discovery": true, "password_reset": true,
    });
    assert_eq!(
        methods(&srv, Some(&tenant)).await,
        with(nothing(), mailed.clone())
    );

    // 4. An enabled provider returns and creates.
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, &google).await;
    let everything = with(
        with(nothing(), mailed),
        json!({ "providers": ["google"], "signup_providers": ["google"] }),
    );
    assert_eq!(methods(&srv, Some(&tenant)).await, everything);
    login_settings(&srv, &session, true, false).await;
    assert_eq!(
        methods(&srv, Some(&tenant)).await,
        with(
            nothing(),
            json!({ "password": true, "providers": ["google"], "email_discovery": true, "password_reset": true }),
        ),
        "signup off keeps returning login and mail"
    );

    // 5. Password off.
    login_settings(&srv, &session, false, true).await;
    assert_eq!(
        methods(&srv, Some(&tenant)).await,
        with(
            nothing(),
            json!({ "providers": ["google"], "signup": true, "signup_providers": ["google"], "email_discovery": true }),
        )
    );
    login_settings(&srv, &session, true, true).await;

    // 6. Required company SSO.
    sqlx::query(
        "INSERT INTO wyrd.auth_login_policies (data_tenant_id, require_company_sso)
         VALUES ($1, true)",
    )
    .bind(srv.data_tenant_id().as_uuid())
    .execute(&superuser)
    .await
    .expect("policy requires company SSO");
    let deployment = json!({
        "signup": true, "local_signup": true, "signup_providers": ["google"],
        "email_discovery": true,
    });
    assert_eq!(
        methods(&srv, Some(&tenant)).await,
        with(nothing(), deployment.clone()),
        "required company SSO without an Active connection leaves no tenant method"
    );
    let mut conn = srv
        .tenant_conn_for(srv.data_tenant_id())
        .await
        .expect("tenant conn opens");
    wyrd_dev_fixtures::pg::seed_active_human_connection(&mut conn)
        .await
        .expect("company connection seeds");
    conn.commit().await.expect("seed commits");
    assert_eq!(
        methods(&srv, Some(&tenant)).await,
        with(
            with(nothing(), deployment.clone()),
            json!({ "company_sso": true })
        ),
        "required company SSO is the only tenant method"
    );

    // 7. Root.
    assert_eq!(methods(&srv, None).await, with(nothing(), deployment));
}

/// A same-origin browser `method` to `uri` carrying `cookie`; returns the
/// status, the `slug` session cookie it set, and the body.
async fn browser(
    srv: &WyrdTestServer,
    uri: &str,
    slug: &str,
    cookie: Option<&str>,
    body: &Value,
) -> (StatusCode, Option<String>, Value) {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    let resp = srv
        .oneshot(
            builder
                .body(Body::from(serde_json::to_vec(body).expect("serializes")))
                .expect("request builds"),
        )
        .await
        .expect("route responds");
    let status = resp.status();
    let session = session_cookie(&resp, slug);
    (status, session, body_json(resp).await)
}

/// Assert `refused` is the method-unavailable refusal and set no session.
///
/// # Panics
/// Panics when the path acted.
fn assert_unavailable(case: &str, refused: &(StatusCode, Option<String>, Value)) {
    assert_eq!(refused.0, StatusCode::UNAUTHORIZED, "{case}: {}", refused.2);
    assert_eq!(refused.2["code"], "WYRD_AUTH_401_INVALID_TOKEN", "{case}");
    assert_eq!(
        refused.2["details"]["reason"], "method_unavailable",
        "{case}"
    );
    assert!(refused.1.is_none(), "{case}: no session is set");
}

/// Every delivered password and social start path refuses a method the
/// report stopped offering after the page rendered, with no User or session:
///   1. password disabled refuses login, invited enrollment, change and
///      administrator-reset redemption;
///   2. required company SSO refuses them again, and a returning social
///      start;
///   3. lifting both restores password sign-in.
///
/// # Panics
/// Panics when a refused path signs in, enrolls or changes a password.
#[tokio::test(flavor = "multi_thread")]
async fn delivered_paths_refuse_a_method_no_longer_offered() {
    let google = wiremock::MockServer::start().await;
    let (srv, session) = methods_server(&google).await;
    mount_platform_provider(
        &google,
        platform_key_set("google-key"),
        wiremock::ResponseTemplate::new(500),
    )
    .await;
    enable_google(&srv, &session, &google).await;
    let slug = srv.pg_fixture().tenant_slug().to_owned();
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let admin = srv
        .bootstrap_service_in_tenant(srv.data_tenant_id(), "methods-admin", &["admin"])
        .await
        .expect("tenant admin bootstraps");
    let admin = srv
        .exchange_api_key(admin.api_key().expect("admin key"))
        .await
        .expect("admin key exchanges");
    let secret = "quiet harbor lantern";
    let (status, created) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/v1/principals/users",
            Some(&admin),
            Some(json!({
                "username": "ada", "password": secret,
                "assignments": [{ "role": "viewer", "scope": "all" }],
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let user = created["principal_id"].as_str().expect("id").to_owned();
    let login = json!({ "tenant": slug, "username": "ada", "password": secret });
    let signed_in = browser(&srv, "/auth/browser/password/login", &slug, None, &login).await;
    assert_eq!(signed_in.0, StatusCode::OK, "{}", signed_in.2);
    let cookie = signed_in.1.expect("a session is set");
    let users = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM wyrd.auth_users")
            .fetch_one(&superuser)
            .await
            .expect("users count")
    };

    let refusals = |case: &'static str| {
        let (srv, slug, cookie, admin, user) = (&srv, &slug, &cookie, &admin, &user);
        let login = login.clone();
        async move {
            let before = users().await;
            assert_unavailable(
                &format!("{case}: login"),
                &browser(srv, "/auth/browser/password/login", slug, None, &login).await,
            );
            let (status, invited) = social_call(
                srv,
                social_request(
                    Method::POST,
                    "/v1/principals/users/invitations",
                    Some(admin),
                    Some(json!({ "recipients": ["grace@example.test"], "assignments": [{ "role": "viewer", "scope": "all" }] })),
                ),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{invited}");
            let link = invited["invitations"][0]["invitation_link"]
                .as_str()
                .expect("link");
            let code = link.rsplit_once("invitation=").expect("code").1;
            assert_unavailable(
                &format!("{case}: enrollment"),
                &browser(
                    srv,
                    "/auth/browser/password/signup",
                    slug,
                    None,
                    &json!({ "tenant": slug, "invitation_code": code, "username": "grace", "password": "amber kestrel lantern" }),
                )
                .await,
            );
            assert_eq!(users().await, before, "{case}: enrollment created no User");
            assert_unavailable(
                &format!("{case}: change"),
                &browser(
                    srv,
                    "/auth/browser/password/change",
                    slug,
                    Some(cookie),
                    &json!({ "tenant": slug, "current_password": "quiet harbor lantern", "new_password": "amber kestrel lantern" }),
                )
                .await,
            );
            let (status, reset) = social_call(
                srv,
                social_request(
                    Method::POST,
                    &format!("/v1/principals/users/{user}/password-reset"),
                    Some(admin),
                    None,
                ),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED, "{reset}");
            assert_unavailable(
                &format!("{case}: reset"),
                &browser(
                    srv,
                    "/auth/browser/password/reset",
                    slug,
                    None,
                    &json!({ "tenant": slug, "reset_code": reset["reset_code"], "new_password": "amber kestrel lantern" }),
                )
                .await,
            );
        }
    };

    // 1. Password disabled after the page rendered.
    login_settings(&srv, &session, false, false).await;
    refusals("password disabled").await;
    login_settings(&srv, &session, true, false).await;

    // 2. Company SSO required.
    sqlx::query(
        "INSERT INTO wyrd.auth_login_policies (data_tenant_id, require_company_sso)
         VALUES ($1, true)",
    )
    .bind(srv.data_tenant_id().as_uuid())
    .execute(&superuser)
    .await
    .expect("policy requires company SSO");
    refusals("company SSO required").await;
    assert_unavailable(
        "company SSO required: social start",
        &browser(
            &srv,
            "/auth/browser/login",
            &slug,
            None,
            &json!({ "tenant": slug, "provider": "google" }),
        )
        .await,
    );

    // 3. Lifted, the old password still signs in.
    sqlx::query("DELETE FROM wyrd.auth_login_policies")
        .execute(&superuser)
        .await
        .expect("policy lifts");
    let again = browser(&srv, "/auth/browser/password/login", &slug, None, &login).await;
    assert_eq!(again.0, StatusCode::OK, "{}", again.2);
}
