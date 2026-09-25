//! In-process governed inference component of `wyrd-server`.
//!
//! `wyrd-server` authenticates the caller, authorizes the exact models, admits
//! the call against tenant limits and budgets, and owns every durable write.
//! It then hands this crate an immutable [`GatewayTenantSnapshot`] and a
//! [`CallPlan`]; [`GatewayEngine::execute`] resolves credentials per attempt,
//! walks the deterministic deployment order and fallback candidates under the
//! caller's deadline and cancellation, and returns attributable attempt
//! evidence for server accounting.
//!
//! The crate never queries Postgres, never authenticates, and exposes no
//! listener: it is a Rust API consumed only inside `wyrd-server`.

mod adapter;
mod credential;
mod endpoint;
mod engine;
mod health;
mod managed;
mod pricing;
mod routing;
mod snapshot;
mod vault;

pub use adapter::{
    BATCH_ENDPOINTS, BatchAction, BatchInput, BuiltinEndpoints, HttpProviderDispatch,
    IngressDialect, MediaRequest, UnsupportedRequest, anthropic_error_body, google_error_body,
    openai_error_body,
};
pub use credential::{CredentialError, CredentialResolver, ProviderSecret, read_secret_file};
pub use endpoint::EndpointPolicy;
pub use engine::{
    AttemptRecord, AttemptResult, AttemptUsage, CallExecution, CallInput, EventStream,
    FailureClass, GatewayEngine, ProviderAttempt, ProviderDispatch, ProviderRefusal, ResponseBody,
    ResponseCapture, StreamAborted, StreamEnd, outcome_error_code, outcome_name,
};
pub use health::DeploymentHealth;
pub use managed::{ManagedSecretBinding, ManagedSecretEnvelope, ManagedSecretKeys, TenantKeyring};
pub use pricing::{GatewayCost, PricedModel};
pub use routing::{CallPlan, PlannedCandidate};
pub use skald_providers::{MediaAnswer, OpenAiMediaRoute, UploadContent, UploadFile};
pub use snapshot::{
    CredentialAssignment, GatewayCredentialSnapshot, GatewayCredentialSource, GatewayTenantSnapshot,
};
pub use vault::VaultBackend;
