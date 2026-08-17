//! Bounded pre-ACK preparation for Scribe requests.

use super::ScribeImpl;
use crate::contracts::{FrameAdmission, IngressPayload, ScribeError, ScribeIngressFrame};
use crate::scribe::admission::MAX_REQUEST_BYTES;
use crate::scribe::execution_lanes::{ScribePersistenceCpuOp, ScribePersistenceCpuResult};
use crate::scribe::material_plan::{IngestMaterialPlan, ScribeIngressPlanner};
use crate::scribe::memory::MemoryCategory;
use crate::scribe::preprocess::{
    AdmittedAppend, AdmittedRows, NativeAdmittedRows, OtlpAdmittedRows, OtlpTypedRows,
};
use crate::scribe::routing::shard_for;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

/// Validates that one decoded request fits the persistence bucket that must own it.
///
/// The bound covers decoded Arrow ownership plus the measured transport frame.
/// Checked addition makes an unrepresentable request fail as oversized before
/// reservation growth, preprocessing, shard dispatch, or WAL work.
///
/// # Errors
///
/// Returns [`ScribeError::DecodedPayloadTooLarge`] when the combined byte count
/// overflows `usize` or exceeds `limit`.
fn validate_decoded_request_size(
    decoded_arrow_bytes: usize,
    wire_bytes: usize,
    limit: usize,
) -> Result<usize, ScribeError> {
    let bytes =
        decoded_arrow_bytes
            .checked_add(wire_bytes)
            .ok_or(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit,
            })?;
    if bytes > limit {
        return Err(ScribeError::DecodedPayloadTooLarge { bytes, limit });
    }
    Ok(bytes)
}

/// Validates authenticated identity and the raw transport ceiling.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for tenant mismatch or a nil tenant,
/// and [`ScribeError::PayloadTooLarge`] above the fixed request ceiling.
fn validate_logical_transport_frame(frame: &ScribeIngressFrame) -> Result<(), ScribeError> {
    if frame.authenticated_tenant != frame.principal.tenant_id
        || frame.authenticated_tenant.as_uuid().is_nil()
    {
        return Err(ScribeError::InvalidFrame);
    }
    if frame.measured_wire_bytes > MAX_REQUEST_BYTES {
        return Err(ScribeError::PayloadTooLarge {
            bytes: frame.measured_wire_bytes,
        });
    }
    Ok(())
}

/// Takes the adapter-decode owner required by an OTLP transport payload.
///
/// Native IPC and engine-only projected rows do not have an adapter typed-
/// decode allocation, so they deliberately return no owner and acquire their
/// one root through the normal Scribe admission branch.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] when any typed OTLP payload arrives
/// without the move-only owner created before adapter construction.
fn take_transport_decode_owner(
    payload: &mut IngressPayload,
) -> Result<Option<crate::contracts::OtlpDecodeOwner>, ScribeError> {
    match payload {
        IngressPayload::OtlpTraces(decoded) => decoded
            .owner
            .take()
            .map(Some)
            .ok_or(ScribeError::InvalidFrame),
        IngressPayload::OtlpMetrics(decoded) => decoded
            .owner
            .take()
            .map(Some)
            .ok_or(ScribeError::InvalidFrame),
        IngressPayload::OtlpLogs(decoded) => decoded
            .owner
            .take()
            .map(Some)
            .ok_or(ScribeError::InvalidFrame),
        IngressPayload::ArrowIpc(_) | IngressPayload::ProjectedArrow(_) => Ok(None),
    }
}

