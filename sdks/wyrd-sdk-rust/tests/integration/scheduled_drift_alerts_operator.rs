//! A scheduled drift check over submitted samples fails and its Operator
//! alerts the on-call hook on the origin its connection supplies; a passing
//! window alerts no one. A second Service control-charts the same latency
//! against its fitted SPC baseline.
//!
//! The story follows the full scheduled lifecycle: register the baseline and
//! the Service, let the real fitter fit the baseline, `observe.submit` the
//! samples, make the schedule due, read the retained result back, and
//! receive the Operator's delivery exactly once.

use std::time::Duration;

use serde_json::{Value, json};
use wyrd_ai::cards::{CardKind, RegistrationReceipt};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Inputs, Submission};
use wyrd_ai::{
    CallProvenance, RequirementDetail, RequirementOutcome, VerificationResult, VerificationVerdict,
};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, Hooks, fixture, hydrate, register, registered};

/// The bearer token the `on-call-hooks` connection sends to the hook.
const ON_CALL_TOKEN: &str = "on-call-hook-token";

/// How long the story waits for the baseline to fit, for the result, and for
/// the alert.
const WAIT: Duration = Duration::from_secs(90);

/// How long the story listens for an alert that must not arrive.
const QUIET: Duration = Duration::from_secs(5);

/// The `latency-watch` deployment: the on-call hook, a verification-capable
/// server whose `on-call-hooks` connection points at the hook, and the
/// registered Service with its fitted baseline.
struct Watch {
    /// The deployment.
    deployment: Deployment,
    /// The local on-call hook the connection points at.
    hooks: Hooks,
    /// The Service registration.
    service: RegistrationReceipt,
    /// The scheduled Verifier every sample is submitted to.
    verifier: &'static str,
}

impl Watch {
    /// Start the `latency-watch` story with its PSI `latency-shift` Verifier.
    ///
    /// # Panics
    /// Panics when a setup step fails or the baseline does not fit in time.
    async fn start() -> Self {
        Self::start_with("latency-watch.yaml", "latency-shift").await
    }

    /// Start the story's `service` Card with [`Self::deploy`] and wait for the
    /// real fitter to fit its scheduled `verifier`'s baseline.
    ///
    /// # Panics
    /// Panics when a setup step fails or the baseline does not fit in time.
    async fn start_with(service: &str, verifier: &'static str) -> Self {
        let watch = Self::deploy(service, verifier).await;
        watch
            .deployment
            .server()
            .wait_for_baseline(
                registered(&watch.service, verifier)
                    .uid
                    .as_ref()
                    .expect("registered Verifier has a UID"),
                WAIT,
            )
            .await
            .expect("the latency baseline fits");
        watch
    }

    /// Start the hook and the server, create the connection, and register the
    /// baseline and the story's `service` Card scheduling `verifier`.
    ///
    /// # Panics
    /// Panics when a setup step fails.
    async fn deploy(service: &str, verifier: &'static str) -> Self {
        let deployment =
            Deployment::start_with(WyrdTestServer::builder().with_verification_runtime_for_test())
                .await;
        let hooks = deployment.hooks("on-call-hooks", ON_CALL_TOKEN).await;
        let cards = deployment.cards().await;
        register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
        let watch = register(
            &cards,
            &format!("cards/scheduled_drift_alerts_operator/{service}"),
        )
        .await;
        Self {
            deployment,
            hooks,
            service: watch,
            verifier,
        }
    }

    /// Submit every latency in `latencies` as the Service through
    /// [`Self::record`].
    ///
    /// # Panics
    /// Panics when [`Self::record`] does.
    async fn submit(&self, latencies: impl IntoIterator<Item = u32>) {
        self.record(latencies.into_iter().map(|latency| json!(latency).into()))
            .await;
    }

    /// Submit every sample in `samples` as the Service, assert each was recorded for the schedule rather than judged,
    /// and drain the samples.
    ///
    /// # Panics
    /// Panics when hydration, loading, a submission, or the drain fails, or
    /// a submission queued an execution.
    async fn record(&self, samples: impl IntoIterator<Item = Inputs>) {
        let cards = self.deployment.cards().await;
        let bundle = hydrate(&cards, &self.service.root).await;
        let watcher = self
            .deployment
            .client(&self.deployment.service_key(&self.service.root).await);
        let state =
            WyrdState::from_path_with(bundle.path().join("bundle"), watcher).expect("bundle loads");
        state.start_bifrost().await.expect("Bifrost starts");
        let run = state.run();
        for sample in samples {
            let submitted = run
                .observe()
                .submit(self.verifier, sample)
                .await
                .expect("the sample is submitted");
            assert!(matches!(submitted, Submission::Recorded), "{submitted:?}");
        }
        state.shutdown().await.expect("samples drain");
        self.deployment
            .server()
            .flush_bifrost()
            .await
            .expect("rows publish");
    }

