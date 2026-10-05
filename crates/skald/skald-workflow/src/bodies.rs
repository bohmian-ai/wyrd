//! Workflow hydration from already-fetched Agent and Prompt Card bodies.
//!
//! Loading environments — the client composing a local bundle or registered
//! graph, and the server validating a composite registration — each own their
//! IO and their body store. They hand Skald a synchronous lookup from an
//! authored reference to the exact body it names; this module turns that
//! lookup into Skald's Agent and Prompt resolvers so lowering, Prompt binding
//! checks, and resolved validation stay in one place. The lookup receives the
//! reference in its authored form, so an environment can serve a loader
//! `Sibling` only from local bodies and an external `Ref` only from its exact
//! registry read.

use chrono::Utc;
use skald_agent::{Agent, PromptResolver};
use skald_prompt::Prompt;
use skald_spec::Prompt as NativePrompt;
use skald_tool::{ToolRegistry, ToolResolver};
use wyrd_spec::card::workflow::{WorkflowAction, WorkflowCard};
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::metadata::{Annotations, Labels};
use wyrd_spec::reference::{CardRef, InlineableRef, Ref};
use wyrd_spec::refs::{ReferenceSlotVisitor, SlotValue};
use wyrd_spec::{AgentCard, AgentSpec};

use crate::workflow_surface::{AgentResolver, Workflow};

/// Synchronous lookup from an authored Agent or Prompt reference to the exact
/// Card body the loading environment fetched for it.
///
/// The reference keeps its authored provenance (`Ref` or `Sibling`); return
/// `None` when the environment holds no body for that provenance and identity.
pub type CardBodies<'a> = dyn Fn(&Ref) -> Option<Spec> + Sync + 'a;

/// Return the Agent and Prompt references `spec` names, in authored form.
///
/// Walks the canonical [`ReferenceSlotVisitor`] and keeps every Agent and
/// Prompt slot that names a Card — external `Ref` or loader `Sibling` — so a
/// loading environment can close the bodies a Workflow needs: a Workflow
/// names its step Agents and inline Agents' Prompts, an Agent names its
/// Prompt, and a Prompt names nothing. Inline bodies and unresolved paths are
/// skipped; hydration refuses an unresolved path itself.
#[must_use]
pub fn card_body_dependencies(spec: &Spec) -> Vec<Ref> {
    let mut spec = spec.clone();
    let mut dependencies = Vec::new();
    ReferenceSlotVisitor::visit(&mut spec, |slot| {
        let dependency = match slot.value {
            SlotValue::InlineableAgent(agent) => agent.to_durable(),
            SlotValue::InlineablePrompt(prompt) => prompt.to_durable(),
            _ => None,
        };
        dependencies.extend(dependency);
    });
    dependencies
}

impl Workflow {
    /// Hydrate a runnable Workflow whose referenced Agents and Prompts come
    /// from `bodies`, binding each Agent's declared tool names from `tools`.
    ///
    /// Runs the pure Workflow contract, lowers every step through Skald's
    /// existing Agent and Prompt construction, and then runs resolved
    /// validation (Prompt-variable bindings, payload kinds, output schemas,
    /// and route dialect). Performs no IO.
    ///
    /// # Errors
    /// Returns the pure Workflow contract error;
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when `bodies` holds no body
    /// for a referenced Agent or Prompt; `WYRD_REGISTRY_400_INVALID_CARD_SPEC`
    /// for an unresolved path or a body of the wrong kind; the Agent tool and
    /// Prompt resolution errors; and the resolved validation errors of
    /// [`Workflow::validate`].
    pub fn from_card_bodies(
        card: WorkflowCard,
        tools: &dyn ToolResolver,
        bodies: &CardBodies<'_>,
    ) -> Result<Self, WyrdError> {
        let resolver = CardBodyResolver {
            bodies,
            tools,
            bind_tools: true,
        };
        let workflow =
            Self::from_card_with_agent_resolver(card, tools, &resolver, Some(&resolver))?;
        workflow.validate()?;
        Ok(workflow)
    }

