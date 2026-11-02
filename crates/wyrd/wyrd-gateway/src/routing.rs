//! Deterministic call planning: the requested target and its fallbacks.
//!
//! Planning is pure. Every candidate is one `<deployment>/<model>` address
//! naming exactly one configured deployment. The requested address comes
//! first, followed by the first defined fallback source — request override,
//! exact `Model` rule, `Operation` rule, then `Global` — without merging
//! rules. Candidates equal to the requested address or already listed are
//! skipped, and a candidate whose deployment is missing, does not serve the
//! operation, or does not permit the model is dropped; a deployment whose
//! adapter cannot represent the request body is dropped later by
//! [`CallPlan::retain_representable`].

use serde_json::Value;
use skald_spec::ProviderName;
use wyrd_spec::gateway::{
    FallbackScope, GatewayCallId, GatewayFallbackOverride, GatewayOperation, ModelRef,
    ProviderAdapter, ProviderDeployment,
};
use wyrd_spec::ids::ProviderDeploymentName;

use crate::adapter::{IngressDialect, MediaRequest, UnsupportedRequest, adapter_name, prepare};
use crate::snapshot::GatewayTenantSnapshot;

/// One address the call may use and the deployment it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCandidate {
    /// Exact deployment/model address.
    pub model: ModelRef,
    /// The deployment the address names.
    pub deployment: ProviderDeployment,
}

/// Immutable ordered candidates for one admitted call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPlan {
    /// Logical call identity.
    pub call_id: GatewayCallId,
    /// Requested operation.
    pub operation: GatewayOperation,
    /// Address the caller asked for.
    pub requested: ModelRef,
    /// Candidates in attempt order; the requested address first when its
    /// deployment can serve it.
    pub candidates: Vec<PlannedCandidate>,
}

impl CallPlan {
    /// Plans `requested` plus its fallback candidates over `snapshot`.
    ///
    /// `fallback` is the caller's validated per-request override; when present
    /// it replaces every tenant rule.
    #[must_use]
    pub fn new(
        snapshot: &GatewayTenantSnapshot,
        call_id: GatewayCallId,
        operation: GatewayOperation,
        requested: ModelRef,
        fallback: Option<&GatewayFallbackOverride>,
    ) -> Self {
        let rules = &snapshot.fallback.rules;
        let rule = |matches: &dyn Fn(&FallbackScope) -> bool| {
            rules
                .iter()
                .find(|rule| matches(&rule.scope))
                .map(|rule| rule.candidates.as_slice())
        };
        let fallback = fallback
            .map(|value| value.candidates.as_slice())
            .or_else(|| rule(&|scope| matches!(scope, FallbackScope::Model { model } if *model == requested)))
            .or_else(|| rule(&|scope| matches!(scope, FallbackScope::Operation { operation: op } if *op == operation)))
            .or_else(|| rule(&|scope| matches!(scope, FallbackScope::Global)))
            .unwrap_or_default();

        let mut models: Vec<&ModelRef> = vec![&requested];
        for model in fallback {
            if !models.contains(&model) {
                models.push(model);
            }
        }
        let candidates = models
            .into_iter()
            .filter_map(|model| {
                snapshot
                    .deployment(&model.deployment)
                    .filter(|deployment| {
                        deployment.serves(operation) && deployment.permits(&model.model)
                    })
                    .map(|deployment| PlannedCandidate {
                        model: model.clone(),
                        deployment: deployment.clone(),
                    })
            })
            .collect();
        Self {
            call_id,
            operation,
            requested,
            candidates,
        }
    }

    /// Restricts the plan to the requested address on exactly `deployment`.
    ///
    /// A call that continues provider-held state, such as a batch or its files,
    /// must reach the deployment holding that state, so fallback candidates are
    /// dropped. The plan is empty when the requested address no longer names
    /// `deployment` or it no longer serves the operation.
    pub fn pin(&mut self, deployment: &ProviderDeploymentName) {
        let requested = &self.requested;
        self.candidates.retain(|candidate| {
            candidate.model == *requested && candidate.deployment.name == *deployment
        });
    }

    /// Keeps only candidates for which `keep` returns true, preserving order.
    pub fn retain(&mut self, mut keep: impl FnMut(&ModelRef) -> bool) {
        self.candidates.retain(|candidate| keep(&candidate.model));
    }

