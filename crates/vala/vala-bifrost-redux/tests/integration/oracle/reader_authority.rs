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

use crate::forge::support as forge_support;
use uuid::Uuid;
use vala_bifrost_redux::oracle::reader_pins::{
    LocalReaderCut, OracleEpochRecovery, OracleReaderAuthority, OracleReaderAuthorityConfig,
    ReaderQueryGuard, RecordingEpochTerminator,
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

/// Drives the aggregation phase: coverage, widening, forking, and narrowing.
///
/// Returns the two guards whose release the caller still needs, so the failure
/// phase can prove a refused release retains exactly this state.
///
/// # Panics
///
/// Panics when any revision, member set, audit sequence, or narrowing outcome
/// differs from the contract.
async fn aggregate_and_narrow(
    fixture: &AuthorityFixture,
    authority: &Arc<OracleReaderAuthority>,
    events: &TableAuthorityIdentity,
    orders: &TableAuthorityIdentity,
    foreign: &TableAuthorityIdentity,
) -> (ReaderQueryGuard, ReaderQueryGuard) {
    // A first admission is a first coverage: exactly one revision, one event.
    let (first, _first_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("first admission protects");
    let opened = fixture.header(events).await.expect("a first header exists");
    assert_eq!(opened.revision, 1);
    assert_eq!(opened.frontier.members.len(), 1);
    assert!(opened.frontier.covers(30));
    assert_eq!(
        fixture.audit_operations(events).await,
        vec!["oracle.table_protection.expanded".to_owned()]
    );

    // A second reader of the same snapshot is already covered: no revision, no
    // mutation, no evidence. That is the whole point of aggregating.
    let (covered, _covered_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("covered admission protects");
    let unchanged = fixture.header(events).await.expect("the header survives");
    assert_eq!(unchanged, opened, "a covered admission writes nothing");
    assert_eq!(fixture.audit_operations(events).await.len(), 1);

    // An older cut on the same chain widens the existing member downward.
    let (older, _older_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(20, 200, &[20, 10]))])
        .await
        .expect("widening admission protects");
    let widened = fixture.header(events).await.expect("the header widened");
    assert_eq!(widened.revision, 2);
    assert_eq!(widened.frontier.members.len(), 1);
    assert!(widened.frontier.covers(30) && widened.frontier.covers(20));

    // A forked lineage is incomparable, so it becomes its own member rather
    // than collapsing into one falsely-oldest snapshot.
    let (forked, _forked_permit) = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(25, 250, &[25, 15]))])
        .await
        .expect("forked admission protects");
    let both = fixture.header(events).await.expect("the header forked");
    assert_eq!(both.revision, 3);
    assert_eq!(both.frontier.members.len(), 2);
    assert!(both.frontier.covers(25) && both.frontier.covers(30));
    assert_eq!(
        fixture.audit_operations(events).await,
        vec![
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.expanded".to_owned(),
        ]
    );

    // Protection is table-local and tenant-local throughout.
    assert!(fixture.header(orders).await.is_none());
    assert!(fixture.header(foreign).await.is_none());

    // Nothing narrows while the query that needed it is still reachable.
    tokio::task::yield_now().await;
    assert_eq!(fixture.header(events).await.as_ref(), Some(&both));

    drop(forked);
    fixture
        .settle(events, "the forked member narrows away", |record| {
            record.is_some_and(|record| record.frontier.members.len() == 1 && record.revision == 4)
        })
        .await;
    drop(older);
    fixture
        .settle(events, "the widened endpoint narrows back", |record| {
            record.is_some_and(|record| record.revision == 5 && !record.frontier.covers(20))
        })
        .await;
    assert!(
        fixture
            .header(events)
            .await
            .expect("still protected")
            .frontier
            .covers(30),
        "narrowing must never drop a snapshot two readers still hold"
    );

    (first, covered)
}

