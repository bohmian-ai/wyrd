//! In-memory Card authoring in the Card YAML's own vocabulary.
//!
//! Each named Card type, such as [`Service`] or [`Verifier`], is created with
//! `new(name, version)`; each sub-value, such as [`Component`] or [`Step`],
//! with the fields that identify it. Every other method is named for the YAML
//! field it edits and takes that field's YAML value: references stay strings
//! such as `inputs.report.text`, operators stay `"equals"` or their object
//! form, and nested objects stay their canonical JSON. A setter such as
//! [`Service::verified_by`] consumes the value and replaces the whole field;
//! an add method such as [`Service::add_verified_by`] appends one entry in
//! place.
//!
//! Construction and editing never validate, read files, or touch the
//! network, so an incomplete or malformed definition, or one with a forward
//! reference, can be assembled freely. A Card value or exact [`CardRef`] may
//! stand at any reference position, as a YAML path does; passing a child
//! captures its current value, so later edits to the child change nothing
//! already passed. Replacing a field drops everything its old value
//! referenced, while another field still referencing the same definition
//! keeps it.
//!
//! [`CardDefinition::declaration`] snapshots the graph, reads each
//! [`CodeSource::file`], and decodes every Card through the loader's own
//! envelope decoding ([`wyrd_loader::parse_card_value`]) and tree validation
//! ([`wyrd_loader::declaration_input_from_cards`]), so equal YAML and code
//! values meet the same rules and YAML field paths. Validation runs again for
//! every snapshot; nothing caches it. Registration goes through
//! [`crate::cards::Cards::declare`] or [`crate::state::WyrdState::declare`],
//! which publish each local Artifact first and then declare the graph.

#![deny(missing_docs)]

mod kinds;
mod verifier;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use wyrd_loader::{DeclarationInput, Diagnostic, LoadError, Severity};
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::error::WyrdError;

pub use kinds::{
    Agent, Artifact, Audit, Binding, Component, Data, Experiment, Mcp, Model, Operator, Prompt,
    Service, Trigger,
};
pub use verifier::{
    Action, AgentAction, AgentCheck, Check, EachCheck, InputSlot, Requirement, Step, Verifier,
};
pub use wyrd_spec::verification::ExecutionPlan;

/// One step of a location inside an authored value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// A field or map key.
    Key(String),
    /// A list position.
    Index(usize),
}

/// A location inside an authored value, relative to that value.
type Location = Vec<Segment>;

/// The YAML field path of `location` within a spec, such as
/// `steps[0].action.inputs.report`, as the loader's decode errors name it.
fn field_path(location: &[Segment]) -> String {
    let mut path = String::new();
    for segment in location {
        match segment {
            Segment::Key(key) => {
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(key);
            }
            Segment::Index(index) => {
                path.push('[');
                path.push_str(&index.to_string());
                path.push(']');
            }
        }
    }
    path
}

/// What one authored value carries besides its JSON: the definitions it
/// references, the map keys added to it twice, and the local code files its
/// snapshot reads.
#[derive(Debug, Clone, Default, PartialEq)]
struct Captures {
    /// Referenced Card definitions, captured by value, local Artifacts
    /// included.
    cards: Vec<CardDefinition>,
    /// Map keys an add method received twice; validation refuses them.
    duplicates: Vec<Location>,
    /// Code-source positions filled from a local file at snapshot time.
    files: Vec<(Location, PathBuf)>,
}

impl Captures {
    /// Take over everything `other` captures.
    fn absorb(&mut self, other: Self) {
        self.cards.extend(other.cards);
        self.duplicates.extend(other.duplicates);
        self.files.extend(other.files);
    }

    /// These captures, relocated beneath `segment`.
    fn under(mut self, segment: &Segment) -> Self {
        for location in self
            .duplicates
            .iter_mut()
            .chain(self.files.iter_mut().map(|(location, _)| location))
        {
            location.insert(0, segment.clone());
        }
        self
    }
}

