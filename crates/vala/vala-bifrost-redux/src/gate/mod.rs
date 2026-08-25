//! Bifrost Gate — the crate-owned authentication, admission, and ingest-routing owner.
//!
//! Gate is the single owner of the Bifrost write boundary. It authenticates one
//! caller against the shared token verifier, enforces the boot-fixed transport
//! and typed-ingest limits, authorizes the record-write permission, resolves the
//! target table, builds the audit event, and hands exactly one bounded frame to
//! Scribe. Gate names no transport type and returns no transport status.
//!
//! The serving surface (`wyrd-server`) owns the tonic and axum adapters, the
//! per-request lifecycle metrics keyed on transport outcome, and Oracle query
//! forwarding. It composes exactly one `Gate` at boot and calls these methods;
//! it never builds a second admission, limit, or authentication owner.

pub mod auth;
pub mod error;
pub mod limits;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use wyrd_auth_oidc::IssuerConfigResolver;
use wyrd_auth_verify::{PermissionResolver, TokenVerifier};
use wyrd_runtime::PermissionCheck;
use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;
use wyrd_tonic::tonic::metadata::MetadataMap;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

use crate::catalog::TableRef;
use crate::contracts::{
    DecodedOtlp, IngressPayload, OtlpDecodeOwner, Scribe, ScribeIngressFrame, ScribeOtlpOutcome,
};
pub use crate::gate::auth::{AuthContext, WYRD_REQUEST_ID_METADATA};
pub use crate::gate::error::IngestError;
use crate::gate::limits::BifrostTransportAdmission;
pub use crate::gate::limits::{IngestLimits, OtlpWireLimits};
use crate::namespaces::BifrostNamespace;
pub use crate::otlp_contract::{IngestOutcome, LogsOutcome, MetricsOutcome};
use wyrd_spec::vala::error::BifrostError;

/// The one Bifrost public authentication, admission, and ingest-routing owner.
///
/// Exactly one `Gate` exists per serving process. Ingest and query admission
/// observe the same `closed` flag, so [`Gate::close`] fences every public
/// protocol at once when lifecycle shutdown begins. The Scribe dependency is
/// optional because a process may serve a Bifrost target that does not own the
/// Scribe role; ingest against such a process refuses with
/// [`IngestError::IngressClosed`] before any resource is reserved.
#[derive(Clone)]
pub struct Gate<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> {
    /// The exact production token verifier shared with every Bifrost transport.
    verifier: Arc<TokenVerifier<R, I>>,
    /// Immutable transport and typed-ingress limits fixed at boot.
    limits: IngestLimits,
    /// Shared bounded encoded-transport capacity from the process resource graph.
    transport: BifrostTransportAdmission,
    /// Selected Scribe ingress, present only when this process owns the role.
    scribe: Option<Arc<dyn Scribe>>,
    /// Shared admission closure observed by ingest and query alike.
    closed: Arc<AtomicBool>,
}

impl<R: PermissionResolver + 'static, I: IssuerConfigResolver + 'static> Gate<R, I> {
    /// Creates the one Gate from boot-validated dependencies.
    ///
    /// The returned Gate has no Scribe. A process that owns the Scribe role
    /// attaches it with [`Gate::with_scribe`] before publication.
    #[must_use]
    pub fn new(
        verifier: Arc<TokenVerifier<R, I>>,
        limits: IngestLimits,
        transport: BifrostTransportAdmission,
    ) -> Self {
        Self {
            verifier,
            limits,
            transport,
            scribe: None,
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Attaches the selected Scribe ingress this process owns.
    #[must_use]
    pub fn with_scribe(mut self, scribe: Arc<dyn Scribe>) -> Self {
        self.scribe = Some(scribe);
        self
    }

    /// Borrows the shared bounded encoded-transport admission owner.
    #[must_use]
    pub fn transport_admission(&self) -> BifrostTransportAdmission {
        self.transport.clone()
    }

    /// Returns the immutable OTLP wire limits shared with the decode adapters.
    #[must_use]
    pub const fn otlp_wire_limits(&self) -> OtlpWireLimits {
        self.limits.otlp
    }

    /// Returns the tonic frame ceiling derived from the same boot snapshot.
    #[must_use]
    pub const fn otlp_decoding_message_size(&self) -> usize {
        self.limits.max_decoding_message_size
    }

    /// Borrows the immutable typed-ingest limits.
    #[must_use]
    pub const fn ingest_limits(&self) -> &IngestLimits {
        &self.limits
    }

    /// Closes every admission represented by this Gate.
    ///
    /// Idempotent: only the first transition logs, so repeated lifecycle
    /// shutdown calls cannot inflate the closure record.
    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            metrics::counter!("bifrost_gate_events_total", "stage" => "close").increment(1);
            tracing::info!("bifrost gate closed");
        }
    }

