//! Tier-2 coverage for the process-global Oracle reader authority.
//!
//! These tests run one real authority against a live Postgres. What they prove
//! is the thing Forge depends on: a query's snapshot is durably protected
//! before the query can read it, protection narrows only after every descendant
//! of that query is gone, and neither liveness nor a lost race can remove
//! protection that is still needed.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use vala_bifrost_redux::oracle::reader_pins::{
    LocalReaderCut, OracleReaderAuthority, OracleReaderAuthorityConfig, RecordingEpochTerminator,
};
use vala_sql::queries::cluster_nodes::ClusterNodes;
use vala_sql::queries::olap_catalog::upsert_table;
use vala_sql::queries::oracle_reader_authority::{BIFROST_CATALOG_NAME, OracleTableProtections};
use vala_sql::row_types::cluster_nodes::RoleRegistration;
use vala_sql::row_types::oracle_reader_authority::{ProtectionRecord, TableAuthorityIdentity};
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    ClusterCapabilities, ClusterNodeKey, ClusterRole, NodeId, OracleCapabilitiesV1, QueryClass,
};

/// How long a durable expectation may take to appear before the test fails.
///
/// Narrowing and release are performed by the authority's own supervised
/// worker, so the durable effect of dropping a guard is observed by polling the
/// database rather than by sleeping for a fixed interval.
const SETTLE_BUDGET: Duration = Duration::from_secs(10);

/// One live database, one Oracle role fence, and helpers to inspect protection.
struct AuthorityFixture {
    /// Repository Postgres fixture owning the isolated database.
    database: PgFixture,
    /// Physical node every epoch in the test is acquired for.
    node_id: Uuid,
    /// Exact `cluster_nodes` Oracle fence the epoch is acquired under.
    fence: u64,
    /// Process shutdown token handed to every started authority.
    shutdown: CancellationToken,
}

