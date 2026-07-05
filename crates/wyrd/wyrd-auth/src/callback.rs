//! Domain helpers for the human OIDC callback flow.

use serde_json::Value;
use wyrd_auth_oidc::TrustedIssuer;
use wyrd_runtime::RoleRef;
use wyrd_spec::error::WyrdError;

use crate::exchange_api_key::role_refs;
use crate::login::LoginStateEntry;

/// Verify the OIDC nonce bound to the login state.
pub fn verify_nonce(state: &LoginStateEntry, claims: &Value) -> Result<(), WyrdError> {
    let Some(nonce) = claims.get("nonce").and_then(Value::as_str) else {
        return Err(invalid_nonce("id token nonce is missing"));
    };
    if nonce != state.nonce {
        return Err(invalid_nonce("id token nonce mismatch"));
    }
    Ok(())
}

/// Map trusted external groups plus issuer defaults to local Wyrd role refs.
pub fn role_names_to_refs(
    trusted: &TrustedIssuer,
    groups: &[String],
) -> Result<Vec<RoleRef>, WyrdError> {
    let mut names = trusted.default_roles.clone();
    for group in groups {
        if let Some(mapped) = trusted.group_role_map.get(group) {
            names.extend(mapped.iter().cloned());
        }
    }
    names.sort_unstable();
    names.dedup();
    role_refs(names).map_err(|_| WyrdError::Internal {
        message: "trusted issuer role mapping is invalid".to_owned(),
        details: serde_json::json!({}),
    })
}

fn invalid_nonce(message: &str) -> WyrdError {
    WyrdError::InvalidNonce {
        message: message.to_owned(),
        details: serde_json::json!({}),
    }
}