/// A value for one authoring field: canonical JSON, plus the Cards, local
/// Artifacts, and local code files it references.
///
/// It converts from any JSON [`Value`] or string and from every authoring
/// value that can stand at a reference position: a Card, which becomes a
/// sibling reference declared in the same graph; a [`CardRef`], which stays an
/// external reference; an [`ArtifactFile`]; and a [`CodeSource`].
#[derive(Debug, Clone, PartialEq)]
pub struct FieldValue {
    /// The canonical JSON this value contributes.
    value: Value,
    /// What the value references.
    captures: Captures,
}

impl FieldValue {
    /// Plain JSON that references nothing.
    fn plain(value: Value) -> Self {
        Self {
            value,
            captures: Captures::default(),
        }
    }
}

impl From<Value> for FieldValue {
    /// The JSON value as is.
    fn from(value: Value) -> Self {
        Self::plain(value)
    }
}

impl From<&str> for FieldValue {
    /// A JSON string, such as a reference or a scope path.
    fn from(value: &str) -> Self {
        Self::plain(Value::from(value))
    }
}

impl From<String> for FieldValue {
    /// A JSON string, such as a reference or a scope path.
    fn from(value: String) -> Self {
        Self::plain(Value::from(value))
    }
}

impl From<CardRef> for FieldValue {
    /// The exact external reference `{kind, name, version}`.
    fn from(card_ref: CardRef) -> Self {
        Self::plain(card_ref.value())
    }
}

impl From<wyrd_spec::reference::CardRef> for FieldValue {
    /// The canonical reference's exact identity; registration pins its UID.
    fn from(card_ref: wyrd_spec::reference::CardRef) -> Self {
        Self::from(CardRef::new(
            card_ref.kind,
            card_ref.name.as_str(),
            card_ref.version.to_string(),
        ))
    }
}

impl From<CardDefinition> for FieldValue {
    /// A sibling reference to `card`, which is declared in the same graph,
    /// or, for an Artifact with a local source, its exact external
    /// reference, published first.
    fn from(card: CardDefinition) -> Self {
        let reference = card.0.card_ref().value();
        Self {
            value: if card.0.source.is_some() {
                reference
            } else {
                json!({ "sibling": reference })
            },
            captures: Captures {
                cards: vec![card],
                ..Captures::default()
            },
        }
    }
}

/// The editable JSON of one object-valued authoring value, with what each of
/// its fields captures.
///
/// Captures are kept per field, so a setter replaces exactly its own field's
/// references and leaves every other field's untouched.
#[derive(Debug, Clone, Default, PartialEq)]
struct Body {
    /// The object's fields, as authored.
    fields: Map<String, Value>,
    /// What each field's current value captures, located from this object.
    captures: BTreeMap<String, Captures>,
    /// A field whose object value is written into this object's own fields,
    /// as `#[serde(flatten)]` reads it.
    flatten: Option<&'static str>,
    /// A value supplied in place of the whole object that is not an object,
    /// such as a raw spec string; it stands as authored, so validation
    /// refuses it as YAML would.
    malformed: Option<Value>,
}

impl Body {
    /// An object holding `fields`.
    fn new<const N: usize>(fields: [(&str, FieldValue); N]) -> Self {
        let mut body = Self::default();
        for (field, value) in fields {
            body.set(field, value);
        }
        body
    }

    /// Replace `field` with `value`, dropping what the old value captured.
    fn set(&mut self, field: &str, value: FieldValue) {
        let key = Segment::Key(field.to_owned());
        self.fields.insert(field.to_owned(), value.value);
        self.captures
            .insert(field.to_owned(), value.captures.under(&key));
    }

    /// Replace the list `field` with `items`.
    fn set_list(&mut self, field: &str, items: impl IntoIterator<Item = FieldValue>) {
        self.set(field, FieldValue::plain(Value::Array(Vec::new())));
        for item in items {
            self.push(field, item);
        }
    }

    /// Append `item` to the list `field`; a non-list value is replaced.
    fn push(&mut self, field: &str, item: FieldValue) {
        if !self.fields.get(field).is_some_and(Value::is_array) {
            self.set(field, FieldValue::plain(Value::Array(Vec::new())));
        }
        let Some(Value::Array(list)) = self.fields.get_mut(field) else {
            return;
        };
        let captures = item.captures.under(&Segment::Index(list.len()));
        list.push(item.value);
        self.absorb(field, captures);
    }