/// Move-only context required to form one admitted row source.
struct AdmittedRowContext {
    /// Authenticated principal moved into the retained source.
    principal: wyrd_runtime::Principal,
    /// Catalog fingerprint required of the projected source schema.
    expected_schema_fingerprint: crate::schema::fingerprint::SchemaFingerprint,
    /// Stable request identity moved into managed-column stamping.
    request_id: wyrd_spec::request_id::RequestId,
    /// Stable logical batch identity.
    batch_id: uuid::Uuid,
    /// One authoritative receipt time shared by planning and projection.
    receipt_micros: i64,
    /// Fixed material ceiling admitted for the current slice.
    material_limit: usize,
    /// Native event-time acceptance window.
    event_time_window: crate::scribe::admission::EventTimeWindow,
    /// Shared publication slot read only after the durable ACK.
    otlp_outcome: Arc<OnceLock<crate::contracts::ScribeOtlpOutcome>>,
    /// Native schema frame start established by preflight.
    native_schema_start: usize,
    /// Native schema frame end established by preflight.
    native_schema_end: usize,
    /// Fixed native source descriptors established by preflight.
    native_sources: [crate::scribe::material_plan::SourceMaterialPlan;
        crate::scribe::material_plan::MAX_SOURCE_PLANS],
    /// Live prefix length within `native_sources`.
    native_source_count: usize,
}

/// Root admission state established before any scalable materialization.
struct RootAdmission {
    /// Catalog fingerprint required of the caller-owned source schema.
    expected_schema_fingerprint: crate::schema::fingerprint::SchemaFingerprint,
    /// One authoritative receipt time shared by planning and projection.
    receipt_micros: i64,
    /// Complete immutable source-derived material plan.
    material_plan: IngestMaterialPlan,
    /// Sole root lease retained through detached persistence work.
    memory: crate::resources::ScribeMemoryLease,
    /// Tenant-qualified physical binding constructed after root admission.
    binding: crate::catalog::TenantTableBinding,
    /// Pod-global item reservation transferred to the shard owner.
    reservation: crate::scribe::admission::InflightFrameReservation,
}

impl ScribeImpl {
    /// Resolves one authenticated logical frame under the Scribe catalog owner.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when no logical-ingress catalog exists, built-in
    /// provisioning or fingerprint lookup fails, or tenant/binding validation
    /// rejects the frame.
    async fn resolve_logical_frame(
        &self,
        frame: &ScribeIngressFrame,
    ) -> Result<crate::schema::fingerprint::SchemaFingerprint, ScribeError> {
        let expected = if let Some(expected) = frame.expected_schema_fingerprint {
            expected
        } else {
            let catalog = self.catalog.as_ref().ok_or_else(|| ScribeError::Internal {
                detail: "Scribe logical ingress requires its catalog owner".to_owned(),
            })?;
            if let Some(definition) = crate::tables::builtin_table(
                frame
                    .table
                    .namespace
                    .as_str()
                    .strip_prefix("vala.")
                    .unwrap_or_default(),
                &frame.table.name,
            ) {
                catalog
                    .ensure_builtin(frame.authenticated_tenant, definition)
                    .await
                    .map_err(scribe_catalog_error)?;
            }
            catalog
                .table_schema_fingerprint(&frame.table, frame.authenticated_tenant)
                .await
                .map_err(scribe_catalog_error)?
        };
        Ok(expected)
    }

    /// Computes one immutable material plan before root admission or binding.
    ///
    /// # Errors
    ///
    /// Returns stable invalid, row, or material refusals when the borrowed
    /// native or typed OTLP payload cannot satisfy the closed V1 limits.
    fn plan_transport_payload(
        &self,
        frame: &ScribeIngressFrame,
        receipt_micros: i64,
        physical_binding_peak_bytes: usize,
    ) -> Result<IngestMaterialPlan, ScribeError> {
        let planner = ScribeIngressPlanner::new(self.ingest_limits);
        let plan = match &frame.payload {
            IngressPayload::ArrowIpc(bytes) => {
                planner.plan_native(bytes, physical_binding_peak_bytes)
            }
            IngressPayload::OtlpTraces(request) => planner.plan_traces(
                &request.request,
                request.decode_bytes,
                physical_binding_peak_bytes,
                receipt_micros,
            ),
            IngressPayload::OtlpMetrics(request) => planner.plan_metrics(
                &request.request,
                request.decode_bytes,
                physical_binding_peak_bytes,
                receipt_micros,
            ),
            IngressPayload::OtlpLogs(request) => planner.plan_logs(
                &request.request,
                request.decode_bytes,
                physical_binding_peak_bytes,
                receipt_micros,
            ),
            IngressPayload::ProjectedArrow(batches) => planner.plan_projected(
                batches,
                frame.measured_wire_bytes,
                physical_binding_peak_bytes,
            ),
        }?;
        if matches!(&frame.payload, IngressPayload::ProjectedArrow(_)) {
            validate_decoded_request_size(
                plan.current_material_bytes,
                plan.request_bytes,
                self.decoded_request_limit(),
            )?;
        }
        Ok(plan)
    }

