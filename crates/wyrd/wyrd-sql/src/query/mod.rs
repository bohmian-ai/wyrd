//! Metadata query compilation for Postgres.

#![deny(missing_docs)]

mod compile;

pub use compile::{FieldColumn, FieldResolver, MAX_REGEX_LEN, compile_query};
