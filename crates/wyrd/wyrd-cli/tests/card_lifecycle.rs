//! CLI Card lifecycle journeys.

use assert_cmd::prelude::*;
use base64::Engine;
use serde_json::{Value, json};
use sha2::Digest;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
use wyrd_sdk::WyrdState;
use wyrd_spec::reference::{CardRef, unresolved_card_ref_paths};
use wyrd_storage::settings::{BackendConfig, StorageSettings};
use wyrd_testing::WyrdTestServer;

fn run_cli(arguments: &[&str]) -> Output {
    run_cli_with_token(arguments, None)
}

fn run_cli_with_token(arguments: &[&str], token: Option<&str>) -> Output {
    let mut command = Command::cargo_bin("wyrd").expect("wyrd binary builds");
    let empty_config =
        std::env::temp_dir().join(format!("wyrd-cli-empty-config-{}", std::process::id()));
    command.env("WYRD_CONFIG_HOME", empty_config);
    if let Some(token) = token {
        command.env("WYRD_ACCESS_TOKEN", token);
    } else {
        command.env_remove("WYRD_ACCESS_TOKEN");
    }
    command
        .env_remove("WYRD_WORKLOAD_TOKEN")
        .env_remove("WYRD_TENANT")
        .env_remove("WYRD_API_KEY");
    command.args(arguments).output().expect("wyrd command runs")
}

async fn run_cli_owned_with_token(arguments: Vec<String>, token: Option<String>) -> Output {
    tokio::task::spawn_blocking(move || {
        let arguments = arguments.iter().map(String::as_str).collect::<Vec<_>>();
        run_cli_with_token(&arguments, token.as_deref())
    })
    .await
    .expect("CLI subprocess joins")
}

async fn run_cli_owned(arguments: Vec<String>) -> Output {
    run_cli_owned_with_token(arguments, None).await
}

macro_rules! run_cli_async_with_token {
    ($token:expr, $($argument:expr),* $(,)?) => {
        run_cli_owned_with_token(
            vec![$($argument.to_owned()),*],
            Some($token.to_owned()),
        )
        .await
    };
}

macro_rules! run_cli_async {
    ($($argument:expr),* $(,)?) => {
        run_cli_owned(vec![$($argument.to_owned()),*]).await
    };
}

fn write_prompt(temp: &TempDir) -> std::path::PathBuf {
    let artifact = b"cli-card-artifact";
    let digest = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(artifact));
    std::fs::write(temp.path().join("prompt.txt"), artifact).expect("artifact writes");
    let path = temp.path().join("prompt.yaml");
    std::fs::write(
        &path,
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: cli-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: prompt.txt\n    sha256: {digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            artifact.len()
        ),
    )
    .expect("prompt card writes");
    path
}

fn write_multi_card_service(temp: &TempDir) -> std::path::PathBuf {
    let prompt_artifact = b"shared-prompt-artifact";
    let prompt_digest =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(prompt_artifact));
    std::fs::write(temp.path().join("shared-prompt.txt"), prompt_artifact)
        .expect("shared prompt artifact writes");
    std::fs::write(
        temp.path().join("shared-prompt.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: shared-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\nartifacts:\n  - relative_path: shared-prompt.txt\n    sha256: {prompt_digest}\n    size_bytes: {}\n    content_type: text/plain\n",
            prompt_artifact.len()
        ),
    )
    .expect("shared prompt writes");

    let model_artifact = b"shared-model-artifact";
    let model_digest =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(model_artifact));
    std::fs::write(temp.path().join("model.bin"), model_artifact).expect("model artifact writes");
    std::fs::write(
        temp.path().join("model.yaml"),
        format!(
            "apiVersion: wyrd/v1\nkind: Model\nmetadata:\n  name: shared-model\n  version: 1.0.0\n  space: default\nspec:\n  interface:\n    kind: Sklearn\n    meta:\n      framework_version: \"1.4.0\"\n      model_subtype: GradientBoostingClassifier\n  task_type: BinaryClassification\n  signature:\n    inputs: []\n    outputs: []\nartifacts:\n  - relative_path: model.bin\n    sha256: {model_digest}\n    size_bytes: {}\n    content_type: application/octet-stream\n",
            model_artifact.len()
        ),
    )
    .expect("model writes");

    for (name, prompt_name) in [("agent-one", "agent-one"), ("agent-two", "agent-two")] {
        std::fs::write(
            temp.path().join(format!("{name}.yaml")),
            format!(
                "apiVersion: wyrd/v1\nkind: Agent\nmetadata:\n  name: {prompt_name}\n  version: 1.0.0\n  space: default\nspec:\n  prompt:\n    kind: Prompt\n    name: shared-prompt\n    version: 1.0.0\n    space: default\n  run_config:\n    max_iterations: 2\n    timeout_ms: 1000\n"
            ),
        )
        .expect("agent writes");
    }

    let service = temp.path().join("service.yaml");
    std::fs::write(
        &service,
        "apiVersion: wyrd/v1
kind: Service
metadata:
  name: hydrated-service
  version: 1.0.0
  space: default
spec:
  service_type: agent
  components:
    - alias: agent-one
      ref:
        kind: Agent
        name: agent-one
        version: 1.0.0
        space: default
    - alias: agent-two
      ref:
        kind: Agent
        name: agent-two
        version: 1.0.0
        space: default
    - alias: model
      ref:
        kind: Model
        name: shared-model
        version: 1.0.0
        space: default
    - alias: prompt
      ref:
        kind: Prompt
        name: shared-prompt
        version: 1.0.0
        space: default
",
    )
    .expect("service writes");
    service
}

/// Owns the committed typed-state Card tree and its temporary test workspace.
///
/// The fixture copies stable Card documents and artifact bytes into an isolated
/// directory so public registration and CLI apply consume the same tree a
/// maintainer can inspect under `tests/fixtures`, while each journey gets a
/// fresh bundle destination.
struct RuntimeServiceFixture {
    /// Temporary workspace containing the copied committed Card tree.
    workspace: TempDir,
    /// Directory containing the copied fixture documents and payloads.
    source_root: PathBuf,
    /// Copied Service document consumed by the CLI apply dispatcher.
    service_path: PathBuf,
    /// Destination used by the complete CLI hydration command.
    bundle_dir: PathBuf,
}

