//! Principal discovery and Role assignment through the authenticated HTTP
//! surface.
//!
//! Drives `GET /v1/principals`, `GET /v1/principals/{id}/roles`, and
//! `PUT`/`DELETE /v1/principals/{id}/roles/{role}` against the in-process
//! server and the repository-managed Postgres: keyset discovery, idempotent
//! direct grants and revokes, `idp`/`direct` coexistence across a login,
//! administration gates and their audited decisions, and the one
//! non-enumerating refusal for every principal that is not assignable here.

use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;
use wyrd_sql::queries::auth::{
    delete_service_account, insert_service_account, replace_idp_user_roles,
};
use wyrd_testing::{Bootstrap, WyrdTestServer};

/// Send one bodyless request as `jwt` and split the answer into status and
/// JSON body.
///
/// # Panics
/// Panics when the request cannot be built, the route fails to respond, or
/// the body is not JSON.
async fn call(
    server: &WyrdTestServer,
    jwt: &str,
    method: Method,
    uri: &str,
) -> (StatusCode, Value) {
    let response = server
        .oneshot_authenticated(
            jwt,
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("request responds");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20)
        .await
        .expect("response body reads");
    (
        status,
        serde_json::from_slice(&bytes).expect("response body is JSON"),
    )
}

/// Bootstrap a user holding `roles` from its identity provider and return
/// its id and access token.
///
/// # Panics
/// Panics when bootstrapping fails or returns a machine principal.
async fn user(server: &WyrdTestServer, name: &str, roles: &[&str]) -> (Uuid, String) {
    match server
        .bootstrap_user(name, roles)
        .await
        .expect("user bootstraps")
    {
        Bootstrap::User { id, jwt } => (id.as_uuid(), jwt),
        Bootstrap::Machine { .. } => panic!("user bootstrap returned a machine principal"),
    }
}

/// Bootstrap a Service principal holding `roles` and return its id and an
/// access token exchanged from its key.
///
/// # Panics
/// Panics when bootstrapping or the key exchange fails.
async fn service(server: &WyrdTestServer, name: &str, roles: &[&str]) -> (Uuid, String) {
    let machine = server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps");
    let jwt = server
        .exchange_api_key(machine.api_key().expect("machine has an API key"))
        .await
        .expect("API key exchanges");
    (machine.id().as_uuid(), jwt)
}

/// The `(role, source)` pairs of an assignment response, in response order.
fn assignments(body: &Value) -> Vec<(String, String)> {
    body["roles"]
        .as_array()
        .expect("roles is an array")
        .iter()
        .map(|assignment| {
            (
                assignment["role"].as_str().expect("role").to_owned(),
                assignment["source"].as_str().expect("source").to_owned(),
            )
        })
        .collect()
}

/// Owned `(role, source)` pairs for comparison.
fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(role, source)| ((*role).to_owned(), (*source).to_owned()))
        .collect()
}

/// `principal`'s retained `operation` decisions as (permission, outcome),
/// sorted by outcome, once every decision staged so far is retained.
///
/// # Panics
/// Panics when audit retention does not settle or the audit rows cannot be read.
async fn decisions(
    server: &WyrdTestServer,
    operation: &str,
    principal: Uuid,
) -> Vec<(String, String)> {
    server
        .await_audit_retained()
        .await
        .expect("audit retention settles");
    let mut rows: Vec<(String, String)> = server
        .retained_audit_records(
            server.data_tenant_id(),
            "permission, outcome",
            &format!("operation = '{operation}' AND audit_principal_id = '{principal}'"),
        )
        .await
        .expect("audit rows read")
        .into_iter()
        .map(|row| {
            (
                row[0].clone().unwrap_or_default(),
                row[1].clone().unwrap_or_default(),
            )
        })
        .collect();
    rows.sort_by(|left, right| left.1.cmp(&right.1));
    rows
}

