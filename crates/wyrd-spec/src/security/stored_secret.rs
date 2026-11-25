//! Deployment wrapping-key maintenance contracts.
//!
//! A platform administrator runs one bounded, synchronous maintenance pass
//! over every recoverable stored secret, moving envelopes that do not name
//! the deployment's configured active wrapping key onto it. The destination
//! is always that active key: the request selects only the mode and an
//! optional work bound, never a key, owner, or tenant.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::DataTenantId;

/// How a maintenance pass moves an envelope that does not name the active
/// wrapping key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoredSecretMaintenanceMode {
    /// Routine rotation: rewrap each data key under the active key. The data
    /// key, payload nonce, and payload ciphertext are kept.
    Routine,
    /// Compromise recovery: reseal each value under a fresh data key and
    /// nonce wrapped by the active key. Run it only after the active key is a
    /// fresh, never-reused reference, and never run `routine` into that key.
    Recovery,
}

/// `POST /platform/stored-secrets/maintenance` body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct StoredSecretMaintenanceRequest {
    /// Routine rewrap or compromise recovery.
    pub mode: StoredSecretMaintenanceMode,
    /// Most envelopes this pass attempts to transition. Omitted, the pass
    /// attempts every observed envelope until its time budget elapses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "server", schema(value_type = Option<u32>, minimum = 1))]
    pub limit: Option<NonZeroU32>,
}

/// Most failed owners one report lists; `remaining` still counts every one.
pub const MAX_REPORTED_FAILURES: usize = 100;

/// What one maintenance pass observed and did.
///
/// Counts describe only this pass's observations: a write committed while it
/// ran can change the next pass's counts. `remaining == 0` is not a global
/// completion, retirement, or backup-safety acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct StoredSecretMaintenanceReport {
    /// The mode the pass ran.
    pub mode: StoredSecretMaintenanceMode,
    /// Observed envelopes that already named the active key.
    pub current: u64,
    /// Envelopes this pass moved to the active key.
    pub changed: u64,
    /// Observed envelopes still not under the active key: they failed, lost
    /// to a concurrent write, or were left by the work bound. Repeat the pass
    /// with the same mode to continue.
    pub remaining: u64,
    /// Whether the pass stopped at its limit or time budget before
    /// attempting every envelope. When the budget ends a store read, that
    /// store and every later one go unobserved and uncounted; repeat while
    /// this is `true`.
    pub bounded: bool,
    /// Up to [`MAX_REPORTED_FAILURES`] owners whose envelope this pass could
    /// not move, with no key or secret material.
    pub failures: Vec<StoredSecretFailure>,
}

/// One owner whose envelope a pass could not move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct StoredSecretFailure {
    /// The owning store.
    pub store: StoredSecretStore,
    /// Owning tenant; absent for the platform connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_tenant_id: Option<DataTenantId>,
    /// The owner within its store: a connection id, issuer URL, or Gateway
    /// credential name.
    pub owner: String,
    /// Why the envelope did not move.
    pub reason: StoredSecretFailureReason,
}

/// A store holding recoverable stored secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoredSecretStore {
    /// Tenant human sign-in connection client secrets.
    HumanConnection,
    /// Tenant workload trusted-issuer client secrets.
    TrustedIssuer,
    /// The platform OIDC connection client secret.
    PlatformOidc,
    /// Gateway managed provider credentials.
    GatewayCredential,
    /// Operator connection secrets.
    OperatorConnection,
    /// Shared social sign-in provider client secrets.
    SocialProvider,
    /// The application-mail SMTP relay password.
    ApplicationMail,
    /// The signup challenge secret.
    SignupChallenge,
    /// The platform derivation key.
    DerivationKey,
}

/// Why one envelope did not move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StoredSecretFailureReason {
    /// The envelope names a wrapping key the deployment no longer allows.
    KeyNotAllowed,
    /// The wrapping backend could not serve the call, or refused it.
    KeyUnavailable,
    /// The envelope did not authenticate under its owner, or is not a
    /// recognized envelope.
    Unopenable,
    /// The row's owner metadata cannot form a valid context.
    InvalidOwner,
    /// A concurrent write changed the row first; that write stands.
    ConcurrentChange,
}
