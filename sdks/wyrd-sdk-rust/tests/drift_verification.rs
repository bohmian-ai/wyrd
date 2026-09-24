//! Rust SDK Drift verification journey through the public `wyrd_sdk` crate.
//!
//! Registers a Parquet baseline Data Card, PSI and SPC Verifiers fitted from
//! it, a Custom Verifier that needs no fit, and an SPC Verifier whose baseline
//! cannot fit. Card status shows each baseline settle without blocking
//! registration. Two Services bind the SPC Verifier through one shared
//! schedule Trigger Card, with two and one inline Operators, and emit Drift
//! observations through `WyrdState`; direct runs of every method then score
//! the window server-side through Oracle and persist their results, PSI bin
//! and SPC X-bar/S evidence, and feature rows, an empty window completes
//! inconclusive with null details and no features, one due occurrence of the
//! shared Trigger runs each binding once, fails on SPC signals, and
//! dispatches only that binding's Operators, and a manual run of one binding
//! dispatches through the same Operator path. Negative flows cover an unready
//! Verifier, a non-Parquet baseline, a retired SPC profile field, a caller
//! without `evals:run`, and a second tenant.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arrow::array::{Float64Array, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use base64::Engine;
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, RegistrationReceipt};
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::verification::{
    BindingId, OperatorDispatchState, StartVerificationRunRequest, Verification,
    VerificationExecutionStatus, VerificationResultId, VerificationRunId, VerificationRunStatus,
};
use wyrd_sdk::{Bifrost, QueueConfig, WyrdClient};
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;
use wyrd_testing::verification::VerificationFixture;

/// Upper bound on every wait for the server-side runtime to make progress.
const WAIT: Duration = Duration::from_secs(90);

/// Rows in the baseline Parquet artifact.
const BASELINE_ROWS: u32 = 100;

/// The Drift feature map every observation projects into one row per series.
#[derive(Serialize)]
struct Features {
    /// Numeric series the PSI and SPC Verifiers monitor.
    latency: f64,
    /// Categorical series the PSI Verifier bins by label.
    tier: String,
    /// Scalar metric the Custom Verifier averages.
    score: f64,
}

/// The summary columns the journey reads back per result.
#[derive(Debug, Deserialize)]
struct ResultRow {
    /// Canonical result execution status.
    execution_status: String,
    /// Common verdict.
    verdict: String,
    /// Implementation summary; null for an unscored inconclusive result.
    details: Option<String>,
    /// Exact subject Card UID the window was read for.
    subject_card_uid: String,
    /// Binding of a binding-created run; null for a direct run.
    binding_id: Option<String>,
}

/// One feature row joined to its parent result.
#[derive(Debug, Deserialize)]
struct FeatureRow {
    /// Monitored feature name.
    feature: String,
    /// Drift method that scored it.
    method: String,
    /// Per-feature verdict.
    verdict: String,
}

/// Unwrap a machine bootstrap into its raw API key.
///
/// # Panics
/// Panics when the bootstrap is a user principal.
fn api_key(bootstrap: Bootstrap) -> String {
    match bootstrap {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("expected a machine principal"),
    }
}

/// Write the baseline Parquet artifact and return its bytes.
///
/// `latency` spreads evenly over `[0, 100)` and `tier` alternates `gold` and
/// `silver`, so a window of high latencies and an unseen tier drifts under
/// PSI and SPC.
///
/// # Panics
/// Panics when the batch or the Parquet file cannot be written.
fn write_baseline_parquet(path: &Path) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("latency", DataType::Float64, false),
        Field::new("tier", DataType::Utf8, false),
    ]));
    let latency: Vec<f64> = (0..BASELINE_ROWS).map(f64::from).collect();
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
            Arc::new(Float64Array::from(latency)),
            Arc::new(StringArray::from_iter_values(
                (0..BASELINE_ROWS).map(|row| if row % 2 == 0 { "gold" } else { "silver" }),
            )),
        ],
    )
    .expect("baseline batch builds");
    let mut bytes = Vec::new();
    let mut writer =
        parquet::arrow::ArrowWriter::try_new(&mut bytes, schema, None).expect("writer opens");
    writer.write(&batch).expect("baseline batch writes");
    writer.close().expect("parquet footer writes");
    std::fs::create_dir_all(path.parent().expect("artifact has a parent"))
        .expect("artifact directory creates");
    std::fs::write(path, &bytes).expect("parquet artifact writes");
    bytes
}

/// Write the baseline Data Card over a Parquet artifact.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_baseline(root: &Path) {
    let bytes = write_baseline_parquet(&root.join("data/data.parquet"));
    let hex = format!("{:x}", sha2::Sha256::digest(&bytes));
    let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&bytes));
    std::fs::write(
        root.join("baseline.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Data\nmetadata:\n  name: drift-baseline\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Parquet\n    meta:\n      compression: Snappy\n  schema:\n    columns:\n      - name: latency\n        dtype: float64\n      - name: tier\n        dtype: string\n  card_refs: []\n  stats:\n    row_count: {BASELINE_ROWS}\n    col_count: 2\n    byte_count: {len}\n    sha256: {hex}\nartifacts:\n  - relative_path: data/data.parquet\n    sha256: {digest}\n    size_bytes: {len}\n    content_type: application/vnd.apache.parquet\n",
            len = bytes.len()
        ),
    )
    .expect("baseline card writes");
}

/// Build one Drift Verifier Card YAML named `name` from its method block.
fn verifier_yaml(name: &str, method: &str) -> String {
    format!(
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: drift\n    spec:\n{method}"
    )
}

/// Distribution signal over the baseline for `features`, indented for a spec.
fn distribution(features: &str) -> String {
    format!(
        "      signal:\n        kind: Distribution\n        baseline_ref:\n          kind: Data\n          name: drift-baseline\n          version: 1.0.0\n          space: default\n        features: [{features}]\n      condition:\n        kind: Statistical\n"
    )
}

/// Service Card YAML named `name` binding `drift-spc` on the shared
/// `drift-daily` Trigger with one inline HTTP Operator per hook in `hooks`.
fn service_yaml(name: &str, hooks: &[&str]) -> String {
    let operators = hooks
        .iter()
        .map(|hook| {
            format!(
                "        - kind: http\n          method: post\n          url: https://hooks.example.test/{hook}\n"
            )
        })
        .collect::<Vec<_>>()
        .concat();
    format!(
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec:\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: drift-spc\n        version: 1.0.0\n        space: default\n      runs_on:\n        kind: Trigger\n        name: drift-daily\n        version: 1.0.0\n        space: default\n      on_failure:\n{operators}"
    )
}