/// A Service's direct grant is idempotent and reported with the complete
/// resulting set; revoke is idempotent too; both allowed decisions are audited
/// under tenant administration.
///
/// # Panics
/// Panics when the server fails to start, a route answers unexpectedly, or
/// the audit expectations fail.
#[tokio::test(flavor = "current_thread")]
async fn direct_service_grants_and_revokes_are_idempotent() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (admin, admin_jwt) = user(&server, "pr-admin", &["admin"]).await;
    let (target, _) = service(&server, "pr-target", &["workload"]).await;
    let uri = format!("/v1/principals/{target}/roles/editor");

    let (status, first) = call(&server, &admin_jwt, Method::PUT, &uri).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["changed"], json!(true));
    assert_eq!(first["kind"], json!("service"));
    assert_eq!(first["role"], json!("editor"));
    assert_eq!(first["principal_id"], json!(target.to_string()));
    assert_eq!(
        assignments(&first),
        pairs(&[("editor", "direct"), ("workload", "direct")])
    );
    let (status, repeat) = call(&server, &admin_jwt, Method::PUT, &uri).await;
    assert_eq!(status, StatusCode::OK, "{repeat}");
    assert_eq!(repeat["changed"], json!(false));
    assert_eq!(assignments(&repeat), assignments(&first));

    let (status, revoked) = call(&server, &admin_jwt, Method::DELETE, &uri).await;
    assert_eq!(status, StatusCode::OK, "{revoked}");
    assert_eq!(revoked["changed"], json!(true));
    assert_eq!(assignments(&revoked), pairs(&[("workload", "direct")]));
    let (status, again) = call(&server, &admin_jwt, Method::DELETE, &uri).await;
    assert_eq!(status, StatusCode::OK, "{again}");
    assert_eq!(again["changed"], json!(false));

    let (status, listed) = call(
        &server,
        &admin_jwt,
        Method::GET,
        &format!("/v1/principals/{target}/roles"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(assignments(&listed), pairs(&[("workload", "direct")]));

    let allowed = |count| vec![("wildcard:wildcard".to_owned(), "allowed".to_owned()); count];
    assert_eq!(
        decisions(&server, "auth.principal.role.grant", admin).await,
        allowed(2)
    );
    assert_eq!(
        decisions(&server, "auth.principal.role.revoke", admin).await,
        allowed(2)
    );
    assert_eq!(
        decisions(&server, "auth.principal.role.list", admin).await,
        vec![("service_accounts:write".to_owned(), "allowed".to_owned())]
    );
}

/// A user's direct and IdP assignments of one Role coexist: login replaces
/// only the IdP set, revoke removes only the direct one, and the effective
/// Roles a new token carries are their union.
///
/// # Panics
/// Panics when the server fails to start, a route answers unexpectedly, or a
/// fixture write fails.
#[tokio::test(flavor = "current_thread")]
async fn direct_user_assignments_survive_login_and_revoke_only_direct() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (_, admin_jwt) = user(&server, "pr-admin", &["admin"]).await;
    let (target, _) = user(&server, "pr-person", &["viewer"]).await;

    let (status, granted) = call(
        &server,
        &admin_jwt,
        Method::PUT,
        &format!("/v1/principals/{target}/roles/viewer"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{granted}");
    assert_eq!(granted["kind"], json!("user"));
    assert_eq!(
        assignments(&granted),
        pairs(&[("viewer", "direct"), ("viewer", "idp")])
    );
    let (status, granted) = call(
        &server,
        &admin_jwt,
        Method::PUT,
        &format!("/v1/principals/{target}/roles/editor"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{granted}");

    // The identity provider now asserts no Role: the next login replaces only
    // the IdP set.
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    assert!(
        replace_idp_user_roles(&mut conn, target, &[])
            .await
            .expect("login replaces the IdP set")
    );
    conn.commit().await.expect("login commits");

    let (status, listed) = call(
        &server,
        &admin_jwt,
        Method::GET,
        &format!("/v1/principals/{target}/roles"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(
        assignments(&listed),
        pairs(&[("editor", "direct"), ("viewer", "direct")])
    );

    let (status, revoked) = call(
        &server,
        &admin_jwt,
        Method::DELETE,
        &format!("/v1/principals/{target}/roles/viewer"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{revoked}");
    assert_eq!(assignments(&revoked), pairs(&[("editor", "direct")]));
}

/// Role writes need tenant administration and discovery needs principal
/// administration; every refusal is audited and writes nothing.
///
/// # Panics
/// Panics when the server fails to start, a caller is not refused, or the
/// audit expectations fail.
#[tokio::test(flavor = "current_thread")]
async fn role_writes_require_tenant_administration() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (target, _) = service(&server, "pr-target", &["workload"]).await;
    let (editor, editor_jwt) = user(&server, "pr-editor", &["editor"]).await;
    server
        .seed_role(
            "credential_admin",
            &["service_accounts:write".parse().expect("permission parses")],
        )
        .await
        .expect("role seeds");
    let (credential_admin, credential_jwt) =
        user(&server, "pr-credentials", &["credential_admin"]).await;
    let uri = format!("/v1/principals/{target}/roles/editor");

    for jwt in [&editor_jwt, &credential_jwt] {
        let (status, body) = call(&server, jwt, Method::PUT, &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
        assert_eq!(body["code"], json!("WYRD_PERMISSION_403_DENIED_RBAC"));
        let (status, body) = call(&server, jwt, Method::DELETE, &uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    }
    let (status, body) = call(&server, &editor_jwt, Method::GET, "/v1/principals").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, listed) = call(
        &server,
        &credential_jwt,
        Method::GET,
        &format!("/v1/principals/{target}/roles"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(assignments(&listed), pairs(&[("workload", "direct")]));

    let denied = vec![("wildcard:wildcard".to_owned(), "denied".to_owned())];
    for principal in [editor, credential_admin] {
        assert_eq!(
            decisions(&server, "auth.principal.role.grant", principal).await,
            denied
        );
        assert_eq!(
            decisions(&server, "auth.principal.role.revoke", principal).await,
            denied
        );
    }
    assert_eq!(
        decisions(&server, "auth.principal.list", editor).await,
        vec![("service_accounts:write".to_owned(), "denied".to_owned())]
    );
}

/// Unknown, deleted, foreign-tenant, and tenant-administrator principals all
/// answer with one non-enumerating not-found error; malformed ids, unknown
/// Roles, and out-of-range pages are validation errors.
///
/// # Panics
/// Panics when the server fails to start, a fixture write fails, or a refusal
/// differs.
#[tokio::test(flavor = "current_thread")]
async fn unassignable_principals_are_not_enumerable() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (admin, admin_jwt) = user(&server, "pr-admin", &["admin"]).await;
    let (deleted, _) = service(&server, "pr-deleted", &[]).await;
    let tenant_admin = Uuid::now_v7();
    let mut conn = server
        .tenant_conn_for(server.data_tenant_id())
        .await
        .expect("tenant connection opens");
    assert!(
        delete_service_account(&mut conn, deleted)
            .await
            .expect("principal deletes")
    );
    insert_service_account(
        &mut conn,
        tenant_admin,
        "tenant_admin",
        None,
        "pr-tenant-admin",
        None,
        admin,
    )
    .await
    .expect("tenant administrator inserts");
    conn.commit().await.expect("fixture commits");
    let other = server.seed_tenant("pr-other").await.expect("tenant seeds");
    let foreign = server
        .bootstrap_service_in_tenant(other, "pr-foreign", &["workload"])
        .await
        .expect("foreign service bootstraps")
        .id()
        .as_uuid();

    for id in [Uuid::now_v7(), deleted, tenant_admin, foreign] {
        for (method, uri) in [
            (Method::GET, format!("/v1/principals/{id}/roles")),
            (Method::PUT, format!("/v1/principals/{id}/roles/viewer")),
            (Method::DELETE, format!("/v1/principals/{id}/roles/viewer")),
        ] {
            let (status, body) = call(&server, &admin_jwt, method, &uri).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body}");
            assert_eq!(body["code"], json!("WYRD_AUTH_404_PRINCIPAL_NOT_FOUND"));
        }
    }

    let (status, page) = call(&server, &admin_jwt, Method::GET, "/v1/principals?limit=200").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    let listed = page["principals"]
        .as_array()
        .expect("principals is an array")
        .iter()
        .map(|principal| principal["principal_id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    for hidden in [deleted, tenant_admin, foreign] {
        assert!(!listed.contains(&hidden.to_string()), "{hidden} is listed");
    }

    for uri in [
        "/v1/principals/not-a-uuid/roles".to_owned(),
        format!("/v1/principals/{admin}/roles/no_such_role"),
        "/v1/principals?limit=0".to_owned(),
        "/v1/principals?limit=201".to_owned(),
        "/v1/principals?kind=tenant_admin".to_owned(),
        "/v1/principals?colour=blue".to_owned(),
    ] {
        let method = if uri.contains("no_such_role") {
            Method::PUT
        } else {
            Method::GET
        };
        let (status, body) = call(&server, &admin_jwt, method, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body}");
        assert_eq!(body["code"], json!("WYRD_SPEC_400_VALIDATION"), "{uri}");
    }
}

/// Discovery filters by exact kind, email, and name and pages by id without
/// gaps or repeats.
///
/// # Panics
/// Panics when the server fails to start or a page differs.
#[tokio::test(flavor = "current_thread")]
async fn discovery_filters_and_pages_by_id() {
    let server = WyrdTestServer::start_in_process()
        .await
        .expect("test server starts");
    let (admin, admin_jwt) = user(&server, "pr-admin", &["admin"]).await;
    let (person, _) = user(&server, "pr-person", &[]).await;
    let (first, _) = service(&server, "pr-first", &[]).await;
    let (second, _) = service(&server, "pr-second", &[]).await;

    let (status, by_email) = call(
        &server,
        &admin_jwt,
        Method::GET,
        "/v1/principals?email=pr-person@test.wyrd",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{by_email}");
    assert_eq!(
        by_email,
        json!({
            "principals": [{
                "principal_id": person.to_string(),
                "kind": "user",
                "status": "active",
                "email": "pr-person@test.wyrd",
                "name": null,
                "card_ref": null,
            }],
            "next": null,
        })
    );
    let (_, by_name) = call(
        &server,
        &admin_jwt,
        Method::GET,
        "/v1/principals?name=pr-second",
    )
    .await;
    assert_eq!(
        by_name["principals"][0]["principal_id"],
        json!(second.to_string())
    );
    assert_eq!(by_name["principals"][0]["kind"], json!("service"));

    let mut seen = Vec::new();
    let mut uri = "/v1/principals?kind=service&limit=1".to_owned();
    loop {
        let (status, page) = call(&server, &admin_jwt, Method::GET, &uri).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        let principals = page["principals"]
            .as_array()
            .expect("principals is an array");
        assert!(principals.len() <= 1);
        seen.extend(
            principals
                .iter()
                .map(|principal| principal["principal_id"].as_str().expect("id").to_owned()),
        );
        match page["next"].as_str() {
            Some(next) => uri = format!("/v1/principals?kind=service&limit=1&after={next}"),
            None => break,
        }
    }
    let mut expected = vec![first.to_string(), second.to_string()];
    expected.sort();
    assert!(
        seen.windows(2).all(|pair| pair[0] < pair[1]),
        "pages are ordered by id without repeats: {seen:?}"
    );
    for id in &expected {
        assert!(seen.contains(id), "{id} is reached by paging");
    }
    assert!(!seen.contains(&admin.to_string()), "kind filters users out");
}
