//! Generate JSON schema goldens.

use std::error::Error as StdError;
use std::fs;
use std::path::Path;

use schemars::schema_for;
use serde_json::{Map, Value, json, to_string_pretty};
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
use wyrd_spec::card::operator::{
    HttpAuth, HttpMethod, NotifyChannel, OperatorAction, OperatorBudget, OperatorSpec,
};
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
use wyrd_spec::card::trigger::{TriggerSchedule, TriggerSource, TriggerSpec};
use wyrd_spec::card::workflow::WorkflowSpec;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{
    ArtifactInventoryResponse, ArtifactManifestEntry, CardLifecycleStatus, CardLocator,
    CardRegistrationOutcome, CardSubmission, CardSummary, CardUploadEntry, CardUploadPlan,
    CreateCardRequest, CreateCardResponse, DeleteCardResponse, GetCardResponse, ListCardsRequest,
    ListCardsResponse, ListVersionsResponse, RegistrationOperationId, RegistrationOutcomeKind,
    RegistrationReplaySeed, RelativeArtifactPath, StoredArtifactEntry,
};
use wyrd_spec::run::{RunKind, RunRef};
use wyrd_spec::security::{SecretRef, TlsConfig};
use wyrd_spec::storage::{
    AbortResponse, DownloadInitRequest, DownloadInitResponse, DownloadPlan,
    LocalBlobUploadResponse, PartUrlResponse, UploadCompleteRequest, UploadCompleteResponse,
    UploadInitRequest, UploadInitResponse, UploadPlan, VerificationGuarantee, WireProtocol,
};
use wyrd_spec::vala::api::{
    AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest, BifrostTableDescription, BifrostTableEntry, CancelRunningQueryRequest,
    CancelRunningQueryResponse, DataTypeSpec, FieldSpec as BifrostFieldSpec,
    GetRunningQueryRequest, ListRunningQueriesResponse, NullOrderWire, PhysicalLayoutWire,
    RegisterOutcome, RegisterTableRequest, RegisterTableResponse,
    RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary, SortDirectionWire,
    SortKeyWire, TableStatus, TimeGranularityWire, TimeUnit,
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
    ProblemExamples { out, golden }.write()?;

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
    write::<TriggerSchedule>(out, golden, "trigger_schedule")?;
    write::<TriggerSource>(out, golden, "trigger_source")?;
    write::<OperatorSpec>(out, golden, "operator_spec")?;
    write::<OperatorAction>(out, golden, "operator_action")?;
    write::<NotifyChannel>(out, golden, "notify_channel")?;
    write::<HttpMethod>(out, golden, "http_method")?;
    write::<HttpAuth>(out, golden, "http_auth")?;
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
    write::<ArtifactManifestEntry>(out, golden, "artifact_manifest_entry")?;
    write::<CardRegistrationOutcome>(out, golden, "card_registration_outcome")?;
    write::<CardUploadPlan>(out, golden, "card_upload_plan")?;
    write::<CardUploadEntry>(out, golden, "card_upload_entry")?;
    write::<CreateCardRequest>(out, golden, "create_card_request")?;
    write::<CreateCardResponse>(out, golden, "create_card_response")?;
    write::<RegistrationReplaySeed>(out, golden, "registration_replay_seed")?;
    write::<RegistrationOperationId>(out, golden, "registration_operation_id")?;
    write::<RelativeArtifactPath>(out, golden, "relative_artifact_path")?;
    write::<CardLifecycleStatus>(out, golden, "card_lifecycle_status")?;
    write::<RegistrationOutcomeKind>(out, golden, "registration_outcome_kind")?;
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
    write::<TimeGranularityWire>(out, golden, "bifrost_time_granularity")?;
    write::<SortDirectionWire>(out, golden, "bifrost_sort_direction")?;
    write::<NullOrderWire>(out, golden, "bifrost_null_order")?;
    write::<SortKeyWire>(out, golden, "bifrost_sort_key")?;
    write::<PhysicalLayoutWire>(out, golden, "bifrost_physical_layout")?;
    write::<RegisterTableRequest>(out, golden, "bifrost_register_table_request")?;
    write::<RegisterOutcome>(out, golden, "bifrost_register_outcome")?;
    write::<RegisterTableResponse>(out, golden, "bifrost_register_table_response")?;
    write::<BifrostQueryRequest>(out, golden, "bifrost_query_request")?;
    write::<RunningQueryLifecycleState>(out, golden, "bifrost_running_query_lifecycle_state")?;
    write::<RunningQueryProgress>(out, golden, "bifrost_running_query_progress")?;
    write::<RunningQuerySummary>(out, golden, "bifrost_running_query_summary")?;
    write::<ListRunningQueriesResponse>(out, golden, "bifrost_list_running_queries_response")?;
    write::<GetRunningQueryRequest>(out, golden, "bifrost_get_running_query_request")?;
    write::<CancelRunningQueryRequest>(out, golden, "bifrost_cancel_running_query_request")?;
    write::<CancelRunningQueryResponse>(out, golden, "bifrost_cancel_running_query_response")?;
    write::<AuditEvent>(out, golden, "bifrost_audit_event")?;
    write::<AuthMethod>(out, golden, "bifrost_audit_auth_method")?;
    write::<AuditOutcome>(out, golden, "bifrost_audit_outcome")?;
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

    write_ts_error_codes(Path::new("sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts"))?;
    Ok(())
}

