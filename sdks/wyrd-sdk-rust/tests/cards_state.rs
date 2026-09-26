//! Rust SDK Card and `WyrdState` user journey through the public `wyrd_sdk` crate.
//!
//! Mirrors the TypeScript `cards-state` journey: register a Service graph,
//! read it back, hydrate complete and metadata-only bundles, stop the server,
//! and load the complete bundle offline. A standalone bound Agent proves its
//! served binding identities are stable `UUIDv7`s. Negative flows cover an
//! under-privileged credential, an unknown alias, and an unhydrated bundle.

use std::path::{Path, PathBuf};

use base64::Engine;
use secrecy::ExposeSecret;
use sha2::Digest;
use wyrd_sdk::bifrost::client_from_options;
use wyrd_sdk::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode};
use wyrd_sdk::state::WyrdState;
use wyrd_testing::Bootstrap;
use wyrd_testing::server::WyrdTestServer;

/// Payload of the single Prompt artifact every bundle must carry verbatim.
const PROMPT_ARTIFACT: &[u8] = b"shared-prompt-artifact";

/// Write a Service graph of one Agent and its artifact-bearing Prompt.
///
/// An artifact-bearing Card registers alone, so the caller registers the
/// Prompt first and the Service references it by exact identity.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_service_graph(root: &Path) -> PathBuf {
    let digest =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(PROMPT_ARTIFACT));
    std::fs::write(root.join("shared-prompt.txt"), PROMPT_ARTIFACT)
        .expect("prompt artifact writes");
    std::fs::write(
        root.join("shared-prompt.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: rust-shared-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: shared-prompt.txt\n    sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            PROMPT_ARTIFACT.len()
        ),
    )
    .expect("prompt card writes");
    std::fs::write(
        root.join("agent.yaml"),
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: rust-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt:\n    kind: Prompt\n    name: rust-shared-prompt\n    version: 1.0.0\n    space: default\n  run_config:\n    max_iterations: 2\n    timeout_ms: 1000\n",
    )
    .expect("agent card writes");
    let service = root.join("service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1\nkind: Service\nmetadata:\n  name: rust-hydrated-service\n  version: 1.0.0\n  space: default\nspec:\n  service_type: agent\n  components:\n    - alias: agent\n      ref: ./agent.yaml\n    - alias: prompt\n      ref:\n        kind: Prompt\n        name: rust-shared-prompt\n        version: 1.0.0\n        space: default\n",
    )
    .expect("service card writes");
    service
}

/// Write an eval Verifier and a standalone Agent bound to it.
///
/// The Agent is the smallest binding owner: one Agent-level binding running the
/// Verifier on an inline `observations_ready` Trigger. It reuses the shared
/// Prompt, so it registers after the Prompt. Returns the Verifier and Agent
/// paths in registration order.
///
/// # Panics
/// Panics when a fixture file cannot be written.
fn write_bound_agent(root: &Path) -> (PathBuf, PathBuf) {
    let verifier = root.join("verifier.yaml");
    std::fs::write(
        &verifier,
        "apiVersion: wyrd/v1\nkind: Verifier\nmetadata:\n  name: rust-eval\n  version: 1.0.0\n  space: default\nspec:\n  implementation:\n    kind: eval\n    spec:\n      tasks: {}\n",
    )
    .expect("verifier card writes");
    let agent = root.join("bound-agent.yaml");
    std::fs::write(
        &agent,
        "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: rust-bound-agent\n  version: 1.0.0\n  space: default\nspec:\n  prompt:\n    kind: Prompt\n    name: rust-shared-prompt\n    version: 1.0.0\n    space: default\n  verified_by:\n    - verifier:\n        kind: Verifier\n        name: rust-eval\n        version: 1.0.0\n        space: default\n      runs_on:\n        kind: observations_ready\n",
    )
    .expect("bound agent card writes");
    (verifier, agent)
}

/// Register the bound Agent twice and read its stable binding identities.
///
/// Reads the Agent through the public Cards handle after each registration and
/// asserts `status.verification.binding_ids` holds one `UUIDv7` that the
/// identical reapply keeps.
///
/// # Panics
/// Panics when a registration or read fails, or the served binding identities
/// are absent, not `UUIDv7`, or change on reapply.
async fn assert_stable_binding_ids(cards: &Cards, root: &Path) {
    let (verifier, agent) = write_bound_agent(root);
    Box::pin(cards.register_from_path(&verifier))
        .await
        .expect("verifier registers");
    let mut served = Vec::new();
    for _ in 0..2 {
        let receipt = Box::pin(cards.register_from_path(&agent))
            .await
            .expect("bound agent registers");
        let card = cards
            .get(CardSelector::exact(receipt.root))
            .await
            .expect("bound agent reads");
        let ids = card
            .status
            .and_then(|status| status.verification)
            .expect("a binding owner serves verification status")
            .binding_ids;
        assert_eq!(ids.len(), 1, "one Agent-level binding");
        assert_eq!(ids[0].as_uuid().get_version_num(), 7);
        served.push(ids);
    }
    assert_eq!(served[0], served[1], "reapply keeps binding identity");
}