    /// Constructs and tenant-validates the physical binding after root admission.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when logical identity cannot form
    /// the canonical tenant-qualified binding or its tenant tripwire fails.
    fn construct_physical_binding(
        facts: crate::catalog::PhysicalBindingFacts<'_>,
        principal_tenant: wyrd_spec::DataTenantId,
    ) -> Result<crate::catalog::TenantTableBinding, ScribeError> {
        let binding = crate::catalog::TenantTableBinding::from_facts(facts)
            .map_err(|_| ScribeError::InvalidFrame)?;
        binding
            .validate_authenticated_tenant(principal_tenant)
            .map_err(|_| ScribeError::InvalidFrame)?;
        Ok(binding)
    }

    /// Converts one move-only transport payload into its retained row source.
    ///
    /// Typed OTLP remains unprojected, native Arrow retains its preflight
    /// descriptors, and the engine-only projected path uses the bounded ingress
    /// CPU lane.
    ///
    /// # Errors
    ///
    /// Returns the bounded ingress decode or material-ceiling error for the
    /// selected payload.
    async fn prepare_admitted_rows(
        &self,
        payload: IngressPayload,
        context: AdmittedRowContext,
    ) -> Result<AdmittedRows, ScribeError> {
        let AdmittedRowContext {
            principal,
            expected_schema_fingerprint,
            request_id,
            batch_id,
            receipt_micros,
            material_limit,
            event_time_window,
            otlp_outcome,
            native_schema_start,
            native_schema_end,
            native_sources,
            native_source_count,
        } = context;
        match payload {
            IngressPayload::ArrowIpc(bytes) => {
                Ok(AdmittedRows::Native(Box::new(NativeAdmittedRows {
                    bytes,
                    principal,
                    expected_schema_fingerprint,
                    request_id,
                    batch_id,
                    event_time_window,
                    receipt_micros,
                    schema_start: native_schema_start,
                    schema_end: native_schema_end,
                    sources: native_sources,
                    source_count: native_source_count,
                })))
            }
            IngressPayload::OtlpTraces(decoded) => {
                Ok(AdmittedRows::Otlp(Box::new(OtlpAdmittedRows {
                    request: OtlpTypedRows::Traces(decoded.request),
                    principal,
                    expected_schema_fingerprint,
                    request_id,
                    batch_id,
                    receipt_micros,
                    material_limit,
                    outcome: otlp_outcome,
                })))
            }
            IngressPayload::OtlpMetrics(decoded) => {
                Ok(AdmittedRows::Otlp(Box::new(OtlpAdmittedRows {
                    request: OtlpTypedRows::Metrics(decoded.request),
                    principal,
                    expected_schema_fingerprint,
                    request_id,
                    batch_id,
                    receipt_micros,
                    material_limit,
                    outcome: otlp_outcome,
                })))
            }
            IngressPayload::OtlpLogs(decoded) => {
                Ok(AdmittedRows::Otlp(Box::new(OtlpAdmittedRows {
                    request: OtlpTypedRows::Logs(decoded.request),
                    principal,
                    expected_schema_fingerprint,
                    request_id,
                    batch_id,
                    receipt_micros,
                    material_limit,
                    outcome: otlp_outcome,
                })))
            }
            payload @ IngressPayload::ProjectedArrow(_) => {
                let rows = self
                    .ingress_cpu
                    .decode(
                        payload,
                        principal,
                        expected_schema_fingerprint,
                        request_id,
                        batch_id,
                        event_time_window,
                    )
                    .await?;
                let decoded_request_bytes = rows.get_array_memory_size();
                if decoded_request_bytes > material_limit {
                    return Err(ScribeError::DecodedPayloadTooLarge {
                        bytes: decoded_request_bytes,
                        limit: material_limit,
                    });
                }
                Ok(AdmittedRows::Projected(rows))
            }
        }
    }