    /// Replace the map `field` with `entries`.
    fn set_map(&mut self, field: &str, entries: impl IntoIterator<Item = (String, FieldValue)>) {
        self.set(field, FieldValue::plain(Value::Object(Map::new())));
        for (key, value) in entries {
            self.insert(field, key, value);
        }
    }

    /// Insert `key` into the map `field`; a non-map value is replaced. A key
    /// already present keeps its value and is recorded for validation to
    /// refuse.
    fn insert(&mut self, field: &str, key: String, value: FieldValue) {
        if !self.fields.get(field).is_some_and(Value::is_object) {
            self.set(field, FieldValue::plain(Value::Object(Map::new())));
        }
        let Some(Value::Object(map)) = self.fields.get_mut(field) else {
            return;
        };
        let segment = Segment::Key(key.clone());
        if map.contains_key(&key) {
            self.absorb(
                field,
                Captures {
                    duplicates: vec![vec![segment]],
                    ..Captures::default()
                },
            );
            return;
        }
        map.insert(key, value.value);
        self.absorb(field, value.captures.under(&segment));
    }

    /// Add `captures`, located within `field`, to that field's captures.
    fn absorb(&mut self, field: &str, captures: Captures) {
        let key = Segment::Key(field.to_owned());
        self.captures
            .entry(field.to_owned())
            .or_default()
            .absorb(captures.under(&key));
    }

    /// The object as authored, with any flattened field written in place,
    /// or the malformed value supplied in its place.
    fn value(&self) -> Value {
        if let Some(malformed) = &self.malformed {
            return malformed.clone();
        }
        let mut fields = self.fields.clone();
        if let Some(flattened) = self.flatten
            && let Some(Value::Object(inner)) = fields.remove(flattened)
        {
            for (key, value) in inner {
                fields.entry(key).or_insert(value);
            }
        }
        Value::Object(fields)
    }

    /// Everything every field captures.
    fn captures(&self) -> Captures {
        let mut all = Captures::default();
        for captures in self.captures.values() {
            all.absorb(captures.clone());
        }
        all
    }

    /// This object as one field value.
    fn into_field(self) -> FieldValue {
        FieldValue {
            value: self.value(),
            captures: self.captures(),
        }
    }
}