/// Write every Verifier, the shared Trigger, and both bound Services.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_verifiers(root: &Path) {
    let cards = [
        (
            "drift-psi",
            format!(
                "      method: Psi\n{}      profile:\n        kind: Psi\n        binning_strategy:\n          kind: EqualWidth\n          n_bins: 10\n        categorical_features: [tier]\n        threshold:\n          kind: Fixed\n          value: 0.25\n",
                distribution("latency, tier")
            ),
        ),
        (
            "drift-spc",
            format!(
                "      method: Spc\n{}      profile:\n        kind: Spc\n        sample_size: 5\n",
                distribution("latency")
            ),
        ),
        (
            "drift-spc-unfit",
            format!(
                "      method: Spc\n{}      profile:\n        kind: Spc\n        sample_size: 5\n",
                distribution("tier")
            ),
        ),
        (
            "drift-custom",
            "      method: Custom\n      signal:\n        kind: Metric\n        name: score\n      condition:\n        kind: Statistical\n      profile:\n        kind: Custom\n        metric_name: score\n        baseline_value: 1.0\n        alert_threshold: 0.5\n".to_owned(),
        ),
    ];
    for (name, method) in cards {
        std::fs::write(
            root.join(format!("{name}.yaml")),
            verifier_yaml(name, &method),
        )
        .expect("verifier card writes");
    }
    std::fs::write(
        root.join("trigger.yaml"),
        "apiVersion: wyrd/v1\nkind: Trigger\nmetadata:\n  name: drift-daily\n  version: 1.0.0\n  space: default\nspec:\n  kind: schedule\n  cron: \"0 0 * * *\"\n",
    )
    .expect("trigger card writes");
    std::fs::write(
        root.join("service.yaml"),
        service_yaml("drift-service", &["drift-a", "drift-b"]),
    )
    .expect("service card writes");
    std::fs::write(
        root.join("service-b.yaml"),
        service_yaml("drift-service-b", &["drift-c"]),
    )
    .expect("second service card writes");
}

/// Register one Card file and return its receipt.
///
/// # Panics
/// Panics when registration fails.
async fn register(cards: &Cards, path: &Path) -> RegistrationReceipt {
    Box::pin(cards.register_from_path(path))
        .await
        .unwrap_or_else(|error| panic!("{} registers: {error:?}", path.display()))
}

