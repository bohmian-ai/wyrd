//! A team registers its support desk Service graph and hydrates it to run
//! offline, through `Cards` and through the in-process CLI.

use serde_json::json;
use wyrd_sdk::cards::{CardSelector, Cards, RegistrationReceipt};
use wyrd_sdk::cli::{self, SelectorArgs};
use wyrd_sdk::state::WyrdState;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::Spec;

use crate::support::{self, Deployment, fixture, hydrate, register};

/// The support desk Service the stories register.
const DESK: &str = "cards/register_and_hydrate/support-desk.yaml";

/// Register the artifact-bearing support Model, then the desk graph that
/// references it, and return both receipts in that order.
async fn register_desk(cards: &Cards) -> (RegistrationReceipt, RegistrationReceipt) {
    let model = register(cards, "cards/register_and_hydrate/support-model.yaml").await;
    (model, register(cards, DESK).await)
}

/// The CLI selector naming `name` at 1.0.0 in the default space.
fn service_selector(name: &str) -> SelectorArgs {
    SelectorArgs {
        kind: Some("Service".to_owned()),
        space: Some("default".to_owned()),
        name: Some(name.to_owned()),
        version: Some("1.0.0".to_owned()),
        uid: None,
    }
}

/// The hydrated desk loads offline with its agent and the Model's artifact
/// bytes exactly as checked in.
///
/// # Panics
/// Panics when registration, hydration, or an offline read differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn service_graph_registers_and_hydrates() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let (_, desk) = register_desk(&cards).await;

    let bundle = hydrate(&cards, &desk.root).await;

    let state = WyrdState::from_path(&bundle.path().join("bundle")).expect("bundle loads offline");
    assert_eq!(state.root_ref(), &desk.root);
    assert_eq!(
        state
            .card("agent")
            .expect("agent resolves")
            .kind
            .wire_name(),
        "Agent"
    );
    let artifacts = state.artifacts("model").expect("model artifacts resolve");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].relative_path(), "support-model.bin");
    assert_eq!(
        std::fs::read(artifacts[0].local_path()).expect("artifact reads"),
        std::fs::read(fixture("cards/register_and_hydrate/support-model.bin"))
            .expect("fixture reads")
    );
    deployment.shutdown().await;
}

/// Registering the unchanged graph again keeps its root and its binding
/// identities.
///
/// # Panics
/// Panics when the second registration or a read differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn registering_the_graph_again_is_idempotent() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let (_, desk) = register_desk(&cards).await;
    let bindings = |card: wyrd_spec::envelope::Card| {
        card.status
            .and_then(|status| status.verification)
            .expect("the desk serves verification status")
            .binding_ids
    };
    let before = bindings(
        cards
            .get(CardSelector::exact(desk.root.clone()))
            .await
            .expect("desk reads"),
    );

    let again = register(&cards, DESK).await;

    assert_eq!(again.root, desk.root);
    let after = bindings(
        cards
            .get(CardSelector::exact(again.root))
            .await
            .expect("desk reads"),
    );
    assert_eq!(after, before);
    deployment.shutdown().await;
}

/// `cards.get` returns each of the eight kinds the fixtures register as its
/// typed spec.
///
/// # Panics
/// Panics when a read fails or a kind or typed field differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cards_get_returns_every_kind_typed() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let baseline = register(&cards, "cards/latency_baseline/latency-baseline.yaml").await;
    let (model, desk) = register_desk(&cards).await;

    let mut read = Vec::new();
    for receipt in [&baseline, &model, &desk] {
        for outcome in &receipt.outcomes {
            read.push(
                cards
                    .get(CardSelector::exact(outcome.card_ref.clone()))
                    .await
                    .expect("registered Card reads"),
            );
        }
    }

    let mut kinds = read
        .iter()
        .map(|card| card.kind.wire_name())
        .collect::<Vec<_>>();
    kinds.sort_unstable();
    assert_eq!(
        kinds,
        [
            "Agent", "Data", "Model", "Operator", "Prompt", "Service", "Trigger", "Verifier"
        ]
    );
    assert!(read.iter().any(|card| matches!(
        &card.spec,
        Spec::Verifier(verifier) if matches!(verifier.implementation, VerifierImplementation::Eval(_))
    )));
    assert!(read.iter().any(|card| matches!(
        &card.spec,
        Spec::Data(data) if data.stats.row_count == Some(100)
    )));
    deployment.shutdown().await;
}

