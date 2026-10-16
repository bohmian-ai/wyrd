//! A scheduled drift check over submitted samples fails and its Operator
//! alerts the on-call hook on the origin its connection supplies; a passing
//! window alerts no one.
//!
//! The story follows the full scheduled lifecycle: register the baseline and
//! the Service, let the real fitter fit the baseline, `observe.submit` the
//! samples, make the schedule due, read the retained result back, and
//! receive the Operator's delivery exactly once.

use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};
use wyrd_ai::cards::RegistrationReceipt;
use wyrd_ai::operator_connections::{
    ConnectionName, CreateOperatorConnectionRequest, HttpConnectionAuth, HttpsOrigin,
    OperatorConnections, SecretBearer,
};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::Submission;
use wyrd_ai::{
    CallProvenance, RequirementDetail, RequirementOutcome, VerificationResult, VerificationVerdict,
};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, fixture, hydrate, register, registered};

/// The bearer token the `on-call-hooks` connection sends to the hook.
const ON_CALL_TOKEN: &str = "on-call-hook-token";

/// How long the story waits for the baseline to fit, for the result, and for
/// the alert.
const WAIT: Duration = Duration::from_secs(90);

/// How long the story listens for an alert that must not arrive.
const QUIET: Duration = Duration::from_secs(5);

/// The local on-call hook: answers 200 and hands every request it receives
/// to the story.
struct Hook(UnboundedSender<Request>);

impl Respond for Hook {
    /// Forward `request` to the story and acknowledge it.
    fn respond(&self, request: &Request) -> ResponseTemplate {
        // The story may have finished and dropped its receiver.
        let _ = self.0.send(request.clone());
        ResponseTemplate::new(200)
    }
}

/// The `latency-watch` deployment: the on-call hook, a verification-capable
/// server whose `on-call-hooks` connection points at the hook, and the
/// registered Service with its fitted baseline.
struct Watch {
    /// The deployment.
    deployment: Deployment,
    /// The local hook; kept so it keeps listening.
    _hook: MockServer,
    /// Every request the hook receives.
    alerts: UnboundedReceiver<Request>,
    /// The Service registration.
    service: RegistrationReceipt,
}

impl Watch {
    /// Start the hook and the server, create the connection, register the
    /// baseline and the Service, and wait for the real fitter to fit the
    /// `latency-shift` baseline.
    ///
    /// # Panics
    /// Panics when a setup step fails or the baseline does not fit in time.
    async fn start() -> Self {
        let hook = MockServer::start().await;
        let (sender, alerts) = unbounded_channel();
        Mock::given(wiremock::matchers::any())
            .respond_with(Hook(sender))
            .mount(&hook)
            .await;
        let deployment =
            Deployment::start_with(WyrdTestServer::builder().with_verification_runtime_for_test())
                .await;
        OperatorConnections::connect_with(&deployment.admin())
            .await
            .expect("handle connects")
            .create(&CreateOperatorConnectionRequest::Http {
                name: ConnectionName::new("on-call-hooks").expect("connection name"),
                origin: HttpsOrigin::parse(&hook.uri()).expect("hook origin parses"),
                auth: HttpConnectionAuth::Bearer {
                    token: SecretBearer::new(ON_CALL_TOKEN.to_owned()),
                },
            })
            .await
            .expect("connection creates");
        let cards = deployment.cards().await;
        register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
        let watch = register(
            &cards,
            "cards/scheduled_drift_alerts_operator/latency-watch.yaml",
        )
        .await;
        deployment
            .server()
            .wait_for_baseline(
                registered(&watch, "latency-shift")
                    .uid
                    .as_ref()
                    .expect("registered Verifier has a UID"),
                WAIT,
            )
            .await
            .expect("the latency baseline fits");
        Self {
            deployment,
            _hook: hook,
            alerts,
            service: watch,
        }
    }

    /// Submit every latency in `latencies` as the Service, assert each was
    /// recorded for the schedule rather than judged, and drain the samples.
    ///
    /// # Panics
    /// Panics when hydration, loading, a submission, or the drain fails, or
    /// a submission queued an execution.
    async fn submit(&self, latencies: impl IntoIterator<Item = u32>) {
        let cards = self.deployment.cards().await;
        let bundle = hydrate(&cards, &self.service.root).await;
        let watcher = self
            .deployment
            .client(&self.deployment.service_key(&self.service.root).await);
        let state =
            WyrdState::from_path_with(bundle.path().join("bundle"), watcher).expect("bundle loads");
        state.start_bifrost().await.expect("Bifrost starts");
        let run = state.run();
        for latency in latencies {
            let submitted = run
                .observe()
                .submit("latency-shift", json!({ "latency": latency }))
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

    /// Assert the hook receives nothing for [`QUIET`].
    ///
    /// # Panics
    /// Panics when an alert arrives.
    async fn assert_quiet(&mut self) {
        if let Ok(Some(alert)) = tokio::time::timeout(QUIET, self.alerts.recv()).await {
            panic!("no alert is due: {} {}", alert.method, alert.url);
        }
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
        watch.alerts.try_recv().is_err(),
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

    let alert = tokio::time::timeout(WAIT, watch.alerts.recv())
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
    watch.assert_quiet().await;
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
    watch.assert_quiet().await;
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

    let result = watch.judge().await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Inconclusive);
    watch.assert_quiet().await;
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
