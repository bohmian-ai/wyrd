//! Thin napi projection of `wyrd_client::verify`'s Card definitions for the
//! `wyrd-ai` `verify` namespace.
//!
//! [`NativeAuthored`] holds one shared Rust authoring value: one of the
//! eleven named Cards, a sub-value, a [`CardRef`], an [`ArtifactFile`], a
//! [`CodeSource`], or an original [`Prompt`] held inline at an Agent. Its fields are edited by their YAML names through
//! `set`, `add`, and `insert`, which dispatch to the shared setters and add
//! methods; the TypeScript wrappers spell those as typed camelCase methods.
//! Arguments arrive as one [`NativeValue`]: canonical JSON plus the
//! authoring values standing at positions inside it, each copied when the
//! value is composed, so later edits to a child change nothing already
//! passed and a receiver is never borrowed while its argument converts.
//! Construction and editing never validate, read files, or touch the
//! network; validation and declaration run the shared Rust owner.

use napi::Either;
use napi::{Error, Result, Status};
use napi_derive::napi;
use serde_json::Value;
use wyrd_client::verify::{
    Action, Agent, Artifact, ArtifactFile, Audit, Binding, CardDefinition, CardRef, Check,
    CodeSource, Component, Data, Experiment, FieldValue, InputSlot, Mcp, Model, OpenAiChatOptions,
    Operator, Prompt, PromptBuilderError, Requirement, ResponseFormat, Service, Step, Trigger,
    Verifier, openai_chat,
};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;

use crate::{NativeLifecycleResult, NativeWyrdError};

/// The refusal of a host argument that does not fit its position.
fn invalid(message: impl Into<String>) -> Error {
    Error::new(Status::InvalidArg, message.into())
}

/// The refusal of a field name that `owner` does not have.
fn unknown(owner: &str, field: &str) -> Error {
    invalid(format!("{owner} has no field `{field}`"))
}

/// One step of a position inside a composed value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// An object key.
    Key(String),
    /// A list position.
    Index(usize),
}

/// One shared authoring value of any family.
#[derive(Debug, Clone)]
enum Authored {
    /// A Service Card.
    Service(Service),
    /// An Agent Card.
    Agent(Agent),
    /// A Verifier Card.
    Verifier(Verifier),
    /// An Artifact Card.
    Artifact(Artifact),
    /// A Model Card.
    Model(Model),
    /// A Data Card.
    Data(Data),
    /// An Experiment Card.
    Experiment(Experiment),
    /// An Mcp Card.
    Mcp(Mcp),
    /// An Audit Card.
    Audit(Audit),
    /// A Trigger Card.
    Trigger(Trigger),
    /// An Operator Card.
    Operator(Operator),
    /// A Service component.
    Component(Component),
    /// A verification binding.
    Binding(Binding),
    /// A Verifier step.
    Step(Step),
    /// A requirement.
    Requirement(Requirement),
    /// A Verifier input slot.
    InputSlot(InputSlot),
    /// A step action.
    Action(Action),
    /// A requirement check.
    Check(Check),
    /// An exact Card reference.
    CardRef(CardRef),
    /// A fixed Artifact file selection.
    ArtifactFile(ArtifactFile),
    /// A code source.
    CodeSource(CodeSource),
    /// An original Prompt, held inline at an Agent's `prompt`.
    Prompt(Prompt),
}

/// Generate [`Authored`]'s per-family dispatch: the family name, the field
/// edits, the Card it is, and the value it contributes at a position.
macro_rules! families {
    (cards { $($card:ident),* } values { $($value:ident),* }) => {
        impl Authored {
            /// The family's public name, for argument errors.
            fn name(&self) -> &'static str {
                match self {
                    $(Self::$card(_) => stringify!($card),)*
                    $(Self::$value(_) => stringify!($value),)*
                }
            }

            /// This value with `field` set from `value`, as its setter sets it.
            ///
            /// # Errors
            /// Returns `InvalidArg` for a field the family does not have or a
            /// value that does not convert; nothing is then set.
            fn set(self, field: &str, value: NativeValue) -> Result<Self> {
                Ok(match self {
                    $(Self::$card(inner) => Self::$card(inner.set(field, value)?),)*
                    $(Self::$value(inner) => Self::$value(inner.set(field, value)?),)*
                })
            }

            /// Append one entry to the list `field`, as its add method does.
            ///
            /// # Errors
            /// As [`Self::set`].
            fn add(&mut self, field: &str, value: NativeValue) -> Result<()> {
                match self {
                    $(Self::$card(inner) => inner.add(field, value),)*
                    $(Self::$value(inner) => inner.add(field, value),)*
                }
            }

            /// Insert the entry `key` into the map `field`, as its add method
            /// does.
            ///
            /// # Errors
            /// As [`Self::set`].
            fn insert(&mut self, field: &str, key: String, value: NativeValue) -> Result<()> {
                match self {
                    $(Self::$card(inner) => inner.insert(field, key, value),)*
                    $(Self::$value(inner) => inner.insert(field, key, value),)*
                }
            }

            /// A copy of the Card this value is, if it is one.
            fn definition(&self) -> Option<CardDefinition> {
                match self {
                    $(Self::$card(card) => Some(card.into()),)*
                    $(Self::$value(_) => None,)*
                }
            }

            /// The value this contributes at a position: a Card becomes a
            /// sibling reference, every other value its canonical JSON.
            fn into_field(self) -> FieldValue {
                match self {
                    $(Self::$card(inner) => inner.into(),)*
                    $(Self::$value(inner) => inner.into(),)*
                }
            }
        }
    };
}