/// Generate field setters and add methods over a [`Body`] reached through
/// `self.<path>`.
///
/// `one` fields take one value; `list` fields take an iterable and add one
/// item; `map` fields take `(key, value)` pairs and add one entry. Each value
/// converts into `$item`, which converts into a [`FieldValue`].
macro_rules! fields {
    ($ty:ty [$($body:ident).+] { $($rest:tt)* }) => {
        impl $ty {
            fields!(@item [$($body).+] $($rest)*);
        }
    };
    (@item [$($body:ident).+]) => {};
    (@item [$($body:ident).+] one $name:ident: $item:ty = $field:literal; $($rest:tt)*) => {
        #[doc = concat!("Set `", $field, "`, replacing its earlier value and whatever that value referenced.")]
        #[must_use]
        pub fn $name(mut self, value: impl Into<$item>) -> Self {
            let value: $item = value.into();
            self.$($body).+.set($field, $crate::authoring::FieldValue::from(value));
            self
        }
        fields!(@item [$($body).+] $($rest)*);
    };
    (@item [$($body:ident).+] list $name:ident, $add:ident: $item:ty = $field:literal; $($rest:tt)*) => {
        #[doc = concat!("Set the whole `", $field, "` list, replacing every earlier entry and whatever it referenced.")]
        #[must_use]
        pub fn $name<I>(mut self, items: I) -> Self
        where
            I: IntoIterator,
            I::Item: Into<$item>,
        {
            self.$($body).+.set_list(
                $field,
                items.into_iter().map(|item| {
                    let item: $item = item.into();
                    $crate::authoring::FieldValue::from(item)
                }),
            );
            self
        }
        #[doc = concat!("Append one entry to `", $field, "`; an equal ID is kept for validation to refuse.")]
        pub fn $add(&mut self, item: impl Into<$item>) -> &mut Self {
            let item: $item = item.into();
            self.$($body).+.push($field, $crate::authoring::FieldValue::from(item));
            self
        }
        fields!(@item [$($body).+] $($rest)*);
    };
    (@item [$($body:ident).+] map $name:ident, $add:ident: $item:ty = $field:literal; $($rest:tt)*) => {
        #[doc = concat!("Set the whole `", $field, "` map, replacing every earlier entry and whatever it referenced.")]
        #[must_use]
        pub fn $name<I, K, V>(mut self, entries: I) -> Self
        where
            I: IntoIterator<Item = (K, V)>,
            K: Into<String>,
            V: Into<$item>,
        {
            self.$($body).+.set_map(
                $field,
                entries.into_iter().map(|(key, value)| {
                    let value: $item = value.into();
                    (key.into(), $crate::authoring::FieldValue::from(value))
                }),
            );
            self
        }
        #[doc = concat!("Insert the entry `key` into `", $field, "`; a key already present keeps its value and validation refuses the repeat.")]
        pub fn $add(&mut self, key: impl Into<String>, value: impl Into<$item>) -> &mut Self {
            let value: $item = value.into();
            self.$($body).+.insert($field, key.into(), $crate::authoring::FieldValue::from(value));
            self
        }
        fields!(@item [$($body).+] $($rest)*);
    };
}

pub(crate) use fields;

/// An exact Card reference: kind, name, and one version, kept as authored
/// until validation.
///
/// At a reference position it stays an external reference to a registered
/// Card, even when the same graph defines that identity, and nothing is
/// uploaded for it; registration resolves and pins its UID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardRef {
    /// The referenced kind.
    kind: CardKind,
    /// The referenced name.
    name: String,
    /// The referenced exact version.
    version: String,
}

/// Generate `CardRef` shorthands, one per named kind.
macro_rules! card_ref_shorthands {
    ($($name:ident => $kind:ident),* $(,)?) => {
        impl CardRef {
            $(
                #[doc = concat!("The exact reference to the ", stringify!($kind), " Card `name` at `version`.")]
                pub fn $name(name: impl Into<String>, version: impl Into<String>) -> Self {
                    Self::new(CardKind::$kind, name, version)
                }
            )*
        }
    };
}

card_ref_shorthands!(
    data => Data,
    model => Model,
    artifact => Artifact,
    experiment => Experiment,
    prompt => Prompt,
    agent => Agent,
    mcp => Mcp,
    service => Service,
    audit => Audit,
    verifier => Verifier,
    trigger => Trigger,
    operator => Operator,
);

impl CardRef {
    /// The exact reference to the `kind` Card `name` at `version`.
    pub fn new(kind: CardKind, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            kind,
            name: name.into(),
            version: version.into(),
        }
    }

    /// The referenced kind.
    #[must_use]
    pub fn kind(&self) -> &CardKind {
        &self.kind
    }

    /// The referenced name, as authored.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The referenced version, as authored.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Select the fixed file or directory prefix `path` of this registered
    /// Artifact version, with no local source and nothing uploaded.
    pub fn file(&self, path: impl Into<String>) -> ArtifactFile {
        ArtifactFile(FieldValue::plain(selection(self.value(), path.into())))
    }

    /// The canonical `{kind, name, version}` reference value.
    fn value(&self) -> Value {
        json!({ "kind": self.kind.wire_name(), "name": self.name, "version": self.version })
    }
}

/// The canonical `{artifact: {card, path}}` selection of `path` in `card`.
fn selection(card: Value, path: String) -> Value {
    json!({ "artifact": { "card": card, "path": path } })
}

/// A fixed file or directory prefix of one exact Artifact version, mounted
/// into a workspace or run as code; execution input cannot replace it.
///
/// A local Artifact's selection publishes that Artifact before its graph.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactFile(
    /// The canonical selection and the Artifact it publishes, if local.
    FieldValue,
);

