//! `vala-eval` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod orchestrator_support;

mod orchestrator_judge_skald;
mod orchestrator_parity;
mod orchestrator_simulator_template;
mod orchestrator_state_machine;