families!(
    cards { Service, Agent, Verifier, Artifact, Model, Data, Experiment, Mcp, Audit, Trigger, Operator }
    values {
        Component, Binding, Step, Requirement, InputSlot, Action, Check, CardRef, ArtifactFile,
        CodeSource, Prompt
    }
);

/// A host argument: canonical JSON plus the authoring values standing at
/// positions inside it, each a copy taken when the argument was composed.
#[napi]
#[derive(Debug, Clone)]
pub struct NativeValue {
    /// The JSON, with `null` at every authoring value's position.
    value: Value,
    /// Each authoring value and its position inside `value`.
    holes: Vec<(Vec<Segment>, Authored)>,
}

#[napi]
impl NativeValue {
    /// Compose an argument from JSON and the authoring values at `paths`.
    ///
    /// # Arguments
    ///
    /// * `value` - The JSON, as the TypeScript wrapper snapshotted it.
    /// * `paths` - One position per authoring value: a list of object keys
    ///   (strings) and list indices (numbers) from the root.
    /// * `holes` - The authoring values, in `paths` order; each is copied now.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when `paths` and `holes` differ in length or a
    /// path segment is neither a string nor a list index.
    #[napi(factory)]
    pub fn compose(value: Value, paths: Vec<Value>, holes: Vec<&NativeAuthored>) -> Result<Self> {
        if paths.len() != holes.len() {
            return Err(invalid("each authoring value needs exactly one path"));
        }
        let holes = paths
            .into_iter()
            .zip(holes)
            .map(|(path, hole)| {
                let Value::Array(segments) = path else {
                    return Err(invalid("an authoring value's path is a list"));
                };
                let path = segments
                    .into_iter()
                    .map(|segment| match segment {
                        Value::String(key) => Ok(Segment::Key(key)),
                        Value::Number(index) => index
                            .as_u64()
                            .and_then(|index| usize::try_from(index).ok())
                            .map(Segment::Index)
                            .ok_or_else(|| invalid("a list index is a non-negative integer")),
                        _ => Err(invalid("a path segment is a key or a list index")),
                    })
                    .collect::<Result<_>>()?;
                Ok((path, hole.inner.clone()))
            })
            .collect::<Result<_>>()?;
        Ok(Self { value, holes })
    }
}

/// One argument split at its root.
enum Parts {
    /// An authoring value standing for the whole argument.
    Hole(Box<Authored>),
    /// Plain JSON with no authoring value inside.
    Json(Value),
    /// A list with authoring values inside, item by item.
    List(Vec<NativeValue>),
    /// An object with authoring values inside, entry by entry.
    Object(Vec<(String, NativeValue)>),
}

impl NativeValue {
    /// Plain JSON with no authoring value inside.
    fn plain(value: Value) -> Self {
        Self {
            value,
            holes: Vec::new(),
        }
    }

    /// The part of this argument at `segment`, taking its authoring values.
    fn child(&mut self, segment: &Segment, value: Value) -> Self {
        let (inside, outside) = std::mem::take(&mut self.holes)
            .into_iter()
            .partition::<Vec<_>, _>(|(path, _)| path.first() == Some(segment));
        self.holes = outside;
        Self {
            value,
            holes: inside
                .into_iter()
                .map(|(mut path, hole)| {
                    path.remove(0);
                    (path, hole)
                })
                .collect(),
        }
    }

