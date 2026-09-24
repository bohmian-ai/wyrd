//! Redux Bifrost namespace enum, including caller-owned datasets.

use iceberg::NamespaceIdent;

/// The closed set of `vala.*` namespaces the Bifrost catalog owns.
///
/// Every built-in table and every caller-owned dataset lives in exactly one of
/// these; the catalog, query admission, and FQN parsing all resolve names
/// through this enum rather than matching namespace strings ad hoc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BifrostNamespace {
    /// `vala.system` — retained audit history.
    Audit,
    /// `vala.bifrost` — Bifrost's own operational tables.
    Bifrost,
    /// `vala.traces` — OpenTelemetry spans.
    Traces,
    /// `vala.metrics` — OpenTelemetry metric points.
    Metrics,
    /// `vala.logs` — OpenTelemetry log records.
    Logs,
    /// `vala.eval` — Eval observations and per-task result items.
    Eval,
    /// `vala.drift` — Drift observations and per-feature result details.
    Drift,
    /// `vala.verification` — the shared per-run Verifier verdict table.
    Verification,
    /// `vala.dev` — development-time agent traces.
    Dev,
    /// `vala.datasets` — caller-owned dynamic tables.
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
        Self::Eval,
        Self::Drift,
        Self::Verification,
        Self::Dev,
        Self::Datasets,
    ];

    /// The wire namespace string (e.g. `"vala.eval"`) every FQN is built from.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Audit => "vala.system",
            Self::Bifrost => "vala.bifrost",
            Self::Traces => "vala.traces",
            Self::Metrics => "vala.metrics",
            Self::Logs => "vala.logs",
            Self::Eval => "vala.eval",
            Self::Drift => "vala.drift",
            Self::Verification => "vala.verification",
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
            "eval" => Some(Self::Eval),
            "drift" => Some(Self::Drift),
            "verification" => Some(Self::Verification),
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

    /// Forge keeps its own namespace allowlist (in Rust and in the Postgres
    /// CHECKs on its task tables); a Bifrost namespace missing from it has
    /// every planning hint refused and is never maintained.
    ///
    /// # Panics
    /// Panics when Forge refuses a known Bifrost namespace.
    #[test]
    fn forge_accepts_every_bifrost_namespace() {
        for ns in BifrostNamespace::ALL {
            assert!(
                vala_sql::row_types::forge_tasks::ForgeTaskTableIdentity::new(
                    crate::catalog::BIFROST_CATALOG_NAME,
                    ns.as_str(),
                    "events",
                )
                .is_ok(),
                "Forge refuses {ns:?}"
            );
        }
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
