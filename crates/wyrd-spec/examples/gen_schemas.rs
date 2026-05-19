//! Generate JSON schema goldens.

use std::fs;
use std::path::Path;

use schemars::schema_for;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::card::audit::AuditSpec;
use wyrd_spec::card::data::DataSpec;
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::card::experiment::ExperimentSpec;
use wyrd_spec::card::mcp::McpSpec;
use wyrd_spec::card::model::ModelSpec;
use wyrd_spec::card::policy::PolicySpec;
use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::card::service::ServiceSpec;
use wyrd_spec::card::service::{LockedComponent, ServiceLock};
use wyrd_spec::card::skill::SkillSpec;
use wyrd_spec::card::subagent::SubAgentSpec;
use wyrd_spec::card::tool::ToolSpec;
use wyrd_spec::card::workflow::WorkflowSpec;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::reference::CardRef;
use wyrd_spec::run::{RunKind, RunRef};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = Path::new("crates/wyrd-spec/schemas");
    let golden = Path::new("crates/wyrd-spec/tests/schemas");
    fs::create_dir_all(out)?;
    fs::create_dir_all(golden)?;

    write::<Card>(out, golden, "card")?;
    write::<CardKind>(out, golden, "card_kind")?;
    write::<CardRef>(out, golden, "card_ref")?;
    write::<RunKind>(out, golden, "run_kind")?;
    write::<RunRef>(out, golden, "run_ref")?;
    write::<ServiceLock>(out, golden, "service_lock")?;
    write::<LockedComponent>(out, golden, "locked_component")?;
    write::<DataSpec>(out, golden, "data_spec")?;
    write::<ModelSpec>(out, golden, "model_spec")?;
    write::<ExperimentSpec>(out, golden, "experiment_spec")?;
    write::<PromptSpec>(out, golden, "prompt_spec")?;
    write::<ToolSpec>(out, golden, "tool_spec")?;
    write::<AgentSpec>(out, golden, "agent_spec")?;
    write::<WorkflowSpec>(out, golden, "workflow_spec")?;
    write::<EvalSpec>(out, golden, "eval_spec")?;
    write::<DriftSpec>(out, golden, "drift_spec")?;
    write::<ServiceSpec>(out, golden, "service_spec")?;
    write::<PolicySpec>(out, golden, "policy_spec")?;
    write::<McpSpec>(out, golden, "mcp_spec")?;
    write::<SkillSpec>(out, golden, "skill_spec")?;
    write::<SubAgentSpec>(out, golden, "subagent_spec")?;
    write::<AuditSpec>(out, golden, "audit_spec")?;
    write::<ArtifactSpec>(out, golden, "artifact_spec")?;
    Ok(())
}

fn write<T: schemars::JsonSchema>(
    out: &Path,
    golden: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let json = serde_json::to_string_pretty(&schema)?;
    fs::write(out.join(format!("{name}.json")), format!("{json}\n"))?;
    fs::write(golden.join(format!("{name}.json")), format!("{json}\n"))?;
    Ok(())
}