/// A Model whose artifact lists no digest registers, and the server records
/// the artifact hash it computed.
///
/// # Panics
/// Panics when registration is refused or records no artifact hash.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn artifact_without_digest_registers() {
    let deployment = Deployment::start().await;

    let model = register(
        &deployment.cards(),
        "cards/register_and_hydrate/support-model.yaml",
    )
    .await;

    assert_eq!(model.root.kind.wire_name(), "Model");
    assert!(model.outcomes[0].artifact_hash.is_some());
    deployment.shutdown().await;
}

/// An artifact whose declared digest does not match its bytes is refused.
///
/// # Panics
/// Panics when registration succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn wrong_artifact_digest_is_refused() {
    let deployment = Deployment::start().await;

    let refused = Box::pin(
        deployment
            .cards()
            .register_from_path(&fixture("invalid/wrong-artifact-digest/support-model.yaml")),
    )
    .await
    .expect_err("a wrong digest is refused");

    assert_eq!(refused.code(), "WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH");
    deployment.shutdown().await;
}

/// A Card of a retired kind is refused at registration.
///
/// # Panics
/// Panics when registration succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn retired_card_kind_is_refused() {
    let deployment = Deployment::start().await;

    let refused = Box::pin(
        deployment
            .cards()
            .register_from_path(&fixture("invalid/retired-drift-kind.yaml")),
    )
    .await
    .expect_err("a retired kind is refused");

    assert_eq!(refused.code(), "WYRD_REGISTRY_400_INVALID_CARD_SPEC");
    deployment.shutdown().await;
}

/// A caller holding only `cards:read` cannot register the graph.
///
/// # Panics
/// Panics when the registration succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn reader_cannot_register_cards() {
    let deployment = Deployment::start().await;
    register_desk(&deployment.cards()).await;
    let reader = Cards::with_client(
        deployment.client(&deployment.scoped_key("card_reader", &["cards:read"]).await),
    );

    let refused = Box::pin(reader.register_from_path(&fixture(DESK)))
        .await
        .expect_err("a reader cannot register");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// `wyrd apply` and `wyrd get`, run in process from a shell configured for
/// the deployment, register the same graph and hydrate a loadable bundle.
///
/// # Panics
/// Panics when a CLI command fails or the bundle differs from the graph.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cli_apply_and_get_round_trip_the_graph() {
    if support::is_child() {
        let applied = cli::apply(&fixture(DESK), None)
            .await
            .expect("apply registers");
        let bundle = std::env::var("BUNDLE").expect("parent names the bundle");
        let summary = cli::get(
            &service_selector("support-desk"),
            std::path::Path::new(&bundle),
            false,
            None,
        )
        .await
        .expect("get hydrates");
        support::report(&json!({ "applied": applied.root, "got": summary.root }));
        return;
    }
    let deployment = Deployment::start().await;
    let (_, desk) = register_desk(&deployment.cards()).await;
    let dir = tempfile::tempdir().expect("bundle directory creates");
    let bundle = dir.path().join("bundle");

    let outcome = deployment
        .run_child(
            "register_and_hydrate::cli_apply_and_get_round_trip_the_graph",
            &deployment.key("cli_admin", &["admin"]).await,
            &[("BUNDLE", bundle.to_str().expect("UTF-8 path"))],
        )
        .await;

    let root = serde_json::to_value(&desk.root).expect("ref serializes");
    assert_eq!(outcome, json!({ "applied": root, "got": root }));
    assert_eq!(
        WyrdState::from_path(&bundle)
            .expect("bundle loads")
            .root_ref(),
        &desk.root
    );
    deployment.shutdown().await;
}

/// A CLI command the server refuses raises `WyrdError` with its catalog code.
///
/// # Panics
/// Panics when the command succeeds or is refused with another code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn refused_cli_command_raises_its_catalog_code() {
    if support::is_child() {
        let dir = tempfile::tempdir().expect("bundle directory creates");
        let refused: wyrd_sdk::WyrdError = cli::get(
            &service_selector("no-such-service"),
            &dir.path().join("bundle"),
            false,
            None,
        )
        .await
        .expect_err("an unknown Service is refused");
        support::report(&json!(refused.code()));
        return;
    }
    let deployment = Deployment::start().await;

    let code = deployment
        .run_child(
            "register_and_hydrate::refused_cli_command_raises_its_catalog_code",
            &deployment.key("cli_admin", &["admin"]).await,
            &[],
        )
        .await;

    assert_eq!(code, "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    deployment.shutdown().await;
}
