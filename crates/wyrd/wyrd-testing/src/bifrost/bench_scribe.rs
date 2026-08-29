//! Real Scribe benchmark execution owned by the reusable Bifrost harness.

use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;

use crate::Bootstrap;
use crate::bifrost::BifrostHarness;
use arrow::array::{Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use bytes::Bytes;
use vala_bifrost_redux::bench_support::WalBenchSupport;
use vala_bifrost_redux::catalog::TimeGranularity;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::contracts::{IngressPayload, Scribe, ScribeIngressFrame};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::schema::fingerprint::SchemaFingerprint;
use vala_bifrost_redux::scribe::persistence::PersistenceFaults;
use vala_bifrost_redux::scribe::tail_rpc::FetchLiveTailRequest;
use vala_sdk::{BifrostFrame, BifrostGrpcTransport};
use wyrd_bench::{
    BenchmarkReadiness, BifrostFaultProfile, BifrostReportEnvelope, BifrostScenario,
    DurableAckReport, DurableAckSample, NegativeFlowReport, ScribeComponentReport,
    ScribeDistribution, select_compact_scribe_cases, summarize_durable_acks,
};
use wyrd_client::WyrdClient;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{GrpcConfig, HttpConfig};
use wyrd_runtime::{Principal, PrincipalKind, permission::PermissionSet};
use wyrd_spec::auth::PrincipalId;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::SyncQueryRequest;

type BenchError = Box<dyn std::error::Error + Send + Sync>;

#[derive(serde::Serialize)]
struct ScribeReport {
    ack: DurableAckReport,
    preflight_sync_p99_us: u64,
    warmup_durable_rows: u64,
    accepted_rows: u64,
    published_rows: u64,
    queryable_rows: u64,
    max_in_flight: u32,
    topology_verified: bool,
    exact_rows_verified: bool,
}

/// Run one real Scribe scenario and emit only durable ACK samples.
pub async fn run(scenario: BifrostScenario) -> Result<(), BenchError> {
    let scenario = selected_case_scenario(scenario)?;
    let mode = std::env::var("WYRD_BIFROST_BENCH_MODE").unwrap_or_default();
    if mode == "components" {
        return run_components(scenario).await;
    }
    let preflight_sync_p99_us = measure_sync_preflight()?;
    if mode == "preflight" || preflight_sync_p99_us > 3_000 {
        let (negative_flows, accepted_rows, published_rows, queryable_rows) =
            run_qualification_evidence(&scenario).await?;
        let mut envelope = BifrostReportEnvelope::new(
            scenario,
            if preflight_sync_p99_us <= 3_000 {
                BenchmarkReadiness::Ready
            } else {
                BenchmarkReadiness::Unsupported
            },
        );
        if preflight_sync_p99_us > 3_000 {
            envelope.failures.push(format!(
                "WAL sync_data p99 {preflight_sync_p99_us} us exceeds the qualified-volume limit of 3000 us"
            ));
        }
        envelope.negative_flows = negative_flows;
        let report = ScribeReport {
            ack: DurableAckReport {
                envelope,
                durable_samples: 0,
                durable_rows: 0,
                ack_p99_us: 0,
                verified: preflight_sync_p99_us <= 3_000,
            },
            preflight_sync_p99_us,
            warmup_durable_rows: 0,
            accepted_rows,
            published_rows,
            queryable_rows,
            max_in_flight: 0,
            topology_verified: false,
            exact_rows_verified: false,
        };
        emit_report(&report)?;
        return Ok(());
    }
    let persistence_faults = PersistenceFaults::default();
    match scenario.fault_profile {
        BifrostFaultProfile::Retry => {
            persistence_faults.fail_next_object_write();
        }
        BifrostFaultProfile::PostgresStall => {
            persistence_faults.fail_next_sql_commit();
        }
        BifrostFaultProfile::ObjectStoreStall => {
            persistence_faults
                .set_object_write_delay_for_test(std::time::Duration::from_millis(100));
        }
        _ => {}
    }
    let harness = match scenario.fault_profile {
        BifrostFaultProfile::None
        | BifrostFaultProfile::Retry
        | BifrostFaultProfile::MemoryPressure
        | BifrostFaultProfile::WalPressure
        | BifrostFaultProfile::ObjectStoreStall
        | BifrostFaultProfile::PostgresStall
        | BifrostFaultProfile::ConcurrentRoleMemory => {
            BifrostHarness::start_with_faults(
                usize::try_from(scenario.pods)?,
                usize::try_from(scenario.tenants)?,
                std::time::Duration::ZERO,
                persistence_faults,
            )
            .await?
        }
        BifrostFaultProfile::DelayedFsync => {
            BifrostHarness::start_with_wal_sync_delay(
                usize::try_from(scenario.pods)?,
                usize::try_from(scenario.tenants)?,
                std::time::Duration::from_millis(u64::from(scenario.fsync_delay_ms)),
            )
            .await?
        }
    };
    let warmup = run_phase(
        &harness,
        &scenario,
        std::time::Duration::from_secs(u64::from(scenario.warmup_seconds)),
        true,
    )
    .await?;
    let (samples, max_in_flight) = run_phase(
        &harness,
        &scenario,
        std::time::Duration::from_secs(u64::from(scenario.measured_seconds)),
        true,
    )
    .await?;

    let mut negative_flow = run_negative_flow(&harness, &scenario).await?;
    if let Some((check, passed)) = run_fault_profile(&harness, &scenario).await? {
        negative_flow.checks.push(check.to_owned());
        negative_flow.passed &= passed;
    }
    let queryable_rows = query_scribe_rows(&harness, &scenario).await?;
    harness.force_seal_all().await?;
    let drained = harness.is_drained()?;
    let topology_verified = harness.inspection_snapshots()?.into_iter().all(|snapshot| {
        snapshot.shard_task_count == 16
            && snapshot.shard_channel_count == 16
            && snapshot.open_wal_stream_count <= 16
            && snapshot.memory_by_shard.iter().copied().sum::<usize>()
                == snapshot.total_accounted_memory
            && snapshot
                .memory_by_bucket
                .iter()
                .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
                .sum::<usize>()
                == snapshot.memory_by_category[4] + snapshot.memory_by_category[5]
    });
    let warmup_durable_rows = warmup
        .0
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .map(|sample| sample.rows)
        .fold(0, u64::saturating_add);
    let measured_durable_rows = samples
        .iter()
        .filter(|sample| sample.durable && sample.response_id_matches)
        .map(|sample| sample.rows)
        .fold(0, u64::saturating_add);
    let expected_published_rows = warmup_durable_rows
        .saturating_add(measured_durable_rows)
        .saturating_add(u64::from(negative_flow.passed));
    let mut published_rows = 0_u64;
    for tenant in harness.tenants() {
        let rows: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(row_count), 0)::bigint
               FROM vala.file_list
              WHERE data_tenant_id = $1
                AND namespace = 'vala.bifrost'
                AND table_name LIKE 'bifrost_bench_events_%'",
        )
        .bind(tenant.as_uuid())
        .fetch_one(harness.cluster().pg_fixture().operator_pool().pool())
        .await?;
        published_rows = published_rows.saturating_add(u64::try_from(rows).unwrap_or(0));
    }
    let exact_rows_verified =
        published_rows == expected_published_rows && queryable_rows == expected_published_rows;
    let mut ack = summarize_durable_acks(&scenario, &samples);
    ack.envelope.negative_flows = negative_flow;
    if warmup.0.is_empty() && ack.envelope.scenario.warmup_seconds > 0 {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("warmup produced no completed durable ACKs".to_owned());
    }
    if samples.len() < usize::try_from(ack.envelope.scenario.verify_samples)? {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("measured samples did not reach the verification minimum".to_owned());
    }
    if max_in_flight > ack.envelope.scenario.max_in_flight {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("in-flight bound was exceeded".to_owned());
    }
    ack.verified &= drained && topology_verified && exact_rows_verified;
    if !drained {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("harness did not drain writable or pending generations".to_owned());
    }
    if !topology_verified {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("fixed-shard inspection did not reconcile".to_owned());
    }
    if !exact_rows_verified {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("published rows did not match durable ACK rows".to_owned());
    }
    if !ack.envelope.negative_flows.passed {
        ack.envelope.readiness = BenchmarkReadiness::NotReady;
        ack.envelope
            .failures
            .push("required negative-flow checks failed".to_owned());
    }
    let report = ScribeReport {
        ack,
        preflight_sync_p99_us,
        warmup_durable_rows,
        accepted_rows: expected_published_rows,
        published_rows,
        queryable_rows,
        max_in_flight,
        topology_verified,
        exact_rows_verified,
    };
    emit_report(&report)?;
    harness.shutdown().await?;
    if !report.ack.verified || !report.ack.envelope.failures.is_empty() {
        return Err("Scribe benchmark did not prove durable ACK verification".into());
    }
    Ok(())
}

