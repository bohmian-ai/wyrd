//! PgFixture coverage for Operator connection persistence: redacted authority,
//! ciphertext-only storage, UUIDv7 identity, forced tenant RLS, updates, and
//! key-version rewrap discovery.
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
    get_connection, insert_connection, list_connections, referenced_key_versions,
    rewrap_connection, stale_key_connections, update_connection,
};

/// Opaque sealed bytes standing in for `wyrd-crypt` output.
fn sealed(marker: u8, key_version: i32) -> SealedSecret {
    SealedSecret {
        ciphertext: vec![marker; 24],
        nonce: [marker; 12],
        wrapped_dek: vec![marker.wrapping_add(1); 48],
        dek_nonce: [marker.wrapping_add(1); 12],
        key_version,
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
    key_version: i32,
) -> StoredConnection {
    insert_connection(
        conn,
        &NewConnection {
            connection_id: OperatorConnectionId::new_v7(),
            name: &name(connection_name),
            config,
            sealed: &sealed(7, key_version),
            secret_version: 1,
            principal: PrincipalId::new(Uuid::now_v7()),
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
        let row = insert(&mut conn, "ops", config, 1).await;
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
            sealed: &sealed(9, 1),
            secret_version: 1,
            principal: PrincipalId::new(Uuid::now_v7()),
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
        "SELECT config::text, secret_ciphertext, key_version
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
        get_connection(&mut foreign, stored[0].view.connection_id, false)
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
    insert(&mut foreign, "ops", &configs()[0], 1).await;
    foreign.commit().await.expect("other tenant commits");

    let forced: (bool, bool) = sqlx::query_as(
        "SELECT relrowsecurity, relforcerowsecurity FROM pg_class
          WHERE oid = 'wyrd.operator_connections'::regclass",
    )
    .fetch_one(fixture.app_pool())
    .await
    .expect("rls metadata reads");
    assert_eq!(forced, (true, true));
    let versions = referenced_key_versions(fixture.operator_pool())
        .await
        .expect("key versions read");
    assert_eq!(versions.len(), 2, "one (tenant, version) pair per tenant");
}

/// Updates keep identity: status-only keeps the secret, a rotation replaces
/// ciphertext and bumps the secret version, and a rewrap is fenced on the
/// secret version it read.
///
/// # Panics
/// Panics when an update loses identity, keeps a stale secret, or a stale
/// rewrap is applied.
#[tokio::test]
async fn updates_rotate_in_place_and_rewrap_is_fenced() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let mut conn = fixture.tenant_conn().await.expect("tenant connection");
    let created = insert(&mut conn, "ops", &configs()[1], 1).await;
    let id = created.view.connection_id;
    let principal = PrincipalId::new(Uuid::now_v7());

    let locked = get_connection(&mut conn, id, true)
        .await
        .expect("locked read")
        .expect("exists");
    let disabled = update_connection(
        &mut conn,
        id,
        &ConnectionChange {
            config: &locked.view.config,
            status: OperatorConnectionStatus::Disabled,
            secret: None,
            principal,
        },
    )
    .await
    .expect("status updates");
    assert_eq!(disabled.view.status, OperatorConnectionStatus::Disabled);
    assert_eq!(disabled.sealed, created.sealed);
    assert_eq!(disabled.view.created_at, created.view.created_at);

    let rotated = update_connection(
        &mut conn,
        id,
        &ConnectionChange {
            config: &locked.view.config,
            status: OperatorConnectionStatus::Active,
            secret: Some((&sealed(3, 2), 2)),
            principal,
        },
    )
    .await
    .expect("secret rotates");
    assert_eq!(rotated.view.connection_id, id);
    assert_eq!(rotated.secret_version, 2);
    assert_eq!(rotated.sealed, sealed(3, 2));

    insert(&mut conn, "legacy", &configs()[0], 1).await;
    let stale = stale_key_connections(&mut conn, 2, 10)
        .await
        .expect("stale rows lock");
    assert_eq!(stale.len(), 1, "only the version-1 row is stale");
    let target = &stale[0];
    assert!(
        !rewrap_connection(
            &mut conn,
            target.view.connection_id,
            target.secret_version + 1,
            &[1, 2, 3],
            &[4; 12],
            2
        )
        .await
        .expect("fenced rewrap answers"),
        "a rewrap read at another secret version is refused"
    );
    assert!(
        rewrap_connection(
            &mut conn,
            target.view.connection_id,
            target.secret_version,
            &[1, 2, 3],
            &[4; 12],
            2
        )
        .await
        .expect("rewrap applies")
    );
    assert!(
        stale_key_connections(&mut conn, 2, 10)
            .await
            .expect("stale rows lock")
            .is_empty()
    );
}
