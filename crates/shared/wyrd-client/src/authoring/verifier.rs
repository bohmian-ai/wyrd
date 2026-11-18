//! Verifier authoring: inputs, steps, requirements, actions, and checks.

use serde_json::Value;
use wyrd_loader::LoadError;
use wyrd_spec::envelope::{CardKind, Spec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::verification::ExecutionPlan;

use super::{Body, Card, CardDefinition, FieldValue, canonical_object, card_methods, fields};

/// A Verifier Card: the evidence it accepts, the steps that compute over it,
/// and the requirements that judge it.
#[derive(Debug, Clone, PartialEq)]
pub struct Verifier {
    /// The Card under construction.
    card: Card,
}

impl Verifier {
    /// An empty Verifier Card named `name` at `version`, or with no version
    /// for the server to select; nothing is validated until its graph is.
    pub fn new<'v>(name: impl Into<String>, version: impl Into<Option<&'v str>>) -> Self {
        Self {
            card: Card::new(CardKind::Verifier, name, version.into()),
        }
    }

    /// The step stages the server would execute, after validating this
    /// Verifier as its YAML file would be.
    ///
    /// A step runs after every step it names in `depends_on` and every step
    /// whose output or status its action or `when` condition references;
    /// steps with no edge between them share a stage.
    ///
    /// # Errors
    /// Returns `WYRD_LOADER_400_INVALID_ENVELOPE` for a field that does not
    /// decode, and the shared Verifier validation's refusal.
    ///
    /// # Panics
    /// Never: a Verifier Card always decodes as a Verifier spec.
    pub fn plan(&self) -> Result<ExecutionPlan, WyrdError> {
        let card = self.card.decode().map_err(LoadError::multiple)?;
        let Spec::Verifier(spec) = card.spec else {
            unreachable!("a Verifier Card decodes as a Verifier spec");
        };
        spec.plan()
    }
}

card_methods!(Verifier);

fields!(Verifier [card.spec] {
    one description: Value = "description";
    map inputs, add_input: InputSlot = "inputs";
    list steps, add_step: Step = "steps";
    list requirements, add_requirement: Requirement = "requirements";
    one decide: Value = "decide";
    one context_capture: Value = "context_capture";
    one order_by: Value = "order_by";
});

impl From<Verifier> for CardDefinition {
    /// The same Card, keeping what it references.
    fn from(verifier: Verifier) -> Self {
        Self(std::sync::Arc::new(verifier.card))
    }
}

impl From<&Verifier> for CardDefinition {
    /// A copy of the Card as it is now.
    fn from(verifier: &Verifier) -> Self {
        Self::from(verifier.clone())
    }
}

impl From<Verifier> for crate::cards::Declarable<'_> {
    /// The authored Card and everything it references.
    fn from(card: Verifier) -> Self {
        Self::Card(Box::new(card.into()))
    }
}

impl From<&Verifier> for crate::cards::Declarable<'_> {
    /// A copy of the authored Card as it is now.
    fn from(card: &Verifier) -> Self {
        Self::Card(Box::new(card.into()))
    }
}

impl From<Verifier> for FieldValue {
    /// A sibling reference to the Verifier, declared in the same graph.
    fn from(verifier: Verifier) -> Self {
        Self::from(CardDefinition::from(verifier))
    }
}

/// The JSON Schema keyword a slot marker such as [`Trace`] sets. Its value is
/// the canonical slot the model field declares in place of a `json` slot, such
/// as `{"type": "trace"}`. [`InputSlot::json`] reads it, so it never reaches a
/// stored Card.
pub const SLOT_KEYWORD: &str = "x-wyrd-slot";

/// A trace slot's value: the trace ID string of the work a Verifier judges.
///
/// A struct field of this type declares a `trace` slot when the struct is a
/// Verifier's [`InputSlot::fields_of`] inputs, where every other field
/// declares a `json` slot, so one struct both declares the inputs and, once
/// serialized, supplies them. Its JSON Schema is a string whose
/// [`SLOT_KEYWORD`] is `{"type": "trace"}`; its JSON value is the bare ID.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Trace(
    /// The 32-hex trace ID the slot's value carries.
    pub String,
);

impl schemars::JsonSchema for Trace {
    /// Inline at each field, so a model's property names the marker directly.
    fn inline_schema() -> bool {
        true
    }

    /// The marker's name.
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Trace".into()
    }

    /// A string schema marked as a trace slot.
    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", SLOT_KEYWORD: { "type": "trace" } })
    }
}

