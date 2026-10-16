//! Agent and Prompt Card resolution for verification Agents.
//!
//! [`TenantRegistry`] is the server's one [`AgentCardResolver`] and
//! [`PromptCardResolver`]. Every lookup is the accepted caller's audited
//! `card:read` through the registry's own authorized reads, so a narrowed
//! or delegated authority can resolve only the Cards it may read, and the
//! decision is recorded like any other Card read.

use async_trait::async_trait;
use vala_eval::blocks::{AgentCardResolver, BlockError, PromptCardResolver};
use wyrd_spec::card::agent::AgentSpec;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;

use crate::components::auth::Caller;
use crate::components::cards::routes::{get_card_by_ref_for, get_card_for};
use crate::state::AppState;

/// Resolves Agent and Prompt Cards as one accepted caller.
pub struct TenantRegistry {
    /// Server state owning the registry.
    state: AppState,
    /// The authority every lookup is authorized and audited under; `None`
    /// when the execution has no accepted caller, and every lookup refuses.
    caller: Option<Caller>,
}

impl TenantRegistry {
    /// A resolver reading Cards as `caller`; with no caller, every lookup
    /// refuses rather than reading without authority.
    #[must_use]
    pub const fn new(state: AppState, caller: Option<Caller>) -> Self {
        Self { state, caller }
    }

    /// Load the spec of the Card `reference` names.
    ///
    /// A reference with a space reads by identity and, when it also pins a
    /// UID, must match it; a UID-only reference reads that exact Card of the
    /// reference's kind. Both reads authorize `card:read` first.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] with no caller, for a reference with
    /// neither space nor UID, or for a denied, absent, or mismatched Card,
    /// and [`BlockError::Retryable`] when the registry is unavailable.
    async fn spec(&self, reference: &CardRef) -> Result<Spec, BlockError> {
        let caller = self.caller.as_ref().ok_or_else(|| BlockError::Terminal {
            reason: format!("no accepted caller may read card {reference}"),
        })?;
        let response = match (&reference.space, &reference.uid) {
            (Some(_), _) => get_card_by_ref_for(&self.state, caller, reference).await,
            (None, Some(uid)) => get_card_for(&self.state, caller, &reference.kind, uid).await,
            (None, None) => {
                return Err(BlockError::Terminal {
                    reason: format!("reference {reference} names no space or uid"),
                });
            }
        };
        Ok(response.map_err(|error| refusal(&error))?.card.spec)
    }
}

#[async_trait]
impl AgentCardResolver for TenantRegistry {
    /// Resolve an Agent Card the caller may read.
    ///
    /// # Errors
    /// Returns [`TenantRegistry::spec`]'s errors, and
    /// [`BlockError::Terminal`] when the Card is not an Agent.
    async fn resolve(&self, agent_ref: &CardRef) -> Result<AgentSpec, BlockError> {
        match self.spec(agent_ref).await? {
            Spec::Agent(spec) => Ok(spec),
            _ => Err(BlockError::Terminal {
                reason: format!("card {agent_ref} is not an Agent"),
            }),
        }
    }
}

#[async_trait]
impl PromptCardResolver for TenantRegistry {
    /// Resolve a Prompt Card the caller may read into its runtime prompt.
    ///
    /// # Errors
    /// Returns [`TenantRegistry::spec`]'s errors, and
    /// [`BlockError::Terminal`] when the Card is not a Prompt.
    async fn resolve(&self, prompt_ref: &CardRef) -> Result<skald_prompt::Prompt, BlockError> {
        match self.spec(prompt_ref).await? {
            Spec::Prompt(spec) => Ok(skald_prompt::Prompt::from_native(spec.prompt)),
            _ => Err(BlockError::Terminal {
                reason: format!("card {prompt_ref} is not a Prompt"),
            }),
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
