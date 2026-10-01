//! Owner-local Oracle lifecycle service on the private peer listener.
//!
//! Callers are trusted cluster processes admitted by the peer mTLS listener;
//! each already authenticated and authorized the public principal whose
//! request it fans out. The receiver never widens that: every operation is
//! keyed by the carried tenant and answers only from entries this process's
//! registry owns for exactly that tenant, so a foreign or absent request is
//! indistinguishable from a missing one.

use std::sync::Arc;

use wyrd_spec::vala::api::{
    CancelOracleLifecycleRequest, CancelOracleLifecycleResponse, GetOracleLifecycleResponse,
    ListOracleLifecyclesRequest, ListOracleLifecyclesResponse, OracleLifecycleLookupRequest,
};
use wyrd_tonic::tonic::{Request, Response, Status};
use wyrd_tonic::wyrd::v1 as proto;
use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_server::{
    OracleLifecycleService, OracleLifecycleServiceServer,
};

/// Private generated adapter over the process-wide running-query registry.
pub struct OracleLifecycleGrpc {
    /// Canonical process-wide owner-local active-query registry.
    registry: Arc<vala_bifrost_redux::oracle::RunningQueryRegistry>,
}

impl OracleLifecycleGrpc {
    /// Creates the owner-local adapter around the canonical registry.
    #[must_use]
    pub fn new(runtime: Arc<crate::state::Oracle>) -> Self {
        Self {
            registry: Arc::clone(runtime.running_queries()),
        }
    }

    /// Creates a test adapter over a populated registry.
    #[cfg(test)]
    fn new_for_test(registry: Arc<vala_bifrost_redux::oracle::RunningQueryRegistry>) -> Self {
        Self { registry }
    }

    /// Returns the generated tonic server wrapper.
    #[must_use]
    pub fn into_server(self) -> OracleLifecycleServiceServer<Self> {
        OracleLifecycleServiceServer::new(self)
    }

    /// Returns the exact local tenant/request summary without cut reconstruction.
    fn local_summary(
        &self,
        request: &OracleLifecycleLookupRequest,
    ) -> Option<wyrd_spec::vala::api::RunningQuerySummary> {
        self.registry
            .list(request.tenant_id)
            .into_iter()
            .find(|query| query.request_id == request.request_id)
    }
}

#[wyrd_tonic::tonic::async_trait]
impl OracleLifecycleService for OracleLifecycleGrpc {
    /// Lists every owner-local entry for the carried tenant.
    ///
    /// # Errors
    ///
    /// Returns `INVALID_ARGUMENT` when the payload does not convert.
    async fn list_lifecycles(
        &self,
        request: Request<proto::ListOracleLifecyclesRequest>,
    ) -> Result<Response<proto::ListOracleLifecyclesResponse>, Status> {
        let listing = ListOracleLifecyclesRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let queries = self.registry.list(listing.tenant_id);
        Ok(Response::new(
            ListOracleLifecyclesResponse { queries }.into(),
        ))
    }

    /// Gets only the exact owner-local tenant/request entry.
    ///
    /// # Errors
    ///
    /// Returns a payload-conversion status, or opaque `NOT_FOUND` when this
    /// process owns no entry for that tenant and request.
    async fn get_lifecycle(
        &self,
        request: Request<proto::GetOracleLifecycleRequest>,
    ) -> Result<Response<proto::GetOracleLifecycleResponse>, Status> {
        let lookup = OracleLifecycleLookupRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let query = self
            .local_summary(&lookup)
            .ok_or_else(|| Status::not_found("Oracle lifecycle is not owned locally"))?;
        Ok(Response::new(GetOracleLifecycleResponse { query }.into()))
    }

    /// Cancels only the exact owner-local tenant/request entry.
    ///
    /// # Errors
    ///
    /// Returns a payload-conversion status, or opaque `NOT_FOUND` when this
    /// process owns no entry for that tenant and request.
    async fn cancel_lifecycle(
        &self,
        request: Request<proto::CancelOracleLifecycleRequest>,
    ) -> Result<Response<proto::CancelOracleLifecycleResponse>, Status> {
        let request = CancelOracleLifecycleRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        let cancelled = self
            .registry
            .cancel(request.tenant_id, &request.request_id)
            .ok_or_else(|| Status::not_found("Oracle lifecycle is not owned locally"))?;
        Ok(Response::new(
            CancelOracleLifecycleResponse {
                request_id: cancelled.request_id,
                cancellation_started: cancelled.cancellation_started,
            }
            .into(),
        ))
    }
}

/// Tenant-scoped lifecycle adapter proofs and shared registry fixtures.
#[cfg(test)]
pub(crate) mod pg_tests {
    use std::sync::Arc;

