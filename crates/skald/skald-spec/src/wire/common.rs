use std::ops::Deref;
use std::sync::Arc;

use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::wire::borrowed::{Borrowed, IgnoredObject};

/// Reduced finish reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolCalls,
    Other,
}

/// Reduced token usage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub reasoning_tokens: u64,
}

/// A JSON Schema a request sends, shared rather than copied.
///
/// Clones share one schema, so copying a request or Prompt, deriving its
/// [`ResponseType`](crate::ResponseType), or keeping it beside a compiled
/// validator never copies the schema; [`make_mut`](Self::make_mut) copies
/// it only when an edit would change a schema something else still holds.
/// It serializes, decodes, and describes itself exactly as the JSON value
/// it holds.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
pub struct SharedSchema(Arc<Value>);

impl SharedSchema {
    /// Shares `schema`.
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self(Arc::new(schema))
    }

    /// Whether `other` shares this very schema, not merely an equal one.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The address of the shared schema, stable while any clone lives.
    #[must_use]
    pub fn address(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    /// The schema for editing, copied first when another clone shares it.
    pub fn make_mut(&mut self) -> &mut Value {
        Arc::make_mut(&mut self.0)
    }

    /// The schema as an owned value, copied only when another clone shares it.
    #[must_use]
    pub fn into_value(self) -> Value {
        Arc::unwrap_or_clone(self.0)
    }
}

impl Deref for SharedSchema {
    type Target = Value;

    /// The shared schema.
    fn deref(&self) -> &Value {
        &self.0
    }
}

impl From<Value> for SharedSchema {
    /// Shares `schema`.
    fn from(schema: Value) -> Self {
        Self::new(schema)
    }
}

impl PartialEq for SharedSchema {
    /// Equal when shared or structurally equal.
    fn eq(&self, other: &Self) -> bool {
        self.ptr_eq(other) || self.0 == other.0
    }
}

impl PartialEq<Value> for SharedSchema {
    /// Equal when the schema equals `other`.
    fn eq(&self, other: &Value) -> bool {
        *self.0 == *other
    }
}

impl Serialize for SharedSchema {
    /// Serializes as the schema's JSON value.
    ///
    /// # Errors
    /// Returns `serializer`'s error when it refuses the value.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SharedSchema {
    /// Decodes any JSON value.
    ///
    /// # Errors
    /// Returns `deserializer`'s error when its input is not JSON.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Value::deserialize(deserializer).map(Self::new)
    }
}

impl Borrowed<'_> for SharedSchema {
    type View = IgnoredAny;
}

/// A [`SharedSchema`] that is always a JSON object, for the provider fields
/// that accept only an object schema; it decodes and describes itself as a
/// JSON object and refuses anything else.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schemars", schemars(transparent))]
#[serde(transparent)]
pub struct SharedObjectSchema(
    #[cfg_attr(feature = "schemars", schemars(with = "Map<String, Value>"))] SharedSchema,
);

impl SharedObjectSchema {
    /// Shares the object schema `members`.
    #[must_use]
    pub fn new(members: Map<String, Value>) -> Self {
        Self(SharedSchema::new(Value::Object(members)))
    }

    /// The schema as shared, for lookup beside schemas of any shape.
    #[must_use]
    pub fn shared(&self) -> &SharedSchema {
        &self.0
    }

    /// The schema's members for editing, copied first when another clone
    /// shares them.
    ///
    /// # Panics
    /// Never panics: the schema is an object from construction on, and only
    /// its members are ever handed out for editing.
    pub fn make_mut(&mut self) -> &mut Map<String, Value> {
        self.0
            .make_mut()
            .as_object_mut()
            .expect("an object schema stays an object")
    }
}

impl Deref for SharedObjectSchema {
    type Target = Map<String, Value>;

    /// The schema's members.
    ///
    /// # Panics
    /// Never panics; see [`make_mut`](Self::make_mut).
    fn deref(&self) -> &Map<String, Value> {
        self.0
            .as_object()
            .expect("an object schema stays an object")
    }
}

impl From<Map<String, Value>> for SharedObjectSchema {
    /// Shares the object schema `members`.
    fn from(members: Map<String, Value>) -> Self {
        Self::new(members)
    }
}

impl From<SharedObjectSchema> for SharedSchema {
    /// The object schema, still shared.
    fn from(schema: SharedObjectSchema) -> Self {
        schema.0
    }
}

impl TryFrom<SharedSchema> for SharedObjectSchema {
    type Error = SharedSchema;

    /// Keeps `schema` shared when it is an object.
    ///
    /// # Errors
    /// Returns `schema` itself, still shared, when it is not an object.
    fn try_from(schema: SharedSchema) -> Result<Self, SharedSchema> {
        if schema.is_object() {
            Ok(Self(schema))
        } else {
            Err(schema)
        }
    }
}

impl<'de> Deserialize<'de> for SharedObjectSchema {
    /// Decodes a JSON object.
    ///
    /// # Errors
    /// Returns `deserializer`'s error when its input is not a JSON object.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Map::deserialize(deserializer).map(Self::new)
    }
}

impl<'a> Borrowed<'a> for SharedObjectSchema {
    type View = IgnoredObject<'a>;
}
