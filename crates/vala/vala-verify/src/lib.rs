//! The Verifier engine.
//!
//! [`blocks`] runs building-block Verifier specs: steps, requirements,
//! checks, and the `decide` rule, producing one verification result.
//!
//! ## Engine boundary
//!
//! - The engine never binds dynamic-language runtimes. There is no
//!   language-binding feature on this crate.
//! - The engine never touches IO directly. Hosts inject executors, an
//!   evidence reader and a [`TraceSource`]; the in-memory trace source ships
//!   here and the server-backed one lives in `wyrd-server`.
//!
//! ## Public surface
//!
//! - `blocks` - the engine, its executor seams and drift evidence.
//! - `operators` - runtime semantics for the `ComparisonOperator` catalog.
//! - `trace_source` - `TraceSource` trait plus in-memory impl.

mod assertion;
pub mod blocks;
pub mod error;
mod jsonpath;
pub mod operators;
pub mod trace_source;
mod walk;

pub use error::ExecError;
pub use operators::{OperatorVerdict, evaluate_operator};
pub use trace_source::{InMemoryTraceSource, TraceSource, TraceUnavailable};
