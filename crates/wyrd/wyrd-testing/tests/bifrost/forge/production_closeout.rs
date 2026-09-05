//! Public, cross-pod qualification of production Forge geometry and cleanup.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arrow::array::{BinaryBuilder, Int64Array, RecordBatch, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use iceberg::spec::DataFile;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use rand::{RngCore, SeedableRng, rngs::StdRng};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::forge::{ForgeConfig, ForgeWorkerCompletionObserver};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::resources::{
    FORGE_MEMORY_FLOOR_BYTES, ROLE_MEMORY_FLOOR_BYTES, ResourceSource, SystemResourceSnapshot,
};
use vala_bifrost_redux::scribe::geometry::DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES;
use vala_sql::row_types::forge_tasks::ForgeTaskStrategy;
use wyrd_client::WyrdClient;
use wyrd_server::config::BifrostRuntimeRole;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::NodeId;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, TestOracleResources, WyrdTestCluster};

use crate::public_support::{
    JourneyTable, ManagedRow, append_values, canonical_order, read_managed_rows, register_table,
    tenant_client, unique_table,
};

/// Diagnostic limit for a scheduler pass; never the production ticker interval.
const PASS_BOUND: Duration = Duration::from_secs(15);
/// Production-sized rewrites may spend several minutes encoding physical bytes.
const REWRITE_BOUND: Duration = Duration::from_secs(600);
/// Payload requests stay below the production 16 MiB ingress limit.
const ROWS_PER_REQUEST: usize = 12_000;
/// One flush carries roughly 610 MiB, producing a 512 MiB object and residue.
const REQUESTS_PER_FLUSH: usize = 52;
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
}