    /// Refuses an Agent call whose requested deployment's adapter is outside
    /// the initial-target compatibility matrix of its Prompt `provider`.
    ///
    /// `openai` and custom Prompts need an `openai` or `openai_compatible`
    /// adapter, `anthropic` an `anthropic`, `google` a `gemini`, `vertex` a
    /// `vertex`, and `jev` a `jev` adapter. Only the requested address is
    /// bound: fallback candidates keep ordinary conversion, and a plan whose
    /// requested deployment no longer serves the call is left to the
    /// unavailable-model refusal.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedRequest`] naming `model` when the requested
    /// deployment's adapter cannot serve a `provider` Prompt.
    pub fn require_compatible_initial(
        &self,
        provider: &ProviderName,
    ) -> Result<(), UnsupportedRequest> {
        let Some(initial) = self
            .candidates
            .iter()
            .find(|candidate| candidate.model == self.requested)
        else {
            return Ok(());
        };
        let compatible = matches!(
            (provider, &initial.deployment.adapter),
            (
                ProviderName::OpenAi | ProviderName::Custom(_),
                ProviderAdapter::OpenAi | ProviderAdapter::OpenAiCompatible { .. },
            ) | (ProviderName::Anthropic, ProviderAdapter::Anthropic)
                | (ProviderName::Google, ProviderAdapter::Gemini)
                | (ProviderName::Vertex, ProviderAdapter::Vertex { .. })
                | (ProviderName::Jev, ProviderAdapter::Jev)
        );
        if compatible {
            Ok(())
        } else {
            Err(UnsupportedRequest::new(
                "model",
                format!(
                    "deployment `{}` uses the {} adapter, which cannot serve a {provider} Prompt",
                    initial.deployment.name,
                    adapter_name(&initial.deployment.adapter)
                ),
            ))
        }
    }

