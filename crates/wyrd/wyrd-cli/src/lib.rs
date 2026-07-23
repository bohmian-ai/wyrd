//! Library surface of the `wyrd` CLI for in-process test drivers.
//!
//! The CLI ships as a binary (`src/main.rs`); this thin facade re-exports the
//! command modules the binary already owns so integration tests can drive the
//! real argument-parsing, request-building, and response-handling code paths
//! in process — exactly what an operator runs — instead of re-implementing the
//! admin wire calls.

pub mod audit;
pub mod auth;
pub mod error;
pub mod load;
pub mod registration;

// Re-export the public loader API
pub use load::{
    AuthoredCard, CardSubmission, Diagnostic, LoadError, LoadedCard, LoadedTree, RegistrationInput,
    Severity, SourceSpan, build_registration_input, build_submissions, emit_json, load,
};