/// Drives the fail-closed phase: a release that cannot commit changes nothing.
///
/// # Panics
///
/// Panics when the refused release mutates the header, its members, or the
/// audit log.
async fn refuse_uncommittable_release(
    fixture: &AuthorityFixture,
    tenant: DataTenantId,
    events: &TableAuthorityIdentity,
    first: ReaderQueryGuard,
    covered: ReaderQueryGuard,
) {
    // A release that cannot commit retains the prior header. Corrupting the
    // stored digest is the same fail-closed path a poisoned row takes.
    let pool = fixture
        .database
        .superuser_pool()
        .await
        .expect("superuser pool");
    let intact = fixture.header(events).await.expect("still protected");
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
    let audits_before = fixture.audit_operations(events).await.len();
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
    assert_eq!(fixture.audit_operations(events).await.len(), audits_before);
    sqlx::query(
        "UPDATE vala.oracle_table_protections SET frontier_digest = $2 WHERE data_tenant_id = $1",
    )
    .bind(tenant.as_uuid())
    .bind(intact.frontier_digest.to_vec())
    .execute(&pool)
    .await
    .expect("digest restore applies");
}

/// Drives the race phase: a queued narrowing against a fresh admission.
///
/// # Panics
///
/// Panics when either ordering leaves a frontier that fails to cover the
/// admission that won.
async fn race_narrowing_against_admission(
    fixture: &AuthorityFixture,
    authority: &Arc<OracleReaderAuthority>,
    orders: &TableAuthorityIdentity,
) {
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
        .settle(orders, "the racing admission stays covered", |record| {
            record.is_some_and(|record| record.frontier.covers(70))
        })
        .await;

    let (racing, _racing_permit) = authority
        .acquire_guard_for_cuts(vec![(orders.clone(), cut(60, 600, &[60]))])
        .await
        .expect("second racing admission protects");
    drop(after_drop);
    fixture
        .settle(orders, "the reverse-order race stays covered", |record| {
            record.is_some_and(|record| record.frontier.covers(60))
        })
        .await;
    drop(racing);
}

