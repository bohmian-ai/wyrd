//! The named Card types other than Verifier, and the Service component and
//! verification binding values.

use std::path::{Path, PathBuf};

use serde_json::Value;
use wyrd_loader::Diagnostic;
use wyrd_spec::envelope::{CardKind, Spec};

use super::{
    ArtifactFile, Body, Captures, Card, CardDefinition, CardRef, FieldValue, LocalArtifact,
    Verifier, card_methods, fields, selection,
};

/// Generate a named Card type: its struct, `new`, the shared Card methods,
/// and its conversions into a [`CardDefinition`] and a sibling reference.
macro_rules! card_kind {
    ($(#[$doc:meta])* $ty:ident => $kind:ident $(, flatten = $flatten:literal)?) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $ty {
            /// The Card under construction.
            card: Card,
        }

        impl $ty {
            #[doc = concat!("An empty ", stringify!($kind), " Card named `name` at `version`; nothing is validated until its graph is.")]
            pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
                Self {
                    card: Card::new(CardKind::$kind, name, version)$(.flattening($flatten))?,
                }
            }
        }

        card_methods!($ty);

        impl From<$ty> for CardDefinition {
            /// The same Card, keeping what it references.
            fn from(card: $ty) -> Self {
                Self(card.card)
            }
        }

        impl From<$ty> for FieldValue {
            /// A sibling reference to the Card, declared in the same graph.
            fn from(card: $ty) -> Self {
                Self::from(CardDefinition::from(card))
            }
        }

        impl From<&$ty> for CardDefinition {
            /// A copy of the Card as it is now.
            fn from(card: &$ty) -> Self {
                Self::from(card.clone())
            }
        }

        impl From<$ty> for $crate::cards::Declarable<'_> {
            /// The authored Card and everything it references.
            fn from(card: $ty) -> Self {
                Self::Card(Box::new(card.into()))
            }
        }

        impl From<&$ty> for $crate::cards::Declarable<'_> {
            /// A copy of the authored Card as it is now.
            fn from(card: &$ty) -> Self {
                Self::Card(Box::new(card.into()))
            }
        }
    };
}

card_kind!(
    /// A Service Card: a running system, its components, and the Verifiers
    /// that check it.
    Service => Service
);

fields!(Service [card.spec] {
    one description: Value = "description";
    one service_type: Value = "service_type";
    list components, add_component: Component = "components";
    list verified_by, add_verified_by: Binding = "verified_by";
    list tables, add_table: Value = "tables";
    one entry_point: Value = "entry_point";
    map deployment, add_deployment: Value = "deployment";
    one runtime: Value = "runtime";
    map service_config, add_service_config: Value = "service_config";
    list credential_refs, add_credential_ref: Value = "credential_refs";
    one content_hash: Value = "content_hash";
    one lock_hash: Value = "lock_hash";
    map metadata, add_metadata: Value = "metadata";
});

card_kind!(
    /// An Agent Card: a prompt run through an execution route with tools.
    Agent => Agent
);

fields!(Agent [card.spec] {
    one prompt: FieldValue = "prompt";
    one execution_route: Value = "execution_route";
    list tool_names, add_tool_name: Value = "tool_names";
    one run_config: Value = "run_config";
    list verified_by, add_verified_by: Binding = "verified_by";
});

card_kind!(
    /// A Prompt Card: provider, model, and the authored prompt.
    Prompt => Prompt
);

fields!(Prompt [card.spec] {
    one provider: Value = "provider";
    one model: Value = "model";
    one system: Value = "system";
    list messages, add_message: Value = "messages";
    one settings: Value = "settings";
    one response_format: Value = "response_format";
    one format: Value = "format";
    map body, add_body: Value = "body";
});

card_kind!(
    /// A Model Card.
    Model => Model
);

fields!(Model [card.spec] {
    one interface: Value = "interface";
    one task_type: Value = "task_type";
    one signature: Value = "signature";
    one sample_input: Value = "sample_input";
    list card_refs, add_card_ref: FieldValue = "card_refs";
});

card_kind!(
    /// A Data Card.
    Data => Data
);