async fn run_qualification_evidence(
    scenario: &BifrostScenario,
) -> Result<(NegativeFlowReport, u64, u64, u64), BenchError> {
    let mut verification = scenario.clone();
    verification.pods = 1;
    verification.tenants = 1;
    verification.tables = 1;
    let harness = BifrostHarness::start(1, 1).await?;
    let negative_flows = run_negative_flow(&harness, &verification).await?;
    let queryable_rows = query_scribe_rows(&harness, &verification).await?;
    harness.force_seal_all().await?;
    let tenant = harness.tenants()[0];
    let published_rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1
            AND namespace = 'vala.bifrost'
            AND table_name = 'bifrost_bench_events_0'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(harness.cluster().pg_fixture().operator_pool().pool())
    .await?;
    harness.shutdown().await?;
    Ok((
        negative_flows,
        1,
        u64::try_from(published_rows).unwrap_or(0),
        queryable_rows,
    ))
}

async fn query_scribe_rows(
    harness: &BifrostHarness,
    scenario: &BifrostScenario,
) -> Result<u64, BenchError> {
    let partition = TimeGranularity::Hour.bucket(chrono::Utc::now())?;
    let mut rows = 0_u64;
    for (tenant_index, tenant) in harness.tenants().iter().copied().enumerate() {
        let scribe = harness
            .scribes()
            .get(tenant_index % harness.scribes().len())
            .ok_or("missing Scribe query owner")?;
        let tail = scribe.tail_service()?;
        for table_index in 0..scenario.tables {
            let binding = TenantTableBinding::resolve((
                tenant,
                TableRef::new(
                    BifrostNamespace::Bifrost,
                    format!("bifrost_bench_events_{table_index}"),
                ),
            ))?;
            rows = rows.saturating_add(
                tail.fetch_hot_batches(FetchLiveTailRequest {
                    binding,
                    target_stream: tail.stream(),
                    start_partition: partition,
                    end_partition: partition,
                    required_columns: vec!["value".to_owned()],
                    predicates: Vec::new(),
                    max_batches: 4_096,
                    max_retained_bytes: 512 * 1024 * 1024,
                })
                .await?
                .iter()
                .map(|batch| u64::try_from(batch.rows.num_rows()).unwrap_or(0))
                .fold(0_u64, u64::saturating_add),
            );
        }
    }
    Ok(rows)
}