    /// Validates, plans, and reserves one complete logical root.
    ///
    /// A transport-decode owner is adopted when present; otherwise Scribe
    /// obtains the root directly. Physical binding and shard attachment happen
    /// only after the complete source-derived plan has been admitted.
    ///
    /// # Errors
    ///
    /// Returns the stable transport, catalog, material, memory, admission, or
    /// shard-attachment refusal before row materialization begins.
    async fn admit_transport_frame(
        &self,
        frame: &mut ScribeIngressFrame,
        lifecycle: &mut crate::scribe::telemetry::ScribeIngressLifecycleOwner,
    ) -> Result<RootAdmission, ScribeError> {
        validate_logical_transport_frame(frame)?;
        let decode_owner = take_transport_decode_owner(&mut frame.payload)?;
        let expected_schema_fingerprint = self.resolve_logical_frame(frame).await?;
        let receipt_micros = crate::scribe::execution_lanes::current_receipt_micros()?;
        let binding_facts =
            crate::catalog::TenantTableBinding::facts(&frame.authenticated_tenant, &frame.table)
                .map_err(|_| ScribeError::InvalidFrame)?;
        let material_plan =
            self.plan_transport_payload(frame, receipt_micros, binding_facts.peak_bytes)?;
        lifecycle.planned(&material_plan);
        let mut memory = match decode_owner {
            Some(owner) => owner.complete(material_plan.root_bytes)?,
            None => match self
                .memory
                .try_reserve_ingress(MemoryCategory::Raw, material_plan.root_bytes)
            {
                Ok(reservation) => reservation,
                Err(_) => self.reserve_ingress_after_pressure_seal(
                    MemoryCategory::Raw,
                    material_plan.root_bytes,
                    &frame.table.name,
                )?,
            },
        };
        lifecycle.reserved(material_plan.root_bytes);
        let binding = Self::construct_physical_binding(binding_facts, frame.principal.tenant_id)?;
        let table = binding.table_ref.fqn();
        let shard = shard_for(
            frame.principal.tenant_id,
            &binding.table_ref,
            frame.batch_id,
        );
        let reservation = self
            .admission
            .try_reserve(table, material_plan.root_bytes)?;
        memory.attach_shard(shard)?;
        #[cfg(any(test, feature = "test-support"))]
        self.pause_admitted_ingest_for_test().await;
        Ok(RootAdmission {
            expected_schema_fingerprint,
            receipt_micros,
            material_plan,
            memory,
            binding,
            reservation,
        })
    }