/// Assert the offline view of a complete bundle and both offline refusals.
///
/// The server is already stopped, so every read proves `WyrdState` needs no
/// network: aliases, typed Cards, verified artifact bytes, an unknown alias,
/// and a metadata-only bundle that lacks payloads.
///
/// # Panics
/// Panics when any alias, artifact, or structured-error expectation fails.
fn assert_offline_state(state: &WyrdState, metadata_bundle: &Path) {
    let aliases = state.aliases().collect::<Vec<_>>();
    for alias in ["agent", "prompt", "root"] {
        assert!(aliases.contains(&alias), "bundle exposes alias {alias}");
    }
    assert_eq!(
        state
            .card("agent")
            .expect("agent resolves")
            .kind
            .wire_name(),
        "Agent"
    );
    assert!(
        state
            .card_ref("prompt")
            .expect("prompt ref resolves")
            .uid
            .is_some()
    );
    let artifacts = state.artifacts("prompt").expect("prompt artifacts resolve");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].relative_path(), "shared-prompt.txt");
    assert_eq!(
        std::fs::read(artifacts[0].local_path()).expect("artifact reads"),
        PROMPT_ARTIFACT
    );

    assert_eq!(
        state
            .card("missing")
            .expect_err("unknown alias is refused")
            .code(),
        "WYRD_SDK_404_UNKNOWN_ALIAS"
    );
    assert_eq!(
        WyrdState::from_path(metadata_bundle)
            .expect_err("metadata-only bundle is refused")
            .code(),
        "WYRD_SDK_400_UNHYDRATED_ARTIFACT"
    );
}

/// Mint a machine API key through the harness's real bootstrap route.
///
/// # Panics
/// Panics when bootstrapping fails or returns a user principal.
async fn machine_key(server: &WyrdTestServer, name: &str, roles: &[&str]) -> String {
    match server
        .bootstrap_service(name, roles)
        .await
        .expect("service bootstraps")
    {
        Bootstrap::Machine { api_key, .. } => api_key.expose_secret().to_owned(),
        Bootstrap::User { .. } => panic!("service bootstrap returned a user principal"),
    }
}

/// Build a public Cards handle from a server URL and credential.
///
/// # Panics
/// Panics when the shared client cannot be assembled.
fn connect(base_url: &str, credential: &str) -> Cards {
    Cards::with_client(
        client_from_options(Some(base_url), Some(credential), None).expect("client builds"),
    )
}

/// Prove the Rust SDK registers, reads, hydrates, and loads offline state.
///
/// # Panics
/// Panics when any journey step or structured-error expectation fails.
#[tokio::test(flavor = "multi_thread")]
async fn registers_reads_hydrates_and_loads_offline_state() {
    let root = tempfile::tempdir().expect("fixture root creates");
    let service = write_service_graph(root.path());
    let bundle = root.path().join("bundle");
    let metadata_bundle = root.path().join("metadata-bundle");
    let server = Box::pin(WyrdTestServer::start_bound())
        .await
        .expect("test server starts");
    let base_url = server
        .base_url()
        .expect("bound server has a URL")
        .to_owned();
    let cards = connect(
        &base_url,
        &machine_key(&server, "rust_cards_admin", &["admin"]).await,
    );

    let prompt = Box::pin(cards.register_from_path(&root.path().join("shared-prompt.yaml")))
        .await
        .expect("prompt registers");
    assert_eq!(prompt.root.kind.wire_name(), "Prompt");

    let receipt = Box::pin(cards.register_from_path(&service))
        .await
        .expect("service registers");
    assert_eq!(receipt.root.kind.wire_name(), "Service");
    assert_eq!(receipt.outcomes.len(), 2);
    let uid = receipt.root.uid.clone().expect("service root has a UID");

    let replay = Box::pin(cards.register_from_path(&service))
        .await
        .expect("service replays");
    assert_eq!(replay.root.uid.as_ref(), Some(&uid));

    let card = cards
        .get(CardSelector::exact(receipt.root.clone()))
        .await
        .expect("service reads");
    assert_eq!(card.kind.wire_name(), "Service");
    assert_eq!(card.metadata.name.as_str(), "rust-hydrated-service");

    assert_stable_binding_ids(&cards, root.path()).await;

    server
        .seed_role(
            "rust_cards_denied",
            &["bifrost_query:read".parse().expect("permission parses")],
        )
        .await
        .expect("denied role seeds");
    let denied = connect(
        &base_url,
        &machine_key(&server, "rust_cards_denied", &["rust_cards_denied"]).await,
    );
    let denied_error = Box::pin(denied.register_from_path(&service))
        .await
        .expect_err("under-privileged registration is refused");
    assert_eq!(denied_error.status(), 403);

    let hydrator = CardGraphHydrator::new(cards.registry_context());
    let selector = CardSelector::exact(receipt.root.clone());
    let summary = Box::pin(hydrator.hydrate(&selector, &bundle, HydrationMode::Complete))
        .await
        .expect("complete bundle hydrates");
    assert_eq!(summary.mode, HydrationMode::Complete);
    assert_eq!(summary.card_count, 3);
    assert_eq!(summary.root.uid.as_ref(), Some(&uid));
    let metadata_summary =
        Box::pin(hydrator.hydrate(&selector, &metadata_bundle, HydrationMode::MetadataOnly))
            .await
            .expect("metadata bundle hydrates");
    assert_eq!(metadata_summary.mode, HydrationMode::MetadataOnly);

    server.shutdown().await.expect("test server shuts down");

    let state = WyrdState::from_path(&bundle).expect("complete bundle loads offline");
    assert_eq!(state.root_ref().uid.as_ref(), Some(&uid));
    assert_offline_state(&state, &metadata_bundle);
}