fn selected_case_scenario(mut scenario: BifrostScenario) -> Result<BifrostScenario, BenchError> {
    let filter = std::env::var("WYRD_BIFROST_CASES").ok();
    apply_selected_case(&mut scenario, filter.as_deref())?;
    Ok(scenario)
}

fn apply_selected_case(
    scenario: &mut BifrostScenario,
    filter: Option<&str>,
) -> Result<(), BenchError> {
    let Some(filter) = filter.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };
    let cases = select_compact_scribe_cases(Some(filter))?;
    if cases.len() != 1 {
        return Err("Scribe benchmark adapters require exactly one WYRD_BIFROST_CASES case".into());
    }
    let case = &cases[0];
    scenario.case_id = Some(case.id.clone());
    scenario.pods = case.pods;
    scenario.tenants = case.tenants;
    scenario.tables = case.tables;
    scenario.max_in_flight = case.producers;
    scenario.items_per_request = u32::try_from(
        u64::from(case.producers)
            .max(case.frame_size_bytes.saturating_div(64))
            .max(1),
    )?;
    scenario.verify_samples = u32::try_from(case.minimum_samples)?;
    scenario.fsync_delay_ms = u32::try_from(case.fsync_delay_ms)?;
    scenario.fault_profile = case.fault_profile;
    scenario.validate()?;
    Ok(())
}

async fn run_negative_flow(
    harness: &BifrostHarness,
    scenario: &BifrostScenario,
) -> Result<NegativeFlowReport, BenchError> {
    if !scenario.require_negative_flows {
        return Ok(NegativeFlowReport::skipped());
    }
    let tenant = *harness
        .tenants()
        .first()
        .ok_or("missing Scribe benchmark tenant")?;
    let scribe = harness
        .scribes()
        .first()
        .cloned()
        .ok_or("missing Scribe benchmark owner")?;
    let batch_id = uuid::Uuid::now_v7();
    let rows = make_batch(1, u64::MAX)?;
    let table = TableRef::new(BifrostNamespace::Bifrost, "bifrost_bench_events_0");
    let first = append_one(
        Arc::clone(&scribe),
        tenant,
        table.clone(),
        rows.clone(),
        batch_id,
    )
    .await?;
    let second = append_one(scribe, tenant, table, rows, batch_id).await?;
    let passed = first.durable
        && second.durable
        && first.response_id_matches
        && second.response_id_matches
        && first.rows == second.rows
        && first.rows == 1;
    Ok(NegativeFlowReport::executed(
        ["duplicate_batch_id_is_idempotent"],
        passed,
    ))
}