    /// Prepares one request and dispatches its owned packet to its fixed shard.
    ///
    /// The global item reservation is acquired before decoding and remains
    /// attached to the `AdmittedAppend`. The ingress CPU lane bounds decoding,
    /// while the bounded shard mailbox controls retained work.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when tenant validation, request bounds, memory or
    /// admission reservation, decoding, preparation, dispatch, or durable ACK
    /// completion fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before dispatch releases local reservations. Cancellation
    /// after dispatch may leave the accepted append in the shard queue; the
    /// shard owns its eventual completion and retry semantics.
    pub(super) async fn prepare_and_dispatch(
        &self,
        mut frame: ScribeIngressFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        let append_started = Instant::now();
        let mut lifecycle = self.ingress_lifecycle.begin();
        let RootAdmission {
            expected_schema_fingerprint,
            receipt_micros,
            material_plan,
            mut memory,
            binding,
            reservation,
        } = match self.admit_transport_frame(&mut frame, &mut lifecycle).await {
            Ok(admission) => admission,
            Err(error) => {
                lifecycle.refuse();
                return Err(error);
            }
        };
        let otlp_outcome = Arc::new(OnceLock::new());
        let tenant = frame.principal.tenant_id;
        let rows = match self
            .prepare_admitted_rows(
                frame.payload,
                AdmittedRowContext {
                    principal: frame.principal,
                    expected_schema_fingerprint,
                    request_id: frame.request_id,
                    batch_id: frame.batch_id,
                    receipt_micros,
                    material_limit: material_plan.current_material_bytes,
                    event_time_window: self.admission.config().event_time_window,
                    otlp_outcome: Arc::clone(&otlp_outcome),
                    native_schema_start: material_plan.native_schema_start,
                    native_schema_end: material_plan.native_schema_end,
                    native_sources: material_plan.sources,
                    native_source_count: material_plan.source_count,
                },
            )
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                lifecycle.refuse();
                return Err(error);
            }
        };
        if let Err(error) = memory.transfer_category(MemoryCategory::Decode) {
            lifecycle.refuse();
            return Err(error);
        }
        if let Err(error) = memory.transfer_category(MemoryCategory::Prepared) {
            lifecycle.refuse();
            return Err(error);
        }

        let (durable_tx, durable_rx) = tokio::sync::oneshot::channel();
        let admitted = AdmittedAppend {
            batch_id: frame.batch_id,
            audit_event: frame.audit_event,
            rows,
            measured_wire_bytes: frame.measured_wire_bytes,
            admitted_bytes: material_plan.root_bytes,
            wal_workspace_bytes: material_plan.wal_workspace_bytes,
            reservation,
            memory,
            tenant,
            table: binding.table_ref,
            queued_at: Instant::now(),
            durable_ack: Some(durable_tx),
            lifecycle,
        };
        let planned_rows_accepted = u64::try_from(material_plan.rows).unwrap_or(u64::MAX);
        let prepared = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::Preprocess(Box::new(admitted)))
            .await?
        {
            ScribePersistenceCpuResult::Prepared(value) => value,
            ScribePersistenceCpuResult::NativeSliceProduced { .. }
            | ScribePersistenceCpuResult::OtlpSliceProduced { .. }
            | ScribePersistenceCpuResult::ParquetEncoded(_)
            | ScribePersistenceCpuResult::ReplayRestored(_) => {
                return Err(ScribeError::Internal {
                    detail: "persistence lane returned the wrong preparation result".to_owned(),
                });
            }
        };
        self.shards
            .try_send(prepared)
            .inspect_err(|_| super::record_scribe_rejection("queue"))?;
        durable_rx.await.map_err(|_| ScribeError::Internal {
            detail: "shard owner dropped durable batch completion".to_owned(),
        })??;
        let published_outcome = otlp_outcome.get().cloned();
        let rows_accepted = published_outcome
            .as_ref()
            .map_or(planned_rows_accepted, |outcome| match outcome {
                crate::contracts::ScribeOtlpOutcome::Traces(value) => {
                    u64::try_from(value.accepted_spans).unwrap_or(u64::MAX)
                }
                crate::contracts::ScribeOtlpOutcome::Metrics(value) => {
                    u64::try_from(value.accepted_points).unwrap_or(u64::MAX)
                }
                crate::contracts::ScribeOtlpOutcome::Logs(value) => {
                    u64::try_from(value.accepted_records).unwrap_or(u64::MAX)
                }
            });
        record_accepted_frame(rows_accepted, append_started.elapsed());
        Ok(FrameAdmission {
            batch_id: frame.batch_id,
            rows_accepted,
            otlp_outcome: published_outcome,
        })
    }

    /// Returns the decoded-request ceiling for the current production or test owner.
    #[must_use]
    fn decoded_request_limit(&self) -> usize {
        #[cfg(any(test, feature = "test-support"))]
        {
            let override_bytes = self
                .decoded_request_limit_for_test
                .load(std::sync::atomic::Ordering::Acquire);
            if override_bytes != 0 {
                return override_bytes;
            }
        }
        self.memory.active_bucket_target_bytes()
    }

    /// Overrides the decoded-request ceiling for one bounded test owner.
    ///
    /// Production construction always leaves the atomic at zero and therefore
    /// uses the active-bucket target. The override affects subsequent requests
    /// only and does not mutate reservations already in flight.
    ///
    /// # Panics
    ///
    /// Panics when `bytes` is zero because zero is reserved to select the
    /// production ceiling.
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_decoded_request_limit_for_test(&self, bytes: usize) {
        assert!(bytes != 0, "decoded request test limit must be nonzero");
        self.decoded_request_limit_for_test
            .store(bytes, std::sync::atomic::Ordering::Release);
    }

    /// Reserve ingress bytes after one coordinated pressure seal and single retry.
    ///
    /// This is the admission-side realization of the D83 flush-first contract:
    /// on an ingress ceiling rejection, request a shard-count-invariant pressure
    /// seal toward the low-water mark ([`ScribeImpl::request_pressure_seal_toward_low_water`]),
    /// then retry the reservation exactly once. The cgroup 90% tripwire is left
    /// as an immediate `IngestBusy` — a container-level limit that a Scribe
    /// pressure seal cannot relieve — and is not retried. A retry that still
    /// fails records the rejection labelled by the ceiling that tripped (D84,
    /// via [`super::record_scribe_ceiling_rejection`] — `cgroup_breaker`,
    /// `ingress_sublimit`, or `bifrost_parent`) and returns `IngestBusy`,
    /// deferring to D71 client backoff for eventual admission. There is no
    /// busy-wait: freezing and persistence proceed asynchronously between the
    /// seal request and retry.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup tripwire is engaged or
    /// when the single post-seal retry still cannot fit under the ingress ceiling.
    fn reserve_ingress_after_pressure_seal(
        &self,
        category: MemoryCategory,
        bytes: usize,
        table: &str,
    ) -> Result<crate::resources::ScribeMemoryLease, ScribeError> {
        if self.memory.cgroup_tripwire_engaged() {
            super::record_scribe_ceiling_rejection(
                crate::scribe::memory::ScribeRejectionCeiling::CgroupBreaker,
            );
            return Err(ScribeError::IngestBusy {
                table: table.to_owned(),
            });
        }
        self.request_pressure_seal_toward_low_water();
        match self.memory.try_reserve_ingress(category, bytes) {
            Ok(reservation) => return Ok(reservation),
            Err(_) => super::record_scribe_ceiling_rejection(
                if self.memory.ingress_sublimit_exceeded(bytes) {
                    crate::scribe::memory::ScribeRejectionCeiling::IngressSublimit
                } else {
                    crate::scribe::memory::ScribeRejectionCeiling::BifrostParent
                },
            ),
        }
        Err(ScribeError::IngestBusy {
            table: table.to_owned(),
        })
    }

    /// Pause one admitted write at the deterministic test-support barrier.
    #[cfg(any(test, feature = "test-support"))]
    async fn pause_admitted_ingest_for_test(&self) {
        if let Some(stall) = self
            .ingest_stall
            .lock()
            .ok()
            .and_then(|mut current| current.take())
        {
            stall.entered.notify_waiters();
            let _completion = super::IngestStallCompletion(&stall);
            stall.release.notified().await;
        }
    }
}