/// Poll a Verifier Card until its baseline reaches `state`, returning its error code.
///
/// # Panics
/// Panics when the Card cannot be read, has no baseline status, or never
/// reaches `state` within [`WAIT`].
async fn wait_baseline(
    cards: &Cards,
    verifier: &RegistrationReceipt,
    state: &str,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let baseline = cards
            .get(CardSelector::exact(verifier.root.clone()))
            .await
            .expect("verifier reads")
            .status
            .and_then(|status| status.verification)
            .and_then(|verification| verification.baseline)
            .expect("a PSI or SPC Verifier serves baseline status");
        assert_eq!(
            baseline.data.name.as_str(),
            "drift-baseline",
            "status names the exact baseline Data"
        );
        if baseline.state.as_str() == state {
            return baseline.error.map(|error| error.code);
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "{} baseline never reached {state}: {baseline:?}",
            verifier.root.name
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Build a direct run request of `verifier` over `subject` for `[start, end)`.
///
/// # Panics
/// Panics when the JSON fixture does not match the wire contract.
fn direct(
    verifier: &RegistrationReceipt,
    subject: &RegistrationReceipt,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> StartVerificationRunRequest {
    serde_json::from_value(serde_json::json!({
        "target": {
            "kind": "verifier",
            "verifier_uid": verifier.root.uid,
            "subject_card_uid": subject.root.uid,
        },
        "input": { "kind": "drift_window", "start": start, "end": end },
    }))
    .expect("run request matches the wire contract")
}

/// Poll `run` until it leaves pending, running, and retrying.
///
/// # Panics
/// Panics when the run cannot be read or never settles within [`WAIT`].
async fn wait_settled(
    verification: &Verification,
    run: &VerificationRunId,
) -> VerificationRunStatus {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let status = verification.get_run(run).await.expect("run reads");
        if !matches!(
            status.status,
            VerificationExecutionStatus::Pending
                | VerificationExecutionStatus::Running
                | VerificationExecutionStatus::Retrying
        ) {
            return status;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "run never settled: {status:?}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Start `request`, wait for it to complete, and read its result and features.
///
/// # Panics
/// Panics when the run does not complete or its rows cannot be read.
async fn complete(
    server: &WyrdTestServer,
    verification: &Verification,
    query: &Bifrost,
    request: &StartVerificationRunRequest,
) -> (ResultRow, Vec<FeatureRow>) {
    let run = verification
        .start_run(request, None)
        .await
        .expect("run starts");
    let status = wait_settled(verification, &run).await;
    assert_eq!(
        status.status,
        VerificationExecutionStatus::Completed,
        "{status:?}"
    );
    assert!(
        status.dispatches.is_empty(),
        "a direct run never dispatches"
    );
    let result_id = status.result_id.expect("a completed run names its result");
    read_result(server, query, &result_id.to_string()).await
}

/// Flush Scribe, then read one result and its feature rows joined on result
/// identity within the caller's tenant-scoped view.
///
/// A tenant that has never scored a report has no feature table yet, which
/// reads as no feature rows.
///
/// # Panics
/// Panics when the flush or either query fails, or the result is not exactly
/// one row.
async fn read_result(
    server: &WyrdTestServer,
    query: &Bifrost,
    result_id: &str,
) -> (ResultRow, Vec<FeatureRow>) {
    server.flush_bifrost().await.expect("flush server Scribe");
    let mut results: Vec<ResultRow> = query
        .sql_as(&format!(
            "SELECT execution_status, verdict, details, subject_card_uid, binding_id \
             FROM vala.verification.results WHERE result_id = '{result_id}'"
        ))
        .await
        .expect("result reads");
    assert_eq!(results.len(), 1, "one summary per result: {results:?}");
    let features: Vec<FeatureRow> = match query
        .sql_as(&format!(
            "SELECT f.feature, f.method, f.verdict \
             FROM vala.drift.result_features f JOIN vala.verification.results r \
               ON f.result_id = r.result_id \
             WHERE r.result_id = '{result_id}' ORDER BY f.feature"
        ))
        .await
    {
        Ok(features) => features,
        Err(error)
            if wyrd_sdk::verification::WyrdError::from(&error).code()
                == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND" =>
        {
            Vec::new()
        }
        Err(error) => panic!("feature rows read: {error:?}"),
    };
    (results.remove(0), features)
}

/// Emit the observation window for `subject` through one `WyrdState` lifetime.
///
/// Returns the Service's own credential, whose Card scope covers the subject
/// every run in the journey verifies.
///
/// Latencies sit high above the baseline range and every tier is new, so PSI
/// and SPC drift; the score mean is 2.0 against a 1.0 baseline, so Custom
/// drifts too.
///
/// # Panics
/// Panics when hydration, startup, an emit, or the drain fails.
async fn emit_window(
    server: &WyrdTestServer,
    admin: &WyrdClient,
    service: &RegistrationReceipt,
    bundle: &Path,
) -> String {
    let rows: Vec<Features> = (0..120_u32)
        .map(|row| Features {
            latency: 150.0 + f64::from(row),
            tier: "bronze".to_owned(),
            score: if row % 2 == 0 { 1.5 } else { 2.5 },
        })
        .collect();
    emit_rows(server, admin, service, bundle, &rows).await
}

/// Emit `rows` as Drift observations of `service` through one `WyrdState`
/// lifetime, drained and flushed before returning.
///
/// Each lifetime is one client batch, so calling this twice for one subject
/// writes two batches of different sizes. Returns the Service's credential.
///
/// # Panics
/// Panics when hydration, startup, an emit, or the drain fails.
async fn emit_rows<T: Serialize>(
    server: &WyrdTestServer,
    admin: &WyrdClient,
    service: &RegistrationReceipt,
    bundle: &Path,
    rows: &[T],
) -> String {
    let hydrator =
        CardGraphHydrator::new(Cards::with_client(WyrdClient::clone(admin)).registry_context());
    Box::pin(hydrator.hydrate(
        &CardSelector::exact(service.root.clone()),
        bundle,
        HydrationMode::Complete,
    ))
    .await
    .expect("service bundle hydrates");
    let credential = api_key(
        server
            .credential_registered_service(&service.root, &["admin"])
            .await
            .expect("service credential issues"),
    );
    let state = WyrdState::from_path(bundle).expect("bundle loads offline");
    state
        .start_bifrost_with_config(&connect(server, &credential), None, QueueConfig::default())
        .await
        .expect("bifrost starts");
    let run = state.run();
    for row in rows {
        run.observe().drift(row, None).expect("drift emits");
    }
    state.shutdown().await.expect("state drains");
    server.flush_bifrost().await.expect("flush server Scribe");
    credential
}

/// Build one public client over the bound test server.
///
/// # Panics
/// Panics when the client cannot be assembled.
fn connect(server: &WyrdTestServer, credential: &str) -> WyrdClient {
    client_from_options(
        Some(server.base_url().expect("bound server has a URL")),
        Some(credential),
        server.grpc_url().as_deref(),
    )
    .expect("client builds")
}

/// Prove PSI, SPC, and Custom Drift fit, score server-side, persist results,
/// and dispatch only a failed scheduled binding result.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn drift_methods_fit_score_persist_and_dispatch() {
    let root = tempfile::tempdir().expect("fixture root creates");
    write_baseline(root.path());
    write_verifiers(root.path());
    let server = Box::pin(
        WyrdTestServer::builder()
            .with_verification_runtime_for_test()
            .start_bound(),
    )
    .await
    .expect("test server starts");
    let admin_key = api_key(
        server
            .bootstrap_service("rust_drift_admin", &["admin"])
            .await
            .expect("admin bootstraps"),
    );
    let admin = connect(&server, &admin_key);
    let cards = Cards::with_client(WyrdClient::clone(&admin));

    register(&cards, &root.path().join("baseline.yaml")).await;
    let psi = register(&cards, &root.path().join("drift-psi.yaml")).await;
    let spc = register(&cards, &root.path().join("drift-spc.yaml")).await;
    let unfit = register(&cards, &root.path().join("drift-spc-unfit.yaml")).await;
    let custom = register(&cards, &root.path().join("drift-custom.yaml")).await;
    let custom_status = cards
        .get(CardSelector::exact(custom.root.clone()))
        .await
        .expect("custom verifier reads")
        .status
        .and_then(|status| status.verification);
    assert!(
        custom_status.and_then(|status| status.baseline).is_none(),
        "a Custom Verifier is ready without a fit job"
    );
    assert_eq!(wait_baseline(&cards, &psi, "ready").await, None);
    assert_eq!(wait_baseline(&cards, &spc, "ready").await, None);
    assert_eq!(
        wait_baseline(&cards, &unfit, "failed").await.as_deref(),
        Some("baseline_fit_failed"),
        "a baseline that cannot fit fails visibly"
    );
    register(&cards, &root.path().join("trigger.yaml")).await;
    let service = register(&cards, &root.path().join("service.yaml")).await;
    let service_b = register(&cards, &root.path().join("service-b.yaml")).await;

    let credential = emit_window(&server, &admin, &service, &root.path().join("bundle")).await;
    emit_window(&server, &admin, &service_b, &root.path().join("bundle-b")).await;
    let verification = Verification::with_client(connect(&server, &credential));
    let query = Bifrost::query_only(&admin);
    let now = Utc::now();
    let window = (
        now - chrono::Duration::hours(1),
        now + chrono::Duration::hours(1),
    );
    let subject = service
        .root
        .uid
        .as_ref()
        .expect("service has a UID")
        .to_string();

    let empty = (
        now - chrono::Duration::days(3),
        now - chrono::Duration::days(2),
    );
    assert_direct_scores(
        &server,
        &verification,
        &query,
        [
            direct(&psi, &service, window.0, window.1),
            direct(&spc, &service, window.0, window.1),
            direct(&custom, &service, window.0, window.1),
            direct(&custom, &service, empty.0, empty.1),
        ],
        &subject,
    )
    .await;

    let unready = verification
        .start_run(&direct(&unfit, &service, window.0, window.1), None)
        .await
        .expect_err("an unready Verifier is refused");
    assert_eq!(unready.code(), "WYRD_VERIFICATION_409_VERIFIER_NOT_READY");

    let bindings = SharedTrigger {
        server: &server,
        cards: &cards,
        verification: &verification,
        query: &query,
    };
    let scheduled = bindings
        .assert_one_occurrence_runs_each_binding([(&service, 2), (&service_b, 1)])
        .await;
    bindings
        .assert_manual_binding_run_dispatches(&service, &scheduled[0], window)
        .await;
    assert_refusals(
        &server,
        root.path(),
        &cards,
        &direct(&psi, &service, window.0, window.1),
    )
    .await;
    server.shutdown().await.expect("test server shuts down");
}

/// An observation carrying only the Custom metric, no PSI or SPC feature.
#[derive(Serialize)]
struct ScoreOnly {
    /// Custom metric value.
    score: f64,
}

/// An observation carrying `latency` but omitting the PSI `tier` feature.
#[derive(Serialize)]
struct LatencyOnly {
    /// Numeric PSI and SPC feature.
    latency: f64,
}

/// A Custom observation whose metric value is text, not a number.
#[derive(Serialize)]
struct TextScore {
    /// Non-numeric value of the Custom metric series.
    score: String,
}

/// Register an unbound Service named `name` as a Drift subject.
///
/// # Panics
/// Panics when the Card file cannot be written or registration fails.
async fn subject(cards: &Cards, root: &Path, name: &str) -> RegistrationReceipt {
    let path = root.join(format!("{name}.yaml"));
    std::fs::write(
        &path,
        format!(
            "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: {name}\n  version: 1.0.0\n  space: default\nspec: {{}}\n"
        ),
    )
    .expect("subject card writes");
    register(cards, &path).await
}

/// A Verification client authenticated as `subject`'s own Service, whose
/// Card scope covers manual runs of it.
///
/// # Panics
/// Panics when the credential cannot be issued.
async fn verifier_of(server: &WyrdTestServer, subject: &RegistrationReceipt) -> Verification {
    let credential = api_key(
        server
            .credential_registered_service(&subject.root, &["admin"])
            .await
            .expect("subject credential issues"),
    );
    Verification::with_client(connect(server, &credential))
}

/// `(feature, method, verdict)` of every feature row, in feature order.
fn verdicts(features: &[FeatureRow]) -> Vec<(&str, &str, &str)> {
    features
        .iter()
        .map(|row| {
            (
                row.feature.as_str(),
                row.method.as_str(),
                row.verdict.as_str(),
            )
        })
        .collect()
}

/// The persisted typed evidence of `feature` in `result`'s details.
///
/// # Panics
/// Panics when the details are absent, not JSON, or carry no evidence for
/// `feature`.
fn evidence(result: &ResultRow, feature: &str) -> serde_json::Value {
    let details: serde_json::Value = serde_json::from_str(
        result
            .details
            .as_deref()
            .expect("a scored report has details"),
    )
    .expect("details are JSON");
    details["features"][feature]["evidence"].clone()
}

/// Assert `result` completed inconclusive before scoring: null details and
/// no feature rows.
///
/// # Panics
/// Panics when the result is scored or not inconclusive.
fn assert_unscored((result, features): &(ResultRow, Vec<FeatureRow>)) {
    assert_eq!(
        (result.execution_status.as_str(), result.verdict.as_str()),
        ("completed", "inconclusive"),
        "{result:?}"
    );
    assert!(result.details.is_none(), "{result:?}");
    assert!(features.is_empty(), "{features:?}");
}

/// Prove each method's edge semantics through the production verification
/// runtime, Oracle, and Bifrost persistence.
///
/// Before the tenant's first Drift write a Custom run completes inconclusive
/// with no details, features, or dispatch. Separate subjects then isolate
/// each case in the one shared observation table: a window matching the
/// baseline distribution passes PSI and Custom, with the Custom mean exactly
/// at its threshold; two in-control subgroups pass SPC with zero-signal
/// evidence until a trailing partial subgroup leaves it unscored; three
/// rows are too few for PSI's minimum sample and leave SPC a partial,
/// unscored subgroup; records
/// without a PSI feature are ignored while one omitting a feature leaves PSI
/// unscored; two client batches of one and three rows average per row, not
/// per batch, and a window ending between them excludes the second; a
/// text-valued metric is inconclusive; and after its fitted profile is
/// retired, a PSI version's historical result stays readable while a new run
/// is refused as legacy.
///
/// # Panics
/// Panics when any run, result, or feature row differs from the expected one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn drift_method_edges_score_through_oracle() {
    let root = tempfile::tempdir().expect("fixture root creates");
    write_baseline(root.path());
    write_verifiers(root.path());
    let server = Box::pin(
        WyrdTestServer::builder()
            .with_verification_runtime_for_test()
            .start_bound(),
    )
    .await
    .expect("test server starts");
    let admin = connect(
        &server,
        &api_key(
            server
                .bootstrap_service("rust_drift_edges", &["admin"])
                .await
                .expect("admin bootstraps"),
        ),
    );
    let cards = Cards::with_client(WyrdClient::clone(&admin));
    let custom = register(&cards, &root.path().join("drift-custom.yaml")).await;
    let now = Utc::now();
    let journey = EdgeJourney {
        server: &server,
        query: Bifrost::query_only(&admin),
        admin,
        cards,
        root: root.path(),
        start: now - chrono::Duration::hours(1),
        end: now + chrono::Duration::hours(1),
    };
    let steady = journey.subject("steady").await;
    assert_unscored(
        &journey
            .run(&custom, &steady, journey.start, journey.end)
            .await,
    );

    register(&journey.cards, &root.path().join("baseline.yaml")).await;
    let psi = register(&journey.cards, &root.path().join("drift-psi.yaml")).await;
    let spc = register(&journey.cards, &root.path().join("drift-spc.yaml")).await;
    assert_eq!(wait_baseline(&journey.cards, &psi, "ready").await, None);
    assert_eq!(wait_baseline(&journey.cards, &spc, "ready").await, None);

    let baseline_like: Vec<Features> = (0..BASELINE_ROWS)
        .map(|row| Features {
            latency: f64::from((row * 37) % BASELINE_ROWS),
            tier: if row % 2 == 0 { "gold" } else { "silver" }.to_owned(),
            score: if row % 2 == 0 { 1.0 } else { 2.0 },
        })
        .collect();
    journey
        .assert_baseline_like_passes(&steady, &psi, &custom, &baseline_like)
        .await;
    journey.assert_calm_spc_passes_until_partial(&spc).await;
    journey
        .assert_sparse_is_inconclusive(&psi, &spc, &baseline_like[..3])
        .await;
    journey
        .assert_incomplete_is_unscored(&psi, &baseline_like)
        .await;
    journey.assert_mean_is_per_row_within_window(&custom).await;
    journey.assert_text_is_unscored(&custom).await;
    journey
        .assert_legacy_profile_is_refused(&psi, &steady)
        .await;

    server.shutdown().await.expect("test server shuts down");
}

/// Shared state of the method-edge journey: one bound server, the admin
/// client that registers subjects and emits their observations, and the
/// default window every run scores unless a case narrows it.
struct EdgeJourney<'a> {
    /// Bound test server running the production verification runtime.
    server: &'a WyrdTestServer,
    /// Admin client used to credential subjects and emit their rows.
    admin: WyrdClient,
    /// Card handle registering subjects on the admin client.
    cards: Cards,
    /// Query-only Bifrost facade reading result and feature rows.
    query: Bifrost,
    /// Fixture root holding Card files and `WyrdState` bundles.
    root: &'a Path,
    /// Inclusive start of the default scoring window.
    start: DateTime<Utc>,
    /// Exclusive end of the default scoring window.
    end: DateTime<Utc>,
}

impl EdgeJourney<'_> {
    /// Run `verifier` directly against `subject` over `[from, to)` as the
    /// subject's own Service, and wait for its persisted result and features.
    ///
    /// # Panics
    /// Panics when the run cannot start, settle, or be read back.
    async fn run(
        &self,
        verifier: &RegistrationReceipt,
        subject: &RegistrationReceipt,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> (ResultRow, Vec<FeatureRow>) {
        let verification = verifier_of(self.server, subject).await;
        complete(
            self.server,
            &verification,
            &self.query,
            &direct(verifier, subject, from, to),
        )
        .await
    }

    /// Register the unbound Service subject `name`.
    ///
    /// # Panics
    /// Panics when registration fails.
    async fn subject(&self, name: &str) -> RegistrationReceipt {
        subject(&self.cards, self.root, name).await
    }

    /// Emit `rows` as `subject` from a bundle named `bundle`.
    ///
    /// # Panics
    /// Panics when emission or its flush fails.
    async fn emit<T: Serialize>(&self, subject: &RegistrationReceipt, bundle: &str, rows: &[T]) {
        let path = self.root.join("bundles").join(bundle);
        emit_rows(self.server, &self.admin, subject, &path, rows).await;
    }

    /// A window matching the baseline distribution passes PSI on both
    /// features, and a Custom mean exactly at its threshold is no drift.
    ///
    /// # Panics
    /// Panics when either run is not a pass with the expected features.
    async fn assert_baseline_like_passes(
        &self,
        steady: &RegistrationReceipt,
        psi: &RegistrationReceipt,
        custom: &RegistrationReceipt,
        rows: &[Features],
    ) {
        self.emit(steady, "steady", rows).await;
        let (result, features) = self.run(psi, steady, self.start, self.end).await;
        assert_eq!(result.verdict, "passed", "{result:?}");
        assert_eq!(
            verdicts(&features),
            [("latency", "Psi", "no_drift"), ("tier", "Psi", "no_drift")]
        );
        let (result, features) = self.run(custom, steady, self.start, self.end).await;
        assert_eq!(
            result.verdict, "passed",
            "a mean at the threshold is no drift"
        );
        assert_eq!(verdicts(&features), [("score", "Custom", "no_drift")]);
    }

    /// Two complete in-control subgroups pass SPC with zero signals, and
    /// the same subject with two more rows ends in a partial subgroup, which
    /// leaves the run unscored rather than shifted or dropped.
    ///
    /// # Panics
    /// Panics when the calm run does not pass with SPC evidence or the
    /// partial run is scored.
    async fn assert_calm_spc_passes_until_partial(&self, spc: &RegistrationReceipt) {
        let calm = self.subject("calm").await;
        let rows = |latencies: &[f64]| -> Vec<Features> {
            latencies
                .iter()
                .map(|latency| Features {
                    latency: *latency,
                    tier: "gold".to_owned(),
                    score: 1.0,
                })
                .collect()
        };
        let subgroup = [48.0, 49.0, 50.0, 51.0, 52.0];
        self.emit(&calm, "calm", &rows(&[subgroup, subgroup].concat()))
            .await;
        let (result, features) = self.run(spc, &calm, self.start, self.end).await;
        assert_eq!(result.verdict, "passed", "{result:?}");
        assert_eq!(verdicts(&features), [("latency", "Spc", "no_drift")]);
        assert_spc_evidence(&result, 2, 0);

        self.emit(&calm, "calm-partial", &rows(&[50.0, 50.0])).await;
        assert_unscored(&self.run(spc, &calm, self.start, self.end).await);
    }

    /// Records carrying no PSI feature are unrelated and leave a passing
    /// window passing; one selected record omitting a configured feature
    /// makes the run inconclusive with no details or feature rows.
    ///
    /// # Panics
    /// Panics when unrelated records change the verdict or an incomplete
    /// record is scored.
    async fn assert_incomplete_is_unscored(&self, psi: &RegistrationReceipt, rows: &[Features]) {
        let gappy = self.subject("gappy").await;
        self.emit(&gappy, "gappy", rows).await;
        let unrelated: Vec<ScoreOnly> = (0..5).map(|_| ScoreOnly { score: 9.0 }).collect();
        self.emit(&gappy, "gappy-unrelated", &unrelated).await;
        let (result, _) = self.run(psi, &gappy, self.start, self.end).await;
        assert_eq!(
            result.verdict, "passed",
            "records without a configured feature do not enter PSI: {result:?}"
        );

        self.emit(&gappy, "gappy-omitted", &[LatencyOnly { latency: 50.0 }])
            .await;
        assert_unscored(&self.run(psi, &gappy, self.start, self.end).await);
    }

    /// A PSI result scored before its Verifier version's fitted profile is
    /// retired stays readable afterwards, while a new run of that version is
    /// refused with `baseline_legacy` and produces no result.
    ///
    /// # Panics
    /// Panics when the historical run or result changes, or the legacy run
    /// is scored.
    async fn assert_legacy_profile_is_refused(
        &self,
        psi: &RegistrationReceipt,
        subject: &RegistrationReceipt,
    ) {
        let verification = verifier_of(self.server, subject).await;
        let request = direct(psi, subject, self.start, self.end);
        let historical = verification
            .start_run(&request, None)
            .await
            .expect("run starts");
        let before = wait_settled(&verification, &historical).await;
        let result_id = before.result_id.expect("a completed run names its result");
        let (scored, _) = read_result(self.server, &self.query, &result_id.to_string()).await;
        assert_eq!(scored.verdict, "passed", "{scored:?}");

        VerificationFixture::provision(
            self.server.state().postgres.wyrd(),
            self.server.pg_fixture().data_tenant_id(),
        )
        .await
        .expect("fixture tenant opens")
        .retire_fitted_format(psi.root.uid.as_ref().expect("verifier has a UID"))
        .await
        .expect("fitted profile retires");

        let legacy = verification
            .start_run(&request, None)
            .await
            .expect("run starts");
        let refused = wait_settled(&verification, &legacy).await;
        assert_eq!(
            refused.status,
            VerificationExecutionStatus::Errored,
            "{refused:?}"
        );
        assert_eq!(
            refused.error.as_ref().map(|error| error.code.as_str()),
            Some("baseline_legacy"),
            "{refused:?}"
        );
        assert!(refused.result_id.is_none(), "a refused run is never scored");

        let after = verification.get_run(&historical).await.expect("run reads");
        assert_eq!(after.result_id, before.result_id);
        let (reread, _) = read_result(self.server, &self.query, &result_id.to_string()).await;
        assert_eq!(
            (reread.verdict.as_str(), reread.details.as_deref()),
            (scored.verdict.as_str(), scored.details.as_deref()),
            "the historical report reads unchanged"
        );
        assert!(evidence(&reread, "tier")["Psi"]["bins"].is_array());
    }

    /// Three rows are below PSI's minimum sample, so PSI is inconclusive per
    /// feature, and form only a partial SPC subgroup, so SPC is unscored.
    ///
    /// # Panics
    /// Panics when PSI is not inconclusive per feature or SPC is scored.
    async fn assert_sparse_is_inconclusive(
        &self,
        psi: &RegistrationReceipt,
        spc: &RegistrationReceipt,
        rows: &[Features],
    ) {
        let sparse = self.subject("sparse").await;
        self.emit(&sparse, "sparse", rows).await;
        let (result, features) = self.run(psi, &sparse, self.start, self.end).await;
        assert_eq!(result.verdict, "inconclusive", "{result:?}");
        assert_eq!(
            verdicts(&features),
            [
                ("latency", "Psi", "inconclusive"),
                ("tier", "Psi", "inconclusive")
            ],
            "three rows are below PSI's minimum sample"
        );
        assert_unscored(&self.run(spc, &sparse, self.start, self.end).await);
    }

    /// Two client batches of one and three rows average per row, not per
    /// batch, and a window bounded between them excludes the other batch.
    ///
    /// # Panics
    /// Panics when any Custom verdict differs from the expected one.
    async fn assert_mean_is_per_row_within_window(&self, custom: &RegistrationReceipt) {
        let weighted = self.subject("weighted").await;
        let scores = |values: &[f64]| -> Vec<Features> {
            values
                .iter()
                .map(|score| Features {
                    latency: 50.0,
                    tier: "gold".to_owned(),
                    score: *score,
                })
                .collect()
        };
        self.emit(&weighted, "weighted-a", &scores(&[1.0])).await;
        let split = Utc::now();
        self.emit(&weighted, "weighted-b", &scores(&[2.0, 2.0, 2.0]))
            .await;
        let (result, _) = self.run(custom, &weighted, self.start, self.end).await;
        assert_eq!(
            result.verdict, "failed",
            "rows average to 1.75; averaging the two batches would give 1.5"
        );
        let (result, _) = self.run(custom, &weighted, self.start, split).await;
        assert_eq!(
            result.verdict, "passed",
            "the window end excludes the second batch"
        );
        let (result, _) = self.run(custom, &weighted, split, self.end).await;
        assert_eq!(
            result.verdict, "failed",
            "the window start excludes the first batch"
        );
    }

    /// A text-valued Custom metric completes inconclusive before scoring.
    ///
    /// # Panics
    /// Panics when the run is scored.
    async fn assert_text_is_unscored(&self, custom: &RegistrationReceipt) {
        let text = self.subject("text").await;
        let rows: Vec<TextScore> = (0..3)
            .map(|_| TextScore {
                score: "high".to_owned(),
            })
            .collect();
        self.emit(&text, "text", &rows).await;
        assert_unscored(&self.run(custom, &text, self.start, self.end).await);
    }
}

/// Prove each direct run scores server-side and persists its result rows.
///
/// PSI, SPC, and Custom each fail the drifted window with their feature rows;
/// Custom over an empty window is inconclusive with null details and no
/// feature rows.
///
/// # Panics
/// Panics when a run or its persisted rows differ from the expected outcome.
async fn assert_direct_scores(
    server: &WyrdTestServer,
    verification: &Verification,
    query: &Bifrost,
    [psi, spc, custom, empty]: [StartVerificationRunRequest; 4],
    subject: &str,
) {
    let (result, features) = complete(server, verification, query, &psi).await;
    assert_eq!(
        (result.execution_status.as_str(), result.verdict.as_str()),
        ("completed", "failed"),
        "{result:?}"
    );
    assert_eq!(result.subject_card_uid, subject);
    assert!(result.binding_id.is_none(), "a direct run has no binding");
    assert!(
        result.details.is_some(),
        "a scored report persists its details"
    );
    assert_eq!(
        features
            .iter()
            .map(|row| (
                row.feature.as_str(),
                row.method.as_str(),
                row.verdict.as_str()
            ))
            .collect::<Vec<_>>(),
        [("latency", "Psi", "drift"), ("tier", "Psi", "drift")],
        "{features:?}"
    );
    let tier = evidence(&result, "tier");
    assert_eq!(tier["Psi"]["sample"], 120, "{tier}");
    let bins = tier["Psi"]["bins"].as_array().expect("PSI bins");
    assert_eq!(
        bins.iter()
            .map(|bin| (
                bin["bin"]["categorical_value"].clone(),
                bin["target_count"].clone()
            ))
            .collect::<Vec<_>>(),
        [
            (serde_json::json!("gold"), serde_json::json!(0)),
            (serde_json::json!("silver"), serde_json::json!(0)),
            (serde_json::Value::Null, serde_json::json!(120)),
        ],
        "every unseen tier lands in the reserved other bin: {tier}"
    );

    let (result, features) = complete(server, verification, query, &spc).await;
    assert_eq!(result.verdict, "failed", "{result:?}");
    assert_eq!(verdicts(&features), [("latency", "Spc", "drift")]);
    assert_spc_evidence(&result, 24, 24);

    let (result, features) = complete(server, verification, query, &custom).await;
    assert_eq!(result.verdict, "failed", "{result:?}");
    assert_eq!(features.len(), 1, "{features:?}");

    let (result, features) = complete(server, verification, query, &empty).await;
    assert_eq!(
        (result.execution_status.as_str(), result.verdict.as_str()),
        ("completed", "inconclusive"),
        "{result:?}"
    );
    assert!(
        result.details.is_none(),
        "an unscored window has null details"
    );
    assert!(features.is_empty(), "an unscored window writes no features");
}

/// Assert `result`'s SPC evidence for `latency`: subgroups of five, the
/// number of complete `subgroups`, and `x_bar_signals` on the X-bar chart.
///
/// The baseline's twenty subgroups of consecutive integers fix the X-bar
/// center at 49.5 and the S center at `sqrt(2.5)`; the persisted limits must
/// be the NIST X-bar/S limits around them.
///
/// # Panics
/// Panics when the evidence is missing or differs.
fn assert_spc_evidence(result: &ResultRow, subgroups: u64, x_bar_signals: u64) {
    let spc = &evidence(result, "latency")["Spc"];
    assert_eq!(spc["subgroup_size"], 5, "{spc}");
    assert_eq!(spc["subgroups"], subgroups, "{spc}");
    assert_eq!(spc["x_bar"]["signals"], x_bar_signals, "{spc}");
    let number = |value: &serde_json::Value| value.as_f64().expect("a number");
    let s_bar = 2.5_f64.sqrt();
    let c4 = 0.939_985_6;
    let width = 3.0 * s_bar / (c4 * 5.0_f64.sqrt());
    assert!(
        (number(&spc["x_bar"]["center"]) - 49.5).abs() < 1e-9,
        "{spc}"
    );
    assert!(
        (number(&spc["x_bar"]["upper"]) - (49.5 + width)).abs() < 1e-5,
        "{spc}"
    );
    assert!((number(&spc["s"]["center"]) - s_bar).abs() < 1e-9, "{spc}");
    assert_eq!(spc["s"]["lower"], 0.0, "B3 is zero for subgroups of five");
}

/// Scheduled and manual activation of the Services sharing the `drift-daily`
/// Trigger, observed through the public run status and Bifrost results.
struct SharedTrigger<'a> {
    /// The bound server, whose fixture tenant owns both bindings.
    server: &'a WyrdTestServer,
    /// Admin Card handle used to read each Service's binding status.
    cards: &'a Cards,
    /// Verification handle of the first Service's credential.
    verification: &'a Verification,
    /// Admin query handle over the tenant's result tables.
    query: &'a Bifrost,
}

/// One settled binding-created run and the identities it reported.
struct BindingRun {
    /// The binding that created the run.
    binding: BindingId,
    /// Canonical result of the run.
    result: VerificationResultId,
    /// Every dispatch the run created, in identity order.
    dispatches: Vec<OperatorDispatchState>,
}

impl SharedTrigger<'_> {
    /// Read the single binding of `service`.
    ///
    /// # Panics
    /// Panics when the Service cannot be read or serves no binding.
    async fn binding(&self, service: &RegistrationReceipt) -> BindingId {
        self.cards
            .get(CardSelector::exact(service.root.clone()))
            .await
            .expect("service reads")
            .status
            .and_then(|status| status.verification)
            .expect("the binding owner serves verification status")
            .binding_ids[0]
    }

    /// Make every binding of `services` due at one occurrence of the shared
    /// Trigger and prove each runs once, isolated, and dispatches only its own
    /// configured Operators.
    ///
    /// The due instant is `PostgreSQL`'s statement time, so the daily
    /// occurrence's window is `[midnight UTC, now)` and holds every
    /// observation just emitted. Each pair is a Service and its Operator
    /// count. Returns the settled runs in `services` order.
    ///
    /// # Panics
    /// Panics when a binding cannot be made due, an occurrence schedules the
    /// wrong number of runs, or a run's identities, verdict, SPC evidence, or
    /// dispatches are wrong.
    async fn assert_one_occurrence_runs_each_binding(
        &self,
        services: [(&RegistrationReceipt, usize); 2],
    ) -> Vec<BindingRun> {
        let seed = VerificationFixture::provision(
            self.server.state().postgres.wyrd(),
            self.server.pg_fixture().data_tenant_id(),
        )
        .await
        .expect("fixture tenant opens");
        let earlier = seed.runs().await.expect("runs read");
        let mut bindings = Vec::new();
        for (service, _) in services {
            let binding = self.binding(service).await;
            seed.make_binding_due(binding)
                .await
                .expect("binding is due");
            bindings.push(binding);
        }
        let deadline = tokio::time::Instant::now() + WAIT;
        let runs = loop {
            let scheduled: Vec<_> = seed
                .runs()
                .await
                .expect("runs read")
                .into_iter()
                .filter(|run| !earlier.contains(run))
                .collect();
            if scheduled.len() >= services.len() {
                break scheduled;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the due occurrence scheduled {scheduled:?}"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        assert_eq!(runs.len(), services.len(), "one run per binding: {runs:?}");

        let mut settled = Vec::new();
        for run in &runs {
            settled.push(self.settle_failed(run).await);
        }
        settled.sort_by_key(|run| bindings.iter().position(|binding| *binding == run.binding));
        for ((service, operators), run) in services.iter().zip(&settled) {
            assert_eq!(
                run.dispatches.len(),
                *operators,
                "only this binding's Operators"
            );
            let (result, _) = read_result(self.server, self.query, &run.result.to_string()).await;
            assert_spc_evidence(&result, 24, 24);
            assert_eq!(
                result.subject_card_uid,
                service
                    .root
                    .uid
                    .as_ref()
                    .expect("service has a UID")
                    .to_string(),
                "each binding verifies its own Service"
            );
        }
        assert_ne!(settled[0].binding, settled[1].binding);
        assert_ne!(settled[0].result, settled[1].result);
        assert!(
            settled[0].dispatches.iter().all(|a| settled[1]
                .dispatches
                .iter()
                .all(|b| a.operator != b.operator)),
            "no Operator crosses bindings"
        );
        settled
    }

    /// Run `service`'s binding manually over `window` and prove its failed
    /// result dispatches to the same Operators as `scheduled`, as new
    /// dispatches of a new result.
    ///
    /// # Panics
    /// Panics when the run cannot start, does not fail, or its dispatches do
    /// not match the binding's Operators.
    async fn assert_manual_binding_run_dispatches(
        &self,
        service: &RegistrationReceipt,
        scheduled: &BindingRun,
        window: (DateTime<Utc>, DateTime<Utc>),
    ) {
        let request: StartVerificationRunRequest = serde_json::from_value(serde_json::json!({
            "target": { "kind": "binding", "binding_id": self.binding(service).await },
            "input": { "kind": "drift_window", "start": window.0, "end": window.1 },
        }))
        .expect("run request matches the wire contract");
        let run = self
            .verification
            .start_run(&request, None)
            .await
            .expect("manual binding run starts");
        let manual = self.settle_failed(&run).await;
        assert_eq!(manual.binding, scheduled.binding);
        assert_ne!(
            manual.result, scheduled.result,
            "a manual run has its own result"
        );
        assert!(
            manual
                .dispatches
                .iter()
                .map(|dispatch| &dispatch.operator)
                .eq(scheduled
                    .dispatches
                    .iter()
                    .map(|dispatch| &dispatch.operator)),
            "the binding's Operator path"
        );
        assert!(
            manual.dispatches.iter().all(|a| scheduled
                .dispatches
                .iter()
                .all(|b| a.dispatch_id != b.dispatch_id)),
            "a manual run creates its own dispatches"
        );
    }

    /// Wait for `run` to complete with a failed binding result.
    ///
    /// # Panics
    /// Panics when the run does not complete, names no result or binding, or
    /// its verdict is not failed.
    async fn settle_failed(&self, run: &VerificationRunId) -> BindingRun {
        let status = wait_settled(self.verification, run).await;
        assert_eq!(
            status.status,
            VerificationExecutionStatus::Completed,
            "{status:?}"
        );
        let result_id = status.result_id.expect("a completed run names its result");
        let (result, _) = read_result(self.server, self.query, &result_id.to_string()).await;
        assert_eq!(result.verdict, "failed", "{result:?}");
        let binding = result
            .binding_id
            .as_deref()
            .expect("a binding run records its binding")
            .parse()
            .expect("binding id parses");
        BindingRun {
            binding,
            result: result_id,
            dispatches: status.dispatches,
        }
    }
}

/// Drive the refusals a real caller hits around Drift verification.
///
/// A non-Parquet baseline and an SPC profile carrying the retired
/// `weco_rule` field are refused at registration, a caller without
/// `evals:run` cannot start `request`, and a second tenant can neither run the
/// first tenant's Verifier nor read its results.
///
/// # Panics
/// Panics when any refusal succeeds or carries the wrong code.
async fn assert_refusals(
    server: &WyrdTestServer,
    root: &Path,
    cards: &Cards,
    request: &StartVerificationRunRequest,
) {
    std::fs::write(
        root.join("custom-baseline.yaml"),
        "apiVersion: wyrd/v1\nkind: Data\nmetadata:\n  name: custom-baseline\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Custom\n    meta:\n      loader_module: fixture\n      loader_class: Data\n      extra: {}\n  schema:\n    columns: []\n  stats:\n    byte_count: 0\n    sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n",
    )
    .expect("custom baseline writes");
    register(cards, &root.join("custom-baseline.yaml")).await;
    std::fs::write(
        root.join("drift-psi-custom.yaml"),
        verifier_yaml(
            "drift-psi-custom",
            &format!(
                "      method: Psi\n{}      profile:\n        kind: Psi\n        binning_strategy:\n          kind: EqualWidth\n          n_bins: 10\n        threshold:\n          kind: Fixed\n          value: 0.25\n",
                distribution("latency").replace("drift-baseline", "custom-baseline")
            ),
        ),
    )
    .expect("verifier card writes");
    let refused = Box::pin(cards.register_from_path(&root.join("drift-psi-custom.yaml")))
        .await
        .expect_err("a non-Parquet baseline is refused");
    assert_eq!(refused.code(), "WYRD_DRIFT_400_VALIDATION", "{refused:?}");

    std::fs::write(
        root.join("drift-spc-weco.yaml"),
        verifier_yaml(
            "drift-spc-weco",
            &format!(
                "      method: Spc\n{}      profile:\n        kind: Spc\n        sample_size: 5\n        weco_rule:\n          rule_string: \"8 16 4 8 2 4 1 1\"\n",
                distribution("latency")
            ),
        ),
    )
    .expect("verifier card writes");
    let retired = Box::pin(cards.register_from_path(&root.join("drift-spc-weco.yaml")))
        .await
        .expect_err("a retired SPC profile field is refused");
    assert!(
        format!("{retired:?}").contains("weco_rule"),
        "the refusal names the retired field: {retired:?}"
    );

    let reader = Verification::with_client(connect(
        server,
        &api_key(
            server
                .bootstrap_service("rust_drift_reader", &["reader"])
                .await
                .expect("reader bootstraps"),
        ),
    ));
    let denied = reader
        .start_run(request, None)
        .await
        .expect_err("a caller without evals:run is refused");
    assert_eq!(denied.status(), 403);

    let other = server
        .seed_tenant("drift-other")
        .await
        .expect("second tenant seeds");
    let other = connect(
        server,
        &api_key(
            server
                .bootstrap_service_in_tenant(other, "rust_drift_other", &["admin"])
                .await
                .expect("second tenant admin bootstraps"),
        ),
    );
    let foreign = Verification::with_client(WyrdClient::clone(&other))
        .start_run(request, None)
        .await
        .expect_err("another tenant cannot run this tenant's Verifier");
    assert_eq!(
        foreign.code(),
        "WYRD_VERIFICATION_400_INVALID_TARGET",
        "{foreign:?}"
    );
    let leaked = Bifrost::query_only(&other)
        .sql_as::<ResultRow>(
            "SELECT execution_status, verdict, details, subject_card_uid, binding_id \
             FROM vala.verification.results",
        )
        .await
        .expect_err("a tenant that never verified has no results table to read");
    assert_eq!(
        wyrd_sdk::verification::WyrdError::from(&leaked).code(),
        "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
        "no result crosses tenants: {leaked:?}"
    );
}
