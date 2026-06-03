//! `Eval` card spec. The body type is owned by `vala::eval` -- this file
//! re-exports it under the `card::` path so `Spec::Eval` keeps its stable
//! import surface.
//!
//! The legacy `EvalProfile` / `EvalAssertion` / `EvalPassGate` /
//! `EvalRubricItem` / `EvalScenario` / `EvalType` types previously defined
//! here were a pre-doctrine sketch. They are removed in favor of the locked
//! `vala::eval::EvalSpec` shape.

pub use crate::vala::eval::EvalSpec;