/// Emit the TypeScript `WyrdErrorCode` literal union from the derive-backed
/// catalog, so `@wyrd/sdk` never hand-maintains the code set.
///
/// Codes are sorted and de-duplicated for a stable, reviewable diff.
///
/// # Errors
/// Returns filesystem failures writing `path`.
fn write_ts_error_codes(path: &Path) -> Result<(), Box<dyn StdError>> {
    let codes: std::collections::BTreeSet<&str> = WyrdError::codes().into_iter().collect();
    let variants: String = codes
        .iter()
        .map(|code| format!("\n  | \"{code}\""))
        .collect();
    fs::write(
        path,
        format!(
            "// Generated by `mise run codegen:regen` from `wyrd_spec::error::WyrdError`. Do not edit.\n\n\
             /** Stable Wyrd error code from the derive-backed catalog. */\n\
             export type WyrdErrorCode ={variants};\n"
        ),
    )?;
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

/// Exports safe BFF examples using the same error projection as the Rust server.
struct ProblemExamples<'a> {
    /// Public generated contract directory.
    out: &'a Path,
    /// Golden directory checked by the existing codegen lane.
    golden: &'a Path,
}

impl ProblemExamples<'_> {
    /// Write catalog-derived examples without duplicating wire metadata.
    ///
    /// # Errors
    /// Returns serialization or filesystem failures from either output.
    fn write(&self) -> Result<(), Box<dyn StdError>> {
        let examples = [
            (
                "validation",
                WyrdError::Validation {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "notFound",
                WyrdError::NotFound {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "conflict",
                WyrdError::Conflict {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "unauthenticated",
                WyrdError::Unauthenticated {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "expired",
                WyrdError::TokenExpired {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "denied",
                WyrdError::PermissionDeniedRbac {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "internal",
                WyrdError::Internal {
                    message: String::new(),
                    details: json!({}),
                },
            ),
            (
                "upstream",
                WyrdError::UpstreamFailure {
                    message: String::new(),
                    details: json!({}),
                },
            ),
        ];
        let mut catalog = Map::new();
        for (name, error) in examples {
            let mut value = error.as_problem_json();
            value["detail"] = Value::from(error.title());
            catalog.insert(name.to_owned(), value);
        }
        let output = to_string_pretty(&sort_keys(Value::Object(catalog)))? + "\n";
        for directory in [self.out, self.golden] {
            fs::write(directory.join("ui_problem_examples.json"), &output)?;
        }
        Ok(())
    }
}

/// Rebuilds `value` with every object's keys in sorted order.
///
/// `serde_json::Map` iterates in insertion order whenever any crate in the
/// build graph enables `serde_json/preserve_order`, and in sorted order
/// otherwise. Sorting before writing keeps the generated catalog byte-stable
/// regardless of the build's feature union, so `codegen:check` compares
/// content rather than feature resolution.
fn sort_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, nested)| (key, sort_keys(nested)))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_keys).collect()),
        scalar => scalar,
    }
}
