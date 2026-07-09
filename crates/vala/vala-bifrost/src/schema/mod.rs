use std::sync::Arc;

use arrow::datatypes::{Field, Schema, SchemaRef};

use crate::types::TableScope;

pub mod fingerprint;
pub mod sort_order;
pub mod system_columns;

pub use system_columns::with_system_columns;

pub fn bifrost_schema(user_fields: Vec<Field>, scope: TableScope) -> SchemaRef {
    let all_fields = with_system_columns(user_fields, scope);
    Arc::new(Schema::new(all_fields))
}