    use chrono::Utc;
    use vala_bifrost_redux::cluster::ClusterSnapshot;
    use vala_bifrost_redux::oracle::{
        OracleQueryAttemptCut, RunningQueryEntry, RunningQueryRegistry,
    };
    use wyrd_spec::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        ClusterCapabilities, ClusterNodeKey, ClusterRole, ClusterRoleLease, NodeId,
        OracleCapabilitiesV1, QueryClass,
    };
    use wyrd_tonic::tonic::Request;
    use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_server::OracleLifecycleService;

    use super::OracleLifecycleGrpc;

    /// Builds one ready Oracle lease for an immutable owner-local cut.
    fn oracle_lease(now: chrono::DateTime<Utc>) -> ClusterRoleLease {
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                role: ClusterRole::Oracle,
            },
            address: "https://oracle.local:50052".to_owned(),
            fencing_token: 1,
            capability_version: 1,
            capabilities: ClusterCapabilities::OracleV1(OracleCapabilitiesV1 {
                storage_protocol_version: 1,
                cpu_cores: 1.0,
                memory_budget_bytes: 1024,
                cpu_cores_per_slot: 1.0,
                memory_bytes_per_slot: 1024,
                raw_slots: 1,
                usable_slots: 1,
                supported_classes: vec![QueryClass::Interactive],
                max_workers_per_query: 1,
            }),
            ready: true,
            started_at: now,
            heartbeat_at: now,
        }
    }

    /// Builds one active owner-local query entry without exposing its cut.
    ///
    /// # Panics
    ///
    /// Panics when the ready Oracle lease cannot produce an immutable cut.
    pub(crate) fn running_entry(tenant: DataTenantId, request_id: RequestId) -> RunningQueryEntry {
        let now = Utc::now();
        let oracle = oracle_lease(now);
        let cut = OracleQueryAttemptCut::try_from_snapshot(
            &ClusterSnapshot::observed(vec![oracle.clone()], now),
            wyrd_spec::vala::api::QueryId::new(uuid::Uuid::now_v7()),
            oracle.key.node_id,
            QueryClass::Interactive,
            now + chrono::Duration::seconds(10),
            now,
            std::time::Duration::from_secs(15),
        )
        .expect("invariant: ready Oracle fixture produces a cut");
        RunningQueryEntry::new(tenant, request_id, QueryClass::Interactive, now, cut)
    }

    /// Every lifecycle operation answers only from the carried tenant's entries.
    ///
    /// A request naming another tenant's request id is the same opaque absence
    /// as a missing request, and a refused cancel leaves the foreign entry
    /// running.
    ///
    /// # Panics
    ///
    /// Panics when a lifecycle operation crosses the carried tenant boundary.
    #[tokio::test]
    async fn lifecycle_operations_are_scoped_to_the_carried_tenant() {
        let owner = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let owner_request_id = RequestId::now_v7();
        let other_request_id = RequestId::now_v7();
        let registry = Arc::new(RunningQueryRegistry::new());
        assert!(registry.insert(running_entry(owner, owner_request_id.clone())));
        assert!(registry.insert(running_entry(other, other_request_id.clone())));
        let service = OracleLifecycleGrpc::new_for_test(Arc::clone(&registry));

        let listed = service
            .list_lifecycles(Request::new(
                wyrd_spec::vala::api::ListOracleLifecyclesRequest { tenant_id: owner }.into(),
            ))
            .await
            .expect("owner list succeeds")
            .into_inner();
        assert_eq!(listed.queries.len(), 1);
        let got = service
            .get_lifecycle(Request::new(
                wyrd_spec::vala::api::OracleLifecycleLookupRequest {
                    tenant_id: owner,
                    request_id: owner_request_id.clone(),
                }
                .into(),
            ))
            .await
            .expect("owner get succeeds")
            .into_inner();
        let got_query = got.query.expect("owner summary is present");
        assert_eq!(got_query.request_id, owner_request_id.to_string());
        assert_eq!(listed.queries[0], got_query);

        let error = service
            .get_lifecycle(Request::new(
                wyrd_spec::vala::api::OracleLifecycleLookupRequest {
                    tenant_id: owner,
                    request_id: other_request_id.clone(),
                }
                .into(),
            ))
            .await
            .expect_err("a foreign request id under the owner tenant stays opaque");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        let error = service
            .cancel_lifecycle(Request::new(
                wyrd_spec::vala::api::CancelOracleLifecycleRequest {
                    tenant_id: owner,
                    request_id: other_request_id.clone(),
                }
                .into(),
            ))
            .await
            .expect_err("a foreign cancel stays opaque");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        assert!(registry.get(other, &other_request_id).is_some());

        let cancel = wyrd_spec::vala::api::CancelOracleLifecycleRequest {
            tenant_id: owner,
            request_id: owner_request_id.clone(),
        };
        let first = service
            .cancel_lifecycle(Request::new(cancel.clone().into()))
            .await
            .expect("owner cancel succeeds")
            .into_inner();
        assert!(first.cancellation_started);
        let second = service
            .cancel_lifecycle(Request::new(cancel.into()))
            .await
            .expect("repeated owner cancel succeeds")
            .into_inner();
        assert!(!second.cancellation_started);
    }
}
