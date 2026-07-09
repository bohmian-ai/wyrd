//! Metadata query language: typed AST, string parser, and validation.
//!
//! Pure and engine-agnostic. Per-engine compilers live in the executing crate.

#![deny(missing_docs)]

mod ast;
mod display;
mod error_detail;
mod parse;
#[cfg(test)]
mod tests;
mod value;

pub use ast::{FieldRef, MAX_DEPTH, MAX_PREDICATES, MetadataQuery, Operator, Predicate};
pub use error_detail::QueryFieldErrorDetail;
pub use value::{Value, ValueType};
