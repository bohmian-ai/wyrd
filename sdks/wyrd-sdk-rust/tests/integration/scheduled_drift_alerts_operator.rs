//! A scheduled drift check fails and its Operator alerts the on-call hook on
//! the origin its connection supplies.

use std::time::Duration;

use serde::Serialize;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};
use wyrd_sdk::cards::CardSelector;
use wyrd_sdk::operator_connections::{
    ConnectionName, CreateOperatorConnectionRequest, HttpConnectionAuth, HttpsOrigin,
    OperatorConnections, SecretBearer,
};
use wyrd_sdk::state::WyrdState;
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, fixture, hydrate, register, registered};

/// The bearer token the `on-call-hooks` connection sends to the hook.
const ON_CALL_TOKEN: &str = "on-call-hook-token";

/// How long the story waits for the baseline to fit and for the alert.
const WAIT: Duration = Duration::from_secs(90);

/// One observed request latency.
#[derive(Serialize)]
struct Latency {
    /// Milliseconds.
    latency: u32,
}

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

/// A failed scheduled run of `latency-shift` makes its path-only Operator
/// POST to the connection's origin with the connection's bearer token.
///
/// # Panics
/// Panics when a step fails, no alert arrives in time, or the alert differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn failed_schedule_alerts_its_operator_on_the_connection_origin() {
    let hook = MockServer::start().await;
    let (alerts, mut received) = unbounded_channel();
    Mock::given(wiremock::matchers::any())
        .respond_with(Hook(alerts))
        .mount(&hook)
        .await;
    let deployment =
        Deployment::start_with(WyrdTestServer::builder().with_verification_runtime_for_test())
            .await;
    OperatorConnections::with_client(deployment.admin())
        .create(&CreateOperatorConnectionRequest::Http {
            name: ConnectionName::new("on-call-hooks").expect("connection name"),
            origin: HttpsOrigin::parse(&hook.uri()).expect("hook origin parses"),
            auth: HttpConnectionAuth::Bearer {
                token: SecretBearer::new(ON_CALL_TOKEN.to_owned()),
            },
        })
        .await
        .expect("connection creates");
    let cards = deployment.cards();
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
    let bundle = hydrate(&cards, &watch.root).await;
    let watcher = deployment.client(&deployment.service_key(&watch.root).await);
    let state = WyrdState::from_path_with_client(bundle.path().join("bundle"), watcher)
        .expect("bundle loads");
    state.start_bifrost().await.expect("Bifrost starts");
    let run = state.run();
    for _ in 0..100 {
        run.observe()
            .drift(&Latency { latency: 99 }, None)
            .expect("drift emits");
    }
    state.shutdown().await.expect("emits drain");
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");
    let binding = cards
        .get(CardSelector::exact(watch.root.clone()))
        .await
        .expect("the watch reads")
        .status
        .and_then(|status| status.verification)
        .expect("the watch serves verification status")
        .binding_ids
        .into_iter()
        .next()
        .expect("the watch has one binding");

    deployment
        .server()
        .make_binding_due(binding)
        .await
        .expect("the binding comes due");

    let alert = tokio::time::timeout(WAIT, received.recv())
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
    deployment.shutdown().await;
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
        .register_from_path(&fixture("invalid/operator-path-without-connection.yaml"))
        .await
        .expect_err("a path-only Operator needs a connection");

    assert_eq!(refused.code(), "WYRD_SPEC_400_INVALID_OPERATOR");
    deployment.shutdown().await;
}
