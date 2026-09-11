//! Typed object half of one Wyrd operation/object RBAC grant.
//!
//! Wyrd RBAC is `(resource, action, scope)`. This module owns the `scope` axis:
//! the closed, domain-tagged union naming *which objects* a grant reaches. It
//! lives in the contract layer because the shape is persisted in
//! `wyrd.auth_roles.permissions`, projected into generated schemas, and read by
//! clients; the runtime permission model in `wyrd-runtime` consumes it rather
//! than redefining it.
//!
//! Scope is required on every permission. A grant that targets no object uses
//! [`PermissionScope::All`], which covers every object of the permission's own
//! resource and action and never widens either of those axes.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Longest accepted catalog or schema identifier segment.
const MAX_IDENTIFIER_LEN: usize = 63;

/// Why one permission scope is not a valid object identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PermissionScopeError {
    /// A catalog or schema segment is empty, over-long, or carries characters
    /// that are not valid in a Bifrost namespace segment.
    #[error("permission scope {field} is not a valid identifier: {value:?}")]
    InvalidIdentifier {
        /// Which component of the scope failed validation.
        field: &'static str,
        /// The rejected value, retained so the operator can see what was stored.
        value: String,
    },
}

/// Objects one permission grant reaches.
///
/// Domain-tagged rather than a shared string object language: a future Wyrd
/// domain that needs object scope adds its own typed variant instead of
/// overloading someone else's identity spelling.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum PermissionScope {
    /// Every object of the covered resource and action.
    All,
    /// One Bifrost schema or table.
    Bifrost(BifrostPermissionScope),
}

/// The Bifrost objects one grant reaches.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BifrostPermissionScope {
    /// Every table currently or later resolved beneath one catalog/schema pair.
    Schema(BifrostSchemaScope),
    /// Exactly one registered table, identified by its stable server-managed UID.
    Table(BifrostTableScope),
}

/// One canonical Bifrost catalog and schema pair.
///
/// These are the *logical* names a caller writes in SQL — catalog `vala`,
/// schema `logs` — not the flattened internal namespace (`vala.logs`) and not
/// the physical Iceberg catalog name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BifrostSchemaScope {
    /// Logical catalog, `vala` for every table Bifrost serves today.
    pub catalog: String,
    /// Logical schema beneath the catalog, such as `logs` or `traces`.
    pub schema: String,
}

/// One exact registered Bifrost table.
///
/// The UID is the authority: it is assigned at registration and never reused,
/// so a grant does not follow a dropped-and-recreated table of the same name.
/// The catalog and schema ride alongside it so the synchronous checker can
/// evaluate schema containment without a catalog lookup.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(deny_unknown_fields)]
pub struct BifrostTableScope {
    /// Logical catalog the table resolves under.
    pub catalog: String,
    /// Logical schema the table resolves under.
    pub schema: String,
    /// Stable server-managed table UID.
    pub table_uid: Uuid,
}

impl PermissionScope {
    /// True when this scope's objects include every object `required` names.
    ///
    /// [`Self::All`] covers everything; a Bifrost scope never covers `All`,
    /// because "all objects" is strictly wider than any single schema or table.
    #[must_use]
    pub fn covers(&self, required: &Self) -> bool {
        match (self, required) {
            (Self::All, _) => true,
            (Self::Bifrost(granted), Self::Bifrost(required)) => granted.covers(required),
            (Self::Bifrost(_), Self::All) => false,
        }
    }

    /// True when this scope names a Bifrost object rather than every object.
    #[must_use]
    pub const fn is_bifrost(&self) -> bool {
        matches!(self, Self::Bifrost(_))
    }

    /// Rejects a scope whose object identity is malformed.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionScopeError::InvalidIdentifier`] when a catalog or
    /// schema segment is empty, longer than 63 characters, or contains anything
    /// other than ASCII alphanumerics, `_`, or `-`.
    pub fn validate(&self) -> Result<(), PermissionScopeError> {
        match self {
            Self::All => Ok(()),
            Self::Bifrost(scope) => scope.validate(),
        }
    }
}

impl BifrostPermissionScope {
    /// True when this Bifrost scope's objects include every object `required` names.
    #[must_use]
    pub fn covers(&self, required: &Self) -> bool {
        match (self, required) {
            (Self::Schema(granted), Self::Schema(required)) => granted == required,
            (Self::Schema(granted), Self::Table(required)) => {
                granted.catalog == required.catalog && granted.schema == required.schema
            }
            (Self::Table(granted), Self::Table(required)) => granted == required,
            (Self::Table(_), Self::Schema(_)) => false,
        }
    }

    /// Rejects a Bifrost scope whose catalog or schema identity is malformed.
    ///
    /// # Errors
    ///
    /// Returns [`PermissionScopeError::InvalidIdentifier`] for an empty,
    /// over-long, or non-identifier catalog or schema segment.
    pub fn validate(&self) -> Result<(), PermissionScopeError> {
        let (catalog, schema) = match self {
            Self::Schema(scope) => (&scope.catalog, &scope.schema),
            Self::Table(scope) => (&scope.catalog, &scope.schema),
        };
        validate_identifier("catalog", catalog)?;
        validate_identifier("schema", schema)
    }

    /// Borrows the logical catalog this scope names.
    #[must_use]
    pub fn catalog(&self) -> &str {
        match self {
            Self::Schema(scope) => &scope.catalog,
            Self::Table(scope) => &scope.catalog,
        }
    }

