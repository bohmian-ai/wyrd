//! Generate the committed JSON schemas under `crates/wyrd-spec/schemas/`.
//!
//! The drift tests and `mise run codegen:check` compare against these files.

use std::error::Error as StdError;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use schemars::schema_for;
use serde_json::{Map, Value, json, to_string_pretty};
use wyrd_spec::auth::{
    AbsoluteUrl, CallbackQuery, GatewayAccess, IssuerUrl, LoginInitResponse, PrincipalKindTag,
    RevokePrincipalRequest, TokenRequest, TokenResponse,
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
    HttpAuth, HttpMethod, NotifyChannel, OperatorAction, OperatorBudget, OperatorFailureContext,
    OperatorSpec, PagerDutySeverity,
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
use wyrd_spec::card::trigger::{TriggerActivation, TriggerSpec};
use wyrd_spec::card::verifier::{VerificationBinding, VerifierImplementation, VerifierSpec};
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRun, WorkflowSpec};
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayAccountingEntryV1, GatewayAttemptSpanFieldsV1, GatewayCallPayloadV1,
    GatewayCapturePolicy, GatewayCapturePolicyWrite, GatewayFallbackOverride,
    GatewayFallbackPolicy, GatewayGovernancePolicy, ProviderCredentialView,
    ProviderCredentialWrite, ProviderDeployment,
};
use wyrd_spec::operator_connection::{
    CreateOperatorConnectionRequest, OperatorConnectionView, UpdateOperatorConnectionRequest,
};
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
    AuditEvent, AuditOutcome, AuthMethod, BifrostQueryRequest, BifrostTableDescription,
    BifrostTableEntry, CancelRunningQueryRequest, CancelRunningQueryResponse, DataTypeSpec,
    FieldSpec as BifrostFieldSpec, GetRunningQueryRequest, ListRunningQueriesResponse,
    NullOrderWire, PhysicalLayoutWire, RegisterOutcome, RegisterTableRequest,
    RegisterTableResponse, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    SortDirectionWire, SortKeyWire, TableStatus, TimeGranularityWire, TimeUnit,
};
use wyrd_spec::vala::eval::{
    AgentTurnSubmission, ComparisonOperator, ConversationTurn, DagError, EvalCondition,
    EvalPassGate, EvalRecordObservation, EvalSampling, EvalScenarioCollection, EvalSpec, EvalTask,
    ExecutionPlan, SimulatedUserMode, SimulatedUserTurn, TurnDirective, UserTurnSubmission,
};
use wyrd_spec::vala::observation::{ObservationEnvelope, ObservationKind, RecordObservation};
use wyrd_spec::vala::trace::{
    AttributeValue, GenAiEvalResult, GenAiSpanRecord, InstrumentationScope, Resource, SpanEvent,
    SpanKind, SpanLink, SpanRecord, SpanStatus, TraceSummaryRecord,
};
use wyrd_spec::verification::{
    StartVerificationRunRequest, StartVerificationRunResponse, VerificationBindingStatus,
    VerificationRunStatus,
};

