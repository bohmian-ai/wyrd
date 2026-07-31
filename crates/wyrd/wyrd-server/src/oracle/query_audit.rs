//! Transactional outbox audit owner for retained Oracle queries.

use async_trait::async_trait;
use vala_bifrost_redux::oracle::{
    AuthorizedQueryContext, BifrostQueryReadDecision, BifrostSecurityViolation, OracleAudit,
    VerifiedSecurityContext,
};
use vala_sql::ValaPostgres;
use wyrd_spec::vala::api::{AuditDecision, AuditDetail, AuditEvent, AuditResult};
use wyrd_spec::vala::error::BifrostError;

/// Closed failure phases for the server Oracle audit transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OracleAuditStoreError {
    /// A tenant-scoped transaction could not be acquired.
    Acquire,
    /// The outbox event could not be appended.
    Append,
    /// The transaction could not be committed durably.
    Commit,
    /// The transaction task failed before returning its result.
    Join,
}

/// Durable store boundary used by the Oracle audit owner.
#[async_trait]
trait OracleAuditStore: Send + Sync {
    /// Appends and commits one event under the authenticated tenant.
    ///
    /// # Errors
    ///
    /// Returns the exact failed transaction phase.
    async fn commit_event(
        &self,
        tenant: wyrd_spec::DataTenantId,
        event: AuditEvent,
    ) -> Result<(), OracleAuditStoreError>;
}

/// Production SQL implementation of the Oracle audit store boundary.
struct PostgresOracleAuditStore {
    /// Tenant-scoped SQL root used to begin outbox transactions.
    vala: ValaPostgres,
}

#[async_trait]
impl OracleAuditStore for PostgresOracleAuditStore {
    /// Runs append and commit in one owned task and reports the exact failure phase.
    ///
    /// # Errors
    ///
    /// Returns acquire, append, commit, or task-join failure without exposing SQL
    /// details across the public query boundary.
    async fn commit_event(
        &self,
        tenant: wyrd_spec::DataTenantId,
        event: AuditEvent,
    ) -> Result<(), OracleAuditStoreError> {
        let vala = self.vala.clone();
        tokio::spawn(async move {
            let mut conn = vala
                .tenant_conn(tenant)
                .await
                .map_err(|_| OracleAuditStoreError::Acquire)?;
            vala_sql::queries::audit_outbox::append_audit(&mut conn, &event)
                .await
                .map_err(|_| OracleAuditStoreError::Append)?;
            conn.commit()
                .await
                .map_err(|_| OracleAuditStoreError::Commit)
        })
        .await
        .map_err(|_| OracleAuditStoreError::Join)?
    }
}

/// Commits Oracle read decisions and tenant violations through the standard outbox.
#[derive(Clone)]
pub struct ServerOracleAudit {
    /// Durable transaction owner used before Oracle performs any row read.
    store: std::sync::Arc<dyn OracleAuditStore>,
}

impl ServerOracleAudit {
    /// Creates the production Oracle audit collaborator from the shared SQL owner.
    #[must_use]
    pub fn new(vala: ValaPostgres) -> Self {
        Self {
            store: std::sync::Arc::new(PostgresOracleAuditStore { vala }),
        }
    }

    /// Appends and commits one scrubbed event under the authenticated tenant.
    ///
    /// The transaction is released before Oracle performs any row read. A failed
    /// append or commit is therefore fail-closed and cannot leak a response frame.
    ///
    /// # Errors
    ///
    /// Returns [`BifrostError::QueryAuditUnavailable`] when tenant connection
    /// acquisition, outbox append, or commit fails.
    async fn commit(
        &self,
        context: &AuthorizedQueryContext,
        operation: &str,
        result: AuditResult,
        detail: AuditDetail,
    ) -> Result<(), BifrostError> {
        let event = AuditEvent::new(
            context.request_id.clone(),
            context.trace_id.clone(),
            operation.to_owned(),
            "bifrost.query".to_owned(),
            context.principal.card_ref().cloned(),
            context.principal.id,
            context.principal.kind.tag(),
            context.auth_method,
            context.permission.clone(),
            AuditDecision::Allow,
            result,
            "scrubbed Bifrost query decision".to_owned(),
        )
        .with_detail(detail);
        self.store
            .commit_event(context.data_tenant_id, event)
            .await
            .map_err(|_| BifrostError::QueryAuditUnavailable)
    }
}

