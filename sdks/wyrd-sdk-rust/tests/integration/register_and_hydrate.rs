//! A team registers its support desk Service graph and hydrates it to run
//! offline, through `Cards` and through the in-process CLI.

use std::path::{Path, PathBuf};

use wyrd_ai::cards::{CardKind, CardRef, CardSelector, Cards, RegistrationReceipt};
use wyrd_ai::cli::{self, SelectorArgs};
use wyrd_ai::state::{HydratedCard, WyrdState};
use wyrd_ai::{Card, Spec, VerifierImplementation};

use crate::support::{Deployment, fixture, hydrate, register, registered};

/// The support desk Service the stories register.
const DESK: &str = "cards/register_and_hydrate/support-desk.yaml";

/// Register the artifact-bearing support Model, then the desk graph that
/// references it, and return both receipts in that order.
///
/// # Panics
/// Panics when a registration is refused.
async fn register_desk(cards: &Cards) -> (RegistrationReceipt, RegistrationReceipt) {
    let model = register(cards, "cards/register_and_hydrate/support-model.yaml").await;
    (model, register(cards, DESK).await)
}

/// The Service whose Agents declare their Prompt inline, by path and by
/// CardRef, beside an explicit `my_prompt` alias of the shared Prompt.
const PROMPT_DESK: &str = "cards/agent_prompt/prompt-desk.yaml";

/// Register the authored Prompt the ref-form Agent pins, then the prompt
/// desk, and return the desk's receipt.
///
/// # Panics
/// Panics when a registration is refused.
async fn register_prompt_desk(cards: &Cards) -> RegistrationReceipt {
    register(cards, "cards/agent_prompt/desk-prompt.yaml").await;
    register(cards, PROMPT_DESK).await
}

/// Write an unversioned Agent named `name` whose inline Prompt sends
/// `message` into `dir`, and return its path.
///
/// # Panics
/// Panics when the file cannot be written.
fn inline_agent(dir: &Path, name: &str, message: &str) -> PathBuf {
    let path = dir.join(format!("{name}-{}.yaml", message.len()));
    std::fs::write(
        &path,
        format!(
            "apiVersion: wyrd/v1
kind: Agent
metadata:
  name: {name}
  space: default
spec:
  prompt:
    request:
      provider: gemini_generate_content
      body:
        contents:
          - role: user
            parts:
              - text: {message}
    model: gpt-4o
    variables: []
    media_variables: []
    response_type: text
"
        ),
    )
    .expect("inline agent writes");
    path
}

/// Register the inline Agent `name` sending `message` and return the exact
/// references the receipt records for the Agent and its Prompt.
///
/// # Panics
/// Panics when registration is refused or records either Card missing.
async fn register_inline(
    cards: &Cards,
    dir: &Path,
    name: &str,
    message: &str,
) -> (CardRef, CardRef) {
    let receipt = cards
        .register_from_path(inline_agent(dir, name, message))
        .await
        .expect("inline agent registers");
    (
        registered(&receipt, name),
        registered(&receipt, &format!("{name}-prompt")),
    )
}

