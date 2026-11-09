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
    /// An empty Verifier Card named `name` at `version`; nothing is
    /// validated until its graph is.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            card: Card::new(CardKind::Verifier, name, version),
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
        Self(verifier.card)
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

/// One named evidence slot of a Verifier.
///
/// Slots are required unless `required(false)` is set; samples and source
/// slots are always required.
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

    /// A JSON value constrained by the JSON Schema `schema`.
    pub fn json(schema: impl Into<Value>) -> Self {
        let mut slot = Self::typed("json");
        slot.body.set("schema", FieldValue::from(schema.into()));
        slot
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

fields!(InputSlot [body] {
    one max_bytes: Value = "max_bytes";
    one required: Value = "required";
});

impl From<Value> for InputSlot {
    /// The canonical slot exactly as authored: a source slot's reference
    /// string, or a slot object; any other value stands as authored, so
    /// validation refuses it as YAML would.
    fn from(value: Value) -> Self {
        match value {
            Value::String(text) => Self::source(text),
            value => Self {
                body: Body::authored(value),
            },
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
    map inputs, add_input: Value = "inputs";
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

    /// Compare the one value at `path` with `expected` by `operator`, such
    /// as `"equals"` or its object form.
    pub fn assert(
        path: impl Into<String>,
        operator: impl Into<Value>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "assert",
            [
                ("path", FieldValue::from(path.into())),
                ("operator", FieldValue::from(operator.into())),
                ("expected", FieldValue::from(expected.into())),
            ],
        )
    }

    /// Compare every value `each` selects through `[*]` with `expected` by
    /// `operator`.
    pub fn each(
        each: impl Into<String>,
        operator: impl Into<Value>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "assert",
            [
                ("each", FieldValue::from(each.into())),
                ("operator", FieldValue::from(operator.into())),
                ("expected", FieldValue::from(expected.into())),
            ],
        )
    }

    /// Compare the spans `span_selector` selects in the trace slot `on` with
    /// `expected` by `operator`.
    pub fn trace(
        on: impl Into<String>,
        span_selector: impl Into<String>,
        operator: impl Into<Value>,
        expected: impl Into<Value>,
    ) -> Self {
        Self::of(
            "trace",
            [
                ("on", FieldValue::from(on.into())),
                ("span_selector", FieldValue::from(span_selector.into())),
                ("operator", FieldValue::from(operator.into())),
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
    /// fields `spec`: method, signal, condition, and profile.
    pub fn drift(on: impl Into<String>, spec: impl Into<Value>) -> Self {
        let mut check = Self::of(
            "drift",
            [
                ("on", FieldValue::from(on.into())),
                ("spec", FieldValue::from(spec.into())),
            ],
        );
        check.body.flatten = Some("spec");
        check
    }
}

fields!(Check [body] {
    map inputs, add_input: Value = "inputs";
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
