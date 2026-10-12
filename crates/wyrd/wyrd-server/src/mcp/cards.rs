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

/// Wire name of the existing kind-qualified UID read.
pub(super) const GET: &str = "cards.get";
/// Wire name of filtered Card discovery.
pub(super) const LIST: &str = "cards.list";
/// Wire name of the exact-reference read.
pub(super) const GET_BY_REF: &str = "cards.get_by_ref";

/// The existing UID-read contract; kind namespaces the server-minted UID.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetCardArgs {
    /// Card kind namespace of the UID.
    kind: CardKind,
    /// Server-minted Card UID.
    card_uid: CardUid,
}

/// Exact identity selected from discovery or a server-derived relationship.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetCardByRefArgs {
    /// Versioned Card identity; reads require its resolved space.
    card_ref: CardRef,
}

/// Advertise all Card reads; each operation still enforces cards:read when called.
pub(super) fn descriptors() -> Vec<Tool> {
    vec![
        tool::<GetCardArgs, GetCardResponse>(
            GET,
            "Read a Card",
            "Read one Card in this caller's tenant by kind and Card UID. \
             card.status.verification carries a Verifier's PSI/SPC baseline status and an owner \
             Card's derived binding_ids. card.relationships.outbound_refs and inbound_refs \
             contain exact related identities in each edge's ref field. Follow them with \
             cards.get_by_ref. Requires cards:read.",
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
        tool::<GetCardByRefArgs, GetCardResponse>(
            GET_BY_REF,
            "Read a Card by exact reference",
            "Read one exact versioned CardRef, including space, in this caller's tenant. \
             card.relationships.outbound_refs and inbound_refs contain exact related identities \
             in each edge's ref field; follow them with repeated reads. Requires cards:read.",
            true,
        ),
    ]
}

impl WyrdMcpHandler {
    /// Read one Card through the existing audited kind-qualified UID operation.
    ///
    /// # Errors
    /// Returns invalid arguments, permission denials, or registry read errors.
    pub(super) async fn mcp_get_card(
        &self,
        caller: Caller,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetCardArgs = parse_args(arguments, GET)?;
        structured(&get_card_for(&self.state, &caller, &args.kind, &args.card_uid).await?)
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

    /// Resolve an exact Card identity through the shared audited read operation.
    ///
    /// # Errors
    /// Returns invalid arguments, permission denials, or registry read errors.
    pub(super) async fn mcp_get_card_by_ref(
        &self,
        caller: Caller,
        arguments: Option<Map<String, Value>>,
    ) -> Result<CallToolResult, WyrdError> {
        let args: GetCardByRefArgs = parse_args(arguments, GET_BY_REF)?;
        structured(&get_card_by_ref_for(&self.state, &caller, &args.card_ref).await?)
    }
}