async fn run_fault_profile(
    harness: &BifrostHarness,
    scenario: &BifrostScenario,
) -> Result<Option<(&'static str, bool)>, BenchError> {
    let Some(scribe) = harness.scribes().first() else {
        return Err("missing Scribe fault-profile owner".into());
    };
    let tenant = harness.tenants()[0];
    let table = TableRef::new(BifrostNamespace::Bifrost, "bifrost_bench_events_0");
    let result = match scenario.fault_profile {
        BifrostFaultProfile::None | BifrostFaultProfile::DelayedFsync => return Ok(None),
        BifrostFaultProfile::MemoryPressure => {
            let roles = harness
                .cluster()
                .server(0)
                .and_then(|server| server.state().bifrost_resources().cloned())
                .ok_or("missing shared Bifrost resource composition")?;
            let pressure = roles
                .oracle()
                .ok_or("missing Oracle resource capability")?
                .try_acquire_worker(vala_bifrost_redux::resources::OracleWorkerClass::Analytical)?;
            let rejected = !append_one(
                Arc::clone(scribe),
                tenant,
                table.clone(),
                make_batch(1, u64::MAX - 1)?,
                uuid::Uuid::now_v7(),
            )
            .await?
            .durable;
            drop(pressure);
            let recovered = append_one(
                Arc::clone(scribe),
                tenant,
                table,
                make_batch(1, u64::MAX - 2)?,
                uuid::Uuid::now_v7(),
            )
            .await?
            .durable;
            (
                "memory_pressure_rejects_and_recovers",
                rejected && recovered,
            )
        }
        BifrostFaultProfile::WalPressure => {
            scribe.trip_wal_disk_full_for_test();
            let rejected = !append_one(
                Arc::clone(scribe),
                tenant,
                table,
                make_batch(1, u64::MAX - 3)?,
                uuid::Uuid::now_v7(),
            )
            .await?
            .durable;
            ("wal_pressure_rejects_before_append", rejected)
        }
        BifrostFaultProfile::ConcurrentRoleMemory => {
            let roles = harness
                .cluster()
                .server(0)
                .and_then(|server| server.state().bifrost_resources().cloned())
                .ok_or("missing shared Bifrost resource composition")?;
            let oracle = roles
                .oracle()
                .ok_or("missing Oracle resource capability")?
                .try_acquire_worker(
                    vala_bifrost_redux::resources::OracleWorkerClass::Interactive,
                )?;
            let snapshot = roles.snapshot()?;
            let passed = snapshot.oracle_memory_used_bytes == oracle.memory_bytes();
            drop(oracle);
            ("concurrent_roles_share_parent", passed)
        }
        BifrostFaultProfile::Retry
        | BifrostFaultProfile::ObjectStoreStall
        | BifrostFaultProfile::PostgresStall => {
            let started = Instant::now();
            for owner in harness.scribes() {
                owner.flush_writable_for_test().await?;
            }
            let deadline = Instant::now() + std::time::Duration::from_secs(10);
            while Instant::now() < deadline && !harness.is_drained()? {
                for owner in harness.scribes() {
                    owner.check_age(Instant::now() + std::time::Duration::from_secs(120));
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            let drained = harness.is_drained()?;
            let passed = drained
                && (scenario.fault_profile != BifrostFaultProfile::ObjectStoreStall
                    || started.elapsed() >= std::time::Duration::from_millis(100));
            (
                match scenario.fault_profile {
                    BifrostFaultProfile::Retry => "failed_persistence_retries",
                    BifrostFaultProfile::ObjectStoreStall => "object_store_stall_is_bounded",
                    BifrostFaultProfile::PostgresStall => "postgres_stall_retries",
                    _ => unreachable!("matched persistence fault profile"),
                },
                passed,
            )
        }
    };
    Ok(Some(result))
}

async fn run_phase(
    harness: &BifrostHarness,
    scenario: &BifrostScenario,
    duration: std::time::Duration,
    collect_samples: bool,
) -> Result<(Vec<DurableAckSample>, u32), BenchError> {
    if duration.is_zero() {
        return Ok((Vec::new(), 0));
    }
    let request_period = std::time::Duration::from_nanos(
        (1_000_000_000_u128
            .saturating_mul(u128::from(scenario.items_per_request))
            .div_ceil(u128::from(scenario.target_items_per_second)))
        .max(1)
        .try_into()?,
    );
    let mut interval = tokio::time::interval(request_period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or("Scribe benchmark deadline overflow")?;
    let mut sequence = 0_u64;
    let mut tasks = JoinSet::new();
    let mut samples = Vec::new();
    let mut max_in_flight = 0_u32;

    while Instant::now() < deadline {
        interval.tick().await;
        if Instant::now() >= deadline {
            break;
        }
        while let Some(result) = tasks.try_join_next() {
            let sample = result??;
            if collect_samples {
                samples.push(sample);
            }
        }
        if tasks.len() >= usize::try_from(scenario.max_in_flight)?
            && let Some(result) = tasks.join_next().await
        {
            let sample = result??;
            if collect_samples {
                samples.push(sample);
            }
        }
        let tenant_index = usize::try_from(sequence)? % harness.tenants().len();
        let tenant = harness.tenants()[tenant_index];
        let table = TableRef::new(
            BifrostNamespace::Bifrost,
            format!(
                "bifrost_bench_events_{}",
                sequence % u64::from(scenario.tables)
            ),
        );
        let scribe = Arc::clone(
            harness
                .scribes()
                .get(tenant_index % harness.scribes().len())
                .ok_or("Scribe benchmark has no Scribe owner")?,
        );
        let rows = make_batch(usize::try_from(scenario.items_per_request)?, sequence)?;
        tasks.spawn(append_one(
            scribe,
            tenant,
            table,
            rows,
            uuid::Uuid::now_v7(),
        ));
        max_in_flight = max_in_flight.max(u32::try_from(tasks.len())?);
        sequence = sequence.wrapping_add(1);
    }
    while let Some(result) = tasks.join_next().await {
        let sample = result??;
        if collect_samples {
            samples.push(sample);
        }
    }
    Ok((samples, max_in_flight))
}

/// The user-visible schema identity the benchmark batch carries.
///
/// `make_batch` also stamps `wyrd_event_time`, which is server-managed and
/// therefore excluded from a table's schema identity. The benchmark hands
/// Scribe already-projected Arrow, so it must state the same projected
/// fingerprint a real client's IPC stream would.
fn bench_source_fingerprint() -> SchemaFingerprint {
    SchemaFingerprint::from_arrow_schema(&Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]))
}

/// Builds the server-created audit event Gate owns for one benchmark frame.
///
/// `Scribe::ingest_frame` never mints an audit record of its own, so the
/// benchmark supplies the allow/success event an authenticated write carries.
fn bench_audit_event(
    principal: &Principal,
    table: &TableRef,
    request_id: &RequestId,
    rows: usize,
) -> wyrd_spec::vala::api::AuditEvent {
    wyrd_spec::vala::api::AuditEvent {
        request_id: request_id.clone(),
        trace_id: None,
        operation: "bifrost.append".to_owned(),
        resource: table.fqn(),
        card_ref: principal.card_ref().cloned(),
        principal_id: principal.id,
        principal_kind: principal.kind.tag(),
        auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
        permission: "bifrost:append".to_owned(),
        decision: wyrd_spec::vala::api::AuditDecision::Allow,
        result: wyrd_spec::vala::api::AuditResult::Success,
        payload_summary: format!("{rows} rows"),
        detail: None,
    }
}

async fn append_one(
    scribe: Arc<vala_bifrost_redux::scribe::ScribeImpl>,
    tenant: wyrd_spec::ids::DataTenantId,
    table: TableRef,
    rows: RecordBatch,
    batch_id: uuid::Uuid,
) -> Result<DurableAckSample, BenchError> {
    let principal = Principal {
        id: PrincipalId::new(uuid::Uuid::now_v7()),
        kind: PrincipalKind::User,
        tenant_id: tenant,
        roles: Vec::new(),
        effective_permissions: PermissionSet::new(),
    };
    let request_id = RequestId::now_v7();
    let audit_event = bench_audit_event(&principal, &table, &request_id, rows.num_rows());
    let started = Instant::now();
    let result = Scribe::ingest_frame(
        scribe.as_ref(),
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            principal,
            table,
            expected_schema_fingerprint: Some(bench_source_fingerprint()),
            request_id,
            batch_id,
            audit_event,
            measured_wire_bytes: 0,
            payload: IngressPayload::ProjectedArrow(vec![rows]),
        },
    )
    .await;
    let latency_us = u64::try_from(started.elapsed().as_micros())?;
    match result {
        Ok(ack) => Ok(DurableAckSample {
            response_id_matches: ack.batch_id == batch_id,
            durable: true,
            latency_us,
            rows: ack.rows_accepted,
        }),
        Err(_) => Ok(DurableAckSample {
            response_id_matches: false,
            durable: false,
            latency_us,
            rows: 0,
        }),
    }
}