    /// Split this argument at its root.
    ///
    /// # Errors
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the JSON.
    fn split(mut self) -> Result<Parts> {
        if let Some(index) = self.holes.iter().position(|(path, _)| path.is_empty()) {
            return Ok(Parts::Hole(Box::new(self.holes.swap_remove(index).1)));
        }
        if self.holes.is_empty() {
            return Ok(Parts::Json(self.value));
        }
        let parts = match std::mem::take(&mut self.value) {
            Value::Array(items) => Parts::List(
                items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| self.child(&Segment::Index(index), item))
                    .collect(),
            ),
            Value::Object(map) => Parts::Object(
                map.into_iter()
                    .map(|(key, value)| {
                        let child = self.child(&Segment::Key(key.clone()), value);
                        (key, child)
                    })
                    .collect(),
            ),
            _ => return Err(invalid("an authoring value's path names no position")),
        };
        if self.holes.is_empty() {
            Ok(parts)
        } else {
            Err(invalid("an authoring value's path names no position"))
        }
    }

    /// The argument at any authoring position: an authoring value as its own
    /// contribution, and JSON with each nested value at its position.
    ///
    /// # Errors
    /// As [`Self::split`].
    fn field(self) -> Result<FieldValue> {
        Ok(match self.split()? {
            Parts::Hole(hole) => hole.into_field(),
            Parts::Json(value) => FieldValue::from(value),
            Parts::List(items) => items
                .into_iter()
                .map(Self::field)
                .collect::<Result<FieldValue>>()?,
            Parts::Object(entries) => entries
                .into_iter()
                .map(|(key, value)| Ok((key, value.field()?)))
                .collect::<Result<FieldValue>>()?,
        })
    }

    /// The argument as plain JSON.
    ///
    /// # Errors
    /// Returns `InvalidArg` naming `field` when an authoring value stands
    /// inside it.
    fn json(self, field: &str) -> Result<Value> {
        match self.holes.first() {
            None => Ok(self.value),
            Some((_, hole)) => Err(invalid(format!(
                "{field} takes JSON, not a {}",
                hole.name()
            ))),
        }
    }

    /// The argument's items when it is a list.
    ///
    /// # Errors
    /// Returns `InvalidArg` naming `field` for any other value.
    fn items(self, field: &str) -> Result<Vec<Self>> {
        match self.split()? {
            Parts::Json(Value::Array(items)) => Ok(items.into_iter().map(Self::plain).collect()),
            Parts::List(items) => Ok(items),
            _ => Err(invalid(format!("{field} takes a list"))),
        }
    }

    /// The argument's entries when it is a map.
    ///
    /// # Errors
    /// Returns `InvalidArg` naming `field` for any other value.
    fn entries(self, field: &str) -> Result<Vec<(String, Self)>> {
        match self.split()? {
            Parts::Json(Value::Object(map)) => Ok(map
                .into_iter()
                .map(|(key, value)| (key, Self::plain(value)))
                .collect()),
            Parts::Object(entries) => Ok(entries),
            _ => Err(invalid(format!("{field} takes a map"))),
        }
    }

    /// A sub-value of type `T`: the matching authoring value, or its
    /// canonical object with each nested value at its position, kept as
    /// authored until validation.
    ///
    /// # Errors
    /// Returns `InvalidArg` naming `field` for an authoring value of another
    /// family or a list holding authoring values.
    fn canonical<T>(self, field: &str, pick: fn(Authored) -> Option<T>) -> Result<T>
    where
        T: From<Value> + FromIterator<(String, FieldValue)>,
    {
        match self.split()? {
            Parts::Hole(hole) => {
                let name = hole.name();
                pick(*hole).ok_or_else(|| invalid(format!("{field} does not take a {name}")))
            }
            Parts::Json(value) => Ok(T::from(value)),
            Parts::Object(entries) => entries
                .into_iter()
                .map(|(key, value)| Ok((key, value.field()?)))
                .collect(),
            Parts::List(_) => Err(invalid(format!("{field} takes an object"))),
        }
    }
}

/// Converts one argument for the field `field` into a native value.
type Convert<T> = fn(&str, NativeValue) -> Result<T>;

/// `value` as plain JSON.
///
/// # Errors
/// As [`NativeValue::json`].
fn json(field: &str, value: NativeValue) -> Result<Value> {
    value.json(field)
}

/// `value` as a string.
///
/// # Errors
/// Returns `InvalidArg` naming `field` for any other value.
fn string(field: &str, value: NativeValue) -> Result<String> {
    match value.json(field)? {
        Value::String(text) => Ok(text),
        _ => Err(invalid(format!("{field} takes a string"))),
    }
}

/// `value` at any authoring position.
///
/// # Errors
/// As [`NativeValue::split`].
fn field(_field: &str, value: NativeValue) -> Result<FieldValue> {
    value.field()
}

/// An input slot: an `InputSlot`, or a source slot's reference string, a
/// canonical slot object, or a bare JSON Schema, as the shared `InputSlot`
/// conversion reads them.
///
/// # Errors
/// Returns `InvalidArg` naming `field` for another authoring value.
fn slot(field: &str, value: NativeValue) -> Result<InputSlot> {
    match value.split()? {
        Parts::Json(value) => Ok(InputSlot::from(value)),
        Parts::Hole(hole) => match *hole {
            Authored::InputSlot(slot) => Ok(slot),
            other => Err(invalid(format!("{field} does not take a {}", other.name()))),
        },
        Parts::List(_) | Parts::Object(_) => Err(invalid(format!("{field} takes JSON"))),
    }
}

/// A verification binding: a `Binding`, its canonical object, or a
/// Verifier, `CardRef` or reference string bound with no trigger or
/// Operators.
///
/// # Errors
/// As [`NativeValue::split`].
fn binding(field: &str, value: NativeValue) -> Result<Binding> {
    if value.value.is_object() {
        return value.canonical(field, |hole| match hole {
            Authored::Binding(binding) => Some(binding),
            other => Some(Binding::new(other.into_field())),
        });
    }
    match value.split()? {
        Parts::Hole(hole) => match *hole {
            Authored::Binding(binding) => Ok(binding),
            other => Ok(Binding::new(other.into_field())),
        },
        Parts::Json(value) => Ok(Binding::new(value)),
        Parts::List(_) | Parts::Object(_) => Err(invalid(format!("{field} takes a binding"))),
    }
}

