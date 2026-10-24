//! Audit decisions reach retained history through the shared Scribe outbox.
//!
//! Every audited surface stages its decision on the process Scribe outbox; the
//! outbox writes `bifrost.system.audit_log` through the pod's own Scribe. These
//! journeys stage decisions through the production [`AuditStage`] seam of a
//! booted server and read them back through the ordinary query path.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use vala_bifrost_redux::oracle::peer::PeerSecurityAudit;
use wyrd_runtime::Permission;
use wyrd_runtime::audit::AuditStage;
use wyrd_server::oracle::PostgresPeerSecurityAudit;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::{Attribution, PrincipalId, PrincipalKindTag, TokenAudience};
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::{AuditEvent, AuditOutcome, BifrostSecurityViolationKind};
use wyrd_testing::WyrdTestServer;

use super::query::ServerJourneyError;

/// Retained history this journey reads back.
const AUDIT_LOG: &str = "bifrost.system.audit_log";

/// One allowed decision for `operation` by a fresh principal.
fn decision(operation: &str) -> AuditEvent {
    AuditEvent::new(
        RequestId::now_v7(),
        None,
        operation.to_owned(),
        "bifrost".to_owned(),
        None,
        Attribution::direct(PrincipalId::new(uuid::Uuid::now_v7())),
        PrincipalKindTag::Service,
        operation.to_owned(),
        AuditOutcome::Allowed,
    )
}

/// Counts the durable Scribe batches `tenant` committed into the audit log.
///
/// Oracle refuses reads of the reserved system owner, so its retention is
/// observed at the fence Scribe commits with every durable batch instead.
///
/// # Errors
/// Returns the tenant-connection or query failure Postgres raised.
async fn retained_audit_batches(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> Result<i64, ServerJourneyError> {
    let mut conn = server.tenant_conn_for(tenant).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM vala.scribe_batch_commits WHERE logical_table_fqn = $1",
    )
    .bind(AUDIT_LOG)
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    Ok(count)
}

/// Retained history carries both credential shapes, each decision exactly
/// once, with its decision content and no retired chain field.
///
/// A decision made without a credential names none and one made with an API
/// key names exactly that key. Both are staged through the server's own audit
/// seam and read back through the public query path under one operation.
///
/// # Errors
/// Returns the server, settle, or query failure.
///
/// # Panics
/// Panics when a decision is missing, duplicated, or misattributed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn retained_history_carries_both_credential_shapes() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let operation = format!(
        "wyrd.journey.audit_credential.{}",
        uuid::Uuid::now_v7().simple()
    );
    let credential = uuid::Uuid::now_v7();
    let audit: &dyn AuditStage = &**server.state().bifrost.scribe_outbox();
    let without = decision(&operation);
    let mut with = decision(&operation);
    with.credential_id = Some(credential);
    audit.stage(tenant, without.clone());
    audit.stage(tenant, with.clone());

    server.await_audit_retained().await?;
    server
        .await_retained_audit_count(tenant, &format!("operation = '{operation}'"), 2)
        .await?;
    let mut rows = server
        .retained_audit_records(
            tenant,
            "sub_principal_id, credential_id, outcome, permission",
            &format!("operation = '{operation}'"),
        )
        .await?;
    rows.sort();
    let mut expected = vec![
        vec![
            Some(without.sub_principal_id.to_string()),
            None,
            Some("allowed".to_owned()),
            Some(operation.clone()),
        ],
        vec![
            Some(with.sub_principal_id.to_string()),
            Some(credential.to_string()),
            Some("allowed".to_owned()),
            Some(operation.clone()),
        ],
    ];
    expected.sort();
    assert_eq!(rows, expected, "each decision is retained once, as decided");
    assert!(
        server
            .retained_audit_records(tenant, "seq", "true")
            .await
            .is_err(),
        "retained history carries no chain sequence"
    );

    server.shutdown().await?;
    Ok(())
}

