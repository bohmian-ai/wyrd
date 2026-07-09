use iceberg::NamespaceIdent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostNamespace {
    System,
    Bifrost,
    Traces,
    Metrics,
    Logs,
    GenAi,
    Eval,
    Drift,
    Dev,
}

impl BifrostNamespace {
    /// All known namespaces. Adding a variant here causes a compile error at every
    /// `match` that is missing a branch — the exhaustiveness guard.
    pub const ALL: [Self; 9] = [
        Self::System,
        Self::Bifrost,
        Self::Traces,
        Self::Metrics,
        Self::Logs,
        Self::GenAi,
        Self::Eval,
        Self::Drift,
        Self::Dev,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "vala.system",
            Self::Bifrost => "vala.bifrost",
            Self::Traces => "vala.traces",
            Self::Metrics => "vala.metrics",
            Self::Logs => "vala.logs",
            Self::GenAi => "vala.genai",
            Self::Eval => "vala.eval",
            Self::Drift => "vala.drift",
            Self::Dev => "vala.dev",
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
            "system" => Some(Self::System),
            "bifrost" => Some(Self::Bifrost),
            "traces" => Some(Self::Traces),
            "metrics" => Some(Self::Metrics),
            "logs" => Some(Self::Logs),
            "genai" => Some(Self::GenAi),
            "eval" => Some(Self::Eval),
            "drift" => Some(Self::Drift),
            "dev" => Some(Self::Dev),
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
}
