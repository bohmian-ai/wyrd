//! Generate the consumed outputs derived from `wyrd-spec` contracts.
//!
//! The written outputs are the UI problem examples and the TypeScript error-code
//! union, which `mise run codegen:check` keeps reproducible. The `contracts`
//! mode prints the SDK wire-contract bundle that the TypeScript type emitter and
//! the docs diagram check read from a pipe or temporary file.

use std::error::Error as StdError;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use schemars::r#gen::SchemaGenerator;
use serde_json::{Map, Value, json, to_string_pretty};
use wyrd_spec::auth::{
    AbsoluteUrl, CallbackQuery, CreateServicePrincipalRequest, CreateServicePrincipalResponse,
    CredentialListResponse, GatewayAccess, IssuedCredential, IssuerUrl, LoginInitResponse,
    PrincipalKindTag, PrincipalPage, PrincipalQuery, PrincipalRoles, RevokePrincipalRequest,
    RoleAssignmentChange, TokenAudience, TokenRequest, TokenResponse,
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
    RegistrationReceipt, RegistrationReplaySeed, RelativeArtifactPath, StoredArtifactEntry,
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
    NullOrderWire, PhysicalLayoutWire, QueryTerminalFrame, RegisterOutcome, RegisterTableRequest,
    RegisterTableResponse, RunningQueryLifecycleState, RunningQueryProgress, RunningQuerySummary,
    SortDirectionWire, SortKeyWire, TableStatus, TimeGranularityWire, TimeUnit,
};
use wyrd_spec::vala::trace::SpanRecord;
use wyrd_spec::verification::{
    Judgment, StartVerificationRunRequest, StartVerificationRunResponse, VerificationBindingStatus,
    VerificationRunStatus,
};

/// Writes the consumed generated outputs, or prints the transient contract bundle.
///
/// Run from the repository root. Without arguments it rewrites the UI problem
/// examples and the TypeScript error-code union. With `contracts` it writes
/// nothing and prints [`contracts`] to stdout for the TypeScript type emitter
/// and the docs diagram check, so no schema tree is committed.
///
/// # Errors
/// Returns serialization or filesystem failures from any output, or an
/// unknown argument.
fn main() -> Result<(), Box<dyn StdError>> {
    match std::env::args().nth(1).as_deref() {
        None => {
            write_problem_examples(Path::new("crates/wyrd-spec/schemas"))?;
            write_ts_error_codes(Path::new("sdks/wyrd-sdk-ts/wyrd/src/error-codes.ts"))
        }
        Some("contracts") => {
            println!("{}", to_string_pretty(&contracts())?);
            Ok(())
        }
        Some(other) => Err(format!("unknown argument {other:?}; expected `contracts`").into()),
    }
}

/// Registers each listed type as a named root of one shared schema generator.
macro_rules! roots {
    ($generator:ident, $($ty:ty),+ $(,)?) => {
        [$((
            stringify!($ty).to_owned(),
            serde_json::to_value($generator.subschema_for::<$ty>())
                .expect("a derived schema serializes"),
        )),+]
    };
}

