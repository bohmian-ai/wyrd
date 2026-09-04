//! Redux Bifrost namespace enum, including caller-owned datasets.

use iceberg::NamespaceIdent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BifrostNamespace {
    Audit,
    Bifrost,
    Traces,
    Metrics,
    Logs,
    GenAi,
    Eval,
    Drift,
    Dev,
    Datasets, // Redux-only variant for dynamic table namespace
}

impl BifrostNamespace {
    /// All known namespaces. Adding a variant here causes a compile error at every
    /// `match` that is missing a branch — the exhaustiveness guard.
    pub const ALL: [Self; 10] = [
        Self::Audit,
        Self::Bifrost,
        Self::Traces,
        Self::Metrics,
        Self::Logs,
        Self::GenAi,
        Self::Eval,
        Self::Drift,
        Self::Dev,
        Self::Datasets,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audit => "vala.system",
            Self::Bifrost => "vala.bifrost",
            Self::Traces => "vala.traces",
            Self::Metrics => "vala.metrics",
            Self::Logs => "vala.logs",
            Self::GenAi => "vala.genai",
            Self::Eval => "vala.eval",
            Self::Drift => "vala.drift",
            Self::Dev => "vala.dev",
            Self::Datasets => "vala.datasets",
        }
    }

    /// Parse a wire namespace string (e.g. `"vala.bifrost"`) into the enum.
    /// Returns `None` for unknown namespaces.
    pub fn from_wire(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|ns| ns.as_str() == s)
    }

    /// Map a [`DomainTable::NAMESPACE`] short segment (e.g. `"traces"`) to the enum.
    /// Returns `None` for unknown segments.
    pub fn from_domain_namespace(segment: &str) -> Option<Self> {
        match segment {
            "system" => Some(Self::Audit),
            "bifrost" => Some(Self::Bifrost),
            "traces" => Some(Self::Traces),
            "metrics" => Some(Self::Metrics),
            "logs" => Some(Self::Logs),
            "genai" => Some(Self::GenAi),
            "eval" => Some(Self::Eval),
            "drift" => Some(Self::Drift),
            "dev" => Some(Self::Dev),
            "datasets" => Some(Self::Datasets),
            _ => None,
        }
    }

    /// Split a fully-qualified `"namespace.table"` FQN into `(Self, table_name)`.
    ///
    /// Returns `None` for names that are not a known Bifrost namespace prefix followed
    /// by a single-segment table (CTEs, aliases, three-part names, unknown namespaces).
    pub fn split_fqn(fqn: &str) -> Option<(Self, String)> {
        for ns in Self::ALL {
            let ns_str = ns.as_str();
            let prefix_len = ns_str.len();
            if fqn.len() > prefix_len + 1
                && fqn.starts_with(ns_str)
                && fqn.as_bytes()[prefix_len] == b'.'
            {
                let name = &fqn[prefix_len + 1..];
                if !name.is_empty() && !name.contains('.') {
                    return Some((ns, name.to_owned()));
                }
            }
        }
        None
    }

    /// # Panics
    /// Never panics — all variant strings are valid Iceberg namespace identifiers.
    pub fn to_namespace_ident(self) -> NamespaceIdent {
        NamespaceIdent::from_strs([self.as_str()]).expect("namespace identifier is always valid")
    }
}

impl std::fmt::Display for BifrostNamespace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Joins a table binding's namespace and table into the canonical Wyrd FQN.
///
/// A wire `TenantTableBinding` carries its namespace either with the `vala.`
/// root (`vala.bifrost`) or with that root already stripped
/// (`TailFenceDrainer::wire_binding`), so every consumer that has to name the
/// catalog's closed namespace set normalizes through this one function rather
/// than guessing which producer built the binding it holds.
pub(crate) fn canonical_table_name(namespace: &str, table: &str) -> String {
    if namespace.starts_with("vala.") {
        format!("{namespace}.{table}")
    } else {
        format!("vala.{namespace}.{table}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_domain_namespace_round_trips_all_known() {
        for ns in BifrostNamespace::ALL {
            let segment = ns
                .as_str()
                .split('.')
                .next_back()
                .expect("namespace has dot");
            assert_eq!(
                BifrostNamespace::from_domain_namespace(segment),
                Some(ns),
                "from_domain_namespace(\"{segment}\") must round-trip to {ns:?}"
            );
        }
    }

    #[test]
    fn from_domain_namespace_unknown_returns_none() {
        assert!(BifrostNamespace::from_domain_namespace("unknown_namespace").is_none());
    }

    #[test]
    fn bifrost_namespace_datasets_variant_round_trips() {
        let datasets = BifrostNamespace::Datasets;
        assert_eq!(datasets.as_str(), "vala.datasets");
        assert_eq!(BifrostNamespace::from_wire("vala.datasets"), Some(datasets));
        assert_eq!(
            BifrostNamespace::from_domain_namespace("datasets"),
            Some(datasets)
        );
    }
}
