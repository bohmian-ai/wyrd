//! Scribe's production telemetry tells the same story as its durable owners.
//!
//! A metric or span on its own is not evidence that the work it names
//! happened, so this module never asserts telemetry in isolation: each
//! production metric delta and captured trace is checked against a fact the
//! same run establishes independently — client receipts, the staging and
//! memtable owners, committed hot files, and the rows a public strict read
//! returns.

use std::collections::{BTreeMap, BTreeSet};

use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_telemetry::CapturedSpan;
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::telemetry::{
    BifrostMetricKind, BifrostMetricSample, BifrostTelemetryCapture, BifrostTelemetryDelta,
};
use wyrd_testing::bifrost::{
    BifrostClusterSpec, WyrdTestCluster, shared_process_telemetry_for_test,
};

use super::support::{
    append_values, await_persistence_drained, published_rows, register_table, sorted_values,
    tenant_client, unique_table,
};

/// Longest a settled pod may take to retire the members its publication replaced.
///
/// Retirement waits for a published member's readers to drain, so it is not
/// complete at the instant `flush_bifrost` returns. The deadline turns a stuck
/// settlement into a diagnosable failure; nothing here measures elapsed time as
/// evidence.
const SETTLEMENT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

/// Staged-backlog gauges the staging owner publishes, all pod aggregates.
const STAGING_GAUGES: [&str; 4] = [
    "bifrost_scribe_staging_live_members",
    "bifrost_scribe_staging_live_bytes",
    "bifrost_scribe_staging_oldest_member_timestamp_seconds",
    "bifrost_scribe_staging_outstanding_claims",
];

/// Write-path families the write, retry, and failure windows print as evidence.
const WRITE_FAMILIES: [&str; 11] = [
    "bifrost_gate_requests_total",
    "bifrost_gate_request_duration_seconds",
    "bifrost_gate_frames_total",
    "bifrost_gate_frame_bytes_total",
    "bifrost_gate_rejections_total",
    "bifrost_scribe_ack_seconds",
    "bifrost_scribe_wal_append_total",
    "bifrost_scribe_wal_append_bytes_total",
    "bifrost_scribe_wal_fsync_total",
    "bifrost_scribe_memtable_rows_inserted_total",
    "bifrost_scribe_rejections_total",
];

/// Memory and lane families the freeze window prints as evidence.
const MEMORY_FAMILIES: [&str; 5] = [
    "bifrost_scribe_active_memtable_bytes",
    "bifrost_scribe_immutable_memtable_bytes",
    "bifrost_scribe_immutable_generation_count",
    "bifrost_scribe_lane_queued",
    "bifrost_scribe_lane_active",
];

/// Staging and publication families the publish window prints as evidence.
const PUBLICATION_FAMILIES: [&str; 8] = [
    "bifrost_scribe_staging_live_members",
    "bifrost_scribe_staging_live_bytes",
    "bifrost_scribe_staging_oldest_member_timestamp_seconds",
    "bifrost_scribe_staging_outstanding_claims",
    "bifrost_scribe_staging_claims_published_total",
    "bifrost_scribe_publication_files_total",
    "bifrost_scribe_publication_bytes_total",
    "bifrost_scribe_seal_failed_total",
];

/// Prints every captured span of one trace with its parentage and outcome.
fn print_trace(delta: &BifrostTelemetryDelta, trace_id: &str, story: &str) {
    for span in delta.spans.iter().filter(|span| span.trace_id == trace_id) {
        let levels: Vec<_> = span
            .events
            .iter()
            .filter_map(|event| event.attributes.get("level"))
            .collect();
        eprintln!(
            "evidence trace story={story} trace={} span={} parent={} name={} duration_nanos={} \
             outcome={:?} batch_id={:?} event_levels={levels:?}",
            span.trace_id,
            span.span_id,
            span.parent_span_id,
            span.name,
            span.duration_nanos,
            attribute(span, "outcome"),
            attribute(span, "batch_id"),
        );
    }
}