/// Generate the converter for a sub-value: its family, or its canonical
/// object kept as authored.
macro_rules! canonical {
    ($($name:ident => $native:ident),* $(,)?) => {
        $(
            #[doc = concat!("A `", stringify!($native), "`, or its canonical object kept as authored.")]
            ///
            /// # Errors
            /// As [`NativeValue::canonical`].
            fn $name(field: &str, value: NativeValue) -> Result<$native> {
                value.canonical(field, |hole| match hole {
                    Authored::$native(inner) => Some(inner),
                    _ => None,
                })
            }
        )*
    };
}

canonical!(
    component => Component,
    step => Step,
    requirement => Requirement,
    action => Action,
    check => Check,
);

/// The items of a list argument, each converted by `convert`.
///
/// # Errors
/// As [`NativeValue::items`] and `convert`.
fn items<T>(field: &str, value: NativeValue, convert: Convert<T>) -> Result<Vec<T>> {
    value
        .items(field)?
        .into_iter()
        .map(|item| convert(field, item))
        .collect()
}

/// The entries of a map argument, each value converted by `convert`.
///
/// # Errors
/// As [`NativeValue::entries`] and `convert`.
fn entries<T>(field: &str, value: NativeValue, convert: Convert<T>) -> Result<Vec<(String, T)>> {
    value
        .entries(field)?
        .into_iter()
        .map(|(key, value)| Ok((key, convert(field, value)?)))
        .collect()
}

/// Field edits by YAML name, dispatched to one family's shared setters and
/// add methods.
trait Fields: Sized {
    /// The family's public name, for argument errors.
    const NAME: &'static str;

    /// This value with `field` set from `value`.
    ///
    /// # Errors
    /// Returns `InvalidArg` for a field the family does not have or a value
    /// that does not convert.
    fn set(self, field: &str, value: NativeValue) -> Result<Self>;

    /// Append `value` to the list `field`.
    ///
    /// # Errors
    /// As [`Fields::set`].
    fn add(&mut self, field: &str, value: NativeValue) -> Result<()>;

    /// Insert the entry `key` into the map `field`.
    ///
    /// # Errors
    /// As [`Fields::set`].
    fn insert(&mut self, field: &str, key: String, value: NativeValue) -> Result<()>;
}

/// Generate [`Fields`] for one family from its one-value, list, and map
/// fields and their converters.
macro_rules! fields {
    ($(
        $ty:ident {
            one { $($one:ident: $one_convert:ident),* $(,)? }
            list { $($list:ident, $list_add:ident: $list_convert:ident),* $(,)? }
            map { $($map:ident, $map_add:ident: $map_convert:ident),* $(,)? }
        }
    )*) => {
        $(
            impl Fields for $ty {
                const NAME: &'static str = stringify!($ty);

                fn set(self, field: &str, value: NativeValue) -> Result<Self> {
                    match field {
                        $(stringify!($one) => Ok(self.$one($one_convert(field, value)?)),)*
                        $(stringify!($list) => Ok(self.$list(items(field, value, $list_convert)?)),)*
                        $(stringify!($map) => Ok(self.$map(entries(field, value, $map_convert)?)),)*
                        _ => {
                            drop(value);
                            Err(unknown(Self::NAME, field))
                        }
                    }
                }

                fn add(&mut self, field: &str, value: NativeValue) -> Result<()> {
                    match field {
                        $(stringify!($list) => {
                            self.$list_add($list_convert(field, value)?);
                            Ok(())
                        })*
                        _ => {
                            drop(value);
                            Err(unknown(Self::NAME, field))
                        }
                    }
                }

                fn insert(&mut self, field: &str, key: String, value: NativeValue) -> Result<()> {
                    match field {
                        $(stringify!($map) => {
                            self.$map_add(key, $map_convert(field, value)?);
                            Ok(())
                        })*
                        _ => {
                            drop((key, value));
                            Err(unknown(Self::NAME, field))
                        }
                    }
                }
            }
        )*
    };
}