fields!(Data [card.spec] {
    one interface: Value = "interface";
    one schema: Value = "schema";
    list card_refs, add_card_ref: FieldValue = "card_refs";
    map splits, add_split: FieldValue = "splits";
    list target_columns, add_target_column: Value = "target_columns";
    one sql: Value = "sql";
    one stats: Value = "stats";
});

card_kind!(
    /// An Experiment Card.
    Experiment => Experiment
);

fields!(Experiment [card.spec] {
    one experiment_type: Value = "experiment_type";
    one description: Value = "description";
    list target_refs, add_target_ref: FieldValue = "target_refs";
    map default_parameters, add_default_parameter: Value = "default_parameters";
    list run_refs, add_run_ref: Value = "run_refs";
    list summary_metrics, add_summary_metric: Value = "summary_metrics";
    one best_run_ref: Value = "best_run_ref";
    list card_refs, add_card_ref: FieldValue = "card_refs";
    map details, add_detail: Value = "details";
});

card_kind!(
    /// An Mcp Card: an MCP server and the tools it exposes.
    Mcp => Mcp
);

fields!(Mcp [card.spec] {
    one description: Value = "description";
    one server_name: Value = "server_name";
    one transport: Value = "transport";
    list tool_refs, add_tool_ref: FieldValue = "tool_refs";
    list scopes, add_scope: Value = "scopes";
    map details, add_detail: Value = "details";
});

card_kind!(
    /// An Audit Card: subjects, policies, and evidence.
    Audit => Audit
);

fields!(Audit [card.spec] {
    one description: Value = "description";
    list subject_refs, add_subject_ref: FieldValue = "subject_refs";
    list policy_refs, add_policy_ref: FieldValue = "policy_refs";
    list evidence_refs, add_evidence_ref: FieldValue = "evidence_refs";
    map details, add_detail: Value = "details";
});

card_kind!(
    /// A Trigger Card: when a binding's Verifier runs.
    Trigger => Trigger
);

fields!(Trigger [card.spec] {
    one description: Value = "description";
    one kind: Value = "kind";
    one cron: Value = "cron";
    one tz: Value = "tz";
    one after: Value = "after";
});

card_kind!(
    /// An Operator Card: what runs when a binding's Verifier fails or
    /// changes verdict.
    Operator => Operator, flatten = "action"
);

fields!(Operator [card.spec] {
    one description: Value = "description";
    one action: Value = "action";
    one budget: Value = "budget";
});

/// An Artifact Card: a local file or directory published before the graph
/// that references it, or an Artifact defined only by its spec.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    /// The Card under construction.
    card: Card,
    /// The local file or directory to publish, if any.
    source: Option<PathBuf>,
    /// Whether supported documents are extracted beside their originals.
    extract: bool,
}

impl Artifact {
    /// An empty Artifact Card named `name` at `version` with no local
    /// source; it is declared in its graph like any sibling Card.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            card: Card::new(CardKind::Artifact, name, version),
            source: None,
            extract: false,
        }
    }

    /// The `file` Artifact named `name` at `version`, published from the
    /// file or directory at `path` before its graph; nothing is read until
    /// it is published.
    pub fn from_path(
        name: impl Into<String>,
        version: impl Into<String>,
        path: impl AsRef<Path>,
    ) -> Self {
        let mut artifact = Self::new(name, version).artifact_kind("file");
        artifact.source = Some(path.as_ref().to_path_buf());
        artifact
    }

    /// Set whether supported documents are extracted beside their originals
    /// when the local source is published.
    #[must_use]
    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    /// Select the fixed file or directory prefix `path` of this exact
    /// Artifact version; a local source is published first.
    pub fn file(&self, path: impl Into<String>) -> ArtifactFile {
        let reference = FieldValue::from(self.clone());
        ArtifactFile(FieldValue {
            value: selection(self.card.card_ref().value(), path.into()),
            captures: reference.captures,
        })
    }

    /// This Artifact's local declaration, once validated.
    ///
    /// # Errors
    /// Returns every diagnostic for this Artifact.
    pub(super) fn local(&self) -> Result<Option<LocalArtifact>, Vec<Diagnostic>> {
        let Some(source) = &self.source else {
            return Ok(None);
        };
        let card = self.card.decode()?;
        let Spec::Artifact(spec) = card.spec else {
            return Ok(None);
        };
        spec.validate().map_err(|error| {
            vec![Diagnostic::from_wyrd_error(
                self.card.label(),
                wyrd_loader::Severity::Error,
                None,
                &error,
            )]
        })?;
        Ok(Some(LocalArtifact {
            metadata: card.metadata,
            spec,
            source: source.clone(),
            extract: self.extract,
        }))
    }

    /// Whether this Artifact publishes from a local source.
    pub(super) fn is_local(&self) -> bool {
        self.source.is_some()
    }
}

