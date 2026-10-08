//! `skald-agent` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod agent_construction;
mod agent_fields;
mod agent_journey;
mod agent_timeout;
mod callbacks;
mod construct;
mod delegate;
mod error_codes;
mod loop_basic;
mod loop_conversation;
mod loop_gemini;
mod loop_responses;
mod session_conversion;
mod session_journal;
mod structured_output;