    /// Make the Service's one binding due and read back the result the real
    /// scheduler, queue, and runner retain for it.
    ///
    /// # Panics
    /// Panics when the binding does not come due or no result is retained.
    async fn judge(&self) -> VerificationResult {
        let binding = self.deployment.binding(&self.service.root).await;
        self.deployment
            .server()
            .make_binding_due(binding)
            .await
            .expect("the binding comes due");
        self.deployment.scheduled_result(&binding, WAIT).await
    }
}

/// A shifted window of submitted latencies is recorded without judging, then
/// fails its scheduled `latency-shift` run against the fitted baseline; the
/// retained result names the Service, the requirement, the window, the
/// baseline, and the PSI statistics, and the path-only Operator POSTs once to
/// the connection's origin with its bearer token.
///
/// # Panics
/// Panics when a step fails, the result differs, or the alert is missing,
/// early, duplicated, or differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn failed_schedule_alerts_its_operator_on_the_connection_origin() {
    let mut watch = Watch::start().await;
    watch.submit(std::iter::repeat_n(99, 100)).await;
    assert!(
        watch.hooks.received.try_recv().is_err(),
        "submitting alone runs no check and alerts no one"
    );

    let result = watch.judge().await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(result.subject, watch.service.root);
    assert_eq!(result.verifier.name.as_str(), "latency-shift");
    assert_eq!(result.run_id, None, "a scheduled execution has no Run");
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    assert_eq!(
        (requirement.id.as_str(), requirement.outcome),
        ("latency-unshifted", RequirementOutcome::Failed)
    );
    assert!(
        matches!(
            &requirement.provenance,
            Some(CallProvenance::Drift { window: Some(window) }) if window.start < window.end
        ),
        "{:?}",
        requirement.provenance
    );
    let check = serde_json::to_value(&requirement.check).expect("the check serializes");
    assert_eq!(
        check["signal"]["baseline_ref"]["name"], "latency-baseline",
        "{check}"
    );
    let Some(RequirementDetail::Drift(report)) = &requirement.detail else {
        panic!("a drift requirement carries its report: {requirement:?}");
    };
    let latency = report
        .features
        .iter()
        .find(|(name, _)| name.as_str() == "latency")
        .map(|(_, feature)| feature)
        .expect("the latency feature is scored");
    assert!(
        latency.score.is_some_and(|score| score > 0.25),
        "{latency:?}"
    );

    let alert = tokio::time::timeout(WAIT, watch.hooks.received.recv())
        .await
        .expect("an alert arrives in time")
        .expect("the hook is still listening");
    assert_eq!(
        (alert.method.as_str(), alert.url.path()),
        ("POST", "/hooks/latency-shift")
    );
    assert_eq!(
        alert
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some(format!("Bearer {ON_CALL_TOKEN}").as_str())
    );
    let body: Value = serde_json::from_slice(&alert.body).expect("the alert body is JSON");
    assert!(
        body.to_string().contains(&result.execution_id.to_string()),
        "the alert names the failed execution: {body}"
    );
    watch.hooks.assert_quiet(QUIET).await;
    watch.deployment.shutdown().await;
}

/// A window spread like the healthy baseline's 0 to 99 ms passes its
/// scheduled run, and the failure Operator is never invoked.
///
/// # Panics
/// Panics when a step fails, the verdict differs, or an alert arrives.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn passing_window_alerts_no_one() {
    let mut watch = Watch::start().await;
    watch.submit((0..100).map(|row| (row * 37) % 100)).await;

    let result = watch.judge().await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Passed);
    watch.hooks.assert_quiet(QUIET).await;
    watch.deployment.shutdown().await;
}