const COMPONENT_WARMUP: usize = 100;
const COMPONENT_SAMPLES: usize = 1_000;

fn measure_sync_preflight() -> Result<u64, BenchError> {
    let temp_dir = tempfile::tempdir()?;
    let tenant = wyrd_spec::DataTenantId::new_v7();
    let support = WalBenchSupport::new(temp_dir.path(), tenant)?;
    let audit = b"{}";
    let data = vec![b'x'; 64];
    for index in 0..COMPONENT_WARMUP {
        let fixture = support.prepare(batch_id(index), audit, &data)?;
        let _ = support.append_and_sync(fixture)?;
    }
    let mut samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in COMPONENT_WARMUP..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = support.prepare(batch_id(index), audit, &data)?;
        let _ = support.append_no_sync(fixture)?;
        let started = Instant::now();
        support.sync_alone()?;
        samples.push(elapsed_us(started));
    }
    samples.sort_unstable();
    Ok(percentile(&samples, 99))
}

#[derive(serde::Serialize)]
struct ScribeComponentsReport {
    envelope: BifrostReportEnvelope,
    components: Vec<ScribeComponentReport>,
    topology_verified: bool,
    exact_rows_verified: bool,
    accepted_rows: u64,
    published_rows: i64,
    queryable_rows: u64,
    fault_cases: Vec<FaultCaseEvidence>,
}

#[derive(serde::Serialize)]
struct FaultCaseEvidence {
    case_id: String,
    fault_profile: BifrostFaultProfile,
    executed: bool,
    passed: bool,
}

async fn run_components(scenario: BifrostScenario) -> Result<(), BenchError> {
    let harness = match scenario.fault_profile {
        BifrostFaultProfile::None
        | BifrostFaultProfile::Retry
        | BifrostFaultProfile::MemoryPressure
        | BifrostFaultProfile::WalPressure
        | BifrostFaultProfile::ObjectStoreStall
        | BifrostFaultProfile::PostgresStall
        | BifrostFaultProfile::ConcurrentRoleMemory => BifrostHarness::start(1, 1).await?,
        BifrostFaultProfile::DelayedFsync => {
            BifrostHarness::start_with_wal_sync_delay(
                1,
                1,
                std::time::Duration::from_millis(u64::from(scenario.fsync_delay_ms)),
            )
            .await?
        }
    };
    let server = harness
        .cluster()
        .server(0)
        .ok_or("missing benchmark server")?;
    let (public_components, duplicate_response_observed, queryable_rows) =
        measure_public_components(
            &harness,
            harness.tenants()[0],
            scenario.require_negative_flows,
        )
        .await?;
    let components = measure_wal_components()?
        .into_iter()
        .chain(public_components)
        .collect::<Vec<_>>();

    server.flush_bifrost().await?;
    let server_snapshot = server.scribe_inspection_snapshot()?;
    let topology_verified = server_snapshot.shard_task_count == 16
        && server_snapshot.shard_channel_count == 16
        && server_snapshot.open_wal_stream_count <= 16
        && server_snapshot.memory_by_shard.iter().sum::<usize>()
            == server_snapshot.total_accounted_memory
        && server_snapshot
            .memory_by_bucket
            .iter()
            .map(|bucket| bucket.writable_bytes + bucket.immutable_bytes)
            .sum::<usize>()
            == server_snapshot.memory_by_category[4] + server_snapshot.memory_by_category[5];
    let tenant = harness.tenants()[0];
    let mut conn = harness.tenant_conn(tenant).await?;
    let published_rows: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(row_count), 0)::bigint
           FROM vala.file_list
          WHERE data_tenant_id = $1
            AND namespace = 'vala.bifrost'
            AND table_name = 'bifrost_component_events'",
    )
    .bind(tenant.as_uuid())
    .fetch_one(&mut **conn.transaction())
    .await?;
    conn.commit().await?;
    let accepted_rows = 2_100_u64;
    let exact_rows_verified =
        component_rows_reconcile(accepted_rows, published_rows, queryable_rows);
    let negative_flow_passed = duplicate_response_observed && exact_rows_verified;
    let fault_cases = verify_fault_cases().await?;
    let fault_cases_passed = fault_cases.iter().all(|case| case.executed && case.passed);
    let mut envelope = BifrostReportEnvelope::new(scenario, BenchmarkReadiness::Ready);
    envelope.negative_flows = if envelope.scenario.require_negative_flows {
        NegativeFlowReport::executed(["duplicate_batch_id_is_idempotent"], negative_flow_passed)
    } else {
        NegativeFlowReport::skipped()
    };
    if !topology_verified
        || !exact_rows_verified
        || !envelope.negative_flows.passed
        || !fault_cases_passed
    {
        envelope.readiness = BenchmarkReadiness::NotReady;
        envelope
            .failures
            .push("component harness did not drain or reconcile topology".to_owned());
    }
    let report = ScribeComponentsReport {
        envelope,
        components,
        topology_verified,
        exact_rows_verified,
        accepted_rows,
        published_rows,
        queryable_rows,
        fault_cases,
    };
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path("components.json"),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    harness.shutdown().await?;
    if report.envelope.readiness != BenchmarkReadiness::Ready {
        return Err("Scribe component benchmark verification failed".into());
    }
    Ok(())
}