/// A system-owner security rejection reaches retained history in exactly one
/// new batch.
///
/// Unverified peer and tail rejections cannot name a tenant, so they stage
/// under `DataTenantId::SYSTEM_OWNER`; the outbox writes them under the
/// platform audit principal, the one principal Scribe admits for the system
/// owner's audit history.
///
/// # Errors
/// Returns the server, Postgres, peer-audit, or settle failure.
///
/// # Panics
/// Panics when the rejection is not retained in exactly one new batch.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn system_owner_security_rejections_retain_once() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let system = DataTenantId::SYSTEM_OWNER;
    server.await_audit_retained().await?;
    let before = retained_audit_batches(&server, system).await?;

    PostgresPeerSecurityAudit::try_new(
        &server.state().postgres,
        Arc::clone(server.state().bifrost.scribe_outbox()),
    )
    .await
    .map_err(|_| "the booted server carries the exact system sentinel")?
    .stage_unverified_ticket_rejection(BifrostSecurityViolationKind::PeerUnknownKey);
    server.await_audit_retained().await?;

    assert_eq!(
        retained_audit_batches(&server, system).await?,
        before + 1,
        "the system-owner rejection must retain in exactly one batch"
    );

    server.shutdown().await?;
    Ok(())
}

/// Retained decisions name the verified subject and actor, allowed or denied,
/// and an unauthenticated request leaves no decision at all (AC-003/004/015).
///
/// User A and Service B may read Cards and Service C may not. A, B for A, C, and C for A each
/// read one absent Card over HTTP, so the tenant retains exactly A/A and A/B
/// allowed and C/C and A/C denied for that Card. A request with no
/// credential, an unparsable token, or A's claims under B's signature is
/// refused before any principal resolves and so is never retained, least of
/// all as A.
///
/// # Errors
/// Returns the server, bootstrap, exchange, request, or settle failure.
///
/// # Panics
/// Panics when a response or a retained pair differs from the story.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn subject_actor_decisions_preserve_trusted_identity() -> Result<(), ServerJourneyError> {
    const READER: &str = "audit_card_reader";
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    server.seed_role(READER, &[Permission::card_read()]).await?;
    let a = server.bootstrap_user("audit-a", &[READER]).await?;
    let b = server.bootstrap_service("audit-b", &[READER]).await?;
    let c = server.bootstrap_service("audit-c", &[]).await?;
    let a_jwt = a.jwt().ok_or("a user carries a token")?.to_owned();
    let (b_jwt, c_jwt) = (
        server.exchange_rooted(&b).await?,
        server.exchange_rooted(&c).await?,
    );
    let card = uuid::Uuid::now_v7();
    let read = async |token: Option<&str>| -> Result<StatusCode, ServerJourneyError> {
        let request = Request::builder()
            .uri(format!("/v1/cards/by-uid/Service/{card}"))
            .body(Body::empty())?;
        Ok(match token {
            Some(token) => server.oneshot_authenticated(token, request).await?,
            None => server.oneshot(request).await?,
        }
        .status())
    };

    let (claims, _) = a_jwt.rsplit_once('.').ok_or("a token has a signature")?;
    let (_, signature) = b_jwt.rsplit_once('.').ok_or("a token has a signature")?;
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("not-a-token".to_owned()), StatusCode::BAD_REQUEST),
        (
            Some(format!("{claims}.{signature}")),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        assert_eq!(read(token.as_deref()).await?, status);
    }
    let b_for_a = server.delegate(&a_jwt, &b_jwt, TokenAudience::Wyrd).await?;
    let c_for_a = server.delegate(&a_jwt, &c_jwt, TokenAudience::Wyrd).await?;
    for (token, status) in [
        (&a_jwt, StatusCode::NOT_FOUND),
        (&b_for_a, StatusCode::NOT_FOUND),
        (&c_jwt, StatusCode::FORBIDDEN),
        (&c_for_a, StatusCode::FORBIDDEN),
    ] {
        assert_eq!(read(Some(token)).await?, status);
    }

    let predicate = format!("resource = 'card:{card}'");
    server.await_audit_retained().await?;
    server
        .await_retained_audit_count(tenant, &predicate, 4)
        .await?;
    let mut rows = server
        .retained_audit_records(
            tenant,
            "sub_principal_id, act_principal_id, outcome",
            &predicate,
        )
        .await?;
    rows.sort();
    let (a, b, c) = (a.id().to_string(), b.id().to_string(), c.id().to_string());
    let pair = |sub: &str, act: &str, outcome: &str| {
        vec![
            Some(sub.to_owned()),
            Some(act.to_owned()),
            Some(outcome.to_owned()),
        ]
    };
    let mut expected = vec![
        pair(&a, &a, "allowed"),
        pair(&a, &b, "allowed"),
        pair(&c, &c, "denied"),
        pair(&a, &c, "denied"),
    ];
    expected.sort();
    assert_eq!(rows, expected, "each decision keeps its verified pair");

    server.shutdown().await?;
    Ok(())
}

