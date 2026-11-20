//! PgFixture coverage for the cross-tenant stored-secret maintenance slots:
//! the listing reads every tenant's envelope with the owner metadata its
//! context binds, and a replacement is a compare-and-swap on the exact bytes
//! read that touches no other column.

use uuid::Uuid;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::principal::PrincipalId;
use wyrd_spec::ids::{ConnectionName, OperatorConnectionId};
use wyrd_spec::operator_connection::OperatorConnectionConfig;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{StoredSecretTable, stored_secrets, swap_stored_secret};
use wyrd_sql::queries::operator_connections::{NewConnection, SealedSecret, insert_connection};

/// How long each read and swap here may wait; nothing contends with them.
const WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// Insert one Slack connection named `name` sealed as `envelope`.
///
/// # Panics
/// Panics when the insert fails or conflicts.
async fn insert(conn: &mut TenantConn<'_>, name: &str, envelope: Vec<u8>) {
    insert_connection(
        conn,
        &NewConnection {
            connection_id: OperatorConnectionId::new_v7(),
            name: &ConnectionName::new(name).expect("test_setup: valid connection name"),
            config: &OperatorConnectionConfig::Slack {
                workspace_id: "T0001".to_owned(),
            },
            sealed: &SealedSecret { envelope },
            secret_version: 1,
            attribution: wyrd_spec::auth::Attribution::direct(PrincipalId::new(Uuid::now_v7())),
        },
    )
    .await
    .expect("connection inserts")
    .expect("connection name is free");
}

/// The listing returns both tenants' envelopes with their context metadata;
/// a swap on the bytes read replaces only the envelope; a swap on stale
/// bytes or under another tenant matches nothing and leaves the winner.
///
/// # Panics
/// Panics when a tenant's envelope is missing, the metadata differs, or a
/// stale or foreign swap replaces anything.
#[tokio::test]
async fn maintenance_lists_every_tenant_and_swaps_only_the_bytes_it_read() {
    let fixture = PgFixture::start().await.expect("fixture starts");
    let other = fixture
        .seed_additional_tenant("stored-secret-other")
        .await
        .expect("second tenant seeds");
    for (tenant, envelope) in [
        (fixture.data_tenant_id(), vec![1_u8; 24]),
        (other, vec![2; 24]),
    ] {
        let mut conn = fixture
            .tenant_conn_for(tenant)
            .await
            .expect("tenant connection");
        insert(&mut conn, "ops-slack", envelope).await;
        conn.commit().await.expect("tenant commits");
    }
    let operator = fixture.operator_pool();
    let table = StoredSecretTable::OperatorConnections;

    let rows = stored_secrets(operator, table, WAIT)
        .await
        .expect("rows list")
        .expect("read finishes");
    assert_eq!(rows.len(), 2, "every tenant's envelope is listed");
    let mut tenants: Vec<Uuid> = rows.iter().filter_map(|row| row.data_tenant_id).collect();
    tenants.sort_unstable();
    let mut expected = vec![fixture.data_tenant_id().as_uuid(), other.as_uuid()];
    expected.sort_unstable();
    assert_eq!(tenants, expected);
    for row in &rows {
        assert_eq!(row.authority, ["slack", "ops-slack"]);
        assert_eq!(row.version, Some(1));
        assert_eq!(row.owner, row.row_key, "the connection id is the owner");
    }

    let first = &rows[0];
    assert!(
        swap_stored_secret(operator, table, first, &[9; 24], WAIT)
            .await
            .expect("swap runs")
            .expect("swap finishes"),
        "a swap on the bytes read wins"
    );
    assert!(
        !swap_stored_secret(operator, table, first, &[8; 24], WAIT)
            .await
            .expect("swap runs")
            .expect("swap finishes"),
        "a swap on stale bytes loses"
    );
    let mut foreign = rows[1].clone();
    foreign.data_tenant_id = first.data_tenant_id;
    assert!(
        !swap_stored_secret(operator, table, &foreign, &[7; 24], WAIT)
            .await
            .expect("swap runs")
            .expect("swap finishes"),
        "a swap under another tenant matches nothing"
    );

    let after = stored_secrets(operator, table, WAIT)
        .await
        .expect("rows list")
        .expect("read finishes");
    let replaced = after
        .iter()
        .find(|row| row.row_key == first.row_key)
        .expect("swapped row");
    assert_eq!(replaced.envelope, vec![9; 24], "the first swap stands");
    assert_eq!(
        (&replaced.authority, replaced.version),
        (&first.authority, first.version),
        "the swap touched only the envelope"
    );
    let untouched = after
        .iter()
        .find(|row| row.row_key == rows[1].row_key)
        .expect("other row");
    assert_eq!(untouched.envelope, rows[1].envelope);
}