    /// Run declarative validation of a Workflow Card and the bodies it names
    /// without binding tools.
    ///
    /// Declared tool names are execution-environment suitability, not Card
    /// validity, so every Agent's tool names are left unbound; every other
    /// pure and resolved check runs exactly as in [`Self::from_card_bodies`].
    /// Registration uses this so it never binds tools, resolves secrets, or
    /// executes.
    ///
    /// # Errors
    /// Returns the errors of [`Self::from_card_bodies`] other than tool
    /// resolution.
    pub fn validate_card_bodies(
        mut card: WorkflowCard,
        bodies: &CardBodies<'_>,
    ) -> Result<(), WyrdError> {
        for step in &mut card.spec.steps {
            let WorkflowAction::Agent(agent) = &mut step.action;
            if let InlineableRef::Inline(agent) = agent {
                agent.tool_names.clear();
            }
        }
        let tools = ToolRegistry::new();
        let resolver = CardBodyResolver {
            bodies,
            tools: &tools,
            bind_tools: false,
        };
        Self::from_card_with_agent_resolver(card, &tools, &resolver, Some(&resolver))?
            .validate()
            .map_err(Into::into)
    }
}

/// Serves an environment's fetched bodies through Skald's Agent and Prompt
/// resolver seams for one hydration or validation pass.
struct CardBodyResolver<'a> {
    /// Environment-owned lookup from authored reference to exact body.
    bodies: &'a CardBodies<'a>,
    /// Tools bound to referenced Agents' declared names.
    tools: &'a dyn ToolResolver,
    /// Whether referenced Agents bind their declared tool names.
    bind_tools: bool,
}

impl CardBodyResolver<'_> {
    /// Return the body `bodies` holds for an identity-bearing reference slot,
    /// with the exact reference it names.
    ///
    /// # Errors
    /// Returns `WYRD_REGISTRY_400_INVALID_CARD_SPEC` for an inline body or an
    /// unresolved path, and `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when no
    /// body is held for the slot's provenance and identity.
    fn body<T>(&self, slot: &InlineableRef<T>) -> Result<(CardRef, Spec), WyrdError> {
        let Some(dependency) = slot.to_durable() else {
            return Err(WyrdError::registry_invalid_card_spec(
                "card dependency is not a card reference",
            ));
        };
        let Some(card_ref) = dependency.as_card_ref().cloned() else {
            return Err(WyrdError::registry_invalid_card_spec(
                "card dependency is an unresolved path",
            ));
        };
        match (self.bodies)(&dependency) {
            Some(spec) if spec.kind() == card_ref.kind => Ok((card_ref, spec)),
            Some(spec) => Err(WyrdError::registry_invalid_card_spec(format!(
                "card dependency {card_ref} resolved to a {} body",
                spec.kind().wire_name()
            ))),
            None => Err(WyrdError::RegistryUnresolvedDependency {
                message: format!("card dependency {card_ref} was not loaded"),
                details: serde_json::json!({ "card_ref": card_ref.to_string() }),
            }),
        }
    }
}

impl AgentResolver for CardBodyResolver<'_> {
    /// Build the runtime Agent for a referenced step from the body held under
    /// the step's authored provenance, keeping its exact identity and UID.
    ///
    /// # Errors
    /// Returns the errors of [`CardBodyResolver::body`] and the Agent tool and
    /// Prompt resolution errors of `Agent::from_card`.
    fn resolve(&self, agent_ref: &InlineableRef<AgentSpec>) -> Result<Agent, WyrdError> {
        let (card_ref, Spec::Agent(mut spec)) = self.body(agent_ref)? else {
            return Err(WyrdError::registry_invalid_card_spec(
                "workflow step Agent did not resolve to an Agent body",
            ));
        };
        if !self.bind_tools {
            spec.tool_names.clear();
        }
        let card = AgentCard {
            space: card_ref
                .space
                .as_ref()
                .map_or_else(|| "default".to_owned(), ToString::to_string),
            name: card_ref.name.to_string(),
            version: card_ref.version.to_string(),
            uid: card_ref
                .uid
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default(),
            labels: Labels::default(),
            annotations: Annotations::default(),
            spec,
            cascade_children: Vec::new(),
            created_at: Utc::now(),
        };
        Agent::from_card(card, self.tools, self)
    }
}

impl PromptResolver for CardBodyResolver<'_> {
    /// Return an inline native Prompt or the Prompt Card body held under the
    /// slot's authored provenance.
    ///
    /// # Errors
    /// Returns the errors of [`CardBodyResolver::body`].
    fn resolve(&self, prompt_ref: &InlineableRef<NativePrompt>) -> Result<Prompt, WyrdError> {
        if let InlineableRef::Inline(prompt) = prompt_ref {
            return Ok(Prompt::from_native((**prompt).clone()));
        }
        match self.body(prompt_ref)? {
            (_, Spec::Prompt(prompt)) => Ok(Prompt::from_native(prompt.prompt)),
            (card_ref, _) => Err(WyrdError::registry_invalid_card_spec(format!(
                "Agent prompt {card_ref} did not resolve to a Prompt body"
            ))),
        }
    }
}
