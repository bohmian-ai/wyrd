//! A Service's run submits samples for the Cards it composes; they read back
//! by run id, stamped with the Card and run they belong to.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;
use tempfile::TempDir;
use wyrd_ai::bifrost::BifrostOptions;
use wyrd_ai::bifrost::QueryParam;
use wyrd_ai::state::{ConfigureOptions, WyrdState};
use wyrd_ai::verify::Submission;
use wyrd_ai::{Bifrost, QueueConfig, RequirementDetail, VerificationVerdict, WyrdClient};
use wyrd_testing::server::WyrdTestServer;

use crate::support::{Deployment, declare, pull};

/// One `vala.drift.observations` row, as read back.
#[derive(Debug, PartialEq, Deserialize)]
struct DriftRow {
    /// The feature name.
    series: String,
    /// The numeric projection.
    num_value: Option<f64>,
    /// The canonical text projection.
    str_value: Option<String>,
    /// The session of the sample; `submit` names none.
    session_id: Option<String>,
    /// The observed Card.
    card_uid: String,
    /// The run every row of one invocation shares.
    run_id: String,
}

/// A deployment with the observed Service registered and hydrated, and the
/// Card bundle's directory.
///
/// # Panics
/// Panics when registration or hydration fails.
async fn observed() -> (Deployment, TempDir) {
    let deployment = Deployment::start().await;
    let cards = deployment.cards().await;
    declare(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let service = declare(&cards, "cards/observe_a_run/observed-service.yaml").await;
    let bundle = pull(&cards, &service.root).await;
    (deployment, bundle)
}

/// The Card bundle under `dir`, loaded offline.
///
/// # Panics
/// Panics when the bundle does not load.
fn state(dir: &TempDir) -> WyrdState {
    WyrdState::from_path(dir.path().join("bundle")).expect("bundle loads offline")
}

/// The Card bundle under `dir`, loaded with its server calls fixed to
/// `client`.
///
/// # Panics
/// Panics when the bundle does not load.
fn state_as(dir: &TempDir, client: WyrdClient) -> WyrdState {
    WyrdState::from_path_with(dir.path().join("bundle"), client).expect("bundle loads offline")
}

/// The observed Service's own client: its family runtime account, run as the
/// Service registered in `dir`'s bundle.
///
/// # Panics
/// Panics when the bundle does not load or the account is not provisioned.
async fn service_client(deployment: &Deployment, dir: &TempDir) -> WyrdClient {
    let service = state(dir).root_ref().clone();
    deployment.service_client(&service).await
}

/// Samples submitted for the `observed-drift` Verifier bound to the run's
/// Model are recorded for its schedule rather than judged, and read back by
/// run id stamped with the Model.
///
/// # Panics
/// Panics when a step fails or the rows differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn run_observations_read_back_by_run_id() {
    let (deployment, dir) = observed().await;
    let state = state_as(&dir, service_client(&deployment, &dir).await);
    state.start_bifrost().await.expect("Bifrost starts");
    let run = state.run();

    let submitted = run
        .for_card("model")
        .expect("model view resolves")
        .observe()
        .submit("observed-drift", json!({ "latency": 12.5 }))
        .await
        .expect("the samples are submitted");
    assert!(matches!(submitted, Submission::Recorded), "{submitted:?}");
    state.shutdown().await.expect("samples drain");
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");

    let run_id = run.run_id().to_string();
    let rows: Vec<DriftRow> = Bifrost::connect_with(&deployment.admin(), BifrostOptions::default())
        .await
        .expect("Bifrost connects")
        .sql_as(
            "SELECT series, num_value, str_value, session_id, card_uid, run_id \
             FROM vala.drift.observations WHERE run_id = $1 ORDER BY series",
            &[QueryParam::String(run_id.clone())],
        )
        .await
        .expect("rows read back");
    let model = state
        .card_ref("model")
        .expect("model resolves")
        .uid
        .as_ref()
        .expect("hydrated Card carries its UID")
        .to_string();
    assert_eq!(
        rows,
        [DriftRow {
            series: "latency".to_owned(),
            num_value: Some(12.5),
            str_value: Some("12.5".to_owned()),
            session_id: None,
            card_uid: model,
            run_id,
        }]
    );
    deployment.shutdown().await;
}

/// A latency far from the Custom drift target, submitted for the Model, fails
/// `observed-drift` when its schedule comes due; the result carries the
/// Custom delta for the latency metric.
///
/// # Panics
/// Panics when a step fails or the result differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn custom_drift_judges_the_submitted_window_on_schedule() {
    let deployment =
        Deployment::start_with(WyrdTestServer::builder().with_verification_runtime_for_test())
            .await;
    let cards = deployment.cards().await;
    declare(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let service = declare(&cards, "cards/observe_a_run/observed-service.yaml").await;
    let dir = pull(&cards, &service.root).await;
    let state = state_as(&dir, service_client(&deployment, &dir).await);
    state.start_bifrost().await.expect("Bifrost starts");
    state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .submit("observed-drift", json!({ "latency": 30.0 }))
        .await
        .expect("the sample is recorded");
    state.shutdown().await.expect("samples drain");
    let binding = deployment.binding(&service.root).await;
    deployment
        .server()
        .make_binding_due(binding)
        .await
        .expect("the binding comes due");

    let result = deployment
        .scheduled_result(&binding, Duration::from_secs(90))
        .await;

    assert_eq!(result.outcome.verdict, VerificationVerdict::Failed);
    assert_eq!(
        &result.subject,
        state.card_ref("model").expect("model resolves")
    );
    let [requirement] = &result.outcome.requirements[..] else {
        panic!("one requirement: {:?}", result.outcome.requirements);
    };
    let Some(RequirementDetail::Drift(report)) = &requirement.detail else {
        panic!("a drift requirement carries its report: {requirement:?}");
    };
    assert!(
        report
            .features
            .values()
            .any(|feature| feature.score.is_some_and(|delta| delta > 5.0)),
        "{report:?}"
    );
    deployment.shutdown().await;
}