card_methods!(Artifact);

fields!(Artifact [card.spec] {
    one artifact_kind: Value = "artifact_kind";
    list artifact_uris, add_artifact_uri: Value = "artifact_uris";
    one content_type: Value = "content_type";
    one size_bytes: Value = "size_bytes";
    one integrity: Value = "integrity";
    one schema_ref: FieldValue = "schema_ref";
    one framework_adapter: Value = "framework_adapter";
    one external_uri: Value = "external_uri";
    map metadata, add_metadata: Value = "metadata";
});

impl From<Artifact> for FieldValue {
    /// A local Artifact is an exact external reference published first; any
    /// other is a sibling reference declared in the same graph.
    fn from(artifact: Artifact) -> Self {
        if !artifact.is_local() {
            return Self::from(CardDefinition(artifact.card));
        }
        Self {
            value: artifact.card.card_ref().value(),
            captures: Captures {
                artifacts: vec![artifact],
                ..Captures::default()
            },
        }
    }
}

/// One Service component: an alias for a referenced Card and the Verifiers
/// that check it.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// The component's fields.
    body: Body,
}

impl Component {
    /// The component `alias` for `reference`: a Card, an exact [`CardRef`],
    /// or a YAML reference path; nothing is validated until its graph is.
    pub fn new(alias: impl Into<String>, reference: impl Into<FieldValue>) -> Self {
        Self {
            body: Body::new([
                ("alias", FieldValue::from(alias.into())),
                ("ref", reference.into()),
            ]),
        }
    }
}

fields!(Component [body] {
    one alias: Value = "alias";
    one r#ref: FieldValue = "ref";
    list verified_by, add_verified_by: Binding = "verified_by";
    one source: Value = "source";
    map config, add_config: Value = "config";
    list credential_refs, add_credential_ref: Value = "credential_refs";
});

impl From<Component> for FieldValue {
    /// The canonical component object.
    fn from(component: Component) -> Self {
        component.body.into_field()
    }
}

/// One verification binding: the Verifier to run, when it runs, and the
/// Operators that react to its verdicts.
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    /// The binding's fields.
    body: Body,
}

impl Binding {
    /// Bind `verifier`: a [`Verifier`], an exact [`CardRef`], or a YAML
    /// reference path.
    pub fn new(verifier: impl Into<FieldValue>) -> Self {
        Self {
            body: Body::new([("verifier", verifier.into())]),
        }
    }
}

fields!(Binding [body] {
    one verifier: FieldValue = "verifier";
    one runs_on: FieldValue = "runs_on";
    list on_failure, add_on_failure: FieldValue = "on_failure";
    list on_verdict_change, add_on_verdict_change: FieldValue = "on_verdict_change";
});

impl From<Verifier> for Binding {
    /// Bind the Verifier with no trigger or Operators.
    fn from(verifier: Verifier) -> Self {
        Self::new(verifier)
    }
}

impl From<CardRef> for Binding {
    /// Bind the registered Verifier with no trigger or Operators.
    fn from(verifier: CardRef) -> Self {
        Self::new(verifier)
    }
}

impl From<&str> for Binding {
    /// Bind the Verifier at the YAML reference path.
    fn from(verifier: &str) -> Self {
        Self::new(verifier)
    }
}

impl From<Binding> for FieldValue {
    /// The canonical binding object.
    fn from(binding: Binding) -> Self {
        binding.body.into_field()
    }
}
