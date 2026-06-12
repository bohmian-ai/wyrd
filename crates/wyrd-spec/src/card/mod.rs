//! Card spec types, one struct per native [`crate::envelope::CardKind`].

pub mod agent;
pub mod artifact;
pub mod audit;
pub mod common;
pub mod data;
pub mod drift;
pub mod eval;
pub mod experiment;
pub mod field;
pub mod mcp;
pub mod model;
pub mod operator;
pub mod policy;
pub mod prompt;
pub mod service;
pub mod trigger;
pub mod workflow;

pub use crate::ids::{ColumnName, QueryName, SplitName};
pub use common::{
    AgentInterface, CredentialRef, Governance, MetricEntry, NonSecretValue, ObservationHooks,
    ParameterValue, ProtocolProfile,
};
pub use data::{ColValue, Inequality};
pub use field::{Dim, FieldSpec};