/// A due schedule with no submitted samples is inconclusive, never a pass,
/// and the failure Operator is never invoked.
///
/// # Panics
/// Panics when a step fails, the verdict differs, or an alert arrives.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn empty_window_is_inconclusive_and_alerts_no_one() {
    let mut watch = Watch::start().await;
    // The Service runs and authenticates, so its binding is active, but
    // submits nothing.
    watch.submit(std::iter::empty::<u32>()).await;

    let result = watch.judge().await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Inconclusive);
    watch.hooks.assert_quiet(QUIET).await;
    watch.deployment.shutdown().await;
}

/// Twenty latencies of 99 ms, four complete subgroups of five, are charted
/// on schedule against the X-bar/S limits fitted from the healthy 0 to 99 ms
/// baseline: every subgroup mean lies above the X-bar limit around its 49.5
/// center while no subgroup spreads, so `latency-in-control` fails with four
/// X-bar signals, no S signal, and its chart evidence.
///
/// # Panics
/// Panics when a step fails or the result or its SPC detail differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn ready_spc_baseline_charts_a_scheduled_window() {
    let watch = Watch::start_with("latency-steady.yaml", "latency-control").await;
    watch.submit(std::iter::repeat_n(99, 20)).await;

    let result = watch.judge().await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(result.verifier.name.as_str(), "latency-control");
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    assert_eq!(
        (requirement.id.as_str(), requirement.outcome),
        ("latency-in-control", RequirementOutcome::Failed)
    );
    let check = serde_json::to_value(&requirement.check).expect("the check serializes");
    assert_eq!(check["method"], "Spc", "{check}");
    let Some(RequirementDetail::Drift(report)) = &requirement.detail else {
        panic!("a drift requirement carries its report: {requirement:?}");
    };
    let latency = report
        .features
        .iter()
        .find(|(name, _)| name.as_str() == "latency")
        .map(|(_, feature)| feature)
        .expect("the latency feature is scored");
    assert_eq!(
        (latency.score, latency.threshold),
        (Some(4.0), Some(0.0)),
        "{latency:?}"
    );
    let chart = &latency
        .evidence
        .as_ref()
        .expect("a scored SPC feature carries its chart")["Spc"];
    assert_eq!(
        (
            &chart["subgroup_size"],
            &chart["subgroups"],
            &chart["x_bar"]["center"],
            &chart["x_bar"]["signals"],
            &chart["s"]["signals"],
        ),
        (&json!(5), &json!(4), &json!(49.5), &json!(4), &json!(0)),
        "{chart}"
    );
    watch.deployment.shutdown().await;
}

/// An Operator with only a path and no connection to supply its origin is
/// refused at registration.
///
/// # Panics
/// Panics when registration succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn path_only_operator_without_connection_is_refused() {
    let deployment = Deployment::start().await;

    let refused = deployment
        .cards()
        .await
        .register_from_path(&fixture("invalid/operator-path-without-connection.yaml"))
        .await
        .expect_err("a path-only Operator needs a connection");

    assert_eq!(refused.code(), "WYRD_SPEC_400_INVALID_OPERATOR");
    deployment.shutdown().await;
}

