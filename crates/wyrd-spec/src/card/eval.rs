//! Eval spec body. The type is owned by `vala::eval` -- this file re-exports
//! it under the `card::` path so the eval-backed `Verifier` implementation
//! keeps its stable import surface. Authors declare it as a `kind: Verifier`
//! card with `implementation.kind: eval`; there is no registrable `Eval` card
//! kind.
//!
//! The legacy `EvalProfile` / `EvalAssertion` / `EvalPassGate` /
//! `EvalRubricItem` / `EvalScenario` / `EvalType` types previously defined
//! here were a pre-doctrine sketch. They are removed in favor of the locked
//! `vala::eval::EvalSpec` shape.

pub use crate::vala::eval::EvalSpec;
