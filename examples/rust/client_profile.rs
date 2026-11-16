//! Fixed client-only workloads for macOS Instruments Allocations recordings.
//!
//! Each workload exercises one operation family offline and checks its
//! result; no server, network, or profiling hook is involved, so a
//! recording sees only the client's own allocations:
//!
//! - `authoring chain8|chain32|shared`: capture and validate a Service chain
//!   of 8 or 32 Cards, or 32 parents sharing one child, each rooted at a
//!   64 KiB Prompt.
//! - `hydration BUNDLE`: hydrate the prepared state bundle at `BUNDLE`.
//! - `access typed|agent|run BUNDLE`: hydrate once, then repeat 1,000 typed
//!   lookups, Agent/Prompt navigations, or run/Card-view openings.
//!
//! Run with:
//!     cargo run -p wyrd-rust-examples --bin client_profile -- authoring chain8

use anyhow::{Context, bail};
use wyrd_ai::serde_json::json;
use wyrd_ai::state::{HydratedCard, WyrdState};
use wyrd_ai::verify::{Agent, CardDefinition, Component, Service};

/// The repeated-operation count of the `access` workload.
const ACCESS_ITERATIONS: usize = 1_000;

/// The Agent every authored graph roots at: an inline Prompt whose one user
/// message carries 64 KiB of text.
fn agent() -> Agent {
    let payload = "x".repeat(64 * 1024);
    Agent::new("profile-agent", "1.0.0")
        .prompt(json!({
            "provider": "openai",
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": format!("{payload} {{{{report}}}}")}],
        }))
        .execution_route(json!({"kind": "wyrd_gateway", "deployment": "openai"}))
}

/// A chain of `length` Services, each the sole component of the next, over
/// the shared Agent.
fn chain(length: usize) -> Service {
    let mut service =
        Service::new("chain-0", "1.0.0").components([Component::new("agent", agent())]);
    for index in 1..length {
        service = Service::new(format!("chain-{index}"), "1.0.0")
            .components([Component::new("inner", service)]);
    }
    service
}

/// A Service of 32 parent Services that each hold the same child Service.
fn shared_child() -> Service {
    let child = Service::new("child", "1.0.0").components([Component::new("agent", agent())]);
    Service::new("parents", "1.0.0").components((0..32).map(|index| {
        let parent = Service::new(format!("parent-{index}"), "1.0.0")
            .components([Component::new("child", child.clone())]);
        Component::new(format!("parent-{index}"), parent)
    }))
}

/// Capture and validate one authored graph, checking its submission count.
fn authoring(root: Service, expected: usize) -> anyhow::Result<()> {
    let declaration = CardDefinition::from(root).declaration()?;
    let submissions = declaration.input.context("an ordinary graph")?.submissions;
    if submissions.len() != expected {
        bail!(
            "expected {expected} submissions, found {}",
            submissions.len()
        );
    }
    Ok(())
}

/// Repeat typed lookups.
fn typed(state: &WyrdState) -> anyhow::Result<()> {
    for _ in 0..ACCESS_ITERATIONS {
        state.prompt("triage_prompt")?;
        state.model("model")?;
        state.verifier("ok_check")?;
        if !matches!(state.get("triage_prompt")?, HydratedCard::Prompt(_)) {
            bail!("triage_prompt is a Prompt");
        }
    }
    Ok(())
}

/// Repeat Agent-to-Prompt navigation.
fn agent_navigation(state: &WyrdState) -> anyhow::Result<()> {
    let prompt = state.prompt("triage_prompt")?;
    for _ in 0..ACCESS_ITERATIONS {
        if !std::ptr::eq(state.agent("agent_triage")?.prompt(), prompt) {
            bail!("the Agent's Prompt is the aliased Prompt");
        }
        if !matches!(state.get("agent_triage")?, HydratedCard::Agent(_)) {
            bail!("agent_triage is an Agent");
        }
    }
    Ok(())
}

/// Repeat run and Card-view openings.
fn runs(state: &WyrdState) -> anyhow::Result<()> {
    for _ in 0..ACCESS_ITERATIONS {
        let run = state.run_for_card("agent_triage")?;
        let view = run.for_card("triage_prompt")?;
        if view.run_id() != run.run_id() {
            bail!("a view shares its run");
        }
    }
    Ok(())
}

/// Run the workload the arguments name.
fn main() -> anyhow::Result<()> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    match arguments
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["authoring", "chain8"] => authoring(chain(8), 9),
        ["authoring", "chain32"] => authoring(chain(32), 33),
        ["authoring", "shared"] => authoring(shared_child(), 35),
        ["hydration", bundle] => WyrdState::from_path(bundle).map(drop).map_err(Into::into),
        ["access", "typed", bundle] => typed(&WyrdState::from_path(bundle)?),
        ["access", "agent", bundle] => agent_navigation(&WyrdState::from_path(bundle)?),
        ["access", "run", bundle] => runs(&WyrdState::from_path(bundle)?),
        _ => bail!(
            "usage: client_profile authoring chain8|chain32|shared | hydration BUNDLE \
             | access typed|agent|run BUNDLE"
        ),
    }
}
