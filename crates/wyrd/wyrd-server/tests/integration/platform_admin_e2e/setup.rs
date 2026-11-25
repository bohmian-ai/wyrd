//! Setup journeys: `wyrd-server setup` makes a fresh deployment usable with
//! a named human administrator, without SMTP or an identity provider.

use std::io::Write as _;
use std::process::{Output, Stdio};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use serde_json::{Value, json};
use wyrd_testing::WyrdTestServer;

use super::{
    SOCIAL_HOST, body_json, operator_command, session_cookie, social_call, social_request,
};

/// The administrator's password, which no output may contain.
const PASSWORD: &str = "amber kestrel lantern";

/// Run `wyrd-server setup` for tenant `acme` against `srv` with `extra`
/// arguments, writing `stdin` when given and closing it otherwise.
///
/// # Panics
/// Panics when the binary cannot be run.
fn setup(
    srv: &WyrdTestServer,
    extra: &[&str],
    stdin: Option<&str>,
    env: &[(&str, &str)],
) -> Output {
    let mut command = operator_command("setup", srv.pg_fixture().database_name());
    command
        .args([
            "--tenant",
            "acme",
            "--server",
            srv.base_url().expect("bound"),
        ])
        .args(extra)
        .envs(env.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("setup runs");
    let mut pipe = child.stdin.take().expect("stdin is piped");
    if let Some(stdin) = stdin {
        pipe.write_all(stdin.as_bytes()).expect("stdin writes");
    }
    drop(pipe);
    child.wait_with_output().expect("setup finishes")
}

/// The output's stdout and stderr, asserting neither shows the password.
fn printed(output: &Output) -> (String, String) {
    let stdout = String::from_utf8(output.stdout.clone()).expect("UTF-8");
    let stderr = String::from_utf8(output.stderr.clone()).expect("UTF-8");
    assert!(
        !stdout.contains(PASSWORD) && !stderr.contains(PASSWORD),
        "setup echoed the password: {stdout} {stderr}"
    );
    (stdout, stderr)
}

/// The value printed after `name: `.
fn field<'a>(stdout: &'a str, name: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .unwrap_or_else(|| panic!("setup prints {name}: {stdout}"))
}

/// `acme`'s directory status and its Users: username, profile email,
/// whether it holds the admin role, and its verified contacts.
async fn acme(
    superuser: &sqlx::PgPool,
) -> (
    Vec<String>,
    Vec<(Option<String>, Option<String>, bool)>,
    i64,
) {
    let statuses = sqlx::query_scalar("SELECT status FROM platform.tenants WHERE slug = 'acme'")
        .fetch_all(superuser)
        .await
        .expect("directory reads");
    let users = sqlx::query_as(
        "SELECT u.username, u.email,
                EXISTS (SELECT 1 FROM wyrd.auth_role_bindings b
                          JOIN wyrd.auth_roles r ON r.id = b.role_id
                         WHERE b.user_id = u.id AND r.name = 'admin')
           FROM wyrd.auth_users u
           JOIN platform.tenants t ON t.data_tenant_id = u.data_tenant_id
          WHERE t.slug = 'acme'",
    )
    .fetch_all(superuser)
    .await
    .expect("users read");
    let contacts = sqlx::query_scalar(
        "SELECT count(*) FROM wyrd.auth_user_contacts c
           JOIN platform.tenants t ON t.data_tenant_id = c.data_tenant_id
          WHERE t.slug = 'acme'",
    )
    .fetch_one(superuser)
    .await
    .expect("contacts read");
    (statuses, users, contacts)
}

/// A browser `POST` of `body` to `uri`, form-encoded when `form`.
fn browser_post(uri: &str, cookie: Option<&str>, body: String, form: bool) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::HOST, SOCIAL_HOST)
        .header(header::ORIGIN, format!("http://{SOCIAL_HOST}"))
        .header(
            header::CONTENT_TYPE,
            if form {
                "application/x-www-form-urlencoded"
            } else {
                "application/json"
            },
        );
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder.body(Body::from(body)).expect("request builds")
}

/// Sign in to `acme` as `founder` with `password`; returns the status and
/// session cookie pair.
async fn sign_in(srv: &WyrdTestServer, password: &str) -> (StatusCode, Option<String>) {
    let resp = srv
        .oneshot(browser_post(
            "/auth/browser/password/login",
            None,
            json!({ "tenant": "acme", "username": "founder", "password": password }).to_string(),
            false,
        ))
        .await
        .expect("login responds");
    let status = resp.status();
    (status, session_cookie(&resp, "acme"))
}