/// A schedule binding with an `inputs` map, empty or repeating the Verifier's
/// own samples features, and a samples slot with `required: false` are each
/// refused at that field, locally by the SDK and by the server over raw HTTP,
/// and none of the refused Cards persists.
///
/// # Panics
/// Panics when a refused shape registers, a refusal does not name its field,
/// or a refused Card is readable.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn scheduled_binding_rejects_redundant_inputs() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards().await;
    for (file, field) in [
        (
            "scheduled-binding-empty-inputs.yaml",
            "verified_by[0].inputs",
        ),
        (
            "scheduled-binding-exact-inputs.yaml",
            "verified_by[0].inputs",
        ),
        ("optional-samples-slot.yaml", "inputs.latency"),
    ] {
        let refused = cards
            .register_from_path(&fixture(&format!("invalid/{file}")))
            .await
            .expect_err("a removed scheduled-samples field is refused");
        assert!(
            format!("{refused:?}").contains(field),
            "{file}: {refused:?}"
        );
    }

    let token = deployment
        .admin()
        .access_token()
        .await
        .expect("the administrator token mints");
    let budget = json!({
        "verifier": { "kind": "Verifier", "name": "latency-error-budget",
                      "version": "1.0.0", "space": "default" },
        "runs_on": { "kind": "schedule", "cron": "0 0 * * *" },
    });
    let mut exact = budget.clone();
    exact["inputs"] = json!({ "latency": { "features": ["latency"] } });
    let mut empty = budget;
    empty["inputs"] = json!({});
    let optional = json!({
        "inputs": { "latency": { "type": "samples", "features": ["latency"], "required": false } },
        "requirements": [{
            "id": "latency-within-budget",
            "statement": "Mean request latency stays at or below 100 ms",
            "check": {
                "kind": "drift", "on": "inputs.latency", "method": "Custom",
                "signal": { "kind": "Metric", "name": "latency" },
                "condition": { "kind": "Statistical" },
                "profile": { "kind": "Custom", "metric_name": "latency",
                             "baseline_value": 0.0, "alert_threshold": 100.0 }
            }
        }]
    });
    for (kind, name, spec, field) in [
        (
            "Service",
            "raw-empty-inputs",
            json!({ "verified_by": [empty] }),
            "verified_by[0].inputs",
        ),
        (
            "Service",
            "raw-exact-inputs",
            json!({ "verified_by": [exact] }),
            "verified_by[0].inputs",
        ),
        (
            "Verifier",
            "raw-optional-samples",
            optional,
            "inputs.latency",
        ),
    ] {
        let response = reqwest::Client::new()
            .post(format!(
                "{}/v1/cards",
                deployment
                    .server()
                    .base_url()
                    .expect("bound server has a URL")
            ))
            .header("x-wyrd-access-token", format!("Bearer {}", token.expose()))
            .header("idempotency-key", name)
            .json(&json!({ "submissions": [{
                "apiVersion": "wyrd/v1",
                "kind": kind,
                "metadata": { "name": name, "version": "1.0.0", "space": "default" },
                "spec": spec,
            }] }))
            .send()
            .await
            .expect("the server answers");
        assert_eq!(response.status(), 400, "{name}");
        let problem: Value = response.json().await.expect("the refusal is problem JSON");
        assert_eq!(
            problem["code"], "WYRD_REGISTRY_400_INVALID_CARD_SPEC",
            "{problem}"
        );
        assert!(
            problem["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains(field)),
            "{name}: {problem}"
        );
        let kind = if kind == "Service" {
            CardKind::Service
        } else {
            CardKind::Verifier
        };
        let absent = cards
            .resolve_latest(
                kind,
                "default".parse().expect("space parses"),
                name.parse().expect("name parses"),
            )
            .await
            .expect_err("a refused Card does not persist");
        assert_eq!(absent.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    }
    deployment.shutdown().await;
}

/// One binding with no `inputs` judges every drift requirement its Verifier
/// declares: 120 ms latency and a 0.01 error rate fail latency and pass the
/// error rate, so the window fails; 80 ms latency with no error-rate samples
/// passes latency and leaves the error rate inconclusive, so the window is
/// inconclusive rather than a pass over latency alone.
///
/// # Panics
/// Panics when a step fails or a requirement is missing or differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn scheduled_binding_scores_every_requirement() {
    for (sample, verdict, outcomes) in [
        (
            Inputs::named([("latency", json!(120)), ("error_rate", json!(0.01))]),
            VerificationVerdict::Failed,
            [RequirementOutcome::Failed, RequirementOutcome::Passed],
        ),
        (
            Inputs::named([("latency", json!(80))]),
            VerificationVerdict::Inconclusive,
            [RequirementOutcome::Passed, RequirementOutcome::Inconclusive],
        ),
    ] {
        let watch = Watch::deploy("latency-budget.yaml", "latency-error-budget").await;
        watch.record(std::iter::repeat_n(sample, 10)).await;

        let result = watch.judge().await;

        assert_eq!(result.outcome.verdict, verdict);
        assert_eq!(result.subject, watch.service.root);
        assert_eq!(result.verifier.name.as_str(), "latency-error-budget");
        let mut judged: Vec<_> = result
            .outcome
            .requirements
            .iter()
            .map(|requirement| {
                assert!(
                    matches!(
                        &requirement.provenance,
                        Some(CallProvenance::Drift { window: Some(window) }) if window.start < window.end
                    ),
                    "{:?}",
                    requirement.provenance
                );
                (requirement.id.as_str(), requirement.outcome)
            })
            .collect();
        judged.sort_by_key(|(id, _)| *id);
        assert_eq!(
            judged,
            [
                ("error-rate-within-budget", outcomes[1]),
                ("latency-within-budget", outcomes[0]),
            ]
        );
        watch.deployment.shutdown().await;
    }
}
