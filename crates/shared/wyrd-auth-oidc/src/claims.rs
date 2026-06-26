//! Claim extraction and mapping.

use crate::error::OidcError;
use crate::registry::{ClaimMapping, ClaimPath};

/// Normalized claims extracted from a verified OIDC token.
///
/// # F04 — no `card_ref`
///
/// `MappedClaims` carries the external subject identity only. It deliberately
/// has no `card_ref` field: a workload that can influence its `sub` claim, k8s
/// namespace/SA name, SPIFFE ID, or any custom claim must not be able to assert
/// another card's roles. The Wyrd `CardRef` is resolved server-side via
/// [`crate::registry::WorkloadBindingResolver`] using the verified `subject`
/// as the lookup key.
///
/// This type is also not a Wyrd `Principal` (R03 rule). The server creates the
/// principal after resolving RBAC and (for workloads) the card binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedClaims {
    /// The verified external subject identity (from the `sub` claim or a
    /// configured claim path).
    pub subject: String,
    /// Email address if the issuer provides it and a path is configured.
    pub email: Option<String>,
    /// Groups or roles extracted from the token (input to RBAC resolution).
    pub groups: Vec<String>,
}

/// Extract and normalize claims from a verified token payload.
///
/// Applies the [`ClaimMapping`] to the raw JSON claims object. The `subject`
/// claim is required; `email` and `groups` are optional.
///
/// # Errors
/// Returns [`OidcError::ClaimMissing`] when the `subject` path resolves to
/// nothing or is not a string.
pub fn map_claims(
    mapping: &ClaimMapping,
    claims: &serde_json::Value,
) -> Result<MappedClaims, OidcError> {
    let subject =
        extract_string(claims, &mapping.subject)?.ok_or_else(|| OidcError::ClaimMissing {
            path: mapping.subject.as_str().to_owned(),
        })?;

    let email = mapping
        .email
        .as_ref()
        .and_then(|path| extract_string(claims, path).ok().flatten());

    let groups = mapping
        .groups
        .as_ref()
        .and_then(|path| extract_string_array(claims, path))
        .unwrap_or_default();

    Ok(MappedClaims {
        subject,
        email,
        groups,
    })
}

/// Navigate a dotted path in a JSON value, returning the value at the leaf.
fn get_at_path<'a>(
    value: &'a serde_json::Value,
    path: &ClaimPath,
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.as_str().split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Extract a `String` from a dotted path. Returns `Ok(None)` when the path
/// is absent (not an error); returns `Ok(Some(...))` for a string value;
/// returns `Ok(None)` for a non-string value (silently skipped).
fn extract_string(
    claims: &serde_json::Value,
    path: &ClaimPath,
) -> Result<Option<String>, OidcError> {
    Ok(get_at_path(claims, path)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

/// Extract a `Vec<String>` from a dotted path that points to an array of
/// strings. Non-string array elements are silently skipped. Returns `None`
/// when the path is absent or does not point to an array.
fn extract_string_array(claims: &serde_json::Value, path: &ClaimPath) -> Option<Vec<String>> {
    let arr = get_at_path(claims, path)?.as_array()?;
    Some(
        arr.iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::registry::{ClaimMapping, ClaimPath};

    fn standard_mapping() -> ClaimMapping {
        ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: Some(ClaimPath::new("email")),
            groups: Some(ClaimPath::new("groups")),
        }
    }

    #[test]
    fn maps_standard_claims() {
        let claims = json!({
            "sub": "user-123",
            "email": "user@example.com",
            "groups": ["admin", "viewer"]
        });
        let mapped = map_claims(&standard_mapping(), &claims).expect("mapping should succeed");

        assert_eq!(mapped.subject, "user-123");
        assert_eq!(mapped.email.as_deref(), Some("user@example.com"));
        assert_eq!(mapped.groups, vec!["admin", "viewer"]);
    }

    #[test]
    fn missing_optional_email_is_not_an_error() {
        let claims = json!({ "sub": "user-123", "groups": [] });
        let mapped = map_claims(&standard_mapping(), &claims).expect("email absence is ok");

        assert_eq!(mapped.subject, "user-123");
        assert!(mapped.email.is_none(), "email should be None when absent");
    }

    #[test]
    fn dotted_path_traverses_nested_objects() {
        let mapping = ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: Some(ClaimPath::new("realm_access.roles")),
        };
        let claims = json!({
            "sub": "svc-acct",
            "realm_access": {
                "roles": ["offline_access", "uma_authorization"]
            }
        });
        let mapped = map_claims(&mapping, &claims).expect("dotted path should resolve");

        assert_eq!(mapped.groups, vec!["offline_access", "uma_authorization"]);
    }

    #[test]
    fn missing_subject_claim_returns_error() {
        let claims = json!({ "email": "x@x.com" });
        let err =
            map_claims(&standard_mapping(), &claims).expect_err("missing sub should be an error");

        assert!(
            matches!(&err, OidcError::ClaimMissing { path } if path == "sub"),
            "expected ClaimMissing for sub, got {err:?}"
        );
    }

    #[test]
    fn absent_groups_path_yields_empty_vec() {
        let mapping = ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: Some(ClaimPath::new("no.such.path")),
        };
        let claims = json!({ "sub": "u1" });
        let mapped = map_claims(&mapping, &claims).expect("absent optional groups is ok");

        assert!(mapped.groups.is_empty());
    }

    #[test]
    fn no_groups_mapping_yields_empty_vec() {
        let mapping = ClaimMapping {
            subject: ClaimPath::new("sub"),
            email: None,
            groups: None,
        };
        let claims = json!({ "sub": "u1", "roles": ["admin"] });
        let mapped = map_claims(&mapping, &claims).expect("no groups mapping is ok");

        assert!(mapped.groups.is_empty(), "groups should default to empty");
    }

    #[test]
    fn mapped_claims_has_no_card_ref_field() {
        // F04 regression guard: exhaustive construction to assert the field set
        // cannot grow to include card_ref without breaking this test.
        let mc = MappedClaims {
            subject: "u".to_owned(),
            email: None,
            groups: vec![],
        };
        let MappedClaims {
            subject,
            email,
            groups,
        } = mc;
        assert_eq!(subject, "u");
        assert!(email.is_none());
        assert!(groups.is_empty());
    }

    #[test]
    fn mapped_claims_is_not_a_principal() {
        // R03 regression guard: MappedClaims must not carry principal-level
        // fields (id, tenant_id, kind, permissions). Assert it is purely
        // external claim data by verifying its type and field names.
        fn assert_only_external_fields(_: &MappedClaims) {}
        let mc = MappedClaims {
            subject: "x".to_owned(),
            email: None,
            groups: vec![],
        };
        assert_only_external_fields(&mc);
    }
}
