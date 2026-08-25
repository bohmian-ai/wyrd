//! Authenticated owner-local Oracle lifecycle service.

use std::sync::Arc;

use wyrd_runtime::{Permission, Principal, PrincipalKind};
use wyrd_spec::DataTenantId;
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
    /// Exact verifier used at the private transport boundary.
    verifier: Arc<crate::state::WyrdTokenVerifier>,
    /// Canonical process-wide owner-local active-query registry.
    registry: Arc<vala_bifrost_redux::oracle::RunningQueryRegistry>,
}

impl OracleLifecycleGrpc {
    /// Creates the owner-local adapter around the canonical registry.
    #[must_use]
    pub fn new(
        verifier: Arc<crate::state::WyrdTokenVerifier>,
        runtime: Arc<crate::state::Oracle>,
    ) -> Self {
        Self {
            verifier,
            registry: Arc::clone(runtime.running_queries()),
        }
    }

    /// Creates a test adapter over a populated registry while retaining production auth.
    #[cfg(test)]
    fn new_for_test(
        state: crate::AppState,
        registry: Arc<vala_bifrost_redux::oracle::RunningQueryRegistry>,
    ) -> Self {
        Self {
            verifier: state
                .auth
                .token_verifier
                .clone()
                .expect("test lifecycle adapter requires a verifier"),
            registry,
        }
    }

    /// Returns the generated tonic server wrapper.
    #[must_use]
    pub fn into_server(self) -> OracleLifecycleServiceServer<Self> {
        OracleLifecycleServiceServer::new(self)
    }

    /// Recovers the authenticated tenant before request payloads are inspected.
    ///
    /// # Errors
    ///
    /// Returns `UNAUTHENTICATED` when the production verifier rejects the request metadata.
    async fn authenticated_principal(
        &self,
        metadata: &wyrd_tonic::tonic::metadata::MetadataMap,
    ) -> Result<Principal, Status> {
        vala_bifrost_redux::gate::auth::authenticate(self.verifier.as_ref(), metadata)
            .await
            .map(|auth| auth.principal)
            .map_err(|error| Status::unauthenticated(error.to_string()))
    }

