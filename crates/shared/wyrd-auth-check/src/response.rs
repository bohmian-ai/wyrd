//! Authz-check response payload.

use serde::{Deserialize, Serialize};
use wyrd_spec::card::policy::PolicyDecision;

/// Successful authz-check response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthzCheckResponse {
    /// Policy decision returned by the mounted hook.
    pub decision: PolicyDecision,
}
