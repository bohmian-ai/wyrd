//! Card discovery and exact reads over MCP, sharing HTTP authorization and audit.

use rmcp::model::{CallToolResult, Tool};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{GetCardResponse, ListCardsRequest, ListCardsResponse};

use super::principals::{parse_args, tool};
use super::{WyrdMcpHandler, structured};
use crate::components::auth::Caller;
use crate::components::cards::routes::{get_card_by_ref_for, get_card_for, list_cards_for};
use crate::state::AppState;

/// Wire name of the one Card read, by exact reference or kind-qualified UID.
pub(super) const GET: &str = "cards.get";
/// Wire name of filtered Card discovery.
pub(super) const LIST: &str = "cards.list";

/// One Card selector: an exact `card_ref` alone, or `kind` with `card_uid`.
///
/// Both forms are optional on the wire so the input schema stays one object;
/// [`Self::read`] accepts exactly one complete form and refuses any other
/// combination before authorization or IO.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct GetCardArgs {
    /// Exact versioned Card identity, including space; use alone.
    #[serde(default)]
    pub(crate) card_ref: Option<CardRef>,
    /// Card kind namespace of `card_uid`; use with `card_uid`.
    #[serde(default)]
    pub(crate) kind: Option<CardKind>,
    /// Server-minted Card UID; use with `kind`.
    #[serde(default)]
    pub(crate) card_uid: Option<CardUid>,
}

/// Dispatches one validated selector to the shared audited read owners.
impl GetCardArgs {
    /// Read the selected Card through the HTTP read's authorization and audit.
    ///
    /// An exact `card_ref` goes to the reference read; `kind` with `card_uid`
    /// goes to the UID read. Any other combination is ambiguous or partial.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] for an ambiguous or partial selector,
    /// then the read owner's permission, not-found, or registry errors.
    pub(crate) async fn read(
        self,
        state: &AppState,
        caller: &Caller,
    ) -> Result<GetCardResponse, WyrdError> {
        match (self.card_ref, self.kind, self.card_uid) {
            (Some(card_ref), None, None) => get_card_by_ref_for(state, caller, &card_ref).await,
            (None, Some(kind), Some(card_uid)) => {
                get_card_for(state, caller, &kind, &card_uid).await
            }
            _ => Err(WyrdError::Validation {
                message: format!(
                    "{GET} takes either card_ref alone or kind with card_uid, not both or part"
                ),
                details: serde_json::json!({ "tool": GET }),
            }),
        }
    }
}

/// Advertise the Card reads; each operation still enforces cards:read when called.
pub(crate) fn descriptors() -> Vec<Tool> {
    vec![
        tool::<GetCardArgs, GetCardResponse>(
            GET,
            "Read a Card",
            "Read one Card in this caller's tenant by an exact versioned card_ref, including \
             space, or by kind with card_uid; pass exactly one form. card.status.verification \
             carries a Verifier's PSI/SPC baseline status and an owner Card's derived \
             binding_ids. card.relationships.outbound_refs and inbound_refs contain exact \
             related identities in each edge's ref field; follow them with cards.get by \
             card_ref. Requires cards:read.",
            true,
        ),
        tool::<ListCardsRequest, ListCardsResponse>(
            LIST,
            "List Cards",
            "Discover Cards in this caller's tenant by the existing filters. Returns metadata \
             summaries and an opaque next_cursor; choose an explicit version before reading. \
             No fuzzy search or automatic version selection. Requires cards:read.",
            true,
        ),
    ]
}

impl WyrdMcpHandler {
    /// Read one Card by either selector through the shared audited reads.
    ///
    /// # Errors
    /// Returns invalid or ambiguous arguments, permission denials, or registry
    /// read errors.
    pub(super) async fn mcp_get_card(
        &self,
        caller: Caller,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetCardArgs = parse_args(arguments, GET)?;
        structured(&args.read(&self.state, &caller).await?)
    }

    /// Discover Cards through HTTP's authorization, audit, and list operation.
    ///
    /// # Errors
    /// Returns invalid arguments, permission denials, or registry list errors.
    pub(super) async fn mcp_list_cards(
        &self,
        caller: Caller,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, WyrdError> {
        let request = parse_args(Some(arguments.unwrap_or_default()), LIST)?;
        structured(&list_cards_for(&self.state, &caller, request).await?)
    }
}
