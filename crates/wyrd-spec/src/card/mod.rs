//! Card spec types, one struct per native [`crate::envelope::CardKind`].

pub mod agent;
pub mod artifact;
pub mod audit;
pub mod common;
pub mod data;
pub mod drift;
pub mod eval;
pub mod experiment;
pub mod mcp;
pub mod model;
pub mod policy;
pub mod prompt;
pub mod service;
pub mod skill;
pub mod subagent;
pub mod tool;
pub mod workflow;

pub use common::{
    AgentInterface, CredentialRef, Governance, MetricEntry, NonSecretValue, ObservationHooks,
    ParameterValue, PromptRole, ProtocolProfile, Provider,
};