    /// Reports whether lifecycle shutdown has closed new Gate work.
    ///
    /// This test-tier probe observes the same atomic checked by request
    /// admission without exposing a second mutable shutdown path.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn is_closed_for_test(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    /// Rejects ingest once the one Gate begins shutdown or Scribe is unusable.
    ///
    /// Two independent conditions close public ingest: lifecycle shutdown, and
    /// a Scribe that is absent, still recovering, or already closed. Both fail
    /// closed with the same stable refusal so a caller cannot distinguish a
    /// draining process from one that never owned the role, and so no ingress
    /// accounting is spent before durable acceptance is possible.
    ///
    /// # Errors
    /// Returns [`IngestError::IngressClosed`] after shutdown begins or while
    /// the selected Scribe is not ready.
    pub fn ensure_ingest_open(&self) -> Result<(), IngestError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(IngestError::IngressClosed);
        }
        if !self.scribe.as_ref().is_some_and(|scribe| scribe.is_ready()) {
            return Err(IngestError::IngressClosed);
        }
        Ok(())
    }

    /// Rejects query work once the one Gate begins shutdown.
    ///
    /// # Errors
    /// Returns [`BifrostError::OracleRoleUnavailable`] after shutdown begins.
    pub fn ensure_query_open(&self) -> Result<(), BifrostError> {
        if self.closed.load(Ordering::Acquire) {
            Err(BifrostError::OracleRoleUnavailable)
        } else {
            Ok(())
        }
    }

    /// Authenticates one private peer request through the shared token verifier.
    ///
    /// Peer forwarding is an internal transport that must remain verifiable
    /// while public admission is closed, so this deliberately skips the
    /// admission check that [`Gate::authenticate_ingest`] applies.
    ///
    /// # Errors
    /// Returns the stable authentication refusal without consulting a role
    /// provider.
    pub async fn authenticate_peer(
        &self,
        metadata: &MetadataMap,
    ) -> Result<AuthContext, IngestError> {
        auth::authenticate(self.verifier.as_ref(), metadata).await
    }

    /// Authenticates one public ingest request after checking Gate admission.
    ///
    /// Admission is checked first so a closing process refuses without paying
    /// for signature verification or an issuer lookup.
    ///
    /// # Errors
    /// Returns the stable authentication or closed-admission refusal.
    pub async fn authenticate_ingest(
        &self,
        metadata: &MetadataMap,
    ) -> Result<AuthContext, IngestError> {
        self.ensure_ingest_open()?;
        self.authenticate_peer(metadata).await
    }

    /// Requests an exact root-backed decode child from Scribe for the adapter.
    ///
    /// # Errors
    /// Returns a stable ingress refusal when admission is closed, when Scribe is
    /// absent, or when Scribe resource accounting rejects the preflighted
    /// typed-request capacity.
    pub fn reserve_otlp_decode(&self, bytes: usize) -> Result<OtlpDecodeOwner, IngestError> {
        self.ensure_ingest_open()?;
        self.scribe()?
            .reserve_otlp_decode(bytes)
            .map_err(IngestError::from_scribe)
    }

    /// Borrows the selected Scribe ingress or refuses as a closed role.
    ///
    /// # Errors
    /// Returns [`IngestError::IngressClosed`] when this process does not own the
    /// Scribe role.
    fn scribe(&self) -> Result<&Arc<dyn Scribe>, IngestError> {
        self.scribe.as_ref().ok_or(IngestError::IngressClosed)
    }

    /// Routes one authenticated trace export into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_spans(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportTraceServiceRequest>,
    ) -> Result<IngestOutcome, IngestError> {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Traces, "spans"),
                decoded.wire_bytes,
                IngressPayload::OtlpTraces(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Traces(outcome) => Ok(outcome),
            _ => Err(IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated metrics export into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_metrics(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportMetricsServiceRequest>,
    ) -> Result<MetricsOutcome, IngestError> {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Metrics, "points"),
                decoded.wire_bytes,
                IngressPayload::OtlpMetrics(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Metrics(outcome) => Ok(outcome),
            _ => Err(IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated log export into the selected Scribe.
    ///
    /// # Errors
    /// Returns the stable Gate, authorization, or Scribe error.
    pub async fn ingest_decoded_resource_logs(
        &self,
        auth: &AuthContext,
        decoded: DecodedOtlp<ExportLogsServiceRequest>,
    ) -> Result<LogsOutcome, IngestError> {
        match self
            .dispatch_otlp(
                auth,
                TableRef::new(BifrostNamespace::Logs, "records"),
                decoded.wire_bytes,
                IngressPayload::OtlpLogs(decoded),
            )
            .await?
        {
            ScribeOtlpOutcome::Logs(outcome) => Ok(outcome),
            _ => Err(IngestError::Internal(
                "Scribe returned the wrong OTLP outcome".to_owned(),
            )),
        }
    }

    /// Routes one authenticated OTLP payload through the exact Scribe frame contract.
    ///
    /// Admission, the frame ceiling, and the record-write permission are all
    /// checked before the payload reaches Scribe, so a refused export never
    /// consumes ingress accounting. The audit event is built here because Gate
    /// is the boundary that knows both the authenticated principal and the
    /// resolved table.
    ///
    /// # Errors
    /// Returns closed admission, payload-limit, RBAC, Scribe, or outcome-shape
    /// errors.
    async fn dispatch_otlp(
        &self,
        auth: &AuthContext,
        table: TableRef,
        measured_wire_bytes: usize,
        payload: IngressPayload,
    ) -> Result<ScribeOtlpOutcome, IngestError> {
        self.ensure_ingest_open()?;
        if measured_wire_bytes > self.limits.max_frame_bytes {
            return Err(IngestError::PayloadTooLarge {
                bytes: u64::try_from(measured_wire_bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(self.limits.max_frame_bytes).unwrap_or(u64::MAX),
            });
        }
        authorize_record_write(auth)?;
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.otlp".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded OTLP frame".to_owned(),
            detail: None,
        };
        self.scribe()?
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                authenticated_tenant: auth.tenant,
                table,
                expected_schema_fingerprint: None,
                request_id: auth.request_id.clone(),
                batch_id: uuid::Uuid::now_v7(),
                audit_event,
                measured_wire_bytes,
                payload,
            })
            .await
            .map_err(IngestError::from_scribe)?
            .otlp_outcome
            .ok_or_else(|| IngestError::Internal("Scribe omitted the OTLP outcome".to_owned()))
    }

    /// Routes one authenticated native Arrow frame into the selected Scribe.
    ///
    /// Once admitted, the durable append is awaited inside the caller's request
    /// future so a disconnected caller can retry the same batch identity against
    /// Scribe deduplication.
    ///
    /// # Errors
    /// Returns the stable Gate validation, authorization, role, or Scribe
    /// refusal.
    pub async fn ingest_native_frame(
        &self,
        auth: &AuthContext,
        frame: InsertBatchRequest,
    ) -> Result<u64, IngestError> {
        self.ensure_ingest_open()?;
        let resolution_started = std::time::Instant::now();
        metrics::counter!("bifrost_gate_events_total", "stage" => "native_frame").increment(1);
        metrics::counter!("bifrost_gate_frame_bytes_total")
            .increment(u64::try_from(frame.arrow_ipc.len()).unwrap_or(u64::MAX));
        validate_batch(&frame, &self.limits)?;
        authorize_record_write(auth)?;
        let (namespace, name) = resolve_fqn(&frame.table)?;
        if namespace == BifrostNamespace::Audit {
            return Err(IngestError::ReservedBuiltinWriteDenied { table: frame.table });
        }
        let batch_id = uuid::Uuid::from_bytes(
            frame
                .wyrd_batch_id
                .as_ref()
                .try_into()
                .map_err(|_| IngestError::RequestValidation("invalid batch id".to_owned()))?,
        );
        let table = TableRef::new(namespace, name);
        let audit_event = wyrd_spec::vala::api::AuditEvent {
            request_id: auth.request_id.clone(),
            trace_id: None,
            operation: "bifrost.ingest_batch".to_owned(),
            resource: table.fqn(),
            card_ref: auth.principal.card_ref().cloned(),
            principal_id: auth.principal.id,
            principal_kind: auth.principal.kind.tag(),
            auth_method: wyrd_spec::vala::api::AuthMethod::Jwt,
            permission: "bifrost:record:write".to_owned(),
            decision: wyrd_spec::vala::api::AuditDecision::Allow,
            result: wyrd_spec::vala::api::AuditResult::Success,
            payload_summary: "one bounded native batch".to_owned(),
            detail: None,
        };
        let measured_wire_bytes = frame.arrow_ipc.len();
        // The durable Scribe write stays inside this request future on purpose.
        // Detaching it onto its own task would orphan the admission owner when a
        // transport drops the handler: the spawned task keeps its admission slot
        // and ingress bytes while nothing observes its terminal. Awaiting inline
        // makes the admission guard drop with the cancelled request, which is the
        // same request-scoped ingest lifetime the ported design commits the WAL
        // under.
        let admission = self
            .scribe()?
            .ingest_frame(ScribeIngressFrame {
                principal: auth.principal.clone(),
                authenticated_tenant: auth.tenant,
                table,
                expected_schema_fingerprint: None,
                request_id: auth.request_id.clone(),
                batch_id,
                audit_event,
                measured_wire_bytes,
                payload: IngressPayload::ArrowIpc(frame.arrow_ipc),
            })
            .await
            .map_err(|error| {
                metrics::counter!("bifrost_gate_events_total", "stage" => "scribe_failure")
                    .increment(1);
                metrics::counter!("bifrost_gate_frames_total", "status" => "rejected").increment(1);
                IngestError::from_scribe(error)
            })?;
        metrics::counter!("bifrost_gate_frames_total", "status" => "accepted").increment(1);
        metrics::counter!("bifrost_gate_rows_total", "status" => "accepted")
            .increment(admission.rows_accepted);
        metrics::counter!("bifrost_gate_rows_total", "status" => "rejected").increment(0);
        metrics::histogram!("bifrost_gate_resolution_seconds")
            .record(resolution_started.elapsed().as_secs_f64());
        Ok(admission.rows_accepted)
    }
}

/// Authorizes one authenticated principal for the Bifrost record-write permission.
///
/// # Errors
/// Returns [`IngestError::RbacDenied`] and its card-scope variants when the
/// principal does not hold `bifrost:record:write`.
fn authorize_record_write(auth: &AuthContext) -> Result<(), IngestError> {
    wyrd_runtime::RbacCheck
        .check(
            &auth.principal,
            &wyrd_runtime::Permission::bifrost_record_write(),
        )
        .into_result()
        .map_err(IngestError::from_rbac)
}

/// Resolves one validated public table name into its closed namespace and local name.
///
/// # Errors
/// Returns a stable request-validation error for an unknown namespace.
pub fn resolve_fqn(fqn: &str) -> Result<(BifrostNamespace, String), IngestError> {
    BifrostNamespace::split_fqn(fqn)
        .ok_or_else(|| IngestError::RequestValidation(format!("unrecognized table fqn: {fqn}")))
}

/// Validates one native Arrow ingress envelope before provider or Scribe IO.
///
/// # Errors
/// Returns a stable request-validation or payload-limit error.
pub fn validate_batch(
    frame: &InsertBatchRequest,
    limits: &IngestLimits,
) -> Result<(), IngestError> {
    if frame.table.is_empty() || frame.wyrd_batch_id.len() != 16 {
        return Err(IngestError::RequestValidation(
            "table and exactly 16-byte wyrd_batch_id are required on every frame".to_owned(),
        ));
    }
    let batch_id = uuid::Uuid::from_bytes(
        frame
            .wyrd_batch_id
            .as_ref()
            .try_into()
            .map_err(|_| IngestError::RequestValidation("invalid batch id".to_owned()))?,
    );
    if batch_id.get_version() != Some(uuid::Version::SortRand) {
        return Err(IngestError::RequestValidation(
            "wyrd_batch_id must be UUIDv7".to_owned(),
        ));
    }
    if frame.arrow_ipc.len() > limits.max_frame_bytes {
        return Err(IngestError::PayloadTooLarge {
            bytes: frame.arrow_ipc.len() as u64,
            limit: limits.max_frame_bytes as u64,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::limits::IngestLimits;
    use super::{AuthContext, Gate, IngestError};
    use crate::contracts::{DecodedOtlp, IngressPayload, ScribeOtlpOutcome};
    use async_trait::async_trait;
    use wyrd_auth_oidc::IssuerConfigResolver;
    use wyrd_auth_verify::PermissionResolver;
    use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::ids::DataTenantId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

    #[derive(Debug)]
    struct TestPermissionResolver;

    impl PermissionResolver for TestPermissionResolver {
        async fn resolve(
            &self,
            _tenant_id: &DataTenantId,
            _roles: &[wyrd_runtime::RoleRef],
        ) -> Result<wyrd_runtime::PermissionSet, wyrd_auth_verify::ResolveError> {
            Ok(wyrd_runtime::PermissionSet::new())
        }
    }

    #[derive(Debug)]
    struct TestIssuerResolver;

    impl IssuerConfigResolver for TestIssuerResolver {
        async fn trusted_issuers(
            &self,
            _tenant: &DataTenantId,
        ) -> Result<Vec<wyrd_auth_oidc::TrustedIssuer>, wyrd_auth_oidc::OidcError> {
            Ok(Vec::new())
        }
    }

    struct TestScribe;

    #[async_trait]
    impl crate::contracts::Scribe for TestScribe {
        fn is_ready(&self) -> bool {
            true
        }

        /// Records one owner-backed frame accepted by the Gate test double.
        ///
        /// # Errors
        ///
        /// This test implementation is infallible after its frame assertions;
        /// assertion failures panic rather than returning [`ScribeError`].
        async fn ingest_frame(
            &self,
            frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            Ok(crate::contracts::FrameAdmission {
                batch_id: uuid::Uuid::now_v7(),
                rows_accepted: 0,
                otlp_outcome: match frame.payload {
                    IngressPayload::OtlpTraces(_) => Some(ScribeOtlpOutcome::Traces(
                        crate::otlp_contract::IngestOutcome {
                            accepted_spans: 0,
                            rejected_spans: 0,
                            rejection_message: None,
                        },
                    )),
                    _ => None,
                },
            })
        }
    }

    struct NotReadyScribe;

    #[async_trait]
    impl crate::contracts::Scribe for NotReadyScribe {
        fn is_ready(&self) -> bool {
            false
        }

        async fn ingest_frame(
            &self,
            _frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            panic!("a not-ready Scribe must be rejected by Gate first");
        }
    }

    struct CountingScribe {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl crate::contracts::Scribe for CountingScribe {
        fn is_ready(&self) -> bool {
            true
        }

        /// Records one owner-backed frame accepted by the counting test double.
        ///
        /// # Errors
        ///
        /// This implementation returns no error after the frame invariants
        /// succeed; invariant violations panic in the test that owns it.
        async fn ingest_frame(
            &self,
            frame: crate::contracts::ScribeIngressFrame,
        ) -> Result<crate::contracts::FrameAdmission, crate::contracts::ScribeError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            assert_eq!(frame.authenticated_tenant, frame.principal.tenant_id);
            assert_eq!(frame.table.fqn(), "vala.traces.spans");
            assert!(frame.expected_schema_fingerprint.is_none());
            assert!(matches!(
                frame.payload,
                IngressPayload::OtlpTraces(crate::contracts::DecodedOtlp { owner: Some(_), .. })
            ));
            Ok(crate::contracts::FrameAdmission {
                batch_id: uuid::Uuid::now_v7(),
                rows_accepted: 0,
                otlp_outcome: Some(ScribeOtlpOutcome::Traces(
                    crate::otlp_contract::IngestOutcome {
                        accepted_spans: 0,
                        rejected_spans: 0,
                        rejection_message: None,
                    },
                )),
            })
        }
    }

    fn auth_context(with_permission: bool) -> AuthContext {
        let tenant = DataTenantId::new_v7();
        let permissions = if with_permission {
            PermissionSet::from_iter([Permission::bifrost_record_write()])
        } else {
            PermissionSet::new()
        };
        let principal = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            permissions,
        );
        AuthContext {
            principal,
            tenant,
            request_id: RequestId::now_v7(),
        }
    }

    /// Couples one trace fixture to a real root-backed decode owner.
    fn decoded_trace(request: ExportTraceServiceRequest) -> DecodedOtlp<ExportTraceServiceRequest> {
        DecodedOtlp::new(request, 0, crate::scribe::otlp_decode_owner_for_test(1))
    }

    /// Builds the exact verifier seam Gate consumes, without any server boot.
    fn test_verifier()
    -> Arc<wyrd_auth_verify::TokenVerifier<TestPermissionResolver, TestIssuerResolver>> {
        let mut keys = std::collections::HashMap::new();
        keys.insert(
            wyrd_auth_verify::Kid::new("test").expect("test kid is valid"),
            Arc::new(
                wyrd_auth_verify::public_key_from_pem(
                    b"-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEAWhCX9H41EwSjJJI1E6X3z5fTKyCZ3v2DsJluJ+DZ8Vw=\n-----END PUBLIC KEY-----\n",
                )
                .expect("test key is valid"),
            ),
        );
        let verifier = wyrd_auth_verify::TokenVerifier::new(
            keys,
            "wyrd",
            Arc::new(TestPermissionResolver),
            wyrd_auth_verify::WyrdAuthVerifySettings::default(),
        );
        Arc::new(verifier)
    }

    /// Composes one Gate over an injected Scribe double the same way boot does.
    fn test_gate(
        scribe: Arc<dyn crate::contracts::Scribe>,
    ) -> Gate<TestPermissionResolver, TestIssuerResolver> {
        Gate::new(
            test_verifier(),
            IngestLimits::default(),
            super::BifrostTransportAdmission::default(),
        )
        .with_scribe(scribe)
    }

    /// Gate composes from injected seams alone: verifier, limits, transport, Scribe.
    #[test]
    fn gate_constructs_with_injected_seams_without_server_boot() {
        let gate = test_gate(Arc::new(TestScribe));
        assert!(gate.ensure_ingest_open().is_ok());
    }

    #[tokio::test]
    async fn gate_enforces_bifrost_record_write() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = test_gate(Arc::new(CountingScribe {
            calls: Arc::clone(&scribe_calls),
        }));

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(false),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("permission must be denied");
        assert!(matches!(error, IngestError::RbacDenied { .. }));
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_authenticates_before_reading_frames() {
        let gate = test_gate(Arc::new(TestScribe));
        let metadata = wyrd_tonic::tonic::metadata::MetadataMap::new();
        assert!(gate.authenticate_ingest(&metadata).await.is_err());
    }

    #[tokio::test]
    async fn gate_routes_logical_frame_without_catalog_resolution() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = test_gate(Arc::new(CountingScribe {
            calls: Arc::clone(&scribe_calls),
        }));

        let outcome = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect("Gate must route without consulting its catalog adapter");
        assert_eq!(outcome.accepted_spans, 0);
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn close_rejects_new_work_before_scribe_handoff() {
        let scribe_calls = Arc::new(AtomicUsize::new(0));
        let gate = test_gate(Arc::new(CountingScribe {
            calls: Arc::clone(&scribe_calls),
        }));
        gate.close();

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("closed Gate must reject new work");
        assert!(matches!(error, IngestError::IngressClosed));
        assert_eq!(scribe_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn gate_rejects_ingest_when_scribe_recovery_is_incomplete() {
        let gate = test_gate(Arc::new(NotReadyScribe));

        let error = gate
            .ingest_decoded_resource_spans(
                &auth_context(true),
                decoded_trace(ExportTraceServiceRequest::default()),
            )
            .await
            .expect_err("Gate must fail closed while Scribe recovery is incomplete");
        assert!(matches!(error, IngestError::IngressClosed));
    }
}