/// One named evidence slot of a Verifier.
///
/// Evidence a caller omits stays missing, never `null`: the checks that need
/// it are inconclusive. A `json` slot's schema alone decides whether a
/// supplied `null` is admitted.
#[derive(Debug, Clone, PartialEq)]
pub struct InputSlot {
    /// The slot's fields.
    body: Body,
}

impl InputSlot {
    /// A slot of `type`.
    fn typed(kind: &str) -> Self {
        Self {
            body: Body::new([("type", FieldValue::from(kind))]),
        }
    }

    /// A JSON value constrained by the JSON Schema `schema`, or the slot a
    /// marker names: a schema carrying [`SLOT_KEYWORD`], as a [`Trace`]
    /// field, a Pydantic `wyrd.verify.Trace` field, or a Zod `trace()` field
    /// emits, is that keyword's canonical slot, such as [`InputSlot::trace`].
    pub fn json(schema: impl Into<Value>) -> Self {
        let mut schema = schema.into();
        if let Some(slot) = schema
            .as_object_mut()
            .and_then(|object| object.remove(SLOT_KEYWORD))
        {
            return Self {
                body: Body::authored(slot),
            };
        }
        let mut slot = Self::typed("json");
        slot.body.set("schema", FieldValue::from(schema));
        slot
    }

    /// A JSON value constrained by the JSON Schema of `T`, such as a struct
    /// deriving `schemars::JsonSchema`: the Rust twin of a Pydantic model or
    /// Zod schema passed as a slot.
    #[must_use]
    pub fn model<T: schemars::JsonSchema>() -> Self {
        Self::json(schemars::schema_for!(T))
    }

    /// One slot per field of `T`, such as a struct deriving
    /// `schemars::JsonSchema`, as [`InputSlot::fields`] reads its schema: the
    /// Rust twin of passing a Pydantic model or Zod object as a Verifier's
    /// whole `inputs`. A value of `T` passed to `verify` then fills every slot
    /// at once. A `T` without fields, such as an enum, declares no slots.
    #[must_use]
    pub fn fields_of<T: schemars::JsonSchema>() -> Vec<(String, Self)> {
        Self::fields(&Value::from(schemars::schema_for!(T))).unwrap_or_default()
    }

    /// One slot per property of the object JSON Schema `schema`, named by
    /// the property and constrained by its schema; `None` when `schema`
    /// declares no `properties`. A property that references one of the
    /// schema's `$defs` becomes that definition, carrying the definitions it
    /// reaches, so nested types and references still resolve. A recursive
    /// type's schema is a root `$ref` to its own definition, whose properties
    /// are the slots.
    #[must_use]
    pub fn fields(schema: &Value) -> Option<Vec<(String, Self)>> {
        let empty = serde_json::Map::new();
        let defs = schema
            .get("$defs")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        let root = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(local_def)
            .and_then(|name| defs.get(name))
            .unwrap_or(schema);
        let properties = root.get("properties").and_then(Value::as_object)?;
        Some(
            properties
                .iter()
                .map(|(name, property)| (name.clone(), Self::from(standalone(property, defs))))
                .collect(),
        )
    }

