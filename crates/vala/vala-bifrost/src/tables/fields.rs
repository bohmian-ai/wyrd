//! Arrow field-type helpers for pre-declared domain-table user schemas.
//!
//! One home for the small field constructors each table's `arrow_fields()` uses.
//! Moved out of the deleted `build/arrow_projection.rs`; behavior identical.

use arrow::datatypes::{DataType, Field, TimeUnit};

pub fn utf8(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Utf8, nullable)
}

pub fn utf8_view(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Utf8View, nullable)
}

pub fn ts_us_utc(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
        nullable,
    )
}

pub fn bool_field(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Boolean, nullable)
}

pub fn int32(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int32, nullable)
}

pub fn int64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Int64, nullable)
}

pub fn uint32(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::UInt32, nullable)
}

pub fn uint64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::UInt64, nullable)
}

pub fn float64(name: &str, nullable: bool) -> Field {
    Field::new(name, DataType::Float64, nullable)
}

pub fn fixed_binary(name: &str, size: i32, nullable: bool) -> Field {
    Field::new(name, DataType::FixedSizeBinary(size), nullable)
}
