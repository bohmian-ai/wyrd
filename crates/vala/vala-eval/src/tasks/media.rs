//! Per-record media bindings for LLM judge context.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// One opaque media payload bound to a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalMediaBinding {
    /// Media identifier referenced by the judge prompt.
    pub id: String,
    /// Opaque payload for the eventual provider-specific renderer.
    pub payload: Value,
}

/// Engine-owned collection of per-record media bindings.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct MediaBindings {
    by_id: BTreeMap<String, EvalMediaBinding>,
}

impl MediaBindings {
    /// Empty binding set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow a binding by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&EvalMediaBinding> {
        self.by_id.get(id)
    }

    /// Insert or replace a binding by its id.
    pub fn insert(&mut self, binding: EvalMediaBinding) {
        self.by_id.insert(binding.id.clone(), binding);
    }

    /// Iterate binding ids in stable order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// True when no bindings are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Project media bindings into the context shape seen by a judge.
#[must_use]
pub fn bindings_as_context(bindings: &MediaBindings) -> Value {
    let mut root = Map::new();
    for id in bindings.ids() {
        if let Some(binding) = bindings.get(id) {
            root.insert(
                id.to_owned(),
                serde_json::json!({ "payload": binding.payload }),
            );
        }
    }
    Value::Object(root)
}
