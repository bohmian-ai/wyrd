//! The shared Wyrd client: the sole SDK-facing Rust client surface.
//!
//! This crate owns client transport configuration, credential resolution,
//! authentication, and the composed client capabilities — [`cards::Cards`],
//! [`storage::WyrdStorageClient`], [`state::WyrdState`], and [`Bifrost`] —
//! that the Rust, Python, and TypeScript SDKs project. [`Workflow`] loads
//! authored Workflow files and registered Workflow Cards into the Skald
//! runtime. Durable shared security refs live in `wyrd-spec`.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod auth;
pub mod bifrost;
pub mod cards;
pub mod client;
pub mod config;
pub(crate) mod credentials_file;
pub mod environment;
pub mod error;
pub mod gateway;
pub mod gateway_credential;
pub mod global_config;
pub mod observe;
pub mod operator_connections;
pub mod platform;
pub mod principals;
pub mod saved_login;
pub mod state;
pub mod storage;
pub mod transport;
pub mod verification;
pub mod workflow;

pub use bifrost::{Bifrost, QueueConfig};
pub use client::WyrdClient;
pub use gateway::Gateway;
pub use global_config::GlobalConfig;
pub use operator_connections::OperatorConnections;
pub use platform::Platform;
pub use principals::Principals;
pub use verification::Verification;
pub use workflow::{PublicWyrdGatewayCaller, Workflow, WorkflowCards, Workflows};
