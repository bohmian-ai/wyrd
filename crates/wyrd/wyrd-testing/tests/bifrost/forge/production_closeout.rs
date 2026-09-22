//! Public, cross-pod qualification of production Forge geometry and cleanup.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::public_support::{
    JourneyTable, ManagedRow, append_values, canonical_order, read_managed_rows, register_table,
    tenant_client, unique_table,
};
use arrow::array::{BinaryBuilder, Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use iceberg::spec::DataFile;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::forge::{ForgeConfig, ForgeWorkerCompletionObserver};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{ResourceSource, SystemResourceSnapshot};
use vala_bifrost_redux::storage::{StorageOperation, StorageOperationBarrier};
use vala_sql::row_types::forge_tasks::{ForgeTaskStrategy, evidence_from_json};
use wyrd_client::WyrdClient;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::write::RawIngest;
use wyrd_testing::bifrost::{
    BifrostClusterSpec, CommitUncertaintyCatalog, TestOracleResources, WyrdTestCluster,
};

/// Reads one node's live Oracle reader-authority fence.
///
/// Durable protection rows are keyed by node and epoch, so an inspection has to
/// name the exact fence the reading node currently holds rather than any value
/// carried on the wire.
///
/// # Panics
/// Panics if the node is absent, composes no Oracle, or reports a negative fence.
fn oracle_fence(cluster: &WyrdTestCluster, node: NodeId) -> u64 {
    let authority = cluster
        .server_by_node(node)
        .expect("the inspected node is composed")
        .state()
        .bifrost
        .oracle()
        .expect("the inspected node composes an Oracle")
        .engine()
        .reader_authority()
        .fencing_token();
    u64::try_from(authority).expect("positive Oracle fence")
}

/// Every Oracle-serving node's ranged-read pause, armed before a lazy public read.
///
/// The destination Oracle answers a delegated cut on its analytical follower,
/// which acquires that node's durable reader protection *before* it opens any
/// object. Stalling the production storage owner at its first ranged read
/// therefore holds the query at a point where the protection it committed is
/// already durable and observable, without adding any production seam.
struct OracleReadBarriers {
    /// One barrier per Oracle-serving node, paired with that node's identity.
    entries: Vec<(NodeId, Arc<StorageOperationBarrier>)>,
}

impl OracleReadBarriers {
    /// Installs one `ReadRange` barrier on every Oracle-serving node.
    ///
    /// # Panics
    /// Panics if no composed node owns both an Oracle and a storage owner.
    fn arm(cluster: &WyrdTestCluster) -> Self {
        let mut entries = Vec::new();
        for server in cluster.servers() {
            if server.state().bifrost.oracle().is_none() {
                continue;
            }
            let Some(storage) = server.state().bifrost_storage() else {
                continue;
            };
            let barrier = StorageOperationBarrier::new(StorageOperation::ReadRange);
            storage.install_operation_barrier_for_test(Arc::clone(&barrier));
            entries.push((server.node_id(), barrier));
        }
        assert!(
            !entries.is_empty(),
            "at least one Oracle node must be armed"
        );
        Self { entries }
    }

    /// Waits for the first node to stall a ranged read and releases the rest.
    ///
    /// Releasing the others is what keeps concurrent maintenance on unrelated
    /// pods running while exactly the reading node stays held.
    async fn first_reached(&self) -> NodeId {
        let reached =
            futures_util::future::select_all(self.entries.iter().map(|(node, barrier)| {
                Box::pin(async move {
                    barrier.wait_until_reached().await;
                    *node
                })
            }))
            .await
            .0;
        for (node, barrier) in &self.entries {
            if *node != reached {
                barrier.release();
            }
        }
        reached
    }

    /// Releases every remaining stall so the held query can finish.
    fn release(&self) {
        for (_, barrier) in &self.entries {
            barrier.release();
        }
    }
}

impl Drop for OracleReadBarriers {
    /// Releases every stall so an early exit cannot leave a query held.
    fn drop(&mut self) {
        self.release();
    }
}

/// Diagnostic limit for a scheduler pass; never the production ticker interval.
const PASS_BOUND: Duration = Duration::from_secs(15);
/// Production-sized rewrites may spend several minutes encoding physical bytes.
const REWRITE_BOUND: Duration = Duration::from_secs(600);
/// Environment flag selecting the standard production geometry profile.
///
/// Journey-only: it changes nothing but the sizes this test's own setup
/// declares, so both modes run the identical body, workload, and assertions.
const PRODUCTION_GEOMETRY_FLAG: &str = "WYRD_FORGE_PRODUCTION_GEOMETRY";

/// Physical sizes one qualification mode declares for Scribe and Iceberg.
///
/// The proof is geometric — target rolling, exactly one residue, multi-row-group
/// outputs, replanned backlog — and every one of those properties is a ratio
/// rather than an absolute size. Scaling all of the sizes together therefore
/// keeps the assertions meaningful while letting the ordinary lane finish in
/// seconds; the production profile runs the same test at the standard sizes.
#[derive(Clone, Copy)]
struct GeometryProfile {
    /// Rows one public append request carries.
    rows_per_request: usize,
    /// Requests one flushed round issues.
    requests_per_flush: usize,
    /// Scribe assembled-object target the staging geometry rolls at.
    scribe_target_bytes: u64,
    /// Iceberg rolling target the table declares.
    iceberg_target_bytes: u64,
    /// Parquet row-group target, always below the file target so a rolled
    /// output necessarily contains more than one group.
    row_group_bytes: u64,
    /// Compaction eligibility threshold this journey's Forge config uses.
    small_file_threshold_bytes: u64,
    /// Whether the qualification sizing of the worker and Oracle pods applies.
    production_resources: bool,
}

impl GeometryProfile {
    /// Scaled default: the same geometry two orders of magnitude smaller.
    const FAST: Self = Self {
        // One round carries the same multiple of the target as production
        // does, but in fewer, larger requests: a staging claim closes on the
        // smallest member prefix whose bytes reach the target, and the merged
        // object re-encodes a fraction below that sum. Coarse members keep the
        // crossing object above the target instead of a fraction under it.
        rows_per_request: 940,
        requests_per_flush: 6,
        scribe_target_bytes: 4 * 1024 * 1024,
        iceberg_target_bytes: 8 * 1024 * 1024,
        row_group_bytes: 1024 * 1024,
        // Just under the file target, as production's is: a packed residue that
        // has not yet reached the target is still small, so the backlog keeps
        // rolling instead of stalling on one intermediate output the next pass
        // may no longer touch.
        small_file_threshold_bytes: 7 * 1024 * 1024,
        production_resources: false,
    };

    /// Standard production sizing, selected only by the focused lane.
    const PRODUCTION: Self = Self {
        rows_per_request: 12_000,
        requests_per_flush: 52,
        scribe_target_bytes: 512 * 1024 * 1024,
        iceberg_target_bytes: 1024 * 1024 * 1024,
        row_group_bytes: 128 * 1024 * 1024,
        small_file_threshold_bytes: 768 * 1024 * 1024,
        production_resources: true,
    };

    /// Reads the profile this process runs, defaulting to the fast one.
    fn selected() -> Self {
        if std::env::var_os(PRODUCTION_GEOMETRY_FLAG).is_some() {
            Self::PRODUCTION
        } else {
            Self::FAST
        }
    }
}
/// Incompressible payload width makes physical targets measurable.
const PAYLOAD_BYTES: usize = 1024;

/// Owns the real servers and passive completion evidence for one journey.
struct CloseoutJourney {
    /// Shared Postgres/storage with independent serving and maintenance nodes.
    cluster: WyrdTestCluster,
    /// Notification source emitted after production worker outcomes.
    observer: ForgeWorkerCompletionObserver,
    /// Stable identity of the initially delayed coordinator.
    coordinator_node: NodeId,
    /// Stable identity of the independent ingest process.
    scribe_node: NodeId,
    /// Stable identity of the independent query process.
    oracle_node: NodeId,
    /// Stable identity of the dedicated maintenance worker.
    worker_node: NodeId,
}

impl CloseoutJourney {
    /// Starts dedicated ingest, query, and worker nodes, retaining a delayed coordinator.
    ///
    /// # Panics
    /// Panics if the production topology cannot start or lacks its observer.
    async fn start() -> Self {
        Self::start_with_config(ForgeConfig {
            small_file_threshold_bytes: GeometryProfile::selected().small_file_threshold_bytes,
            ..ForgeConfig::default()
        })
        .await
    }

    /// Starts the same role topology with the journey's maintenance policy.
    ///
    /// # Panics
    /// Panics if the real role graph cannot start or its observer is absent.
    async fn start_with_config(config: ForgeConfig) -> Self {
        let profile = GeometryProfile::selected();
        let mut spec = BifrostClusterSpec::dedicated_forge_workers().with_scribe_geometry_for_test(
            vala_bifrost_redux::scribe::geometry::ScribeGeometry::default()
                .with_staging_target_file_size_bytes(profile.scribe_target_bytes)
                .expect("the selected staging target is a valid geometry"),
        );
        let coordinator_node = spec.nodes[3].node_id;
        let scribe_node = spec.nodes[0].node_id;
        let oracle_node = spec.nodes[2].node_id;
        let worker_node = spec.nodes[1].node_id;
        spec.nodes[3].roles = spec.nodes[0].roles.clone();
        spec.nodes[0].roles = [BifrostRuntimeRole::Scribe].into_iter().collect();
        spec.nodes[2].roles = [BifrostRuntimeRole::Oracle].into_iter().collect();
        // Only the qualification profile needs an oversized worker pod: its
        // 512 MiB inputs decode into a working set the admission estimate puts
        // well past an ordinary pod. The scaled default admits its plans on the
        // harness default budget.
        if profile.production_resources {
            spec.nodes[1].forge_compaction_memory_limit_bytes = Some(16 * 1024 * 1024 * 1024);
            spec.nodes[1].oracle = Some(TestOracleResources {
                data_root_parent: None,
                system_resources: Some(SystemResourceSnapshot {
                    memory_limit_bytes: 32 * 1024 * 1024 * 1024,
                    effective_cpu: 4,
                    scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
                    scratch_available_bytes: 4 * 1024 * 1024 * 1024,
                    memory_source: ResourceSource::Injected,
                    cpu_source: ResourceSource::Injected,
                }),
            });
        }
        if !profile.production_resources {
            // The scaled default runs the same journey on the resource plan
            // every node reports for itself: no Forge compaction budget and no
            // injected snapshot anywhere. Each Oracle now derives its own local
            // capacity, so two replicas no longer have to agree on one durable
            // ceiling.
            for node in &spec.nodes {
                assert_eq!(
                    node.forge_compaction_memory_limit_bytes, None,
                    "the fast profile overrides no Forge compaction budget"
                );
                assert!(
                    node.oracle
                        .as_ref()
                        .is_none_or(|oracle| oracle.system_resources.is_none()),
                    "the fast profile injects no node resource snapshot"
                );
            }
        }
        let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
            spec, config, true, true,
        )
        .await
        .expect("separate production roles start");
        let observer = cluster
            .forge_completion_observer()
            .expect("worker observer");
        assert_eq!(cluster.configured_node_ids().len(), 4);
        eprintln!(
            "closeout role/node identities: {:?}",
            cluster.configured_node_ids()
        );
        Self {
            cluster,
            observer,
            coordinator_node,
            scribe_node,
            oracle_node,
            worker_node,
        }
    }

    /// Advances only the retained deterministic maintenance clocks.
    ///
    /// # Panics
    /// Panics if the requested test time is unrepresentable.
    fn advance_maintenance(&self, duration: chrono::Duration) {
        for server in self.cluster.servers() {
            server
                .forge_clock()
                .advance(duration)
                .expect("maintenance time advances");
        }
    }

    /// Drives real passes until one compacted file owns this small test table.
    ///
    /// # Panics
    /// Panics if promotion/rewrite fails or cannot settle the fixed-hour data.
    async fn compact_small_table(
        &self,
        binding: &TenantTableBinding,
    ) -> (i64, BTreeMap<String, DataFile>) {
        let mut previous = None;
        for _ in 0..12 {
            self.scheduler_pass().await;
            self.drain_tasks().await;
            let current = self.live_files(binding).await;
            if current.1.len() == 1
                && previous == Some(current.0)
                && current.1.keys().all(|path| path.contains("/data/forge/"))
            {
                return current;
            }
            previous = Some(current.0);
        }
        panic!("small fixed-hour table did not converge through real maintenance");
    }

    /// Requires a named object to be physically absent, rejecting other IO errors.
    ///
    /// # Panics
    /// Panics on a storage error other than absence.
    async fn object_missing(&self, path: &str) -> bool {
        match self.cluster.storage_operator().stat(path).await {
            Ok(_) => false,
            Err(error) if error.kind() == opendal::ErrorKind::NotFound => true,
            Err(error) => panic!("object inspection failed: {error}"),
        }
    }

    /// Observes an exact production expired-cleanup delete with protection held.
    ///
    /// The post-delete pause permits inspection of its still-prepared durable
    /// claim before settlement; no SQL transaction spans the storage effect.
    ///
    /// # Panics
    /// Panics if deletion stalls, has the wrong owner/route, loses a protected
    /// object, or lacks its terminal settlement.
    async fn collect_exact(
        &self,
        binding: &TenantTableBinding,
        path: &str,
        protected: &BTreeMap<String, DataFile>,
    ) {
        let worker = self
            .cluster
            .server_by_node(self.worker_node)
            .expect("worker node");
        let control = worker
            .forge_object_store_control_for_test()
            .expect("real storage control");
        control.pause_after_delete_for_path(path);
        let drive = async {
            for _ in 0..12 {
                self.scheduler_pass().await;
                self.drain_tasks().await;
                if self.object_missing(path).await {
                    return;
                }
            }
            panic!("production cleanup did not delete {path}");
        };
        let inspect = async {
            tokio::time::timeout(PASS_BOUND, control.wait_for_completed_delete())
                .await
                .expect("named object is actually deleted");
            assert!(self.object_missing(path).await);
            self.assert_objects(protected).await;
            let mut conn = self
                .coordinator()
                .tenant_conn_for(binding.tenant)
                .await
                .expect("tenant inspection");
            let rows: Vec<(Uuid, Uuid, serde_json::Value)> = sqlx::query_as(
                "SELECT t.task_id, t.claimed_by, t.evidence FROM vala.forge_tasks t \
                 JOIN vala.forge_tasks s ON s.task_id=(t.plan->'parameters'->>'source_task_id')::uuid \
                 WHERE t.strategy='expired_cleanup' AND t.state='prepared' \
                 AND s.strategy='snapshot_expiry' AND s.state='succeeded'",
            ).fetch_all(&mut **conn.transaction()).await.expect("prepared cleanup claim");
            conn.commit().await.expect("inspection releases SQL");
            let matching: Vec<_> = rows
                .into_iter()
                .filter_map(|(task, owner, raw)| {
                    let evidence = evidence_from_json(raw).expect("validated cleanup evidence");
                    let index = usize::try_from(evidence.prepared_candidate_index?)
                        .expect("candidate index");
                    (evidence.cleanup_candidates[index].path.as_str() == path)
                        .then_some((task, owner))
                })
                .collect();
            assert_eq!(
                matching.len(),
                1,
                "exact expired-cleanup candidate owns the delete"
            );
            assert_eq!(matching[0].1, self.worker_node.as_uuid());
            control.release_completed_delete();
            matching[0].0
        };
        let ((), task) = tokio::join!(drive, inspect);
        let mut conn = self
            .coordinator()
            .tenant_conn_for(binding.tenant)
            .await
            .expect("tenant lineage");
        // Cleanup evaluates no principal permission, so its own task evidence —
        // not audit — counts the deletions it made.
        let deleted: i64 = sqlx::query_scalar(
            "SELECT COALESCE((evidence->>'deleted_candidate_count')::bigint, 0) \
             FROM vala.forge_tasks WHERE task_id=$1",
        )
        .bind(task)
        .fetch_one(&mut **conn.transaction())
        .await
        .expect("cleanup deletion lineage");
        conn.commit()
            .await
            .expect("lineage inspection releases SQL");
        assert!(deleted > 0);
        eprintln!(
            "expired cleanup task={task}, worker={}, physically deleted={path}",
            self.worker_node.as_uuid()
        );
    }

    /// Lists every physical object under this table's Forge output root.
    ///
    /// The listing is the same real prefix the collection route walks, so the
    /// difference between two listings is exactly what a rewrite closed, with
    /// no fixture ever naming an object on the writer's behalf.
    ///
    /// # Panics
    /// Panics when the storage listing fails.
    async fn forge_objects(&self, binding: &TenantTableBinding) -> BTreeSet<String> {
        let prefix = format!("{}/data/forge/", binding.object_prefix.as_str());
        self.cluster
            .storage_operator()
            .list_with(&prefix)
            .recursive(true)
            .await
            .expect("Forge output listing")
            .into_iter()
            .filter(|entry| entry.metadata().is_file())
            .map(|entry| entry.path().to_owned())
            .collect()
    }

    /// Asserts the coordinator's production orphan predicate for each object.
    ///
    /// The verdict comes from the retained protection loader on a node that is
    /// not the one executing the work, which is what makes it observable while
    /// a worker holds a catalog commit open.
    ///
    /// # Panics
    /// Panics when the production classifier fails or returns another verdict.
    async fn assert_eligibility(
        &self,
        binding: &TenantTableBinding,
        paths: &BTreeSet<String>,
        expected: &str,
        why: &str,
    ) {
        for path in paths {
            assert_eq!(
                self.coordinator()
                    .forge_gc_eligibility_for_test(binding, path)
                    .await
                    .expect("production eligibility classification"),
                expected,
                "{why}: {path}"
            );
        }
    }

    /// Corroborates collection with its own lineage operation identity.
    ///
    /// Forge evaluates no principal permission, so `vala.forge_operation_state`
    /// — not audit — is the authority this reads: the route owns an
    /// `orphan_gc` operation that reached a terminal phase.
    ///
    /// # Panics
    /// Panics when the route wrote no operation, its operation never settled,
    /// or a destructive sibling route ran beside it.
    async fn assert_orphan_evidence(&self, tenant: DataTenantId) {
        let mut conn = self
            .coordinator()
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped lineage inspection");
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT operation_id, phase \
             FROM vala.forge_operation_state \
             WHERE family='orphan_gc'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("orphan collection lineage evidence");
        let strategies: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT strategy FROM vala.forge_tasks WHERE data_tenant_id=wyrd.current_tenant()",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("durable strategy inventory");
        conn.commit()
            .await
            .expect("read-only lineage inspection completes");
        assert!(
            !rows.is_empty(),
            "the collection route owns its own lineage operation identity"
        );
        for (operation, phase) in &rows {
            assert!(
                phase == "committed" || phase == "recovered",
                "collection left operation {operation} in phase {phase}"
            );
            eprintln!("orphan collection operation {operation}: {phase}");
        }
        assert!(
            strategies
                .iter()
                .all(|strategy| strategy != "snapshot_expiry" && strategy != "expired_cleanup"),
            "a sibling destructive route ran beside collection: {strategies:?}"
        );
        let metrics = self
            .cluster
            .telemetry()
            .snapshot()
            .expect("production metrics");
        assert!(
            metrics.iter().any(
                |sample| sample.family == "bifrost_forge_deleted_objects_total"
                    && sample.value > 0.0
            ),
            "collection reported no physical deletion"
        );
        eprintln!(
            "collection worker identities={:?}",
            self.observer.completed_workers()
        );
    }

    /// Registers the payload schema through the retained server catalog harness.
    ///
    /// # Panics
    /// Panics if registration or tenant-qualified identity validation fails.
    async fn register_payload_table(&self, tenant: DataTenantId, name: String) -> JourneyTable {
        let table_ref = TableRef::new(BifrostNamespace::Datasets, &name);
        self.scribe()
            .create_bifrost_table_for_test(CreateTableRequest {
                table: table_ref.clone(),
                tenant,
                user_fields: vec![
                    Field::new("value", DataType::Int64, false),
                    Field::new("payload", DataType::Binary, false),
                ],
                physical_layout: None,
                audit: None,
            })
            .await
            .expect("payload table registration");
        JourneyTable {
            qualified: format!("{}.{name}", BifrostNamespace::Datasets.as_str()),
            name,
            binding: TenantTableBinding::resolve((tenant, table_ref)).expect("table binding"),
        }
    }

    /// Declares the qualification's 1 GiB policy before any data is written.
    ///
    /// This configures the real Iceberg table, whose undeclared dependency
    /// default is 512 MiB; it does not replace a planner or execute maintenance.
    ///
    /// # Panics
    /// Panics if catalog configuration fails or does not preserve the declared target.
    async fn declare_geometry(&self, binding: &TenantTableBinding) {
        let profile = GeometryProfile::selected();
        let catalog = self.scribe().bifrost_catalog().iceberg_catalog();
        let table = catalog
            .load_table(&binding.table_ident())
            .await
            .expect("qualification table");
        let tx = Transaction::new(&table);
        let tx = tx
            .update_table_properties()
            .set(
                "write.target-file-size-bytes".to_owned(),
                profile.iceberg_target_bytes.to_string(),
            )
            // Below the file target in both modes, so every output that rolled
            // at the target necessarily closed more than one row group.
            .set(
                "write.parquet.row-group-size-bytes".to_owned(),
                profile.row_group_bytes.to_string(),
            )
            .apply(tx)
            .expect("declared table geometry");
        let table = tx
            .commit_once(catalog.as_ref())
            .await
            .expect("table policy commits");
        assert_eq!(
            table
                .metadata()
                .properties()
                .get("write.target-file-size-bytes")
                .map(String::as_str),
            Some(profile.iceberg_target_bytes.to_string().as_str())
        );
    }

    /// Reads back the rolling target the table itself declares, in bytes.
    ///
    /// Geometry is judged against the published property rather than a
    /// hard-coded figure, so the assertions describe the policy the writer was
    /// actually given rather than a size the test happens to expect.
    ///
    /// # Panics
    /// Panics when the table is unreadable or declares no parsable target.
    async fn declared_target_bytes(&self, binding: &TenantTableBinding) -> u64 {
        self.scribe()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("geometry table")
            .metadata()
            .properties()
            .get("write.target-file-size-bytes")
            .expect("declared rolling target")
            .parse()
            .expect("declared rolling target is a byte count")
    }

    /// Borrows the live coordinator node.
    ///
    /// # Panics
    /// Panics if the retained node is absent.
    fn coordinator(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.coordinator_node)
            .expect("coordinator node")
    }

    /// Borrows the dedicated Scribe serving node.
    ///
    /// # Panics
    /// Panics if the ingest node is absent.
    fn scribe(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.scribe_node)
            .expect("Scribe node")
    }

    /// Borrows the separate Oracle serving node.
    ///
    /// # Panics
    /// Panics if the retained node is absent.
    fn oracle(&self) -> &WyrdTestServer {
        self.cluster
            .server_by_node(self.oracle_node)
            .expect("Oracle node")
    }

    /// Drains tasks while one deliberately abandoned rewrite is expected.
    ///
    /// The ordinary drain treats any returned worker error as a defect. One
    /// scenario needs a real attempt to be refused after it closed its outputs
    /// and before it prepared, so this variant accepts exactly the refusal that
    /// scenario injects at the publication reacquisition it drives, and still
    /// refuses every other worker error. The observer keeps every error it ever
    /// saw, so every drain after that injection has to use this variant.
    ///
    /// # Panics
    /// Panics on SQL failure, a stalled attempt, or any other worker error.
    async fn drain_tasks_allowing_injected_refusal(&self) {
        tokio::time::timeout(REWRITE_BOUND, async {
            loop {
                for error in self.observer.returned_errors() {
                    assert!(
                        error.contains("injected Forge catalog load failure"),
                        "worker failed for an unexpected reason: {error}"
                    );
                }
                let next = self.observer.attempts() + 1;
                let (pending, attempts): (i64, i64) = sqlx::query_as(
                    "SELECT count(*) FILTER (WHERE state NOT IN \
                     ('succeeded', 'failed', 'cancelled')), \
                     coalesce(sum(attempt_count), 0)::bigint FROM vala.forge_tasks",
                )
                .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
                .await
                .expect("durable task and ownership inspection");
                if pending == 0
                    && self.observer.attempts()
                        >= usize::try_from(attempts).expect("nonnegative attempts")
                {
                    break;
                }
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .expect("the abandoned rewrite and its retry both settle");
    }

    /// Settles the planning pass the coordinator runs as soon as it starts.
    ///
    /// A coordinator plans immediately on start, so a fixture that drives its
    /// own passes must settle that boot pass before arranging the world.
    /// Otherwise the boot pass plans concurrently with the first driven pass
    /// and the run observes tasks neither pass alone accounts for.
    ///
    /// # Panics
    /// Panics if the boot pass does not complete in 15 seconds.
    async fn await_boot_pass(&self) {
        tokio::time::timeout(
            PASS_BOUND,
            self.coordinator()
                .wait_for_forge_scheduler_passes_for_test(1),
        )
        .await
        .expect("a freshly started coordinator completes its boot planning pass");
    }

    /// Requests and observes one real scheduler pass before inspecting SQL.
    ///
    /// # Panics
    /// Panics if the supervisor does not complete the requested pass in 15 seconds.
    async fn scheduler_pass(&self) {
        let server = self.coordinator();
        let before = server.completed_forge_scheduler_passes_for_test();
        let started = Instant::now();
        server.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            PASS_BOUND,
            server.wait_for_forge_scheduler_passes_for_test(before + 1),
        )
        .await
        .expect("production scheduler responds to its trigger within 15 seconds");
        eprintln!(
            "scheduler passes {before} -> {} in {:?}",
            server.completed_forge_scheduler_passes_for_test(),
            started.elapsed()
        );
    }

    /// Drains already-created tasks using completion notifications and durable state.
    ///
    /// # Panics
    /// Panics on SQL failure, a stalled attempt, or a production worker error.
    async fn drain_tasks(&self) {
        tokio::time::timeout(REWRITE_BOUND, async {
            loop {
                assert!(
                    self.observer.returned_errors().is_empty(),
                    "worker failed: {:?}",
                    self.observer.returned_errors()
                );
                let next = self.observer.attempts() + 1;
                let (pending, attempts): (i64, i64) = sqlx::query_as(
                    "SELECT count(*) FILTER (WHERE state NOT IN \
                     ('succeeded', 'failed', 'cancelled')), \
                     coalesce(sum(attempt_count), 0)::bigint FROM vala.forge_tasks",
                )
                .fetch_one(self.cluster.pg_fixture().operator_pool().pool())
                .await
                .expect("durable task and ownership inspection");
                if pending == 0
                    && self.observer.attempts()
                        >= usize::try_from(attempts).expect("nonnegative attempts")
                {
                    break;
                }
                self.observer.wait_for_attempts_at_least(next).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "worker drain stalled: {:?}",
                self.observer.returned_errors()
            )
        });
        assert!(
            self.observer.returned_errors().is_empty(),
            "{:?}",
            self.observer.returned_errors()
        );
    }

    /// Reads exact live files from the current catalog snapshot's manifests.
    ///
    /// # Panics
    /// Panics on missing catalog authority, unreadable manifests, or duplicate files.
    async fn live_files(&self, binding: &TenantTableBinding) -> (i64, BTreeMap<String, DataFile>) {
        let table = self
            .coordinator()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("catalog table");
        let snapshot = table
            .metadata()
            .current_snapshot()
            .expect("published snapshot");
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("manifest list");
        let mut files = BTreeMap::new();
        for manifest in manifests.entries() {
            let manifest = manifest
                .load_manifest(table.file_io())
                .await
                .expect("manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                let file = entry.data_file();
                let start = file
                    .file_path()
                    .find(binding.object_prefix.as_str())
                    .expect("file belongs to this tenant/table prefix");
                assert!(
                    files
                        .insert(file.file_path()[start..].to_owned(), file.clone())
                        .is_none()
                );
            }
        }
        (snapshot.snapshot_id(), files)
    }

    /// Counts the snapshots the table currently retains.
    ///
    /// Publication is per plan, so the retained snapshot count — not the task
    /// count — is what an attempt's operations have to line up against.
    ///
    /// # Panics
    /// Panics when the catalog cannot load the table.
    async fn snapshot_count(&self, binding: &TenantTableBinding) -> usize {
        self.coordinator()
            .bifrost_catalog()
            .iceberg_catalog()
            .load_table(&binding.table_ident())
            .await
            .expect("catalog table")
            .metadata()
            .snapshots()
            .count()
    }

    /// Corroborates every completed rewrite with its lineage and metrics.
    ///
    /// An attempt publishes each of its admitted plans independently, so a
    /// completed rewrite settles *one operation per plan*, not one per task —
    /// and a task whose planning finds nothing to rewrite self-settles with
    /// none. The task count therefore bounds nothing; each plan holds its own
    /// operation in `vala.forge_operation_state`, settled into a terminal
    /// phase. Forge evaluates no principal permission, so that projection — not
    /// audit — is the authority read here.
    ///
    /// Returns how many operations that evidence covers, so the caller can hold
    /// it against the snapshots the same passes published.
    ///
    /// # Panics
    /// Panics on unsettled operation evidence, on two plans sharing one
    /// operation identity, or on missing physical data-flow counters.
    async fn assert_rewrite_evidence(&self, tenant: DataTenantId) -> usize {
        let mut conn = self
            .coordinator()
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped lineage inspection");
        let rows: Vec<(Uuid, String, serde_json::Value)> = sqlx::query_as(
            "SELECT operation_id, phase, prepared_detail \
             FROM vala.forge_operation_state \
             WHERE family='iceberg_rewrite'",
        )
        .fetch_all(&mut **conn.transaction())
        .await
        .expect("rewrite lineage evidence");
        conn.commit()
            .await
            .expect("read-only lineage inspection completes");
        assert_eq!(
            rows.iter()
                .map(|(operation, ..)| *operation)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            rows.len(),
            "no two published plans settled under one operation identity"
        );
        let operations = rows.len();
        for (operation, phase, detail) in rows {
            assert_eq!(
                phase, "committed",
                "rewrite operation {operation} did not settle"
            );
            eprintln!("rewrite {operation}, phase {phase}, fenced preparation {detail}");
        }
        let metrics = self
            .cluster
            .telemetry()
            .snapshot()
            .expect("production metrics");
        for family in [
            "bifrost_forge_input_files_total",
            "bifrost_forge_input_bytes_total",
            "bifrost_forge_output_files_total",
            "bifrost_forge_output_bytes_total",
        ] {
            assert!(
                metrics.iter().any(|sample| sample.family == family
                    && sample.value > 0.0
                    && sample
                        .labels
                        .get("task_type")
                        .is_some_and(|kind| kind == "small_files")),
                "{family}"
            );
        }
        let authority = self
            .oracle()
            .state()
            .bifrost
            .oracle()
            .expect("Oracle role")
            .engine()
            .reader_authority();
        eprintln!(
            "Oracle epoch node={} fence={}; worker identities={:?}",
            authority.node_id(),
            authority.fencing_token(),
            self.observer.completed_workers()
        );
        operations
    }

    /// Proves each named object still physically exists at its committed size.
    ///
    /// # Panics
    /// Panics if an object disappeared or its physical size differs from Iceberg.
    async fn assert_objects(&self, files: &BTreeMap<String, DataFile>) {
        for (path, file) in files {
            let metadata = self
                .cluster
                .storage_operator()
                .stat(path)
                .await
                .expect("retained object");
            assert_eq!(metadata.content_length(), file.file_size_in_bytes());
        }
    }
}

/// Owns bounded, deterministic incompressible data and its acknowledged row set.
struct GeometryWorkload {
    /// Pinned event time keeps every request in one physical partition.
    event_time: chrono::DateTime<chrono::Utc>,
    /// Installed standard generator avoids a separate random-byte implementation.
    random: StdRng,
    /// Exact acknowledged managed identities and payload values.
    expected: Vec<ManagedRow>,
    /// Sizes this process runs the qualification at.
    profile: GeometryProfile,
}

impl GeometryWorkload {
    /// Starts one reproducible workload with no acknowledged rows.
    fn new() -> Self {
        Self {
            event_time: chrono::Utc::now(),
            random: StdRng::seed_from_u64(0x5eed),
            expected: Vec::new(),
            profile: GeometryProfile::selected(),
        }
    }

    /// Appends one target-sized round through authenticated public gRPC.
    ///
    /// # Panics
    /// Panics on malformed local Arrow data or any refused public append.
    async fn append_round(&mut self, client: &WyrdClient, table: &str) {
        let transport = RawIngest::connect(client).await.expect("ingest transport");
        for _ in 0..self.profile.requests_per_flush {
            let first = i64::try_from(self.expected.len()).expect("bounded row count");
            let values: Vec<i64> = (first
                ..first + i64::try_from(self.profile.rows_per_request).expect("bounded request"))
                .collect();
            self.append_batch(&transport, table, &values).await;
        }
    }

    /// Appends exact values with the workload's fixed event-time partition.
    ///
    /// # Panics
    /// Panics on malformed Arrow data or a refused public append.
    async fn append_batch(&mut self, transport: &RawIngest, table: &str, values: &[i64]) {
        let batch = self.batch(values);
        let mut ipc = Vec::new();
        {
            let mut writer =
                arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
                    .expect("IPC writer");
            writer.write(&batch).expect("IPC batch");
            writer.finish().expect("IPC terminal");
        }
        let batch_id = Uuid::now_v7();
        transport
            .insert(table, batch_id, ipc)
            .await
            .expect("public append acknowledged");
        self.expected.extend(
            values
                .iter()
                .enumerate()
                .map(|(ordinal, value)| ManagedRow {
                    batch_id,
                    row_ordinal: i32::try_from(ordinal).expect("bounded ordinal"),
                    value: *value,
                }),
        );
    }

    /// Builds the next bounded batch without changing production object geometry.
    ///
    /// # Panics
    /// Panics if the locally constructed arrays disagree with their schema.
    fn batch(&mut self, values: &[i64]) -> RecordBatch {
        let mut payload = BinaryBuilder::with_capacity(values.len(), values.len() * PAYLOAD_BYTES);
        let mut block = [0_u8; PAYLOAD_BYTES];
        for _ in values {
            self.random.fill_bytes(&mut block);
            payload.append_value(block);
        }
        RecordBatch::try_new(
            Arc::new(Schema::new(vec![
                Field::new("value", DataType::Int64, false),
                Field::new("payload", DataType::Binary, false),
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
            ])),
            vec![
                Arc::new(Int64Array::from(values.to_vec())),
                Arc::new(payload.finish()),
                Arc::new(
                    TimestampMicrosecondArray::from(vec![
                        self.event_time.timestamp_micros();
                        values.len()
                    ])
                    .with_timezone("UTC"),
                ),
            ],
        )
        .expect("payload batch")
    }
}

/// Proves each output rolled at the declared target on a whole row group.
///
/// A writer can only close a file on a completed row group, so the exact
/// property being checked is that the *final* group is the one that carried the
/// file across the target: every earlier group ended below it, and nothing was
/// written after the crossing. That distinguishes a correct rolling threshold
/// from a writer that keeps appending past its target or cuts early, which a
/// size band cannot. Exactly one file — the last residue — stays below target.
///
/// # Panics
/// Panics when no output crosses the target, when a crossing file started its
/// final group at or beyond the target, when more than one residue exists, or
/// when a row group escapes its physical file.
fn assert_output_geometry(outputs: &BTreeMap<String, DataFile>, rows: usize, target: u64) {
    let output_sizes: Vec<_> = outputs.values().map(DataFile::file_size_in_bytes).collect();
    eprintln!(
        "Forge physical bytes: {output_sizes:?}; exact rows: {}",
        rows
    );
    let residues = output_sizes.iter().filter(|size| **size < target).count();
    assert_eq!(
        residues, 1,
        "only the final residue stays below the declared target {target}: {output_sizes:?}"
    );
    assert!(
        output_sizes.iter().any(|size| *size >= target),
        "at least one output rolled at the declared target {target}: {output_sizes:?}"
    );
    for (path, file) in outputs {
        let offsets = file.split_offsets().expect("row-group offsets");
        assert!(!offsets.is_empty(), "{path} reports its row groups");
        assert!(
            offsets.windows(2).all(|pair| pair[0] < pair[1]),
            "{path} row-group offsets ascend: {offsets:?}"
        );
        assert!(
            offsets.iter().all(|offset| {
                u64::try_from(*offset).is_ok_and(|offset| offset < file.file_size_in_bytes())
            }),
            "{path} row groups stay inside the physical file"
        );
        if file.file_size_in_bytes() < target {
            continue;
        }
        assert!(
            offsets.len() > 1,
            "a rolled replacement has multiple row groups: {path}"
        );
        let final_group = u64::try_from(
            *offsets
                .last()
                .expect("a non-empty offset list has a last entry"),
        )
        .expect("a row-group offset inside the file is representable");
        assert!(
            final_group < target,
            "{path} was still below the declared target {target} when its final row group opened, \
             so that group is the one that crossed: final group at {final_group}, size {}",
            file.file_size_in_bytes()
        );
    }
}

/// Qualifies real 512 MiB Scribe inputs and approximately 1 GiB Forge outputs.
///
/// Public exact rows, manifest membership, old-object presence and a second
/// unchanged pass jointly distinguish publication from destructive cleanup.
///
/// # Panics
/// Panics on any route, geometry, exactness, tenancy, or convergence violation.
#[tokio::test]
#[ignore = "requires Postgres and production-sized object storage"]
async fn compaction_geometry_exact_rows_and_non_destructive_second_pass() {
    let mut journey = CloseoutJourney::start().await;
    let tenant = journey.cluster.data_tenant_id();
    let neighbour = journey
        .cluster
        .add_tenant(&unique_table("geometry_neighbour"))
        .await
        .expect("second tenant");
    let name = unique_table("geometry");
    let table = journey.register_payload_table(tenant, name).await;
    let neighbour_table = register_table(journey.scribe(), neighbour, &table.name).await;
    journey.declare_geometry(&table.binding).await;
    journey.declare_geometry(&neighbour_table.binding).await;
    let target = journey.declared_target_bytes(&table.binding).await;
    let writer = tenant_client(journey.scribe(), tenant).await;
    let reader = tenant_client(journey.oracle(), tenant).await;
    let neighbour_writer = tenant_client(journey.scribe(), neighbour).await;
    let neighbour_reader = tenant_client(journey.oracle(), neighbour).await;
    // The neighbour is written in separate flushed rounds so it owns several
    // hot objects of its own. That gives it real maintenance demand, and its
    // tasks then compete for the same worker as the geometry table's plans
    // rather than sitting idle beside them.
    let mut neighbour_rows = Vec::new();
    for values in [[9001, 9002, 9003], [9004, 9005, 9006], [9007, 9008, 9009]] {
        neighbour_rows.extend(
            append_values(
                &neighbour_writer,
                &neighbour_table.qualified,
                Uuid::now_v7(),
                &values,
            )
            .await,
        );
        journey
            .scribe()
            .flush_bifrost()
            .await
            .expect("the competing tenant's rows flush");
    }
    let neighbour_expected = canonical_order(neighbour_rows);
    let mut workload = GeometryWorkload::new();
    for round in 0..2 {
        let started = Instant::now();
        workload.append_round(&writer, &table.qualified).await;
        journey
            .scribe()
            .flush_bifrost()
            .await
            .expect("publicly acknowledged rows flush");
        eprintln!(
            "geometry input round {round} flushed in {:?}",
            started.elapsed()
        );
    }
    let hot = journey
        .scribe()
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &table.name)
        .await
        .expect("physical Scribe inputs");
    let sizes: Vec<_> = hot.iter().map(|file| file.file_size).collect();
    eprintln!("Scribe physical bytes: {sizes:?}");
    assert!(
        hot.iter()
            .filter(|file| file.file_size >= workload.profile.scribe_target_bytes)
            .count()
            >= 2,
        "multiple inputs at the selected staging target required: {sizes:?}"
    );
    workload.expected = canonical_order(workload.expected);
    assert_eq!(
        read_managed_rows(&reader, &table.qualified).await,
        workload.expected
    );
    assert_eq!(
        read_managed_rows(&neighbour_reader, &table.qualified).await,
        neighbour_expected
    );
    journey
        .cluster
        .restart_node(journey.coordinator_node)
        .await
        .expect("coordinator starts after complete ingestion");
    journey.await_boot_pass().await;
    for server in journey.cluster.servers().iter() {
        server
            .forge_clock()
            .advance(chrono::Duration::days(1))
            .expect("closed partition");
    }
    journey.scheduler_pass().await;
    journey.drain_tasks().await;
    let (promoted_snapshot, inputs) = journey.live_files(&table.binding).await;
    assert_eq!(
        inputs.len(),
        hot.len(),
        "promotion retains every physical Scribe object"
    );
    let (neighbour_promoted, _) = journey.live_files(&neighbour_table.binding).await;
    let published_before = journey.snapshot_count(&table.binding).await;
    // The managed core first normalizes promoted-file identity, then packs
    // current-recipe files. Continue packing residues until a pass is unchanged.
    let mut replacement = journey.live_files(&table.binding).await;
    let mut passes = 0;
    for pass in 0..8 {
        passes += 1;
        journey.scheduler_pass().await;
        journey.drain_tasks().await;
        let next = journey.live_files(&table.binding).await;
        eprintln!(
            "rewrite pass {pass}: {:?}",
            next.1
                .values()
                .map(DataFile::file_size_in_bytes)
                .collect::<Vec<_>>()
        );
        let unchanged = next.0 == replacement.0;
        replacement = next;
        // A pass that publishes nothing is not necessarily the end of the
        // backlog: a busy worker can return one before the packing it owes has
        // run. The cut is settled only once it is both unchanged and rolled at
        // the declared target, which is the property this loop is packing for.
        if unchanged
            && replacement
                .1
                .values()
                .any(|file| file.file_size_in_bytes() >= target)
        {
            break;
        }
    }
    let (replacement_snapshot, outputs) = replacement;
    assert!(
        outputs
            .values()
            .any(|file| file.file_size_in_bytes() >= target),
        "geometry backlog must roll at the declared target: {:?}",
        outputs
            .values()
            .map(DataFile::file_size_in_bytes)
            .collect::<Vec<_>>()
    );
    assert_ne!(replacement_snapshot, promoted_snapshot);
    assert!(
        passes > 1,
        "the geometry backlog is packed over more than one pass, so its residue \
         is replanned rather than published in one commit"
    );
    let (neighbour_snapshot, neighbour_files) = journey.live_files(&neighbour_table.binding).await;
    assert_ne!(
        neighbour_snapshot, neighbour_promoted,
        "the competing table's own maintenance drained through the same worker \
         as the geometry table's plans"
    );
    assert!(
        outputs.keys().all(|path| !inputs.contains_key(path)),
        "new cuts use replacements"
    );
    assert_output_geometry(&outputs, workload.expected.len(), target);
    journey.assert_objects(&inputs).await;
    journey.assert_objects(&outputs).await;
    // A public read that returns the right rows can still be reading a stale
    // cut. Pause the follower at its first-batch gate and require its durable
    // protection to name the replacement snapshot: that is what proves the
    // post-compaction read pinned the new cut rather than the promoted one.
    let barriers = OracleReadBarriers::arm(&journey.cluster);
    let paused_read = read_managed_rows(&reader, &table.qualified);
    let inspect_protection = async {
        let reader_node = tokio::time::timeout(PASS_BOUND, barriers.first_reached())
            .await
            .expect("lazy query reaches its first ranged read");
        let record = journey
            .cluster
            .server_by_node(reader_node)
            .expect("the stalled reader is a composed node")
            .oracle_table_protection_for_test(
                &table.binding,
                reader_node,
                oracle_fence(&journey.cluster, reader_node),
            )
            .await
            .expect("protection read")
            .expect("durable protection before the first ranged read");
        assert!(
            record
                .frontier
                .members
                .iter()
                .any(|member| member.protected_snapshot_id == replacement_snapshot),
            "the post-compaction read pins the replacement snapshot \
             {replacement_snapshot}: {:?}",
            record.frontier.members
        );
        barriers.release();
    };
    let (actual, ()) = tokio::join!(paused_read, inspect_protection);
    assert_eq!(actual, workload.expected);
    let rewrites = journey
        .observer
        .completed_strategies()
        .iter()
        .filter(|strategy| {
            **strategy
                == vala_sql::row_types::forge_tasks::ForgeClaimStrategy::Known(
                    ForgeTaskStrategy::SmallFiles,
                )
        })
        .count();
    assert!(rewrites > 0);
    journey.scheduler_pass().await;
    journey.drain_tasks().await;
    let (second_snapshot, second_files) = journey.live_files(&table.binding).await;
    assert_eq!(
        second_snapshot, replacement_snapshot,
        "unchanged pass publishes no snapshot"
    );
    assert_eq!(
        second_files.keys().collect::<Vec<_>>(),
        outputs.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        journey.live_files(&neighbour_table.binding).await.0,
        neighbour_snapshot
    );
    journey.assert_objects(&neighbour_files).await;
    assert_eq!(
        read_managed_rows(&neighbour_reader, &table.qualified).await,
        neighbour_expected
    );
    journey.assert_objects(&inputs).await;
    // Each admitted plan publishes on its own, so the rewrite passes owe one
    // operation per snapshot they added — not one per completed task.
    let operations = journey.assert_rewrite_evidence(tenant).await;
    assert_eq!(
        operations,
        journey.snapshot_count(&table.binding).await - published_before,
        "every snapshot the rewrite passes published carries its own operation"
    );
    // Audited reads above keep publishing retained audit, so the coordinator
    // can plan and start audit-log maintenance after the last drain. Ownership
    // is therefore judged once every role has drained, when every attempt
    // guard must have returned its increment.
    let telemetry = journey.cluster.telemetry().clone();
    journey.cluster.shutdown().await.expect("all roles drain");
    for sample in telemetry
        .snapshot()
        .expect("production metrics")
        .iter()
        .filter(|sample| sample.family == "bifrost_forge_active_tasks")
    {
        assert_eq!(sample.value, 0.0, "settled ownership: {sample:?}");
    }
}