/// Regenerates every committed wyrd-spec schema, test schema fixture, and the
/// TypeScript error-code union. Run from the repository root.
///
/// # Errors
/// Returns serialization or filesystem failures from any output.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = Path::new("crates/wyrd-spec/schemas");
    fs::create_dir_all(out)?;
    write_problem_examples(out)?;

    write::<Card>(out, "card")?;
    write::<CardKind>(out, "card_kind")?;
    write::<CardRef>(out, "card_ref")?;
    write::<RunKind>(out, "run_kind")?;
    write::<RunRef>(out, "run_ref")?;
    write::<ServiceLock>(out, "service_lock")?;
    write::<LockedComponent>(out, "locked_component")?;
    write::<FieldSpec>(out, "field_spec")?;
    write::<DataSchema>(out, "data_schema")?;
    write::<SplitStrategy>(out, "split_strategy")?;
    write::<DataSplit>(out, "data_split")?;
    write::<DataInterface>(out, "data_interface")?;
    write::<SqlLogic>(out, "sql_logic")?;
    write::<DataStats>(out, "data_stats")?;
    write::<DataSpec>(out, "data_spec")?;
    write::<ModelSpec>(out, "model_spec")?;
    write::<ModelInterface>(out, "model_interface")?;
    write::<TaskType>(out, "task_type")?;
    write::<ModelSignature>(out, "model_signature")?;
    write::<SampleInput>(out, "sample_input")?;
    write::<SampleInputKind>(out, "sample_input_kind")?;
    write::<TorchSaveFormat>(out, "torch_save_format")?;
    write::<TfSaveFormat>(out, "tf_save_format")?;
    write::<HuggingFaceTask>(out, "hugging_face_task")?;
    write::<ExperimentSpec>(out, "experiment_spec")?;
    write::<PromptSpec>(out, "prompt_spec")?;
    write::<PromptRef>(out, "prompt_ref")?;
    write::<ParameterName>(out, "parameter_name")?;
    write::<AgentSpec>(out, "agent_spec")?;
    write::<WorkflowSpec>(out, "workflow_spec")?;
    write::<WorkflowRun>(out, "workflow_run")?;
    write::<CreateWorkflowRunRequest>(out, "create_workflow_run_request")?;
    write::<CardEvalSpec>(out, "eval_spec")?;
    write::<DriftSpec>(out, "drift_spec")?;
    write::<TriggerSpec>(out, "trigger_spec")?;
    write::<TriggerActivation>(out, "trigger_activation")?;
    write::<VerifierSpec>(out, "verifier_spec")?;
    write::<VerifierImplementation>(out, "verifier_implementation")?;
    write::<VerificationBinding>(out, "verification_binding")?;
    write::<OperatorSpec>(out, "operator_spec")?;
    write::<OperatorAction>(out, "operator_action")?;
    write::<NotifyChannel>(out, "notify_channel")?;
    write::<HttpMethod>(out, "http_method")?;
    write::<HttpAuth>(out, "http_auth")?;
    write::<OperatorBudget>(out, "operator_budget")?;
    write::<PagerDutySeverity>(out, "pager_duty_severity")?;
    write::<OperatorFailureContext>(out, "operator_failure_context")?;
    write::<SourceSpec>(out, "source_spec")?;
    write::<SourceKind>(out, "source_kind")?;
    write::<SqlConnection>(out, "sql_connection")?;
    write::<MetricsConnection>(out, "metrics_connection")?;
    write::<LogConnection>(out, "log_connection")?;
    write::<TraceConnection>(out, "trace_connection")?;
    write::<SourceAuth>(out, "source_auth")?;
    write::<ServiceSpec>(out, "service_spec")?;
    write::<ServiceRuntime>(out, "service_runtime")?;
    write::<ServiceRuntimeKind>(out, "service_runtime_kind")?;
    write::<ServiceRuntimeMode>(out, "service_runtime_mode")?;
    write::<ServiceRuntimePolicy>(out, "service_runtime_policy")?;
    write::<PolicySpec>(out, "policy_spec")?;
    write::<InvokeContext>(out, "invoke_context")?;
    write::<InvokeOutcome>(out, "invoke_outcome")?;
    write::<PolicyDecision>(out, "policy_decision")?;
    write::<McpSpec>(out, "mcp_spec")?;
    write::<AuditSpec>(out, "audit_spec")?;
    write::<ArtifactSpec>(out, "artifact_spec")?;
    write::<FrameworkAdapterRef>(out, "framework_adapter_ref")?;
    write::<UploadInitRequest>(out, "upload_init_request")?;
    write::<UploadInitResponse>(out, "upload_init_response")?;
    write::<UploadPlan>(out, "upload_plan")?;
    write::<UploadCompleteRequest>(out, "upload_complete_request")?;
    write::<UploadCompleteResponse>(out, "upload_complete_response")?;
    write::<PartUrlResponse>(out, "part_url_response")?;
    write::<AbortResponse>(out, "abort_response")?;
    write::<LocalBlobUploadResponse>(out, "local_blob_upload_response")?;
    write::<DownloadInitRequest>(out, "download_init_request")?;
    write::<DownloadInitResponse>(out, "download_init_response")?;
    write::<DownloadPlan>(out, "download_plan")?;
    write::<WireProtocol>(out, "wire_protocol")?;
    write::<VerificationGuarantee>(out, "verification_guarantee")?;

    // Card registration wire contracts (task 01).
    write::<CardSubmission>(out, "card_submission")?;
    write::<ArtifactManifestEntry>(out, "artifact_manifest_entry")?;
    write::<CardRegistrationOutcome>(out, "card_registration_outcome")?;
    write::<CardUploadPlan>(out, "card_upload_plan")?;
    write::<CardUploadEntry>(out, "card_upload_entry")?;
    write::<CreateCardRequest>(out, "create_card_request")?;
    write::<CreateCardResponse>(out, "create_card_response")?;
    write::<RegistrationReplaySeed>(out, "registration_replay_seed")?;
    write::<RegistrationOperationId>(out, "registration_operation_id")?;
    write::<RelativeArtifactPath>(out, "relative_artifact_path")?;
    write::<CardLifecycleStatus>(out, "card_lifecycle_status")?;
    write::<RegistrationOutcomeKind>(out, "registration_outcome_kind")?;
    write::<GetCardResponse>(out, "get_card_response")?;
    write::<DeleteCardResponse>(out, "delete_card_response")?;
    write::<CardSummary>(out, "card_summary")?;
    write::<ListCardsRequest>(out, "list_cards_request")?;
    write::<ListCardsResponse>(out, "list_cards_response")?;
    write::<ListVersionsResponse>(out, "list_versions_response")?;
    write::<CardLocator>(out, "card_locator")?;
    write::<ArtifactInventoryResponse>(out, "artifact_inventory_response")?;
    write::<StoredArtifactEntry>(out, "stored_artifact_entry")?;

    // Auth contracts.
    write::<TokenRequest>(out, "auth_token_request")?;
    write::<TokenResponse>(out, "auth_token_response")?;
    write::<AbsoluteUrl>(out, "auth_url")?;
    write::<IssuerUrl>(out, "auth_issuer_url")?;
    write::<LoginInitResponse>(out, "auth_login_init_response")?;
    write::<CallbackQuery>(out, "auth_callback_query")?;
    write::<PrincipalKindTag>(out, "auth_principal_kind")?;
    write::<RevokePrincipalRequest>(out, "auth_revoke_principal_request")?;

    // Phase 4 section 16: shared security primitives.
    write::<SecretRef>(out, "security_secret_ref")?;
    write::<TlsConfig>(out, "security_tls_config")?;

    // Gateway V1 administration, accounting, and capture contracts.
    write::<GatewayAccess>(out, "auth_gateway_access")?;
    write::<ProviderCredentialWrite>(out, "gateway_provider_credential_write")?;
    write::<ProviderCredentialView>(out, "gateway_provider_credential_view")?;
    write::<ProviderDeployment>(out, "gateway_provider_deployment")?;
    write::<GatewayFallbackPolicy>(out, "gateway_fallback_policy")?;
    write::<GatewayFallbackOverride>(out, "gateway_fallback_override")?;
    write::<GatewayGovernancePolicy>(out, "gateway_governance_policy")?;
    write::<GatewayCapturePolicyWrite>(out, "gateway_capture_policy_write")?;
    write::<GatewayCapturePolicy>(out, "gateway_capture_policy")?;
    write::<GatewayAccountingEntryV1>(out, "gateway_accounting_entry_v1")?;
    write::<GatewayCallPayloadV1>(out, "gateway_call_payload_v1")?;
    write::<GatewayAttemptSpanFieldsV1>(out, "gateway_attempt_span_fields_v1")?;

    // Stage 3 C2a: Bifrost wire contract (table management + query).
    write::<BifrostTableEntry>(out, "bifrost_table_entry")?;
    write::<BifrostTableDescription>(out, "bifrost_table_description")?;
    write::<TableStatus>(out, "bifrost_table_status")?;
    write::<DataTypeSpec>(out, "bifrost_data_type_spec")?;
    write::<BifrostFieldSpec>(out, "bifrost_field_spec")?;
    write::<TimeUnit>(out, "bifrost_time_unit")?;
    write::<TimeGranularityWire>(out, "bifrost_time_granularity")?;
    write::<SortDirectionWire>(out, "bifrost_sort_direction")?;
    write::<NullOrderWire>(out, "bifrost_null_order")?;
    write::<SortKeyWire>(out, "bifrost_sort_key")?;
    write::<PhysicalLayoutWire>(out, "bifrost_physical_layout")?;
    write::<RegisterTableRequest>(out, "bifrost_register_table_request")?;
    write::<RegisterOutcome>(out, "bifrost_register_outcome")?;
    write::<RegisterTableResponse>(out, "bifrost_register_table_response")?;
    write::<BifrostQueryRequest>(out, "bifrost_query_request")?;
    write::<RunningQueryLifecycleState>(out, "bifrost_running_query_lifecycle_state")?;
    write::<RunningQueryProgress>(out, "bifrost_running_query_progress")?;
    write::<RunningQuerySummary>(out, "bifrost_running_query_summary")?;
    write::<ListRunningQueriesResponse>(out, "bifrost_list_running_queries_response")?;
    write::<GetRunningQueryRequest>(out, "bifrost_get_running_query_request")?;
    write::<CancelRunningQueryRequest>(out, "bifrost_cancel_running_query_request")?;
    write::<CancelRunningQueryResponse>(out, "bifrost_cancel_running_query_response")?;
    write::<AuditEvent>(out, "bifrost_audit_event")?;
    write::<AuthMethod>(out, "bifrost_audit_auth_method")?;
    write::<AuditOutcome>(out, "bifrost_audit_outcome")?;
    write::<StartVerificationRunRequest>(out, "start_verification_run_request")?;
    write::<StartVerificationRunResponse>(out, "start_verification_run_response")?;
    write::<VerificationBindingStatus>(out, "verification_binding_status")?;
    write::<VerificationRunStatus>(out, "verification_run_status")?;
    write::<CreateOperatorConnectionRequest>(out, "create_operator_connection_request")?;
    write::<UpdateOperatorConnectionRequest>(out, "update_operator_connection_request")?;
    write::<OperatorConnectionView>(out, "operator_connection_view")?;
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
    let variants = codes.iter().fold(String::new(), |mut variants, code| {
        write!(variants, "\n  | \"{code}\"").expect("writing to a String cannot fail");
        variants
    });
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

/// Writes `T`'s JSON schema to `out/<name>.json` with the 2020-12 meta-schema.
///
/// The bytes match what the in-crate drift tests regenerate, so a changed
/// wire type shows up as a drift until this generator is rerun.
///
/// # Errors
/// Returns serialization or filesystem failures.
fn write<T: schemars::JsonSchema>(
    out: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut schema = schema_for!(T);
    schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    let json = serde_json::to_string_pretty(&schema)?;
    fs::write(out.join(format!("{name}.json")), format!("{json}\n"))?;
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

/// Exports safe BFF examples to `out` using the same error projection as the Rust server.
///
/// The UI imports `ui_problem_examples.json` so its problem payloads match the
/// derive-backed catalog without duplicating wire metadata.
///
/// # Errors
/// Returns serialization or filesystem failures.
fn write_problem_examples(out: &Path) -> Result<(), Box<dyn StdError>> {
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
    fs::write(out.join("ui_problem_examples.json"), output)?;
    Ok(())
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