impl RuntimeServiceFixture {
    /// Names of the committed documents and payloads that form this journey.
    const FILES: &[&str] = &[
        "primary.bin",
        "shadow.bin",
        "training.bin",
        "triage-prompt.yaml",
        "model-primary.yaml",
        "model-shadow.yaml",
        "training.yaml",
        "agent-triage.yaml",
        "agent-inline.yaml",
        "quality.yaml",
        "model-drift.yaml",
        "runtime.yaml",
        "typed-service.yaml",
    ];

    /// Exact alias projection expected from the committed Service graph.
    const EXPECTED_ALIASES: &[&str] = &[
        "agent_inline",
        "agent_triage",
        "default-Data-training-1.0.0",
        "default-Prompt-triage-prompt-1.0.0",
        "model_drift",
        "model_primary",
        "model_shadow",
        "quality_eval",
        "root",
        "runtime_workflow",
        "training_data",
        "triage_prompt",
    ];

    /// Create an isolated copy of the committed fixture tree.
    ///
    /// # Errors
    /// Returns an IO error when the temporary workspace cannot be created, a
    /// committed fixture cannot be read, or a fixture copy cannot be written.
    fn new() -> Result<Self, std::io::Error> {
        let workspace = tempfile::tempdir()?;
        let source_root = workspace.path().join("source");
        let committed_root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/card_lifecycle/typed_state");
        for relative in Self::FILES {
            let destination = source_root.join(relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(committed_root.join(relative), &destination)?;
        }
        let service_path = source_root.join("typed-service.yaml");
        let bundle_dir = workspace.path().join("bundle");
        Ok(Self {
            workspace,
            source_root,
            service_path,
            bundle_dir,
        })
    }

    /// Return the copied path for one fixture document or artifact.
    ///
    /// # Panics
    /// Panics only when called with a relative path that is not part of the
    /// committed fixture tree; all journey call sites use [`Self::FILES`].
    fn path(&self, relative: &str) -> PathBuf {
        assert!(Self::FILES.contains(&relative));
        let path = self.source_root.join(relative);
        assert!(path.starts_with(&self.source_root));
        path
    }

    /// Return the Service document consumed by the CLI apply dispatcher.
    #[must_use]
    fn service_path(&self) -> &Path {
        &self.service_path
    }

    /// Return the complete-hydration destination used by the CLI get command.
    #[must_use]
    fn bundle_dir(&self) -> &Path {
        debug_assert!(self.bundle_dir.starts_with(self.workspace.path()));
        &self.bundle_dir
    }

    /// Register every dependency in the fixture through the public Cards API.
    ///
    /// Models and Data carry the three artifact payloads whose bytes the
    /// offline state later verifies. The remaining Cards make the Service
    /// graph complete before the public CLI apply call.
    ///
    /// # Errors
    /// Returns the first registry or fixture-loading error encountered while
    /// registering a dependency through [`wyrd_registry::Cards`].
    async fn register_dependencies(
        &self,
        cards: &wyrd_registry::Cards,
    ) -> Result<(), wyrd_spec::error::WyrdError> {
        for relative in [
            "triage-prompt.yaml",
            "model-primary.yaml",
            "model-shadow.yaml",
            "training.yaml",
            "agent-triage.yaml",
            "agent-inline.yaml",
            "quality.yaml",
            "model-drift.yaml",
            "runtime.yaml",
        ] {
            cards.register_from_path(&self.path(relative)).await?;
        }
        Ok(())
    }

    /// Return the exact aliases expected from the Service graph and generated
    /// canonical aliases emitted by hydration.
    #[must_use]
    fn expected_aliases() -> &'static [&'static str] {
        Self::EXPECTED_ALIASES
    }
}

fn first_stderr_json(output: &Output) -> Value {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .next()
        .expect("CLI emits a machine-readable error first");
    serde_json::from_str(line).expect("CLI error is JSON")
}