#[async_trait]
impl OracleAudit for ServerOracleAudit {
    /// Commits the immutable read decision before Oracle reads query rows.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the standard outbox transaction cannot
    /// commit.
    async fn append_read_decision(
        &self,
        context: &AuthorizedQueryContext,
        decision: BifrostQueryReadDecision,
    ) -> Result<(), BifrostError> {
        self.commit(
            context,
            "bifrost.query.read_decision",
            AuditResult::Success,
            decision.into_detail(),
        )
        .await
    }

    /// Commits a verified tenant-source violation as an independent failure event.
    ///
    /// # Errors
    ///
    /// Returns audit unavailable when the standard outbox transaction cannot
    /// commit.
    async fn append_security_violation(
        &self,
        context: VerifiedSecurityContext,
        violation: BifrostSecurityViolation,
    ) -> Result<(), BifrostError> {
        self.commit(
            &context.query,
            "bifrost.query.security_violation",
            AuditResult::Failure,
            AuditDetail::BifrostSecurityViolation {
                violation: violation.violation,
                phase: violation.phase,
                query_digest: context.query_digest,
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use arrow::datatypes::{DataType, Field, Schema};
    use axum::http::{StatusCode, header};
    use datafusion::catalog::default_table_source::provider_as_source;
    use datafusion::datasource::empty::EmptyTable;
    use datafusion::logical_expr::LogicalPlanBuilder;
    use vala_bifrost_redux::cluster::ClusterRegistry;
    use vala_bifrost_redux::oracle::{
        Oracle, OracleBuildConfig, OracleConfig, OracleMemoryResources, OracleSlotManager,
        QueryOptions, TailTransportDirectory,
    };
    use vala_bifrost_redux::scribe::memory::BifrostMemoryGovernor;
    use vala_sql::queries::oracle_admission::OracleAdmissionLeases;
    use wyrd_runtime::permission::{Permission, PermissionSet};
    use wyrd_runtime::{Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{
        AuthMethod, BifrostSecurityPhase, BifrostSecurityViolationKind, OracleCapabilitiesV1,
        QueryClass, VisibilityMode,
    };

    use super::*;

    /// Deterministic store that fails at one requested transaction phase.
    struct FailingStore {
        /// Failure returned to the server audit owner.
        failure: OracleAuditStoreError,
    }

    #[async_trait]
    impl OracleAuditStore for FailingStore {
        /// Returns the configured append or commit failure.
        ///
        /// # Errors
        ///
        /// Always returns the configured transaction failure.
        async fn commit_event(
            &self,
            _tenant: wyrd_spec::DataTenantId,
            _event: AuditEvent,
        ) -> Result<(), OracleAuditStoreError> {
            Err(self.failure)
        }
    }

    /// Builds one internally authenticated query context for audit fault tests.
    ///
    /// # Panics
    ///
    /// Panics if the static principal and tenant unexpectedly diverge.
    fn context() -> AuthorizedQueryContext {
        let tenant = "01890f28-7c4a-7000-98e7-4f4a3c2d1b01"
            .parse()
            .expect("static tenant");
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_query_read()]),
        );
        AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            Permission::bifrost_query_read().to_string(),
        )
        .expect("matching tenant context")
    }

    /// Builds a bounded structured detail for transaction fault tests.
    fn detail() -> AuditDetail {
        AuditDetail::BifrostSecurityViolation {
            violation: BifrostSecurityViolationKind::TenantRow,
            phase: BifrostSecurityPhase::Source,
            query_digest: None,
        }
    }

