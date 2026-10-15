//! Bounded structural analysis of declared JSON Schemas.
//!
//! Registration refuses a reference whose selectors cannot exist under a
//! known output schema, and a check Agent whose declared response cannot
//! produce the typed Verdict. Every analysis shares one finite budget across
//! branch recursion and local `$ref` hops, so a self-referencing authored
//! schema ends the walk instead of the stack.

use serde_json::Value;

use super::reference::PathSegment;

/// Schema nodes and `$ref` hops one analysis may visit.
const BUDGET: usize = 1024;

/// Returns `false` only when `path` provably selects nothing under `schema`.
///
/// `root` resolves local `#/...` `$ref` pointers. `oneOf`/`anyOf` admit the
/// path when any branch does. A field declared by `properties` or by an
/// exactly anchored literal-name pattern such as `^score$` is followed into
/// its declaration. Anything the walk cannot decide — an unknown
/// keyword, an open object, any other pattern property, a missing `type`, an unresolved pointer, or an
/// exhausted budget — is admitted so unknown shapes stay usable and fall back
/// to missing-value handling at execution.
pub(super) fn admits(root: &Value, schema: &Value, path: &[PathSegment]) -> bool {
    Walk { budget: BUDGET }.admits(root, schema, path)
}

/// Returns whether every value `authored` admits has the shape `typed`
/// requires.
///
/// Compares types (taken from `const`/`enum` values when no `type` is
/// declared), closed value sets (`enum`, `const`, and combinators of them),
/// `allOf` parts (every typed part must hold; one authored part or the
/// authored node's own keywords suffice), object properties and exactly
/// anchored literal-name `patternProperties` such as `^score$`, required and closed fields, and array items,
/// following local `$ref` pointers on both sides. String formats, patterns,
/// and numeric bounds are left to output decoding. An exhausted budget or an
/// unresolved pointer is not shown to fit.
pub(super) fn fits(authored: &Value, typed: &Value) -> bool {
    Walk { budget: BUDGET }.fits(authored, authored, typed, typed)
}

/// One analysis and the work it may still spend.
struct Walk {
    /// Remaining node visits and `$ref` hops.
    budget: usize,
}

impl Walk {
    /// Spends one unit of work; `false` once the budget is exhausted.
    fn spend(&mut self) -> bool {
        self.budget = self.budget.saturating_sub(1);
        self.budget > 0
    }

    /// Follows local `$ref` pointers; `None` when a pointer cannot be
    /// resolved or the budget runs out.
    fn resolve<'a>(&mut self, root: &'a Value, mut schema: &'a Value) -> Option<&'a Value> {
        while let Some(pointer) = schema.get("$ref").and_then(Value::as_str) {
            if !self.spend() {
                return None;
            }
            schema = root.pointer(pointer.strip_prefix('#')?)?;
        }
        Some(schema)
    }

