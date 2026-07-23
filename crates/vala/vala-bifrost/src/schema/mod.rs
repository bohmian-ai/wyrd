use std::sync::Arc;

use arrow::datatypes::{Field, Schema, SchemaRef};

pub mod fingerprint;
pub mod managed_columns;
pub mod sort_order;

pub use managed_columns::with_managed_columns;

pub fn bifrost_schema(user_fields: Vec<Field>) -> SchemaRef {
    let all_fields = with_managed_columns(user_fields);
    Arc::new(Schema::new(all_fields))
}