/// Prints every Bifrost and Oracle family the production exposition holds.
///
/// One line per family with its series count, so the run's output is an
/// inventory of what an operator's scrape actually sees.
///
/// # Panics
///
/// Panics when the production recorder cannot render.
fn print_family_inventory(telemetry: &BifrostTelemetryCapture) {
    let mut families = BTreeMap::<String, usize>::new();
    for sample in telemetry.snapshot().expect("production metrics render") {
        if sample.kind != BifrostMetricKind::HistogramBucket {
            *families.entry(sample.family).or_default() += 1;
        }
    }
    for (family, series) in families {
        eprintln!("evidence inventory family={family} series={series}");
    }
}

/// Sums one family's window delta over every label set of one sample kind.
fn delta_value(delta: &BifrostTelemetryDelta, family: &str, kind: BifrostMetricKind) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| sample.family == family && sample.kind == kind)
        .map(|sample| sample.value)
        .sum()
}

/// Sums one counter family's window delta for one exact label value.
fn labelled_delta(delta: &BifrostTelemetryDelta, family: &str, key: &str, value: &str) -> f64 {
    delta
        .metrics
        .iter()
        .filter(|sample| {
            sample.family == family && sample.labels.get(key).map(String::as_str) == Some(value)
        })
        .map(|sample| sample.value)
        .sum()
}

/// Reads one unlabeled gauge from an absolute production snapshot.
fn gauge(samples: &[BifrostMetricSample], family: &str) -> Option<f64> {
    samples
        .iter()
        .find(|sample| {
            sample.family == family
                && sample.kind == BifrostMetricKind::Gauge
                && sample.labels.is_empty()
        })
        .map(|sample| sample.value)
}

/// Reads one scrubbed span attribute.
fn attribute<'a>(span: &'a CapturedSpan, key: &str) -> Option<&'a str> {
    span.attributes.get(key).map(String::as_str)
}

/// Returns the spans of one exact production operation name.
fn spans_named<'a>(delta: &'a BifrostTelemetryDelta, name: &str) -> Vec<&'a CapturedSpan> {
    delta
        .spans
        .iter()
        .filter(|span| span.name == name)
        .collect()
}

/// Asserts that no write or Scribe span in a window carries a routine `INFO` event.
///
/// Successful transitions are explained by the operation span itself and its
/// metrics; a per-transition `INFO` record would be a second, redundant series.
///
/// # Panics
///
/// Panics when a write-path span carries an `INFO`-level event.
fn assert_no_routine_info(delta: &BifrostTelemetryDelta, phase: &str) {
    let routine: Vec<_> = delta
        .spans
        .iter()
        .filter(|span| {
            span.name.starts_with("bifrost.scribe.")
                || span.name == "bifrost.gate.write"
                || span.name == "dispatch_native_frame"
        })
        .flat_map(|span| {
            span.events
                .iter()
                .filter(|event| event.attributes.get("level").map(String::as_str) == Some("INFO"))
                .map(move |event| (span.name.as_str(), event.name.as_str()))
        })
        .collect();
    assert!(
        routine.is_empty(),
        "{phase} must not emit per-transition INFO events: {routine:?}"
    );
}

/// Opens one production telemetry window.
///
/// # Panics
///
/// Panics when the installed recorder cannot be rendered.
fn checkpoint(
    telemetry: &BifrostTelemetryCapture,
) -> wyrd_testing::bifrost::telemetry::BifrostTelemetryCheckpoint {
    telemetry
        .checkpoint()
        .expect("production telemetry checkpoint")
}

