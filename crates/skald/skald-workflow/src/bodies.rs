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

/// Lookup from the exact Agent Card reference a step executes to the tools
/// that Agent's declared names bind to.
///
/// A referenced Agent is looked up by its exact authored reference; an inline
/// Agent has no reference of its own and is looked up by its Workflow's
/// reference. The resolver is used only while hydrating that Agent.
pub type AgentTools<'a> = dyn Fn(&CardRef) -> Box<dyn ToolResolver + 'a> + Sync + 'a;

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
    /// from `bodies`, binding each Agent's declared tool names from the
    /// resolver `tools` returns for that Agent.
    ///
    /// `tools` receives a referenced Agent's exact reference and, for an
    /// inline Agent, the Workflow's own reference, so an environment can bind
    /// tools to the executing Agent's identity.
    ///
    /// Runs the pure Workflow contract, lowers every step through Skald's
    /// existing Agent and Prompt construction, and then runs resolved
    /// validation (Prompt-variable bindings, payload kinds, output schemas,
    /// and route dialect). Performs no IO.
    ///
    /// # Errors
    /// Returns the pure Workflow contract error; the Workflow identity errors
    /// of [`WorkflowCard::card_ref`];
    /// `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` when `bodies` holds no body
    /// for a referenced Agent or Prompt; `WYRD_REGISTRY_400_INVALID_CARD_SPEC`
    /// for an unresolved path or a body of the wrong kind; the Agent tool and
    /// Prompt resolution errors; and the resolved validation errors of
    /// [`Workflow::validate`].
    pub fn from_card_bodies(
        card: WorkflowCard,
        tools: &AgentTools<'_>,
        bodies: &CardBodies<'_>,
    ) -> Result<Self, WyrdError> {
        let inline_tools = tools(&card.card_ref()?);
        let resolver = CardBodyResolver {
            bodies,
            tools,
            bind_tools: true,
        };
        let workflow = Self::from_card_with_agent_resolver(
            card,
            inline_tools.as_ref(),
            &resolver,
            Some(&resolver),
        )?;
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
            tools: &|_| Box::new(ToolRegistry::new()),
            bind_tools: false,
        };
        Self::from_card_with_agent_resolver(card, &tools, &resolver, Some(&resolver))?
            .validate()
            .map_err(Into::into)
    }
}

/// Serves an environment's fetched bodies through Skald's Agent and Prompt
/// resolver seams for one hydration or validation pass.
struct CardBodyResolver<'a, 't> {
    /// Environment-owned lookup from authored reference to exact body.
    bodies: &'a CardBodies<'a>,
    /// Per-Agent tools bound to referenced Agents' declared names.
    tools: &'a AgentTools<'t>,
    /// Whether referenced Agents bind their declared tool names.
    bind_tools: bool,
}

impl CardBodyResolver<'_, '_> {
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

impl AgentResolver for CardBodyResolver<'_, '_> {
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
        let tools = (self.tools)(&card_ref);
        Agent::from_card(card, tools.as_ref(), self)
    }
}

impl PromptResolver for CardBodyResolver<'_, '_> {
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use skald_agent::Agent;
    use skald_tool::{AgentTool, ToolRegistry};
    use wyrd_spec::envelope::Spec;
    use wyrd_spec::reference::Ref;

    use crate::test_support::{RecordingTool, agent, bindings, lock};
    use crate::workflow_surface::Workflow;

    /// Recording tool named `name`.
    fn tool(name: &str) -> Arc<dyn AgentTool> {
        Arc::new(RecordingTool {
            name: name.to_owned(),
            calls: AtomicUsize::new(0),
        })
    }

    /// Registered fixture Agent `name` in space `team` declaring `tool_name`.
    fn registered(name: &str, tool_name: &str) -> Agent {
        agent(name, &format!("{name} static"), None)
            .version("1.0.0")
            .space("team")
            .with_tool(tool(tool_name))
    }

    /// Each Agent binds its declared tools from the resolver returned for its
    /// own exact reference, and an inline Agent from the Workflow's reference;
    /// an Agent handed another Agent's resolver could not bind its tool.
    #[test]
    fn agent_tools_follow_each_agent_reference() {
        let alpha = registered("alpha", "alpha_tool");
        let beta = registered("beta", "beta_tool");
        let bodies: Vec<_> = [&alpha, &beta]
            .into_iter()
            .map(|agent| {
                let card_ref = agent
                    .card_ref()
                    .expect("fixture identity is valid")
                    .expect("fixture Agent is referenced");
                (card_ref, Spec::Agent(agent.to_spec()))
            })
            .collect();
        let inline = agent("gamma", "gamma static", None).with_tool(tool("gamma_tool"));
        let card = Workflow::builder("flow")
            .version("1.0.0")
            .space("team")
            .add(alpha)
            .and_then(|b| b.add(beta))
            .and_then(|b| b.add(inline))
            .and_then(|b| b.with_outputs(bindings(&[("text", "steps.gamma.output.text")])))
            .and_then(super::super::workflow_surface::WorkflowBuilder::build)
            .and_then(|workflow| Ok(workflow.to_card()?))
            .expect("fixture Workflow builds");
        let tool_for: BTreeMap<&str, &str> = [
            ("alpha", "alpha_tool"),
            ("beta", "beta_tool"),
            ("flow", "gamma_tool"),
        ]
        .into();
        let asked = Mutex::new(Vec::new());

        let workflow = Workflow::from_card_bodies(
            card,
            &|card_ref| {
                lock(&asked).push(card_ref.clone());
                let registry = ToolRegistry::new();
                if let Some(name) = tool_for.get(card_ref.name.as_str()) {
                    registry
                        .register(tool(name))
                        .expect("fixture tool name is unique");
                }
                Box::new(registry)
            },
            &|dependency| match dependency {
                Ref::Ref(card_ref) => bodies
                    .iter()
                    .find(|(held, _)| held.same_identity(card_ref))
                    .map(|(_, spec)| spec.clone()),
                _ => None,
            },
        )
        .expect("every Agent binds its own tools");

        for (step, expected) in [
            ("alpha", "alpha_tool"),
            ("beta", "beta_tool"),
            ("gamma", "gamma_tool"),
        ] {
            let names: Vec<&str> = workflow.resolved_agents[step]
                .tools()
                .iter()
                .map(|tool| tool.name())
                .collect();
            assert_eq!(names, [expected], "step {step} binds its own tool");
        }
        let mut asked: Vec<String> = lock(&asked)
            .iter()
            .map(|card_ref| {
                format!(
                    "{}/{}@{}",
                    card_ref.kind.wire_name(),
                    card_ref.name,
                    card_ref.version
                )
            })
            .collect();
        asked.sort();
        assert_eq!(
            asked,
            [
                "Agent/alpha@1.0.0",
                "Agent/beta@1.0.0",
                "Workflow/flow@1.0.0"
            ],
            "tools are asked for by each Agent's reference and the Workflow's"
        );
    }
}
