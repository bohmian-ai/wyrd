//! A Service's run emits observations for the Cards it composes; they read
//! back by run id, stamped with the Card and run they belong to.

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use wyrd_sdk::bifrost::QueryParam;
use wyrd_sdk::state::WyrdState;
use wyrd_sdk::{Bifrost, QueueConfig};
use wyrd_spec::vala::ids::SessionId;

use crate::support::{Deployment, hydrate, register};

/// The session the drift emit belongs to.
const SESSION: &str = "0190f5a4-8c3e-7b21-9d4f-3a6b2c1d0e9f";

/// The drift features one emit projects into one row each.
#[derive(Serialize)]
struct Features {
    /// A numeric series.
    latency: f64,
    /// A categorical series.
    tier: &'static str,
}

/// One `vala.drift.observations` row, as read back.
#[derive(Debug, PartialEq, Deserialize)]
struct DriftRow {
    /// The feature name.
    series: String,
    /// The numeric projection; `None` for a categorical series.
    num_value: Option<f64>,
    /// The canonical text projection.
    str_value: Option<String>,
    /// The session the emit named.
    session_id: Option<String>,
    /// The observed Card.
    card_uid: String,
    /// The run every row of one invocation shares.
    run_id: String,
}

/// A deployment with the observed Service registered and hydrated, and the
/// hydrated bundle's directory.
///
/// # Panics
/// Panics when registration or hydration fails.
async fn observed() -> (Deployment, TempDir) {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    register(&cards, "cards/observe_a_run/observed-model.yaml").await;
    let service = register(&cards, "cards/observe_a_run/observed-service.yaml").await;
    let bundle = hydrate(&cards, &service.root).await;
    (deployment, bundle)
}

/// The hydrated bundle under `dir`, loaded offline.
///
/// # Panics
/// Panics when the bundle does not load.
fn state(dir: &TempDir) -> WyrdState {
    WyrdState::from_path(&dir.path().join("bundle")).expect("bundle loads offline")
}

/// A drift emit on the run's Model view reads back by run id, one row per
/// feature, stamped with the Model and the session.
///
/// # Panics
/// Panics when a step fails or the rows differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn run_observations_read_back_by_run_id() {
    let (deployment, dir) = observed().await;
    let state = state(&dir);
    let service = deployment
        .service_key(state.root_ref(), &[])
        .await;
    state
        .start_bifrost_with(&deployment.client(&service), None)
        .await
        .expect("Bifrost starts");
    let run = state.run();

    run.for_card("model")
        .expect("model view resolves")
        .observe()
        .drift(
            &Features {
                latency: 12.5,
                tier: "gold",
            },
            Some(SessionId(SESSION.parse().expect("session parses"))),
        )
        .expect("drift emits");
    state.shutdown().await.expect("emits drain");
    deployment
        .server()
        .flush_bifrost()
        .await
        .expect("rows publish");

    let run_id = run.run_id().to_string();
    let rows: Vec<DriftRow> = Bifrost::query_only(&deployment.admin())
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
    let row = |series: &str, num_value, str_value: &str| DriftRow {
        series: series.to_owned(),
        num_value,
        str_value: Some(str_value.to_owned()),
        session_id: Some(SESSION.to_owned()),
        card_uid: model.clone(),
        run_id: run_id.clone(),
    };
    assert_eq!(
        rows,
        [row("latency", Some(12.5), "12.5"), row("tier", None, "gold")]
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
/// observations; the refusal surfaces when the emit drains.
///
/// # Panics
/// Panics when the drain succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn card_scoped_key_cannot_write_another_cards_observations() {
    let (deployment, dir) = observed().await;
    let state = state(&dir);
    state
        .start_bifrost_with(&deployment.admin(), None)
        .await
        .expect("Bifrost starts");

    state
        .run_for_card("model")
        .expect("model run opens")
        .observe()
        .drift(
            &Features {
                latency: 12.5,
                tier: "gold",
            },
            None,
        )
        .expect("drift enqueues");
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
    let state = state(&dir);
    let service = deployment.service_key(state.root_ref(), &[]).await;

    let refused = state
        .start_bifrost_with_config(
            &deployment.client(&service),
            None,
            QueueConfig::with_client_byte_limit(Some(1024)),
        )
        .await
        .expect_err("the budget is refused");

    assert_eq!(refused.code(), "WYRD_CLIENT_400_CONFIG_INVALID");
    deployment.shutdown().await;
}