fields! {
    Service {
        one {
            description: json, service_type: json, entry_point: json, runtime: json,
            content_hash: json, lock_hash: json,
        }
        list {
            components, add_component: component, verified_by, add_verified_by: binding,
            tables, add_table: json, credential_refs, add_credential_ref: json,
        }
        map {
            deployment, add_deployment: json, service_config, add_service_config: json,
            metadata, add_metadata: json, labels, add_label: string,
            annotations, add_annotation: string,
        }
    }
    Agent {
        one { prompt: field, execution_route: json, run_config: json }
        list { tool_names, add_tool_name: string, verified_by, add_verified_by: binding }
        map { labels, add_label: string, annotations, add_annotation: string }
    }
    Verifier {
        one { description: json, decide: json, context_capture: json, order_by: json }
        list { steps, add_step: step, requirements, add_requirement: requirement }
        map { inputs, add_input: slot, labels, add_label: string, annotations, add_annotation: string }
    }
    Artifact {
        one {
            artifact_kind: json, content_type: json, size_bytes: json, integrity: json,
            schema_ref: field, framework_adapter: json, external_uri: json,
        }
        list { artifact_uris, add_artifact_uri: json }
        map {
            metadata, add_metadata: json, labels, add_label: string,
            annotations, add_annotation: string,
        }
    }
    Model {
        one { interface: json, task_type: json, signature: json, sample_input: json }
        list { card_refs, add_card_ref: field }
        map { labels, add_label: string, annotations, add_annotation: string }
    }
    Data {
        one { interface: json, schema: json, sql: json, stats: json }
        list { card_refs, add_card_ref: field, target_columns, add_target_column: json }
        map { splits, add_split: field, labels, add_label: string, annotations, add_annotation: string }
    }
    Experiment {
        one { experiment_type: json, description: json, best_run_ref: json }
        list {
            target_refs, add_target_ref: field, run_refs, add_run_ref: json,
            summary_metrics, add_summary_metric: json, card_refs, add_card_ref: field,
        }
        map {
            default_parameters, add_default_parameter: json, details, add_detail: json,
            labels, add_label: string, annotations, add_annotation: string,
        }
    }
    Mcp {
        one { description: json, server_name: json, transport: json }
        list { tool_refs, add_tool_ref: field, scopes, add_scope: json }
        map { details, add_detail: json, labels, add_label: string, annotations, add_annotation: string }
    }
    Audit {
        one { description: json }
        list {
            subject_refs, add_subject_ref: field, policy_refs, add_policy_ref: field,
            evidence_refs, add_evidence_ref: field,
        }
        map { details, add_detail: json, labels, add_label: string, annotations, add_annotation: string }
    }
    Trigger {
        one { description: json, kind: json, cron: json, tz: json, after: json }
        list {}
        map { labels, add_label: string, annotations, add_annotation: string }
    }
    Operator {
        one { description: json, action: json, budget: json }
        list {}
        map { labels, add_label: string, annotations, add_annotation: string }
    }
    Component {
        one { source: json }
        list { verified_by, add_verified_by: binding, credential_refs, add_credential_ref: json }
        map { config, add_config: json }
    }
    Binding {
        one { runs_on: field }
        list { on_failure, add_on_failure: field, on_verdict_change, add_on_verdict_change: field }
        map {}
    }
    Step {
        one { when: json, timeout_seconds: json, retries: json }
        list { depends_on, add_depends_on: json, requirements, add_requirement: requirement }
        map {}
    }
    Requirement { one {} list {} map {} }
    InputSlot { one { max_bytes: json } list {} map {} }
    Action {
        one { runtime: json }
        list {}
        map { inputs, bind_input: json, workspace, add_workspace: field }
    }
    Check {
        one { runtime: json, response_format: json }
        list {}
        map { inputs, bind_input: json, workspace, add_workspace: field }
    }
    CardRef { one {} list {} map {} }
    ArtifactFile { one {} list {} map {} }
    CodeSource { one {} list {} map {} }
    Prompt { one {} list {} map {} }
}

/// The Card kind named `kind`.
///
/// # Errors
/// Returns `InvalidArg` for a name that is no Card kind.
fn kind_arg(kind: &str) -> Result<CardKind> {
    CardKind::from_wire_name(kind).ok_or_else(|| invalid(format!("unknown Card kind {kind:?}")))
}

/// The OpenAI Chat options `Prompt.openaiChat` takes, by their builder
/// names.
#[napi(object)]
pub struct NativeOpenAiChatOptions {
    /// System message prepended to `messages`.
    pub system: Option<String>,
    /// User message strings, each one native user message.
    pub messages: Option<Vec<String>>,
    /// The response format emitted into the native request.
    pub response_format: Option<NativeResponseFormat>,
    /// OpenAI prompt cache key.
    pub prompt_cache_key: Option<String>,
    /// Native OpenAI Chat generation settings, by their wire names.
    pub settings: Option<Value>,
}

/// One response format: `text`, `json_object`, `json_schema` with its
/// `name` and `schema`, or the canonical `verdict` with optional custom
/// `data` schema.
#[napi(object)]
pub struct NativeResponseFormat {
    /// The format's name.
    pub r#type: String,
    /// A JSON Schema format's name.
    pub name: Option<String>,
    /// A JSON Schema format's schema.
    pub schema: Option<Value>,
    /// A Verdict's custom data schema.
    pub data: Option<Value>,
}