/// Drives the lock-order phase: opposite table orders must never deadlock.
///
/// # Panics
///
/// Panics when the two admissions do not both complete inside the budget.
async fn admit_opposite_table_orders(
    authority: &Arc<OracleReaderAuthority>,
    events: &TableAuthorityIdentity,
    orders: &TableAuthorityIdentity,
) {
    // Two admissions naming the same tables in opposite orders serialize on the
    // canonical key order, so neither can wait on a lock the other holds.
    let ascending = Arc::clone(authority);
    let descending = Arc::clone(authority);
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

    let (first, covered) =
        aggregate_and_narrow(&fixture, &authority, &events, &orders, &foreign).await;
    refuse_uncommittable_release(&fixture, tenant, &events, first, covered).await;
    race_narrowing_against_admission(&fixture, &authority, &orders).await;
    admit_opposite_table_orders(&authority, &events, &orders).await;

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

/// Proves a live epoch's durable revisions, deadlines, and silent renewal.
///
/// # Panics
///
/// Panics when a revision, deadline relationship, or audit sequence differs
/// from the contract.
async fn assert_live_epoch_accounting(
    fixture: &AuthorityFixture,
    authority: &Arc<OracleReaderAuthority>,
) {
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

    assert_live_epoch_accounting(&fixture, &authority).await;

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

/// Registers one more Oracle node and starts its activated epoch.
///
/// Each node in this journey needs its own fenced role row, because protection
/// is keyed by node and fence: a follower must be able to protect the same
/// table the leader protects, under its own epoch, without touching the
/// leader's row.
///
/// # Panics
///
/// Panics when registration, acquisition, or activation fails.
async fn oracle_epoch(
    fixture: &forge_support::PromotionIntegrationFixture,
    address: &str,
) -> (Arc<OracleReaderAuthority>, Uuid, u64) {
    let node_id = Uuid::now_v7();
    let mut conn = fixture
        .vala
        .tenant_conn(DataTenantId::SYSTEM_OWNER)
        .await
        .expect("system connection");
    let row = ClusterNodes::new(fixture.vala.clone())
        .register(
            &mut conn,
            &RoleRegistration {
                key: ClusterNodeKey {
                    node_id: NodeId::new(node_id),
                    role: ClusterRole::Oracle,
                },
                address: address.into(),
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
    let fence = row.lease.fencing_token;
    let authority = OracleReaderAuthority::start(OracleReaderAuthorityConfig {
        vala: fixture.vala.clone(),
        operator_pool: fixture.operator_pool.clone(),
        node_id,
        fencing_token: fence,
        max_concurrent_queries: 4,
        terminator: Arc::new(RecordingEpochTerminator::default()) as Arc<_>,
        shutdown: CancellationToken::new(),
    })
    .await
    .expect("epoch acquires");
    authority.activate().await.expect("epoch activates");
    (authority, node_id, fence)
}

/// Reads one node's durable protection header for one table.
///
/// # Panics
///
/// Panics when the read fails, which means the stored evidence is corrupt.
async fn node_protection(
    fixture: &forge_support::PromotionIntegrationFixture,
    identity: &TableAuthorityIdentity,
    node_id: Uuid,
    fence: u64,
) -> Option<ProtectionRecord> {
    let mut conn = fixture
        .vala
        .tenant_conn(identity.tenant)
        .await
        .expect("tenant connection");
    let record = OracleTableProtections::new(&mut conn)
        .read(identity, node_id, i64::try_from(fence).expect("fence fits"))
        .await
        .expect("protection read");
    conn.commit().await.expect("protection read commits");
    record
}

/// Commits one real Iceberg snapshot over the fixture's sealed hot objects.
///
/// The journey needs a snapshot that exists, is dated, and has an ancestry the
/// authority can walk, so it is produced by the production promotion route
/// rather than written by hand.
///
/// # Panics
///
/// Panics when the supervised promotion does not commit.
async fn commit_one_snapshot(fixture: &forge_support::PromotionIntegrationFixture) {
    let object_store = forge_support::CountingObjectStore::new(Arc::clone(&fixture.staging));
    let mut forge = forge_support::SupervisedPromotion::start(
        fixture,
        fixture.catalog.iceberg_catalog(),
        object_store as Arc<dyn vala_bifrost_redux::forge::ForgeObjectStore>,
        vala_bifrost_redux::forge::ForgeClock::system(),
    );
    forge.run_one_success().await;
    forge.shutdown().await;
}

/// Proves a leader and its followers protect before IO and release after join.
///
/// The ordering under test spans two epochs over one table: identity
/// resolution takes no protection, the leader's protection is durable before
/// any manifest or data object opens, each follower protects the signed cut
/// under its *own* epoch before it resolves a source, a fenced permit refuses
/// every later open, and neither epoch's release touches the other's row.
///
/// # Panics
///
/// Panics when protection is absent before an open, when a fenced permit still
/// reads, or when a release happens in the wrong order.
#[tokio::test]
async fn leader_and_followers_protect_before_io_and_join_before_release() {
    let fixture = forge_support::PromotionIntegrationFixture::start("reader_journey").await;
    commit_one_snapshot(&fixture).await;
    let (leader, leader_node, leader_fence) =
        oracle_epoch(&fixture, "http://oracle-leader:5002").await;
    let (follower, follower_node, follower_fence) =
        oracle_epoch(&fixture, "http://oracle-follower:5002").await;

    // Identity resolution is metadata-only: it names the cut and protects nothing.
    let prepared = fixture
        .catalog
        .prepare_reader_identity(&fixture.binding.table_ref, fixture.tenant)
        .await
        .expect("the registered table resolves");
    let (identity, local) =
        vala_bifrost_redux::oracle::reader_pins::local_cut_from_prepared(&prepared)
            .expect("the prepared identity is protectable")
            .expect("promotion committed a snapshot to protect");
    assert!(
        node_protection(&fixture, &identity, leader_node, leader_fence)
            .await
            .is_none(),
        "resolving an identity must not claim protection"
    );

    // Protection is durable before the cut opens a manifest or a data object.
    let (leader_guard, leader_permit) = leader
        .acquire_guard(std::slice::from_ref(&prepared))
        .await
        .expect("the leader protects its cut");
    let opened = node_protection(&fixture, &identity, leader_node, leader_fence)
        .await
        .expect("leader protection is durable before any source open");
    assert!(opened.frontier.covers(local.snapshot_id));
    let pinned = fixture
        .catalog
        .materialize_reader_cut(prepared, &leader_permit)
        .await
        .expect("the protected cut materializes");
    assert!(
        !pinned.iceberg_file_paths.is_empty(),
        "the materialized cut opened the committed manifest under its permit"
    );

    let signed =
        vala_bifrost_redux::oracle::reader_pins::follower_reader_cut(&pinned, leader_fence)
            .expect("the pinned cut signs")
            .expect("a committed snapshot signs a follower cut");
    follower_protection_precedes_resolution(
        &fixture,
        &follower,
        &identity,
        &signed,
        (follower_node, follower_fence),
        &opened,
        (leader_node, leader_fence),
    )
    .await;

    // Neither epoch's release touches the other's row.
    assert!(
        node_protection(&fixture, &identity, follower_node, follower_fence)
            .await
            .is_none(),
        "the follower released its own protection"
    );
    let held = node_protection(&fixture, &identity, leader_node, leader_fence)
        .await
        .expect("one epoch's retirement never releases another's protection");
    assert_eq!(held, opened);
    drop(leader_guard);
    drop(leader_permit);
    leader.retire().await.expect("the leader retires");
    assert!(
        node_protection(&fixture, &identity, leader_node, leader_fence)
            .await
            .is_none(),
        "the leader releases only after its own query is gone"
    );
}

/// Drives the follower phase: protect, retry, fence, and release.
///
/// # Panics
///
/// Panics when the follower resolves before protecting, when a fenced permit
/// still reads or exposes, when loss removes protection, or when the follower's
/// lifecycle disturbs the leader's header.
async fn follower_protection_precedes_resolution(
    fixture: &forge_support::PromotionIntegrationFixture,
    follower: &Arc<OracleReaderAuthority>,
    identity: &TableAuthorityIdentity,
    signed: &wyrd_spec::vala::api::FollowerReaderCut,
    follower_epoch: (Uuid, u64),
    leader_header: &ProtectionRecord,
    leader_epoch: (Uuid, u64),
) {
    let (follower_node, follower_fence) = follower_epoch;
    let (leader_node, leader_fence) = leader_epoch;
    assert!(
        node_protection(fixture, identity, follower_node, follower_fence)
            .await
            .is_none(),
        "a follower starts with no protection of its own"
    );
    let cuts = vec![(identity.clone(), signed.clone())];
    let (attempt, attempt_permit) = follower
        .acquire_follower_guard(&cuts)
        .await
        .expect("the follower protects the signed cut");
    let header = node_protection(fixture, identity, follower_node, follower_fence)
        .await
        .expect("each follower's own epoch protects before it resolves");
    assert!(header.frontier.covers(signed.snapshot_id));
    attempt_permit
        .begin_io()
        .expect("a protected follower may open its assigned source");

    // A retry takes its own guard while the reachable attempt still holds one,
    // and releasing the old attempt cannot narrow what the retry still needs.
    let (retry, retry_permit) = follower
        .acquire_follower_guard(&cuts)
        .await
        .expect("a retry acquires its own guard");
    drop(attempt);
    assert!(
        node_protection(fixture, identity, follower_node, follower_fence)
            .await
            .expect("the retry still needs the snapshot")
            .frontier
            .covers(signed.snapshot_id)
    );

    // Loss stops every read the fragment could still start or expose, and
    // removes nothing: the follower cannot prove its descendants stopped.
    follower.self_fence().await;
    assert!(
        attempt_permit.begin_io().is_err(),
        "a fenced epoch starts no new source IO"
    );
    assert!(
        retry_permit.expose_result().is_err(),
        "a read begun before loss cannot expose bytes afterward"
    );
    assert!(
        node_protection(fixture, identity, follower_node, follower_fence)
            .await
            .is_some(),
        "no protection is removed after loss"
    );
    let reprepared = fixture
        .catalog
        .prepare_reader_identity(&fixture.binding.table_ref, fixture.tenant)
        .await
        .expect("the registered table still resolves");
    assert!(
        fixture
            .catalog
            .materialize_reader_cut(reprepared, &retry_permit)
            .await
            .is_err(),
        "every source open a cut needs goes through its permit"
    );
    assert_eq!(
        node_protection(fixture, identity, leader_node, leader_fence)
            .await
            .as_ref(),
        Some(leader_header),
        "a follower's whole lifecycle never touches the leader's header"
    );

    drop(retry);
    follower.retire().await.expect("the follower retires");
}

/// Proves an expired epoch this node cannot reclaim fails startup recovery
/// rather than being logged past, so the local epoch never activates.
///
/// Recovery runs before activation precisely so this node does not begin
/// serving while a dead peer still holds protection. Reporting success after a
/// per-epoch reclaim failed defeats that ordering: startup would continue, the
/// epoch would activate, and readiness would be published over retention that
/// was never released. The failure must therefore reach the caller, and the
/// epoch must remain exactly as acquired.
///
/// # Panics
///
/// Panics when recovery reports success, when the local epoch admits, or when
/// its durable row moved past acquisition.
#[tokio::test]
async fn startup_recovery_failure_prevents_epoch_activation_and_readiness() {
    let fixture = AuthorityFixture::start().await;
    let terminator = Arc::new(RecordingEpochTerminator::default());
    let authority = OracleReaderAuthority::start(OracleReaderAuthorityConfig {
        vala: fixture.database.vala_postgres().clone(),
        operator_pool: fixture.database.operator_pool().clone(),
        node_id: fixture.node_id,
        fencing_token: fixture.fence,
        max_concurrent_queries: 1,
        terminator: Arc::clone(&terminator) as Arc<_>,
        shutdown: fixture.shutdown.clone(),
    })
    .await
    .expect("epoch acquires");

    // An expired epoch already past invalidation is enumerated by the sweep and
    // then matches no reclaimable row, which is exactly the per-epoch recovery
    // failure startup must not absorb.
    let pool = fixture
        .database
        .superuser_pool()
        .await
        .expect("superuser pool");
    let stale_fence = i64::try_from(fixture.fence).expect("fence fits") + 1_000;
    sqlx::query(
        "INSERT INTO vala.oracle_reader_epochs \
           (epoch_owner_tenant_id, node_id, fencing_token, state, state_revision, \
            acquired_at, renewed_at, lease_expires_at, invalidated_at) \
         VALUES ($1, $2, $3, 'invalidated', 1, now() - interval '2 minutes', \
                 now() - interval '2 minutes', now() - interval '1 minute', now())",
    )
    .bind(uuid::Uuid::from(DataTenantId::SYSTEM_OWNER))
    .bind(fixture.node_id)
    .bind(stale_fence)
    .execute(&pool)
    .await
    .expect("the unreclaimable expired epoch seeds");

    let recovery = OracleEpochRecovery::new(
        fixture.database.operator_pool().clone(),
        fixture.database.vala_postgres().clone(),
    );
    let error = recovery
        .reclaim_expired(64)
        .await
        .expect_err("a per-epoch reclaim failure is a startup failure");
    assert!(
        error.to_string().contains("epoch"),
        "the reported failure names the epoch recovery could not reclaim: {error}"
    );

    assert!(
        !authority.admits(),
        "an epoch whose startup recovery failed never opens admission"
    );
    assert_eq!(
        fixture
            .epoch_row()
            .await
            .expect("the local epoch row survives")
            .state,
        vala_sql::row_types::oracle_reader_authority::OracleEpochState::Acquired,
        "startup stopped before activation"
    );

    authority
        .retire()
        .await
        .expect("the acquired epoch retires");
}

/// Proves narrowing costs nothing when the released cut is still covered, and
/// that a narrowing whose commit fails is retried exactly once.
///
/// Two guards on identical cuts leave one frontier. Releasing the first
/// changes nothing durable, so it must remove its pin locally without a
/// statement, a compare-and-set, or an audit row — the wrong shape here is a
/// no-op revision bump on every concurrent query's release. Releasing the
/// second does change the durable set, so a failed commit must leave the pin
/// exactly where it was and the same command must be applied again, which is
/// what makes the retry meaningful rather than a second no-op.
///
/// # Panics
///
/// Panics when the covered release spends SQL, when the retried release does
/// not become durable, or when the injected fault is not consumed.
#[tokio::test]
async fn narrowing_retries_once_and_covered_duplicates_need_no_sql() {
    let fixture = AuthorityFixture::start().await;
    let (authority, _terminator) = fixture.authority(2).await;
    let tenant = fixture.tenant().await;
    let events = fixture.table(tenant, "events").await;

    let first = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("the first query protects");
    let second = authority
        .acquire_guard_for_cuts(vec![(events.clone(), cut(30, 300, &[30, 20, 10]))])
        .await
        .expect("the second query shares that protection");

    let expanded = fixture
        .header(&events)
        .await
        .expect("the shared protection is durable");
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec!["oracle.table_protection.expanded".to_owned()],
        "an already covered second query commits nothing"
    );

    // Releasing a covered duplicate leaves the frontier identical, so it must
    // not reach Postgres at all.
    drop(first);
    fixture
        .settle(&events, "the shared protection is retained", |record| {
            record.is_some_and(|record| record.frontier.covers(30))
        })
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        fixture
            .header(&events)
            .await
            .expect("the shared protection survives")
            .revision,
        expanded.revision,
        "releasing a covered duplicate commits no revision"
    );
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec!["oracle.table_protection.expanded".to_owned()],
        "releasing a covered duplicate writes no audit row"
    );

    // The last release does change the durable set. Its first commit fails, so
    // the pin must survive and the same command must be applied again.
    authority.inject_protection_faults_for_test(1);
    drop(second);
    fixture
        .settle(&events, "the retried release", |record| record.is_none())
        .await;
    assert_eq!(
        authority.pending_protection_faults_for_test(),
        0,
        "the injected fault was consumed by the first attempt"
    );
    assert_eq!(
        fixture.audit_operations(&events).await,
        vec![
            "oracle.table_protection.expanded".to_owned(),
            "oracle.table_protection.released".to_owned(),
        ],
        "the retry commits exactly one release"
    );

    authority.retire().await.expect("the epoch retires");
}

/// Proves catalog promotion under a prepared reader identity is detected and
/// restarts the complete admission rather than materializing a stale cut.
///
/// Protection is taken against the metadata document preparation read, so a
/// promotion in between leaves a guard that covers a snapshot the query is no
/// longer going to read. The authoritative reload is what notices, and the
/// answer is a complete restart: the guard is dropped and every table is
/// prepared, protected, revalidated, and materialized again. A second drift is
/// reported rather than chased.
///
/// # Panics
///
/// Panics when a real promotion is not detected, when the restart does not
/// re-prepare and materialize the complete set, or when a second drift does not
/// surface as a typed metadata mismatch.
#[tokio::test]
async fn catalog_promotion_between_prepare_and_materialize_restarts_all_tables() {
    let fixture = forge_support::PromotionIntegrationFixture::start("reader_revalidate").await;
    let (authority, _node, _fence) = oracle_epoch(&fixture, "http://oracle-revalidate:5002").await;

    // A real promotion between preparation and revalidation is exactly the
    // drift the authoritative reload exists to find.
    let prepared = fixture
        .catalog
        .prepare_reader_identity(&fixture.binding.table_ref, fixture.tenant)
        .await
        .expect("the registered table resolves");
    fixture
        .catalog
        .revalidate_reader_identity(&prepared)
        .await
        .expect("an unpromoted identity revalidates");
    commit_one_snapshot(&fixture).await;
    let drift = fixture
        .catalog
        .revalidate_reader_identity(&prepared)
        .await
        .expect_err("a promoted table no longer holds the prepared identity");
    assert!(
        matches!(
            drift,
            vala_bifrost_redux::catalog::BifrostCatalogError::MetadataMismatch(_)
        ),
        "promotion is reported as metadata drift, not as a load failure: {drift:?}"
    );
    let reprepared = fixture
        .catalog
        .prepare_reader_identity(&fixture.binding.table_ref, fixture.tenant)
        .await
        .expect("the promoted table resolves again");
    fixture
        .catalog
        .revalidate_reader_identity(&reprepared)
        .await
        .expect("a freshly prepared identity revalidates");

    // Driving the production sequence with one drift must restart it whole:
    // every table prepared again, and none materialized under the guard that
    // was dropped.
    let tables = vec![fixture.binding.table_ref.clone()];
    vala_bifrost_redux::catalog::reset_prepared_identity_count_for_test();
    vala_bifrost_redux::catalog::reset_sealed_pin_count_for_test();
    vala_bifrost_redux::catalog::inject_revalidation_faults_for_test(1);
    let materialized =
        vala_bifrost_redux::oracle::planner::OraclePlanner::protect_and_materialize_for_test(
            &tables,
            fixture.tenant,
            std::time::Instant::now() + Duration::from_secs(30),
            &fixture.catalog,
            &authority,
        )
        .await
        .expect("the restarted attempt materializes the complete set");
    assert_eq!(materialized, tables.len());
    assert_eq!(
        vala_bifrost_redux::catalog::pending_revalidation_faults_for_test(),
        0,
        "the first attempt consumed the injected drift"
    );
    assert_eq!(
        vala_bifrost_redux::catalog::prepared_identity_count_for_test(),
        tables.len() * 2,
        "the restart re-prepares every table, not only the one that drifted"
    );
    assert_eq!(
        vala_bifrost_redux::catalog::sealed_pin_count_for_test(),
        tables.len(),
        "nothing is materialized under the guard the drift discarded"
    );

    // A second drift is a typed failure, not a third attempt.
    vala_bifrost_redux::catalog::reset_prepared_identity_count_for_test();
    vala_bifrost_redux::catalog::reset_sealed_pin_count_for_test();
    vala_bifrost_redux::catalog::inject_revalidation_faults_for_test(2);
    let error =
        vala_bifrost_redux::oracle::planner::OraclePlanner::protect_and_materialize_for_test(
            &tables,
            fixture.tenant,
            std::time::Instant::now() + Duration::from_secs(30),
            &fixture.catalog,
            &authority,
        )
        .await
        .expect_err("a second drift under one query is refused");
    assert!(
        matches!(
            error,
            wyrd_spec::vala::error::BifrostError::MetadataMismatch { .. }
        ),
        "the second drift surfaces as a typed metadata mismatch: {error:?}"
    );
    assert_eq!(
        vala_bifrost_redux::catalog::prepared_identity_count_for_test(),
        tables.len() * 2,
        "admission restarts exactly once"
    );
    assert_eq!(
        vala_bifrost_redux::catalog::sealed_pin_count_for_test(),
        0,
        "a refused admission materializes nothing"
    );

    authority.retire().await.expect("the epoch retires");
}
