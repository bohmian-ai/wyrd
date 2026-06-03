//! Pin the sanctioned `wyrd::agent` and `wyrd::workflow` Rust surface.
//!
//! This is a compile-test fixture: every line below must compile. If a
//! `skald-agent` or `skald-workflow` rename breaks one of these paths, this test
//! fails and the re-export must be updated.

#[allow(unused_imports)]
mod _agent {
    use wyrd::agent::{Agent, AgentError, AgentResult, AgentRun, FinishReason, RunConfig};
}

#[allow(unused_imports)]
mod _workflow {
    use wyrd::workflow::{
        Context, ContextSnapshot, Task, TaskDef, TaskEvent, TaskOutcome, TaskStatus, Workflow,
        WorkflowDef, WorkflowError, WorkflowResult, WorkflowRun, default_max_retries,
    };
}

#[test]
fn surface_is_callable() {
    let _ = wyrd::agent::RunConfig::default();
    let _ = wyrd::agent::Agent::run_prompt;
    let _ = wyrd::workflow::default_max_retries();
}