    /// [`admits`] within the remaining budget.
    fn admits(&mut self, root: &Value, schema: &Value, path: &[PathSegment]) -> bool {
        if !self.spend() {
            return true;
        }
        let Some(schema) = self.resolve(root, schema) else {
            return true;
        };
        let Some((segment, rest)) = path.split_first() else {
            return true;
        };
        for keyword in ["oneOf", "anyOf"] {
            if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
                return branches
                    .iter()
                    .any(|branch| self.admits(root, branch, path));
            }
        }
        match segment {
            PathSegment::Field(name) => {
                if let Some(property) = schema.get("properties").and_then(|props| props.get(name)) {
                    return self.admits(root, property, rest);
                }
                let patterns = schema.get("patternProperties").and_then(Value::as_object);
                for (pattern, property) in patterns.into_iter().flatten() {
                    match literal_name(pattern) {
                        Some(literal) if literal == name => {
                            return self.admits(root, property, rest);
                        }
                        Some(_) => {}
                        None => return true,
                    }
                }
                if excludes_type(schema, "object") {
                    return false;
                }
                match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => false,
                    Some(additional @ Value::Object(_)) => self.admits(root, additional, rest),
                    _ => true,
                }
            }
            PathSegment::Index(_) | PathSegment::Wildcard => match schema.get("items") {
                Some(items @ Value::Object(_)) => self.admits(root, items, rest),
                Some(_) => true,
                None => !excludes_type(schema, "array"),
            },
        }
    }

    /// [`fits`] within the remaining budget, with each side's root for its
    /// own `$ref` pointers.
    fn fits(&mut self, a_root: &Value, authored: &Value, t_root: &Value, typed: &Value) -> bool {
        if !self.spend() {
            return false;
        }
        let (Some(authored), Some(typed)) =
            (self.resolve(a_root, authored), self.resolve(t_root, typed))
        else {
            return false;
        };
        for keyword in ["oneOf", "anyOf"] {
            if let Some(branches) = authored.get(keyword).and_then(Value::as_array) {
                return branches
                    .iter()
                    .all(|branch| self.fits(a_root, branch, t_root, typed));
            }
        }
        if let Some(parts) = typed.get("allOf").and_then(Value::as_array) {
            return parts
                .iter()
                .all(|part| self.fits(a_root, authored, t_root, part));
        }
        if let Some(parts) = authored.get("allOf").and_then(Value::as_array)
            && parts
                .iter()
                .any(|part| self.fits(a_root, part, t_root, typed))
        {
            return true;
        }
        if let Some(allowed) = self.values(t_root, typed) {
            return self
                .values(a_root, authored)
                .is_some_and(|values| values.iter().all(|value| allowed.contains(value)));
        }
        for keyword in ["oneOf", "anyOf"] {
            if let Some(branches) = typed.get(keyword).and_then(Value::as_array) {
                return branches
                    .iter()
                    .any(|branch| self.fits(a_root, authored, t_root, branch));
            }
        }
        let wanted = types(typed);
        if wanted.is_empty() {
            return true;
        }
        let mut declared = types(authored);
        if declared.is_empty()
            && let Some(values) = self.values(a_root, authored)
        {
            declared = values.into_iter().map(json_type).collect();
        }
        let allowed =
            |ty: &&str| wanted.contains(ty) || (*ty == "integer" && wanted.contains(&"number"));
        if declared.is_empty() || !declared.iter().all(allowed) {
            return false;
        }
        self.fits_object(a_root, authored, t_root, typed)
            && match typed.get("items") {
                Some(items) => authored
                    .get("items")
                    .is_some_and(|own| self.fits(a_root, own, t_root, items)),
                None => true,
            }
    }

    /// Requires an authored object to declare every typed required field as
    /// a required property that fits, to add no field a closed typed object
    /// refuses, and to stay closed when the typed object is. A literal-name
    /// pattern property declares that one field: refining a declared property
    /// adds nothing, otherwise it is compared like a property. Other patterns
    /// are left to output decoding.
    fn fits_object(
        &mut self,
        a_root: &Value,
        authored: &Value,
        t_root: &Value,
        typed: &Value,
    ) -> bool {
        let Some(properties) = typed.get("properties").and_then(Value::as_object) else {
            return true;
        };
        let closed =
            |schema: &Value| schema.get("additionalProperties") == Some(&Value::Bool(false));
        let own = authored.get("properties").and_then(Value::as_object);
        let own_required = required(authored);
        if closed(typed) && !closed(authored) {
            return false;
        }
        let complete = required(typed).iter().all(|field| {
            own_required.contains(field) && own.is_some_and(|own| own.contains_key(*field))
        });
        let known = own_required
            .iter()
            .all(|field| properties.contains_key(*field));
        let named = authored
            .get("patternProperties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter_map(|(pattern, schema)| Some((literal_name(pattern)?, schema)));
        for (name, schema) in named {
            if own.is_some_and(|own| own.contains_key(name)) {
                continue;
            }
            let fits = match properties.get(name) {
                Some(expected) => self.fits(a_root, schema, t_root, expected),
                None => !closed(typed),
            };
            if !fits {
                return false;
            }
        }
        complete
            && known
            && own
                .into_iter()
                .flatten()
                .all(|(name, schema)| match properties.get(name) {
                    Some(expected) => self.fits(a_root, schema, t_root, expected),
                    None => !closed(typed),
                })
    }

    /// The closed set of values a schema admits — its `const`, its `enum`,
    /// or the union over `oneOf`/`anyOf` branches that all have one — or
    /// `None` when the schema admits an open set.
    fn values<'a>(&mut self, root: &'a Value, schema: &'a Value) -> Option<Vec<&'a Value>> {
        if !self.spend() {
            return None;
        }
        let schema = self.resolve(root, schema)?;
        if let Some(value) = schema.get("const") {
            return Some(vec![value]);
        }
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            return Some(values.iter().collect());
        }
        let branches = ["oneOf", "anyOf"]
            .into_iter()
            .find_map(|keyword| schema.get(keyword).and_then(Value::as_array))?;
        let mut all = Vec::new();
        for branch in branches {
            all.extend(self.values(root, branch)?);
        }
        Some(all)
    }
}