/// Setup creates a named human administrator a person can use at once:
///   1. without `--admin-username`, or without a password on stdin or a
///      terminal, setup fails before claiming anything;
///   2. setup interrupted before activation leaves `acme` unactivated, and
///      rerunning it resumes to one active tenant with exactly one human
///      admin; the password never appears in any output, the optional email
///      stays unverified, and the automation credential is still printed;
///   3. setup prints the login URL and username; the admin signs in with the
///      password, approves a real CLI device code with that session, and the
///      CLI token reads the admin's own principal;
///   4. a completed rerun changes nothing and resets no password;
///   5. machine-only tenant creation still needs no human admin.
///
/// # Panics
/// Panics when setup or a sign-in step differs.
#[tokio::test(flavor = "multi_thread")]
async fn setup_creates_a_named_human_admin_without_smtp_or_an_idp() {
    let srv = WyrdTestServer::builder()
        .start_bound()
        .await
        .expect("server starts");
    let superuser = srv
        .pg_fixture()
        .superuser_pool()
        .expect("superuser pool opens");
    let human = [
        "--admin-username",
        "founder",
        "--admin-password-stdin",
        "--admin-email",
        "founder@example.test",
    ];

    // 1. Missing human inputs claim nothing.
    let refused = setup(&srv, &[], None, &[]);
    let (_, stderr) = printed(&refused);
    assert!(!refused.status.success());
    assert!(stderr.contains("--admin-username"), "{stderr}");
    let refused = setup(&srv, &["--admin-username", "founder"], None, &[]);
    let (_, stderr) = printed(&refused);
    assert!(!refused.status.success(), "no terminal and no stdin flag");
    assert!(stderr.contains("--admin-password-stdin"), "{stderr}");
    let refused = setup(&srv, &human, Some("short\n"), &[]);
    assert!(!refused.status.success(), "the password policy refuses");
    assert!(acme(&superuser).await.0.is_empty(), "nothing was claimed");

    // 2. An interrupted setup resumes to one human admin.
    sqlx::raw_sql(
        "CREATE FUNCTION wyrd.refuse_acme_activation() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
             IF NEW.slug = 'acme' AND NEW.status = 'active' THEN
                 RAISE EXCEPTION 'injected activation failure';
             END IF;
             RETURN NEW;
         END $$;
         CREATE TRIGGER refuse_acme BEFORE UPDATE ON platform.tenants
             FOR EACH ROW EXECUTE FUNCTION wyrd.refuse_acme_activation();",
    )
    .execute(&superuser)
    .await
    .expect("the fault installs");
    let interrupted = setup(&srv, &human, Some(&format!("{PASSWORD}\n")), &[]);
    let (stdout, _) = printed(&interrupted);
    assert!(!interrupted.status.success(), "{stdout}");
    let (statuses, users, _) = acme(&superuser).await;
    assert_ne!(
        statuses,
        vec!["active".to_owned()],
        "an interrupted setup is not active"
    );
    assert!(users.len() <= 1, "{users:?}");
    let root = stdout
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
        .expect("the first run initialized the platform root")
        .to_owned();
    sqlx::raw_sql("DROP TRIGGER refuse_acme ON platform.tenants")
        .execute(&superuser)
        .await
        .expect("the fault clears");
    let done = setup(
        &srv,
        &human,
        Some(&format!("{PASSWORD}\n")),
        &[("WYRD_PLATFORM_CREDENTIAL", &root)],
    );
    let (stdout, stderr) = printed(&done);
    assert!(done.status.success(), "{stdout} {stderr}");
    assert!(field(&stdout, "admin_credential").starts_with("wyrd_sk_"));
    assert_eq!(field(&stdout, "admin_username"), "founder");
    assert!(
        field(&stdout, "login_url").ends_with("/t/acme/login"),
        "{stdout}"
    );
    assert_eq!(
        acme(&superuser).await,
        (
            vec!["active".to_owned()],
            vec![(
                Some("founder".to_owned()),
                Some("founder@example.test".to_owned()),
                true
            )],
            0,
        ),
        "one active tenant, one human admin, and an unverified email"
    );

    // 3. Password sign-in, CLI device approval and scoped access.
    let (status, session) = sign_in(&srv, PASSWORD).await;
    assert_eq!(status, StatusCode::OK);
    let session = session.expect("a password session");
    let resp = srv
        .oneshot(browser_post(
            "/auth/device_authorization",
            None,
            "client_id=wyrd-cli&tenant=acme".to_owned(),
            true,
        ))
        .await
        .expect("device authorization responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let device = body_json(resp).await;
    let form = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("tenant", "acme")
        .append_pair(
            "user_code",
            device["user_code"].as_str().expect("user code"),
        )
        .append_pair("decision", "approve")
        .finish();
    let resp = srv
        .oneshot(browser_post("/auth/device", Some(&session), form, true))
        .await
        .expect("approval responds");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "the signed-in session approves"
    );
    let poll = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:device_code")
        .append_pair(
            "device_code",
            device["device_code"].as_str().expect("device code"),
        )
        .append_pair("client_id", "wyrd-cli")
        .finish();
    let resp = srv
        .oneshot(browser_post("/auth/token", None, poll, true))
        .await
        .expect("token responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let token = body_json(resp).await;
    let (status, me) = social_call(
        &srv,
        social_request(
            Method::GET,
            "/v1/principals/me",
            token["access_token"].as_str(),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{me}");
    assert_eq!(me["username"], "founder", "{me}");

    // 4. A completed rerun resets nothing.
    let again = setup(
        &srv,
        &["--admin-username", "founder", "--admin-password-stdin"],
        Some("another quiet harbor\n"),
        &[("WYRD_PLATFORM_CREDENTIAL", &root)],
    );
    let (stdout, _) = printed(&again);
    assert!(again.status.success(), "{stdout}");
    assert!(stdout.contains("already set up"), "{stdout}");
    assert!(!stdout.contains("wyrd_sk_"), "a rerun discloses nothing");
    assert_eq!(acme(&superuser).await.1.len(), 1, "no second admin");
    assert_eq!(
        sign_in(&srv, "another quiet harbor").await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(sign_in(&srv, PASSWORD).await.0, StatusCode::OK);

    // 5. Machine-only creation needs no human admin.
    let platform = super::platform_session(&srv, &root).await;
    let (status, created) = social_call(
        &srv,
        social_request(
            Method::POST,
            "/platform/tenants",
            Some(&platform),
            Some(json!({ "slug": "machines", "display_name": "Machines" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert!(
        created.get("human_admin").is_none_or(Value::is_null),
        "{created}"
    );
}
