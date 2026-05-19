//! JSON schema helpers.

use schemars::schema::RootSchema;

/// Generate a root schema for a type.
#[must_use]
pub fn schema_for<T: schemars::JsonSchema>() -> RootSchema {
    schemars::schema_for!(T)
}
