//! `skald-runtime` integration target.
//!
//! Every integration test of this crate links into this one binary so the
//! crate's dependency cone is linked once. Each submodule owns one surface;
//! nextest selects a surface with `-E 'test(/^<module>::/)'`.

mod support;

mod mock_echo;
mod runtime_dispatch;
mod runtime_mock;
mod runtime_options;
mod runtime_telemetry;
mod runtime_text;
