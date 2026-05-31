use std::sync::{Arc, Mutex};

use serde_json::value::RawValue;
use skald_agent::{FinishReason, Observer};
use vala_client::{
    AgentErrorRecord, AgentFinishRecord, AgentStartRecord, IterationRecord, ToolCallRecord,
    ToolResultRecord, ValaClient, ValaClientResult,
};
use wyrd_observe::{RedactionPolicy, RunId, RunIdSource, SamplingPolicy, WyrdObserver};

#[derive(Default)]
struct RecordingValaClient {
    starts: Mutex<Vec<AgentStartRecord>>,
    iterations: Mutex<Vec<IterationRecord>>,
    tool_calls: Mutex<Vec<ToolCallRecord>>,
    tool_results: Mutex<Vec<ToolResultRecord>>,
    finishes: Mutex<Vec<AgentFinishRecord>>,
    errors: Mutex<Vec<AgentErrorRecord>>,
}

impl ValaClient for RecordingValaClient {
    fn observe_agent_start(&self, record: AgentStartRecord) -> ValaClientResult<()> {
        self.starts
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }

    fn observe_iteration(&self, record: IterationRecord) -> ValaClientResult<()> {
        self.iterations
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }

    fn observe_tool_call(&self, record: ToolCallRecord) -> ValaClientResult<()> {
        self.tool_calls
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }

    fn observe_tool_result(&self, record: ToolResultRecord) -> ValaClientResult<()> {
        self.tool_results
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }

    fn observe_agent_finish(&self, record: AgentFinishRecord) -> ValaClientResult<()> {
        self.finishes
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }

    fn observe_agent_error(&self, record: AgentErrorRecord) -> ValaClientResult<()> {
        self.errors
            .lock()
            .expect("recording client lock should not be poisoned")
            .push(record);
        Ok(())
    }
}

#[test]
fn terminal_events_always_emit_even_when_sampling_drops_iterations() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::FromRequest(RunId::from_string("r1".to_owned())),
        vala.clone(),
        SamplingPolicy::bucket(100, 0).expect("100 is non-zero"),
        RedactionPolicy::strict(),
    );

    observer.on_agent_start("a", 10);
    observer.on_iteration("a", 1);
    observer.on_iteration("a", 2);
    observer.on_agent_finish("a", FinishReason::Stop, 2);

    assert_eq!(
        vala.starts
            .lock()
            .expect("recording client lock should not be poisoned")
            .len(),
        1
    );
    assert_eq!(
        vala.finishes
            .lock()
            .expect("recording client lock should not be poisoned")
            .len(),
        1
    );
    assert_eq!(
        vala.iterations
            .lock()
            .expect("recording client lock should not be poisoned")
            .len(),
        0
    );
}

#[test]
fn critical_tool_force_samples_even_under_aggressive_sampling() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::NewSession,
        vala.clone(),
        SamplingPolicy::bucket(100, 0).expect("100 is non-zero"),
        RedactionPolicy::strict(),
    );
    let args = RawValue::from_string(r#"{"x":1}"#.to_owned()).expect("test JSON must be valid");

    observer.on_tool_call("a", "database_write", &args);

    assert_eq!(
        vala.tool_calls
            .lock()
            .expect("recording client lock should not be poisoned")
            .len(),
        1
    );
}

#[test]
fn on_tool_call_emits_redacted_args() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::FromRequest(RunId::from_string("r1".to_owned())),
        vala.clone(),
        SamplingPolicy::All,
        RedactionPolicy::strict(),
    );
    let args = RawValue::from_string(r#"{"username":"alice","password":"hunter2"}"#.to_owned())
        .expect("test JSON must be valid");

    observer.on_tool_call("a", "login", &args);

    let calls = vala
        .tool_calls
        .lock()
        .expect("recording client lock should not be poisoned");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].redacted_args["username"], "alice");
    assert_eq!(calls[0].redacted_args["password"], "<REDACTED>");
}

#[test]
fn on_tool_result_emits_low_volume_result() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::NewSession,
        vala.clone(),
        SamplingPolicy::bucket(100, 0).expect("100 is non-zero"),
        RedactionPolicy::strict(),
    );

    observer.on_tool_result("a", "lookup", false);

    let results = vala
        .tool_results
        .lock()
        .expect("recording client lock should not be poisoned");
    assert_eq!(results.len(), 1);
    assert!(!results[0].ok);
}

#[test]
fn on_agent_error_emits_record_with_mapped_code() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::FromRequest(RunId::from_string("r1".to_owned())),
        vala.clone(),
        SamplingPolicy::All,
        RedactionPolicy::strict(),
    );

    observer.on_agent_error(
        "a",
        "SKALD_AGENT_500_MAX_ITERATIONS",
        "agent exceeded max iterations",
    );

    let errors = vala
        .errors
        .lock()
        .expect("recording client lock should not be poisoned");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "SKALD_AGENT_500_MAX_ITERATIONS");
    assert_eq!(
        errors[0].wyrd_error_code.as_deref(),
        Some("WYRD_AGENT_500_MAX_ITERATIONS")
    );
}

#[test]
fn on_agent_error_for_unmapped_code_carries_none_variant() {
    let vala = Arc::new(RecordingValaClient::default());
    let observer = WyrdObserver::new(
        RunIdSource::NewSession,
        vala.clone(),
        SamplingPolicy::All,
        RedactionPolicy::strict(),
    );

    observer.on_agent_error("a", "SKALD_AGENT_999_FUTURE", "future failure");

    let errors = vala
        .errors
        .lock()
        .expect("recording client lock should not be poisoned");
    assert_eq!(errors[0].code, "SKALD_AGENT_999_FUTURE");
    assert!(errors[0].wyrd_error_code.is_none());
}