    /// A slot of `type` listing `items` under `field`.
    fn listing<I>(kind: &str, field: &str, items: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Value>,
    {
        let mut slot = Self::typed(kind);
        slot.body.set_list(
            field,
            items.into_iter().map(|item| FieldValue::from(item.into())),
        );
        slot
    }

    /// A pinned file in one of `formats`, such as `"pdf"`.
    pub fn media<I>(formats: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Value>,
    {
        Self::listing("media", "formats", formats)
    }

    /// A trace resolved through tenant-scoped trace reads.
    #[must_use]
    pub fn trace() -> Self {
        Self::typed("trace")
    }

    /// Recorded measurements of `features` for a scheduled window.
    pub fn samples<I>(features: I) -> Self
    where
        I: IntoIterator,
        I::Item: Into<Value>,
    {
        Self::listing("samples", "features", features)
    }

    /// A group Verifier's input authored as the source reference `text`,
    /// such as `verifiers.<name>.inputs.<slot>`.
    pub fn source(text: impl Into<String>) -> Self {
        let mut body = Body::default();
        body.fields
            .insert("source".to_owned(), Value::from(text.into()));
        Self { body }
    }
}

/// `property` as a schema of its own: a lone local `$ref` replaced by the
/// definition it names, with the other keys kept, and `$defs` holding every
/// definition the result still references.
fn standalone(property: &Value, defs: &serde_json::Map<String, Value>) -> Value {
    let mut schema = property.clone();
    if let Value::Object(object) = &mut schema
        && let Some(target) = object
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(local_def)
        && let Some(Value::Object(definition)) = defs.get(target)
    {
        object.remove("$ref");
        for (key, value) in definition {
            object.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    let mut reached = serde_json::Map::new();
    let mut pending = vec![schema.clone()];
    while let Some(value) = pending.pop() {
        for name in local_refs(&value) {
            if let Some(definition) = defs.get(name.as_str())
                && !reached.contains_key(&name)
            {
                reached.insert(name, definition.clone());
                pending.push(definition.clone());
            }
        }
    }
    if let (Value::Object(object), false) = (&mut schema, reached.is_empty()) {
        object.insert("$defs".to_owned(), Value::Object(reached));
    }
    schema
}

/// The `$defs` name a local `#/$defs/<name>` reference points at.
fn local_def(reference: &str) -> Option<&str> {
    reference.strip_prefix("#/$defs/")
}

/// Every `$defs` name `value` references locally, at any depth.
fn local_refs(value: &Value) -> Vec<String> {
    match value {
        Value::Object(object) => object
            .iter()
            .flat_map(|(key, value)| match (key.as_str(), value) {
                ("$ref", Value::String(reference)) => local_def(reference)
                    .map(str::to_owned)
                    .into_iter()
                    .collect(),
                _ => local_refs(value),
            })
            .collect(),
        Value::Array(items) => items.iter().flat_map(local_refs).collect(),
        _ => Vec::new(),
    }
}

fields!(InputSlot [body] {
    one max_bytes: Value = "max_bytes";
});

impl From<Value> for InputSlot {
    /// A source slot's reference string, a canonical slot object exactly as
    /// authored when its `type` is `json`, `media`, `trace` or `samples`, or
    /// else the JSON Schema of a `json` slot. Values are schemas, never
    /// examples to infer one from; validation refuses a malformed one.
    fn from(value: Value) -> Self {
        let canonical = value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|kind| matches!(kind, "json" | "media" | "trace" | "samples"));
        match value {
            Value::String(text) => Self::source(text),
            value if canonical => Self {
                body: Body::authored(value),
            },
            schema => Self::json(schema),
        }
    }
}

canonical_object!(Step, Requirement, Action, Check);

impl From<InputSlot> for FieldValue {
    /// The canonical slot object, or a source slot's reference string.
    fn from(slot: InputSlot) -> Self {
        match slot.body.fields.get("source") {
            Some(text) if slot.body.fields.len() == 1 => Self::plain(text.clone()),
            _ => slot.body.into_field(),
        }
    }
}

/// One data-producing Verifier step.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// The step's fields.
    body: Body,
}

impl Step {
    /// The step `id` running `action`.
    pub fn new(id: impl Into<String>, action: impl Into<Action>) -> Self {
        Self {
            body: Body::new([
                ("id", FieldValue::from(id.into())),
                ("action", FieldValue::from(action.into())),
            ]),
        }
    }
}

fields!(Step [body] {
    list depends_on, add_depends_on: Value = "depends_on";
    one when: Value = "when";
    list requirements, add_requirement: Requirement = "requirements";
    one timeout_seconds: Value = "timeout_seconds";
    one retries: Value = "retries";
});

impl From<Step> for FieldValue {
    /// The canonical step object.
    fn from(step: Step) -> Self {
        step.body.into_field()
    }
}

/// One requirement, producing exactly one verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct Requirement {
    /// The requirement's fields.
    body: Body,
}

impl Requirement {
    /// The requirement `id` claiming `statement`, judged by `check`.
    pub fn new(
        id: impl Into<String>,
        statement: impl Into<String>,
        check: impl Into<Check>,
    ) -> Self {
        Self {
            body: Body::new([
                ("id", FieldValue::from(id.into())),
                ("statement", FieldValue::from(statement.into())),
                ("check", FieldValue::from(check.into())),
            ]),
        }
    }
}

impl From<Requirement> for FieldValue {
    /// The canonical requirement object.
    fn from(requirement: Requirement) -> Self {
        requirement.body.into_field()
    }
}

/// A step action: extract, an Agent call, or sandboxed code.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    /// The action's fields.
    body: Body,
}