/// Owns two tenants and the real old cut held across destructive maintenance.
struct ReaderCleanupJourney {
    /// Independent serving and maintenance nodes.
    roles: CloseoutJourney,
    /// Fixed-hour data whose successive snapshots the reader protects.
    table: JourneyTable,
    /// Same public name in a different tenant.
    neighbour_table: JourneyTable,
    /// Authenticated public query client on the dedicated Oracle node.
    reader: WyrdClient,
    /// Authenticated neighbor query client on that same public endpoint.
    neighbour_reader: WyrdClient,
    /// Public ingest transport retained across successive appends.
    transport: RawIngest,
    /// Fixed partition, deterministic payload and exact acknowledged identities.
    workload: GeometryWorkload,
    /// Neighbor identities that must remain unchanged throughout cleanup.
    neighbour_expected: Vec<ManagedRow>,
    /// Snapshot selected before the paused query starts.
    old_snapshot: i64,
    /// Exact data files the paused query must retain.
    old_files: BTreeMap<String, DataFile>,
    /// Earlier replaced data object eligible for deletion while the query lives.
    earlier_object: String,
}

impl ReaderCleanupJourney {
    /// Writes two real hot files, then publishes and compacts the reader's cut.
    ///
    /// # Panics
    /// Panics if public setup, promotion, or compaction fails.
    async fn start() -> Self {
        let mut roles = CloseoutJourney::start_with_config(ForgeConfig {
            snapshot_expiry_enabled: true,
            ..ForgeConfig::default()
        })
        .await;
        let tenant = roles.cluster.data_tenant_id();
        let neighbour = roles
            .cluster
            .add_tenant(&unique_table("reader_neighbour"))
            .await
            .expect("neighbor tenant");
        let table = roles
            .register_payload_table(tenant, unique_table("retained_reader"))
            .await;
        let neighbour_table = register_table(roles.scribe(), neighbour, &table.name).await;
        let writer = tenant_client(roles.scribe(), tenant).await;
        let reader = tenant_client(roles.oracle(), tenant).await;
        let neighbour_writer = tenant_client(roles.scribe(), neighbour).await;
        let neighbour_reader = tenant_client(roles.oracle(), neighbour).await;
        let transport = RawIngest::connect(&writer).await.expect("public ingest");
        let neighbour_expected = canonical_order(
            append_values(
                &neighbour_writer,
                &table.qualified,
                Uuid::now_v7(),
                &[9001, 9002],
            )
            .await,
        );
        let mut workload = GeometryWorkload::new();
        for values in [&[1, 2][..], &[3, 4]] {
            workload
                .append_batch(&transport, &table.qualified, values)
                .await;
            roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("acknowledged inputs flush");
        }
        roles
            .cluster
            .restart_node(roles.coordinator_node)
            .await
            .expect("coordinator starts");
        roles.await_boot_pass().await;
        roles.advance_maintenance(chrono::Duration::hours(2));
        roles.scheduler_pass().await;
        roles.drain_tasks().await;
        let (_, earlier_files) = roles.live_files(&table.binding).await;
        assert!(earlier_files.len() >= 2, "two real promoted inputs");
        let earlier_object = earlier_files.keys().next().expect("earlier object").clone();
        let (old_snapshot, old_files) = roles.compact_small_table(&table.binding).await;
        assert!(!old_files.contains_key(&earlier_object));
        roles.assert_objects(&earlier_files).await;
        assert_eq!(
            read_managed_rows(&reader, &table.qualified).await,
            canonical_order(workload.expected.clone())
        );
        Self {
            roles,
            table,
            neighbour_table,
            reader,
            neighbour_reader,
            transport,
            workload,
            neighbour_expected,
            old_snapshot,
            old_files,
            earlier_object,
        }
    }

