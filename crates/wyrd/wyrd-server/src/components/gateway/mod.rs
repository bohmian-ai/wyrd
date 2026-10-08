//! Tenant gateway administration.
//!
//! [`GatewayAdministration`] owns every credential, deployment, and policy
//! operation: typed authorization with decision audit, tenant-scoped RLS
//! transactions, lifecycle rules, and redacted views.
//! HTTP handlers and MCP tools are thin projections of its methods.
//! [`GatewayAdministration::snapshot`] hands the gateway runtime one immutable,
//! statement-consistent view of the tenant configuration per admission.

mod batches;
mod capture;
pub(crate) mod ingress;
mod invocation;
mod ledger;
mod multipart;
#[cfg(test)]
mod pg_administration_tests;
#[cfg(test)]
mod pg_invocation_tests;
pub(crate) mod routes;
mod service;
mod workflow;

pub use batches::{BatchAnswer, GatewayBatches};
pub use capture::CallCapture;
pub(crate) use capture::CaptureDrop;
#[cfg(test)]
pub(crate) use capture::{recording, tests as capture_tests};
pub use ingress::gateway_ingress_router;
pub(crate) use invocation::unconnected_engine;
pub use invocation::{GatewayCallRequest, GatewayCallResponse, GatewayInvocation};
pub use routes::gateway_router;
pub use service::GatewayAdministration;
pub(crate) use service::invalid;
pub(crate) use workflow::ServerWyrdGatewayCaller;
pub use wyrd_gateway::{GatewayCredentialSnapshot, GatewayCredentialSource, GatewayTenantSnapshot};
