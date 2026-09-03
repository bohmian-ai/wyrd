//! Bifrost integration target.
//!
//! Each submodule owns one Bifrost surface. Modules whose bodies are archived
//! stay out of this list until their tests are restored against a current
//! production invariant, so the target never advertises coverage it does not run.

mod distributed_compat;
mod forge;
mod oracle;
mod scribe;