    /// Waits for every Oracle epoch to narrow its durable frontier after terminal.
    ///
    /// # Panics
    /// Panics if an epoch retains the completed old query or inspection fails.
    async fn wait_for_reader_release(&self) {
        tokio::time::timeout(PASS_BOUND, async {
            loop {
                let mut retained = false;
                for server in self.roles.cluster.servers() {
                    if let Some(oracle) = server.state().bifrost.oracle() {
                        let authority = oracle.engine().reader_authority();
                        let record = server
                            .oracle_table_protection_for_test(
                                &self.table.binding,
                                server.node_id(),
                                u64::try_from(authority.fencing_token())
                                    .expect("positive Oracle fence"),
                            )
                            .await
                            .expect("durable frontier inspection");
                        retained |= record.is_some_and(|record| {
                            record
                                .frontier
                                .members
                                .iter()
                                .any(|member| member.ancestry_path.contains(&self.old_snapshot))
                        });
                    }
                }
                if !retained {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("terminal query releases durable protection");
    }

    /// Holds a public reader while earlier objects are deleted, then releases it.
    ///
    /// # Panics
    /// Panics if the query was not protected, cleanup touches its inputs, or its
    /// terminal rows change after another process publishes and expires snapshots.
    async fn protect_during_cleanup(&mut self) {
        let expected = canonical_order(self.workload.expected.clone());
        let barriers = OracleReadBarriers::arm(&self.roles.cluster);
        let query = read_managed_rows(&self.reader, &self.table.qualified);
        let maintenance = async {
            let reader_node = tokio::time::timeout(PASS_BOUND, barriers.first_reached())
                .await
                .expect("lazy query reaches its first ranged read");
            assert_ne!(reader_node, self.roles.worker_node);
            let record = self
                .roles
                .cluster
                .server_by_node(reader_node)
                .expect("the stalled reader is a composed node")
                .oracle_table_protection_for_test(
                    &self.table.binding,
                    reader_node,
                    oracle_fence(&self.roles.cluster, reader_node),
                )
                .await
                .expect("protection read")
                .expect("durable protection before the first ranged read");
            assert!(
                record
                    .frontier
                    .members
                    .iter()
                    .any(|member| member.protected_snapshot_id == self.old_snapshot)
            );
            self.workload
                .append_batch(&self.transport, &self.table.qualified, &[5, 6])
                .await;
            self.roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("new rows flush");
            let (current, _) = self.roles.compact_small_table(&self.table.binding).await;
            assert_ne!(current, self.old_snapshot);
            self.roles.advance_maintenance(chrono::Duration::days(2));
            self.roles
                .collect_exact(&self.table.binding, &self.earlier_object, &self.old_files)
                .await;
            let metadata = self
                .roles
                .coordinator()
                .bifrost_catalog()
                .iceberg_catalog()
                .load_table(&self.table.binding.table_ident())
                .await
                .expect("retained snapshot inspection");
            assert!(
                metadata
                    .metadata()
                    .snapshot_by_id(self.old_snapshot)
                    .is_some()
            );
            barriers.release();
        };
        let (actual, ()) = tokio::join!(query, maintenance);
        assert_eq!(
            actual, expected,
            "old reader returns exactly its protected cut"
        );
        self.wait_for_reader_release().await;
    }

    /// Moves the rewrite head onward and observes exact old-object deletion.
    ///
    /// # Panics
    /// Panics if released inputs cannot be collected, current or neighbor rows
    /// change, or production roles fail to drain.
    async fn finish(mut self) {
        self.workload
            .append_batch(&self.transport, &self.table.qualified, &[7, 8])
            .await;
        self.roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        let (_, current) = self.roles.compact_small_table(&self.table.binding).await;
        self.roles.advance_maintenance(chrono::Duration::days(2));
        let old_path = self.old_files.keys().next().expect("old compacted object");
        self.roles
            .collect_exact(&self.table.binding, old_path, &current)
            .await;
        for path in self.old_files.keys() {
            assert!(
                self.roles.object_missing(path).await,
                "released old object is deleted"
            );
        }
        assert_eq!(
            read_managed_rows(&self.reader, &self.table.qualified).await,
            canonical_order(self.workload.expected)
        );
        assert_eq!(
            read_managed_rows(&self.neighbour_reader, &self.neighbour_table.qualified).await,
            self.neighbour_expected
        );
        self.roles.assert_objects(&current).await;
        self.roles
            .cluster
            .shutdown()
            .await
            .expect("all production roles drain");
    }
}

/// An old public reader survives real cross-pod expiration and physical cleanup.
///
/// # Panics
/// Panics when durable protection, exact deletion ownership, or tenant rows fail.
#[tokio::test]
#[ignore = "requires Postgres and independent Oracle/Forge roles"]
async fn lazy_old_reader_survives_cross_pod_expiration_and_physical_cleanup() {
    let mut journey = ReaderCleanupJourney::start().await;
    journey.protect_during_cleanup().await;
    journey.finish().await;
}

/// Owns two tenants and the real output one refused publication leaves behind.
struct OrphanJourney {
    /// Independent serving and maintenance nodes.
    roles: CloseoutJourney,
    /// Table whose rewrite is refused at the real catalog boundary.
    table: JourneyTable,
    /// Same public name in a different tenant.
    neighbour_table: JourneyTable,
    /// Authenticated public query client on the dedicated Oracle node.
    reader: WyrdClient,
    /// Authenticated neighbor query client on that same public endpoint.
    neighbour_reader: WyrdClient,
    /// Public ingest transport retained across successive appends.
    transport: RawIngest,
    /// Fixed partition, deterministic payload and exact acknowledged identities.
    workload: GeometryWorkload,
    /// Neighbor identities that must remain unchanged throughout collection.
    neighbour_expected: Vec<ManagedRow>,
    /// Shared control over the real Forge catalog publication boundary.
    catalog: Arc<CommitUncertaintyCatalog>,
    /// Exact objects the refused rewrite closed and never published.
    orphans: BTreeSet<String>,
}

impl OrphanJourney {
    /// Publishes a real compacted cut, then refuses the next rewrite's commit.
    ///
    /// # Panics
    /// Panics if public setup, promotion, or the refused publication does not
    /// leave at least one closed output that no snapshot names.
    async fn start() -> Self {
        // The production collection route ages objects against the wall clock
        // its planning demand is stamped with, not the manual maintenance
        // clock, so the terminal floor has to be a real interval this journey
        // can outlive. Two seconds is long enough that the object is provably
        // young while its own rewrite is still open and short enough that the
        // bounded collection loop below crosses it.
        let mut roles = CloseoutJourney::start_with_config(ForgeConfig {
            orphan_gc_ttl: std::time::Duration::from_secs(2),
            maintenance_trigger_interval: std::time::Duration::from_millis(1),
            ..ForgeConfig::default()
        })
        .await;
        let catalog = roles
            .cluster
            .commit_uncertainty_catalog()
            .expect("the topology wraps the real Forge catalog boundary");
        let tenant = roles.cluster.data_tenant_id();
        let neighbour = roles
            .cluster
            .add_tenant(&unique_table("orphan_neighbour"))
            .await
            .expect("neighbor tenant");
        let table = roles
            .register_payload_table(tenant, unique_table("never_published"))
            .await;
        let neighbour_table = register_table(roles.scribe(), neighbour, &table.name).await;
        let writer = tenant_client(roles.scribe(), tenant).await;
        let reader = tenant_client(roles.oracle(), tenant).await;
        let neighbour_writer = tenant_client(roles.scribe(), neighbour).await;
        let neighbour_reader = tenant_client(roles.oracle(), neighbour).await;
        let transport = RawIngest::connect(&writer).await.expect("public ingest");
        let neighbour_expected = canonical_order(
            append_values(
                &neighbour_writer,
                &table.qualified,
                Uuid::now_v7(),
                &[7001, 7002],
            )
            .await,
        );
        let mut workload = GeometryWorkload::new();
        for values in [&[1, 2][..], &[3, 4]] {
            workload
                .append_batch(&transport, &table.qualified, values)
                .await;
            roles
                .scribe()
                .flush_bifrost()
                .await
                .expect("acknowledged inputs flush");
        }
        roles
            .cluster
            .restart_node(roles.coordinator_node)
            .await
            .expect("coordinator starts");
        roles.await_boot_pass().await;
        // No maintenance-clock advance here. This journey's terminal age floor
        // is a real interval measured by the production planning demand, so a
        // manual clock running ahead of storage would report every object as
        // already old and erase the young window the scenario has to observe.
        // The table owes maintenance on its own trigger interval instead.
        roles.compact_small_table(&table.binding).await;

        // One more acknowledged flush, promoted on its own pass. Promotion is a
        // catalog commit too, so it has to be finished and live before the seam
        // is armed; otherwise the held commit would be the promotion's, which
        // writes no Forge output and therefore strands nothing.
        workload
            .append_batch(&transport, &table.qualified, &[5, 6])
            .await;
        roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        roles.scheduler_pass().await;
        roles.drain_tasks().await;
        let (_, promoted) = roles.live_files(&table.binding).await;
        assert!(
            promoted.len() >= 2,
            "the promoted input is live and a rewrite is now due"
        );

        Self::prove_commit_window_retention(&roles, &table, &catalog).await;

        // A further acknowledged flush, promoted on its own pass, leaves the
        // table owing one more real rewrite: the attempt that will be refused.
        workload
            .append_batch(&transport, &table.qualified, &[7, 8])
            .await;
        roles
            .scribe()
            .flush_bifrost()
            .await
            .expect("later rows flush");
        roles.scheduler_pass().await;
        roles.drain_tasks().await;
        let orphans = Self::strand_one_generation(&roles, &table, &catalog).await;
        Self {
            roles,
            table,
            neighbour_table,
            reader,
            neighbour_reader,
            transport,
            workload,
            neighbour_expected,
            catalog,
            orphans,
        }
    }

    /// Proves an open and a commit-uncertain rewrite both retain their outputs.
    ///
    /// Two production windows, observed from the coordinator because the worker
    /// is the process inside the catalog call: a rewrite whose operation row is
    /// prepared and whose commit has not been delegated, and one whose commit
    /// the catalog accepted but has not acknowledged. Collection must refuse
    /// the outputs in both, and neither verdict may come from a fixture.
    ///
    /// # Panics
    /// Panics if either window is never reached, the rewrite closed no output,
    /// or the production predicate does not protect it.
    async fn prove_commit_window_retention(
        roles: &CloseoutJourney,
        table: &JourneyTable,
        catalog: &Arc<CommitUncertaintyCatalog>,
    ) {
        let before = roles.forge_objects(&table.binding).await;
        catalog.pause_before_commit();
        let drive = async {
            roles.scheduler_pass().await;
            roles.drain_tasks().await;
        };
        let inspect = async {
            tokio::time::timeout(REWRITE_BOUND, catalog.wait_for_before_commit())
                .await
                .expect("a real rewrite reaches the catalog publication boundary");
            let open: BTreeSet<String> = roles
                .forge_objects(&table.binding)
                .await
                .difference(&before)
                .cloned()
                .collect();
            assert!(
                !open.is_empty(),
                "the held rewrite closed at least one real output"
            );
            roles
                .assert_eligibility(&table.binding, &open, "Protected", "open rewrite")
                .await;
            // Arm the post-acceptance hold before releasing this one, so the
            // retry's own commit cannot slip past the uncertain window.
            catalog.pause_after_commit();
            catalog.reject_paused_before_commit();
            tokio::time::timeout(REWRITE_BOUND, catalog.wait_for_commit())
                .await
                .expect("the retry's commit is held after the catalog accepted it");
            roles
                .assert_eligibility(&table.binding, &open, "Protected", "uncertain commit")
                .await;
            catalog.release_paused_commit();
        };
        tokio::join!(drive, inspect);
    }

    /// Refuses one real rewrite after it closed its outputs and before it prepared.
    ///
    /// The refusal is a production one. The rewrite is held at the point its
    /// managed execution is finished and its publication has not yet reacquired
    /// authoritative metadata, and that reacquisition is then made to fail. The
    /// objects the attempt already closed are therefore named by no snapshot,
    /// no operation row, and the retry runs under a
    /// new attempt identity that cannot reuse them. While the attempt is still
    /// open those same objects must still be retained, but the age floor is the
    /// only authority that can retain them: a protection root requires the
    /// prepared operation row this attempt never reaches. The retention is
    /// observed from the coordinator because the worker is the process holding
    /// the rewrite.
    ///
    /// # Panics
    /// Panics if the barrier is never reached, the refused rewrite closed no
    /// output, or the production predicate does not retain it.
    async fn strand_one_generation(
        roles: &CloseoutJourney,
        table: &JourneyTable,
        catalog: &Arc<CommitUncertaintyCatalog>,
    ) -> BTreeSet<String> {
        let before = roles.forge_objects(&table.binding).await;
        roles.observer.hold_after_next_rewrite_handoff_for_test();
        let drive = async {
            roles.scheduler_pass().await;
            roles.drain_tasks_allowing_injected_refusal().await;
        };
        let inspect = async {
            tokio::time::timeout(
                REWRITE_BOUND,
                roles.observer.wait_for_held_rewrite_handoff_for_test(),
            )
            .await
            .expect("a real rewrite reaches its post-execution barrier");
            let orphans: BTreeSet<String> = roles
                .forge_objects(&table.binding)
                .await
                .difference(&before)
                .cloned()
                .collect();
            assert!(
                !orphans.is_empty(),
                "the held rewrite closed at least one real output"
            );
            // The attempt admits every eligible plan, and a sibling plan can
            // already have prepared and published while this one is held. Its
            // outputs are live and protected by that prepared operation, so the
            // stranded generation is the held plan's own outputs: the ones no
            // operation row names, retained by the age floor alone.
            // They must still be retained while the attempt is open. Which
            // authority answers is not fixed: the age floor retains an object
            // no operation names, and a sibling plan of the same attempt that
            // has already prepared blocks destructive maintenance for the whole
            // table until it settles. Either verdict is retention; only
            // `Eligible` would be a collectible live attempt's output.
            for path in &orphans {
                let verdict = roles
                    .coordinator()
                    .forge_gc_eligibility_for_test(&table.binding, path)
                    .await
                    .expect("the production orphan predicate answers");
                assert!(
                    verdict == "TooYoung" || verdict == "Protected",
                    "an open attempt's closed output must be retained: {path} => {verdict}"
                );
            }
            // Refuse the metadata reacquisition this rewrite performs next,
            // which is the last authority it consults before preparing.
            catalog.fail_next_load_table();
            roles.observer.release_held_rewrite_handoff_for_test();
            orphans
        };
        let ((), orphans) = tokio::join!(drive, inspect);

        // Sibling plans of the same attempt publish on their own operations, so
        // the stranded generation is the closed outputs no snapshot names once
        // the refusal has settled the attempt.
        let (_, live) = roles.live_files(&table.binding).await;
        let orphans: BTreeSet<String> = orphans
            .into_iter()
            .filter(|path| !live.contains_key(path))
            .collect();
        assert!(
            !orphans.is_empty(),
            "the refused rewrite left at least one output no snapshot names"
        );
        let named: BTreeSet<String> = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT prepared_detail FROM vala.forge_operation_state \
             WHERE data_tenant_id = $1 AND family = 'iceberg_rewrite'",
        )
        .bind(table.binding.tenant.as_uuid())
        .fetch_all(roles.cluster.pg_fixture().operator_pool().pool())
        .await
        .expect("Forge operation-state inspection")
        .iter()
        .flat_map(|detail| {
            detail
                .get("output_paths")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|path| path.as_str().map(str::to_owned))
        .collect();
        assert!(
            orphans
                .iter()
                .all(|path| !named.iter().any(|output| output.ends_with(path))),
            "the refused plan never prepared, so no operation row names its \
             outputs: {orphans:?} in {named:?}"
        );
        orphans
    }

    /// Crosses the terminal age floor and collects exactly the stranded output.
    ///
    /// # Panics
    /// Panics if collection removes a protected object, misses the orphan, or
    /// either tenant's exact rows or production evidence change.
    async fn finish(self) {
        self.roles
            .assert_eligibility(
                &self.table.binding,
                &self.orphans,
                "TooYoung",
                "young output",
            )
            .await;
        let (_, live) = self.roles.live_files(&self.table.binding).await;
        self.roles.advance_maintenance(chrono::Duration::days(2));
        // Deletion ownership is an equality, not a membership test: the pass
        // must remove the tracked generation and nothing else this table owns.
        let before_objects = self.roles.forge_objects(&self.table.binding).await;
        assert!(
            self.orphans.is_subset(&before_objects),
            "the tracked outputs are still present before collection"
        );
        self.roles
            .assert_eligibility(
                &self.table.binding,
                &self.orphans,
                "Eligible",
                "aged output",
            )
            .await;

        let before = self
            .roles
            .coordinator()
            .completed_forge_scheduler_passes_for_test();
        for _ in 0..12 {
            self.roles.scheduler_pass().await;
            self.roles.drain_tasks_allowing_injected_refusal().await;
            let mut remaining = false;
            for path in &self.orphans {
                remaining |= !self.roles.object_missing(path).await;
            }
            if !remaining {
                break;
            }
        }
        for path in &self.orphans {
            assert!(
                self.roles.object_missing(path).await,
                "the never-published output survived its own collection route: {path}"
            );
        }
        assert!(
            self.roles
                .coordinator()
                .completed_forge_scheduler_passes_for_test()
                > before,
            "collection ran through the existing scheduler trigger"
        );
        // Ownership is proved by difference, not by membership: every object
        // this table owned before the pass must survive it except the tracked
        // generation. The passes driven above can publish their own new
        // outputs, so the surviving set is a superset of that remainder rather
        // than equal to it; what may not happen is any other removal.
        let after_objects = self.roles.forge_objects(&self.table.binding).await;
        let survivors: BTreeSet<String> =
            before_objects.difference(&self.orphans).cloned().collect();
        assert!(
            survivors.is_subset(&after_objects),
            "collection removed an object outside the tracked generation: {:?}",
            survivors.difference(&after_objects).collect::<Vec<_>>()
        );
        assert!(
            after_objects.is_disjoint(&self.orphans),
            "the tracked generation survived its own collection route"
        );
        self.roles.assert_objects(&live).await;
        self.roles
            .assert_orphan_evidence(self.table.binding.tenant)
            .await;
        assert_eq!(
            read_managed_rows(&self.reader, &self.table.qualified).await,
            canonical_order(self.workload.expected.clone())
        );
        assert_eq!(
            read_managed_rows(&self.neighbour_reader, &self.neighbour_table.qualified).await,
            self.neighbour_expected
        );
        drop(self.transport);
        drop(self.catalog);
        self.roles
            .cluster
            .shutdown()
            .await
            .expect("all production roles drain");
    }
}

/// A real never-published Forge output is collected only after terminal age.
///
/// # Panics
/// Panics when protection, exact deletion ownership, or tenant rows fail.
#[tokio::test]
#[ignore = "requires Postgres and independent Oracle/Forge roles"]
async fn failed_never_published_output_is_collected_after_terminal_age() {
    let journey = OrphanJourney::start().await;
    journey.finish().await;
}
