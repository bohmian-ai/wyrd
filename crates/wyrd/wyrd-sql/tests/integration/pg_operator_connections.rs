//! PgFixture coverage for Operator connection persistence: redacted authority,
//! envelope-only storage, UUIDv7 identity, forced tenant RLS, updates fenced
//! on the observed row.
//!
//! Every tenant read and write runs through [`TenantConn`], so forced RLS
//! applies exactly as it does for the server.

use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::{
    HttpAuthScheme, HttpsOrigin, OperatorConnectionConfig, OperatorConnectionStatus,
    OperatorProvider,
};
use wyrd_sql::TenantConn;
use wyrd_sql::queries::operator_connections::{
    ConnectionChange, NewConnection, SealedSecret, StoredConnection, find_connection,
    get_connection, insert_connection, list_connections, update_connection,
};

/// Opaque envelope bytes standing in for `[kms]` output.
fn sealed(marker: u8) -> SealedSecret {
    SealedSecret {
        envelope: vec![marker; 24],
    }
}

/// Parse a connection name fixture.
///
/// # Panics
/// Panics when `name` is not a valid connection name.
fn name(name: &str) -> ConnectionName {
    ConnectionName::new(name).expect("test_setup: valid connection name")
}

/// Insert one connection with `config`, expecting success.
///
/// # Panics
/// Panics when the insert fails or conflicts.
async fn insert(
    conn: &mut TenantConn<'_>,
    connection_name: &str,
    config: &OperatorConnectionConfig,
) -> StoredConnection {
    insert_connection(
        conn,
        &NewConnection {
            connection_id: OperatorConnectionId::new_v7(),
            name: &name(connection_name),
            config,
            sealed: &sealed(7),
            secret_version: 1,
            attribution: wyrd_spec::auth::Attribution::direct(PrincipalId::new(Uuid::now_v7())),
        },
    )
    .await
    .expect("connection inserts")
    .expect("connection name is free")
}

/// The three provider authorities.
///
/// # Panics
/// Panics when the origin fixture is invalid.
fn configs() -> [OperatorConnectionConfig; 3] {
    [
        OperatorConnectionConfig::Slack {
            workspace_id: "T0001".to_owned(),
        },
        OperatorConnectionConfig::PagerDuty {},
        OperatorConnectionConfig::Http {
            origin: HttpsOrigin::parse("https://hooks.example.com").expect("origin"),
            auth: HttpAuthScheme::Header {
                name: "X-Api-Key".to_owned(),
            },
        },
    ]
}

/// Every provider persists a UUIDv7 row with redacted authority and only
/// ciphertext, names are unique per provider, and the table forces RLS so
/// another tenant can neither see nor collide with the rows.
///
/// # Panics
/// Panics when a stored shape, uniqueness rule, or isolation guarantee breaks.
#[tokio::test]
async fn connections_store_redacted_authority_and_isolate_tenants() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("operator-connections-other")
        .await
        .expect("second tenant seeds");
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let mut stored = Vec::new();
    for config in &configs() {
        let row = insert(&mut conn, "ops", config).await;
        assert_eq!(row.view.connection_id.as_uuid().get_version_num(), 7);
        assert_eq!(row.view.config, *config);
        assert_eq!(row.view.status, OperatorConnectionStatus::Active);
        assert_eq!(row.secret_version, 1);
        stored.push(row);
    }
    let duplicate = insert_connection(
        &mut conn,
        &NewConnection {
            connection_id: OperatorConnectionId::new_v7(),
            name: &name("ops"),
            config: &configs()[0],
            sealed: &sealed(9),
            secret_version: 1,
            attribution: wyrd_spec::auth::Attribution::direct(PrincipalId::new(Uuid::now_v7())),
        },
    )
    .await
    .expect("conflicting insert answers");
    assert!(duplicate.is_none(), "(tenant, provider, name) is unique");

    let listed = list_connections(&mut conn).await.expect("list reads");
    assert_eq!(listed.len(), 3);
    let found = find_connection(&mut conn, OperatorProvider::Http, &name("ops"))
        .await
        .expect("find reads")
        .expect("http connection exists");
    assert_eq!(found, stored[2]);
    let raw: (String, Vec<u8>, i32) = sqlx::query_as(
        "SELECT config::text, secret_ciphertext, secret_version
           FROM wyrd.operator_connections WHERE connection_id = $1",
    )
    .bind(stored[2].view.connection_id.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await
    .expect("raw row reads");
    assert!(!raw.0.contains("value") && !raw.0.contains("token"));
    assert_eq!(raw.1, vec![7_u8; 24]);
    assert_eq!(raw.2, 1);
    conn.commit().await.expect("tenant commits");

    let mut foreign = fixture
        .tenant_conn_for(other)
        .await
        .expect("other tenant connection");
    assert!(
        list_connections(&mut foreign)
            .await
            .expect("list")
            .is_empty()
    );
    assert!(
        get_connection(&mut foreign, stored[0].view.connection_id)
            .await
            .expect("get answers")
            .is_none()
    );
    assert!(
        find_connection(&mut foreign, OperatorProvider::Slack, &name("ops"))
            .await
            .expect("find answers")
            .is_none()
    );
    insert(&mut foreign, "ops", &configs()[0]).await;
    foreign.commit().await.expect("other tenant commits");

    let forced: (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class
          WHERE oid = 'wyrd.operator_connections'::regclass",
    )
    .fetch_one(fixture.app_pool())
    .await
    .expect("rls metadata reads");
    assert_eq!(forced, (true, true));
}

/// Updates keep identity: status-only keeps the secret, a rotation replaces
/// the envelope and bumps the secret version, and an update read before
/// another write committed changes nothing.
///
/// # Panics
/// Panics when an update loses identity, keeps a stale secret, a stale update
/// is applied, or a fresh one is refused.
#[tokio::test]
async fn updates_rotate_in_place_and_are_fenced_on_the_observed_row() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let created = insert(&mut conn, "ops", &configs()[1]).await;
    let id = created.view.connection_id;
    let principal = PrincipalId::new(Uuid::now_v7());

    let disabled = update_connection(
        &mut conn,
        &created,
        &ConnectionChange {
            config: &created.view.config,
            status: OperatorConnectionStatus::Disabled,
            secret: None,
            principal,
        },
    )
    .await
    .expect("status updates")
    .expect("the observed row is current");
    assert_eq!(disabled.view.status, OperatorConnectionStatus::Disabled);
    assert_eq!(disabled.sealed, created.sealed);
    assert_eq!(disabled.view.created_at, created.view.created_at);

    let rotate = ConnectionChange {
        config: &created.view.config,
        status: OperatorConnectionStatus::Active,
        secret: Some((&sealed(3), 2)),
        principal,
    };
    assert_eq!(
        update_connection(&mut conn, &created, &rotate)
            .await
            .expect("stale update answers"),
        None,
        "an update observed before the disable commits nothing"
    );
    let rotated = update_connection(&mut conn, &disabled, &rotate)
        .await
        .expect("secret rotates")
        .expect("the observed row is current");
    assert_eq!(rotated.view.connection_id, id);
    assert_eq!(rotated.secret_version, 2);
    assert_eq!(rotated.sealed, sealed(3));
}