impl From<ArtifactFile> for FieldValue {
    /// The canonical `{artifact: {card, path}}` selection.
    fn from(file: ArtifactFile) -> Self {
        file.0
    }
}

/// The source of a code action or check: inline text or a local file
/// captured as inline text when its graph is validated.
///
/// An [`ArtifactFile`] is the third canonical form.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeSource(
    /// The canonical source value, or a placeholder for a file not yet read.
    FieldValue,
);

impl CodeSource {
    /// Run `text`, embedded in the Card as `{inline: text}`.
    pub fn inline(text: impl Into<String>) -> Self {
        Self(FieldValue::plain(json!({ "inline": text.into() })))
    }

    /// Run the UTF-8 file at `path`, read when the graph is validated or
    /// declared and embedded as `{inline: text}`; nothing is read now, and
    /// each snapshot reads the file afresh.
    pub fn file(path: impl AsRef<Path>) -> Self {
        Self(FieldValue {
            value: Value::Null,
            captures: Captures {
                files: vec![(Vec::new(), path.as_ref().to_path_buf())],
                ..Captures::default()
            },
        })
    }
}

impl From<CodeSource> for FieldValue {
    /// The canonical code source.
    fn from(source: CodeSource) -> Self {
        source.0
    }
}

/// One named Card under construction: identity, labels and annotations, and
/// its spec.
#[derive(Debug, Clone, PartialEq)]
struct Card {
    /// The kind, fixed by the authoring type.
    kind: CardKind,
    /// The name, as authored.
    name: String,
    /// The exact version, as authored.
    version: String,
    /// `labels` and `annotations`.
    metadata: Body,
    /// The spec.
    spec: Body,
    /// The local file or directory an Artifact publishes from first, if
    /// any; only an Artifact sets one.
    source: Option<PathBuf>,
    /// Whether that local source's supported documents are extracted
    /// beside their originals.
    extract: bool,
}

impl Card {
    /// An empty `kind` Card named `name` at `version`.
    fn new(kind: CardKind, name: impl Into<String>, version: impl Into<String>) -> Self {
        Self::with_spec(kind, name, version, Body::default())
    }