    /// Drops candidates whose adapter cannot represent `body` in `ingress`
    /// with its `media` route and files.
    ///
    /// Runs before admission so an unrepresentable request never reaches a
    /// provider or consumes budget, while a request some candidate can carry
    /// still routes to it.
    ///
    /// # Errors
    ///
    /// Returns the first [`UnsupportedRequest`] when no candidate can
    /// represent the request; the plan is then empty.
    pub fn retain_representable(
        &mut self,
        ingress: IngressDialect,
        stream: bool,
        body: &Value,
        media: Option<&MediaRequest>,
    ) -> Result<(), UnsupportedRequest> {
        let operation = self.operation;
        let mut first = None;
        self.candidates.retain(|candidate| {
            match prepare(
                ingress,
                operation,
                stream,
                body,
                &candidate.deployment,
                &candidate.model.model,
                media,
            ) {
                Ok(_) => true,
                Err(error) => {
                    first.get_or_insert(error);
                    false
                }
            }
        });
        match first {
            Some(error) if self.candidates.is_empty() => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wyrd_spec::gateway::{GatewayCapturePolicy, GatewayFallbackPolicy};

    /// Builds an OpenAI-compatible deployment `name` with optional model and
    /// capability lists.
    ///
    /// # Panics
    /// Panics when the fixture is not a valid deployment.
    fn deployment(
        name: &str,
        models: Option<&[&str]>,
        capabilities: Option<&[&str]>,
    ) -> ProviderDeployment {
        let mut value = json!({
            "name": name,
            "adapter": {"openai_compatible": {"base_url": "https://acme.example/v1"}},
            "auth": "none",
        });
        if let Some(models) = models {
            value["models"] = json!(models);
        }
        if let Some(capabilities) = capabilities {
            value["capabilities"] = json!(capabilities);
        }
        serde_json::from_value(value).expect("deployment decodes")
    }

    /// Builds a snapshot from deployments and a fallback policy document.
    fn snapshot(
        mut deployments: Vec<ProviderDeployment>,
        fallback: serde_json::Value,
    ) -> GatewayTenantSnapshot {
        deployments.sort_by(|a, b| a.name.cmp(&b.name));
        GatewayTenantSnapshot {
            deployments,
            credentials: Vec::new(),
            fallback: serde_json::from_value::<GatewayFallbackPolicy>(fallback).expect("fallback"),
            governance: wyrd_spec::gateway::GatewayGovernancePolicy::default(),
            capture: serde_json::from_value::<GatewayCapturePolicy>(
                json!({"mode": "disabled", "payload_fields": [], "version": 1}),
            )
            .expect("capture"),
        }
    }

    /// Parses an address.
    ///
    /// # Panics
    /// Panics when `value` is not an address.
    fn model(value: &str) -> ModelRef {
        ModelRef::from_projection(value).expect("model")
    }

    /// Candidate addresses of a plan.
    fn models(plan: &CallPlan) -> Vec<String> {
        plan.candidates
            .iter()
            .map(|c| c.model.to_string())
            .collect()
    }

    /// Plans a chat call for `requested` with no override.
    fn chat(snap: &GatewayTenantSnapshot, requested: &str) -> CallPlan {
        CallPlan::new(
            snap,
            GatewayCallId::new_v7(),
            GatewayOperation::ChatCompletions,
            model(requested),
            None,
        )
    }

    /// The deployment prefix selects exactly one target: two same-adapter
    /// deployments stay independent, an unrestricted deployment accepts any
    /// model suffix including slashes, an allowlist and capability set
    /// narrow it, and a missing deployment plans nothing.
    ///
    /// # Panics
    ///
    /// Panics when a plan names another deployment or keeps a target its
    /// deployment does not permit.
    #[test]
    fn named_target_and_allowlist_resolution() {
        let snap = snapshot(
            vec![
                deployment("primary", None, None),
                deployment("dedicated", Some(&["gpt-4o"]), Some(&["chat_completions"])),
            ],
            json!({"rules": []}),
        );
        let primary = chat(&snap, "primary/org/model-v3");
        assert_eq!(models(&primary), ["primary/org/model-v3"]);
        assert_eq!(primary.candidates[0].deployment.name.as_str(), "primary");
        assert_eq!(primary.candidates[0].model.model.as_str(), "org/model-v3");
        let dedicated = chat(&snap, "dedicated/gpt-4o");
        assert_eq!(
            dedicated.candidates[0].deployment.name.as_str(),
            "dedicated"
        );
        assert!(
            chat(&snap, "dedicated/gpt-4.1").candidates.is_empty(),
            "an allowlist refuses an unlisted model"
        );
        assert!(chat(&snap, "missing/gpt-4o").candidates.is_empty());
        assert!(
            CallPlan::new(
                &snap,
                GatewayCallId::new_v7(),
                GatewayOperation::Embeddings,
                model("dedicated/gpt-4o"),
                None,
            )
            .candidates
            .is_empty(),
            "declared capabilities narrow the adapter's operations"
        );
        assert_eq!(
            models(&CallPlan::new(
                &snap,
                GatewayCallId::new_v7(),
                GatewayOperation::Embeddings,
                model("primary/text-embedding-3-small"),
                None,
            )),
            ["primary/text-embedding-3-small"],
            "omitted capabilities permit every adapter operation"
        );
    }

    /// An Agent's requested deployment must use an adapter its Prompt
    /// provider is compatible with, whatever the deployment is named, while
    /// an incompatible fallback candidate and a missing requested deployment
    /// are left to ordinary routing.
    ///
    /// # Panics
    ///
    /// Panics when a compatible pair is refused or an incompatible one is
    /// admitted.
    #[test]
    fn agent_initial_target_follows_the_prompt_provider_matrix() {
        let native = |name: &str, adapter: Value| -> ProviderDeployment {
            serde_json::from_value(json!({"name": name, "adapter": adapter, "auth": "none"}))
                .expect("deployment decodes")
        };
        let snap = snapshot(
            vec![
                native("openai", json!("openai")),
                deployment("compat", None, None),
                native("claude", json!("anthropic")),
                native("vertex", json!("gemini")),
                native(
                    "gcp",
                    json!({"vertex": {"project": "acme", "location": "us-central1"}}),
                ),
                native("judge", json!("jev")),
            ],
            json!({"rules": [{"scope": "global", "candidates": [model("claude/m")]}]}),
        );
        let provider = |name: &str| name.parse::<ProviderName>().expect("provider");
        for (prompt, deployment, compatible) in [
            ("openai", "openai", true),
            ("openai", "compat", true),
            ("local", "compat", true),
            ("anthropic", "claude", true),
            ("google", "vertex", true),
            ("vertex", "gcp", true),
            ("jev", "judge", true),
            ("openai", "claude", false),
            ("openai", "gcp", false),
            ("google", "gcp", false),
            ("vertex", "vertex", false),
            ("anthropic", "openai", false),
            ("jev", "openai", false),
        ] {
            let plan = chat(&snap, &format!("{deployment}/m"));
            let outcome = plan.require_compatible_initial(&provider(prompt));
            assert_eq!(outcome.is_ok(), compatible, "{prompt} -> {deployment}");
            if let Err(error) = outcome {
                assert_eq!(error.field, "model");
            }
        }
        assert!(
            chat(&snap, "missing/m")
                .require_compatible_initial(&provider("openai"))
                .is_ok(),
            "a missing requested deployment is the unavailable-model refusal"
        );
    }

    /// Override, exact model, operation, and global rules apply in that
    /// precedence without merging; the requested address and duplicates are
    /// skipped and incapable candidates are dropped.
    ///
    /// # Panics
    /// Panics when a fixture is invalid or the plan holds other candidates.
    #[test]
    fn fallback_precedence_is_first_defined_without_merging() {
        let chat_only = Some(&["chat_completions"][..]);
        let snap = snapshot(
            vec![
                deployment("dep-a", None, chat_only),
                deployment("dep-b", None, chat_only),
                deployment("dep-c", None, chat_only),
                deployment("dep-d", None, chat_only),
                deployment("dep-e", None, Some(&["embeddings"])),
            ],
            json!({"rules": [
                {"scope": "global", "candidates": [model("dep-a/a"), model("dep-d/d")]},
                {"scope": {"operation": {"operation": "chat_completions"}}, "candidates": [model("dep-c/c"), model("dep-e/e")]},
                {"scope": {"model": {"model": model("dep-a/a")}}, "candidates": [model("dep-b/b")]},
            ]}),
        );
        let call = GatewayCallId::new_v7();
        let chat = GatewayOperation::ChatCompletions;
        let plan = |requested: &str, fallback: Option<GatewayFallbackOverride>| {
            CallPlan::new(&snap, call, chat, model(requested), fallback.as_ref())
        };

        let over: GatewayFallbackOverride =
            serde_json::from_value(json!({"candidates": [model("dep-d/d"), model("dep-c/c")]}))
                .expect("override");
        assert_eq!(
            models(&plan("dep-a/a", Some(over))),
            ["dep-a/a", "dep-d/d", "dep-c/c"]
        );
        assert_eq!(models(&plan("dep-a/a", None)), ["dep-a/a", "dep-b/b"]);
        assert_eq!(
            models(&plan("dep-b/b", None)),
            ["dep-b/b", "dep-c/c"],
            "operation rule wins over global; incapable dep-e is dropped"
        );
        assert!(
            CallPlan::new(
                &snap,
                call,
                GatewayOperation::Embeddings,
                model("dep-d/d"),
                None
            )
            .candidates
            .is_empty(),
            "an incapable requested address and incapable global candidates are dropped"
        );
    }

    /// Pinning keeps only the requested address on the named deployment,
    /// dropping fallback candidates, and empties the plan for another name.
    ///
    /// # Panics
    ///
    /// Panics when a pinned plan keeps another deployment or address.
    #[test]
    fn pin_keeps_only_the_requested_address_on_its_deployment() {
        let snap = snapshot(
            vec![
                deployment("dep-a", None, None),
                deployment("dep-b", None, None),
            ],
            json!({"rules": [{"scope": "global", "candidates": [model("dep-b/m")]}]}),
        );
        let plan = || {
            CallPlan::new(
                &snap,
                GatewayCallId::new_v7(),
                GatewayOperation::Batches,
                model("dep-a/m"),
                None,
            )
        };
        let name = |value: &str| ProviderDeploymentName::new(value).expect("name");
        let mut pinned = plan();
        assert_eq!(models(&pinned), ["dep-a/m", "dep-b/m"]);
        pinned.pin(&name("dep-a"));
        assert_eq!(models(&pinned), ["dep-a/m"]);
        let mut fallback = plan();
        fallback.pin(&name("dep-b"));
        assert!(
            fallback.candidates.is_empty(),
            "a fallback deployment is not the pin"
        );
    }

    /// Fallback stays within the protocol by capability and representability
    /// alone: a `systemone` question keeps only deployments serving that
    /// operation, and an ordinary chat body drops every Jev deployment because
    /// the Jev adapter cannot represent it.
    ///
    /// # Panics
    ///
    /// Panics when a plan keeps a candidate that cannot carry the request.
    #[test]
    fn fallback_keeps_only_capable_and_representable_candidates() {
        let jev = |name: &str| -> ProviderDeployment {
            serde_json::from_value(json!({
                "name": name,
                "adapter": "jev",
                "auth": {"bearer": {"credential": "jev-key"}},
            }))
            .expect("jev deployment decodes")
        };
        let snap = snapshot(
            vec![jev("jev-a"), jev("jev-b"), deployment("chat", None, None)],
            json!({"rules": [{"scope": "global", "candidates": [
                model("jev-a/jev-latest"), model("jev-b/jev-preview"), model("chat/gpt-4o"),
            ]}]}),
        );
        let call = GatewayCallId::new_v7();
        assert_eq!(
            models(&CallPlan::new(
                &snap,
                call,
                GatewayOperation::SystemOne,
                model("jev-b/jev-preview"),
                None
            )),
            ["jev-b/jev-preview", "jev-a/jev-latest"],
            "only systemone-capable deployments answer a Jev question"
        );
        let mut chat = CallPlan::new(
            &snap,
            call,
            GatewayOperation::ChatCompletions,
            model("chat/gpt-4o"),
            None,
        );
        chat.retain_representable(
            IngressDialect::OpenAi,
            false,
            &json!({"model": "chat/gpt-4o", "messages": [{"role": "user", "content": "hi"}]}),
            None,
        )
        .expect("ordinary chat is representable");
        assert_eq!(
            models(&chat),
            ["chat/gpt-4o"],
            "an ordinary chat body is not a Jev request"
        );
    }
}
