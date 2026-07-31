//! Generate JSON schema goldens.

use std::fs;
use std::path::Path;

use schemars::schema_for;
use wyrd_spec::auth::{
    AbsoluteUrl, CallbackQuery, IssuerUrl, LoginInitResponse, PrincipalKindTag,
    RevokePrincipalRequest, RevokePrincipalResponse, TokenRequest, TokenResponse,
};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::card::artifact::{ArtifactSpec, FrameworkAdapterRef};
use wyrd_spec::card::audit::AuditSpec;
use wyrd_spec::card::data::{
    DataInterface, DataSchema, DataSpec, DataSplit, DataStats, SplitStrategy, SqlLogic,
};
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::eval::EvalSpec as CardEvalSpec;
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
use wyrd_spec::registry::{
    ArtifactInventoryResponse, ArtifactManifestEntry, CardLifecycleStatus, CardLocator,
    CardSubmission, CardSummary, CreateCardRequest, CreateCardResponse, DeleteCardResponse,
    GetCardResponse, ListCardsRequest, ListCardsResponse, ListVersionsResponse,
    RegisterOutcome as CardRegisterOutcome, RegistrationOperationId, RegistrationReceipt,
    RelativeArtifactPath, StoredArtifactEntry, SubmissionMetadata,
};
use wyrd_spec::run::{RunKind, RunRef};
use wyrd_spec::security::{SecretRef, TlsConfig};
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, DownloadInitResponse, DownloadPlan,
    LocalBlobUploadResponse, PartUrlResponse, UploadCompleteRequest, UploadCompleteResponse,
    UploadInitRequest, UploadInitResponse, UploadPlan, VerificationGuarantee, WireProtocol,
};
use wyrd_spec::vala::api::{
    AuditDecision, AuditEvent, AuditResult, AuthMethod, BifrostErrorDescriptor,
    BifrostPermissionDescriptor, BifrostQueryRequest, BifrostTableDescription, BifrostTableEntry,
    DataTypeSpec, FieldSpec as BifrostFieldSpec, PartitionColumnSpec, PartitionTransformWire,
    QueryParam, RegisterOutcome, RegisterTableRequest, RegisterTableResponse, SyncQueryRequest,
    TableStatus, TimeUnit,
};
use wyrd_spec::vala::eval::{
    AgentTurnSubmission, ComparisonOperator, ConversationTurn, DagError, EvalCondition,
    EvalPassGate, EvalRecordObservation, EvalRunOpenRequest, EvalRunOpenResponse, EvalSampling,
    EvalScenarioCollection, EvalSpec, EvalTask, ExecutionPlan, SimulatedUserMode,
    SimulatedUserTurn, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::observation::{ObservationEnvelope, ObservationKind, RecordObservation};
use wyrd_spec::vala::trace::{
    AttributeValue, GenAiEvalResult, GenAiSpanRecord, InstrumentationScope, Resource, SpanEvent,
    SpanKind, SpanLink, SpanRecord, SpanStatus, TraceSummaryRecord,
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
    write::<CardEvalSpec>(out, golden, "eval_spec")?;
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
    write::<LocalBlobUploadResponse>(out, golden, "local_blob_upload_response")?;
    write::<DownloadInitRequest>(out, golden, "download_init_request")?;
    write::<DownloadInitResponse>(out, golden, "download_init_response")?;
    write::<DownloadPlan>(out, golden, "download_plan")?;
    write::<WireProtocol>(out, golden, "wire_protocol")?;
    write::<VerificationGuarantee>(out, golden, "verification_guarantee")?;

    // Card registration wire contracts (task 01).
    write::<CardSubmission>(out, golden, "card_submission")?;
    write::<SubmissionMetadata>(out, golden, "submission_metadata")?;
    write::<ArtifactManifestEntry>(out, golden, "artifact_manifest_entry")?;
    write::<CreateCardRequest>(out, golden, "create_card_request")?;
    write::<CreateCardResponse>(out, golden, "create_card_response")?;
    write::<RegistrationReceipt>(out, golden, "registration_receipt")?;
    write::<RegistrationOperationId>(out, golden, "registration_operation_id")?;
    write::<RelativeArtifactPath>(out, golden, "relative_artifact_path")?;
    write::<CardLifecycleStatus>(out, golden, "card_lifecycle_status")?;
    write::<CardRegisterOutcome>(out, golden, "card_register_outcome")?;
    write::<GetCardResponse>(out, golden, "get_card_response")?;
    write::<DeleteCardResponse>(out, golden, "delete_card_response")?;
    write::<CardSummary>(out, golden, "card_summary")?;
    write::<ListCardsRequest>(out, golden, "list_cards_request")?;
    write::<ListCardsResponse>(out, golden, "list_cards_response")?;
    write::<ListVersionsResponse>(out, golden, "list_versions_response")?;
    write::<CardLocator>(out, golden, "card_locator")?;
    write::<ArtifactInventoryResponse>(out, golden, "artifact_inventory_response")?;
    write::<StoredArtifactEntry>(out, golden, "stored_artifact_entry")?;

    // Auth contracts.
    write::<TokenRequest>(out, golden, "auth_token_request")?;
    write::<TokenResponse>(out, golden, "auth_token_response")?;
    write::<AbsoluteUrl>(out, golden, "auth_url")?;
    write::<IssuerUrl>(out, golden, "auth_issuer_url")?;
    write::<LoginInitResponse>(out, golden, "auth_login_init_response")?;
    write::<CallbackQuery>(out, golden, "auth_callback_query")?;
    write::<PrincipalKindTag>(out, golden, "auth_principal_kind")?;
    write::<RevokePrincipalRequest>(out, golden, "auth_revoke_principal_request")?;
    write::<RevokePrincipalResponse>(out, golden, "auth_revoke_principal_response")?;

    // Phase 4 section 16: shared security primitives.
    write::<SecretRef>(out, golden, "security_secret_ref")?;
    write::<TlsConfig>(out, golden, "security_tls_config")?;

    // Stage 3 C2a: Bifrost wire contract (table management + query).
    write::<BifrostTableEntry>(out, golden, "bifrost_table_entry")?;
    write::<BifrostTableDescription>(out, golden, "bifrost_table_description")?;
    write::<TableStatus>(out, golden, "bifrost_table_status")?;
    write::<DataTypeSpec>(out, golden, "bifrost_data_type_spec")?;
    write::<BifrostFieldSpec>(out, golden, "bifrost_field_spec")?;
    write::<TimeUnit>(out, golden, "bifrost_time_unit")?;
    write::<PartitionTransformWire>(out, golden, "bifrost_partition_transform")?;
    write::<PartitionColumnSpec>(out, golden, "bifrost_partition_column_spec")?;
    write::<RegisterTableRequest>(out, golden, "bifrost_register_table_request")?;
    write::<RegisterOutcome>(out, golden, "bifrost_register_outcome")?;
    write::<RegisterTableResponse>(out, golden, "bifrost_register_table_response")?;
    write::<QueryParam>(out, golden, "bifrost_query_param")?;
    write::<BifrostQueryRequest>(out, golden, "bifrost_query_request")?;
    write::<SyncQueryRequest>(out, golden, "bifrost_sync_query_request")?;
    write::<AuditEvent>(out, golden, "bifrost_audit_event")?;
    write::<AuthMethod>(out, golden, "bifrost_audit_auth_method")?;
    write::<AuditDecision>(out, golden, "bifrost_audit_decision")?;
    write::<AuditResult>(out, golden, "bifrost_audit_result")?;
    // Stage 3 C7: Bifrost capability catalog — error descriptors + permission descriptors.
    // One source (these types in vala::api), two consumers: gen_schemas (snapshot) +
    // bifrost.list_errors / bifrost.list_permissions MCP tools. Schema drift detected here.
    write::<BifrostErrorDescriptor>(out, golden, "bifrost_error_descriptor")?;
    write::<BifrostPermissionDescriptor>(out, golden, "bifrost_permission_descriptor")?;
    let eval_fixtures = Path::new("crates/wyrd-spec/tests/fixtures/eval/schemas");
    fs::create_dir_all(eval_fixtures)?;
    write_fixture::<EvalSpec>(eval_fixtures, "eval_spec")?;
    write_fixture::<EvalTask>(eval_fixtures, "eval_task")?;
    write_fixture::<EvalCondition>(eval_fixtures, "eval_condition")?;
    write_fixture::<ComparisonOperator>(eval_fixtures, "comparison_operator")?;
    write_fixture::<ExecutionPlan>(eval_fixtures, "execution_plan")?;
    write_fixture::<DagError>(eval_fixtures, "dag_error")?;
    write_fixture::<EvalRecordObservation>(eval_fixtures, "eval_record_observation")?;
    write_fixture::<EvalScenarioCollection>(eval_fixtures, "eval_scenario_collection")?;
    write_fixture::<EvalPassGate>(eval_fixtures, "eval_pass_gate")?;
    write_fixture::<EvalSampling>(eval_fixtures, "eval_sampling")?;
    write_fixture::<EvalRunOpenRequest>(eval_fixtures, "eval_run_open_request")?;
    write_fixture::<EvalRunOpenResponse>(eval_fixtures, "eval_run_open_response")?;
    write_fixture::<SimulatedUserMode>(eval_fixtures, "simulated_user_mode")?;
    write_fixture::<TurnDirective>(eval_fixtures, "turn_directive")?;
    write_fixture::<ConversationTurn>(eval_fixtures, "conversation_turn")?;
    write_fixture::<AgentTurnSubmission>(eval_fixtures, "agent_turn_submission")?;
    write_fixture::<UserTurnSubmission>(eval_fixtures, "user_turn_submission")?;
    write_fixture::<SimulatedUserTurn>(eval_fixtures, "simulated_user_turn")?;

    let trace_fixtures = Path::new("crates/wyrd-spec/tests/fixtures/trace/schemas");
    fs::create_dir_all(trace_fixtures)?;
    write_fixture::<SpanRecord>(trace_fixtures, "span_record")?;
    write_fixture::<SpanKind>(trace_fixtures, "span_kind")?;
    write_fixture::<SpanStatus>(trace_fixtures, "span_status")?;
    write_fixture::<SpanEvent>(trace_fixtures, "span_event")?;
    write_fixture::<SpanLink>(trace_fixtures, "span_link")?;
    write_fixture::<Resource>(trace_fixtures, "resource")?;
    write_fixture::<InstrumentationScope>(trace_fixtures, "instrumentation_scope")?;
    write_fixture::<TraceSummaryRecord>(trace_fixtures, "trace_summary_record")?;
    write_fixture::<GenAiSpanRecord>(trace_fixtures, "gen_ai_span_record")?;
    write_fixture::<GenAiEvalResult>(trace_fixtures, "gen_ai_eval_result")?;
    write_fixture::<AttributeValue>(trace_fixtures, "attribute_value")?;

    let obs_fixtures = Path::new("crates/wyrd-spec/tests/fixtures/observation/schemas");
    fs::create_dir_all(obs_fixtures)?;
    write_fixture::<ObservationEnvelope>(obs_fixtures, "observation_envelope")?;
    write_fixture::<ObservationKind>(obs_fixtures, "observation_kind")?;
    write_fixture::<RecordObservation>(obs_fixtures, "record_observation")?;
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

fn write_fixture<T: schemars::JsonSchema>(
    dir: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let json = serde_json::to_string_pretty(&schema)?;
    fs::write(dir.join(format!("{name}.schema.json")), format!("{json}\n"))?;
    Ok(())
}
