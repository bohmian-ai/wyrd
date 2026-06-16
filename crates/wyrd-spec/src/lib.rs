//! Wyrd foundation types.
//!
//! `wyrd-spec` is pure data: no IO, no async runtime, no PyO3, and no server
//! framework dependencies. It defines the Card envelope, v1 Spec payloads,
//! cross-cutting identifiers, and generated schema surfaces used by every
//! downstream crate.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod actor;
pub mod auth;
pub mod authz;
pub mod card;
pub mod envelope;
pub mod error;
pub mod format;
pub mod ids;
pub mod metadata;
pub mod redaction;
pub mod reference;
pub mod request_id;
pub mod run;
pub mod schema;
pub mod storage;
pub mod trace;
pub mod version;

pub use authz::{Principal, Role, Scope};
pub use card::agent::{AgentCard, AgentCardError, AgentRunConfigSpec, AgentSpec};
pub use card::data::{
    ArrowFormat, ArrowMeta, ColValue, ColorMode, CustomDataMeta, DataInterface, DataSchema,
    DataSpec, DataSplit, DataStats, HuggingfaceMeta, ImageFormat, ImageMeta, Inequality,
    JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta, ParquetCompression,
    ParquetMeta, PolarsMeta, SplitStrategy, SqlLogic, SqlMeta, TextMeta, TorchMeta,
    TorchSaveFormat,
};
pub use card::field::{Dim, FieldSpec};
pub use card::prompt::{
    CardLoadFormat, ParameterName, PromptRef, PromptSpec, extract_text_placeholders,
    parse_card_bytes, parse_spec_bytes, serialize_card, serialize_spec_bytes,
};
pub use card::workflow::{
    WorkflowAction, WorkflowCard, WorkflowCardError, WorkflowRetryPolicy, WorkflowSpec,
    WorkflowStep, WorkflowValidationError,
};
pub use ids::{ColumnName, DataTenantId, QueryName, RoleName, SplitName, TenantSlug};
pub use metadata::{
    AnnotationKey, AnnotationValue, Annotations, CardMetadata, LabelKey, LabelValue, Labels,
    MetadataError,
};
pub use reference::AgentRef;
pub use skald_spec::{MessageNum, Prompt, ProviderRequest, ProviderResponse, ResponseType};