    /// Rejects a payload tenant that differs from authenticated identity.
    ///
    /// # Errors
    ///
    /// Returns opaque `NOT_FOUND` when the authenticated and requested tenants differ.
    fn require_owner(authenticated: &Principal, requested: DataTenantId) -> Result<(), Status> {
        let tenant_self = authenticated.tenant_id == requested
            && !matches!(authenticated.kind, PrincipalKind::Service { .. });
        let platform_peer = authenticated.tenant_id == DataTenantId::SYSTEM_OWNER
            && matches!(authenticated.kind, PrincipalKind::Service { .. })
            && authenticated
                .effective_permissions
                .contains(&Permission::bifrost_oracle_peer_invoke());
        (tenant_self || platform_peer)
            .then_some(())
            .ok_or_else(|| Status::not_found("Oracle lifecycle is not owned locally"))
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
    /// Lists every owner-local entry for the authenticated tenant.
    ///
    /// # Errors
    ///
    /// Returns an authentication, payload-conversion, or tenant-ownership status.
    async fn list_lifecycles(
        &self,
        request: Request<proto::ListOracleLifecyclesRequest>,
    ) -> Result<Response<proto::ListOracleLifecyclesResponse>, Status> {
        let principal = self.authenticated_principal(request.metadata()).await?;
        let listing = ListOracleLifecyclesRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Self::require_owner(&principal, listing.tenant_id)?;
        let queries = self.registry.list(listing.tenant_id);
        Ok(Response::new(
            ListOracleLifecyclesResponse { queries }.into(),
        ))
    }

    /// Gets only the exact owner-local tenant/request entry.
    ///
    /// # Errors
    ///
    /// Returns an authentication, payload-conversion, tenant-ownership, or
    /// owner-local absence status.
    async fn get_lifecycle(
        &self,
        request: Request<proto::GetOracleLifecycleRequest>,
    ) -> Result<Response<proto::GetOracleLifecycleResponse>, Status> {
        let principal = self.authenticated_principal(request.metadata()).await?;
        let lookup = OracleLifecycleLookupRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Self::require_owner(&principal, lookup.tenant_id)?;
        let query = self
            .local_summary(&lookup)
            .ok_or_else(|| Status::not_found("Oracle lifecycle is not owned locally"))?;
        Ok(Response::new(GetOracleLifecycleResponse { query }.into()))
    }

    /// Cancels only the exact owner-local tenant/request entry.
    ///
    /// # Errors
    ///
    /// Returns an authentication, payload-conversion, tenant-ownership, or
    /// owner-local absence status.
    async fn cancel_lifecycle(
        &self,
        request: Request<proto::CancelOracleLifecycleRequest>,
    ) -> Result<Response<proto::CancelOracleLifecycleResponse>, Status> {
        let principal = self.authenticated_principal(request.metadata()).await?;
        let request = CancelOracleLifecycleRequest::try_from(request.into_inner())
            .map_err(|error| Status::invalid_argument(error.to_string()))?;
        Self::require_owner(&principal, request.tenant_id)?;
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

/// PostgreSQL-backed tenant and lifecycle adapter proofs.
#[cfg(test)]
pub(crate) mod pg_tests {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    use chrono::Utc;
    use secrecy::SecretString;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use vala_bifrost_redux::cluster::ClusterSnapshot;
    use vala_bifrost_redux::oracle::{
        OracleQueryAttemptCut, RunningQueryEntry, RunningQueryRegistry,
    };
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::{
        Kid, TokenPrincipalRef, TokenVerifier, WyrdAuthVerifySettings, public_key_from_pem,
    };
    use wyrd_runtime::permission::PermissionSet;
    use wyrd_runtime::{Permission, Principal, PrincipalId, PrincipalKind, RoleRef};
    use wyrd_semver::VersionBlock;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::{PrincipalId as AuthPrincipalId, PrincipalKindTag};
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::{CardRef, CardRefScope};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        CancelOracleLifecycleRequest, ClusterCapabilities, ClusterNodeKey, ClusterRole,
        ClusterRoleLease, NodeId, OracleCapabilitiesV1, QueryClass,
    };
    use wyrd_storage::{BackendSigner, LocalSigner, StorageHandle};
    use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_server::OracleLifecycleService;

    use super::OracleLifecycleGrpc;
    use crate::auth::permission_resolver::SqlPermissionResolver;
    use crate::components::auth::ServerAuth;
    use crate::postgres::ServerPostgres;

    const TEST_PRIVATE_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMC4CAQAwBQYDK2VwBCIEID78cHNjuFihX8aWPytQRoR2iUKHVXgdh92bcTcjQTYV\n-----END PRIVATE KEY-----\n";
    const TEST_PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n";

    /// Builds real server authentication handles and their matching issuer.
    ///
    /// # Panics
    ///
    /// Panics when cryptographic, SQL, storage, or catalog test fixtures cannot initialize.
    async fn authenticated_state() -> (crate::AppState, Arc<IssuingKey>) {
        let app_pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        let mut keys = HashMap::new();
        keys.insert(
            Kid::new("test").expect("test key id is valid"),
            Arc::new(public_key_from_pem(TEST_PUBLIC_KEY_PEM).expect("test key parses")),
        );
        let verifier = Arc::new(TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(SqlPermissionResolver::new(Arc::new(app_pool.clone()))),
            WyrdAuthVerifySettings {
                allowed_clock_skew: StdDuration::ZERO,
                ..WyrdAuthVerifySettings::default()
            },
        ));
        let issuing_key = Arc::new(
            IssuingKey::from_ed_pem(
                SecretString::from(TEST_PRIVATE_KEY_PEM),
                Kid::new("test").expect("test key id is valid"),
                "wyrd",
            )
            .expect("test issuing key parses"),
        );
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(app_pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(app_pool);
        let postgres = Arc::new(ServerPostgres::from_parts(wyrd, vala));
        let root = tempfile::tempdir().expect("test storage root");
        let signer = LocalSigner::new(root.path().to_path_buf()).expect("local signer");
        let state = crate::test_support::test_app_state(
            postgres,
            Arc::new(StorageHandle::new(BackendSigner::Local(signer))),
            crate::test_support::test_catalog().await,
        )
        .with_auth(ServerAuth {
            issuing_key: Some(Arc::clone(&issuing_key)),
            token_verifier: Some(verifier),
            ..ServerAuth::default()
        });
        (state, issuing_key)
    }

    /// Creates authenticated metadata for one tenant through the production JWT shape.
    ///
    /// # Panics
    ///
    /// Panics when the fixture token cannot be signed or encoded as metadata.
    fn authenticated_request<T>(
        body: T,
        issuer: &IssuingKey,
        tenant: DataTenantId,
    ) -> wyrd_tonic::tonic::Request<T> {
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindTag::User,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        let token = issuer
            .issue_user_access_token(
                principal,
                Vec::<RoleRef>::new(),
                chrono::Duration::minutes(5),
            )
            .expect("test token signs");
        let mut request = wyrd_tonic::tonic::Request::new(body);
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {token}")
                .parse()
                .expect("bearer metadata parses"),
        );
        request
    }

    /// Builds a signed Service request for the private adapter authority boundary.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot sign or encode the Service bearer.
    fn service_request<T>(
        body: T,
        issuer: &IssuingKey,
        tenant: DataTenantId,
        card_ref: CardRef,
    ) -> wyrd_tonic::tonic::Request<T> {
        let token = issuer
            .issue_service_access_token(
                PrincipalId::new(uuid::Uuid::now_v7()),
                tenant,
                card_ref.clone(),
                CardRefScope::own(&card_ref),
                Vec::<RoleRef>::new(),
                chrono::Duration::minutes(5),
            )
            .expect("test Service token signs");
        let mut request = wyrd_tonic::tonic::Request::new(body);
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {token}")
                .parse()
                .expect("Service bearer metadata parses"),
        );
        request
    }