    /// Borrows the logical schema this scope names.
    #[must_use]
    pub fn schema(&self) -> &str {
        match self {
            Self::Schema(scope) => &scope.schema,
            Self::Table(scope) => &scope.schema,
        }
    }
}

/// Rejects a catalog or schema segment that is not a safe namespace identifier.
///
/// # Errors
///
/// Returns [`PermissionScopeError::InvalidIdentifier`] naming `field` when
/// `value` is empty, longer than 63 characters, or carries a character outside
/// ASCII alphanumerics, `_`, and `-`.
fn validate_identifier(field: &'static str, value: &str) -> Result<(), PermissionScopeError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_IDENTIFIER_LEN
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        });
    if valid {
        Ok(())
    } else {
        Err(PermissionScopeError::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BifrostPermissionScope, BifrostSchemaScope, BifrostTableScope, PermissionScope,
        PermissionScopeError,
    };
    use uuid::Uuid;

    /// Builds one table scope under `vala` for the supplied schema and UID.
    fn table(schema: &str, uid: Uuid) -> PermissionScope {
        PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
            catalog: "vala".to_owned(),
            schema: schema.to_owned(),
            table_uid: uid,
        }))
    }

    /// Builds one schema scope under `vala` for the supplied schema.
    fn schema(schema: &str) -> PermissionScope {
        PermissionScope::Bifrost(BifrostPermissionScope::Schema(BifrostSchemaScope {
            catalog: "vala".to_owned(),
            schema: schema.to_owned(),
        }))
    }

    /// Proves `All` is the top of the object lattice: it covers every scope and
    /// no narrower scope can ever satisfy an object-wide requirement.
    #[test]
    fn all_covers_every_object_and_is_covered_by_nothing_narrower() {
        let uid = Uuid::from_u128(1);

        assert!(PermissionScope::All.covers(&PermissionScope::All));
        assert!(PermissionScope::All.covers(&schema("logs")));
        assert!(PermissionScope::All.covers(&table("logs", uid)));
        assert!(!schema("logs").covers(&PermissionScope::All));
        assert!(!table("logs", uid).covers(&PermissionScope::All));
    }

    /// Proves a schema grant reaches current and future tables beneath exactly
    /// that catalog/schema pair, and never another schema.
    #[test]
    fn schema_scope_covers_every_table_in_that_exact_schema() {
        let one = Uuid::from_u128(1);
        let two = Uuid::from_u128(2);

        assert!(schema("logs").covers(&table("logs", one)));
        assert!(schema("logs").covers(&table("logs", two)));
        assert!(!schema("logs").covers(&table("traces", one)));
        assert!(!schema("logs").covers(&schema("traces")));
    }

    /// Proves a table grant follows the stable UID alone: it covers no sibling
    /// table and never widens back to its own schema.
    #[test]
    fn table_scope_covers_only_the_exact_uid() {
        let one = Uuid::from_u128(1);
        let two = Uuid::from_u128(2);

        assert!(table("traces", one).covers(&table("traces", one)));
        assert!(!table("traces", one).covers(&table("traces", two)));
        assert!(!table("traces", one).covers(&schema("traces")));
    }

    /// Proves the persisted scope JSON is exactly the approved domain-tagged
    /// projection, so the wire form cannot drift from the spec.
    #[test]
    fn scope_json_matches_the_approved_projection() {
        let uid = Uuid::from_u128(0x1234);

        assert_eq!(
            serde_json::to_value(PermissionScope::All).expect("all serializes"),
            serde_json::json!("all")
        );
        assert_eq!(
            serde_json::to_value(schema("logs")).expect("schema scope serializes"),
            serde_json::json!({"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}})
        );
        assert_eq!(
            serde_json::to_value(table("traces", uid)).expect("table scope serializes"),
            serde_json::json!({
                "bifrost": {"table": {
                    "catalog": "vala",
                    "schema": "traces",
                    "table_uid": uid.to_string(),
                }}
            })
        );
    }

    /// Proves object identities are validated, not merely parsed: an empty or
    /// over-long segment and a flattened namespace are all refused.
    #[test]
    fn malformed_identities_are_rejected() {
        assert!(PermissionScope::All.validate().is_ok());
        assert!(schema("logs").validate().is_ok());
        assert_eq!(
            schema("").validate(),
            Err(PermissionScopeError::InvalidIdentifier {
                field: "schema",
                value: String::new(),
            })
        );
        assert!(
            PermissionScope::Bifrost(BifrostPermissionScope::Schema(BifrostSchemaScope {
                catalog: "vala.logs".to_owned(),
                schema: "logs".to_owned(),
            }))
            .validate()
            .is_err(),
            "a flattened namespace is not a catalog identifier"
        );
        assert!(
            schema(&"x".repeat(64)).validate().is_err(),
            "an over-long schema segment is rejected"
        );
    }

    /// Proves a non-UUID table identity fails at decode, so no unresolvable
    /// object identity can reach the synchronous checker.
    #[test]
    fn malformed_table_identity_is_rejected_at_decode() {
        let error = serde_json::from_value::<PermissionScope>(serde_json::json!({
            "bifrost": {"table": {"catalog": "vala", "schema": "traces", "table_uid": "not-a-uid"}}
        }))
        .expect_err("a non-UUID table identity does not decode");

        assert!(error.to_string().contains("UUID") || error.to_string().contains("uuid"));
    }
}