impl CloseoutJourney {
    /// Starts dedicated ingest, query, and worker nodes, retaining a delayed coordinator.
    ///
    /// # Panics
    /// Panics if the production topology cannot start or lacks its observer.
    async fn start() -> Self {
        let mut spec = BifrostClusterSpec::dedicated_forge_workers();
        let coordinator_node = spec.nodes[3].node_id;
        let scribe_node = spec.nodes[0].node_id;
        let oracle_node = spec.nodes[2].node_id;
        spec.nodes[3].roles = spec.nodes[0].roles.clone();
        spec.nodes[0].roles = [BifrostRuntimeRole::Scribe].into_iter().collect();
        spec.nodes[2].roles = [BifrostRuntimeRole::Oracle].into_iter().collect();
        // Both Oracle replicas must derive the same durable admission ceiling.
        // The dedicated replica needs no Scribe or Forge protected floors.
        spec.nodes[2].oracle = Some(TestOracleResources {
            spill_root: None,
            system_resources: Some(SystemResourceSnapshot {
                memory_limit_bytes: 3 * 1024 * 1024 * 1024
                    - ROLE_MEMORY_FLOOR_BYTES
                    - FORGE_MEMORY_FLOOR_BYTES,
                effective_cpu: 4,
                scratch_capacity_bytes: 4 * 1024 * 1024 * 1024,
                scratch_available_bytes: 4 * 1024 * 1024 * 1024,
                memory_source: ResourceSource::Injected,
                cpu_source: ResourceSource::Injected,
            }),
        });
        let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
            spec,
            ForgeConfig {
                // Admit production hot files through the existing small-file policy.
                small_file_threshold_bytes: 768 * 1024 * 1024,
                ..ForgeConfig::default()
            },
            true,
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
        }
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
                "1073741824".to_owned(),
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
            Some("1073741824")
        );
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
                     ('succeeded', 'failed', 'cancelled', 'unschedulable')), \
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

    /// Corroborates one completed rewrite with transactional audit and metrics.
    ///
    /// # Panics
    /// Panics on absent or contradictory operation/audit evidence, an unfinished
    /// ownership gauge, or missing physical data-flow counters.
    async fn assert_rewrite_evidence(&self, tenant: DataTenantId, expected: usize) {
        let mut conn = self
            .coordinator()
            .tenant_conn_for(tenant)
            .await
            .expect("tenant-scoped audit inspection");
        let rows: Vec<(Uuid, i64, i64, String, String, serde_json::Value)> = sqlx::query_as(
            "SELECT o.operation_id, o.prepared_audit_seq, o.terminal_audit_seq, \
             p.operation, t.operation, o.prepared_detail \
             FROM vala.forge_operation_state o \
             JOIN vala.audit_outbox p ON p.data_tenant_id=o.data_tenant_id AND p.seq=o.prepared_audit_seq \
             JOIN vala.audit_outbox t ON t.data_tenant_id=o.data_tenant_id AND t.seq=o.terminal_audit_seq \
             WHERE o.family='iceberg_rewrite'",
        ).fetch_all(&mut **conn.transaction()).await.expect("rewrite audit evidence");
        conn.commit()
            .await
            .expect("read-only audit inspection completes");
        assert_eq!(
            rows.len(),
            expected,
            "every completed rewrite has one audited terminal settlement"
        );
        for (operation, prepared, terminal, prepared_name, terminal_name, detail) in rows {
            assert!(prepared < terminal);
            assert_eq!(prepared_name, "forge.iceberg_rewrite.prepared");
            assert_eq!(terminal_name, "forge.iceberg_rewrite.committed");
            eprintln!(
                "rewrite {operation}, audit {prepared}->{terminal}, fenced preparation {detail}"
            );
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
        for sample in metrics
            .iter()
            .filter(|sample| sample.family == "bifrost_forge_active_tasks")
        {
            assert_eq!(sample.value, 0.0, "settled ownership: {sample:?}");
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
}

impl GeometryWorkload {
    /// Starts one reproducible workload with no acknowledged rows.
    fn new() -> Self {
        Self {
            event_time: chrono::Utc::now(),
            random: StdRng::seed_from_u64(0x5eed),
            expected: Vec::new(),
        }
    }

    /// Appends one target-sized round through authenticated public gRPC.
    ///
    /// # Panics
    /// Panics on malformed local Arrow data or any refused public append.
    async fn append_round(&mut self, client: &WyrdClient, table: &str) {
        let transport = vala_sdk::grpc::BifrostGrpcTransport::connect(client)
            .await
            .expect("ingest transport");
        for _ in 0..REQUESTS_PER_FLUSH {
            let first = i64::try_from(self.expected.len()).expect("bounded row count");
            let values: Vec<i64> = (first
                ..first + i64::try_from(ROWS_PER_REQUEST).expect("bounded request"))
                .collect();
            let batch = self.batch(&values);
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
                .insert_batch(table, batch_id.into_bytes(), ipc)
                .await
                .expect("public append acknowledged");
            self.expected
                .extend(
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(ordinal, value)| ManagedRow {
                            batch_id,
                            row_ordinal: i32::try_from(ordinal).expect("bounded ordinal"),
                            value,
                        }),
                );
        }
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

/// Checks physical policy targets and ordered row-group boundaries from Iceberg.
///
/// # Panics
/// Panics when no approximately 1 GiB output/residue exists or row groups escape
/// their physical file. Targets are approximate because writers roll whole groups.
fn assert_output_geometry(outputs: &BTreeMap<String, DataFile>, rows: usize) {
    let output_sizes: Vec<_> = outputs.values().map(DataFile::file_size_in_bytes).collect();
    eprintln!(
        "Forge physical bytes: {output_sizes:?}; exact rows: {}",
        rows
    );
    assert!(
        output_sizes.iter().any(|size| *size >= 900 * 1024 * 1024),
        "approximately 1 GiB output: {output_sizes:?}"
    );
    assert!(
        output_sizes.iter().any(|size| *size < 900 * 1024 * 1024),
        "valid final residue: {output_sizes:?}"
    );
    for file in outputs.values() {
        let offsets = file.split_offsets().expect("row-group offsets");
        assert!(!offsets.is_empty());
        if file.file_size_in_bytes() >= 900 * 1024 * 1024 {
            assert!(
                offsets.len() > 1,
                "a production replacement has multiple row groups"
            );
        }
        assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(offsets.iter().all(|offset| {
            u64::try_from(*offset).is_ok_and(|offset| offset < file.file_size_in_bytes())
        }));
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
    let writer = tenant_client(journey.scribe(), tenant).await;
    let reader = tenant_client(journey.oracle(), tenant).await;
    let neighbour_writer = tenant_client(journey.scribe(), neighbour).await;
    let neighbour_reader = tenant_client(journey.oracle(), neighbour).await;
    let neighbour_expected = canonical_order(
        append_values(
            &neighbour_writer,
            &neighbour_table.qualified,
            Uuid::now_v7(),
            &[9001, 9002, 9003],
        )
        .await,
    );
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
            .filter(|file| file.file_size >= DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES)
            .count()
            >= 2,
        "multiple production 512 MiB inputs required: {sizes:?}"
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
    let (neighbour_snapshot, neighbour_files) = journey.live_files(&neighbour_table.binding).await;
    // The managed core first normalizes promoted-file identity, then packs
    // current-recipe files. Continue packing residues until a pass is unchanged.
    let mut replacement = journey.live_files(&table.binding).await;
    for pass in 0..8 {
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
        if unchanged {
            assert!(
                replacement
                    .1
                    .values()
                    .any(|file| file.file_size_in_bytes() >= 900 * 1024 * 1024),
                "geometry backlog must make progress before reaching an unchanged cut"
            );
            break;
        }
    }
    let (replacement_snapshot, outputs) = replacement;
    assert_ne!(replacement_snapshot, promoted_snapshot);
    assert!(
        outputs.keys().all(|path| !inputs.contains_key(path)),
        "new cuts use replacements"
    );
    assert_output_geometry(&outputs, workload.expected.len());
    journey.assert_objects(&inputs).await;
    journey.assert_objects(&outputs).await;
    assert_eq!(
        read_managed_rows(&reader, &table.qualified).await,
        workload.expected
    );
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
    journey.assert_rewrite_evidence(tenant, rewrites).await;
    journey.cluster.shutdown().await.expect("all roles drain");
}
