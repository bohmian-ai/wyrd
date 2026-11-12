//! In-memory Card authoring in the Card YAML's own vocabulary.
//!
//! Each named Card type, such as [`Service`] or [`Verifier`], is created with
//! `new(name, version)`, where the version is optional and the server selects
//! an omitted one at declaration; each sub-value, such as [`Component`] or [`Step`],
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
//! reference, can be assembled freely. A Card value, which becomes a sibling
//! reference, or an exact [`CardRef`] may stand at any reference position, as
//! a YAML path does; passing a child
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
//! which publish each local Artifact first, bind its registered reference
//! into everything that names it, and then declare the graph.

#![deny(missing_docs)]

mod kinds;
mod verifier;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Map, Value, json};
use wyrd_loader::{DeclarationInput, Diagnostic, LoadError, Severity};
use wyrd_spec::card::artifact::ArtifactSpec;
use wyrd_spec::envelope::{CardKind, Metadata, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::graph::sibling_target;
use wyrd_spec::refs::ReferenceSlotVisitor;
use wyrd_spec::registry::CardSubmission;

pub use kinds::{
    Agent, Artifact, Audit, Binding, Component, Data, Experiment, Mcp, Model, Operator, Service,
    Table, Trigger,
};
pub use verifier::{Action, Check, InputSlot, Requirement, Step, Verifier};
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Captures {
    /// Referenced Card definitions, captured by value, local Artifacts
    /// included; each is an immutable snapshot that copies share.
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
/// It converts from any JSON [`Value`] or string, from the canonical typed
/// values a field takes, such as a [`skald_prompt::Prompt`] or an
/// [`wyrd_spec::ExecutionRoute`], and from every authoring value that can
/// stand at a reference position: a Card, which becomes a sibling reference
/// declared in the same graph; a [`CardRef`], which stays an external
/// reference; an [`ArtifactFile`]; and a [`CodeSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl FromIterator<(String, FieldValue)> for FieldValue {
    /// The JSON object of `entries`, keeping what each entry references at
    /// its key; a key given twice keeps its first value and validation
    /// refuses the repeat.
    fn from_iter<I: IntoIterator<Item = (String, FieldValue)>>(entries: I) -> Self {
        Body::assembled(entries).into_field()
    }
}

impl FromIterator<FieldValue> for FieldValue {
    /// The JSON list of `items`, keeping what each item references at its
    /// position.
    fn from_iter<I: IntoIterator<Item = FieldValue>>(items: I) -> Self {
        let mut list = Vec::new();
        let mut captures = Captures::default();
        for (index, item) in items.into_iter().enumerate() {
            list.push(item.value);
            captures.absorb(item.captures.under(&Segment::Index(index)));
        }
        Self {
            value: Value::Array(list),
            captures,
        }
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

/// Generate `From<T> for FieldValue` for canonical typed field values, each
/// written as the JSON its canonical type serializes to.
macro_rules! canonical_values {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<$ty> for FieldValue {
                /// The canonical JSON form of the value.
                ///
                /// # Panics
                /// Never: the canonical type serializes to JSON with string
                /// keys.
                fn from(value: $ty) -> Self {
                    Self::plain(
                        serde_json::to_value(value)
                            .expect("a canonical spec value serializes to JSON"),
                    )
                }
            }
        )*
    };
}

canonical_values!(
    skald_spec::Prompt,
    wyrd_spec::ExecutionRoute,
    wyrd_spec::AgentRunConfigSpec,
    wyrd_spec::verification::ComparisonOperator,
);

impl From<skald_prompt::Prompt> for FieldValue {
    /// The Prompt inline, as its native canonical request; the server
    /// registers it with the Card that holds it.
    fn from(prompt: skald_prompt::Prompt) -> Self {
        Self::from(prompt.into_native())
    }
}

impl From<CardDefinition> for FieldValue {
    /// A sibling reference to `card`, carrying its version only when one is
    /// set. A Card without a local source is declared in the same graph; an
    /// Artifact with one is published first and its registered reference
    /// replaces the sibling.
    fn from(card: CardDefinition) -> Self {
        Self {
            value: json!({ "sibling": card.0.sibling() }),
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// The canonical object `value` exactly as authored: its fields when it
    /// is an object, otherwise the malformed value validation refuses.
    fn authored(value: Value) -> Self {
        match value {
            Value::Object(fields) => Self {
                fields,
                ..Self::default()
            },
            malformed => Self {
                malformed: Some(malformed),
                ..Self::default()
            },
        }
    }

    /// An object holding `fields`.
    fn new<const N: usize>(fields: [(&str, FieldValue); N]) -> Self {
        let mut body = Self::default();
        for (field, value) in fields {
            body.set(field, value);
        }
        body
    }

    /// The object of the received `entries`: a key received twice keeps its
    /// first value and what it captures, and the repeat is recorded for
    /// validation to refuse, as [`Self::insert`] records a map key.
    fn assembled(entries: impl IntoIterator<Item = (String, FieldValue)>) -> Self {
        let mut body = Self::default();
        for (key, value) in entries {
            if body.fields.contains_key(&key) {
                body.captures
                    .entry(key.clone())
                    .or_default()
                    .duplicates
                    .push(vec![Segment::Key(key)]);
            } else {
                body.set(&key, value);
            }
        }
        body
    }

    /// Replace `field` with `value`, dropping what the old value captured.
    /// The flattened field's captures are located from this object, where
    /// its entries are written.
    fn set(&mut self, field: &str, value: FieldValue) {
        let captures = if self.flatten == Some(field) {
            value.captures
        } else {
            value.captures.under(&Segment::Key(field.to_owned()))
        };
        self.fields.insert(field.to_owned(), value.value);
        self.captures.insert(field.to_owned(), captures);
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
    /// or the malformed value supplied in its place; a copy, since the body
    /// stays editable.
    fn value(&self) -> Value {
        Self::written(self.malformed.clone(), self.fields.clone(), self.flatten)
    }

    /// The object `fields` with the `flatten` field written in place, or
    /// `malformed` when one stands in place of the object.
    fn written(
        malformed: Option<Value>,
        mut fields: Map<String, Value>,
        flatten: Option<&'static str>,
    ) -> Value {
        if let Some(malformed) = malformed {
            return malformed;
        }
        if let Some(flattened) = flatten
            && let Some(Value::Object(inner)) = fields.remove(flattened)
        {
            for (key, value) in inner {
                fields.entry(key).or_insert(value);
            }
        }
        Value::Object(fields)
    }

    /// Every captured definition, in field order, without copying any.
    fn cards(&self) -> impl Iterator<Item = &CardDefinition> {
        self.captures.values().flat_map(|captures| &captures.cards)
    }

    /// Every repeated key, located from this object, in field order.
    fn duplicates(&self) -> impl Iterator<Item = &Location> {
        self.captures
            .values()
            .flat_map(|captures| &captures.duplicates)
    }

    /// This object as one field value, moving its JSON and captures.
    fn into_field(self) -> FieldValue {
        let mut captures = Captures::default();
        for field in self.captures.into_values() {
            captures.absorb(field);
        }
        FieldValue {
            value: Self::written(self.malformed, self.fields, self.flatten),
            captures,
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

/// Generate `From<Value>` and `FromIterator<(String, FieldValue)>` for
/// sub-values held as one [`Body`]: the canonical object form, kept as
/// authored until validation.
macro_rules! canonical_object {
    ($($ty:ty),* $(,)?) => {
        $(
            impl From<Value> for $ty {
                /// The canonical object form, exactly as authored; a value
                /// that is not an object stands as authored, so validation
                /// refuses it as YAML would.
                fn from(value: Value) -> Self {
                    Self {
                        body: $crate::authoring::Body::authored(value),
                    }
                }
            }

            impl FromIterator<(String, $crate::authoring::FieldValue)> for $ty {
                /// The canonical object form with `entries` as its fields,
                /// each keeping what it references; a key given twice keeps
                /// its first value and validation refuses the repeat.
                fn from_iter<I>(entries: I) -> Self
                where
                    I: IntoIterator<Item = (String, $crate::authoring::FieldValue)>,
                {
                    Self {
                        body: $crate::authoring::Body::assembled(entries),
                    }
                }
            }
        )*
    };
}

pub(crate) use canonical_object;

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
#[derive(Debug, Clone, PartialEq, Eq)]
struct Card {
    /// The kind, fixed by the authoring type.
    kind: CardKind,
    /// The name, as authored.
    name: String,
    /// The version, as authored; `None` lets the server select it.
    version: Option<String>,
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
    /// An empty `kind` Card named `name` at `version`, if any.
    fn new(kind: CardKind, name: impl Into<String>, version: Option<&str>) -> Self {
        Self::with_spec(kind, name, version, Body::default())
    }

    /// A `kind` Card named `name` at `version`, if any, with `spec`.
    fn with_spec(
        kind: CardKind,
        name: impl Into<String>,
        version: Option<&str>,
        spec: Body,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            version: version.map(str::to_owned),
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

    /// The exact reference this Card registers as, when its version is set.
    fn card_ref(&self) -> Option<CardRef> {
        self.version
            .as_deref()
            .map(|version| CardRef::new(self.kind.clone(), &self.name, version))
    }

    /// The canonical sibling reference `{kind, name, version?}` naming this
    /// Card within its declaration.
    fn sibling(&self) -> Value {
        let mut sibling = json!({ "kind": self.kind.wire_name(), "name": self.name });
        if let Some(version) = &self.version {
            sibling["version"] = Value::from(version.as_str());
        }
        sibling
    }

    /// The identity two definitions of one Card share: kind, name, and the
    /// version as authored.
    fn identity(&self) -> (&CardKind, &str, Option<&str>) {
        (&self.kind, &self.name, self.version.as_deref())
    }

    /// The label naming this Card in diagnostics, in place of a file path:
    /// `Kind/name`, followed by `@version` when one is set.
    fn label(&self) -> PathBuf {
        let mut label = format!("{}/{}", self.kind.wire_name(), self.name);
        if let Some(version) = &self.version {
            label.push('@');
            label.push_str(version);
        }
        PathBuf::from(label)
    }

    /// The envelope YAML would hold, with every local code file read now.
    ///
    /// # Errors
    /// Returns a diagnostic for each map key added twice and each code file
    /// that cannot be read as UTF-8 text.
    fn envelope(&self) -> Result<Value, Vec<Diagnostic>> {
        let mut spec = self.spec.value();
        let mut diagnostics: Vec<Diagnostic> = self
            .spec
            .duplicates()
            .map(|location| {
                format!(
                    "Invalid spec for kind {}: {}",
                    self.kind.wire_name(),
                    field_path(location)
                )
            })
            .chain(
                self.metadata
                    .duplicates()
                    .map(|location| format!("metadata.{}", field_path(location))),
            )
            .map(|path| {
                Diagnostic::invalid_envelope(
                    self.label(),
                    format!("{path}: duplicate entry; each key may be added once"),
                )
            })
            .collect();
        for (location, file) in self
            .spec
            .captures
            .values()
            .flat_map(|captures| &captures.files)
        {
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
            if let Some(version) = &self.version {
                fields.insert("version".to_owned(), Value::from(version.as_str()));
            }
        }
        Ok(json!({ "kind": self.kind.wire_name(), "metadata": metadata, "spec": spec }))
    }

    /// Decode this Card's envelope as its YAML document would be decoded.
    ///
    /// # Errors
    /// Returns every diagnostic for this Card.
    fn decode(&self) -> Result<wyrd_loader::AuthoredCard, Vec<Diagnostic>> {
        let envelope = self.envelope()?;
        wyrd_loader::parse_card_value(&self.label(), envelope).map_err(|d| vec![d])
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
        let mut card = self.decode()?;
        wyrd_loader::declaration_input_from_cards(std::slice::from_mut(&mut card))
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

/// Generate the methods every named Card type shares: metadata setters,
/// its reference, its authored spec, and validation.
macro_rules! card_methods {
    ($ty:ty) => {
        impl $ty {
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

            /// The exact reference this Card registers as, when its version
            /// is set; without one, the server selects the version at
            /// declaration.
            #[must_use]
            pub fn card_ref(&self) -> Option<$crate::authoring::CardRef> {
                self.card.card_ref()
            }

            /// The spec as authored so far, before validation; a
            /// [`CodeSource::file`](crate::verify::CodeSource::file) position is `null` until its graph is
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
                $crate::authoring::Graph::declaration(&self.card).map(drop)
            }
        }
    };
}

pub(crate) use card_methods;

/// Any one named authored Card, such as a [`Service`] or [`Verifier`], with
/// its kind erased.
///
/// Only the named Card types build one, each converting with what it
/// references. A definition is an immutable snapshot: copying it, or the
/// Card that captures it, shares it and everything it captures instead of
/// copying them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDefinition(
    /// The Card, shared by every copy.
    Arc<Card>,
);

impl CardDefinition {
    /// The exact reference this Card registers as, when its version is set;
    /// without one, the server selects the version at declaration.
    #[must_use]
    pub fn card_ref(&self) -> Option<CardRef> {
        self.0.card_ref()
    }

    /// The spec as authored so far, before validation; a
    /// [`CodeSource::file`](crate::verify::CodeSource::file) position is `null` until its graph is validated.
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
    /// to the same Card are one submission. Siblings are matched among the
    /// whole capture, local Artifacts included, so an omitted sibling naming
    /// a local and another Artifact of one name is refused before anything
    /// publishes. A local Artifact is referenced as a sibling until
    /// [`Declaration::bind_published`] replaces every sibling that matched it
    /// with the published reference, and no local path enters a submission.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] when two different
    /// definitions share one identity, a local Artifact and a Card without
    /// that source included, and `WYRD_LOADER_400_INVALID_ENVELOPE` carrying every diagnostic for
    /// an invalid graph: a malformed or incomplete field by its YAML path, a
    /// repeated key, an unreadable code file, an invalid version, an
    /// ambiguous versionless sibling, and each shared tree validation
    /// refusal.
    pub fn declaration(&self) -> Result<Declaration, WyrdError> {
        Graph::declaration(&self.0)
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
    /// Every authored Card, local Artifacts included, as the submissions
    /// sibling matching selects among; siblings keep the targets they
    /// matched here while their local Artifacts are published. Only kind
    /// and identity are kept, and none when nothing publishes first.
    targets: Vec<CardSubmission>,
}

impl Declaration {
    /// Record that the first of `artifacts`, a local Artifact, was just
    /// published as `published`: remove it and replace every sibling whose
    /// authored target it is with that exact registered reference, in the
    /// Artifacts still to publish and in the graph.
    ///
    /// A sibling is bound by the target shared sibling matching selected
    /// among every authored Card, so a sibling naming another Card of the
    /// same kind and name, or another version, keeps its own target. With
    /// no Artifact left to publish, nothing changes.
    ///
    /// # Errors
    /// Returns [`WyrdError::internal`] when a validated submission no longer
    /// decodes or serializes, or the published Artifact is not one of the
    /// authored targets.
    pub fn bind_published(
        &mut self,
        published: &wyrd_spec::reference::CardRef,
    ) -> Result<(), WyrdError> {
        if self.artifacts.is_empty() {
            return Ok(());
        }
        let artifact = self.artifacts.remove(0);
        let target = self
            .targets
            .iter()
            .position(|target| {
                target.kind == CardKind::Artifact
                    && target.metadata.name == artifact.metadata.name
                    && target.metadata.version == artifact.metadata.version
            })
            .ok_or_else(|| WyrdError::internal("a published local Artifact is not authored"))?;
        let targets = &self.targets;
        let name = artifact.metadata.name.as_str();
        for artifact in &mut self.artifacts {
            let mut spec = Spec::Artifact(std::mem::take(&mut artifact.spec));
            bind_sibling(&mut spec, targets, target, published);
            let Spec::Artifact(bound) = spec else {
                unreachable!("binding keeps the spec's kind");
            };
            artifact.spec = bound;
        }
        let Some(input) = &mut self.input else {
            return Ok(());
        };
        // Only a submission naming an Artifact sibling of this name can hold
        // a sibling matched to it; every other one is left as validated.
        for submission in input
            .submissions
            .iter_mut()
            .filter(|submission| names_artifact_sibling(&submission.spec, name))
        {
            let mut spec = Spec::from_kind_and_value(&submission.kind, submission.spec.take())
                .map_err(|error| {
                    WyrdError::internal(format!("a validated submission does not decode: {error}"))
                })?;
            bind_sibling(&mut spec, targets, target, published);
            submission.spec = serde_json::to_value(&spec).map_err(|error| {
                WyrdError::internal(format!(
                    "a validated submission does not serialize: {error}"
                ))
            })?;
        }
        Ok(())
    }
}

/// Whether `value` holds, at any depth, a sibling reference
/// `{sibling: {kind: Artifact, name}}` naming an Artifact called `name`.
fn names_artifact_sibling(value: &Value, name: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields.get("sibling").is_some_and(|sibling| {
                sibling["kind"] == CardKind::Artifact.wire_name() && sibling["name"] == name
            }) || fields
                .values()
                .any(|value| names_artifact_sibling(value, name))
        }
        Value::Array(items) => items.iter().any(|item| names_artifact_sibling(item, name)),
        _ => false,
    }
}

/// Bind every sibling in `spec` whose matched target among `targets` is
/// `target` to `card_ref`.
fn bind_sibling(
    spec: &mut Spec,
    targets: &[CardSubmission],
    target: usize,
    card_ref: &wyrd_spec::reference::CardRef,
) {
    ReferenceSlotVisitor::visit(spec, |mut slot| {
        if slot
            .value
            .as_sibling()
            .is_some_and(|sibling| sibling_target(sibling, targets) == Ok(Some(target)))
        {
            slot.value.bind_sibling(card_ref.clone());
        }
    });
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

/// The distinct Cards, local Artifacts included, reachable from one root,
/// borrowed from that root's captures rather than copied out of them.
#[derive(Default)]
struct Graph<'a> {
    /// Cards, dependencies first.
    cards: Vec<&'a Card>,
}

impl<'a> Graph<'a> {
    /// Snapshot, decode, and validate the graph `root` roots.
    ///
    /// # Errors
    /// As [`CardDefinition::declaration`].
    fn declaration(root: &'a Card) -> Result<Declaration, WyrdError> {
        let mut graph = Self::default();
        graph.absorb(root)?;
        let has_local = graph.cards.iter().any(|card| card.source.is_some());
        let mut diagnostics = Vec::new();
        let mut every = Vec::new();
        let mut authored = Vec::new();
        let mut artifacts = Vec::new();
        for card in &graph.cards {
            let decoded = match &card.source {
                Some(source) => card.local(source).and_then(|local| {
                    artifacts.push(local);
                    card.decode()
                }),
                // Only a graph that publishes local Artifacts first declares
                // a subset of the whole capture.
                None if has_local => card.decode().inspect(|card| authored.push(card.clone())),
                None => card.decode(),
            };
            match decoded {
                Ok(card) => every.push(card),
                Err(errors) => diagnostics.extend(errors),
            }
        }
        if !diagnostics.is_empty() {
            return Err(LoadError::multiple(diagnostics).into());
        }
        // The whole capture, local Artifacts included, is matched and
        // validated as its YAML tree would be before anything publishes.
        let whole = wyrd_loader::declaration_input_from_cards(&mut every)?;
        if !has_local {
            return Ok(Declaration {
                artifacts,
                input: Some(whole),
                targets: Vec::new(),
            });
        }
        // Sibling matching reads only each target's kind and identity.
        let targets = whole
            .submissions
            .into_iter()
            .map(|target| CardSubmission {
                spec: Value::Null,
                artifacts: Vec::new(),
                ..target
            })
            .collect();
        let input = if authored.is_empty() {
            None
        } else {
            Some(wyrd_loader::declaration_input_from_cards(&mut authored)?)
        };
        Ok(Declaration {
            artifacts,
            input,
            targets,
        })
    }

    /// Add `card` after everything it captures, unless an equal definition
    /// is already added; a copy shares its captures, so most repeats are
    /// recognized by address.
    ///
    /// # Errors
    /// Returns [`WyrdError::SpecDuplicateSubmission`] for a different
    /// definition of an identity already added.
    fn absorb(&mut self, card: &'a Card) -> Result<(), WyrdError> {
        if self.known(card).is_some_and(|known| known == card) {
            return Ok(());
        }
        for definition in card.spec.cards() {
            self.absorb(&definition.0)?;
        }
        match self.known(card) {
            Some(known) if same_content(known, card) => {}
            Some(_) => return Err(conflict(card)),
            None => self.cards.push(card),
        }
        Ok(())
    }

    /// The added Card with `card`'s identity, if any.
    fn known(&self, card: &Card) -> Option<&'a Card> {
        self.cards
            .iter()
            .copied()
            .find(|known| known.identity() == card.identity())
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
    if left == right {
        return true;
    }
    match (left.decode(), right.decode()) {
        (Ok(left), Ok(right)) => left.metadata == right.metadata && left.spec == right.spec,
        _ => left == right,
    }
}

/// The refusal of two different definitions of `card`'s identity.
fn conflict(card: &Card) -> WyrdError {
    let card_ref = card.label().display().to_string();
    WyrdError::SpecDuplicateSubmission {
        message: format!("{card_ref} is defined twice with different content"),
        details: json!({ "card_ref": card_ref }),
    }
}

/// Offline authoring tests.
#[cfg(test)]
mod tests;
