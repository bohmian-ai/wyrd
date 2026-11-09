//! Agent and Prompt Card resolution for verification Agents.
//!
//! [`TenantRegistry`] is the server's one [`AgentCardResolver`] and
//! [`PromptCardResolver`]. A Verifier's own definitions are engine-internal:
//! every lookup is the accepted caller's audited `verifier:run` on the
//! executing Verifier, never `cards:read`, followed by the tenant's exact
//! Card read, which excludes deleted Cards. A definition the read cannot
//! return is [`BlockError::Unavailable`], so the execution errors rather
//! than judging work it could not run.

use async_trait::async_trait;
use vala_verify::blocks::{AgentCardResolver, BlockError, PromptCardResolver};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

use wyrd_runtime::Permission;
use wyrd_spec::ids::CardUid;

use crate::audit;
use crate::components::auth::Caller;
use crate::components::cards::routes::card_resource;
use crate::components::cards::service::get_card_spec_by_ref;
use crate::state::AppState;

/// Resolves one executing Verifier's Agent and Prompt Cards as its accepted
/// caller.
pub struct TenantRegistry {
    /// Server state owning the registry.
    state: AppState,
    /// The authority every lookup is authorized and audited under; `None`
    /// when the execution has no accepted caller, and every lookup refuses.
    caller: Option<Caller>,
    /// The executing Verifier, whose `verifier:run` authorizes each lookup.
    verifier: CardUid,
}

impl TenantRegistry {
    /// A resolver reading `verifier`'s definitions as `caller`; with no
    /// caller, every lookup refuses rather than reading without authority.
    #[must_use]
    pub const fn new(state: AppState, caller: Option<Caller>, verifier: CardUid) -> Self {
        Self {
            state,
            caller,
            verifier,
        }
    }

    /// Load the spec of the Card `reference` names.
    ///
    /// The reference reads by its exact kind, name, and version identity and,
    /// when it also pins a UID, must match it. The read authorizes and
    /// audits `verifier:run` on the executing Verifier first.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] with no caller,
    /// [`BlockError::Unavailable`] for a denied, absent, deleted, or
    /// mismatched Card, and [`BlockError::Retryable`] when the registry is
    /// unavailable.
    async fn spec(&self, reference: &CardRef) -> Result<Spec, BlockError> {
        let caller = self.caller.as_ref().ok_or_else(|| BlockError::Terminal {
            reason: format!("no accepted caller may read card {reference}"),
        })?;
        let unavailable = |error: &WyrdError| {
            if error.status() >= 500 {
                refusal(error)
            } else {
                gone(reference)
            }
        };
        audit::authorize(
            &self.state,
            caller,
            &Permission::verifier_run(self.verifier.clone()),
            "verifier.definition.read",
            &card_resource(reference),
        )
        .map_err(|error| unavailable(&error))?;
        let row = get_card_spec_by_ref(&self.state, caller, reference)
            .await
            .map_err(|error| unavailable(&error))?;
        if reference
            .uid
            .as_ref()
            .is_some_and(|uid| *uid != row.card_uid)
        {
            return Err(gone(reference));
        }
        Ok(row.spec)
    }
}

/// The unavailability of required definition `reference`.
fn gone(reference: &CardRef) -> BlockError {
    BlockError::Unavailable {
        reason: format!("card {reference} is unavailable"),
    }
}

#[async_trait]
impl AgentCardResolver for TenantRegistry {
    /// Resolve the executing Verifier's exact Agent Card.
    ///
    /// # Errors
    /// Returns [`TenantRegistry::spec`]'s errors, and
    /// [`BlockError::Unavailable`] when the Card is not an Agent.
    async fn resolve(&self, agent_ref: &CardRef) -> Result<AgentSpec, BlockError> {
        match self.spec(agent_ref).await? {
            Spec::Agent(spec) => Ok(spec),
            _ => Err(gone(agent_ref)),
        }
    }
}

#[async_trait]
impl PromptCardResolver for TenantRegistry {
    /// Resolve the executing Verifier's exact Prompt Card into its runtime
    /// prompt.
    ///
    /// # Errors
    /// Returns [`TenantRegistry::spec`]'s errors, and
    /// [`BlockError::Unavailable`] when the Card is not a Prompt.
    async fn resolve(&self, prompt_ref: &CardRef) -> Result<skald_prompt::Prompt, BlockError> {
        match self.spec(prompt_ref).await? {
            Spec::Prompt(spec) => Ok(skald_prompt::Prompt::from_native(spec.prompt)),
            _ => Err(gone(prompt_ref)),
        }
    }
}

/// The block error of a refused registry read: retryable when the registry
/// failed (status 500 and above), terminal for a denial or bad reference.
pub(super) fn refusal(error: &WyrdError) -> BlockError {
    let reason = format!("the Card cannot be read: {error}");
    if error.status() >= 500 {
        BlockError::Retryable { reason }
    } else {
        BlockError::Terminal { reason }
    }
}
