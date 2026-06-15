//! Generate JSON schema goldens.

use std::fs;
use std::path::Path;

use schemars::schema_for;
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::artifact::{ArtifactSpec, FrameworkAdapterRef};
use wyrd_spec::card::audit::AuditSpec;
use wyrd_spec::card::data::{
    DataInterface, DataSchema, DataSpec, DataSplit, DataStats, SplitStrategy, SqlLogic,
};
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::eval::EvalSpec;
use wyrd_spec::card::experiment::ExperimentSpec;
use wyrd_spec::card::field::FieldSpec;
use wyrd_spec::card::mcp::McpSpec;
use wyrd_spec::card::model::{
    HuggingFaceTask, ModelInterface, ModelSignature, ModelSpec, SampleInput, SampleInputKind,
    TaskType, TfSaveFormat, TorchSaveFormat,
};
use wyrd_spec::card::operator::{OperatorBudget, OperatorInput, OperatorSpec};
use wyrd_spec::card::policy::{InvokeContext, InvokeOutcome, PolicyDecision, PolicySpec};
use wyrd_spec::card::prompt::{ParameterName, PromptRef, PromptSpec};
use wyrd_spec::card::service::{LockedComponent, ServiceLock};
use wyrd_spec::card::service::{
    ServiceRuntime, ServiceRuntimeKind, ServiceRuntimeMode, ServiceRuntimePolicy, ServiceSpec,
};
use wyrd_spec::card::source::{
    LogConnection, MetricsConnection, SourceAuth, SourceKind, SourceSpec, SqlConnection,
    TraceConnection,
};
use wyrd_spec::card::trigger::{TriggerSource, TriggerSpec};
use wyrd_spec::card::workflow::WorkflowSpec;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::reference::CardRef;
use wyrd_spec::run::{RunKind, RunRef};
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, DownloadInitResponse, DownloadPlan, PartUrlResponse,
    UploadCompleteRequest, UploadCompleteResponse, UploadInitRequest, UploadInitResponse,
    UploadPlan, VerificationGuarantee, WireProtocol,
};

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
    write::<FieldSpec>(out, golden, "field_spec")?;
    write::<DataSchema>(out, golden, "data_schema")?;
    write::<SplitStrategy>(out, golden, "split_strategy")?;
    write::<DataSplit>(out, golden, "data_split")?;
    write::<DataInterface>(out, golden, "data_interface")?;
    write::<SqlLogic>(out, golden, "sql_logic")?;
    write::<DataStats>(out, golden, "data_stats")?;
    write::<DataSpec>(out, golden, "data_spec")?;
    write::<ModelSpec>(out, golden, "model_spec")?;
    write::<ModelInterface>(out, golden, "model_interface")?;
    write::<TaskType>(out, golden, "task_type")?;
    write::<ModelSignature>(out, golden, "model_signature")?;
    write::<SampleInput>(out, golden, "sample_input")?;
    write::<SampleInputKind>(out, golden, "sample_input_kind")?;
    write::<TorchSaveFormat>(out, golden, "torch_save_format")?;
    write::<TfSaveFormat>(out, golden, "tf_save_format")?;
    write::<HuggingFaceTask>(out, golden, "hugging_face_task")?;
    write::<ExperimentSpec>(out, golden, "experiment_spec")?;
    write::<PromptSpec>(out, golden, "prompt_spec")?;
    write::<PromptRef>(out, golden, "prompt_ref")?;
    write::<ParameterName>(out, golden, "parameter_name")?;
    write::<AgentSpec>(out, golden, "agent_spec")?;
    write::<WorkflowSpec>(out, golden, "workflow_spec")?;
    write::<EvalSpec>(out, golden, "eval_spec")?;
    write::<DriftSpec>(out, golden, "drift_spec")?;
    write::<TriggerSpec>(out, golden, "trigger_spec")?;
    write::<TriggerSource>(out, golden, "trigger_source")?;
    write::<OperatorSpec>(out, golden, "operator_spec")?;
    write::<OperatorInput>(out, golden, "operator_input")?;
    write::<OperatorBudget>(out, golden, "operator_budget")?;
    write::<SourceSpec>(out, golden, "source_spec")?;
    write::<SourceKind>(out, golden, "source_kind")?;
    write::<SqlConnection>(out, golden, "sql_connection")?;
    write::<MetricsConnection>(out, golden, "metrics_connection")?;
    write::<LogConnection>(out, golden, "log_connection")?;
    write::<TraceConnection>(out, golden, "trace_connection")?;
    write::<SourceAuth>(out, golden, "source_auth")?;
    write::<ServiceSpec>(out, golden, "service_spec")?;
    write::<ServiceRuntime>(out, golden, "service_runtime")?;
    write::<ServiceRuntimeKind>(out, golden, "service_runtime_kind")?;
    write::<ServiceRuntimeMode>(out, golden, "service_runtime_mode")?;
    write::<ServiceRuntimePolicy>(out, golden, "service_runtime_policy")?;
    write::<PolicySpec>(out, golden, "policy_spec")?;
    write::<InvokeContext>(out, golden, "invoke_context")?;
    write::<InvokeOutcome>(out, golden, "invoke_outcome")?;
    write::<PolicyDecision>(out, golden, "policy_decision")?;
    write::<McpSpec>(out, golden, "mcp_spec")?;
    write::<AuditSpec>(out, golden, "audit_spec")?;
    write::<ArtifactSpec>(out, golden, "artifact_spec")?;
    write::<FrameworkAdapterRef>(out, golden, "framework_adapter_ref")?;
    write::<UploadInitRequest>(out, golden, "upload_init_request")?;
    write::<UploadInitResponse>(out, golden, "upload_init_response")?;
    write::<UploadPlan>(out, golden, "upload_plan")?;
    write::<UploadCompleteRequest>(out, golden, "upload_complete_request")?;
    write::<UploadCompleteResponse>(out, golden, "upload_complete_response")?;
    write::<PartUrlResponse>(out, golden, "part_url_response")?;
    write::<AbortResponse>(out, golden, "abort_response")?;
    write::<DownloadInitRequest>(out, golden, "download_init_request")?;
    write::<DownloadInitResponse>(out, golden, "download_init_response")?;
    write::<DownloadPlan>(out, golden, "download_plan")?;
    write::<WireProtocol>(out, golden, "wire_protocol")?;
    write::<VerificationGuarantee>(out, golden, "verification_guarantee")?;
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
