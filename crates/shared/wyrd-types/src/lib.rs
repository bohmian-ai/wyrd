//! Wyrd column types and their JSON Schema and Arrow forms.
//!
//! The one owner of how a declared column type maps between a model's JSON
//! Schema, the `FieldSpec` wire declaration, and Arrow: the three timestamp
//! types, the Variant type, and the schema mappings that carry them. Writers,
//! the server, and every SDK import these rather than restating them.

#![deny(missing_docs)]

pub mod schema;
pub mod timestamp;
pub mod variant;

pub use schema::{
    arrow_schema_to_fieldspec, field_to_spec, fieldspec_to_arrow, is_extension_key,
    json_schema_to_arrow, json_schema_to_fieldspec, spec_to_field, writable_schema,
};
pub use timestamp::{TimestampKind, TimestampLtz, TimestampNtz, TimestampTz};
