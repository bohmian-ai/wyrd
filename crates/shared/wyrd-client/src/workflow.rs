//! Client Workflow loading: authored files and registered Workflow Cards
//! hydrated into the one Skald runtime.
//!
//! [`Workflow`] is a thin facade over [`skald_workflow::Workflow`]. It owns no
//! graph, parser, validation rule, or executor: `wyrd_loader` parses the
//! authored bundle, the Cards graph owner reads exact registered bodies, and
//! Skald hydrates, validates, and runs. Rust cannot attach client IO methods
//! to the foreign Skald type, so loading lives here. Loading never registers,
//! executes, or resolves execution secrets.

use std::path::Path;

use skald_workflow::{Workflow as SkaldWorkflow, WorkflowInput, WorkflowResult, WorkflowRun};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;

use crate::cards::{CardGraphHydrator, CardSelector, Cards, WorkflowBodies};

/// A loaded, validated Workflow ready to run on the local Skald runtime.
#[derive(Debug, Clone)]
pub struct Workflow {
    /// The hydrated Skald Workflow every run delegates to.
    inner: SkaldWorkflow,
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
    /// Cancellation may stop after completed filesystem or registry reads; no
    /// partial Workflow is returned and nothing durable is written.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` carrying the loader
    /// diagnostics when the bundle fails to load or `path` does not define
    /// exactly one Workflow Card; the client configuration error when an
    /// external ref needs a client that cannot be built; the Cards read and
    /// traversal errors for a missing, denied, or mismatched dependency;
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for an inactive dependency;
    /// and the hydration and validation errors of
    /// [`SkaldWorkflow::from_card_bodies`].
    pub async fn from_path(path: impl AsRef<Path>) -> Result<Self, WyrdError> {
        let path = path.as_ref();
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
        let (workflow, mut bodies) = WorkflowBodies::authored(&tree, &entry)?;
        let refs = bodies.external_refs(&workflow);
        if !refs.is_empty() {
            let cards = Cards::new(None, None)?;
            CardGraphHydrator::new(cards.registry_context())
                .resolve_external(&mut bodies, &refs)
                .await?;
        }
        bodies.hydrate(workflow).map(Self::from)
    }

    /// Run on the process-default native provider registry.
    ///
    /// # Errors
    /// Returns the pre-dispatch errors of [`SkaldWorkflow::run`]; step
    /// failures are reported in the returned run.
    pub async fn run(&self, input: impl Into<WorkflowInput>) -> WorkflowResult<WorkflowRun> {
        self.inner.run(input).await
    }

    /// Borrow the hydrated Skald Workflow, for runs with explicit execution
    /// dependencies, limits, or cancellation.
    #[must_use]
    pub fn as_skald(&self) -> &SkaldWorkflow {
        &self.inner
    }

    /// Take the hydrated Skald Workflow.
    #[must_use]
    pub fn into_skald(self) -> SkaldWorkflow {
        self.inner
    }
}

impl From<SkaldWorkflow> for Workflow {
    /// Wrap an already hydrated Skald Workflow, such as a native builder's.
    fn from(inner: SkaldWorkflow) -> Self {
        Self { inner }
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
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for a selector of
    /// another kind, `WYRD_REGISTRY_400_VERSION_REQUIRED` for a versionless
    /// named selector, the Cards read and traversal errors,
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` for an inactive Card, and the
    /// hydration and validation errors of [`SkaldWorkflow::from_card_bodies`].
    pub async fn load(&self, selector: &CardSelector) -> Result<Workflow, WyrdError> {
        let kind = match selector {
            CardSelector::Named { version: None, .. } => {
                return Err(WyrdError::RegistryVersionRequired {
                    message: "registered Workflow loading requires an exact version".to_owned(),
                    details: serde_json::json!({}),
                });
            }
            CardSelector::Named { kind, .. } | CardSelector::Uid { kind, .. } => kind,
            CardSelector::Exact(card_ref) => &card_ref.kind,
        };
        if *kind != CardKind::Workflow {
            return Err(WyrdError::registry_invalid_card_spec(format!(
                "registered Workflow loading requires a Workflow selector, not {}",
                kind.wire_name()
            )));
        }
        CardGraphHydrator::new(self.cards.registry_context())
            .load_workflow(selector)
            .await
            .map(Workflow::from)
    }
}
/// Authored-file loading over the checked-in code-review bundle.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use serde_json::json;
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
                .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        let dependencies =
            WorkflowExecutionDependencies::new(skald_runtime::ProviderRegistry::new())
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
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .len(),
            3,
            "refused loads dispatch nothing"
        );
    }
}
