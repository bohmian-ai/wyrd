use iceberg::NamespaceIdent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BifrostNamespace {
    System,
    Bifrost,
    Traces,
    Eval,
}

impl BifrostNamespace {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "vala.system",
            Self::Bifrost => "vala.bifrost",
            Self::Traces => "vala.traces",
            Self::Eval => "vala.eval",
        }
    }

    pub fn to_namespace_ident(self) -> NamespaceIdent {
        NamespaceIdent::from_strs([self.as_str()])
            .expect("namespace identifier is always valid")
    }
}