impl NativeResponseFormat {
    /// The shared format this names; a JSON Schema format's schema is
    /// checked by the builder, so its refusal is the inner error.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for an unknown `type`, a JSON Schema format
    /// without `name` or `schema`, or a Verdict `data` that is not an
    /// object.
    fn native(self) -> Result<std::result::Result<ResponseFormat, PromptBuilderError>> {
        Ok(match self.r#type.as_str() {
            "text" => Ok(ResponseFormat::text()),
            "json_object" => Ok(ResponseFormat::json_object()),
            "json_schema" => {
                let (Some(name), Some(schema)) = (self.name, self.schema) else {
                    return Err(invalid(
                        "a json_schema response format takes name and schema",
                    ));
                };
                ResponseFormat::json_schema(name, schema)
            }
            "verdict" => Ok(ResponseFormat::verdict(match self.data {
                None => None,
                Some(Value::Object(data)) => Some(data),
                Some(_) => return Err(invalid("a verdict's data takes a JSON Schema object")),
            })),
            other => return Err(invalid(format!("unknown response format {other:?}"))),
        })
    }
}

/// An exact Card reference's identity, as authored.
#[napi(object, object_from_js = false)]
pub struct NativeCardIdentity {
    /// The referenced kind's wire name.
    pub kind: String,
    /// The referenced name.
    pub name: String,
    /// The referenced exact version.
    pub version: String,
}

/// One shared Rust authoring value, edited in place by its YAML field names.
#[napi]
pub struct NativeAuthored {
    /// The shared native value.
    inner: Authored,
}

impl NativeAuthored {
    /// The authored Card this value is, copied as it is now, for
    /// declaration.
    ///
    /// # Errors
    /// Returns `InvalidArg` when this value is not a Card.
    pub(crate) fn declarable(&self) -> Result<CardDefinition> {
        self.inner
            .definition()
            .ok_or_else(|| invalid(format!("a {} is not a Card", self.inner.name())))
    }

    /// The Node-facing handle owning `inner`; each JS wrapper holds exactly
    /// one, and arguments copy out of it rather than borrow it.
    fn of(inner: Authored) -> Self {
        Self { inner }
    }
}

#[napi]
impl NativeAuthored {
    /// An empty `kind` Card named `name`, at `version` when given, else at
    /// the version registration assigns; nothing is validated until its
    /// graph is.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for a kind that is not an authorable Card kind.
    #[napi(factory)]
    pub fn card(kind: String, name: String, version: Option<String>) -> Result<Self> {
        let version = version.as_deref();
        Ok(Self::of(match kind_arg(&kind)? {
            CardKind::Service => Authored::Service(Service::new(name, version)),
            CardKind::Agent => Authored::Agent(Agent::new(name, version)),
            CardKind::Verifier => Authored::Verifier(Verifier::new(name, version)),
            CardKind::Artifact => Authored::Artifact(Artifact::new(name, version)),
            CardKind::Model => Authored::Model(Model::new(name, version)),
            CardKind::Data => Authored::Data(Data::new(name, version)),
            CardKind::Experiment => Authored::Experiment(Experiment::new(name, version)),
            CardKind::Mcp => Authored::Mcp(Mcp::new(name, version)),
            CardKind::Audit => Authored::Audit(Audit::new(name, version)),
            CardKind::Trigger => Authored::Trigger(Trigger::new(name, version)),
            CardKind::Operator => Authored::Operator(Operator::new(name, version)),
            other => {
                return Err(invalid(format!(
                    "{} is not an authorable Card kind",
                    other.wire_name()
                )));
            }
        }))
    }

    /// The `file` Artifact named `name`, at `version` when given, published
    /// from the file or directory at `path` before its graph; nothing is
    /// read now.
    #[napi(factory)]
    pub fn artifact_from_path(name: String, version: Option<String>, path: String) -> Self {
        Self::of(Authored::Artifact(Artifact::from_path(
            name,
            version.as_deref(),
            path,
        )))
    }

    /// The original native OpenAI Chat Prompt for `model`, built by the
    /// shared Prompt builder, or the builder's stable refusal.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for `settings` that are not OpenAI Chat
    /// settings or a `response_format` that names no format; the builder's
    /// own refusal, such as `WYRD_PROMPT_400_EMPTY_MODEL`, is returned as
    /// the error value.
    #[napi]
    pub fn prompt_openai_chat(
        model: String,
        options: NativeOpenAiChatOptions,
    ) -> Result<Either<NativeAuthored, NativeWyrdError>> {
        let settings = options
            .settings
            .map(serde_json::from_value)
            .transpose()
            .map_err(|error| invalid(format!("settings: {error}")))?
            .unwrap_or_default();
        let response_format = options
            .response_format
            .map(NativeResponseFormat::native)
            .transpose()?;
        let built = response_format.transpose().and_then(|response_format| {
            openai_chat(
                model,
                OpenAiChatOptions {
                    system: options.system,
                    messages: options.messages.unwrap_or_default(),
                    response_format,
                    prompt_cache_key: options.prompt_cache_key,
                    settings,
                    ..OpenAiChatOptions::default()
                },
            )
        });
        Ok(match built {
            Ok(prompt) => Either::A(Self::of(Authored::Prompt(prompt))),
            Err(error) => Either::B(NativeWyrdError::from_wyrd(&WyrdError::from(error))),
        })
    }

