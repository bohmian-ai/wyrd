//! Single server-side resolver for a principal's card scope.
//!
//! Every service/agent token-mint path resolves the minted principal's card
//! scope through [`resolve_card_scope`] so no site can forget it. Resolution is
//! a control-plane read that runs once at issue time; the scope then rides the
//! signed token as a claim (see `wyrd-auth-issue`).

use wyrd_runtime::CardScope;
use wyrd_spec::envelope::Spec;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::cards::get_card_by_ref;

/// Resolve the set of cards the minted principal may tag data with.
///
/// - **Service**: the Service card's `components` refs unioned with the Service
///   card itself (so a componentless or standalone Service can tag itself).
/// - **Agent**: exactly the agent's own bound card.
/// - **User** (or any other kind): the empty scope — Users cannot tag writes.
///
/// Fails closed: a Service card that cannot be loaded or is not a Service spec
/// returns an error so the caller refuses to sign a token with an unknown scope.
///
/// # Errors
/// Returns the registry [`WyrdError`] from [`get_card_by_ref`] when the Service
/// card cannot be loaded, or an invalid-spec error when the loaded card is not
/// a Service card.
pub(crate) async fn resolve_card_scope(
    conn: &mut TenantConn<'_>,
    principal_kind: &str,
    card_ref: &CardRef,
) -> Result<CardScope, WyrdError> {
    match principal_kind {
        "service" => {
            let parsed = get_card_by_ref(
                conn,
                card_ref.kind.clone(),
                &card_ref.space,
                &card_ref.name,
                &card_ref.version,
            )
            .await?;
            let Spec::Service(service) = parsed.spec else {
                return Err(WyrdError::registry_invalid_card_spec(format!(
                    "card {card_ref} is not a Service card"
                )));
            };
            let mut cards: Vec<CardRef> = service
                .components
                .into_iter()
                .map(|component| component.card_ref)
                .collect();
            cards.push(card_ref.clone());
            Ok(CardScope::new(cards))
        }
        "agent" => Ok(CardScope::new([card_ref.clone()])),
        _ => Ok(CardScope::default()),
    }
}
