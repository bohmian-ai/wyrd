//! Deterministic call planning: fallback candidates and deployment order.
//!
//! Planning is pure. The requested model comes first, followed by the first
//! defined fallback source — request override, exact `Model` rule,
//! `Operation` rule, then `Global` — without merging rules. Candidates equal
//! to the requested model or already listed are skipped, and a candidate with
//! no deployment declaring the operation is dropped. Each remaining
//! candidate's deployments are ordered by weighted selection keyed by the
//! logical call id, so every replica picks the same first deployment.

use serde_json::Value;
use wyrd_spec::gateway::{
    FallbackScope, GatewayCallId, GatewayFallbackOverride, GatewayOperation, ModelRef,
    ProviderDeployment,
};
use wyrd_spec::ids::ProviderDeploymentName;

use crate::adapter::{IngressDialect, MediaRequest, UnsupportedRequest, prepare};
use crate::snapshot::GatewayTenantSnapshot;

/// One model the call may use, with its deployments in attempt order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCandidate {
    /// Exact model.
    pub model: ModelRef,
    /// Capable deployments in deterministic attempt order.
    pub deployments: Vec<ProviderDeployment>,
}

/// Immutable ordered candidates for one admitted call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallPlan {
    /// Logical call identity keying deployment selection.
    pub call_id: GatewayCallId,
    /// Requested operation.
    pub operation: GatewayOperation,
    /// Model the caller asked for.
    pub requested: ModelRef,
    /// Candidates in attempt order; the requested model first when capable.
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
                let capable: Vec<&ProviderDeployment> = snapshot
                    .deployments
                    .iter()
                    .filter(|d| d.model == *model && d.capabilities.contains(&operation))
                    .collect();
                (!capable.is_empty()).then(|| PlannedCandidate {
                    model: model.clone(),
                    deployments: weighted_order(&capable, call_id)
                        .into_iter()
                        .cloned()
                        .collect(),
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

    /// Restricts the plan to the requested model on exactly `deployment`.
    ///
    /// A call that continues provider-held state, such as a batch or its files,
    /// must reach the deployment holding that state, so fallback candidates and
    /// sibling deployments are dropped. The plan is empty when `deployment` no
    /// longer serves the requested model's operation.
    pub fn pin(&mut self, deployment: &ProviderDeploymentName) {
        let requested = &self.requested;
        self.candidates.retain_mut(|candidate| {
            candidate
                .deployments
                .retain(|planned| planned.name == *deployment);
            candidate.model == *requested && !candidate.deployments.is_empty()
        });
    }

    /// Keeps only candidates for which `keep` returns true, preserving order.
    pub fn retain(&mut self, mut keep: impl FnMut(&ModelRef) -> bool) {
        self.candidates.retain(|candidate| keep(&candidate.model));
    }

    /// Drops deployments whose adapter cannot represent `body` in `ingress`
    /// with its `media` route and files, then candidates left without
    /// deployments.
    ///
    /// Runs before admission so an unrepresentable request never reaches a
    /// provider or consumes budget, while a request some candidate can carry
    /// still routes to it.
    ///
    /// # Errors
    ///
    /// Returns the first [`UnsupportedRequest`] when no deployment of any
    /// candidate can represent the request; the plan is then empty.
    pub fn retain_representable(
        &mut self,
        ingress: IngressDialect,
        stream: bool,
        body: &Value,
        media: Option<&MediaRequest>,
    ) -> Result<(), UnsupportedRequest> {
        let operation = self.operation;
        let mut first = None;
        for candidate in &mut self.candidates {
            candidate.deployments.retain(|deployment| {
                match prepare(ingress, operation, stream, body, deployment, media) {
                    Ok(_) => true,
                    Err(error) => {
                        first.get_or_insert(error);
                        false
                    }
                }
            });
        }
        self.candidates
            .retain(|candidate| !candidate.deployments.is_empty());
        match first {
            Some(error) if self.candidates.is_empty() => Err(error),
            _ => Ok(()),
        }
    }
}

/// Orders `deployments` (sorted by name) for `call_id`.
///
/// The call id modulo the total weight picks the first deployment by
/// cumulative weight; the others follow in name order after it, wrapping.
/// Planning orders every capable deployment; the engine reapplies it to the
/// healthy subset. An empty slice yields an empty order.
pub(crate) fn weighted_order<'d>(
    deployments: &[&'d ProviderDeployment],
    call_id: GatewayCallId,
) -> Vec<&'d ProviderDeployment> {
    let total: u128 = deployments
        .iter()
        .map(|d| u128::from(d.routing_weight.get()))
        .sum();
    let Some(mut pick) = call_id.as_uuid().as_u128().checked_rem(total) else {
        return Vec::new();
    };
    let first = deployments
        .iter()
        .position(|d| {
            let weight = u128::from(d.routing_weight.get());
            if pick < weight {
                true
            } else {
                pick -= weight;
                false
            }
        })
        .unwrap_or(0);
    deployments[first..]
        .iter()
        .chain(&deployments[..first])
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wyrd_spec::gateway::{GatewayCapturePolicy, GatewayFallbackPolicy};

    /// Builds a deployment of `model` named `name` with `weight` and operations.
    fn deployment(name: &str, model: &str, weight: u32, ops: &[&str]) -> ProviderDeployment {
        serde_json::from_value(json!({
            "name": name,
            "model": ModelRef::from_projection(model).expect("model"),
            "adapter": {"openai_compatible": {"base_url": "https://acme.example/v1"}},
            "auth": "none",
            "capabilities": ops,
            "routing_weight": weight,
        }))
        .expect("deployment decodes")
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

    /// Parses a model projection.
    fn model(value: &str) -> ModelRef {
        ModelRef::from_projection(value).expect("model")
    }

    /// Candidate models of a plan.
    fn models(plan: &CallPlan) -> Vec<String> {
        plan.candidates
            .iter()
            .map(|c| format!("{}/{}", c.model.provider.as_str(), c.model.model.as_str()))
            .collect()
    }

    /// Override, exact model, operation, and global rules apply in that
    /// precedence without merging; the requested model and duplicates are
    /// skipped and incapable candidates are dropped.
    #[test]
    fn fallback_precedence_is_first_defined_without_merging() {
        let snap = snapshot(
            vec![
                deployment("dep-a", "acme/a", 1, &["chat_completions"]),
                deployment("dep-b", "acme/b", 1, &["chat_completions"]),
                deployment("dep-c", "acme/c", 1, &["chat_completions"]),
                deployment("dep-d", "acme/d", 1, &["chat_completions"]),
                deployment("dep-e", "acme/e", 1, &["embeddings"]),
            ],
            json!({"rules": [
                {"scope": "global", "candidates": [model("acme/a"), model("acme/d")]},
                {"scope": {"operation": {"operation": "chat_completions"}}, "candidates": [model("acme/c"), model("acme/e")]},
                {"scope": {"model": {"model": model("acme/a")}}, "candidates": [model("acme/b")]},
            ]}),
        );
        let call = GatewayCallId::new_v7();
        let chat = GatewayOperation::ChatCompletions;
        let plan = |requested: &str, fallback: Option<GatewayFallbackOverride>| {
            CallPlan::new(&snap, call, chat, model(requested), fallback.as_ref())
        };

        let over: GatewayFallbackOverride =
            serde_json::from_value(json!({"candidates": [model("acme/d"), model("acme/c")]}))
                .expect("override");
        assert_eq!(
            models(&plan("acme/a", Some(over))),
            ["acme/a", "acme/d", "acme/c"]
        );
        assert_eq!(models(&plan("acme/a", None)), ["acme/a", "acme/b"]);
        assert_eq!(
            models(&plan("acme/b", None)),
            ["acme/b", "acme/c"],
            "operation rule wins over global; incapable acme/e is dropped"
        );
        assert!(
            CallPlan::new(
                &snap,
                call,
                GatewayOperation::Embeddings,
                model("acme/d"),
                None
            )
            .candidates
            .is_empty(),
            "an incapable requested model and incapable global candidates are dropped"
        );
        let global = snapshot(
            vec![
                deployment("dep-a", "acme/a", 1, &["chat_completions"]),
                deployment("dep-d", "acme/d", 1, &["chat_completions"]),
            ],
            json!({"rules": [{"scope": "global", "candidates": [model("acme/a"), model("acme/d")]}]}),
        );
        assert_eq!(
            models(&CallPlan::new(&global, call, chat, model("acme/a"), None)),
            ["acme/a", "acme/d"],
            "global self candidate is skipped at runtime"
        );
    }

    /// Selection is a pure function of the call id and weights: repeated
    /// planning agrees, every deployment appears once, and first picks follow
    /// the weights.
    #[test]
    fn weighted_selection_is_deterministic_by_call_id() {
        let snap = snapshot(
            vec![
                deployment("heavy", "acme/a", 3, &["chat_completions"]),
                deployment("light", "acme/a", 1, &["chat_completions"]),
            ],
            json!({"rules": []}),
        );
        let mut heavy_first = 0;
        for low in 0..400_u128 {
            let call = GatewayCallId::from_uuid(uuid::Uuid::from_u128((7_u128 << 76) | low));
            let plan = CallPlan::new(
                &snap,
                call,
                GatewayOperation::ChatCompletions,
                model("acme/a"),
                None,
            );
            let again = CallPlan::new(
                &snap,
                call,
                GatewayOperation::ChatCompletions,
                model("acme/a"),
                None,
            );
            assert_eq!(plan, again);
            let names: Vec<&str> = plan.candidates[0]
                .deployments
                .iter()
                .map(|d| d.name.as_str())
                .collect();
            assert_eq!(names.len(), 2);
            if names[0] == "heavy" {
                heavy_first += 1;
            }
        }
        assert_eq!(heavy_first, 300, "weight 3:1 over a uniform residue range");
    }

    /// Pinning keeps only the named deployment of the requested model, dropping
    /// sibling deployments and fallback candidates, and empties the plan for a
    /// deployment that no longer serves it.
    ///
    /// # Panics
    ///
    /// Panics when a pinned plan keeps another deployment or model.
    #[test]
    fn pin_keeps_only_the_named_deployment_of_the_requested_model() {
        let snap = snapshot(
            vec![
                deployment("dep-a1", "acme/a", 1, &["batches"]),
                deployment("dep-a2", "acme/a", 1, &["batches"]),
                deployment("dep-b", "acme/b", 1, &["batches"]),
            ],
            json!({"rules": [{"scope": "global", "candidates": [model("acme/b")]}]}),
        );
        let plan = || {
            CallPlan::new(
                &snap,
                GatewayCallId::new_v7(),
                GatewayOperation::Batches,
                model("acme/a"),
                None,
            )
        };
        let name = |value: &str| ProviderDeploymentName::new(value).expect("name");
        let mut pinned = plan();
        pinned.pin(&name("dep-a2"));
        assert_eq!(models(&pinned), ["acme/a"]);
        assert_eq!(pinned.candidates[0].deployments.len(), 1);
        assert_eq!(pinned.candidates[0].deployments[0].name.as_str(), "dep-a2");
        let mut fallback = plan();
        fallback.pin(&name("dep-b"));
        assert!(
            fallback.candidates.is_empty(),
            "a fallback deployment is not the pin"
        );
    }
}