/// A run's views name their alias and share its run id; a run opened for
/// another alias is a different run.
///
/// # Panics
/// Panics when an alias does not resolve or a view differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn run_view_exposes_its_alias() {
    let (deployment, dir) = observed().await;
    let state = state(&dir);

    let run = state.run();
    let model = run.for_card("model").expect("model view resolves");
    let agent = state.run_for_card("agent").expect("agent run opens");

    assert_eq!(
        [run.alias(), model.alias(), agent.alias()],
        ["root", "model", "agent"]
    );
    assert_eq!(model.run_id(), run.run_id());
    assert_ne!(agent.run_id(), run.run_id());
    deployment.shutdown().await;
}

/// A key whose Card scope does not cover the Model cannot write the Model's
/// samples; the refusal surfaces when the submission drains.
///
/// # Panics
/// Panics when the drain succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn card_scoped_key_cannot_write_another_cards_observations() {
    let (deployment, dir) = observed().await;
    let state = state_as(&dir, deployment.admin());
    state.start_bifrost().await.expect("Bifrost starts");

    state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .submit("observed-drift", json!({ "latency": 12.5 }))
        .await
        .expect("the samples enqueue");
    let refused = state.shutdown().await.expect_err("the write is refused");

    assert_eq!(refused.code(), "WYRD_VALA_403_BIFROST_CARD_SCOPE");
    deployment.shutdown().await;
}

/// A client byte budget too small to seal one message is refused when
/// Bifrost starts.
///
/// # Panics
/// Panics when the start succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn unsealable_byte_budget_is_refused_at_connect() {
    let (deployment, dir) = observed().await;
    let state = state_as(&dir, service_client(&deployment, &dir).await);

    let refused = state
        .start_bifrost_with_config(None, QueueConfig::with_client_byte_limit(Some(1024)))
        .await
        .expect_err("the budget is refused");

    assert_eq!(refused.code(), "WYRD_CLIENT_400_CONFIG_INVALID");
    deployment.shutdown().await;
}

/// Two states of one Service, configured for `prod` and `staging`, verify the
/// Service-bound `service-check` (whose binding selects `prod`) on the root
/// and the unscoped Agent-bound `agent-check` on the Agent component. Each
/// result names its own subject and the effective environment, directly and
/// queued; the `staging` state's root check conflicts with the binding and is
/// refused before any execution; a late configure is refused.
///
/// # Panics
/// Panics when a step fails or a result names another subject or
/// environment.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn configured_service_context_reaches_both_verification_subjects() {
    let deployment =
        Deployment::start_with(WyrdTestServer::builder().with_verification_runtime_for_test())
            .await;
    let cards = deployment.cards().await;
    let service = declare(&cards, "cards/observe_a_run/configured-service.yaml").await;
    let dir = pull(&cards, &service.root).await;
    let root = state(&dir).root_ref().clone();
    let mut states = Vec::new();
    for environment in ["prod", "staging"] {
        let state = state_as(&dir, deployment.service_client(&root).await);
        state
            .configure(ConfigureOptions {
                deployment_environment: Some(environment.to_owned()),
                ..ConfigureOptions::default()
            })
            .expect("configure succeeds before use");
        states.push((state, environment));
    }

    for (state, environment) in &states {
        let run = state.run();
        let agent = run.for_card("agent").expect("agent view resolves");
        let result = agent
            .observe()
            .verify("agent-check", json!("yes"))
            .await
            .expect("the Agent check runs");
        assert_eq!(&result.subject, agent.subject());
        assert_eq!(result.deployment_environment.as_deref(), Some(*environment));
        let Submission::Queued(execution) = agent
            .observe()
            .submit("agent-check", json!("yes"))
            .await
            .expect("the Agent check enqueues")
        else {
            panic!("Run evidence queues an execution");
        };
        let queued = execution
            .wait(Duration::from_mins(1))
            .await
            .expect("the queued Agent check completes");
        assert_eq!(&queued.subject, agent.subject());
        assert_eq!(queued.deployment_environment.as_deref(), Some(*environment));

        let root = run.observe().verify("service-check", json!("yes")).await;
        if *environment == "prod" {
            let result = root.expect("the Service check runs");
            assert_eq!(&result.subject, state.root_ref());
            assert_eq!(result.deployment_environment.as_deref(), Some("prod"));
        } else {
            let refused = root.expect_err("the staging Service check conflicts");
            assert_eq!(refused.code(), "WYRD_SPEC_400_VALIDATION");
            assert_eq!(refused.problem().details["field"], "deployment_environment");
        }
        assert_eq!(
            state
                .configure(ConfigureOptions::default())
                .expect_err("configure after use is refused")
                .code(),
            "WYRD_SDK_409_STATE_CONFIG_FROZEN"
        );
    }
    deployment.shutdown().await;
}