impl AuthorityFixture {
    /// Starts Postgres and registers this node's Oracle role.
    ///
    /// # Panics
    ///
    /// Panics when the fixture or the membership registration fails.
    async fn start() -> Self {
        let database = PgFixture::start().await.expect("Postgres fixture");
        let node_id = Uuid::now_v7();
        let nodes = ClusterNodes::new(database.vala_postgres().clone());
        let mut conn = database
            .vala_postgres()
            .tenant_conn(DataTenantId::SYSTEM_OWNER)
            .await
            .expect("system connection");
        let row = nodes
            .register(
                &mut conn,
                &RoleRegistration {
                    key: ClusterNodeKey {
                        node_id: NodeId::new(node_id),
                        role: ClusterRole::Oracle,
                    },
                    address: "http://oracle:5002".into(),
                    capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                        storage_protocol_version: 1,
                        cpu_cores: 4.0,
                        memory_budget_bytes: 4096,
                        cpu_cores_per_slot: 1.0,
                        memory_bytes_per_slot: 1024,
                        raw_slots: 4,
                        usable_slots: 3,
                        supported_classes: vec![QueryClass::Interactive],
                        max_workers_per_query: 3,
                    }),
                    started_at: chrono::Utc::now(),
                },
            )
            .await
            .expect("oracle role registers");
        conn.commit().await.expect("registration commits");
        Self {
            database,
            node_id,
            fence: row.lease.fencing_token,
            shutdown: CancellationToken::new(),
        }
    }

    /// Starts one activated authority with a recording terminator.
    ///
    /// # Panics
    ///
    /// Panics when acquisition or activation fails.
    async fn authority(
        &self,
        concurrency: usize,
    ) -> (Arc<OracleReaderAuthority>, Arc<RecordingEpochTerminator>) {
        let terminator = Arc::new(RecordingEpochTerminator::default());
        let authority = OracleReaderAuthority::start(OracleReaderAuthorityConfig {
            vala: self.database.vala_postgres().clone(),
            operator_pool: self.database.operator_pool().clone(),
            node_id: self.node_id,
            fencing_token: self.fence,
            max_concurrent_queries: concurrency,
            terminator: Arc::clone(&terminator) as Arc<_>,
            shutdown: self.shutdown.clone(),
        })
        .await
        .expect("epoch acquires");
        authority.activate().await.expect("epoch activates");
        (authority, terminator)
    }

    /// Seeds one tenant and returns its durable identity.
    ///
    /// # Panics
    ///
    /// Panics when the tenant cannot be seeded.
    async fn tenant(&self) -> DataTenantId {
        let tenant = DataTenantId::new_v7();
        self.database
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("reader-{}", tenant.as_uuid().simple()),
            )
            .await
            .expect("tenant seeds");
        tenant
    }

    /// Registers one Bifrost table and returns its authority identity.
    ///
    /// # Panics
    ///
    /// Panics when the registration cannot commit.
    async fn table(&self, tenant: DataTenantId, name: &str) -> TableAuthorityIdentity {
        let table_uid = *Uuid::now_v7().as_bytes();
        let mut conn = vala_sql::TenantConn::acquire(self.database.app_pool(), tenant)
            .await
            .expect("tenant connection");
        upsert_table(
            &mut conn,
            &table_uid,
            &format!("vala.bifrost.{name}"),
            &[3_u8; 32],
            &serde_json::json!({
                "partition": { "column": "wyrd_event_time", "granularity": "hour" },
                "sort_keys": [],
                "bloom_columns": []
            }),
        )
        .await
        .expect("table registers");
        conn.commit().await.expect("registration commits");
        TableAuthorityIdentity {
            tenant,
            table_uid,
            catalog_name: BIFROST_CATALOG_NAME.to_owned(),
            namespace_name: "vala.bifrost".to_owned(),
            table_name: name.to_owned(),
        }
    }

    /// Reads the durable protection header this epoch holds for one table.
    ///
    /// # Panics
    ///
    /// Panics when the read fails, which means the stored evidence is corrupt.
    async fn header(&self, identity: &TableAuthorityIdentity) -> Option<ProtectionRecord> {
        let mut conn = vala_sql::TenantConn::acquire(self.database.app_pool(), identity.tenant)
            .await
            .expect("tenant connection");
        OracleTableProtections::new(&mut conn)
            .read(
                identity,
                self.node_id,
                i64::try_from(self.fence).expect("fence fits"),
            )
            .await
            .expect("protection read")
    }

    /// Lists the protection audit operations recorded for one table.
    ///
    /// # Panics
    ///
    /// Panics when the audit rows cannot be read.
    async fn audit_operations(&self, identity: &TableAuthorityIdentity) -> Vec<String> {
        let pool = self
            .database
            .superuser_pool()
            .await
            .expect("superuser pool");
        sqlx::query_scalar(
            "SELECT operation FROM vala.audit_outbox \
              WHERE data_tenant_id = $1 AND resource = $2 ORDER BY seq",
        )
        .bind(identity.tenant.as_uuid())
        .bind(format!(
            "{}/{}/{}/{}",
            identity.tenant, identity.catalog_name, identity.namespace_name, identity.table_name
        ))
        .fetch_all(&pool)
        .await
        .expect("audit rows read")
    }

    /// Reads this epoch's durable row exactly as Postgres holds it.
    ///
    /// # Panics
    ///
    /// Panics when the read fails, which means the stored state is corrupt.
    async fn epoch_row(
        &self,
    ) -> Option<vala_sql::row_types::oracle_reader_authority::OracleEpochRow> {
        let mut conn =
            vala_sql::TenantConn::acquire(self.database.app_pool(), DataTenantId::SYSTEM_OWNER)
                .await
                .expect("system connection");
        vala_sql::queries::oracle_reader_authority::OracleReaderEpochs::new(&mut conn)
            .expect("epochs are system owned")
            .read(self.node_id, i64::try_from(self.fence).expect("fence fits"))
            .await
            .expect("epoch read")
    }

    /// Lists the epoch lifecycle audit operations this node recorded.
    ///
    /// # Panics
    ///
    /// Panics when the audit rows cannot be read.
    async fn epoch_audit_operations(&self) -> Vec<String> {
        let pool = self
            .database
            .superuser_pool()
            .await
            .expect("superuser pool");
        sqlx::query_scalar(
            "SELECT operation FROM vala.audit_outbox \
              WHERE data_tenant_id = $1 AND resource = $2 ORDER BY seq",
        )
        .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
        .bind(format!(
            "oracle/reader_epoch/{}/{}",
            self.node_id, self.fence
        ))
        .fetch_all(&pool)
        .await
        .expect("audit rows read")
    }

    /// Polls until one table's durable header satisfies `predicate`.
    ///
    /// # Panics
    ///
    /// Panics when the expectation is not durable within [`SETTLE_BUDGET`].
    async fn settle<F>(&self, identity: &TableAuthorityIdentity, what: &str, predicate: F)
    where
        F: Fn(Option<&ProtectionRecord>) -> bool,
    {
        let deadline = tokio::time::Instant::now() + SETTLE_BUDGET;
        loop {
            let record = self.header(identity).await;
            if predicate(record.as_ref()) {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "{what} never became durable; last header was {record:?}"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// Builds one local cut with an exact ancestry, newest first.
fn cut(snapshot_id: i64, timestamp_ms: i64, ancestry: &[i64]) -> LocalReaderCut {
    LocalReaderCut {
        snapshot_id,
        timestamp_ms,
        ancestry_path: ancestry.to_vec(),
    }
}

/// Proves one authority aggregates every reader and narrows conservatively.
///
/// The durable frontier is the union of what every live query still needs, not
/// a row per query: a covered admission writes nothing at all, a widening
/// writes exactly one revision, and nothing narrows until the query that needed
/// it is gone.
///
/// # Panics
///
/// Panics when any revision, member set, audit sequence, race outcome, or
/// failed-release retention differs from the contract.
#[tokio::test]
async fn process_global_authority_aggregates_and_releases_conservatively() {
    let fixture = AuthorityFixture::start().await;
    let (authority, _terminator) = fixture.authority(8).await;
    let tenant = fixture.tenant().await;
    let other_tenant = fixture.tenant().await;
    let events = fixture.table(tenant, "events").await;
    let orders = fixture.table(tenant, "orders").await;
    let foreign = fixture.table(other_tenant, "events").await;

    // A first admission is a first coverage: exactly one revision, one event.
    let (first, _first_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("first admission protects");
    let opened = fixture
        .header(&events)
        .await
        .expect("a first header exists");
    assert_eq!(opened.revision, 1);
    assert_eq!(opened.frontier.members.len(), 1);
    assert!(opened.frontier.covers(30));
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec!["oracle.table_protection.expanded".to_owned()]
    );

    // A second reader of the same snapshot is already covered: no revision, no
    // mutation, no evidence. That is the whole point of aggregating.
    let (covered, _covered_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("covered admission protects");
    let unchanged = fixture.header(&events).await.expect("the header survives");
    assert_eq!(unchanged, opened, "a covered admission writes nothing");
    assert_eq!(fixture.audit_operations(&events).await.len(), 1);

    // An older cut on the same chain widens the existing member downward.
    let (older, _older_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(20, 200, &[20, 10]))])
        .await
        .expect("widening admission protects");
    let widened = fixture.header(&events).await.expect("the header widened");
    assert_eq!(widened.revision, 2);
    assert_eq!(widened.frontier.members.len(), 1);
    assert!(widened.frontier.covers(30) && widened.frontier.covers(20));

    // A forked lineage is incomparable, so it becomes its own member rather
    // than collapsing into one falsely-oldest snapshot.
    let (forked, _forked_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(25, 250, &[25, 15]))])
        .await
        .expect("forked admission protects");
    let both = fixture.header(&events).await.expect("the header forked");
    assert_eq!(both.revision, 3);
    assert_eq!(both.frontier.members.len(), 2);
    assert!(both.frontier.covers(25) && both.frontier.covers(30));
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec![
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.expanded".to_owned(),
        ]
    );

    // Protection is table-local and tenant-local throughout.
    assert!(fixture.header(&orders).await.is_none());
    assert!(fixture.header(&foreign).await.is_none());

    // Nothing narrows while the query that needed it is still reachable.
    tokio::task::yield_now().await;
    assert_eq!(fixture.header(&events).await.as_ref(), Some(&both));

    drop(forked);
    fixture
        .settle(&events, "the forked member narrows away", |record| {
            record.is_some_and(|record| record.frontier.members.len() == 1 && record.revision == 4)
        })
        .await;
    drop(older);
    fixture
        .settle(&events, "the widened endpoint narrows back", |record| {
            record.is_some_and(|record| record.revision == 5 && !record.frontier.covers(20))
        })
        .await;
    assert!(
        fixture
            .header(&events)
            .await
            .expect("still protected")
            .frontier
            .covers(30),
        "narrowing must never drop a snapshot two readers still hold"
    );

    // A release that cannot commit retains the prior header. Corrupting the
    // stored digest is the same fail-closed path a poisoned row takes.
    let pool = fixture
        .database
        .superuser_pool()
        .await
        .expect("superuser pool");
    let intact = fixture.header(&events).await.expect("still protected");
    sqlx::query(
        "UPDATE vala.oracle_table_protections SET frontier_digest = $2 WHERE data_tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .bind(vec![0_u8; 32])
    .execute(&pool)
    .await
    .expect("digest corruption applies");
    drop(covered);
    drop(first);
    let audits_before = fixture.audit_operations(&events).await.len();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (revision, members): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT revision FROM vala.oracle_table_protections WHERE data_tenant_id = $1), \
                (SELECT count(*) FROM vala.oracle_table_protection_members \
                  WHERE data_tenant_id = $1)",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("post-failure state reads");
    assert_eq!(
        (revision, members),
        (intact.revision, 1),
        "a failed release keeps the prior protection exactly"
    );
    assert_eq!(fixture.audit_operations(&events).await.len(), audits_before);
    sqlx::query(
        "UPDATE vala.oracle_table_protections SET frontier_digest = $2 WHERE data_tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .bind(intact.frontier_digest.to_vec())
    .execute(&pool)
    .await
    .expect("digest restore applies");

    // A queued narrowing racing a new admission for the same table, in both
    // orders, must leave a frontier that covers the admission that won.
    let (held, _held_permit) = authority
        .acquire_guard_for_cuts(vec![(orders.clone(), cut(70, 700, &[70, 60]))])
        .await
        .expect("race admission protects");
    drop(held);
    let (after_drop, _after_permit) = authority
        .acquire_guard_for_cuts(vec![(orders.clone(), cut(70, 700, &[70, 60]))])
        .await
        .expect("admission after a queued narrowing protects");
    fixture
        .settle(&orders, "the racing admission stays covered", |record| {
            record.is_some_and(|record| record.frontier.covers(70))
        })
        .await;

    let (racing, _racing_permit) = authority
        .acquire_guard_for_cuts(vec![(orders.clone(), cut(60, 600, &[60]))])
        .await
        .expect("second racing admission protects");
    drop(after_drop);
    fixture
        .settle(&orders, "the reverse-order race stays covered", |record| {
            record.is_some_and(|record| record.frontier.covers(60))
        })
        .await;
    drop(racing);

    // Two admissions naming the same tables in opposite orders serialize on the
    // canonical key order, so neither can wait on a lock the other holds.
    let ascending = Arc::clone(&authority);
    let descending = Arc::clone(&authority);
    let (left, right) = (events.clone(), orders.clone());
    let forward = tokio::spawn(async move {
        ascending
            .acquire_guard_for_cuts(vec![
                (left.clone(), cut(90, 900, &[90])),
                (right.clone(), cut(91, 910, &[91])),
            ])
            .await
            .expect("forward multi-table admission protects")
    });
    let (left, right) = (events.clone(), orders.clone());
    let backward = tokio::spawn(async move {
        descending
            .acquire_guard_for_cuts(vec![
                (right.clone(), cut(91, 910, &[91])),
                (left.clone(), cut(90, 900, &[90])),
            ])
            .await
            .expect("reverse multi-table admission protects")
    });
    let both_orders = tokio::time::timeout(SETTLE_BUDGET, async {
        (
            forward.await.expect("forward task"),
            backward.await.expect("backward task"),
        )
    })
    .await
    .expect("opposite orders never deadlock");
    drop(both_orders);

    authority.retire().await.expect("epoch retires");
    assert!(fixture.header(&events).await.is_none());
    assert!(fixture.header(&orders).await.is_none());
    assert_eq!(
        fixture
            .audit_operations(&events)
            .await
            .last()
            .map(String::as_str),
        Some("oracle.table_protection.released")
    );
}

