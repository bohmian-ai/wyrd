//! Native tool declarations and provider message helpers for Skald.
//!
//! This crate describes tool schemas and creates provider-native tool-use
//! message fragments. It does not execute tools or call providers.

#![allow(clippy::module_name_repetitions)]

pub mod def;
pub mod error;
pub mod helpers;
#[cfg(feature = "python")]
pub mod python;
pub mod registry;
pub mod resolver;
mod toolerror;
mod trait_;

pub use def::ToolDef;
pub use error::{SkaldToolError, SkaldToolResult};
pub use helpers::{
    anthropic_tool_result_block, anthropic_tool_use_block, google_function_call_part,
    google_function_response_part, openai_function_tool_call, openai_tool_result_message,
};
#[cfg(feature = "python")]
pub use python::python_register;
pub use registry::{ToolRegistry, default_registry};
pub use resolver::ToolResolver;
pub use toolerror::{ToolError, ToolResult};
pub use trait_::AgentTool;
