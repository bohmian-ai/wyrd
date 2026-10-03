//! Client Workflow loading: authored files and registered Workflow Cards
//! hydrated into the one Skald runtime.
//!
//! [`Workflow`] is a thin facade over [`skald_workflow::Workflow`]. It owns no
//! graph, parser, validation rule, or executor: `wyrd_loader` parses the
//! authored bundle, the Cards graph owner reads exact registered bodies, and
//! Skald hydrates, validates, and runs. Rust cannot attach client IO methods
//! to the foreign Skald type, so loading lives here. Loading never registers,
//! executes, or resolves execution secrets; running prepares only the local
//! dependencies the Workflow's routes select.

use std::path::{Path, PathBuf};

use skald_runtime::ProviderRegistry;
use skald_workflow::{
    Workflow as SkaldWorkflow, WorkflowInput, WorkflowResult, WorkflowRun, WorkflowRunOptions,
};
use tokio::task::JoinError;
use wyrd_loader::LoadedTree;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;

use crate::WyrdClient;
use crate::cards::{CardGraphHydrator, CardSelector, Cards, WorkflowBodies};
use crate::global_config::{GlobalConfig, LocalWorkflowConfig};
use local::SelectedRoutes;

mod gateway;
mod local;
mod remote;

pub use gateway::PublicWyrdGatewayCaller;
pub use remote::Workflows;

/// A loaded, validated Workflow ready to run on the local Skald runtime.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// The hydrated Skald Workflow every run delegates to.
    inner: SkaldWorkflow,
    /// Client that loaded registered Cards, reused for `wyrd_gateway` calls
    /// so its connection overrides stay in effect.
    client: Option<WyrdClient>,
}

impl Workflow {
    /// Load the Workflow Card defined in the file at `path` and its bundle.
    ///
    /// The shared loader resolves `path`, `inline`, and sibling dependencies
    /// relative to their containing files within its sandbox and runs the
    /// pure contract validation. A wholly local bundle constructs no client
    /// and makes no registry request. The first external `ref` lazily builds
    /// the default [`Cards`] handle from ambient client configuration, which
    /// then reads each external Agent and Prompt and its locked transitive
    /// closure exactly; a loaded sibling never satisfies an external ref with
    /// the same identity. Resolved validation runs before the Workflow is
    /// returned, so a refusal dispatches nothing.
    ///
    /// The synchronous bundle load and entry canonicalization run together on
    /// Tokio's blocking pool, and so does building the [`Cards`] handle from
    /// configuration and credential files, so filesystem reads never occupy
    /// the polling thread. Cancellation may stop after completed filesystem or registry
    /// reads, and an already started bundle read may finish after the future
    /// is dropped; no partial Workflow is returned and nothing durable is
    /// written.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_500_INTERNAL` when the blocking load task panics
    /// or is cancelled by runtime shutdown;
    /// `WYRD_REGISTRY_400_INVALID_CARD_SPEC` carrying the loader
    /// diagnostics when the bundle fails to load or `path` does not define
    /// exactly one Workflow Card; the client configuration error when an
    /// external ref needs a client that cannot be built; the Cards read and
    /// traversal errors for a missing, denied, or mismatched dependency;
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for an inactive dependency;
    /// and the hydration and validation errors of
    /// [`SkaldWorkflow::from_card_bodies`].
    pub async fn from_path(path: impl AsRef<Path>) -> Result<Self, WyrdError> {
        let path = path.as_ref().to_path_buf();
        let (tree, entry) = tokio::task::spawn_blocking(move || load_bundle(&path))
            .await
            .map_err(|error| blocking_task_failed("workflow_bundle_load", &error))??;
        let (workflow, mut bodies) = WorkflowBodies::authored(&tree, &entry)?;
        let refs = bodies.external_refs(&workflow);
        let mut client = None;
        if !refs.is_empty() {
            let cards = tokio::task::spawn_blocking(|| Cards::new(None, None))
                .await
                .map_err(|error| blocking_task_failed("workflow_cards_client", &error))??;
            CardGraphHydrator::new(cards.registry_context())
                .resolve_external(&mut bodies, &refs)
                .await?;
            client = Some(cards.engine.client.clone());
        }
        let inner = bodies.hydrate(workflow)?;
        Ok(Self { inner, client })
    }