/// Preserves caller-actionable catalog failures across Scribe's private boundary.
fn scribe_catalog_error(error: crate::catalog::BifrostCatalogError) -> ScribeError {
    match error {
        crate::catalog::BifrostCatalogError::TableNotFound(table) => {
            ScribeError::TableNotFound { table }
        }
        crate::catalog::BifrostCatalogError::FingerprintMismatch(table) => {
            ScribeError::FingerprintMismatch { table }
        }
        other => ScribeError::Internal {
            detail: other.to_string(),
        },
    }
}

/// Emits the accepted-frame counters and ACK latency histogram.
///
/// Extracted from `prepare_and_dispatch` to keep that function within the
/// line-length limit; all metrics writes are stateless and have no natural
/// owner struct, so a free function is appropriate here.
fn record_accepted_frame(rows_accepted: u64, elapsed: std::time::Duration) {
    metrics::counter!("bifrost_scribe_frames_total", "status" => "accepted").increment(1);
    metrics::counter!("bifrost_scribe_rows_total", "status" => "accepted").increment(rows_accepted);
    metrics::histogram!("bifrost_scribe_ack_seconds").record(elapsed.as_secs_f64());
}

#[cfg(test)]
mod tests {
    use super::{ScribeImpl, take_transport_decode_owner, validate_decoded_request_size};
    use crate::catalog::TableRef;
    use crate::contracts::{DecodedOtlp, IngressPayload, ScribeAppend, ScribeError};
    use crate::namespaces::BifrostNamespace;
    use crate::schema::SchemaFingerprint;
    use crate::scribe::memory::MemoryCategory;
    use arrow::array::{StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;
    use wyrd_tonic::otlp::logs_service::ExportLogsServiceRequest;
    use wyrd_tonic::otlp::metrics_service::ExportMetricsServiceRequest;
    use wyrd_tonic::otlp::trace_service::ExportTraceServiceRequest;

    /// Every typed OTLP payload fails closed without its adapter decode owner.
    #[test]
    fn otlp_transport_requires_one_decode_owner() {
        let mut payloads = [
            IngressPayload::OtlpTraces(DecodedOtlp {
                request: Box::new(ExportTraceServiceRequest::default()),
                wire_bytes: 0,
                decode_bytes: 0,
                owner: None,
            }),
            IngressPayload::OtlpMetrics(DecodedOtlp {
                request: Box::new(ExportMetricsServiceRequest::default()),
                wire_bytes: 0,
                decode_bytes: 0,
                owner: None,
            }),
            IngressPayload::OtlpLogs(DecodedOtlp {
                request: Box::new(ExportLogsServiceRequest::default()),
                wire_bytes: 0,
                decode_bytes: 0,
                owner: None,
            }),
        ];
        for payload in &mut payloads {
            assert!(matches!(
                take_transport_decode_owner(payload),
                Err(ScribeError::InvalidFrame)
            ));
        }

        let mut native = IngressPayload::ArrowIpc(bytes::Bytes::new());
        assert!(
            take_transport_decode_owner(&mut native)
                .expect("native owner branch")
                .is_none()
        );
        let mut projected = IngressPayload::ProjectedArrow(Vec::new());
        assert!(
            take_transport_decode_owner(&mut projected)
                .expect("projected owner branch")
                .is_none()
        );
    }

    /// The admission memory path rejects with `IngestBusy` **only** when the
    /// coordinated pressure seal cannot free ingress capacity (the D83
    /// reject-only-when-backlogged contract), and admits as soon as capacity is
    /// available.
    ///
    /// A freshly constructed Scribe holds no writable buckets, so a pressure
    /// seal selects no victims and cannot drain a pinned ceiling: the single
    /// post-seal retry must still reject. Once the pin is released (capacity
    /// available, as it would be after an asynchronous seal completed), the
    /// same admission path admits instead of rejecting — proving the rejection
    /// is conditional on genuine backlog rather than unconditional.
    #[tokio::test]
    async fn admission_rejects_only_when_seal_cannot_free() {
        let scribe = ScribeImpl::new();
        let ceiling = scribe.memory.ingress_limit_bytes();

        // Pin ingress at its ceiling: no bucket exists to seal, so the retry
        // after the pressure-seal request cannot fit and must reject.
        let pinned = scribe
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, ceiling)
            .expect("pin ingress at the ceiling");
        assert!(
            matches!(
                scribe.reserve_ingress_after_pressure_seal(MemoryCategory::Raw, 1, "table"),
                Err(ScribeError::IngestBusy { .. })
            ),
            "a backlogged ceiling must reject after the seal request"
        );

        // Release the pin so capacity is available; the same path now admits.
        drop(pinned);
        let admitted = scribe
            .reserve_ingress_after_pressure_seal(MemoryCategory::Raw, ceiling / 2, "table")
            .expect("admits once ingress capacity is available");
        drop(admitted);

        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }

    /// An oversized decoded request is refused at the pre-WAL size gate while
    /// the exact ceiling remains available to a subsequent bounded request.
    #[tokio::test]
    async fn oversized_decoded_request_is_rejected_before_wal_append() {
        let limit = 64 * 1024;
        assert!(matches!(
            validate_decoded_request_size(limit, 1, limit),
            Err(ScribeError::DecodedPayloadTooLarge {
                bytes,
                limit: actual_limit,
            }) if bytes == limit + 1 && actual_limit == limit
        ));
        assert_eq!(
            validate_decoded_request_size(48 * 1024, 16 * 1024, limit)
                .expect("request at the exact decoded ceiling is valid"),
            limit
        );
        assert!(matches!(
            validate_decoded_request_size(usize::MAX, 1, limit),
            Err(ScribeError::DecodedPayloadTooLarge {
                bytes: usize::MAX,
                limit: actual_limit,
            }) if actual_limit == limit
        ));
        let scribe = ScribeImpl::new();
        scribe.set_decoded_request_limit_for_test(limit);
        let tenant = DataTenantId::new(uuid::Uuid::now_v7()).expect("random tenant is valid");
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
        };
        let make_append = |value: String| {
            let schema = Arc::new(Schema::new(vec![
                Field::new(
                    "wyrd_event_time",
                    DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                    false,
                ),
                Field::new("value", DataType::Utf8, false),
            ]));
            let rows = RecordBatch::try_new(
                Arc::clone(&schema),
                vec![
                    Arc::new(
                        TimestampMicrosecondArray::from(vec![
                            chrono::Utc::now().timestamp_micros(),
                        ])
                        .with_timezone("UTC"),
                    ),
                    Arc::new(StringArray::from(vec![value])),
                ],
            )
            .expect("decoded-size fixture batch");
            ScribeAppend {
                principal: principal.clone(),
                table: TableRef::new(BifrostNamespace::Bifrost, "decoded_size_bound"),
                schema_fingerprint: SchemaFingerprint::from_arrow_schema(schema.as_ref()),
                request_id: RequestId::now_v7(),
                batch_id: uuid::Uuid::now_v7(),
                measured_wire_bytes: 0,
                rows,
            }
        };
        let admission_before = scribe.admission_snapshot();
        let memory_before = scribe.memory_snapshot();
        let wal_before = scribe.wal_bytes_on_disk();
        let stats_before = scribe
            .memtable_stats()
            .expect("pre-rejection memtable stats");
        let error = scribe
            .append_durable(make_append("x".repeat(limit)))
            .await
            .expect_err("decoded request above 64 KiB must fail");
        assert!(matches!(
            error,
            ScribeError::DecodedPayloadTooLarge {
                bytes,
                limit: actual_limit,
            } if bytes > limit && actual_limit == limit
        ));
        assert_eq!(scribe.admission_snapshot(), admission_before);
        assert_eq!(scribe.memory_snapshot(), memory_before);
        assert_eq!(scribe.wal_bytes_on_disk(), wal_before);
        assert_eq!(
            scribe
                .memtable_stats()
                .expect("post-rejection memtable stats"),
            stats_before
        );
        scribe
            .append_durable(make_append("small".to_owned()))
            .await
            .expect("sub-limit request remains durably admissible");
        assert!(scribe.wal_bytes_on_disk() > wal_before);
        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }
}
