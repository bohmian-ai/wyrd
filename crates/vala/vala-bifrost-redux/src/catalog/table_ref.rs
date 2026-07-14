//! Table reference type — (namespace, `table_name`) pair.

use crate::namespaces::BifrostNamespace;

/// A reference to a Bifrost table: namespace + table name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableRef {
    pub namespace: BifrostNamespace,
    pub table_name: String,
}

impl TableRef {
    /// Construct a new `TableRef`.
    pub fn new(namespace: BifrostNamespace, table_name: impl Into<String>) -> Self {
        Self {
            namespace,
            table_name: table_name.into(),
        }
    }

    /// Fully-qualified name: `"vala.namespace.table"`.
    pub fn fqn(&self) -> String {
        format!("{}.{}", self.namespace.as_str(), self.table_name)
    }

    /// Parse a fully-qualified name into `(namespace, table_name)`.
    pub fn from_fqn(fqn: &str) -> Option<Self> {
        BifrostNamespace::split_fqn(fqn)
            .map(|(namespace, table_name)| Self::new(namespace, table_name))
    }
}

impl std::fmt::Display for TableRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fqn())
    }
}