    /// Run on the process-default native provider registry.
    ///
    /// # Errors
    /// Returns the errors of [`Self::run_with`].
    pub async fn run(&self, input: impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun> {
        self.run_with(skald_runtime::default_registry().as_ref(), input)
            .await
    }

    /// Run on `native` with the shared local dependencies its routes select.
    ///
    /// A step on the `wyrd_gateway` route calls the public gateway through
    /// the client that loaded the Workflow's registered Cards, or else a
    /// client built from the shared configuration. Only when a step resolves
    /// to an `ext_gateway` route is the shared client configuration loaded;
    /// then each selected binding it configures has its secret headers
    /// resolved, and no other binding is read. A selected binding absent from
    /// configuration is refused by Skald before any dispatch. Loading a
    /// Workflow never performs this preparation. Reading the configuration,
    /// building the gateway client, and reading secrets run on Tokio's
    /// blocking pool, so filesystem reads never occupy the polling thread.
    ///
    /// Dropping the future stops the run locally. A model call already sent
    /// to a gateway or provider is not rolled back, and nothing is resent.
    ///
    /// # Errors
    /// Returns the client configuration error when the shared configuration
    /// cannot be read, `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when no gateway
    /// client can be built for a `wyrd_gateway` step or a selected binding is
    /// absent, invalid, or has an unreadable secret, and the other
    /// pre-dispatch errors of [`SkaldWorkflow::run_with_options`]; step
    /// failures are reported in the returned run.
    pub async fn run_with(
        &self,
        native: &ProviderRegistry,
        input: impl Into<WorkflowInput>,
    ) -> WorkflowResult<WorkflowRun> {
        let routes = SelectedRoutes::of(self.inner.spec());
        let (needs_config, needs_gateway) = (routes.needs_config(), routes.needs_gateway());
        let loaded = self.client.clone();
        let (config, gateway) = tokio::task::spawn_blocking(move || {
            load_local_setup(needs_config, needs_gateway, loaded)
        })
        .await
        .map_err(|error| blocking_task_failed("workflow_local_setup", &error))??;
        let dependencies = routes
            .dependencies(native.clone(), &config, gateway)
            .await?;
        self.inner
            .run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
    }

    /// Borrow the hydrated Skald Workflow, for runs with explicit execution
    /// dependencies, limits, or cancellation.
    #[must_use]
    pub fn as_skald(&self) -> &SkaldWorkflow {
        &self.inner
    }

    /// Mutably borrow the hydrated Skald Workflow, for authoring edits that
    /// keep the client that loaded it.
    #[must_use]
    pub fn as_skald_mut(&mut self) -> &mut SkaldWorkflow {
        &mut self.inner
    }

    /// Take the hydrated Skald Workflow, dropping this facade.
    ///
    /// The returned Skald Workflow does not keep the client that loaded the
    /// registered Cards, and its runs get no automatic shared dependencies:
    /// the caller supplies every gateway, binding, and provider through
    /// [`SkaldWorkflow::run_with_options`]. Use [`Self::as_skald`] or
    /// [`Self::as_skald_mut`] to keep them.
    #[must_use]
    pub fn into_skald(self) -> SkaldWorkflow {
        self.inner
    }
}

/// Read the shared configuration and gateway client a run's routes select.
///
/// This is the synchronous filesystem half of [`Workflow::run_with`], which
/// runs it on the blocking pool. The shared configuration is read only when
/// `needs_config`. A gateway client is returned only when `needs_gateway`:
/// `loaded`, the client that loaded the Workflow's registered Cards, when
/// present, else one built from the shared configuration and credentials.
///
/// # Errors
/// Returns the client configuration error when the shared configuration
/// cannot be read, and `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE` when a gateway
/// client is needed and none can be built.
fn load_local_setup(
    needs_config: bool,
    needs_gateway: bool,
    loaded: Option<WyrdClient>,
) -> Result<(LocalWorkflowConfig, Option<WyrdClient>), WyrdError> {
    let config = if needs_config {
        GlobalConfig::load().map_err(WyrdError::from)?.workflow
    } else {
        LocalWorkflowConfig::default()
    };
    let gateway = match (loaded, needs_gateway) {
        (_, false) => None,
        (Some(client), true) => Some(client),
        (None, true) => Some(WyrdClient::from_global().map_err(|error| {
            WyrdError::WorkflowBindingUnavailable {
                message: format!("no Wyrd gateway client is available: {error}"),
                details: serde_json::json!({ "route": "wyrd_gateway" }),
            }
        })?),
    };
    Ok((config, gateway))
}

/// The Workflow error for a blocking-pool task that panicked or was
/// cancelled by runtime shutdown, naming the `boundary` it ran.
fn blocking_task_failed(boundary: &str, error: &JoinError) -> WyrdError {
    WyrdError::WorkflowInternal {
        message: format!("{boundary} task failed: {error}"),
        details: serde_json::json!({ "boundary": boundary }),
    }
}

/// Load the authored bundle at `path` and canonicalize its entry file.
///
/// This is the synchronous filesystem half of [`Workflow::from_path`], which
/// runs it on the blocking pool. The canonical entry identifies the Workflow
/// file among the loaded tree's sources.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` carrying the loader
/// diagnostics when the bundle fails to load, or naming `path` when it cannot
/// be canonicalized.
fn load_bundle(path: &Path) -> Result<(LoadedTree, PathBuf), WyrdError> {
    let tree = wyrd_loader::load(path).map_err(|error| WyrdError::RegistryInvalidCardSpec {
        message: format!("workflow bundle failed to load: {error}"),
        details: serde_json::json!({ "path": path, "diagnostics": error.diagnostics }),
    })?;
    let entry = path.canonicalize().map_err(|error| {
        WyrdError::registry_invalid_card_spec(format!(
            "workflow path {} cannot be resolved: {error}",
            path.display()
        ))
    })?;
    Ok((tree, entry))
}

impl From<SkaldWorkflow> for Workflow {
    /// Wrap an already hydrated Skald Workflow, such as a native builder's.
    fn from(inner: SkaldWorkflow) -> Self {
        Self {
            inner,
            client: None,
        }
    }
}

/// Typed Workflow view over an existing [`Cards`] handle.
///
/// Returned by [`Cards::workflow`]; it shares the handle's transport and
/// credentials and adds no transport of its own.
#[derive(Clone, Copy)]
pub struct WorkflowCards<'a> {
    /// Registry handle every read goes through.
    cards: &'a Cards,
}