/// Proves lease loss self-fences and retirement releases in the fixed order.
///
/// The ordering is the safety property: admission closes, the loss edge is
/// audited, IO is cancelled, descendants are joined, and only then may any
/// table be released or the epoch row deleted.
///
/// # Panics
///
/// Panics when any revision, deadline relationship, permit refusal, terminator
/// invocation, audit sequence, or release ordering differs from the contract.
#[tokio::test]
async fn epoch_lifecycle_self_fences_and_retires_in_order() {
    let fixture = AuthorityFixture::start().await;
    let (authority, terminator) = fixture.authority(2).await;
    let tenant = fixture.tenant().await;
    let events = fixture.table(tenant, "events").await;

    // Acquisition is revision 1 and activation is revision 2, both audited.
    let row = fixture.epoch_row().await.expect("the epoch row exists");
    assert_eq!(
        row.state,
        vala_sql::row_types::oracle_reader_authority::OracleEpochState::Active
    );
    assert_eq!(row.state_revision, 2);
    assert!(row.activated_at.is_some());

    // Every deadline is derived from the lease the database reported, with the
    // fixed conservative margins between them.
    let deadlines = authority.deadlines().await;
    assert_eq!(
        deadlines.join.duration_since(deadlines.admission_cutoff),
        Duration::from_secs(4)
    );
    assert_eq!(
        deadlines.no_io.duration_since(deadlines.admission_cutoff),
        Duration::from_secs(8)
    );
    assert!(
        deadlines.no_io <= tokio::time::Instant::now() + Duration::from_secs(28),
        "the no-IO deadline keeps the unconditional database-time allowance"
    );

    // Renewal advances the revision and deliberately records nothing.
    authority.renew().await.expect("renewal applies");
    assert_eq!(
        fixture
            .epoch_row()
            .await
            .expect("the epoch row survives renewal")
            .state_revision,
        3
    );
    assert_eq!(
        fixture.epoch_audit_operations().await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
        ]
    );

    // A held query may read; the same permit must refuse once the epoch is
    // fenced, including for a read that had already begun.
    let (guard, permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("admission protects");
    permit
        .begin_io()
        .expect("IO is permitted under a live epoch");
    permit
        .expose_result()
        .expect("results are exposable under a live epoch");

    // Self-fencing with an unjoined descendant is exactly the condition the
    // terminator exists for: this process can no longer prove it stopped
    // reading, so it says so rather than releasing anything.
    authority.self_fence().await;
    assert_eq!(terminator.invocations(), 1);
    assert!(!authority.admits());
    assert!(
        permit.begin_io().is_err(),
        "a fenced epoch starts no new IO"
    );
    assert!(
        permit.expose_result().is_err(),
        "a read begun before loss cannot expose bytes after it"
    );
    assert!(
        authority
            .acquire_guard_for_cuts(vec![(events.clone(), cut(40, 400, &[40]))])
            .await
            .is_err(),
        "a fenced epoch admits nothing further"
    );

    // Protection survives the fence. Only the release sequence removes it.
    let fenced = fixture.header(&events).await.expect("protection survives");
    assert!(fenced.frontier.covers(30));
    let draining = fixture.epoch_row().await.expect("the epoch row survives");
    assert_eq!(
        draining.state,
        vala_sql::row_types::oracle_reader_authority::OracleEpochState::Draining
    );
    assert_eq!(
        fixture
            .epoch_audit_operations()
            .await
            .last()
            .map(String::as_str),
        Some("oracle.reader_epoch.draining")
    );

    // Retirement after a self-fence owes no second loss edge; it joins the now
    // released descendant, releases each table, invalidates, then deletes.
    drop(guard);
    authority
        .retire()
        .await
        .expect("a fenced epoch still retires");
    assert!(fixture.header(&events).await.is_none());
    assert!(
        fixture.epoch_row().await.is_none(),
        "a retired epoch leaves no row"
    );
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec![
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.released".to_owned(),
        ]
    );
    assert_eq!(
        fixture.epoch_audit_operations().await,
        vec![
            "oracle.reader_epoch.acquired".to_owned(),
            "oracle.reader_epoch.activated".to_owned(),
            "oracle.reader_epoch.draining".to_owned(),
            "oracle.reader_epoch.invalidated".to_owned(),
            "oracle.reader_epoch.retired".to_owned(),
        ]
    );
}