    /// A `kind` Card named `name` at `version` with `spec`.
    fn with_spec(
        kind: CardKind,
        name: impl Into<String>,
        version: impl Into<String>,
        spec: Body,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            version: version.into(),
            metadata: Body::default(),
            spec,
            source: None,
            extract: false,
        }
    }

    /// This Card with the spec field `field` written into the spec itself,
    /// as `#[serde(flatten)]` reads it.
    fn flattening(mut self, field: &'static str) -> Self {
        self.spec.flatten = Some(field);
        self
    }

    /// The exact reference this Card registers as.
    fn card_ref(&self) -> CardRef {
        CardRef::new(self.kind.clone(), &self.name, &self.version)
    }

    /// The label naming this Card in diagnostics, in place of a file path.
    fn label(&self) -> PathBuf {
        PathBuf::from(format!(
            "{}/{}@{}",
            self.kind.wire_name(),
            self.name,
            self.version
        ))
    }

    /// The envelope YAML would hold, with every local code file read now.
    ///
    /// # Errors
    /// Returns a diagnostic for each map key added twice and each code file
    /// that cannot be read as UTF-8 text.
    fn envelope(&self) -> Result<Value, Vec<Diagnostic>> {
        let mut spec = self.spec.value();
        let captures = self.spec.captures();
        let mut diagnostics: Vec<Diagnostic> = captures
            .duplicates
            .iter()
            .map(|location| {
                format!(
                    "Invalid spec for kind {}: {}",
                    self.kind.wire_name(),
                    field_path(location)
                )
            })
            .chain(
                self.metadata
                    .captures()
                    .duplicates
                    .iter()
                    .map(|location| format!("metadata.{}", field_path(location))),
            )
            .map(|path| {
                Diagnostic::invalid_envelope(
                    self.label(),
                    format!("{path}: duplicate entry; each key may be added once"),
                )
            })
            .collect();
        for (location, file) in &captures.files {
            match std::fs::read_to_string(file) {
                Ok(text) => {
                    if let Some(slot) = value_at(&mut spec, location) {
                        *slot = json!({ "inline": text });
                    }
                }
                Err(error) => diagnostics.push(Diagnostic::io(file.clone(), &error)),
            }
        }
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        let mut metadata = self.metadata.value();
        if let Value::Object(fields) = &mut metadata {
            fields.insert("name".to_owned(), Value::from(self.name.as_str()));
            fields.insert("version".to_owned(), Value::from(self.version.as_str()));
        }
        Ok(json!({ "kind": self.kind.wire_name(), "metadata": metadata, "spec": spec }))
    }

    /// Decode this Card's envelope as its YAML document would be decoded,
    /// requiring an exact version.
    ///
    /// # Errors
    /// Returns every diagnostic for this Card.
    fn decode(&self) -> Result<wyrd_loader::AuthoredCard, Vec<Diagnostic>> {
        let envelope = self.envelope()?;
        let card = wyrd_loader::parse_card_value(&self.label(), envelope).map_err(|d| vec![d])?;
        if card.metadata.resolved_pin().is_none() {
            return Err(vec![Diagnostic::from_wyrd_error(
                self.label(),
                Severity::Error,
                None,
                &WyrdError::RegistryInvalidVersionBlock {
                    message: "metadata.version: an authored Card needs an exact version".to_owned(),
                    details: json!({ "path": "metadata.version", "version": self.version }),
                },
            )]);
        }
        Ok(card)
    }

    /// This Artifact's sole publication from `source`, after its descriptor
    /// passes the same decoding and tree validation as any Card.
    ///
    /// # Errors
    /// Returns every diagnostic for this Artifact, such as an unresolved
    /// path reference or a blank `artifact_kind`.
    ///
    /// # Panics
    /// Never: only an Artifact Card has a local source.
    fn local(&self, source: &Path) -> Result<LocalArtifact, Vec<Diagnostic>> {
        let card = self.decode()?;
        wyrd_loader::declaration_input_from_cards(std::slice::from_ref(&card))
            .map_err(|error| error.diagnostics)?;
        let Spec::Artifact(spec) = card.spec else {
            unreachable!("only an Artifact Card has a local source");
        };
        spec.validate().map_err(|error| {
            vec![Diagnostic::from_wyrd_error(
                self.label(),
                Severity::Error,
                None,
                &error,
            )]
        })?;
        Ok(LocalArtifact {
            metadata: card.metadata,
            spec,
            source: source.to_path_buf(),
            extract: self.extract,
        })
    }
}

/// The JSON at `location` inside `value`, if it exists.
fn value_at<'a>(value: &'a mut Value, location: &[Segment]) -> Option<&'a mut Value> {
    location
        .iter()
        .try_fold(value, |value, segment| match segment {
            Segment::Key(key) => value.get_mut(key.as_str()),
            Segment::Index(index) => value.get_mut(*index),
        })
}