/// The value of the first `field` anywhere inside `value`, depth first.
pub(super) fn find_field<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Option<&'a serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map
            .get(field)
            .or_else(|| map.values().find_map(|nested| find_field(nested, field))),
        serde_json::Value::Array(items) => items.iter().find_map(|item| find_field(item, field)),
        _ => None,
    }
}

/// A retained credential issuance decision keeps the issuing request's
/// verified pair, matching the key's creation pair
/// (REQ-002/010, FIND-TASK-001-5).
///
/// Administrator User A issues one credential directly and Service B issues
/// one for A over HTTP. The two retained `auth.credential.issue` decisions
/// carry the pair each key row was created with: A/A, then A/B for B acting
/// for A. No event retains either issued secret.
///
/// # Errors
/// Returns the server, bootstrap, exchange, request, query, or settle failure.
///
/// # Panics
/// Panics when an issuance fails or a retained event's pair or redaction
/// differs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the serialized Postgres-backed journey lane"]
async fn issued_key_audit_keeps_delegation() -> Result<(), ServerJourneyError> {
    let server = WyrdTestServer::start_bound().await?;
    let tenant = server.data_tenant_id();
    let a = server.bootstrap_user("issue-a", &["admin"]).await?;
    let b = server.bootstrap_service("issue-b", &["admin"]).await?;
    let target = server.bootstrap_service("issue-target", &[]).await?;
    let a_jwt = a.jwt().ok_or("a user carries a token")?.to_owned();
    let b_jwt = server.exchange_rooted(&b).await?;
    let b_for_a = server.delegate(&a_jwt, &b_jwt, TokenAudience::Wyrd).await?;

    let mut issued = Vec::new();
    for token in [&a_jwt, &b_for_a] {
        let response = server
            .oneshot_authenticated(
                token,
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/principals/{}/credentials", target.id()))
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 65_536).await?;
        let key: wyrd_spec::auth::IssuedCredential = serde_json::from_slice(&body)?;
        issued.push(key);
    }

    server.await_audit_retained().await?;
    let rows = server
        .retained_audit_records(
            tenant,
            "sub_principal_id, act_principal_id, detail",
            "operation = 'auth.credential.issue'",
        )
        .await?;
    assert_eq!(rows.len(), 2, "one decision per issuance: {rows:?}");
    for key in issued {
        let mut conn = server.tenant_conn_for(tenant).await?;
        let (sub, act): (uuid::Uuid, uuid::Uuid) = sqlx::query_as(
            "SELECT sub_principal_id, act_principal_id FROM wyrd.auth_api_keys WHERE id = $1",
        )
        .bind(key.id)
        .fetch_one(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        let (sub, act) = (sub.to_string(), act.to_string());
        let [row] = rows
            .iter()
            .filter(|row| row[1].as_deref() == Some(act.as_str()))
            .collect::<Vec<_>>()[..]
        else {
            return Err(format!("expected one decision acted by {act}: {rows:?}").into());
        };
        assert_eq!(
            row[0].as_deref(),
            Some(sub.as_str()),
            "the decision keeps the key's creation pair"
        );
        assert!(
            rows.iter()
                .flatten()
                .flatten()
                .all(|text| !text.contains(key.credential.expose())),
            "no retained event carries the issued secret"
        );
    }
    let a = a.id().to_string();
    assert!(
        rows.iter().all(|row| row[0].as_deref() == Some(a.as_str())),
        "every issuance is A's"
    );

    server.shutdown().await?;
    Ok(())
}