    /// Builds one ready Oracle lease for an immutable owner-local cut.
    fn oracle_lease(now: chrono::DateTime<Utc>) -> ClusterRoleLease {
        ClusterRoleLease {
            key: ClusterNodeKey {
                node_id: NodeId::new(uuid::Uuid::now_v7()),
                role: ClusterRole::Oracle,
            },
            address: "http://oracle.local".to_owned(),
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

    /// Nonowners receive the same opaque absence as a missing local request.
    ///
    /// # Panics
    ///
    /// Panics when production JWT verification or the owner/nonowner lifecycle
    /// contract diverges from its tenant-safe behavior.
    #[tokio::test]
    async fn tenant_self_authority_remains_compatible() {
        let owner = DataTenantId::new_v7();
        let other = DataTenantId::new_v7();
        let owner_request_id = RequestId::now_v7();
        let other_request_id = RequestId::now_v7();
        let registry = Arc::new(RunningQueryRegistry::new());
        assert!(registry.insert(running_entry(owner, owner_request_id.clone())));
        assert!(registry.insert(running_entry(other, other_request_id.clone())));
        let (state, issuer) = authenticated_state().await;
        let service = OracleLifecycleGrpc::new_for_test(state, Arc::clone(&registry));
        let lookup = wyrd_spec::vala::api::OracleLifecycleLookupRequest {
            tenant_id: owner,
            request_id: owner_request_id.clone(),
        };
        let list = wyrd_spec::vala::api::ListOracleLifecyclesRequest { tenant_id: owner };
        let listed = service
            .list_lifecycles(authenticated_request(list.into(), &issuer, owner))
            .await
            .expect("owner list succeeds")
            .into_inner();
        assert_eq!(listed.queries.len(), 1);
        let got = service
            .get_lifecycle(authenticated_request(lookup.into(), &issuer, owner))
            .await
            .expect("owner get succeeds")
            .into_inner();
        let got_query = got.query.expect("owner summary is present");
        assert_eq!(got_query.request_id, owner_request_id.to_string());
        assert_eq!(listed.queries[0], got_query);
        let foreign = wyrd_spec::vala::api::OracleLifecycleLookupRequest {
            tenant_id: other,
            request_id: other_request_id.clone(),
        };
        let error = service
            .get_lifecycle(authenticated_request(
                foreign.clone().into(),
                &issuer,
                owner,
            ))
            .await
            .expect_err("cross-tenant lookup stays opaque");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        let foreign_cancel = wyrd_spec::vala::api::CancelOracleLifecycleRequest {
            tenant_id: other,
            request_id: other_request_id.clone(),
        };
        let error = service
            .cancel_lifecycle(authenticated_request(foreign_cancel.into(), &issuer, owner))
            .await
            .expect_err("cross-tenant cancel stays opaque");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        assert!(registry.get(other, &other_request_id).is_some());
        let other_visible = service
            .get_lifecycle(authenticated_request(foreign.into(), &issuer, other))
            .await
            .expect("other owner sees its own query")
            .into_inner();
        assert_eq!(
            other_visible.query.expect("other summary").request_id,
            other_request_id.to_string()
        );
        let cancel = wyrd_spec::vala::api::CancelOracleLifecycleRequest {
            tenant_id: owner,
            request_id: owner_request_id.clone(),
        };
        let first = service
            .cancel_lifecycle(authenticated_request(cancel.clone().into(), &issuer, owner))
            .await
            .expect("owner cancel succeeds")
            .into_inner();
        assert!(first.cancellation_started);
        let second = service
            .cancel_lifecycle(authenticated_request(cancel.into(), &issuer, owner))
            .await
            .expect("repeated owner cancel succeeds")
            .into_inner();
        assert!(!second.cancellation_started);
    }

    /// Platform peer authority requires the exact platform Service permission tuple.
    #[tokio::test]
    async fn platform_peer_authority_is_tenant_scoped_and_non_disclosing() {
        let tenant = DataTenantId::new_v7();
        let card_ref = CardRef {
            kind: CardKind::Service,
            name: CardName::new("oracle-peer").expect("name"),
            version: VersionBlock::parse("1.0.0").expect("version"),
            space: SpaceName::new("system").expect("space"),
            uid: None,
        };
        let service_kind = PrincipalKind::Service {
            card_ref: card_ref.clone(),
            card_ref_scope: CardRefScope::own(&card_ref),
        };
        let principal = |kind, tenant_id, permissions| Principal {
            id: AuthPrincipalId::new(uuid::Uuid::now_v7()),
            kind,
            tenant_id,
            roles: Vec::new(),
            effective_permissions: permissions,
        };
        let allowed = principal(
            service_kind.clone(),
            DataTenantId::SYSTEM_OWNER,
            PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
        );
        assert!(OracleLifecycleGrpc::require_owner(&allowed, tenant).is_ok());
        for denied in [
            principal(
                PrincipalKind::User,
                DataTenantId::SYSTEM_OWNER,
                PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
            ),
            principal(
                service_kind.clone(),
                tenant,
                PermissionSet::from_iter([Permission::bifrost_oracle_peer_invoke()]),
            ),
            principal(
                service_kind,
                DataTenantId::SYSTEM_OWNER,
                PermissionSet::new(),
            ),
        ] {
            let error = OracleLifecycleGrpc::require_owner(&denied, tenant)
                .expect_err("noncanonical peer is denied");
            assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        }

        let request_id = RequestId::now_v7();
        let registry = Arc::new(RunningQueryRegistry::new());
        assert!(registry.insert(running_entry(tenant, request_id.clone())));
        let (state, issuer) = authenticated_state().await;
        let service = OracleLifecycleGrpc::new_for_test(state, Arc::clone(&registry));
        let denied_cancel = CancelOracleLifecycleRequest {
            tenant_id: tenant,
            request_id: request_id.clone(),
        };
        let error = service
            .cancel_lifecycle(service_request(
                denied_cancel.into(),
                &issuer,
                tenant,
                card_ref,
            ))
            .await
            .expect_err("tenant Service cannot enter tenant-self authority");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::NotFound);
        assert!(registry.get(tenant, &request_id).is_some());
    }
}