/// Generate the methods every named Card type shares: identity and
/// metadata setters, its reference, its authored spec, and validation.
macro_rules! card_methods {
    ($ty:ty) => {
        impl $ty {
            /// Set the Card's name, as authored.
            #[must_use]
            pub fn name(mut self, name: impl Into<String>) -> Self {
                self.card.name = name.into();
                self
            }

            /// Set the Card's exact version, as authored.
            #[must_use]
            pub fn version(mut self, version: impl Into<String>) -> Self {
                self.card.version = version.into();
                self
            }

            /// Set the whole `metadata.labels` map.
            #[must_use]
            pub fn labels<I, K, V>(mut self, labels: I) -> Self
            where
                I: IntoIterator<Item = (K, V)>,
                K: Into<String>,
                V: Into<String>,
            {
                self.card.metadata.set_map(
                    "labels",
                    labels.into_iter().map(|(key, value)| {
                        (
                            key.into(),
                            $crate::authoring::FieldValue::from(value.into()),
                        )
                    }),
                );
                self
            }

            /// Insert the label `key`; a repeated key is refused at validation.
            pub fn add_label(
                &mut self,
                key: impl Into<String>,
                value: impl Into<String>,
            ) -> &mut Self {
                self.card.metadata.insert(
                    "labels",
                    key.into(),
                    $crate::authoring::FieldValue::from(value.into()),
                );
                self
            }

            /// Set the whole `metadata.annotations` map.
            #[must_use]
            pub fn annotations<I, K, V>(mut self, annotations: I) -> Self
            where
                I: IntoIterator<Item = (K, V)>,
                K: Into<String>,
                V: Into<String>,
            {
                self.card.metadata.set_map(
                    "annotations",
                    annotations.into_iter().map(|(key, value)| {
                        (
                            key.into(),
                            $crate::authoring::FieldValue::from(value.into()),
                        )
                    }),
                );
                self
            }

            /// Insert the annotation `key`; a repeated key is refused at
            /// validation.
            pub fn add_annotation(
                &mut self,
                key: impl Into<String>,
                value: impl Into<String>,
            ) -> &mut Self {
                self.card.metadata.insert(
                    "annotations",
                    key.into(),
                    $crate::authoring::FieldValue::from(value.into()),
                );
                self
            }

            /// The exact reference this Card registers as.
            #[must_use]
            pub fn card_ref(&self) -> $crate::authoring::CardRef {
                self.card.card_ref()
            }

            /// The spec as authored so far, before validation; a
            /// [`CodeSource::file`](crate::authoring::CodeSource::file) position is `null` until its graph is
            /// validated.
            #[must_use]
            pub fn spec(&self) -> serde_json::Value {
                self.card.spec.value()
            }

            /// Validate the complete graph this Card roots, exactly as its
            /// declaration would, without registering or uploading anything.
            ///
            /// # Errors
            /// Returns [`CardDefinition::declaration`]'s refusals.
            pub fn validate(&self) -> Result<(), wyrd_spec::error::WyrdError> {
                $crate::authoring::Graph::declaration(
                    $crate::authoring::FieldValue::from(self.clone()).captures,
                )
                .map(drop)
            }
        }
    };
}

pub(crate) use card_methods;

/// A Card of any registrable kind, authored as its kind and canonical spec
/// JSON.
///
/// Every named Card type converts into one, keeping what it references.
#[derive(Debug, Clone, PartialEq)]
pub struct CardDefinition(
    /// The Card.
    Card,
);

impl CardDefinition {
    /// Define the `kind` Card `name` at `version` with the canonical `spec`
    /// JSON, as its YAML `spec` would hold it.
    pub fn new(
        kind: CardKind,
        name: impl Into<String>,
        version: impl Into<String>,
        spec: Value,
    ) -> Self {
        let body = match spec {
            Value::Object(fields) => Body {
                fields,
                ..Body::default()
            },
            malformed => Body {
                malformed: Some(malformed),
                ..Body::default()
            },
        };
        Self(Card::with_spec(kind, name, version, body))
    }

    /// The exact reference this Card registers as.
    #[must_use]
    pub fn card_ref(&self) -> CardRef {
        self.0.card_ref()
    }

    /// The spec as authored so far, before validation; a
    /// [`CodeSource::file`](crate::authoring::CodeSource::file) position is `null` until its graph is validated.
    #[must_use]
    pub fn spec(&self) -> Value {
        self.0.spec.value()
    }

    /// Snapshot and validate this Card's graph as its declaration: the local
    /// Artifacts to publish first, dependencies first, and the containing
    /// graph's input.
    ///
    /// Every code file is read now and every Card, local Artifacts
    /// included, is decoded and validated as its YAML file would be. Each
    /// referenced definition joins the graph once; definitions that decode
    /// to the same Card are one submission. A local Artifact is referenced
    /// by its exact identity, so the graph does not depend on publication,
    /// and no local path enters a submission.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] when two different
    /// definitions share one identity, a local Artifact and a Card without
    /// that source included, and `WYRD_LOADER_400_INVALID_ENVELOPE` carrying every diagnostic for
    /// an invalid graph: a malformed or incomplete field by its YAML path, a
    /// repeated key, an unreadable code file, a version that is not exact,
    /// and each shared tree validation refusal.
    pub fn declaration(&self) -> Result<Declaration, WyrdError> {
        Graph::declaration(FieldValue::from(self.clone()).captures)
    }
}

