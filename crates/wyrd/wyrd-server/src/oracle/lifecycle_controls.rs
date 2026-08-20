//! Tenant-authorized logical lifecycle controls over all current Oracle owners.

use std::collections::BTreeMap;
use std::sync::Arc;

use vala_bifrost_redux::oracle::RunningQueryRegistry;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::BifrostError;
use wyrd_spec::vala::api::{
    CancelOracleLifecycleRequest, CancelOracleLifecycleResponse, ListOracleLifecyclesRequest,
    OracleLifecycleLookupRequest, RunningQuerySummary,
};

use super::{OracleLifecycleOutcome, OracleLifecycleTransport};
use vala_bifrost_redux::cluster::ClusterRegistry;

/// One logical lifecycle policy owner over local and current-ready peers.
#[derive(Clone)]
pub struct RunningQueryControls {
    /// Exact owner-local registry used by private peer serving.
    registry: Arc<RunningQueryRegistry>,
    /// Canonical authenticated current-ready remote transport.
    lifecycle_transport: Arc<OracleLifecycleTransport>,
    /// Shared current-ready cluster membership owner.
    cluster: Arc<ClusterRegistry>,
}

impl RunningQueryControls {
    /// Creates the facade from explicit process-owned dependencies.
    #[must_use]
    pub fn new(
        registry: Arc<RunningQueryRegistry>,
        lifecycle_transport: Arc<OracleLifecycleTransport>,
        cluster: Arc<ClusterRegistry>,
    ) -> Self {
        Self {
            registry,
            lifecycle_transport,
            cluster,
        }
    }

    /// Lists all current-ready owners for the authenticated tenant.
    ///
    /// # Errors
    /// Returns a stable permission, audit, contradiction, or unavailable error.
    pub async fn list(
        &self,
        tenant_id: DataTenantId,
    ) -> Result<Vec<RunningQuerySummary>, WyrdError> {
        let _membership = self.cluster.snapshot();
        let remote = self
            .lifecycle_transport
            .list(ListOracleLifecyclesRequest { tenant_id })
            .await;
        let mut merged = BTreeMap::new();
        for summary in self.registry.list(tenant_id) {
            merged.insert(summary.request_id.as_str().to_owned(), summary);
        }
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(summaries) => {
                    for summary in summaries {
                        if let Some(current) = merged.get(summary.request_id.as_str())
                            && current != &summary
                        {
                            return Err(BifrostError::RunningQueryConflict.into());
                        }
                        merged.insert(summary.request_id.as_str().to_owned(), summary);
                    }
                }
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        Ok(merged.into_values().collect())
    }

    /// Gets the sole current-ready owner for one authenticated request.
    ///
    /// # Errors
    /// Returns the locked permission, audit, not-found, conflict, or unavailable error.
    pub async fn get(
        &self,
        tenant_id: DataTenantId,
        request_id: RequestId,
    ) -> Result<RunningQuerySummary, WyrdError> {
        let _membership = self.cluster.snapshot();
        let lookup = OracleLifecycleLookupRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let remote = self.lifecycle_transport.get(lookup).await;
        let mut owners = self
            .registry
            .list(tenant_id)
            .into_iter()
            .filter(|summary| summary.request_id == request_id)
            .collect::<Vec<_>>();
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(summary) => owners.push(summary),
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        match owners.len() {
            0 => Err(BifrostError::RunningQueryNotFound.into()),
            1 => Ok(owners.remove(0)),
            _ => Err(BifrostError::RunningQueryConflict.into()),
        }
    }

    /// Cancels the sole current-ready owner without retrying ambiguous mutation.
    ///
    /// # Errors
    /// Returns the locked permission, audit, not-found, conflict, or unavailable error.
    pub async fn cancel(
        &self,
        tenant_id: DataTenantId,
        request_id: RequestId,
    ) -> Result<CancelOracleLifecycleResponse, WyrdError> {
        let _membership = self.cluster.snapshot();
        let request = CancelOracleLifecycleRequest {
            tenant_id,
            request_id: request_id.clone(),
        };
        let remote = self.lifecycle_transport.cancel(request).await;
        let mut owners = Vec::new();
        if let Some(cancelled) = self.registry.cancel(tenant_id, &request_id) {
            owners.push(CancelOracleLifecycleResponse {
                request_id: cancelled.request_id,
                cancellation_started: cancelled.cancellation_started,
            });
        }
        for node in remote {
            match node.outcome {
                OracleLifecycleOutcome::Found(response) => owners.push(response),
                OracleLifecycleOutcome::Absent => {}
                OracleLifecycleOutcome::Unavailable => {
                    return Err(BifrostError::RunningQueryControlUnavailable.into());
                }
            }
        }
        match owners.len() {
            0 => Err(BifrostError::RunningQueryNotFound.into()),
            1 => Ok(owners.remove(0)),
            _ => Err(BifrostError::RunningQueryConflict.into()),
        }
    }
}
