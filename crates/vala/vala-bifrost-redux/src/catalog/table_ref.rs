//! Table reference type — `(BifrostNamespace, name)` pair per CONTRACTS §13.
//!
//! One strongly-typed identifier that flows end-to-end: `SealKey`, WAL records,
//! memtable buckets, manifest entries, Parquet paths, and audit records all
//! carry this shape. The namespace is a closed enum (see `namespaces.rs`) and
//! the table name is validated against `is_safe_name` before use in
//! object-store paths (guarded via `parse_fqn`; `new` stays infallible for
//! known-safe literals in tests and internal call sites).

use crate::namespaces::BifrostNamespace;

/// Reject strings unsafe for object-store path segments.
///
/// Allows `[A-Za-z0-9_.-]`, max 63 chars. Rejects empty, `..`, `/`, `\`, and
/// leading/trailing `.` — blocks path traversal on the local filesystem store.
#[must_use]
pub fn is_safe_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 63 {
        return false;
    }
    if s.contains("..") || s.contains('/') || s.contains('\\') {
        return false;
    }
    if s.starts_with('.') || s.ends_with('.') {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// A reference to a Bifrost table: namespace + name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    /// Namespace (closed enum — see `BifrostNamespace`).
    pub namespace: BifrostNamespace,
    /// Table name within the namespace.
    pub name: String,
}

impl TableRef {
    /// Construct a `TableRef` without validating `name`.
    ///
    /// Use `parse_fqn` when the input is user-supplied. This constructor is
    /// infallible for known-safe literals in tests and internal callers.
    pub fn new(namespace: BifrostNamespace, name: impl Into<String>) -> Self {
        Self {
            namespace,
            name: name.into(),
        }
    }

    /// Fully-qualified name: `"bifrost.<ns>.<name>"`.
    #[must_use]
    pub fn fqn(&self) -> String {
        format!("{}.{}", self.namespace.as_str(), self.name)
    }

    /// The Bifrost table object scope naming this table's registered identity.
    ///
    /// The namespace splits into the scope's catalog and schema (`bifrost` and
    /// `traces` for `bifrost.traces`), and the UID pins the exact registration, so
    /// one scope is shared by Oracle query reads and Gate record writes.
    #[must_use]
    pub fn permission_scope(
        &self,
        table_uid: &crate::catalog::TableUid,
    ) -> wyrd_runtime::PermissionScope {
        let (catalog, schema) = self
            .namespace
            .as_str()
            .split_once('.')
            .unwrap_or(("bifrost", self.namespace.as_str()));
        wyrd_runtime::PermissionScope::Bifrost(wyrd_runtime::BifrostPermissionScope::Table(
            wyrd_runtime::BifrostTableScope {
                catalog: catalog.to_owned(),
                schema: schema.to_owned(),
                table_uid: uuid::Uuid::from_bytes(*table_uid.as_bytes()),
            },
        ))
    }

    /// Parse a fully-qualified name into a `TableRef`.
    ///
    /// Returns `None` if the namespace is not a known `BifrostNamespace` or if
    /// the table name fails `is_safe_name`.
    #[must_use]
    pub fn parse_fqn(fqn: &str) -> Option<Self> {
        let (namespace, name) = BifrostNamespace::split_fqn(fqn)?;
        if !is_safe_name(&name) {
            return None;
        }
        Some(Self::new(namespace, name))
    }
}

impl std::fmt::Display for TableRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fqn())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A constructed reference renders its fully qualified `bifrost.<schema>.<name>`
    /// name and keeps the bare table name.
    ///
    /// # Panics
    ///
    /// Panics when the rendered FQN or the stored name differs from the input.
    #[test]
    fn new_and_fqn_round_trip() {
        let t = TableRef::new(BifrostNamespace::Bifrost, "events");
        assert_eq!(t.fqn(), "bifrost.bifrost.events");
        assert_eq!(t.name, "events");
    }

    /// A known namespace FQN parses into its namespace and bare table name.
    ///
    /// # Panics
    ///
    /// Panics when `bifrost.bifrost.events` fails to parse or decodes to another
    /// namespace or name.
    #[test]
    fn parse_fqn_accepts_known_namespace() {
        let t = TableRef::parse_fqn("bifrost.bifrost.events").expect("known ns");
        assert_eq!(t.namespace, BifrostNamespace::Bifrost);
        assert_eq!(t.name, "events");
    }

    #[test]
    fn parse_fqn_rejects_unknown_namespace() {
        assert!(TableRef::parse_fqn("unknown.foo").is_none());
        assert!(TableRef::parse_fqn("no_dot").is_none());
    }

    /// Every namespace renders, parses, and scopes under the one `bifrost`
    /// catalog, and the retired `vala` spelling resolves nothing.
    ///
    /// # Panics
    ///
    /// Panics when an FQN lacks the `bifrost.` prefix or does not round-trip, a
    /// `vala.` spelling parses, or the permission scope is not a table scope under
    /// the `bifrost` catalog and the namespace's schema.
    #[test]
    fn canonical_catalog_has_no_alias() {
        let uid = crate::catalog::TableUid::from_bytes([7; 16]);
        for namespace in BifrostNamespace::ALL {
            let table = TableRef::new(namespace, "events");
            let fqn = table.fqn();
            assert!(fqn.starts_with("bifrost."), "{fqn}");
            assert_eq!(TableRef::parse_fqn(&fqn), Some(table.clone()));
            let schema = namespace
                .as_str()
                .strip_prefix("bifrost.")
                .expect("bifrost root");
            assert!(TableRef::parse_fqn(&format!("vala.{schema}.events")).is_none());
            let wyrd_runtime::PermissionScope::Bifrost(
                wyrd_runtime::BifrostPermissionScope::Table(scope),
            ) = table.permission_scope(&uid)
            else {
                panic!("a table scope");
            };
            assert_eq!(
                (scope.catalog.as_str(), scope.schema.as_str()),
                ("bifrost", schema)
            );
        }
    }

    /// Names carrying `..` or a leading `/` never parse, so a table reference cannot
    /// escape its namespace's storage path.
    ///
    /// # Panics
    ///
    /// Panics when either traversal FQN parses.
    #[test]
    fn parse_fqn_rejects_path_traversal() {
        assert!(TableRef::parse_fqn("bifrost.bifrost.../etc/passwd").is_none());
        assert!(TableRef::parse_fqn("bifrost.bifrost./absolute").is_none());
    }

    #[test]
    fn is_safe_name_boundaries() {
        assert!(is_safe_name("events"));
        assert!(is_safe_name("events_v2"));
        assert!(is_safe_name("events-2026"));
        assert!(!is_safe_name(""));
        assert!(!is_safe_name(".hidden"));
        assert!(!is_safe_name("hidden."));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name("a\\b"));
        assert!(!is_safe_name("a..b"));
        assert!(!is_safe_name(&"x".repeat(64)));
    }
}