#[test]
fn plan_is_local_and_returns_machine_readable_card_projection() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = write_prompt(&temp);
    let output = run_cli(&[
        "plan",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("plan output is JSON");
    assert_eq!(report["ok"], true);
    assert_eq!(report["cards"][0]["kind"], "Prompt");
    assert_eq!(report["cards"][0]["name"], "cli-prompt");
    assert!(report["cards"][0].get("source_path").is_none());
}

#[test]
fn invalid_local_reference_returns_card_load_error_without_credentials() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = temp.path().join("missing.yaml");
    let output = run_cli(&[
        "plan",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    let report: Value = serde_json::from_slice(&output.stdout).expect("plan error is JSON");
    assert_eq!(report["ok"], false);
    assert_eq!(first_stderr_json(&output)["code"], "WYRD_CLI_400_CARD_LOAD");
}

#[test]
fn apply_rejects_an_unreadable_artifact_manifest_before_authentication() {
    let temp = tempfile::tempdir().expect("tempdir creates");
    let path = write_prompt(&temp);
    let contents = std::fs::read_to_string(&path).expect("prompt reads");
    std::fs::write(
        &path,
        contents.replace("relative_path: prompt.txt", "relative_path: missing.txt"),
    )
    .expect("invalid prompt writes");

    let output = run_cli(&[
        "apply",
        path.to_str().expect("prompt path is UTF-8"),
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    assert_eq!(first_stderr_json(&output)["code"], "WYRD_CLI_400_CARD_LOAD");
}

#[test]
fn delete_rejects_ambiguous_named_selector_before_client_construction() {
    let output = run_cli(&[
        "delete",
        "--kind",
        "Prompt",
        "--space",
        "default",
        "--name",
        "cli-prompt",
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    assert_eq!(
        first_stderr_json(&output)["code"],
        "WYRD_CLI_400_DELETE_SELECTOR_EXACT"
    );
}

#[test]
fn get_requires_an_output_directory_before_client_construction() {
    let output = run_cli(&[
        "get",
        "--kind",
        "Prompt",
        "--space",
        "default",
        "--name",
        "cli-prompt",
        "--format",
        "json",
    ]);

    assert_eq!(output.status.code(), Some(64));
    assert_eq!(
        first_stderr_json(&output)["code"],
        "WYRD_CLI_400_INVALID_ARGUMENT"
    );
}

#[test]
fn get_help_describes_hydration_and_does_not_advertise_argv_credentials() {
    let output = run_cli(&["get", "--help"]);
    let help = String::from_utf8_lossy(&output.stdout);

    assert_eq!(output.status.code(), Some(64));
    assert!(help.contains("reachable graph"));
    assert!(help.contains("--output-dir"));
    assert!(help.contains("complete artifact downloads"));
    assert!(help.contains("--metadata-only"));
    assert!(help.contains("not runnable"));
    assert!(!help.contains("--token"));
}

#[test]
fn card_help_rejects_removed_token_argument() {
    let output = run_cli(&["list", "--token", "secret-value"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(64));
    assert!(stderr.contains("unexpected argument '--token'"));
    assert!(!stderr.contains("secret-value"));
}

#[test]
fn card_rejects_remote_cleartext_before_resolving_credentials() {
    let output = run_cli(&["list", "--server", "http://example.com", "--format", "json"]);

    assert!(!output.status.success());
    let error = first_stderr_json(&output);
    assert_eq!(error["code"], "WYRD_CLI_400_CLIENT_CONFIG");
    assert!(
        error["message"]
            .as_str()
            .expect("configuration detail is a string")
            .contains("remote cleartext HTTP is not allowed")
    );
}

#[test]
fn card_reports_the_credential_chain_when_no_credential_is_available() {
    let output = run_cli(&[
        "list",
        "--server",
        "https://example.com",
        "--format",
        "json",
    ]);

    assert!(!output.status.success());
    let error = first_stderr_json(&output);
    assert_eq!(error["code"], "WYRD_CLI_401_NO_CREDENTIALS");
    assert!(
        error["remediation"]
            .as_str()
            .expect("credential remediation is a string")
            .contains("WYRD_ACCESS_TOKEN")
    );
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use secrecy::SecretString;
    use std::sync::Arc;
    use wyrd_client::WyrdClient;
    use wyrd_client::auth::AuthMiddleware;
    use wyrd_client::config::ClientConfig;
    use wyrd_client::transport::{HttpTransport, ResolvedCredential};
    use wyrd_registry::Cards;
    use wyrd_testing::Bootstrap;

    async fn start_cli_server() -> (
        WyrdTestServer,
        String,
        TempDir,
        CancellationToken,
        tokio::task::JoinHandle<()>,
    ) {
        let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("CLI test listener binds");
        socket
            .set_nonblocking(true)
            .expect("CLI test listener becomes nonblocking");
        let address = socket
            .local_addr()
            .expect("CLI test listener has an address");
        let base_url = format!("http://{address}");
        let storage_root = tempfile::tempdir().expect("CLI storage root creates");
        let server = WyrdTestServer::builder()
            .with_storage_settings(StorageSettings {
                backend: BackendConfig::Local {
                    root: storage_root.path().to_path_buf(),
                },
                require_encryption: false,
                presign_ttl: std::time::Duration::from_secs(600),
                part_size_bytes: 16 * 1024 * 1024,
                multipart_threshold_bytes: 100 * 1024 * 1024,
                public_base_url: Some(base_url.clone()),
            })
            .start_in_process()
            .await
            .expect("CLI test server starts");
        let router = wyrd_server::build_router(server.state().clone());
        let listener =
            tokio::net::TcpListener::from_std(socket).expect("CLI test listener converts to Tokio");
        let shutdown = CancellationToken::new();
        let serve_shutdown = shutdown.clone();
        let serve_handle = tokio::spawn(async move {
            let _ = wyrd_server::app::serve::serve(router, listener, serve_shutdown).await;
        });
        (server, base_url, storage_root, shutdown, serve_handle)
    }

    async fn stop_cli_server(
        server: WyrdTestServer,
        shutdown: CancellationToken,
        serve_handle: tokio::task::JoinHandle<()>,
    ) {
        shutdown.cancel();
        serve_handle.await.expect("CLI test server joins");
        server.shutdown().await.expect("test server shuts down");
    }

    /// Build the public registry handle used by the CLI journey's dependency
    /// registration calls.
    ///
    /// The handle exercises the same authenticated HTTP client boundary as a
    /// caller; it does not access the registry engine or database directly.
    ///
    /// # Panics
    /// Panics when the authenticated transport cannot be assembled from the
    /// test server URL and bootstrapped bearer token.
    fn public_cards_client(base_url: &str, jwt: &str) -> Cards {
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.to_owned();
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from(jwt.to_owned())),
        )
        .expect("registry auth middleware builds");
        let transport = HttpTransport::new(&config.http, Arc::clone(&auth))
            .expect("registry HTTP transport builds");
        Cards::with_client(WyrdClient::from_parts(auth, transport, config.grpc))
    }

    /// Run one CLI dispatcher invocation and decode its successful JSON body.
    ///
    /// # Errors
    /// Returns a string error when the subprocess exits unsuccessfully or its
    /// stdout is not a JSON value.
    async fn run_cli_json(arguments: Vec<String>, token: &str) -> Result<Value, String> {
        let output = run_cli_owned_with_token(arguments, Some(token.to_owned())).await;
        if !output.status.success() {
            return Err(format!(
                "CLI exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("CLI stdout is not JSON: {error}"))
    }

    /// Decode the exact Service root reference from an apply receipt.
    ///
    /// # Errors
    /// Returns a JSON decoding error when the receipt has no valid `root`
    /// CardRef object.
    fn exact_root_ref(receipt: &Value) -> Result<CardRef, serde_json::Error> {
        serde_json::from_value(receipt["root"].clone())
    }

    /// Build the exact UID-based CLI get command for a Service root.
    ///
    /// # Errors
    /// Returns an error when the root lacks its required UID or the output
    /// directory cannot be represented as UTF-8 for command-line transport.
    fn exact_get_args(root: &CardRef, bundle: &Path, server: &str) -> Result<Vec<String>, String> {
        let uid = root
            .uid
            .as_ref()
            .ok_or_else(|| "Service root receipt has no UID".to_owned())?;
        let bundle = bundle
            .to_str()
            .ok_or_else(|| "bundle path is not UTF-8".to_owned())?;
        Ok(vec![
            "get".to_owned(),
            "--kind".to_owned(),
            root.kind.wire_name().to_owned(),
            "--uid".to_owned(),
            uid.to_string(),
            "--output-dir".to_owned(),
            bundle.to_owned(),
            "--server".to_owned(),
            server.to_owned(),
            "--format".to_owned(),
            "json".to_owned(),
        ])
    }

    /// Assert that every hydrated model and data artifact has the expected
    /// bytes, digest, and verified local path.
    ///
    /// The assertions prove artifact payloads remain external to typed holder
    /// values while the `WyrdState` artifact index still exposes verified files.
    ///
    /// # Panics
    /// Panics when an expected artifact is absent, unreadable, or differs from
    /// its committed fixture bytes or digest.
    fn assert_verified_artifact_bytes(state: &WyrdState) {
        for (alias, expected) in [
            ("model_primary", b"primary-model\n".as_slice()),
            ("model_shadow", b"shadow-model\n".as_slice()),
            ("training_data", b"training-data\n".as_slice()),
        ] {
            let artifact = state.artifacts(alias).expect("artifact list resolves");
            assert_eq!(artifact.len(), 1, "{alias} has one fixture artifact");
            let artifact = &artifact[0];
            assert_eq!(
                std::fs::read(artifact.local_path()).expect("artifact bytes read"),
                expected
            );
            assert_eq!(artifact.size_bytes(), expected.len() as u64);
            let expected_digest =
                base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(expected));
            assert_eq!(artifact.sha256(), expected_digest);
            assert!(artifact.local_path().is_file());
        }
    }

    /// Assert exact version and UID identity for every Card represented by an
    /// alias in the hydrated graph.
    ///
    /// # Panics
    /// Panics when an alias is unresolved, a CardRef is not versioned, a UID is
    /// absent or empty, or aliases do not cover ten unique Cards.
    fn assert_all_refs_are_uid_pinned(state: &WyrdState) {
        let aliases = state.aliases().collect::<Vec<_>>();
        let mut refs = BTreeSet::new();
        for alias in aliases {
            let card_ref = state.card_ref(alias).expect("alias CardRef resolves");
            assert!(card_ref.version.as_str().contains('.'));
            let uid = card_ref.uid.as_ref().expect("CardRef has UID");
            assert!(!uid.to_string().is_empty());
            refs.insert(card_ref.to_string());
        }
        assert_eq!(refs.len(), 10);
    }

    /// Assert no loaded Card retains an authored or unresolved path reference.
    ///
    /// # Panics
    /// Panics when an alias or Card cannot be loaded or any unique envelope
    /// still contains an unresolved reference path.
    fn assert_no_unresolved_paths(state: &WyrdState) {
        let aliases = state.aliases().collect::<Vec<_>>();
        let mut refs = BTreeSet::new();
        for alias in aliases {
            let card_ref = state.card_ref(alias).expect("alias CardRef resolves");
            if refs.insert(card_ref.to_string()) {
                let card = state.card(alias).expect("Card envelope resolves");
                assert!(
                    unresolved_card_ref_paths(&card.spec).is_empty(),
                    "{} contains unresolved paths",
                    card_ref
                );
            }
        }
    }

    #[tokio::test]
    async fn card_lifecycle_cli_journey() {
        if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir creates");
        let path = write_prompt(&temp);
        let (server, base_url, _storage_root, shutdown, serve_handle) = start_cli_server().await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("cli-lifecycle-writer", &["writer"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("writer bootstrap returned a non-user principal");
        };
        let Bootstrap::User {
            jwt: denied_jwt, ..
        } = server
            .bootstrap_user("cli-lifecycle-denied", &[])
            .await
            .expect("denied user bootstraps")
        else {
            panic!("denied bootstrap returned a non-user principal");
        };

        let path_string = path.to_str().expect("prompt path is UTF-8");
        let apply_arguments = vec![
            "apply".to_owned(),
            path_string.to_owned(),
            "--server".to_owned(),
            base_url.clone(),
            "--format".to_owned(),
            "json".to_owned(),
        ];
        let plan = run_cli_async!("plan", path_string, "--format", "json");
        assert!(
            plan.status.success(),
            "{}",
            String::from_utf8_lossy(&plan.stderr)
        );

        let denied = run_cli_async_with_token!(
            &denied_jwt,
            "apply",
            path_string,
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert_eq!(denied.status.code(), Some(77));
        assert_eq!(first_stderr_json(&denied)["status"], 403);

        let applied = run_cli_owned_with_token(apply_arguments, Some(jwt.clone())).await;
        assert!(
            applied.status.success(),
            "{}",
            String::from_utf8_lossy(&applied.stderr)
        );
        let receipt: Value = serde_json::from_slice(&applied.stdout).expect("receipt is JSON");
        let uid = receipt["root"]["uid"]
            .as_str()
            .expect("receipt contains a root UID")
            .to_owned();

        let replay = run_cli_async_with_token!(
            &jwt,
            "apply",
            path_string,
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            replay.status.success(),
            "{}",
            String::from_utf8_lossy(&replay.stderr)
        );
        let replay_receipt: Value =
            serde_json::from_slice(&replay.stdout).expect("replay receipt is JSON");
        assert_eq!(replay_receipt["root"]["uid"], uid);

        let denied_get = run_cli_async_with_token!(
            &denied_jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("denied-hydration")
                .to_str()
                .expect("denied hydration path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert_eq!(denied_get.status.code(), Some(77));
        assert_eq!(first_stderr_json(&denied_get)["status"], 403);
        assert!(!temp.path().join("denied-hydration").exists());

        let get = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("hydrated")
                .to_str()
                .expect("hydration path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            get.status.success(),
            "{}",
            String::from_utf8_lossy(&get.stderr)
        );
        let hydrated: Value = serde_json::from_slice(&get.stdout).expect("get output is JSON");
        assert_eq!(hydrated["root"]["uid"], uid);
        assert_eq!(hydrated["mode"], "complete");
        assert_eq!(hydrated["card_count"], 1);
        assert!(
            !std::fs::read(temp.path().join("hydrated/metadata.yaml"))
                .expect("hydrated metadata reads")
                .is_empty()
        );
        assert_eq!(
            std::fs::read(temp.path().join("hydrated/cards/root/artifacts/prompt.txt"))
                .expect("hydrated artifact reads"),
            b"cli-card-artifact"
        );

        let metadata_only = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("metadata-only")
                .to_str()
                .expect("metadata path is UTF-8"),
            "--metadata-only",
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            metadata_only.status.success(),
            "{}",
            String::from_utf8_lossy(&metadata_only.stderr)
        );
        let metadata: Value =
            serde_json::from_slice(&metadata_only.stdout).expect("metadata output is JSON");
        assert_eq!(metadata["mode"], "metadata");
        assert!(
            !temp
                .path()
                .join("metadata-only/cards/root/artifacts/prompt.txt")
                .exists()
        );

        let rerun = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("metadata-only")
                .to_str()
                .expect("rerun path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            rerun.status.success(),
            "{}",
            String::from_utf8_lossy(&rerun.stderr)
        );
        let rerun_summary: Value = serde_json::from_slice(&rerun.stdout).expect("rerun JSON");
        assert_eq!(rerun_summary["mode"], "complete");
        assert_eq!(
            std::fs::read(
                temp.path()
                    .join("metadata-only/cards/root/artifacts/prompt.txt")
            )
            .expect("rerun artifact reads"),
            b"cli-card-artifact"
        );

        let list = run_cli_async_with_token!(
            &jwt,
            "list",
            "--kind",
            "Prompt",
            "--name",
            "cli-prompt",
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            list.status.success(),
            "{}",
            String::from_utf8_lossy(&list.stderr)
        );
        let listed: Value = serde_json::from_slice(&list.stdout).expect("list output is JSON");
        assert_eq!(listed["items"].as_array().expect("list items").len(), 1);

        let latest = run_cli_async_with_token!(
            &jwt,
            "latest",
            "--kind",
            "Prompt",
            "--space",
            "default",
            "--name",
            "cli-prompt",
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            latest.status.success(),
            "{}",
            String::from_utf8_lossy(&latest.stderr)
        );
        let resolved: Value =
            serde_json::from_slice(&latest.stdout).expect("latest output is JSON");
        assert_eq!(resolved["card_ref"]["uid"], uid);

        let latest_dir = temp.path().join("latest-hydrated");
        let latest_get = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--space",
            "default",
            "--name",
            "cli-prompt",
            "--output-dir",
            latest_dir.to_str().expect("latest path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            latest_get.status.success(),
            "{}",
            String::from_utf8_lossy(&latest_get.stderr)
        );
        let latest_summary: Value =
            serde_json::from_slice(&latest_get.stdout).expect("latest hydration JSON");
        assert_eq!(latest_summary["root"]["uid"], uid);
        let latest_manifest: Value = serde_yaml::from_reader(
            std::fs::File::open(latest_dir.join("metadata.yaml")).expect("latest manifest opens"),
        )
        .expect("latest manifest reads");
        assert_eq!(latest_manifest["root"]["version"], "1.0.0");

        let destination = temp.path().join("loaded");
        let destination_string = destination.to_str().expect("destination path is UTF-8");
        let loaded = run_cli_async_with_token!(
            &jwt,
            "load",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--path",
            destination_string,
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(
            loaded.status.success(),
            "{}",
            String::from_utf8_lossy(&loaded.stderr)
        );
        let loaded_json: Value =
            serde_json::from_slice(&loaded.stdout).expect("load output is JSON");
        assert_eq!(loaded_json["materialized"], true);
        assert_eq!(
            std::fs::read(destination.join("prompt.txt")).expect("artifact reads"),
            b"cli-card-artifact"
        );

        let superuser = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        let original_storage_path: String = sqlx::query_scalar(
            "SELECT storage_path FROM wyrd.storage_artifact_metadata WHERE card_uid = $1",
        )
        .bind(&uid)
        .fetch_one(&superuser)
        .await
        .expect("artifact storage path reads");
        let correct_digest = base64::engine::general_purpose::STANDARD
            .encode(sha2::Sha256::digest(b"cli-card-artifact"));
        sqlx::query("UPDATE wyrd.storage_artifact_metadata SET sha256 = $1 WHERE card_uid = $2")
            .bind("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
            .bind(&uid)
            .execute(&superuser)
            .await
            .expect("artifact digest corrupts");
        let digest_mismatch = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("digest-mismatch")
                .to_str()
                .expect("digest mismatch path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(!digest_mismatch.status.success());
        assert_eq!(
            first_stderr_json(&digest_mismatch)["code"],
            "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED"
        );
        assert!(!temp.path().join("digest-mismatch").exists());

        sqlx::query(
            "UPDATE wyrd.storage_artifact_metadata SET sha256 = $1, size_bytes = $2 WHERE card_uid = $3",
        )
        .bind(&correct_digest)
        .bind(999_i64)
        .bind(&uid)
        .execute(&superuser)
        .await
        .expect("artifact size corrupts");
        let size_mismatch = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("size-mismatch")
                .to_str()
                .expect("size mismatch path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(!size_mismatch.status.success());
        assert_eq!(
            first_stderr_json(&size_mismatch)["code"],
            "WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED"
        );
        assert!(!temp.path().join("size-mismatch").exists());

        sqlx::query(
            "UPDATE wyrd.storage_artifact_metadata SET size_bytes = $1, storage_path = $2 WHERE card_uid = $3",
        )
        .bind(b"cli-card-artifact".len() as i64)
        .bind("../escape")
        .bind(&uid)
        .execute(&superuser)
        .await
        .expect("artifact path corrupts");
        let unsafe_path = run_cli_async_with_token!(
            &jwt,
            "get",
            "--kind",
            "Prompt",
            "--uid",
            &uid,
            "--output-dir",
            temp.path()
                .join("unsafe-path")
                .to_str()
                .expect("unsafe path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );
        assert!(!unsafe_path.status.success());
        assert_eq!(
            first_stderr_json(&unsafe_path)["code"],
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
        );
        assert!(!temp.path().join("unsafe-path").exists());
        sqlx::query(
            "UPDATE wyrd.storage_artifact_metadata SET size_bytes = $1, sha256 = $2, storage_path = $3 WHERE card_uid = $4",
        )
        .bind(b"cli-card-artifact".len() as i64)
        .bind(&correct_digest)
        .bind(&original_storage_path)
        .bind(&uid)
        .execute(&superuser)
        .await
        .expect("artifact metadata restores");

        let deleted = run_cli_async_with_token!(
            &jwt, "delete", "--kind", "Prompt", "--uid", &uid, "--server", &base_url, "--format",
            "json",
        );
        assert!(
            deleted.status.success(),
            "{}",
            String::from_utf8_lossy(&deleted.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&deleted.stdout).expect("delete output is JSON")["deleted"],
            true
        );

        stop_cli_server(server, shutdown, serve_handle).await;
    }

    /// Prove the public CLI apply/get path publishes a complete bundle that
    /// remains usable through native typed WyrdState after server shutdown.
    ///
    /// The journey exercises public registration, CLI subprocesses, complete
    /// artifact hydration, exact alias projection, and offline typed access
    /// after the originating server and connection endpoint are gone.
    ///
    /// # Panics
    /// Panics when fixture creation, public registration, CLI apply/get,
    /// server shutdown, offline loading, or any typed-state invariant fails.
    ///
    /// # Cancellation
    /// Cancellation may leave temporary fixture files or an embedded test
    /// server awaiting cleanup; the published bundle is written atomically and
    /// offline loading does not leave partial in-memory state.
    #[tokio::test]
    #[ignore = "requires embedded Postgres WyrdTestServer"]
    async fn cli_bundle_loads_typed_wyrdstate_after_server_shutdown() {
        let fixture = RuntimeServiceFixture::new().expect("typed fixture copies");
        let server = WyrdTestServer::start_bound()
            .await
            .expect("bound WyrdTestServer starts");
        let base_url = server
            .base_url()
            .expect("bound server exposes base URL")
            .to_owned();
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("typed-state-journey", &["writer"])
            .await
            .expect("journey writer bootstraps")
        else {
            panic!("journey bootstrap returned a non-user principal");
        };

        let cards = public_cards_client(&base_url, &jwt);
        fixture
            .register_dependencies(&cards)
            .await
            .expect("journey dependencies register through public Cards API");
        let apply = run_cli_json(
            vec![
                "apply".to_owned(),
                fixture
                    .service_path()
                    .to_str()
                    .expect("service path is UTF-8")
                    .to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            &jwt,
        )
        .await;
        let receipt = apply.expect("CLI apply succeeds");
        let service_ref = exact_root_ref(&receipt).expect("apply root is an exact CardRef");
        assert_eq!(service_ref.kind, wyrd_spec::envelope::CardKind::Service);
        assert_eq!(service_ref.name.as_str(), "typed-service");
        assert_eq!(service_ref.version.as_str(), "1.0.0");
        assert!(service_ref.uid.is_some());

        let summary = run_cli_json(
            exact_get_args(&service_ref, fixture.bundle_dir(), &base_url)
                .expect("exact get arguments build"),
            &jwt,
        )
        .await
        .expect("CLI get succeeds");
        assert_eq!(summary["mode"], "complete");
        assert_eq!(summary["card_count"], 10);
        assert_eq!(summary["downloaded_artifact_count"], 3);

        drop(cards);
        server.shutdown().await.expect("bound server shuts down");

        let state =
            WyrdState::from_path(fixture.bundle_dir()).expect("complete bundle loads offline");
        assert_eq!(state.root_ref(), &service_ref);
        assert_eq!(state.service().metadata.name.as_str(), "typed-service");
        assert_eq!(
            state
                .model("model_primary")
                .expect("primary model resolves")
                .name,
            "primary"
        );
        assert_eq!(
            state
                .model("model_shadow")
                .expect("shadow model resolves")
                .name,
            "shadow"
        );
        assert_eq!(
            state.data("training_data").expect("data resolves").name,
            "training"
        );
        assert_eq!(
            state
                .agent("agent_triage")
                .expect("shared agent resolves")
                .name,
            "triage"
        );
        assert_eq!(
            state
                .agent("agent_inline")
                .expect("inline agent resolves")
                .name,
            "inline"
        );
        assert_eq!(
            state.prompt("triage_prompt").expect("prompt resolves").name,
            "triage-prompt"
        );
        assert_eq!(
            state
                .agent_prompt("agent_triage")
                .expect("shared prompt resolves"),
            &state
                .prompt("triage_prompt")
                .expect("prompt resolves")
                .metadata
                .prompt
        );
        let inline_prompt = state
            .agent_prompt("agent_inline")
            .expect("inline prompt resolves");
        assert_eq!(inline_prompt.model, "gpt-4o");
        assert!(
            serde_json::to_string(&inline_prompt.request)
                .expect("inline prompt serializes")
                .contains("Inline triage")
        );
        assert!(
            state
                .eval("quality_eval")
                .expect("eval resolves")
                .tasks
                .is_empty()
        );
        assert_eq!(
            state
                .drift("model_drift")
                .expect("drift resolves")
                .description
                .as_deref(),
            Some("fixture drift")
        );
        assert_eq!(
            state
                .workflow("runtime_workflow")
                .expect("workflow resolves")
                .name,
            "runtime"
        );
        assert_verified_artifact_bytes(&state);
        assert_all_refs_are_uid_pinned(&state);
        assert_no_unresolved_paths(&state);
        let aliases = state.aliases().collect::<Vec<_>>();
        assert_eq!(aliases, RuntimeServiceFixture::expected_aliases());
    }

    /// Prove real registry output hydrates into runtime state and rejects metadata-only output.
    #[tokio::test]
    async fn multi_card_service_get_hydrates_complete_and_metadata_bundles() {
        if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir creates");
        let service_path = write_multi_card_service(&temp);
        let (server, base_url, storage_root, shutdown, serve_handle) = start_cli_server().await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("cli-multi-card-writer", &["writer"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("writer bootstrap returned a non-user principal");
        };
        let Bootstrap::User {
            jwt: denied_jwt, ..
        } = server
            .bootstrap_user("cli-multi-card-denied", &[])
            .await
            .expect("denied user bootstraps")
        else {
            panic!("denied bootstrap returned a non-user principal");
        };

        let cards = public_cards_client(&base_url, &jwt);
        for path in [
            temp.path().join("shared-prompt.yaml"),
            temp.path().join("model.yaml"),
            temp.path().join("agent-one.yaml"),
            temp.path().join("agent-two.yaml"),
        ] {
            cards
                .register_from_path(&path)
                .await
                .expect("related Card registers through shared path API");
        }
        let registration = cards
            .register_from_path(&service_path)
            .await
            .expect("root Service registers through shared path API");
        let service_uid = registration
            .root
            .uid
            .as_ref()
            .expect("service UID exists")
            .clone();
        let service_uid_string = service_uid.to_string();

        let superuser = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        let blob_uri: String =
            sqlx::query_scalar("SELECT card_blob_uri FROM wyrd.cards WHERE card_uid = $1")
                .bind(service_uid.as_uuid())
                .fetch_one(&superuser)
                .await
                .expect("service blob URI reads");
        let blob_path = storage_root.path().join(
            blob_uri
                .strip_prefix("wyrd://")
                .expect("service blob URI has the Wyrd scheme"),
        );
        let original_blob = std::fs::read(&blob_path).expect("service blob reads");
        let service_ref = json!({
            "kind": "Service",
            "name": "hydrated-service",
            "version": "1.0.0",
            "space": "default",
            "uid": service_uid_string,
        });
        let service_ref_string = format!(
            "default/Service/hydrated-service@1.0.0#{}",
            registration.root.uid.as_ref().expect("service UID exists")
        );

        let mut cycle_blob: Value =
            serde_json::from_slice(&original_blob).expect("service blob JSON parses");
        cycle_blob["relationships"]["outbound"]
            .as_array_mut()
            .expect("service outbound relationship list exists")
            .push(Value::String(service_ref_string.clone()));
        cycle_blob["relationships"]["outbound_refs"]
            .as_array_mut()
            .expect("service typed relationship list exists")
            .push(json!({ "card_ref": service_ref, "alias": "self" }));
        std::fs::write(
            &blob_path,
            serde_json::to_vec(&cycle_blob).expect("cycle blob serializes"),
        )
        .expect("cycle blob writes");
        let cycle_dir = temp.path().join("cycle-service");
        let cycle = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string.clone(),
                "--output-dir".to_owned(),
                cycle_dir.to_str().expect("cycle path is UTF-8").to_owned(),
                "--metadata-only".to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(jwt.clone()),
        )
        .await;
        assert!(!cycle.status.success());
        assert_eq!(
            first_stderr_json(&cycle)["code"],
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
        );
        assert!(
            !first_stderr_json(&cycle)["message"]
                .as_str()
                .expect("cycle error message is a string")
                .is_empty()
        );
        assert!(!cycle_dir.exists());
        std::fs::write(&blob_path, &original_blob).expect("service blob restores");

        let model_uid: String = sqlx::query_scalar(
            "SELECT card_uid::text FROM wyrd.cards WHERE kind = 'Model' AND name = 'shared-model'",
        )
        .fetch_one(&superuser)
        .await
        .expect("model UID reads");
        let mut alias_blob: Value =
            serde_json::from_slice(&original_blob).expect("service blob JSON parses");
        alias_blob["relationships"]["outbound_refs"]
            .as_array_mut()
            .expect("service typed relationship list exists")
            .push(json!({
                "card_ref": {
                    "kind": "Model",
                    "name": "shared-model",
                    "version": "1.0.0",
                    "space": "default",
                    "uid": model_uid.clone(),
                },
                "alias": "agent-one",
            }));
        let alias_dir = temp.path().join("alias-conflict-service");
        std::fs::write(
            &blob_path,
            serde_json::to_vec(&alias_blob).expect("alias blob serializes"),
        )
        .expect("alias blob writes");
        let alias_conflict = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string.clone(),
                "--output-dir".to_owned(),
                alias_dir
                    .to_str()
                    .expect("alias conflict path is UTF-8")
                    .to_owned(),
                "--metadata-only".to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(jwt.clone()),
        )
        .await;
        assert!(!alias_conflict.status.success());
        assert_eq!(
            first_stderr_json(&alias_conflict)["code"],
            "WYRD_REGISTRY_400_INVALID_CARD_SPEC"
        );
        assert!(!alias_dir.exists());
        std::fs::write(&blob_path, &original_blob).expect("service blob restores");

        let denied_dir = temp.path().join("denied-service-hydration");
        let denied = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string.clone(),
                "--output-dir".to_owned(),
                denied_dir
                    .to_str()
                    .expect("denied path is UTF-8")
                    .to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(denied_jwt),
        )
        .await;
        assert_eq!(denied.status.code(), Some(77));
        assert_eq!(first_stderr_json(&denied)["status"], 403);
        assert!(!denied_dir.exists());

        let full_dir = temp.path().join("full-service");
        let full = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string.clone(),
                "--output-dir".to_owned(),
                full_dir.to_str().expect("full path is UTF-8").to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(jwt.clone()),
        )
        .await;
        assert!(
            full.status.success(),
            "{}",
            String::from_utf8_lossy(&full.stderr)
        );
        let full_summary: Value = serde_json::from_slice(&full.stdout).expect("full JSON");
        assert_eq!(full_summary["mode"], "complete");
        assert_eq!(full_summary["card_count"], 5);
        assert_eq!(full_summary["artifact_count"], 2);
        assert_eq!(full_summary["downloaded_artifact_count"], 2);
        assert_eq!(
            std::fs::read(full_dir.join("cards/model/artifacts/model.bin"))
                .expect("model payload reads"),
            b"shared-model-artifact"
        );
        assert_eq!(
            std::fs::read(
                full_dir
                    .join("cards/default-Prompt-shared-prompt-1.0.0/artifacts/shared-prompt.txt")
            )
            .expect("prompt payload reads"),
            b"shared-prompt-artifact"
        );
        let manifest: Value = serde_yaml::from_reader(
            std::fs::File::open(full_dir.join("metadata.yaml")).expect("full manifest opens"),
        )
        .expect("full manifest reads");
        let prompt_manifest = manifest["cards"]
            .as_array()
            .expect("manifest cards")
            .iter()
            .find(|card| card["card_ref"]["kind"] == "Prompt")
            .expect("prompt manifest exists");
        assert!(
            prompt_manifest["aliases"]
                .as_array()
                .expect("prompt aliases")
                .iter()
                .any(|alias| alias == "prompt")
        );

        let state = WyrdState::from_path(&full_dir)
            .expect("registry-produced complete bundle loads as WyrdState");
        assert_eq!(state.root_ref(), &registration.root);
        assert_eq!(state.service().kind, wyrd_spec::envelope::CardKind::Service);
        assert_eq!(
            state
                .card_ref("model")
                .expect("model alias resolves")
                .uid
                .as_ref()
                .map(ToString::to_string),
            Some(model_uid.clone()),
        );
        assert_eq!(
            state.aliases().collect::<Vec<_>>(),
            vec![
                "agent-one",
                "agent-two",
                "default-Prompt-shared-prompt-1.0.0",
                "model",
                "prompt",
                "root",
            ],
        );

        let model_artifacts = state.artifacts("model").expect("model artifacts resolve");
        let expected_model_digest = base64::engine::general_purpose::STANDARD
            .encode(sha2::Sha256::digest(b"shared-model-artifact"));
        assert_eq!(model_artifacts.len(), 1);
        assert_eq!(model_artifacts[0].relative_path(), "model.bin");
        assert_eq!(
            model_artifacts[0].size_bytes(),
            b"shared-model-artifact".len() as u64
        );
        assert_eq!(model_artifacts[0].sha256(), expected_model_digest);
        assert_eq!(
            model_artifacts[0].content_type(),
            Some("application/octet-stream")
        );
        assert!(model_artifacts[0].local_path().is_absolute());
        assert!(
            model_artifacts[0]
                .local_path()
                .starts_with(std::fs::canonicalize(&full_dir).expect("bundle canonicalizes"))
        );
        assert_eq!(
            std::fs::read(model_artifacts[0].local_path()).expect("verified model artifact reads"),
            b"shared-model-artifact",
        );

        let metadata_dir = temp.path().join("metadata-service");
        let metadata = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string.clone(),
                "--output-dir".to_owned(),
                metadata_dir
                    .to_str()
                    .expect("metadata path is UTF-8")
                    .to_owned(),
                "--metadata-only".to_owned(),
                "--server".to_owned(),
                base_url.clone(),
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(jwt.clone()),
        )
        .await;
        assert!(
            metadata.status.success(),
            "{}",
            String::from_utf8_lossy(&metadata.stderr)
        );
        let metadata_summary: Value =
            serde_json::from_slice(&metadata.stdout).expect("metadata JSON");
        assert_eq!(metadata_summary["mode"], "metadata");
        assert_eq!(metadata_summary["card_count"], 5);
        assert!(
            !metadata_dir
                .join("cards/model/artifacts/model.bin")
                .exists()
        );
        assert!(metadata_dir.join("cards/model/artifacts.yaml").exists());
        let error = WyrdState::from_path(&metadata_dir)
            .expect_err("metadata-only producer output is not runtime state");
        assert_eq!(error.code(), "WYRD_SDK_400_UNHYDRATED_ARTIFACT");

        sqlx::query(
            "UPDATE wyrd.cards SET status = 'deleted' \
             WHERE kind = 'Prompt' AND space = 'default' AND name = 'shared-prompt' \
               AND version = '1.0.0'",
        )
        .execute(&superuser)
        .await
        .expect("related prompt becomes unavailable");

        let missing_dir = temp.path().join("missing-related");
        let missing = run_cli_owned_with_token(
            vec![
                "get".to_owned(),
                "--kind".to_owned(),
                "Service".to_owned(),
                "--uid".to_owned(),
                service_uid_string,
                "--output-dir".to_owned(),
                missing_dir
                    .to_str()
                    .expect("missing path is UTF-8")
                    .to_owned(),
                "--metadata-only".to_owned(),
                "--server".to_owned(),
                base_url,
                "--format".to_owned(),
                "json".to_owned(),
            ],
            Some(jwt),
        )
        .await;
        assert_eq!(missing.status.code(), Some(66));
        assert_eq!(first_stderr_json(&missing)["status"], 404);
        assert!(!missing_dir.exists());

        stop_cli_server(server, shutdown, serve_handle).await;
    }

    #[tokio::test]
    async fn apply_surfaces_backend_completion_failure() {
        if std::env::var("WYRD_CLI_E2E").as_deref() != Ok("1") {
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir creates");
        let path = write_prompt(&temp);
        let (server, base_url, _storage_root, shutdown, serve_handle) = start_cli_server().await;
        let Bootstrap::User { jwt, .. } = server
            .bootstrap_user("cli-completion-failure", &["writer"])
            .await
            .expect("writer bootstraps")
        else {
            panic!("writer bootstrap returned a non-user principal");
        };
        let superuser = server
            .pg_fixture()
            .superuser_pool()
            .await
            .expect("superuser pool opens");
        sqlx::query(
            r#"CREATE OR REPLACE FUNCTION vala.test_fail_cli_card_completion_audit()
               RETURNS trigger LANGUAGE plpgsql AS $$
               BEGIN
                 IF NEW.operation = 'card.registration.complete' THEN
                   RAISE EXCEPTION 'injected CLI completion audit failure';
                 END IF;
                 RETURN NEW;
               END;
               $$;"#,
        )
        .execute(&superuser)
        .await
        .expect("failure function installs");
        sqlx::query(
            r#"CREATE TRIGGER test_fail_cli_card_completion_audit
               BEFORE INSERT ON vala.audit_outbox
               FOR EACH ROW EXECUTE FUNCTION vala.test_fail_cli_card_completion_audit()"#,
        )
        .execute(&superuser)
        .await
        .expect("failure trigger installs");

        let output = run_cli_async_with_token!(
            &jwt,
            "apply",
            path.to_str().expect("prompt path is UTF-8"),
            "--server",
            &base_url,
            "--format",
            "json",
        );

        assert_eq!(
            output.status.code(),
            Some(69),
            "stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(first_stderr_json(&output)["status"], 502);
        assert_eq!(
            first_stderr_json(&output)["code"],
            "WYRD_SPEC_502_UPSTREAM_FAILURE"
        );
        stop_cli_server(server, shutdown, serve_handle).await;
    }
}