impl Action {
    /// An action of `kind` with `fields`.
    fn of<const N: usize>(kind: &str, fields: [(&str, FieldValue); N]) -> Self {
        let mut body = Body::new(fields);
        body.set("kind", FieldValue::from(kind));
        Self { body }
    }

    /// Extract text and structure from the media slot `from`, such as
    /// `inputs.report`.
    pub fn extract(from: impl Into<String>) -> Self {
        Self::of("extract", [("from", FieldValue::from(from.into()))])
    }

    /// Invoke `agent`: an [`super::Agent`], an exact [`super::CardRef`], a
    /// YAML reference path, or inline Agent JSON.
    pub fn agent(agent: impl Into<FieldValue>) -> Self {
        Self::of("agent", [("agent", agent.into())])
    }

    /// Run `entrypoint` of `source` as sandboxed code returning JSON data.
    pub fn code(source: impl Into<FieldValue>, entrypoint: impl Into<String>) -> Self {
        Self::of(
            "code",
            [
                ("source", source.into()),
                ("entrypoint", FieldValue::from(entrypoint.into())),
            ],
        )
    }
}

fields!(Action [body] {
    map inputs, bind_input: Value = "inputs";
    map workspace, add_workspace: FieldValue = "workspace";
    one runtime: Value = "runtime";
});

impl From<Action> for FieldValue {
    /// The canonical action object.
    fn from(action: Action) -> Self {
        action.body.into_field()
    }
}

/// A verdict-producing requirement check.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// The check's fields.
    body: Body,
}

impl Check {
    /// A check of `kind` with `fields`.
    fn of<const N: usize>(kind: &str, fields: [(&str, FieldValue); N]) -> Self {
        let mut body = Body::new(fields);
        body.set("kind", FieldValue::from(kind));
        Self { body }
    }

    /// Compare the one value at `path` with `expected` by `operator`: a
    /// [`ComparisonOperator`](wyrd_spec::verification::ComparisonOperator),
    /// or its YAML form such as `"equals"`.
    pub fn assert(
        path: impl Into<String>,
        operator: impl Into<FieldValue>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "assert",
            [
                ("path", FieldValue::from(path.into())),
                ("operator", operator.into()),
                ("expected", FieldValue::from(expected.into())),
            ],
        )
    }

    /// Compare every value `each` selects through `[*]` with `expected` by
    /// `operator`.
    pub fn each(
        each: impl Into<String>,
        operator: impl Into<FieldValue>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "assert",
            [
                ("each", FieldValue::from(each.into())),
                ("operator", operator.into()),
                ("expected", FieldValue::from(expected.into())),
            ],
        )
    }

    /// Compare the spans `span_selector` selects in the trace slot `on` with
    /// `expected` by `operator`.
    pub fn trace(
        on: impl Into<String>,
        span_selector: impl Into<String>,
        operator: impl Into<FieldValue>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "trace",
            [
                ("on", FieldValue::from(on.into())),
                ("span_selector", FieldValue::from(span_selector.into())),
                ("operator", operator.into()),
                ("expected", FieldValue::from(expected.into())),
            ],
        )
    }

    /// Judge with `agent`, which returns the Verdict schema.
    pub fn agent(agent: impl Into<FieldValue>) -> Self {
        Self::of("agent", [("agent", agent.into())])
    }

    /// Judge with `entrypoint` of `source`, which returns the Verdict schema.
    pub fn code(source: impl Into<FieldValue>, entrypoint: impl Into<String>) -> Self {
        Self::of(
            "code",
            [
                ("source", source.into()),
                ("entrypoint", FieldValue::from(entrypoint.into())),
            ],
        )
    }

    /// Judge statistical drift over the samples slot `on` with the drift
    /// fields `spec`: method, signal, condition, and profile. A Card or
    /// exact [`super::CardRef`] may stand at `signal.baseline_ref`, as at any other
    /// reference position.
    pub fn drift(on: impl Into<String>, spec: impl Into<FieldValue>) -> Self {
        let mut check = Self::of("drift", [("on", FieldValue::from(on.into()))]);
        check.body.flatten = Some("spec");
        check.body.set("spec", spec.into());
        check
    }
}

fields!(Check [body] {
    map inputs, bind_input: Value = "inputs";
    map workspace, add_workspace: FieldValue = "workspace";
    one runtime: Value = "runtime";
    one response_format: Value = "response_format";
});

impl From<Check> for FieldValue {
    /// The canonical check object.
    fn from(check: Check) -> Self {
        check.body.into_field()
    }
}