/// Polls the staging owner until publication has settled, or the deadline passes.
///
/// Settled means no claim is outstanding, no staged member survives its
/// published object, and every Scribe execution lane has no waiting or running
/// job, which is the terminal state the run's committed files and exact
/// read-back are then checked against.
///
/// # Panics
///
/// Panics when the backlog is not inspectable, or when the pod has not
/// settled inside [`SETTLEMENT_DEADLINE`].
async fn await_settled(server: &WyrdTestServer, telemetry: &BifrostTelemetryCapture) {
    let deadline = tokio::time::Instant::now() + SETTLEMENT_DEADLINE;
    loop {
        let backlog = server
            .scribe_staging_backlog_for_test()
            .expect("the pod's staged backlog is inspectable");
        let lanes: Vec<_> = telemetry
            .snapshot()
            .expect("production metrics render")
            .into_iter()
            .filter(|sample| {
                ["bifrost_scribe_lane_queued", "bifrost_scribe_lane_active"]
                    .contains(&sample.family.as_str())
                    && sample.value != 0.0
            })
            .collect();
        if backlog.outstanding_claims == 0 && backlog.live_members == 0 && lanes.is_empty() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "publication did not settle inside {SETTLEMENT_DEADLINE:?}: {backlog:?}, \
             busy lanes {lanes:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

/// Scrapes the four staged-backlog gauges and pairs them with the owner's backlog.
///
/// Prints one evidence line naming the phase, the scraped values, and the
/// staging owner's values, so the run's output records the production samples
/// beside the fact they are compared with.
///
/// # Panics
///
/// Panics when the recorder cannot render, the owner is not inspectable, or
/// the scraped gauges disagree with the owner's backlog.
fn scrape_staging(
    server: &WyrdTestServer,
    telemetry: &BifrostTelemetryCapture,
    phase: &str,
) -> [Option<f64>; 4] {
    let samples = telemetry.snapshot().expect("production metrics render");
    let scraped = STAGING_GAUGES.map(|family| gauge(&samples, family));
    let owner = server
        .scribe_staging_backlog_for_test()
        .expect("the pod's staged backlog is inspectable");
    let expected = [
        Some(owner.live_members as f64),
        Some(owner.live_bytes as f64),
        Some(
            owner
                .oldest_ready_at
                .map_or(0.0, |ready_at| ready_at.timestamp() as f64),
        ),
        Some(owner.outstanding_claims as f64),
    ];
    eprintln!("evidence staged_backlog phase={phase} scraped={scraped:?} owner={owner:?}");
    assert_eq!(
        scraped, expected,
        "{phase}: the scraped staging gauges equal the staging owner's backlog"
    );
    scraped
}

/// Staged backlog survives an abrupt restart and settles after publication.
///
/// A one-node cluster acknowledges a known batch and freezes it into durable
/// staging, below the object target and before its dwell, without requesting
/// publication. The production Prometheus exposition then shows nonzero
/// members and bytes and a persisted oldest ready time. The node is killed
/// without a drain and replaced over its retained roots. Before publication is
/// requested, the replacement's exposition still shows nonzero members and
/// bytes, the same oldest ready time, and the claims the restored owner
/// actually holds, and the client still reads every acknowledged row. A
/// resent batch inserts nothing. Publication then commits the rows once and
/// the backlog and claim gauges reach zero.
///
/// The recorder is process-global and survives the in-process restart, so an
/// unchanged value here cannot prove the replacement re-emitted it; the
/// fresh-recorder `restored_stage_republishes_backlog` test proves emission
/// origin. This journey proves exposition, readback, and settlement.
///
/// # Panics
///
/// Panics when the cluster cannot start, terminate, or restart the node, when
/// a public append, read, or publication fails, or when a scrape disagrees with
/// the staging owner, the persisted ready time, or the committed files.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn staged_backlog_survives_abrupt_restart() {
    let mut cluster = WyrdTestCluster::start_spec(BifrostClusterSpec::one_mixed())
        .await
        .expect("the one-pod mixed cluster starts");
    let telemetry = cluster.telemetry().clone();
    let tenant = cluster.data_tenant_id();
    let name = unique_table("staged_restart");
    let table = format!("{}.{name}", BifrostNamespace::Datasets.as_str());
    let rows: Vec<i64> = (0..48).collect();
    let batch_id = uuid::Uuid::now_v7();
    let (node, before) = {
        let server = cluster.server(0).expect("the mixed pod is running");
        register_table(server, tenant, BifrostNamespace::Datasets, &name).await;
        let client = tenant_client(server, tenant).await;
        append_values(&client, &table, batch_id, &rows)
            .await
            .expect("the batch is acknowledged");
        let scribe = server.bifrost_scribe().expect("the pod owns a Scribe");
        scribe
            .flush_writable_for_test()
            .await
            .expect("every writable bucket freezes");
        await_persistence_drained(&scribe).await;
        assert_eq!(
            published_rows(server, tenant, &name).await,
            0,
            "the staged rows wait below the object target and dwell"
        );
        (
            server.node_id(),
            scrape_staging(server, &telemetry, "staged before restart"),
        )
    };
    let [Some(members), Some(bytes), Some(oldest), _] = before else {
        panic!("every staging gauge is exposed: {before:?}");
    };
    assert!(members > 0.0 && bytes > 0.0 && oldest > 0.0, "{before:?}");

    let roots = cluster
        .terminate_node_abruptly_for_test(node)
        .await
        .expect("the pod is terminated without a drain");
    cluster
        .restart_terminated_node_at_new_address(node, roots)
        .await
        .expect("a replacement pod restarts on the retained roots");
    let server = cluster.server(0).expect("the replacement pod is running");
    let restored = scrape_staging(server, &telemetry, "restored before publication");
    let [Some(members), Some(bytes), Some(restored_oldest), _] = restored else {
        panic!("every staging gauge is exposed after restart: {restored:?}");
    };
    assert!(members > 0.0 && bytes > 0.0, "{restored:?}");
    assert_eq!(
        restored_oldest, oldest,
        "the restored backlog keeps its persisted oldest ready time"
    );
    let client = tenant_client(server, tenant).await;
    assert_eq!(
        sorted_values(&client, &table).await,
        rows,
        "the restored pod serves every acknowledged row before publication"
    );

    let resend = checkpoint(&telemetry);
    append_values(&client, &table, batch_id, &rows)
        .await
        .expect("a resent acknowledged batch is acknowledged");
    let resent = telemetry.delta_since(&resend).expect("resend window");
    let inserted = delta_value(
        &resent,
        "bifrost_scribe_memtable_rows_inserted_total",
        BifrostMetricKind::Counter,
    );
    eprintln!("evidence staged_backlog phase=resend rows_inserted_delta={inserted}");
    assert_eq!(
        inserted, 0.0,
        "a resent batch does not claim a second insertion"
    );

    server
        .flush_bifrost()
        .await
        .expect("the restored rows publish");
    await_settled(server, &telemetry).await;
    let settled = scrape_staging(server, &telemetry, "settled after publication");
    assert_eq!(settled, [Some(0.0); 4], "backlog and claims settle to zero");
    let committed = published_rows(server, tenant, &name).await;
    eprintln!("evidence staged_backlog phase=published committed_rows={committed}");
    assert_eq!(
        committed,
        rows.len() as u64,
        "the committed files hold every acknowledged row exactly once"
    );
    assert_eq!(sorted_values(&client, &table).await, rows);

    cluster
        .shutdown()
        .await
        .expect("the cluster drains cleanly");
}

/// Scribe's production telemetry reconciles with its durable owners.
///
/// The case walks an ordinary hot path — write, retry, freeze, stalled
/// publication, publish, read — then a controlled WAL failure. At each boundary
/// it compares the installed production recorder's delta and the captured
/// traces with a fact the run establishes without telemetry:
///
/// - each acknowledged write is one successful Gate write trace carrying its
///   batch identity, one ACK-attempt sample, and exactly its rows newly
///   inserted into the memtable;
/// - a same-batch retry is acknowledged and counted as an ACK attempt but
///   inserts nothing;
/// - after the freeze and before publication, the staged-backlog gauges equal
///   the staging owner's members, bytes, oldest ready time, and claims, while
///   the acknowledged rows stay readable;
/// - publication's committed file and byte counters equal the committed hot
///   files, every publication trace names the tenant and succeeded, and the
///   backlog gauges and lane gauges settle to zero;
/// - ordinary success emits no per-transition `INFO` event; and
/// - a WAL sync fault fails the client write with one correlated failure event
///   on its failed write trace, a failed fsync sample, and no insertion.
///
/// Audit publication is disabled so the process-wide Scribe counters move only
/// with this case's own writes.
///
/// # Panics
///
/// Panics when a public append, freeze, publication, or read fails, when an
/// owner is not inspectable, or when any telemetry fact disagrees with the
/// durable fact observed beside it.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_hot_path_telemetry_reconciles() {
    let (_telemetry_guard, telemetry) =
        shared_process_telemetry_for_test().expect("process production telemetry");
    let server = WyrdTestServer::builder()
        .without_audit_publication_for_test()
        .start_bound()
        .await
        .expect("the Scribe production harness starts");
    let tenant = server.data_tenant_id();
    let name = unique_table("telemetry_reconcile");
    let table = register_table(&server, tenant, BifrostNamespace::Datasets, &name).await;
    let client = tenant_client(&server, tenant).await;
    let scribe = server.bifrost_scribe().expect("the server owns a Scribe");
    let journey = checkpoint(&telemetry);

    // Idle: the staging owner holds nothing.
    let idle = server
        .scribe_staging_backlog_for_test()
        .expect("the pod's staged backlog is inspectable");
    assert_eq!((idle.live_members, idle.outstanding_claims), (0, 0));

    // Write: four acknowledged batches.
    let write = checkpoint(&telemetry);
    let expected: Vec<i64> = (0..64).collect();
    let mut batches = Vec::new();
    for chunk in expected.chunks(16) {
        let batch_id = uuid::Uuid::now_v7();
        append_values(&client, &table, batch_id, chunk)
            .await
            .unwrap_or_else(|error| panic!("append {batch_id} is acknowledged: {error:?}"));
        batches.push(batch_id);
    }
    let written = telemetry.delta_since(&write).expect("write window");
    eprintln!(
        "evidence write acknowledged_batches=4 acknowledged_rows=64 samples: {}",
        written.evidence(&WRITE_FAMILIES)
    );
    assert_eq!(
        delta_value(
            &written,
            "bifrost_scribe_memtable_rows_inserted_total",
            BifrostMetricKind::Counter
        ),
        64.0,
        "the shard owner counts exactly the acknowledged rows as newly inserted"
    );
    assert_eq!(
        delta_value(
            &written,
            "bifrost_scribe_ack_seconds",
            BifrostMetricKind::HistogramCount
        ),
        4.0,
        "each acknowledged batch is one ACK attempt"
    );
    assert!(
        labelled_delta(
            &written,
            "bifrost_scribe_wal_fsync_total",
            "outcome",
            "success"
        ) > 0.0,
        "acknowledged writes are WAL-synced"
    );
    let writes = spans_named(&written, "bifrost.gate.write");
    assert_eq!(writes.len(), 4, "one write trace per acknowledged batch");
    assert!(
        writes
            .iter()
            .all(|span| attribute(span, "outcome") == Some("success")),
        "every acknowledged write trace terminates successfully: {writes:?}"
    );
    let traced: BTreeSet<String> = spans_named(&written, "dispatch_native_frame")
        .iter()
        .filter(|span| attribute(span, "tenant") == Some(tenant.to_string().as_str()))
        .filter_map(|span| attribute(span, "batch_id").map(str::to_owned))
        .collect();
    let sent: BTreeSet<String> = batches.iter().map(ToString::to_string).collect();
    assert_eq!(
        traced, sent,
        "the write traces carry each request's batch id"
    );
    print_trace(&written, &writes[0].trace_id, "write_success");
    assert!(
        spans_named(&written, "bifrost.scribe.wal.append").is_empty(),
        "per-append WAL spans are DEBUG detail, not routine operation traces"
    );
    assert_no_routine_info(&written, "a successful write");

    // Retry: the same batch is acknowledged again and inserts nothing.
    let retry = checkpoint(&telemetry);
    append_values(&client, &table, batches[0], &expected[..16])
        .await
        .expect("a same-batch retry is acknowledged");
    let retried = telemetry.delta_since(&retry).expect("retry window");
    eprintln!(
        "evidence retry acknowledged_batches=1 new_rows=0 samples: {}",
        retried.evidence(&WRITE_FAMILIES)
    );
    assert_eq!(
        delta_value(
            &retried,
            "bifrost_scribe_ack_seconds",
            BifrostMetricKind::HistogramCount
        ),
        1.0,
        "the retry is a successful ACK attempt"
    );
    assert_eq!(
        delta_value(
            &retried,
            "bifrost_scribe_memtable_rows_inserted_total",
            BifrostMetricKind::Counter
        ),
        0.0,
        "a replayed batch must not claim a second insertion"
    );
    assert_eq!(
        scribe
            .memtable_stats()
            .expect("memtable stats")
            .writable_rows,
        64,
        "the memtable owner holds each acknowledged row once"
    );

    // Freeze, publication not yet requested: the staged backlog is visible
    // and equals the staging owner, while acknowledged rows stay readable.
    let freeze = checkpoint(&telemetry);
    scribe
        .flush_writable_for_test()
        .await
        .expect("every writable bucket freezes");
    await_persistence_drained(&scribe).await;
    // The memtable gauges are refreshed by the pod's own age tick, so the
    // window stays open until that production emitter has published the
    // frozen state rather than closing on a sample taken before it ran.
    let deadline = tokio::time::Instant::now() + SETTLEMENT_DEADLINE;
    while gauge(
        &telemetry.snapshot().expect("production metrics render"),
        "bifrost_scribe_active_memtable_bytes",
    ) != Some(0.0)
    {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the age tick did not publish the frozen memtable"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let frozen = telemetry.delta_since(&freeze).expect("freeze window");
    eprintln!(
        "evidence freeze memtable_writable_rows_after={} samples: {}",
        scribe
            .memtable_stats()
            .expect("memtable stats")
            .writable_rows,
        frozen.evidence(&MEMORY_FAMILIES)
    );
    let staged = server
        .scribe_staging_backlog_for_test()
        .expect("the pod's staged backlog is inspectable");
    assert!(
        staged.live_members > 0 && staged.live_bytes > 0,
        "{staged:?}"
    );
    assert_eq!(staged.outstanding_claims, 0, "no claim before publication");
    let oldest = staged
        .oldest_ready_at
        .expect("a live member has a ready time")
        .timestamp();
    let samples = telemetry.snapshot().expect("production metrics render");
    let published_gauges: Vec<Option<f64>> = STAGING_GAUGES
        .iter()
        .map(|family| gauge(&samples, family))
        .collect();
    assert_eq!(
        published_gauges,
        vec![
            Some(staged.live_members as f64),
            Some(staged.live_bytes as f64),
            Some(oldest as f64),
            Some(0.0),
        ],
        "the staging gauges equal the staging owner's backlog"
    );
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "acknowledged rows stay readable while publication is stalled"
    );

    // Publish: committed output counters equal the committed hot files.
    let publish = checkpoint(&telemetry);
    server
        .flush_bifrost()
        .await
        .expect("the staged members publish");
    await_settled(&server, &telemetry).await;
    let published_window = telemetry.delta_since(&publish).expect("publish window");
    let files = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    eprintln!(
        "evidence publish committed_files={} committed_bytes={} committed_rows={} samples: {}",
        files.len(),
        files.iter().map(|file| file.file_size).sum::<u64>(),
        files.iter().map(|file| file.row_count).sum::<u64>(),
        published_window.evidence(&PUBLICATION_FAMILIES)
    );
    assert_eq!(
        files.iter().map(|file| file.row_count).sum::<u64>(),
        64,
        "the committed objects account for every acknowledged row exactly once"
    );
    assert_eq!(
        delta_value(
            &published_window,
            "bifrost_scribe_publication_files_total",
            BifrostMetricKind::Counter
        ),
        files.len() as f64,
        "committed publication files equal the committed hot files"
    );
    assert_eq!(
        delta_value(
            &published_window,
            "bifrost_scribe_publication_bytes_total",
            BifrostMetricKind::Counter
        ),
        files.iter().map(|file| file.file_size).sum::<u64>() as f64,
        "committed publication bytes equal the committed hot-file sizes"
    );
    assert!(
        delta_value(
            &published_window,
            "bifrost_scribe_staging_claims_published_total",
            BifrostMetricKind::Counter
        ) >= 1.0
    );
    let samples = telemetry.snapshot().expect("production metrics render");
    for family in STAGING_GAUGES {
        assert_eq!(
            gauge(&samples, family),
            Some(0.0),
            "{family} settles to zero"
        );
    }
    assert_eq!(
        sorted_values(&client, &table).await,
        expected,
        "the published run still reads back exactly the acknowledged rows"
    );

    // The whole successful journey: each staged generation is one
    // publication trace naming its tenant, shard, and generation.
    let success = telemetry.delta_since(&journey).expect("journey window");
    let publications = spans_named(&success, "bifrost.scribe.visibility.publish");
    assert!(!publications.is_empty(), "staging is traced");
    for span in &publications {
        assert_eq!(attribute(span, "tenant"), Some(tenant.to_string().as_str()));
        assert!(attribute(span, "shard_id").is_some() && attribute(span, "generation").is_some());
        assert_eq!(attribute(span, "outcome"), Some("success"), "{span:?}");
    }
    assert_no_routine_info(&success, "the successful journey");

    // Controlled failure: a WAL sync fault fails the write with one correlated
    // failure event, a failed fsync sample, and no insertion.
    let failure = checkpoint(&telemetry);
    server
        .trip_bifrost_wal_sync_fault_for_test()
        .expect("the server owns a Scribe WAL");
    append_values(&client, &table, uuid::Uuid::now_v7(), &[64])
        .await
        .expect_err("a write whose WAL sync failed is not acknowledged");
    let failed = telemetry.delta_since(&failure).expect("failure window");
    eprintln!(
        "evidence wal_failure client_acknowledged=false samples: {}",
        failed.evidence(&WRITE_FAMILIES)
    );
    assert!(
        labelled_delta(
            &failed,
            "bifrost_scribe_wal_fsync_total",
            "outcome",
            "failed"
        ) >= 1.0,
        "the WAL owner reports the failed sync"
    );
    assert_eq!(
        delta_value(
            &failed,
            "bifrost_scribe_memtable_rows_inserted_total",
            BifrostMetricKind::Counter
        ),
        0.0,
        "an unacknowledged write inserts nothing"
    );
    let failed_writes: Vec<_> = spans_named(&failed, "bifrost.gate.write")
        .into_iter()
        .filter(|span| attribute(span, "outcome") == Some("failed"))
        .collect();
    assert!(!failed_writes.is_empty(), "the failed write is traced");
    for span in &failed_writes {
        print_trace(&failed, &span.trace_id, "write_failure");
    }
    for span in failed_writes {
        // The whole trace, not only the root: a failure re-logged by a child
        // span would repeat the reason under the same correlation identity.
        let failures: Vec<_> = failed
            .spans
            .iter()
            .filter(|child| child.trace_id == span.trace_id)
            .flat_map(|child| &child.events)
            .filter(|event| {
                matches!(
                    event.attributes.get("level").map(String::as_str),
                    Some("WARN" | "ERROR")
                )
            })
            .collect();
        assert_eq!(
            failures.len(),
            1,
            "one correlated failure event per failed write trace: {span:?}"
        );
        assert!(
            failures[0].attributes.contains_key("error"),
            "the failure event carries its reason: {failures:?}"
        );
    }

    print_family_inventory(&telemetry);
    server.shutdown().await.expect("the server drains cleanly");
}
