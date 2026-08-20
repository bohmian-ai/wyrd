//! Private Oracle peer authority owned by the server boot boundary.

use std::sync::Arc;
use vala_bifrost_redux::oracle::dispatcher::OraclePeerWorker;

mod audit_wal;
mod lifecycle_controls;
/// Private owner-local lifecycle transport over the canonical query runtime.
pub(crate) mod lifecycle_service;
mod lifecycle_transport;
mod peer_audit;
mod peer_authority;
mod peer_credentials;
mod peer_service;
mod query_audit;
mod tail_audit;
mod tail_authority;
mod tail_discovery;

pub use lifecycle_controls::RunningQueryControls;
pub use lifecycle_service::OracleLifecycleGrpc;
pub use lifecycle_transport::{
    OracleLifecycleNodeOutcome, OracleLifecycleOutcome, OracleLifecycleTransport,
};
pub use peer_audit::PostgresPeerSecurityAudit;
pub use peer_authority::OraclePeerAuthority;
pub use peer_credentials::ServerOraclePeerCredentials;
pub use peer_service::OraclePeerGrpc;
#[cfg(feature = "test-support")]
pub use query_audit::{
    AuditRelayPauseGuard, AuditTelemetryLabelDomains, audit_telemetry_label_domains,
};
pub use query_audit::{AuditShutdownReport, OracleAuditPublisher};
pub use tail_audit::PostgresTailSecurityAudit;
pub use tail_authority::ScribeTailAuthority;
pub use tail_discovery::RegistryTailStreamDiscovery;

/// Oracle-owned private peer service retained by the Oracle runtime.
#[derive(Clone)]
pub struct OraclePeerRuntime {
    /// Fenced worker whose authority uses the retained verified audit writer.
    worker: Arc<OraclePeerWorker>,
    /// Verified writer retained for the full peer-service lifetime.
    security_audit: Arc<PostgresPeerSecurityAudit>,
    /// Canonical authenticated client retained for lifecycle control fanout.
    lifecycle_transport: Arc<OracleLifecycleTransport>,
}

impl OraclePeerRuntime {
    /// Binds a worker to the exact audit writer used during its construction.
    #[must_use]
    pub fn new(
        worker: Arc<OraclePeerWorker>,
        security_audit: Arc<PostgresPeerSecurityAudit>,
        lifecycle_transport: Arc<OracleLifecycleTransport>,
    ) -> Self {
        Self {
            worker,
            security_audit,
            lifecycle_transport,
        }
    }
}

impl OraclePeerRuntime {
    /// Returns the fenced worker mounted by the generated tonic service.
    #[must_use]
    pub fn worker(&self) -> Arc<OraclePeerWorker> {
        Arc::clone(&self.worker)
    }

    /// Returns the scrubbed security writer retained by the peer service.
    #[must_use]
    pub fn security_audit(&self) -> Arc<PostgresPeerSecurityAudit> {
        Arc::clone(&self.security_audit)
    }

    /// Returns the one authenticated lifecycle transport built at boot.
    #[must_use]
    pub fn lifecycle_transport(&self) -> Arc<OracleLifecycleTransport> {
        Arc::clone(&self.lifecycle_transport)
    }
}