async fn verify_fault_cases() -> Result<Vec<FaultCaseEvidence>, BenchError> {
    let cases = wyrd_bench::compact_scribe_matrix()
        .into_iter()
        .filter(|case| case.fault_profile != BifrostFaultProfile::None)
        .collect::<Vec<_>>();
    let mut evidence = Vec::with_capacity(cases.len());
    for case in cases {
        let faults = PersistenceFaults::default();
        match case.fault_profile {
            BifrostFaultProfile::Retry => faults.fail_next_object_write(),
            BifrostFaultProfile::PostgresStall => faults.fail_next_sql_commit(),
            BifrostFaultProfile::ObjectStoreStall => {
                faults.set_object_write_delay_for_test(std::time::Duration::from_millis(100));
            }
            _ => {}
        }
        let delay = if case.fault_profile == BifrostFaultProfile::DelayedFsync {
            std::time::Duration::from_millis(case.fsync_delay_ms)
        } else {
            std::time::Duration::ZERO
        };
        let harness = BifrostHarness::start_with_faults(1, 1, delay, faults).await?;
        let scenario = BifrostScenario {
            case_id: Some(case.id.clone()),
            fault_profile: case.fault_profile,
            fsync_delay_ms: u32::try_from(case.fsync_delay_ms)?,
            ..BifrostScenario::smoke(wyrd_bench::BifrostLane::Scribe)
        };
        let base = append_one(
            Arc::clone(&harness.scribes()[0]),
            harness.tenants()[0],
            TableRef::new(BifrostNamespace::Bifrost, "bifrost_bench_events_0"),
            make_batch(1, 42)?,
            uuid::Uuid::now_v7(),
        )
        .await?;
        let passed = if case.fault_profile == BifrostFaultProfile::DelayedFsync {
            base.durable && base.latency_us >= case.fsync_delay_ms.saturating_mul(1_000)
        } else {
            run_fault_profile(&harness, &scenario)
                .await?
                .is_some_and(|(_, passed)| passed)
        };
        harness.shutdown().await?;
        evidence.push(FaultCaseEvidence {
            case_id: case.id,
            fault_profile: case.fault_profile,
            executed: true,
            passed,
        });
    }
    Ok(evidence)
}

fn component_rows_reconcile(accepted_rows: u64, published_rows: i64, queryable_rows: u64) -> bool {
    published_rows >= 0
        && u64::try_from(published_rows).unwrap_or(0) == accepted_rows
        && queryable_rows == accepted_rows
}