impl Cards {
    /// Return the typed Workflow view over this handle.
    #[must_use]
    pub fn workflow(&self) -> WorkflowCards<'_> {
        WorkflowCards { cards: self }
    }
}

impl WorkflowCards<'_> {
    /// Load a registered Workflow and its locked Agent and Prompt graph.
    ///
    /// `selector` must be a Workflow UID selector, an exact reference, or a
    /// named selector with an exact version; existing selector identity
    /// assertions apply. Every Card is read by exact identity along
    /// UID-bearing relationships and must be active, so later versions never
    /// float in. The returned Workflow keeps its registered identity for run
    /// snapshots. Nothing is registered or executed.
    ///
    /// Cancellation may stop after completed registry reads; no partial
    /// Workflow is returned and nothing durable is written.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_400_INVALID_CARD_REF` before any read for a
    /// selector of another kind or a versionless named selector, the Cards
    /// read and traversal errors,
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for an inactive Card, and the
    /// hydration and validation errors of [`SkaldWorkflow::from_card_bodies`].
    pub async fn load(&self, selector: &CardSelector) -> Result<Workflow, WyrdError> {
        let kind = match selector {
            CardSelector::Named { version: None, .. } => {
                return Err(WyrdError::WorkflowInvalidCardRef {
                    message: "registered Workflow loading requires an exact version".to_owned(),
                    details: serde_json::json!({ "field": "version" }),
                });
            }
            CardSelector::Named { kind, .. } | CardSelector::Uid { kind, .. } => kind,
            CardSelector::Exact(card_ref) => &card_ref.kind,
        };
        if *kind != CardKind::Workflow {
            return Err(WyrdError::WorkflowInvalidCardRef {
                message: format!(
                    "registered Workflow loading requires a Workflow selector, not {}",
                    kind.wire_name()
                ),
                details: serde_json::json!({ "field": "kind" }),
            });
        }
        let inner = CardGraphHydrator::new(self.cards.registry_context())
            .load_workflow(selector)
            .await?;
        Ok(Workflow {
            inner,
            client: Some(self.cards.engine.client.clone()),
        })
    }
}
/// Authored-file loading over the checked-in code-review bundle.
#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, PoisonError};

    use async_trait::async_trait;
    use secrecy::SecretString;
    use serde_json::{Value, json};
    use skald_providers::ProviderError;
    use skald_spec::ProviderResponse;
    use skald_spec::wire::openai_chat::OpenAiChatResponse;
    use skald_workflow::{
        WorkflowExecutionDependencies, WorkflowRunOptions, WorkflowRunStatus, WyrdGatewayCall,
        WyrdGatewayCaller,
    };
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::auth::AuthMiddleware;
    use crate::config::ClientConfig;
    use crate::transport::HttpTransport;
    use crate::transport::config::HttpConfig;
    use crate::transport::credential::ResolvedCredential;

    /// Deterministic gateway answering each reviewer by its system role.
    #[derive(Default)]
    struct ReviewGateway {
        /// Serialized requests in arrival order.
        requests: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl WyrdGatewayCaller for ReviewGateway {
        /// Answer with a fixed review per reviewer, recording the request.
        ///
        /// # Errors
        /// Never returns an error; every request receives its fixed review.
        async fn call(
            &self,
            call: WyrdGatewayCall,
            _cancellation: &CancellationToken,
        ) -> Result<ProviderResponse, ProviderError> {
            let request = serde_json::to_string(&call.request).unwrap_or_default();
            let text = if request.contains("security reviewer") {
                "SECURITY-FINDINGS"
            } else if request.contains("correctness reviewer") {
                "CORRECTNESS-FINDINGS"
            } else {
                "FINAL-REVIEW"
            };
            self.requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(request);
            Ok(chat_text(text))
        }
    }

    /// One OpenAI Chat completion carrying `text`.
    ///
    /// # Panics
    /// Panics if the static completion fixture stops decoding, which is a
    /// test-fixture invariant.
    fn chat_text(text: &str) -> ProviderResponse {
        let response: OpenAiChatResponse = serde_json::from_value(json!({
            "id": "resp",
            "object": "chat.completion",
            "created": 0,
            "model": "gpt-5-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": text },
                "finish_reason": "stop"
            }]
        }))
        .expect("static completion decodes");
        ProviderResponse::OpenAiChatCompletion(response)
    }

    /// The checked-in code-review bundle directory.
    fn bundle() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../examples/workflows/code-review")
    }

    /// Copy the bundle's Agent and Prompt Cards into a temp directory with
    /// `edit` applied to the Workflow YAML.
    ///
    /// # Panics
    /// Panics when a bundle file cannot be read, copied, or written.
    fn edited_bundle(edit: impl Fn(String) -> String) -> TempDir {
        let temp = TempDir::new().expect("temp directory creates");
        for dir in ["agents", "prompts"] {
            std::fs::create_dir(temp.path().join(dir)).expect("bundle directory creates");
            for file in ["security", "correctness", "final-reviewer"] {
                std::fs::copy(
                    bundle().join(format!("{dir}/{file}.yaml")),
                    temp.path().join(format!("{dir}/{file}.yaml")),
                )
                .expect("bundle file copies");
            }
        }
        let workflow =
            std::fs::read_to_string(bundle().join("workflow.yaml")).expect("workflow reads");
        std::fs::write(temp.path().join("workflow.yaml"), edit(workflow)).expect("workflow writes");
        temp
    }

    /// Load and run the checked-in bundle from its entry file: path-loaded
    /// Prompt Cards back each Agent and both reviewers feed the final
    /// reviewer's declared Prompt variables through the existing binder. The
    /// wholly local bundle loads without constructing a client, so it makes no
    /// registry request. An external ref — including one naming the same
    /// identity as a loaded sibling — must be read from the registry and is
    /// refused when it cannot be; extra Prompt bindings and a route/dialect
    /// mismatch are refused at load. No refused load dispatches a call.
    ///
    /// # Panics
    /// Panics when the bundle stops loading or running as asserted, or a
    /// refusal is missing or carries the wrong code.
    #[tokio::test]
    async fn from_path_uses_existing_loader() {
        let workflow = Workflow::from_path(bundle().join("workflow.yaml"))
            .await
            .expect("local bundle loads without a registry");
        assert_eq!(
            workflow.as_skald().step_ids(),
            vec!["security", "correctness", "final_review"]
        );

        let gateway = Arc::new(ReviewGateway::default());
        let dependencies = WorkflowExecutionDependencies::new(ProviderRegistry::new())
            .with_wyrd_gateway(Arc::clone(&gateway) as Arc<dyn WyrdGatewayCaller>);
        let input = serde_json::Map::from_iter([(
            "code".to_owned(),
            json!("diff --git a/src/auth.rs b/src/auth.rs"),
        )]);
        let run = workflow
            .as_skald()
            .run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
            .expect("run starts");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(run.outputs["review"], json!("FINAL-REVIEW"));
        assert!(run.workflow.is_none());
        let requests = gateway
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(requests.len(), 3);
        let last = requests.last().expect("final request recorded");
        assert!(last.contains("SECURITY-FINDINGS") && last.contains("CORRECTNESS-FINDINGS"));
        assert!(last.contains("diff --git a/src/auth.rs"));

        let shadowed = edited_bundle(|yaml| {
            yaml.replacen(
                "    - id: correctness",
                "    - id: registered_security\n      action:\n        type: agent\n        target:\n          kind: Agent\n          name: security-reviewer\n          version: \"1.0.0\"\n      inputs:\n        code: input.code\n\n    - id: correctness",
                1,
            )
        });
        Workflow::from_path(shadowed.path().join("workflow.yaml"))
            .await
            .expect_err("a loaded sibling never satisfies an external ref");

        let extra = edited_bundle(|yaml| {
            yaml.replacen(
                "        code: input.code\n      timeout_seconds",
                "        code: input.code\n        extra: input.code\n      timeout_seconds",
                1,
            )
        });
        let error = Workflow::from_path(extra.path().join("workflow.yaml"))
            .await
            .expect_err("extra binding is refused");
        assert_eq!(error.code(), "WYRD_WORKFLOW_422_VALIDATION");
        assert!(error.to_string().contains("steps[0].inputs.extra"));

        let dialect = edited_bundle(|yaml| {
            yaml.replacen(
                "    kind: wyrd_gateway",
                "    kind: ext_gateway\n    protocol: anthropic_messages\n    base_url: https://gateway.example.com\n    credential_binding: review-gateway",
                1,
            )
        });
        let error = Workflow::from_path(dialect.path().join("workflow.yaml"))
            .await
            .expect_err("route dialect mismatch is refused");
        assert_eq!(error.code(), "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED");
        assert_eq!(
            gateway
                .requests
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len(),
            3,
            "refused loads dispatch nothing"
        );
    }

    /// Write `contents` to an owner-only secret file inside `dir`.
    ///
    /// # Panics
    /// Panics when the file cannot be written or restricted.
    fn secret_file(dir: &Path, contents: &str) -> PathBuf {
        let path = dir.join("review-secret");
        std::fs::write(&path, contents).expect("secret writes");
        #[cfg(unix)]
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .expect("secret restricts");
        path
    }

    /// Parse a `[workflow]` client configuration section.
    ///
    /// # Panics
    /// Panics when the TOML is not a valid client configuration.
    fn workflow_config(toml: &str) -> LocalWorkflowConfig {
        toml::from_str::<GlobalConfig>(toml)
            .expect("client configuration parses")
            .workflow
    }

    /// Run `workflow` on the dependencies its routes select from `config`.
    ///
    /// # Errors
    /// Returns the dependency preparation and pre-dispatch run errors.
    async fn run_selected(
        workflow: &Workflow,
        config: &LocalWorkflowConfig,
    ) -> WorkflowResult<WorkflowRun> {
        let dependencies = SelectedRoutes::of(workflow.as_skald().spec())
            .dependencies(ProviderRegistry::new(), config, None)
            .await?;
        let input = serde_json::Map::from_iter([("code".to_owned(), json!("diff"))]);
        workflow
            .as_skald()
            .run_with_options(&dependencies, input, WorkflowRunOptions::default())
            .await
    }

    /// An `ext_gateway` bundle runs through the binding named in the shared
    /// client configuration: loading makes no call, only the selected
    /// binding's secret is read and sent, and an unused binding naming an
    /// unreadable secret is never touched. An absent binding, a binding for
    /// another protocol, and an unreadable selected secret are refused before
    /// any dispatch. A `wyrd_gateway` Workflow selects no configuration and
    /// calls the public ingress through the client it carries.
    ///
    /// # Panics
    /// Panics when a run, refusal, or upstream request differs from the
    /// asserted behavior.
    #[tokio::test]
    async fn selected_local_dependencies_use_shared_config() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(json!({
                "id": "resp",
                "object": "chat.completion",
                "created": 0,
                "model": "gpt-5-5",
                "choices": [{
                    "index": 0,
                    "message": { "role": "assistant", "content": "REVIEWED" },
                    "finish_reason": "stop"
                }]
            })))
            .mount(&server)
            .await;
        let route = format!(
            "    kind: ext_gateway\n    protocol: openai_chat\n    base_url: {}/v1\n    credential_binding: review-gateway",
            server.uri()
        );
        let external = edited_bundle(|yaml| yaml.replacen("    kind: wyrd_gateway", &route, 1));
        let workflow = Workflow::from_path(external.path().join("workflow.yaml"))
            .await
            .expect("ext_gateway bundle loads");
        assert!(
            server
                .received_requests()
                .await
                .unwrap_or_default()
                .is_empty(),
            "loading makes no call"
        );

        let secret = secret_file(external.path(), "s3cret");
        let configured = workflow_config(&format!(
            r#"
            [workflow.external_gateway_bindings.review-gateway]
            protocol = "openai_chat"
            origin = "{origin}"
            secret_headers = {{ x-review-secret = {{ source = "file", path = "{secret}" }} }}

            [workflow.external_gateway_bindings.unused]
            protocol = "openai_chat"
            origin = "{origin}"
            secret_headers = {{ x-unused = {{ source = "file", path = "{missing}" }} }}
            "#,
            origin = server.uri(),
            secret = secret.display(),
            missing = external.path().join("missing").display(),
        ));
        let run = run_selected(&workflow, &configured)
            .await
            .expect("configured binding runs");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        assert_eq!(run.outputs["review"], json!("REVIEWED"));
        let requests = server.received_requests().await.unwrap_or_default();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| {
            request
                .headers
                .get("x-review-secret")
                .map(|value| value.as_bytes())
                == Some(b"s3cret".as_slice())
                && !request.headers.contains_key("x-unused")
        }));
        assert!(
            !serde_json::to_string(&run)
                .unwrap_or_default()
                .contains("s3cret")
        );

        for (config, code) in [
            (
                LocalWorkflowConfig::default(),
                "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
            ),
            (
                workflow_config(&format!(
                    "[workflow.external_gateway_bindings.review-gateway]\nprotocol = \"anthropic_messages\"\norigin = \"{}\"\n",
                    server.uri()
                )),
                "WYRD_WORKFLOW_422_ROUTE_UNSUPPORTED",
            ),
            (
                workflow_config(&format!(
                    "[workflow.external_gateway_bindings.review-gateway]\nprotocol = \"openai_chat\"\norigin = \"{}\"\nsecret_headers = {{ x-review-secret = {{ source = \"file\", path = \"{}\" }} }}\n",
                    server.uri(),
                    external.path().join("missing").display()
                )),
                "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE",
            ),
        ] {
            let error = run_selected(&workflow, &config)
                .await
                .expect_err("unusable binding is refused");
            assert_eq!(error.code(), code);
        }
        assert_eq!(
            server.received_requests().await.unwrap_or_default().len(),
            3,
            "refused runs dispatch nothing"
        );

        // A wyrd_gateway Workflow calls the public ingress through the client
        // it carries, with the Prompt's model and no ext_gateway preparation.
        let wyrd = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::to_value(match chat_text("REVIEWED") {
                        ProviderResponse::OpenAiChatCompletion(response) => response,
                        _ => unreachable!("chat_text builds a chat completion"),
                    })
                    .expect("completion serializes"),
                ),
            )
            .mount(&wyrd)
            .await;
        let loaded = Workflow::from_path(bundle().join("workflow.yaml"))
            .await
            .expect("local bundle loads");
        let routes = SelectedRoutes::of(loaded.as_skald().spec());
        assert!(routes.needs_gateway() && !routes.needs_config());
        let gateway = Workflow {
            inner: loaded.into_skald(),
            client: Some(bearer_client(&wyrd.uri())),
        };
        let input = serde_json::Map::from_iter([("code".to_owned(), json!("diff"))]);
        let run = gateway
            .run_with(&ProviderRegistry::new(), input)
            .await
            .expect("gateway run starts");
        assert_eq!(run.status, WorkflowRunStatus::Succeeded);
        let requests = wyrd.received_requests().await.unwrap_or_default();
        assert_eq!(requests.len(), 3);
        assert!(requests.iter().all(|request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
            body["model"] == json!("openai/gpt-5-5")
                && request
                    .headers
                    .get("x-wyrd-access-token")
                    .map(|value| value.as_bytes())
                    == Some(b"Bearer test-bearer".as_slice())
        }));
    }

    /// A bearer-authenticated client pointed at `base_url`.
    ///
    /// # Panics
    /// Panics when the fixed test client cannot be assembled.
    fn bearer_client(base_url: &str) -> WyrdClient {
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.to_owned();
        let auth = AuthMiddleware::new(
            &config,
            ResolvedCredential::BearerToken(SecretString::from("test-bearer")),
        )
        .expect("auth builds");
        let transport = HttpTransport::new(
            &HttpConfig {
                base_url: base_url.to_owned(),
                ..HttpConfig::default()
            },
            Arc::clone(&auth),
        )
        .expect("transport builds");
        WyrdClient::from_parts(auth, transport, config.grpc)
    }
}