    /// The component `alias` for `reference`: a Card, an exact `CardRef`,
    /// or a reference path.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn component(alias: String, reference: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Component(Component::new(
            alias,
            reference.clone().field()?,
        ))))
    }

    /// Bind `verifier`: a Verifier, an exact `CardRef`, or a reference path.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn binding(verifier: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Binding(Binding::new(
            verifier.clone().field()?,
        ))))
    }

    /// The step `id` running `action`: an `Action` or its canonical object.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for an authoring value that is not an Action.
    #[napi(factory)]
    pub fn step(id: String, action: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Step(Step::new(
            id,
            self::action("action", action.clone())?,
        ))))
    }

    /// The requirement `id` claiming `statement`, judged by `check`: a
    /// `Check` or its canonical object.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for an authoring value that is not a Check.
    #[napi(factory)]
    pub fn requirement(id: String, statement: String, check: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Requirement(Requirement::new(
            id,
            statement,
            self::check("check", check.clone())?,
        ))))
    }

    /// A JSON value constrained by the JSON Schema `schema`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for a schema holding an authoring value.
    #[napi(factory)]
    pub fn slot_json(schema: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::InputSlot(InputSlot::json(
            schema.clone().json("schema")?,
        ))))
    }

    /// A pinned file in one of `formats`, such as `"pdf"`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when `formats` is not a list of JSON values.
    #[napi(factory)]
    pub fn slot_media(formats: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::InputSlot(InputSlot::media(items(
            "formats",
            formats.clone(),
            json,
        )?))))
    }

    /// A trace resolved through tenant-scoped trace reads.
    #[napi(factory)]
    pub fn slot_trace() -> Self {
        Self::of(Authored::InputSlot(InputSlot::trace()))
    }

    /// Recorded measurements of `features` for a scheduled window.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when `features` is not a list of JSON values.
    #[napi(factory)]
    pub fn slot_samples(features: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::InputSlot(InputSlot::samples(items(
            "features",
            features.clone(),
            json,
        )?))))
    }

    /// A group Verifier's input authored as the source reference `text`.
    #[napi(factory)]
    pub fn slot_source(text: String) -> Self {
        Self::of(Authored::InputSlot(InputSlot::source(text)))
    }

    /// Extract text and structure from the media slot `from`.
    #[napi(factory)]
    pub fn action_extract(from: String) -> Self {
        Self::of(Authored::Action(Action::extract(from)))
    }

    /// Invoke `agent`: an Agent, an exact `CardRef`, a reference path, or
    /// inline Agent JSON.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn action_agent(agent: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Action(Action::agent(
            agent.clone().field()?,
        ))))
    }

    /// Run `entrypoint` of `source` as sandboxed code returning JSON data.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn action_code(source: &NativeValue, entrypoint: String) -> Result<Self> {
        Ok(Self::of(Authored::Action(Action::code(
            source.clone().field()?,
            entrypoint,
        ))))
    }

    /// Compare the one value at `path` with `expected` by `operator`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for an operator or expected value holding an
    /// authoring value.
    #[napi(factory)]
    pub fn check_assert(
        path: String,
        operator: &NativeValue,
        expected: &NativeValue,
    ) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::assert(
            path,
            operator.clone().json("operator")?,
            expected.clone().json("expected")?,
        ))))
    }

    /// Compare every value `each` selects through `[*]` with `expected` by
    /// `operator`.
    ///
    /// # Errors
    ///
    /// As [`Self::check_assert`].
    #[napi(factory)]
    pub fn check_each(
        each: String,
        operator: &NativeValue,
        expected: &NativeValue,
    ) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::each(
            each,
            operator.clone().json("operator")?,
            expected.clone().json("expected")?,
        ))))
    }

    /// Compare the spans `span_selector` selects in the trace slot `on` with
    /// `expected` by `operator`.
    ///
    /// # Errors
    ///
    /// As [`Self::check_assert`].
    #[napi(factory)]
    pub fn check_trace(
        on: String,
        span_selector: String,
        operator: &NativeValue,
        expected: &NativeValue,
    ) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::trace(
            on,
            span_selector,
            operator.clone().json("operator")?,
            expected.clone().json("expected")?,
        ))))
    }

    /// Judge with `agent`, which returns the Verdict schema.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn check_agent(agent: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::agent(
            agent.clone().field()?,
        ))))
    }

    /// Judge with `entrypoint` of `source`, which returns the Verdict schema.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn check_code(source: &NativeValue, entrypoint: String) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::code(
            source.clone().field()?,
            entrypoint,
        ))))
    }

    /// Judge statistical drift over the samples slot `on` with the drift
    /// fields `spec`: method, signal, condition, and profile, with a Card or
    /// exact `CardRef` at `signal.baseline_ref`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when an authoring value's path names no
    /// position inside the argument.
    #[napi(factory)]
    pub fn check_drift(on: String, spec: &NativeValue) -> Result<Self> {
        Ok(Self::of(Authored::Check(Check::drift(
            on,
            spec.clone().field()?,
        ))))
    }

    /// The exact reference to the `kind` Card `name` at `version`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for an unknown kind.
    #[napi(factory)]
    pub fn reference(kind: String, name: String, version: String) -> Result<Self> {
        Ok(Self::of(Authored::CardRef(CardRef::new(
            kind_arg(&kind)?,
            name,
            version,
        ))))
    }

    /// Run `text`, embedded in the Card as `{inline: text}`.
    #[napi(factory)]
    pub fn code_inline(text: String) -> Self {
        Self::of(Authored::CodeSource(CodeSource::inline(text)))
    }

    /// Run the UTF-8 file at `path`, read when the graph is validated or
    /// declared; nothing is read now.
    #[napi(factory)]
    pub fn code_file(path: String) -> Self {
        Self::of(Authored::CodeSource(CodeSource::file(path)))
    }

    /// Set the field `field` from `value`, replacing its earlier value and
    /// whatever it referenced.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` for a field this value does not have or a value
    /// that does not convert; nothing is then set.
    #[napi]
    pub fn set(&mut self, field: String, value: &NativeValue) -> Result<()> {
        self.inner = self.inner.clone().set(&field, value.clone())?;
        Ok(())
    }

    /// Append `value` to the list `field`; a repeated ID is refused at
    /// validation.
    ///
    /// # Errors
    ///
    /// As [`Self::set`]; nothing is then appended.
    #[napi]
    pub fn add(&mut self, field: String, value: &NativeValue) -> Result<()> {
        self.inner.add(&field, value.clone())
    }

    /// Insert the entry `key` into the map `field`; a repeated key is
    /// refused at validation.
    ///
    /// # Errors
    ///
    /// As [`Self::set`]; nothing is then inserted.
    #[napi]
    pub fn insert(&mut self, field: String, key: String, value: &NativeValue) -> Result<()> {
        self.inner.insert(&field, key, value.clone())
    }

    /// Set whether supported documents are extracted beside their originals
    /// when this local Artifact is published.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not an Artifact.
    #[napi]
    pub fn extract(&mut self, extract: bool) -> Result<()> {
        match &mut self.inner {
            Authored::Artifact(artifact) => {
                *artifact = artifact.clone().extract(extract);
                Ok(())
            }
            other => Err(invalid(format!("a {} has no extract", other.name()))),
        }
    }

    /// Select the fixed file or directory prefix `path` of this exact
    /// Artifact version; a local Artifact is published first.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is neither an Artifact nor a
    /// `CardRef`.
    #[napi]
    pub fn file(&self, path: String) -> Result<Self> {
        match &self.inner {
            Authored::Artifact(artifact) => {
                Ok(Self::of(Authored::ArtifactFile(artifact.file(path))))
            }
            Authored::CardRef(card_ref) => {
                Ok(Self::of(Authored::ArtifactFile(card_ref.file(path))))
            }
            other => Err(invalid(format!("a {} has no file", other.name()))),
        }
    }

    /// The exact reference this Card registers as, or `None` while it has
    /// no version and registration assigns one.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not a Card.
    #[napi]
    pub fn card_ref(&self) -> Result<Option<NativeAuthored>> {
        Ok(self
            .declarable()?
            .card_ref()
            .map(|card_ref| Self::of(Authored::CardRef(card_ref))))
    }

    /// This exact reference's kind, name, and version, as authored.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not a `CardRef`.
    #[napi]
    pub fn identity(&self) -> Result<NativeCardIdentity> {
        match &self.inner {
            Authored::CardRef(card_ref) => Ok(NativeCardIdentity {
                kind: card_ref.kind().wire_name().to_owned(),
                name: card_ref.name().to_owned(),
                version: card_ref.version().to_owned(),
            }),
            other => Err(invalid(format!("a {} has no identity", other.name()))),
        }
    }

    /// The spec as authored so far, before validation; a local code file's
    /// position is `null` until validation.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not a Card.
    #[napi]
    pub fn spec(&self) -> Result<Value> {
        Ok(self.declarable()?.spec())
    }

    /// Validate the complete graph this Card roots exactly as its
    /// declaration would, reading each local code file, without registering
    /// or uploading anything.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not a Card; the shared
    /// validation's refusal is returned in the result.
    #[napi]
    pub fn validate(&self) -> Result<NativeLifecycleResult> {
        NativeLifecycleResult::outcome(self.declarable()?.declaration().map(drop))
    }

    /// The step stages the server would execute, after validating this
    /// Verifier as its YAML file would be, as `{stages, task_count}`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidArg` when this value is not a Verifier; the shared
    /// validation's refusal is returned in the result.
    #[napi]
    pub fn plan(&self) -> Result<NativeLifecycleResult> {
        match &self.inner {
            Authored::Verifier(verifier) => NativeLifecycleResult::outcome(verifier.plan()),
            other => Err(invalid(format!("a {} has no plan", other.name()))),
        }
    }
}