/// PostgreSQL-backed production composition and mounted-transport proofs.
#[cfg(test)]
pub(crate) mod pg_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use tempfile::tempdir;
    use vala_bifrost_redux::cluster::ClusterRegistry;
    use vala_bifrost_redux::oracle::dispatcher::{DispatchError, OraclePeerCredentials};
    use vala_bifrost_redux::scribe::ScribeImpl;
    use vala_bifrost_redux::scribe::tail_rpc::{
        TailTicketAudience, TailTicketClaims, TailTicketMinter,
    };
    use vala_bifrost_redux::scribe::wal::{WalConfig, WalWriter};
    use wyrd_auth_issue::IssuingKey;
    use wyrd_auth_verify::TokenPrincipalRef;
    use wyrd_runtime::{PrincipalId, RoleRef};
    use wyrd_spec::auth::PrincipalKindTag;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        CancelOracleLifecycleRequest, NodeId as ClusterNodeId, OracleCapabilitiesV1,
        OracleLifecycleLookupRequest, QueryClass, ScribeCapabilitiesV1,
    };
    use wyrd_tonic::tonic::transport::{Channel, Endpoint};
    use wyrd_tonic::tonic_health::server::health_reporter;
    use wyrd_tonic::wyrd::v1;
    use wyrd_tonic::wyrd::v1::bifrost_query_service_client::BifrostQueryServiceClient;
    use wyrd_tonic::wyrd::v1::oracle_lifecycle_service_client::OracleLifecycleServiceClient;
    use wyrd_tonic::wyrd::v1::oracle_peer_service_client::OraclePeerServiceClient;
    use wyrd_tonic::wyrd::v1::scribe_tail_service_client::ScribeTailServiceClient;

    use super::OracleLifecycleOutcome;
    use crate::state::{AppState, Scribe};

    /// Credential fixture that records acquisition and rejects one normal call.
    struct CredentialProbe {
        /// Production-signed credential owner installed by the boot fixture.
        inner: Arc<dyn OraclePeerCredentials>,
        /// Number of non-refresh acquisitions observed across boot and calls.
        normal_calls: AtomicUsize,
        /// Number of forced refresh acquisitions observed after rejection.
        forced_refreshes: AtomicUsize,
    }

    #[async_trait]
    impl OraclePeerCredentials for CredentialProbe {
        /// Returns real signed credentials except for the first transport call.
        ///
        /// # Errors
        ///
        /// Propagates an error from the production-shaped credential fixture.
        async fn bearer(&self, force_refresh: bool) -> Result<String, DispatchError> {
            if force_refresh {
                self.forced_refreshes.fetch_add(1, Ordering::SeqCst);
                return self.inner.bearer(true).await;
            }
            let call = self.normal_calls.fetch_add(1, Ordering::SeqCst);
            if call == 2 {
                return Ok("rejected-test-bearer".to_owned());
            }
            self.inner.bearer(false).await
        }
    }

    /// Builds one immutable production-shaped role state and its peer bearer.
    ///
    /// # Panics
    ///
    /// Panics when shared Postgres, authentication, Scribe, or Oracle fixtures
    /// cannot establish the requested topology.
    pub(crate) async fn real_api_serving_state(
        role: crate::config::BifrostTarget,
    ) -> (AppState, String) {
        let (state, bearer, _) = real_api_serving_state_inner(role, false).await;
        (state, bearer)
    }

    /// Builds production composition with an observable credential refresh probe.
    ///
    /// # Panics
    ///
    /// Panics when the production-shaped fixture cannot install the probe.
    async fn real_api_serving_state_with_probe(
        role: crate::config::BifrostTarget,
    ) -> (AppState, String, Arc<CredentialProbe>) {
        let (state, bearer, probe) = real_api_serving_state_inner(role, true).await;
        (
            state,
            bearer,
            probe.expect("credential probe requested by fixture"),
        )
    }

    /// Builds the shared production-shaped state with optional credential observation.
    ///
    /// # Panics
    ///
    /// Panics when shared Postgres, authentication, Scribe, or Oracle fixtures
    /// cannot establish the requested topology.
    async fn real_api_serving_state_inner(
        role: crate::config::BifrostTarget,
        observe_credentials: bool,
    ) -> (AppState, String, Option<Arc<CredentialProbe>>) {
        let state = AppState::new(
            crate::test_support::test_server_postgres().await,
            crate::test_support::test_storage().await,
            crate::test_support::test_catalog().await,
        )
        .with_bifrost_resources(crate::boot::pg_tests::oracle_scribe_test_resources());
        let mut config = crate::config::WyrdServerConfig::default();
        config.role = role;
        assert!(
            config
                .bifrost_roles()
                .contains(&crate::config::BifrostRuntimeRole::Oracle)
        );
        assert!(
            config
                .bifrost_roles()
                .contains(&crate::config::BifrostRuntimeRole::Scribe)
        );
        let signing_key = IssuingKey::generate_ephemeral_pem().expect("test signing key");
        config.auth.signing_key = Some(signing_key.clone());
        let (state, base_credentials) =
            crate::boot::pg_tests::with_test_oracle_peer_credentials(state, &config, &signing_key)
                .await;
        let probe = observe_credentials.then(|| {
            Arc::new(CredentialProbe {
                inner: Arc::clone(&base_credentials),
                normal_calls: AtomicUsize::new(0),
                forced_refreshes: AtomicUsize::new(0),
            })
        });
        let credentials: Arc<dyn OraclePeerCredentials> = probe.as_ref().map_or_else(
            || Arc::clone(&base_credentials),
            |probe| Arc::clone(probe) as Arc<dyn OraclePeerCredentials>,
        );
        let peer_bearer = credentials
            .bearer(false)
            .await
            .expect("test peer credential resolves");
        let node_id = ClusterNodeId::new(uuid::Uuid::now_v7());
        let cluster = Arc::new(ClusterRegistry::new(state.postgres.vala().clone(), node_id));
        let scribe_role = cluster
            .register_scribe(
                "http://127.0.0.1:0",
                ScribeCapabilitiesV1 {
                    tail_protocol_version: 1,
                },
            )
            .await
            .expect("Scribe fence registers");
        cluster
            .activate(&scribe_role)
            .await
            .expect("Scribe fence activates");
        cluster
            .refresh_snapshot()
            .await
            .expect("Scribe snapshot refreshes");
        let writer_epoch = i64::try_from(scribe_role.fencing_token)
            .expect("Scribe fencing token fits writer epoch");
        let wal_root = tempdir().expect("Scribe WAL root");
        let wal = Arc::new(
            WalWriter::new(
                wal_root.path(),
                *uuid::Uuid::now_v7().as_bytes(),
                writer_epoch,
                WalConfig::default(),
            )
            .expect("Scribe WAL"),
        );
        let scribe = Arc::new(ScribeImpl::new_for_embedded_with_deps_and_catalog(
            Arc::new(state.storage.operator().clone()),
            wal,
            &uuid::Uuid::now_v7().to_string(),
            writer_epoch,
            Arc::clone(state.bifrost_catalog()),
        ));
        let tail_audit = Arc::new(
            super::PostgresTailSecurityAudit::try_new(&state.postgres)
                .await
                .expect("tail audit initializes"),
        );
        let ingest = Scribe::new(scribe, None)
            .with_tail_authority(Arc::new(
                super::ScribeTailAuthority::from_pem(&signing_key, tail_audit)
                    .expect("tail authority initializes"),
            ))
            .with_scribe_role(Arc::clone(&cluster), scribe_role.clone());
        let state = state.with_bifrost_ingest(Arc::new(ingest));
        let state = crate::boot::compose_test_api_serving_target(
            state,
            &config,
            &signing_key,
            node_id,
            cluster,
            credentials,
        )
        .await
        .expect("production-shaped combined runtime attaches");
        let verifier = state.auth.token_verifier.clone().expect("test verifier");
        let state = state.compose_bifrost_gate(verifier, config.bifrost.scribe.ingest_limits());
        (state, peer_bearer, probe)
    }

    /// Issues a production-shaped user bearer for the mounted lifecycle tenant.
    ///
    /// # Panics
    ///
    /// Panics when the production boot fixture lacks its issuer or token signing fails.
    pub(crate) fn tenant_bearer(state: &AppState, tenant: wyrd_spec::DataTenantId) -> String {
        let principal = TokenPrincipalRef {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKindTag::User,
            tenant_id: tenant,
            card_ref: None,
            card_ref_scope: Default::default(),
        };
        state
            .auth
            .issuing_key
            .as_ref()
            .expect("production-shaped state retains issuer")
            .issue_user_access_token(
                principal,
                Vec::<RoleRef>::new(),
                chrono::Duration::minutes(5),
            )
            .expect("mounted lifecycle bearer signs")
    }

    /// Serves the fully composed application router through a real TCP channel.
    ///
    /// # Panics
    ///
    /// Panics when the loopback listener, router, or client channel cannot start.
    pub(crate) async fn serve(state: &AppState) -> (Channel, tokio_util::sync::CancellationToken) {
        let (_, health) = health_reporter();
        let router = crate::grpc::build_app_grpc(
            state,
            health,
            crate::grpc::GrpcRouterConfig {
                reflection_enabled: false,
                tls_identity: None,
            },
        )
        .expect("real gRPC composition mounts selected capabilities");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener binds");
        let address = listener.local_addr().expect("listener has local address");
        let shutdown = tokio_util::sync::CancellationToken::new();
        let server_shutdown = shutdown.clone();
        tokio::spawn(async move {
            router
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    server_shutdown.cancelled_owned(),
                )
                .await
                .expect("application gRPC server runs");
        });
        let channel = Endpoint::from_shared(format!("http://{address}"))
            .expect("loopback endpoint is valid")
            .connect()
            .await
            .expect("application gRPC channel connects");
        (channel, shutdown)
    }

    /// Adds the production peer bearer to one generated client request.
    ///
    /// # Panics
    ///
    /// Panics only when a signed bearer cannot be represented as gRPC metadata.
    pub(crate) fn peer_request<T>(body: T, bearer: &str) -> wyrd_tonic::tonic::Request<T> {
        let mut request = wyrd_tonic::tonic::Request::new(body);
        request.metadata_mut().insert(
            "x-wyrd-access-token",
            format!("Bearer {bearer}")
                .parse()
                .expect("signed bearer is valid metadata"),
        );
        request
    }

    /// Probes the public query adapter and distinguishes a mount from absence.
    ///
    /// # Panics
    ///
    /// Panics when a mounted public query surface reports `UNIMPLEMENTED`.
    async fn assert_public_query_mounted(channel: Channel) {
        let error = BifrostQueryServiceClient::new(channel)
            .query(v1::BifrostQueryRequest {
                sql: "SELECT 1".to_owned(),
                visibility: v1::VisibilityMode::PublishedOnly.into(),
                freshness: v1::FreshnessPolicy::Strict.into(),
                deadline_ms: None,
            })
            .await
            .expect_err("missing bearer is rejected by the mounted query adapter");
        assert_eq!(error.code(), wyrd_tonic::tonic::Code::Unauthenticated);
    }

    /// Proves all four role topologies through immutable composition and TCP gRPC.
    ///
    /// Oracle and Scribe positive probes enter their generated adapters with a
    /// production-signed service bearer for both API-serving targets.
    ///
    /// # Panics
    ///
    /// Panics when production composition, identity, service presence, role
    /// fencing, authentication, or shutdown differs from the closed matrix.
    #[test]
    fn server_role_matrix_mounts_exact_peer_capabilities() {
        wyrd_runtime::runtime().block_on(async {
            let (mut all_state, all_bearer) =
                real_api_serving_state(crate::config::BifrostTarget::All).await;
            let (mut server_state, server_bearer) =
                real_api_serving_state(crate::config::BifrostTarget::Server).await;
            for (state, bearer) in [(&all_state, all_bearer), (&server_state, server_bearer)] {
                let combined = state.oracle_peer().expect("combined peer");
                assert_ne!(combined.scribe().registered_role().fencing_token, 0);
                assert!(Arc::ptr_eq(
                    &state.bifrost_query().expect("combined runtime").peer(),
                    &combined
                ));
                let (channel, shutdown) = serve(state).await;
                let oracle_snapshot = state
                    .oracle_peer()
                    .as_ref()
                    .map(|runtime| runtime.oracle())
                    .map(|oracle| oracle.cluster().snapshot());
                let oracle_lease = oracle_snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.live_oracles().first().cloned());
                let reserve =
                    oracle_lease.map_or_else(v1::ReserveNodeSlotsRequest::default, |lease| {
                        v1::ReserveNodeSlotsRequest {
                            query_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
                            leader_node_id: lease.key.node_id.as_uuid().to_string(),
                            leader_fencing_token: lease.fencing_token,
                            query_class: v1::QueryClass::Interactive.into(),
                            slot_units: 1,
                            expires_at_unix_ms: u64::try_from(
                                (chrono::Utc::now() + chrono::Duration::seconds(5))
                                    .timestamp_millis(),
                            )
                            .expect("future test deadline is nonnegative"),
                        }
                    });
                let peer_result = OraclePeerServiceClient::new(channel.clone())
                    .reserve_slots(peer_request(reserve, &bearer))
                    .await;
                peer_result.expect("mounted Oracle reservation succeeds");
                let tail_query_id = uuid::Uuid::now_v7();
                let tail_tenant = crate::test_support::test_tenant().await;
                let tail_stream = combined.scribe().tail_reader().stream_identity();
                let tail_request = v1::ListActiveStreamsRequest {
                    tail_ticket: combined
                        .scribe()
                        .tail_authority()
                        .expect("production tail authority")
                        .mint_tail_ticket(&TailTicketClaims {
                            query_id: tail_query_id,
                            tenant_id: tail_tenant,
                            canonical_table: "vala.bifrost.role_matrix".to_owned(),
                            node_id: tail_stream.node_id.as_uuid(),
                            writer_epoch: u64::try_from(tail_stream.writer_epoch.as_i64())
                                .expect("test writer epoch is nonnegative"),
                            deadline: chrono::Utc::now() + chrono::Duration::seconds(5),
                            audience: TailTicketAudience::List,
                            nonce: uuid::Uuid::now_v7().as_bytes().to_vec(),
                        })
                        .expect("production tail ticket signs"),
                    binding: Some(v1::TenantTableBinding {
                        tenant_id: tail_tenant.to_string(),
                        namespace: "bifrost".to_owned(),
                        table: "role_matrix".to_owned(),
                    }),
                    query_id: tail_query_id.as_bytes().to_vec(),
                };
                let tail_result = ScribeTailServiceClient::new(channel.clone())
                    .list_active_streams(peer_request(tail_request, &bearer))
                    .await;
                tail_result.expect("mounted Scribe listing succeeds");
                let lifecycle_tenant = wyrd_spec::DataTenantId::new_v7();
                let lifecycle_request = v1::ListOracleLifecyclesRequest {
                    tenant_id: lifecycle_tenant.to_string(),
                };
                let lifecycle_result = OracleLifecycleServiceClient::new(channel.clone())
                    .list_lifecycles(peer_request(
                        lifecycle_request,
                        &tenant_bearer(state, lifecycle_tenant),
                    ))
                    .await;
                lifecycle_result.expect("mounted lifecycle listing succeeds");
                assert_public_query_mounted(channel).await;
                shutdown.cancel();
            }
            all_state
                .bifrost_query()
                .cloned()
                .expect("Oracle runtime")
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .await;
            server_state
                .bifrost_query()
                .cloned()
                .expect("combined runtime")
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .await;
        });
    }

    /// Production composition invokes one registry through refreshing credentials.
    ///
    /// # Panics
    ///
    /// Panics when production composition, membership, authentication, refresh,
    /// owner-local registry identity, or lifecycle semantics diverge.
    #[test]
    pub(crate) fn authenticated_lifecycle_transport_reuses_canonical_credentials_and_registry() {
        wyrd_runtime::runtime().block_on(async {
            let (mut state, _, credentials) =
                real_api_serving_state_with_probe(crate::config::BifrostTarget::Server).await;
            let query = state.bifrost_query().expect("query runtime");
            let peer = state.oracle_peer().expect("combined peer");
            assert!(Arc::ptr_eq(
                query.lifecycle_transport(),
                &peer.oracle().lifecycle_transport(),
            ));

            let tenant = wyrd_spec::DataTenantId::new_v7();
            let request_id = RequestId::now_v7();
            assert!(query.running_queries().insert(
                crate::oracle::lifecycle_service::pg_tests::running_entry(
                    tenant,
                    request_id.clone(),
                ),
            ));
            let (_, health) = health_reporter();
            let router = crate::grpc::build_app_grpc(
                &state,
                health,
                crate::grpc::GrpcRouterConfig {
                    reflection_enabled: false,
                    tls_identity: None,
                },
            )
            .expect("lifecycle service mounts");
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("loopback listener binds");
            let address = listener.local_addr().expect("listener has address");
            let shutdown = tokio_util::sync::CancellationToken::new();
            let server_shutdown = shutdown.clone();
            tokio::spawn(async move {
                router
                    .serve_with_incoming_shutdown(
                        tokio_stream::wrappers::TcpListenerStream::new(listener),
                        server_shutdown.cancelled_owned(),
                    )
                    .await
                    .expect("lifecycle server runs");
            });

            let remote_node = ClusterNodeId::new(uuid::Uuid::now_v7());
            let remote_cluster = ClusterRegistry::new(state.postgres.vala().clone(), remote_node);
            remote_cluster
                .register_oracle(
                    &format!("http://{address}"),
                    OracleCapabilitiesV1 {
                        peer_protocol_version: 1,
                        storage_protocol_version: 1,
                        cpu_cores: 1.0,
                        memory_budget_bytes: 1_024,
                        cpu_cores_per_slot: 1.0,
                        memory_bytes_per_slot: 1_024,
                        raw_slots: 1,
                        usable_slots: 1,
                        supported_classes: vec![QueryClass::Interactive],
                        max_workers_per_query: 1,
                    },
                )
                .await
                .expect("remote Oracle registers");
            peer.oracle()
                .cluster()
                .refresh_snapshot()
                .await
                .expect("canonical cluster observes remote Oracle");

            let lookup = OracleLifecycleLookupRequest {
                tenant_id: tenant,
                request_id: request_id.clone(),
            };
            let listed = query.lifecycle_transport().list(lookup.clone()).await;
            assert_eq!(listed.len(), 1);
            assert!(matches!(
                &listed[0].outcome,
                OracleLifecycleOutcome::Found(queries)
                    if queries.len() == 1 && queries[0].request_id == request_id
            ));
            let got = query.lifecycle_transport().get(lookup).await;
            assert!(matches!(
                &got[0].outcome,
                OracleLifecycleOutcome::Found(summary) if summary.request_id == request_id
            ));
            let cancelled = query
                .lifecycle_transport()
                .cancel(CancelOracleLifecycleRequest {
                    tenant_id: tenant,
                    request_id: request_id.clone(),
                })
                .await;
            assert!(matches!(
                &cancelled[0].outcome,
                OracleLifecycleOutcome::Found(response)
                    if response.request_id == request_id && response.cancellation_started
            ));
            assert_eq!(credentials.normal_calls.load(Ordering::SeqCst), 5);
            assert_eq!(credentials.forced_refreshes.load(Ordering::SeqCst), 1);
            assert!(query.running_queries().get(tenant, &request_id).is_some());
            shutdown.cancel();
            state
                .bifrost_query()
                .cloned()
                .expect("query runtime")
                .shutdown(std::time::Instant::now() + std::time::Duration::from_secs(2))
                .await;
        });
    }
}