/// The JSON types a schema's `type` declares; empty when it declares none.
fn types(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(ty)) => vec![ty.as_str()],
        Some(Value::Array(declared)) => declared.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// The JSON type of one value, as a schema `type` names it.
fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// The one field name an exactly anchored literal pattern such as `^score$`
/// matches, or `None` for any pattern that could match another name.
fn literal_name(pattern: &str) -> Option<&str> {
    let name = pattern.strip_prefix('^')?.strip_suffix('$')?;
    (!name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
    .then_some(name)
}

/// The field names a schema's `required` lists.
fn required(schema: &Value) -> Vec<&str> {
    schema
        .get("required")
        .and_then(Value::as_array)
        .map(|fields| fields.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// Returns whether the schema's declared `type` rules out `wanted`.
fn excludes_type(schema: &Value, wanted: &str) -> bool {
    !types(schema).is_empty() && !types(schema).contains(&wanted)
}

#[cfg(test)]
mod tests {
    //! Known-missing selector detection over local schemas.

    use serde_json::json;

    use super::*;

    /// Field, index, and wildcard selectors are refused only when the schema
    /// proves them absent.
    ///
    /// # Panics
    /// Panics when a selector is admitted or refused incorrectly.
    #[test]
    fn schema_path_refuses_only_known_missing_selectors() {
        let schema = json!({
            "definitions": {"Item": {"type": "object", "properties": {"name": {"type": "string"}}, "additionalProperties": false}},
            "type": "object",
            "properties": {
                "items": {"type": "array", "items": {"$ref": "#/definitions/Item"}},
                "open": {"type": "object"},
                "maybe": {"anyOf": [{"$ref": "#/definitions/Item"}, {"type": "null"}]},
                "text": {"type": "string"}
            },
            "additionalProperties": false
        });
        let field = |name: &str| PathSegment::Field(name.to_owned());
        assert!(admits(
            &schema,
            &schema,
            &[field("items"), PathSegment::Wildcard, field("name")]
        ));
        assert!(admits(
            &schema,
            &schema,
            &[field("open"), field("anything")]
        ));
        assert!(admits(&schema, &schema, &[field("maybe"), field("name")]));
        assert!(!admits(&schema, &schema, &[field("missing")]));
        assert!(!admits(
            &schema,
            &schema,
            &[field("items"), PathSegment::Index(0), field("nope")]
        ));
        assert!(!admits(&schema, &schema, &[field("text"), field("x")]));
        assert!(!admits(
            &schema,
            &schema,
            &[field("text"), PathSegment::Index(0)]
        ));
        assert!(admits(&json!({}), &json!({}), &[field("x")]));
    }
}