/// Builds one JSON document holding every SDK-facing wire contract.
///
/// All roots share a single draft-07 generator, so each Rust type appears once
/// under `definitions` by its schema name, and `roots` maps each root's Rust
/// type name (or its import alias) to its schema. Consumers resolve `#/definitions/<name>` within this
/// one document. The bundle is transient: it is printed, never committed.
///
/// # Panics
/// Panics if a derived schema fails to serialize, which a `JsonSchema` derive
/// cannot produce.
fn contracts() -> Value {
    let mut generator = SchemaGenerator::default();
    let roots: Map<String, Value> = roots!(
        generator,
        Card,
        CardKind,
        CardRef,
        RunKind,
        RunRef,
        ServiceLock,
        LockedComponent,
        FieldSpec,
        DataSchema,
        SplitStrategy,
        DataSplit,
        DataInterface,
        SqlLogic,
        DataStats,
        DataSpec,
        ModelSpec,
        ModelInterface,
        TaskType,
        ModelSignature,
        SampleInput,
        SampleInputKind,
        TorchSaveFormat,
        TfSaveFormat,
        HuggingFaceTask,
        ExperimentSpec,
        PromptSpec,
        PromptRef,
        ParameterName,
        AgentSpec,
        CardEvalSpec,
        DriftSpec,
        TriggerSpec,
        TriggerActivation,
        VerifierSpec,
        VerifierImplementation,
        VerificationBinding,
        OperatorSpec,
        OperatorAction,
        NotifyChannel,
        HttpMethod,
        HttpAuth,
        OperatorBudget,
        PagerDutySeverity,
        OperatorFailureContext,
        SourceSpec,
        SourceKind,
        SqlConnection,
        MetricsConnection,
        LogConnection,
        TraceConnection,
        SourceAuth,
        ServiceSpec,
        ServiceRuntime,
        ServiceRuntimeKind,
        ServiceRuntimeMode,
        ServiceRuntimePolicy,
        PolicySpec,
        InvokeContext,
        InvokeOutcome,
        PolicyDecision,
        McpSpec,
        AuditSpec,
        ArtifactSpec,
        FrameworkAdapterRef,
        UploadInitRequest,
        UploadInitResponse,
        UploadPlan,
        UploadCompleteRequest,
        UploadCompleteResponse,
        PartUrlResponse,
        AbortResponse,
        LocalBlobUploadResponse,
        DownloadInitRequest,
        DownloadInitResponse,
        DownloadPlan,
        WireProtocol,
        VerificationGuarantee,
        CardSubmission,
        ArtifactManifestEntry,
        CardRegistrationOutcome,
        CardUploadPlan,
        CardUploadEntry,
        CreateCardRequest,
        CreateCardResponse,
        RegistrationReceipt,
        RegistrationReplaySeed,
        RegistrationOperationId,
        RelativeArtifactPath,
        CardLifecycleStatus,
        RegistrationOutcomeKind,
        GetCardResponse,
        DeleteCardResponse,
        CardSummary,
        ListCardsRequest,
        ListCardsResponse,
        ListVersionsResponse,
        CardLocator,
        ArtifactInventoryResponse,
        StoredArtifactEntry,
        TokenRequest,
        TokenResponse,
        AbsoluteUrl,
        IssuerUrl,
        LoginInitResponse,
        CallbackQuery,
        PrincipalKindTag,
        RevokePrincipalRequest,
        TokenAudience,
        CreateServicePrincipalRequest,
        CreateServicePrincipalResponse,
        IssuedCredential,
        CredentialListResponse,
        PrincipalRoles,
        RoleAssignmentChange,
        PrincipalPage,
        PrincipalQuery,
        SecretRef,
        TlsConfig,
        GatewayAccess,
        ProviderCredentialWrite,
        ProviderCredentialView,
        ProviderDeployment,
        GatewayFallbackPolicy,
        GatewayFallbackOverride,
        GatewayGovernancePolicy,
        GatewayCapturePolicyWrite,
        GatewayCapturePolicy,
        GatewayAccountingEntryV1,
        GatewayCallPayloadV1,
        GatewayAttemptSpanFieldsV1,
        BifrostTableEntry,
        BifrostTableDescription,
        TableStatus,
        DataTypeSpec,
        BifrostFieldSpec,
        TimeUnit,
        TimeGranularityWire,
        SortDirectionWire,
        NullOrderWire,
        SortKeyWire,
        PhysicalLayoutWire,
        RegisterTableRequest,
        RegisterOutcome,
        RegisterTableResponse,
        BifrostQueryRequest,
        RunningQueryLifecycleState,
        RunningQueryProgress,
        RunningQuerySummary,
        ListRunningQueriesResponse,
        GetRunningQueryRequest,
        CancelRunningQueryRequest,
        CancelRunningQueryResponse,
        QueryTerminalFrame,
        AuditEvent,
        AuthMethod,
        AuditOutcome,
        StartVerificationRunRequest,
        StartVerificationRunResponse,
        VerificationBindingStatus,
        VerificationRunStatus,
        Judgment,
        CreateOperatorConnectionRequest,
        UpdateOperatorConnectionRequest,
        OperatorConnectionView,
        SpanRecord,
    )
    .into_iter()
    .collect();
    let definitions: Map<String, Value> = generator
        .definitions()
        .iter()
        .map(|(name, schema)| {
            let schema = serde_json::to_value(schema).expect("a derived schema serializes");
            (name.clone(), schema)
        })
        .collect();
    sort_keys(json!({ "roots": roots, "definitions": definitions }))
}

/// Emit the TypeScript `WyrdErrorCode` literal union from the derive-backed
/// catalog, so `wyrd-ai` never hand-maintains the code set.
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