fn measure_wal_components() -> Result<Vec<ScribeComponentReport>, BenchError> {
    let tenant = wyrd_spec::DataTenantId::new_v7();
    let audit = b"{}";
    let data = vec![b'x'; 64];
    let fixture_bytes = u64::try_from(audit.len() + data.len())?;

    let prepare_dir = tempfile::tempdir()?;
    let prepare = WalBenchSupport::new(prepare_dir.path(), tenant)?;
    let mut prepare_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let started = Instant::now();
        let fixture = prepare.prepare(batch_id(index), audit, &data)?;
        let _ = fixture.crc32();
        if index >= COMPONENT_WARMUP {
            prepare_samples.push(elapsed_us(started));
        }
    }

    let append_dir = tempfile::tempdir()?;
    let append = WalBenchSupport::new(append_dir.path(), tenant)?;
    let mut append_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut append_bytes = 0_u64;
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = append.prepare(batch_id(index), audit, &data)?;
        let started = Instant::now();
        let evidence = append.append_no_sync(fixture)?;
        if index >= COMPONENT_WARMUP {
            append_samples.push(elapsed_us(started));
            append_bytes = append_bytes.saturating_add(evidence.bytes);
        }
    }
    append.sync_alone()?;

    let sync_dir = tempfile::tempdir()?;
    let sync = WalBenchSupport::new(sync_dir.path(), tenant)?;
    let mut sync_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    for index in 0..COMPONENT_SAMPLES {
        let fixture = sync.prepare(batch_id(index), audit, &data)?;
        let _ = sync.append_no_sync(fixture)?;
        let started = Instant::now();
        sync.sync_alone()?;
        sync_samples.push(elapsed_us(started));
    }

    let one_dir = tempfile::tempdir()?;
    let one = WalBenchSupport::new(one_dir.path(), tenant)?;
    let mut one_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut one_bytes = 0_u64;
    for index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixture = one.prepare(batch_id(index), audit, &data)?;
        let started = Instant::now();
        let evidence = one.append_and_sync(fixture)?;
        if index >= COMPONENT_WARMUP {
            one_samples.push(elapsed_us(started));
            one_bytes = one_bytes.saturating_add(evidence.bytes);
        }
    }

    let group_dir = tempfile::tempdir()?;
    let group = WalBenchSupport::new(group_dir.path(), tenant)?;
    let mut group_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let mut group_bytes = 0_u64;
    for group_index in 0..(COMPONENT_WARMUP + COMPONENT_SAMPLES) {
        let fixtures = (0..64_usize)
            .map(|frame| {
                group.prepare(
                    batch_id(group_index.saturating_mul(64).saturating_add(frame)),
                    audit,
                    &data,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let started = Instant::now();
        let evidence = group.append_group_one_sync(fixtures)?;
        if evidence.frames != 64 || evidence.fsyncs != 1 {
            return Err("64-frame WAL group did not issue one sync".into());
        }
        if group_index >= COMPONENT_WARMUP {
            group_samples.push(elapsed_us(started));
            group_bytes = group_bytes.saturating_add(evidence.bytes);
        }
    }

    Ok(vec![
        component_report(
            "wal_prepare_crc_no_io",
            "production WAL preparation",
            fixture_bytes,
            prepare_samples,
            fixture_bytes.saturating_mul(COMPONENT_SAMPLES as u64),
        ),
        component_report(
            "wal_append_no_sync",
            "production WAL append before sync_data",
            fixture_bytes,
            append_samples,
            append_bytes,
        ),
        component_report(
            "sync_alone",
            "production WAL sync_data",
            fixture_bytes,
            sync_samples,
            sync.bytes_on_disk(),
        ),
        component_report(
            "one_frame_append_sync",
            "production one-frame append plus sync",
            fixture_bytes,
            one_samples,
            one_bytes,
        ),
        component_report(
            "64_frame_append_one_sync",
            "production 64-frame grouped append plus sync",
            fixture_bytes.saturating_mul(64),
            group_samples,
            group_bytes,
        ),
    ])
}

async fn measure_public_components(
    harness: &BifrostHarness,
    tenant: wyrd_spec::DataTenantId,
    require_negative_flows: bool,
) -> Result<(Vec<ScribeComponentReport>, bool, u64), BenchError> {
    let server = harness
        .cluster()
        .server(0)
        .ok_or("missing benchmark server")?;
    let table_name = "bifrost_component_events";
    let table = TableRef::new(BifrostNamespace::Bifrost, table_name);
    let catalog = server
        .state()
        .bifrost_catalog()
        .expect("benchmark server exposes its Bifrost catalog");
    catalog
        .create_table(vala_bifrost_redux::catalog::CreateTableRequest {
            table,
            user_fields: vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Utf8, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await?;
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, "bifrost-component-writer", &["admin"])
        .await?;
    let api_key = match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key,
        Bootstrap::User { .. } => return Err("component bootstrap returned a user".into()),
    };
    let client = WyrdClient::with_config(ClientConfig {
        grpc: GrpcConfig {
            endpoint: server.grpc_url().ok_or("missing benchmark gRPC URL")?,
            connect_retries: 0,
            ..GrpcConfig::default()
        },
        http: HttpConfig {
            base_url: server
                .base_url()
                .ok_or("missing benchmark HTTP URL")?
                .to_owned(),
            ..HttpConfig::default()
        },
        api_key: Some(api_key),
        ..ClientConfig::default()
    })?;
    let transport = BifrostGrpcTransport::connect(&client).await?;
    let payload = component_ipc()?;
    let fqn = format!("vala.bifrost.{table_name}");
    let duplicate_batch_id = uuid::Uuid::now_v7().into_bytes();
    let mut duplicate_response_observed = !require_negative_flows;
    for index in 0..COMPONENT_WARMUP {
        let batch_id = if index == 0 {
            duplicate_batch_id
        } else {
            uuid::Uuid::now_v7().into_bytes()
        };
        transport
            .send_frame(BifrostFrame {
                table: fqn.clone(),
                batch_id,
                arrow_ipc: payload.clone(),
            })
            .await?;
        if index == 0 && require_negative_flows {
            duplicate_response_observed = transport
                .send_frame(BifrostFrame {
                    table: fqn.clone(),
                    batch_id: duplicate_batch_id,
                    arrow_ipc: payload.clone(),
                })
                .await
                .is_ok();
        }
        let _ = index;
    }
    let mut gate_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let started = Instant::now();
    for _ in 0..COMPONENT_SAMPLES {
        let sample_started = Instant::now();
        transport
            .send_frame(BifrostFrame {
                table: fqn.clone(),
                batch_id: uuid::Uuid::now_v7().into_bytes(),
                arrow_ipc: payload.clone(),
            })
            .await?;
        gate_samples.push(elapsed_us(sample_started));
    }
    let gate_elapsed = started.elapsed();

    let mut sdk_samples = Vec::with_capacity(COMPONENT_SAMPLES);
    let started = Instant::now();
    for _ in 0..COMPONENT_SAMPLES {
        let sample_started = Instant::now();
        let frame = BifrostFrame {
            table: fqn.clone(),
            batch_id: uuid::Uuid::now_v7().into_bytes(),
            arrow_ipc: payload.clone(),
        };
        transport.send_frame(frame).await?;
        sdk_samples.push(elapsed_us(sample_started));
    }
    let sdk_elapsed = started.elapsed();
    let bytes = u64::try_from(payload.len())?.saturating_mul(COMPONENT_SAMPLES as u64);
    server.flush_bifrost().await?;
    let queryable_rows = client
        .request_arrow(
            reqwest::Method::POST,
            "/v1/query",
            Some(&SyncQueryRequest {
                sql: format!("SELECT id FROM \"{fqn}\""),
                params: Vec::new(),
            }),
        )
        .await?
        .row_count
        .unwrap_or_default();
    Ok((
        vec![
            component_report_with_elapsed(
                "gate_ack",
                "unary Gate to Scribe durable ACK",
                u64::try_from(payload.len())?,
                gate_samples,
                bytes,
                gate_elapsed,
            ),
            component_report_with_elapsed(
                "sdk_gate_scribe",
                "public SDK transport through Gate to Scribe",
                u64::try_from(payload.len())?,
                sdk_samples,
                bytes,
                sdk_elapsed,
            ),
        ],
        duplicate_response_observed,
        queryable_rows,
    ))
}

fn component_ipc() -> Result<Bytes, BenchError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64])),
            Arc::new(StringArray::from(vec!["component"])),
        ],
    )?;
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref())?;
    writer.write(&batch)?;
    writer.finish()?;
    Ok(Bytes::from(bytes))
}