    /// Builds a ready Oracle whose production server audit fails at one store phase.
    ///
    /// # Panics
    ///
    /// Panics when shared SQL/catalog fixtures, membership, Oracle construction,
    /// or startup reconciliation fail.
    async fn oracle_with_failure(
        failure: OracleAuditStoreError,
    ) -> (
        Oracle,
        Arc<ClusterRegistry>,
        vala_bifrost_redux::cluster::RegisteredRole,
    ) {
        let node_id = wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7());
        let cluster = Arc::new(ClusterRegistry::new(
            crate::test_support::test_operator_pool().await,
            node_id,
        ));
        let role = cluster
            .reserve_oracle(
                "127.0.0.1:50052",
                OracleCapabilitiesV1 {
                    peer_protocol_version: 1,
                    storage_protocol_version: 1,
                    cpu_cores: 16.0,
                    memory_budget_bytes: 1024 * 1024 * 1024,
                    cpu_cores_per_slot: 1.0,
                    memory_bytes_per_slot: 64 * 1024 * 1024,
                    raw_slots: 16,
                    usable_slots: 16,
                    supported_classes: vec![QueryClass::Interactive, QueryClass::Analytical],
                    max_workers_per_query: 0,
                },
            )
            .await
            .expect("reserve Oracle");
        cluster.activate(&role).await.expect("activate Oracle");
        cluster.refresh_snapshot().await.expect("Oracle snapshot");
        let vala = crate::test_support::test_vala_postgres().await;
        let oracle = Oracle::new(OracleBuildConfig {
            catalog: crate::test_support::test_redux_catalog().await,
            vala,
            admission_leases: OracleAdmissionLeases::new(
                crate::test_support::test_operator_pool().await,
            ),
            cluster: Arc::clone(&cluster),
            local_role: role.clone(),
            local_slots: Arc::new(OracleSlotManager::new(16, 16)),
            memory: OracleMemoryResources {
                governor: BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("memory governor"),
                reconciliation_limit_bytes: 64 * 1024 * 1024,
            },
            tails: Arc::new(TailTransportDirectory::default()),
            audit: Arc::new(ServerOracleAudit {
                store: Arc::new(FailingStore { failure }),
            }),
            peer_ticket_minter: Arc::new(
                vala_bifrost_redux::oracle::peer::DeterministicTestSigner {
                    key_id: "test".to_owned(),
                },
            ),
            peer_transports: None,
            config: OracleConfig {
                tenant_interactive_slots: 16,
                tenant_analytical_slots: 16,
                max_workers_per_query: 0,
                ..OracleConfig::default()
            },
        })
        .expect("Oracle");
        tokio::time::timeout(Duration::from_secs(2), async {
            while !oracle.is_ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Oracle startup reconciliation");
        (oracle, cluster, role)
    }

    /// Proves append and commit faults both refuse a query before streaming.
    #[tokio::test]
    async fn append_and_commit_failures_are_fail_closed() {
        for failure in [OracleAuditStoreError::Append, OracleAuditStoreError::Commit] {
            let audit = ServerOracleAudit {
                store: Arc::new(FailingStore { failure }),
            };
            let error = audit
                .commit(
                    &context(),
                    "bifrost.query.read_decision",
                    AuditResult::Success,
                    detail(),
                )
                .await
                .expect_err("audit transaction fault must refuse query");
            assert_eq!(error, BifrostError::QueryAuditUnavailable);
        }
    }

    /// Proves Oracle audit faults reach both transports before any frame exists.
    #[test]
    fn oracle_audit_faults_are_prebyte_http_and_grpc_errors() {
        wyrd_runtime::runtime().block_on(async {
            for failure in [OracleAuditStoreError::Append, OracleAuditStoreError::Commit] {
                let (oracle, cluster, role) = oracle_with_failure(failure).await;
                let source =
                    provider_as_source(Arc::new(EmptyTable::new(Arc::new(Schema::new(vec![
                        Field::new("value", DataType::Int64, false),
                    ])))));
                let plan = LogicalPlanBuilder::scan("fixture", source, None)
                    .expect("fixture scan")
                    .build()
                    .expect("bound empty read plan");
                let result = oracle
                    .query_plan(
                        context(),
                        plan,
                        QueryOptions {
                            visibility: VisibilityMode::PublishedOnly,
                            deadline: Instant::now() + Duration::from_secs(2),
                        },
                    )
                    .await;
                assert!(
                    result.is_err(),
                    "audit failure must not construct an Oracle frame stream"
                );
                let error =
                    result.expect_err("audit failure must precede Oracle stream construction");
                assert_eq!(error, BifrostError::QueryAuditUnavailable);

                let http = crate::query::routes::query_error_response(error.clone().into());
                assert_eq!(http.status(), StatusCode::SERVICE_UNAVAILABLE);
                assert!(
                    !http.headers().contains_key("x-wyrd-schema-fingerprint"),
                    "audit failure must occur before HTTP schema metadata"
                );
                assert_ne!(
                    http.headers().get(header::CONTENT_TYPE),
                    Some(
                        &"application/vnd.wyrd.bifrost-query-stream"
                            .parse()
                            .expect("static query media type")
                    )
                );
                let grpc = crate::grpc::query::query_status(error.into());
                assert_eq!(grpc.code(), wyrd_tonic::tonic::Code::Unavailable);

                oracle
                    .shutdown(Instant::now() + Duration::from_secs(1))
                    .await;
                cluster
                    .shutdown_role(role)
                    .await
                    .expect("unregister Oracle");
            }
        });
    }
}