/// A Card graph's declaration: local Artifacts published first, each as its
/// own sole submission, then the containing graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    /// Local Artifacts, dependencies first, each published before the graph.
    pub artifacts: Vec<LocalArtifact>,
    /// The containing graph, in dependency-first order with no local sources;
    /// `None` when the root is itself a local Artifact, whose publication,
    /// last in `artifacts`, is the whole declaration.
    pub input: Option<DeclarationInput>,
}

/// One validated local Artifact to publish before its graph.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalArtifact {
    /// The Artifact's exact metadata.
    pub metadata: Metadata,
    /// The Artifact's spec.
    pub spec: ArtifactSpec,
    /// The regular file or directory to publish.
    pub source: PathBuf,
    /// Whether supported documents are extracted beside their originals.
    pub extract: bool,
}

/// The distinct Cards, local Artifacts included, reachable from one root.
#[derive(Default)]
struct Graph {
    /// Cards without their captures, dependencies first.
    cards: Vec<Card>,
}

impl Graph {
    /// Snapshot, decode, and validate everything `captures` references.
    ///
    /// # Errors
    /// As [`CardDefinition::declaration`].
    fn declaration(captures: Captures) -> Result<Declaration, WyrdError> {
        let mut graph = Self::default();
        graph.absorb(captures)?;
        let mut diagnostics = Vec::new();
        let mut authored = Vec::new();
        let mut artifacts = Vec::new();
        for card in &graph.cards {
            let decoded = match &card.source {
                Some(source) => card.local(source).map(|local| artifacts.push(local)),
                None => card.decode().map(|card| authored.push(card)),
            };
            if let Err(errors) = decoded {
                diagnostics.extend(errors);
            }
        }
        if !diagnostics.is_empty() {
            return Err(LoadError::multiple(diagnostics).into());
        }
        let input = if authored.is_empty() {
            None
        } else {
            Some(wyrd_loader::declaration_input_from_cards(&authored)?)
        };
        Ok(Declaration { artifacts, input })
    }

    /// Add every Card `captures` references, local Artifacts included, each
    /// after everything it references.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] for a different
    /// definition of an identity already added.
    fn absorb(&mut self, captures: Captures) -> Result<(), WyrdError> {
        for CardDefinition(card) in captures.cards {
            self.absorb(card.spec.captures())?;
            match self
                .cards
                .iter()
                .find(|known| known.card_ref() == card.card_ref())
            {
                Some(known) if same_content(known, &card) => {}
                Some(_) => return Err(conflict(&card.card_ref())),
                None => self.cards.push(card),
            }
        }
        Ok(())
    }
}

/// Whether two definitions of one identity are the same submission: the
/// same local source, if any, and the same decoded Card, however each
/// spelled its defaults.
///
/// A definition that does not decode compares as authored; its snapshot
/// refuses later.
fn same_content(left: &Card, right: &Card) -> bool {
    if (&left.source, left.extract) != (&right.source, right.extract) {
        return false;
    }
    match (left.decode(), right.decode()) {
        (Ok(left), Ok(right)) => left.metadata == right.metadata && left.spec == right.spec,
        _ => left == right,
    }
}

/// The refusal of two different definitions of `card_ref`.
fn conflict(card_ref: &CardRef) -> WyrdError {
    let card_ref = format!(
        "{}/{}@{}",
        card_ref.kind.wire_name(),
        card_ref.name,
        card_ref.version
    );
    WyrdError::SpecDuplicateSubmission {
        message: format!("{card_ref} is defined twice with different content"),
        details: json!({ "card_ref": card_ref }),
    }
}

/// Offline authoring tests.
#[cfg(test)]
mod tests;