fn batch_id(index: usize) -> [u8; 16] {
    let mut id = [0_u8; 16];
    id[..8].copy_from_slice(&(index as u64).to_le_bytes());
    id
}

fn component_report(
    name: &str,
    path: &str,
    fixture_bytes: u64,
    samples: Vec<u64>,
    bytes: u64,
) -> ScribeComponentReport {
    let elapsed_us = samples.iter().copied().sum::<u64>().max(1);
    component_report_with_elapsed(
        name,
        path,
        fixture_bytes,
        samples,
        bytes,
        std::time::Duration::from_micros(elapsed_us),
    )
}

fn component_report_with_elapsed(
    name: &str,
    path: &str,
    fixture_bytes: u64,
    mut samples: Vec<u64>,
    bytes: u64,
    elapsed: std::time::Duration,
) -> ScribeComponentReport {
    samples.sort_unstable();
    let operations = samples.len() as u64;
    let elapsed_us = u64::try_from(elapsed.as_micros())
        .unwrap_or(u64::MAX)
        .max(1);
    let seconds = elapsed_us as f64 / 1_000_000.0;
    ScribeComponentReport {
        name: name.to_owned(),
        path: path.to_owned(),
        provenance: "current Bifrost production seam".to_owned(),
        warmup_operations: COMPONENT_WARMUP as u64,
        fixture_bytes,
        operations,
        bytes,
        elapsed_us,
        operations_per_second: operations as f64 / seconds,
        mib_per_second: bytes as f64 / seconds / 1_048_576.0,
        distribution: ScribeDistribution {
            count: operations,
            p50: percentile(&samples, 50),
            p95: percentile(&samples, 95),
            p99: (operations >= 100).then(|| percentile(&samples, 99)),
            max: samples.last().copied().unwrap_or(0),
        },
    }
}

fn percentile(sorted: &[u64], percentile: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len().saturating_mul(percentile).saturating_add(99) / 100)
        .max(1)
        .min(sorted.len());
    sorted[rank - 1]
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
}

fn make_batch(rows: usize, sequence: u64) -> Result<RecordBatch, arrow::error::ArrowError> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", DataType::Int64, false),
        Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".to_owned().into())),
            false,
        ),
    ]));
    let now = chrono::Utc::now().timestamp_micros();
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from(
                (0..rows)
                    .map(|index| {
                        i64::try_from(sequence)
                            .unwrap_or(i64::MAX)
                            .saturating_mul(i64::try_from(rows).unwrap_or(i64::MAX))
                            .saturating_add(i64::try_from(index).unwrap_or(i64::MAX))
                    })
                    .collect::<Vec<_>>(),
            )),
            Arc::new(
                TimestampMicrosecondArray::from(
                    (0..rows)
                        .map(|index| now.saturating_add(i64::try_from(index).unwrap_or(i64::MAX)))
                        .collect::<Vec<_>>(),
                )
                .with_timezone("UTC"),
            ),
        ],
    )
}

fn emit_report(report: &ScribeReport) -> Result<(), BenchError> {
    let path = std::env::var_os("WYRD_BIFROST_REPORT").map_or_else(
        || repository_report_path("scribe.json"),
        std::path::PathBuf::from,
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(report)?))?;
    Ok(())
}

fn repository_report_path(file_name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .join("target/bifrost-benchmarks/production-readiness")
        .join(file_name)
}

#[cfg(test)]
mod tests {
    use super::{BifrostFaultProfile, BifrostScenario, apply_selected_case};
    use wyrd_bench::BifrostLane;

    #[test]
    fn scribe_scenarios_consume_every_declared_field() {
        let mut scenario = BifrostScenario::smoke(BifrostLane::Scribe);
        apply_selected_case(&mut scenario, Some("64k-w64-t10-p3-tbl8-dispersed"))
            .expect("selected Scribe case");
        assert_eq!(
            scenario.case_id.as_deref(),
            Some("64k-w64-t10-p3-tbl8-dispersed")
        );
        assert_eq!(scenario.pods, 3);
        assert_eq!(scenario.tenants, 10);
        assert_eq!(scenario.tables, 8);
        assert_eq!(scenario.max_in_flight, 64);
        assert_eq!(scenario.items_per_request, 1_024);
        assert_eq!(scenario.verify_samples, 1_000);
        assert_eq!(scenario.fault_profile, BifrostFaultProfile::None);
        assert_eq!(scenario.fsync_delay_ms, 0);

        for (case, profile) in [
            ("retry", BifrostFaultProfile::Retry),
            ("memory-pressure", BifrostFaultProfile::MemoryPressure),
            ("wal-pressure", BifrostFaultProfile::WalPressure),
            ("object-store-stall", BifrostFaultProfile::ObjectStoreStall),
            ("postgres-stall", BifrostFaultProfile::PostgresStall),
            (
                "concurrent-role-memory",
                BifrostFaultProfile::ConcurrentRoleMemory,
            ),
        ] {
            let mut scenario = BifrostScenario::smoke(BifrostLane::Scribe);
            apply_selected_case(&mut scenario, Some(case)).expect("selected fault case");
            assert_eq!(scenario.fault_profile, profile);
            assert_eq!(scenario.case_id.as_deref(), Some(case));
        }
    }

    #[test]
    fn component_report_reconciles_accepted_published_and_queryable_rows() {
        assert!(super::component_rows_reconcile(2_100, 2_100, 2_100));
        assert!(!super::component_rows_reconcile(2_100, 2_099, 2_100));
        assert!(!super::component_rows_reconcile(2_100, 2_100, 2_099));
    }
}
