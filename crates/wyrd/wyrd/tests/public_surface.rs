//! Pin the sanctioned C12 `wyrd::agent` Rust surface.

#[test]
fn use_wyrd_agent_module_imports_compile() {
    use wyrd::agent::Agent;
    use wyrd::agent::{AgentBuilder, AgentTool, CallbackOutcome, ToolDef};
    use wyrd::agent::{AgentDelegateTool, Journal, NoSession, NoopJournal, SessionMemory};

    let _: Option<&Agent> = None;
    let _: Option<&AgentBuilder> = None;
    let _: Option<&dyn AgentTool> = None;
    let _: Option<&ToolDef> = None;
    let _: Option<&CallbackOutcome<String>> = None;
    let _: Option<&AgentDelegateTool> = None;
    let _: Option<&dyn Journal> = None;
    let _: Option<&NoSession> = None;
    let _: Option<&NoopJournal> = None;
    let _: Option<&dyn SessionMemory> = None;
}

#[test]
fn use_wyrd_agent_observer_imports_compile() {
    use wyrd::agent::{NoopObserver, Observer};

    let _: Option<&NoopObserver> = None;
    fn _takes_observer(_observer: std::sync::Arc<dyn Observer>) {}
}

#[test]
fn use_wyrd_agent_runconfig_imports_compile() {
    use wyrd::agent::{AgentContext, AgentRun, FinishReason, RunConfig};

    let _: Option<&RunConfig> = None;
    let _: Option<&AgentRun> = None;
    let _: Option<&AgentContext> = None;
    let _: Option<&FinishReason> = None;
}