/// The Prompt reference the registry persisted in Agent `agent`'s spec.
///
/// # Panics
/// Panics when the Agent cannot be read or pins no Card reference.
async fn persisted_prompt(cards: &Cards, agent: &CardRef) -> CardRef {
    let card = cards
        .get(CardSelector::exact(agent.clone()))
        .await
        .expect("agent reads");
    let Spec::Agent(spec) = card.spec else {
        panic!("{agent} is an Agent");
    };
    spec.prompt
        .as_card_ref()
        .expect("the persisted Agent pins a registered Prompt")
        .clone()
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

    let state = WyrdState::from_path(bundle.path().join("bundle")).expect("bundle loads offline");
    assert_eq!(state.root_ref(), &desk.root);
    assert_eq!(
        state.card("agent").expect("agent resolves").kind,
        CardKind::Agent
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

/// The latest Active version of the registered desk resolves to its exact
/// reference, and a name never registered is not found.
///
/// # Panics
/// Panics when the resolve fails or differs, or the unknown name is not
/// refused with the not-found code.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn latest_version_resolves_to_the_registered_card() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let (_, desk) = register_desk(&cards).await;

    let latest = cards
        .resolve_latest(
            CardKind::Service,
            "default".parse().expect("space parses"),
            "support-desk".parse().expect("name parses"),
        )
        .await
        .expect("the latest desk resolves");
    let refused = cards
        .resolve_latest(
            CardKind::Service,
            "default".parse().expect("space parses"),
            "never-registered".parse().expect("name parses"),
        )
        .await
        .expect_err("an unregistered name is refused");

    assert_eq!(latest, desk.root);
    assert_eq!(refused.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
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
    let bindings = |card: Card| {
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
        .map(|card| card.kind.clone())
        .collect::<Vec<_>>();
    kinds.sort();
    let mut expected = vec![
        CardKind::Agent,
        CardKind::Data,
        CardKind::Model,
        CardKind::Operator,
        CardKind::Prompt,
        CardKind::Service,
        CardKind::Trigger,
        CardKind::Verifier,
    ];
    expected.sort();
    assert_eq!(kinds, expected);
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

    assert_eq!(model.root.kind, CardKind::Model);
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

    let refused = deployment
        .cards()
        .register_from_path(&fixture("invalid/wrong-artifact-digest/support-model.yaml"))
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

    let refused = deployment
        .cards()
        .register_from_path(&fixture("invalid/retired-drift-kind.yaml"))
        .await
        .expect_err("a retired kind is refused");

    assert_eq!(refused.code(), "WYRD_LOADER_400_INVALID_ENVELOPE");
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

    let refused = reader
        .register_from_path(&fixture(DESK))
        .await
        .expect_err("a reader cannot register");

    assert_eq!(refused.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
    deployment.shutdown().await;
}

/// `wyrd apply` and `wyrd get`, run in process as the administrator,
/// register the same graph and hydrate a loadable bundle.
///
/// # Panics
/// Panics when a CLI command fails or the bundle differs from the graph.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn cli_apply_and_get_round_trip_the_graph() {
    let deployment = Deployment::start().await;
    let (_, desk) = register_desk(&deployment.cards()).await;
    let dir = tempfile::tempdir().expect("bundle directory creates");
    let bundle = dir.path().join("bundle");

    let applied = cli::apply(&fixture(DESK), Some(deployment.admin()))
        .await
        .expect("apply registers");
    let summary = cli::get(
        &service_selector("support-desk"),
        &bundle,
        false,
        Some(deployment.admin()),
    )
    .await
    .expect("get hydrates");

    assert_eq!(applied.root, desk.root);
    assert_eq!(summary.root, desk.root);
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
    let deployment = Deployment::start().await;
    let dir = tempfile::tempdir().expect("bundle directory creates");

    let refused = cli::get(
        &service_selector("no-such-service"),
        &dir.path().join("bundle"),
        false,
        Some(deployment.admin()),
    )
    .await
    .expect_err("an unknown Service is refused");

    assert_eq!(refused.code(), "WYRD_REGISTRY_404_CARD_NOT_FOUND");
    deployment.shutdown().await;
}

/// An inline Prompt registers as its own `<agent>-prompt` Card that replay
/// reuses, changed content versions while older Agents stay pinned, and
/// another Agent never shares; an independently authored line of the same
/// name and another tenant keep their own Cards; path and CardRef Prompts
/// keep their authored identity, and the generated Prompt hydrates without a
/// Service component.
///
/// # Panics
/// Panics when a registration, read, or hydration differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn agent_prompt_registration_identity() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let dir = tempfile::tempdir().expect("fixture directory creates");

    let (agent, prompt) =
        register_inline(&cards, dir.path(), "first-agent", "Answer briefly.").await;
    let replay = register_inline(&cards, dir.path(), "first-agent", "Answer briefly.").await;
    let (changed_agent, changed_prompt) =
        register_inline(&cards, dir.path(), "first-agent", "Answer at length.").await;
    let (_, other_prompt) =
        register_inline(&cards, dir.path(), "second-agent", "Answer briefly.").await;

    assert_eq!(prompt.kind, CardKind::Prompt);
    assert_eq!(prompt.space, agent.space);
    assert_eq!(persisted_prompt(&cards, &agent).await, prompt);
    assert_eq!(replay, (agent.clone(), prompt.clone()));
    assert_ne!(changed_prompt.version, prompt.version);
    assert_eq!(
        persisted_prompt(&cards, &changed_agent).await,
        changed_prompt
    );
    assert_eq!(persisted_prompt(&cards, &agent).await, prompt);
    assert_eq!(other_prompt.name.as_str(), "second-agent-prompt");
    assert_ne!(other_prompt.uid, prompt.uid);

    let authored = dir.path().join("clash-agent-prompt.yaml");
    std::fs::write(
        &authored,
        "apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: clash-agent-prompt
  version: 1.0.0
  space: default
spec:
  provider: openai
  model: gpt-4o
  messages: [Authored independently.]
",
    )
    .expect("authored prompt writes");
    let independent = cards
        .register_from_path(&authored)
        .await
        .expect("authored prompt registers");
    let (_, clash) = register_inline(&cards, dir.path(), "clash-agent", "Answer briefly.").await;
    assert_ne!(clash.version, independent.root.version);
    assert_ne!(clash.uid, independent.root.uid);

    let tenant = Cards::with_client(
        deployment.client(&deployment.other_tenant_admin("prompt-tenant").await),
    );
    let (_, foreign) = register_inline(&tenant, dir.path(), "first-agent", "Answer briefly.").await;
    assert_ne!(foreign.uid, prompt.uid);
    assert_eq!(
        cards
            .get(CardSelector::exact(foreign))
            .await
            .expect_err("another tenant's Prompt is invisible")
            .code(),
        "WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE"
    );

    let desk = register_prompt_desk(&cards).await;
    let authored = registered(&desk, "desk-prompt");
    let inline = registered(&desk, "inline-agent-prompt");
    assert_eq!(authored.version.to_string(), "1.0.0");
    for (name, expected) in [
        ("path-agent", &authored),
        ("ref-agent", &authored),
        ("inline-agent", &inline),
    ] {
        assert_eq!(
            &persisted_prompt(&cards, &registered(&desk, name)).await,
            expected
        );
    }
    let bundle = hydrate(&cards, &desk.root).await;
    let state = WyrdState::from_path(bundle.path().join("bundle")).expect("bundle loads");
    let aliases = state
        .aliases_for(&inline)
        .expect("the generated Prompt hydrates");
    assert_eq!(
        state
            .prompt(&aliases[0])
            .expect("the generated Prompt reads")
            .as_card_ref()
            .expect("prompt identity is valid"),
        inline
    );
    deployment.shutdown().await;
}

/// Every Agent of the hydrated prompt desk navigates offline to its
/// registered Prompt Card; path and CardRef forms agree with the explicit
/// `my_prompt` alias, generic lookup returns kind-appropriate Cards, and
/// unknown or mismatched aliases are refused with stable codes.
///
/// # Panics
/// Panics when registration, hydration, a read, or a refusal differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn hydrated_agent_prompt_card_navigation() {
    let deployment = Deployment::start().await;
    let cards = deployment.cards();
    let desk = register_prompt_desk(&cards).await;
    let bundle = hydrate(&cards, &desk.root).await;
    deployment.shutdown().await;

    let state = WyrdState::from_path(bundle.path().join("bundle")).expect("bundle loads offline");
    let explicit = state.prompt("my_prompt").expect("explicit alias reads");
    let explicit_ref = explicit.as_card_ref().expect("prompt identity is valid");
    assert_eq!(explicit_ref, registered(&desk, "desk-prompt"));
    for (alias, prompt) in [
        ("inline_agent", "inline-agent-prompt"),
        ("path_agent", "desk-prompt"),
        ("ref_agent", "desk-prompt"),
    ] {
        let agent = state.agent(alias).expect("agent reads");
        let card = agent.prompt();
        assert_eq!(
            card.as_card_ref().expect("prompt identity is valid"),
            registered(&desk, prompt)
        );
        assert_eq!(agent.prompt_ref(), &registered(&desk, prompt));
        let Ok(HydratedCard::Agent(generic)) = state.get(alias) else {
            panic!("{alias} is looked up as an Agent");
        };
        assert!(std::ptr::eq(generic.prompt(), card));
    }
    assert!(std::ptr::eq(
        state.agent("path_agent").expect("agent reads").prompt(),
        explicit
    ));
    assert!(
        matches!(state.get("my_prompt"), Ok(HydratedCard::Prompt(card)) if std::ptr::eq(card, explicit))
    );
    assert_eq!(
        state
            .get("missing")
            .err()
            .expect("unknown alias is refused")
            .code(),
        "WYRD_SDK_404_UNKNOWN_ALIAS"
    );
    assert_eq!(
        state
            .agent("my_prompt")
            .err()
            .expect("a Prompt is not an Agent")
            .code(),
        "WYRD_SDK_400_CARD_KIND_MISMATCH"
    );
}
