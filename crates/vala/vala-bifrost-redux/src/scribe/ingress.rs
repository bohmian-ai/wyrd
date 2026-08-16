//! Bounded pre-ACK preparation for Scribe requests.

use super::ScribeImpl;
use crate::contracts::{
    FrameAdmission, IngressPayload, ScribeError, ScribeIngressFrame, ScribeOtlpOutcome,
};
use crate::scribe::admission::{MAX_REQUEST_BYTES, REQUEST_OVERHEAD_BYTES};
use crate::scribe::execution_lanes::{ScribePersistenceCpuOp, ScribePersistenceCpuResult};
use crate::scribe::memory::MemoryCategory;
use crate::scribe::preprocess::AdmittedAppend;
use crate::scribe::routing::shard_for;
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

/// Result of projecting or forwarding one bounded transport payload.
enum PreparedTransportPayload {
    /// Empty OTLP export acknowledged without entering durable row admission.
    Empty(FrameAdmission),
    /// Rows ready for the existing decoded-payload admission path.
    Rows {
        /// Native or projected payload consumed by the ingress CPU decoder.
        payload: IngressPayload,
        /// Optional OTLP outcome returned after durable row acknowledgment.
        otlp_outcome: Option<ScribeOtlpOutcome>,
    },
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
    ) -> Result<
        (
            crate::schema::fingerprint::SchemaFingerprint,
            crate::catalog::TenantTableBinding,
        ),
        ScribeError,
    > {
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
                    .map_err(|error| ScribeError::Internal {
                        detail: error.to_string(),
                    })?;
            }
            catalog
                .table_schema_fingerprint(&frame.table, frame.authenticated_tenant)
                .await
                .map_err(|error| ScribeError::Internal {
                    detail: error.to_string(),
                })?
        };
        let binding = crate::catalog::TenantTableBinding::resolve((
            frame.authenticated_tenant,
            frame.table.clone(),
        ))
        .map_err(|_| ScribeError::InvalidFrame)?;
        binding
            .validate_authenticated_tenant(frame.principal.tenant_id)
            .map_err(|_| ScribeError::InvalidFrame)?;
        Ok((expected, binding))
    }

    /// Projects raw OTLP on Scribe's bounded CPU lane or forwards engine payloads.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::InvalidFrame`] when OTLP projection rejects the
    /// bounded payload, or the CPU lane fails to complete the projection.
    async fn project_transport_payload(
        &self,
        payload: IngressPayload,
        batch_id: uuid::Uuid,
    ) -> Result<PreparedTransportPayload, ScribeError> {
        let (batch, outcome) = match payload {
            IngressPayload::OtlpTraces(request) => {
                let projected = self
                    .ingress_cpu
                    .run(move || {
                        crate::gate::collector::project_resource_spans(&request)
                            .map_err(|_| ScribeError::InvalidFrame)
                    })
                    .await?;
                (
                    projected.batch,
                    ScribeOtlpOutcome::Traces(projected.outcome),
                )
            }
            IngressPayload::OtlpMetrics(request) => {
                let projected = self
                    .ingress_cpu
                    .run(move || {
                        crate::gate::collector::project_resource_metrics(&request)
                            .map_err(|_| ScribeError::InvalidFrame)
                    })
                    .await?;
                (
                    projected.batch,
                    ScribeOtlpOutcome::Metrics(projected.outcome),
                )
            }
            IngressPayload::OtlpLogs(request) => {
                let projected = self
                    .ingress_cpu
                    .run(move || {
                        crate::gate::collector::project_resource_logs(&request)
                            .map_err(|_| ScribeError::InvalidFrame)
                    })
                    .await?;
                (projected.batch, ScribeOtlpOutcome::Logs(projected.outcome))
            }
            payload @ (IngressPayload::ArrowIpc(_) | IngressPayload::ProjectedArrow(_)) => {
                return Ok(PreparedTransportPayload::Rows {
                    payload,
                    otlp_outcome: None,
                });
            }
        };
        Ok(match batch {
            Some(batch) => PreparedTransportPayload::Rows {
                payload: IngressPayload::ProjectedArrow(vec![batch]),
                otlp_outcome: Some(outcome),
            },
            None => PreparedTransportPayload::Empty(FrameAdmission {
                batch_id,
                rows_accepted: 0,
                otlp_outcome: Some(outcome),
            }),
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
        frame: ScribeIngressFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        let append_started = Instant::now();
        validate_logical_transport_frame(&frame)?;
        let (expected_schema_fingerprint, binding) = self.resolve_logical_frame(&frame).await?;
        let (payload, otlp_outcome) = match self
            .project_transport_payload(frame.payload, frame.batch_id)
            .await?
        {
            PreparedTransportPayload::Empty(admission) => return Ok(admission),
            PreparedTransportPayload::Rows {
                payload,
                otlp_outcome,
            } => (payload, otlp_outcome),
        };
        let table = binding.table_ref.fqn();
        let shard = shard_for(
            frame.principal.tenant_id,
            &binding.table_ref,
            frame.batch_id,
        );

        let initial_bytes = frame
            .measured_wire_bytes
            .saturating_add(REQUEST_OVERHEAD_BYTES);
        let mut reservation = self.admission.try_reserve(table.clone(), initial_bytes)?;
        let mut memory = match self
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, initial_bytes)
        {
            Ok(reservation) => reservation,
            Err(_) => self.reserve_ingress_after_pressure_seal(
                MemoryCategory::Raw,
                initial_bytes,
                &table,
            )?,
        };
        memory.attach_shard(shard)?;
        #[cfg(any(test, feature = "test-support"))]
        self.pause_admitted_ingest_for_test().await;

        let rows = self
            .ingress_cpu
            .decode(
                payload,
                frame.principal.clone(),
                expected_schema_fingerprint,
                frame.request_id.clone(),
                frame.batch_id,
                self.admission.config().event_time_window,
            )
            .await?;
        let decoded_request_bytes = validate_decoded_request_size(
            rows.get_array_memory_size(),
            frame.measured_wire_bytes,
            self.decoded_request_limit(),
        )?;
        memory.transfer_category(MemoryCategory::Decode)?;
        let estimated_bytes = decoded_request_bytes.saturating_add(REQUEST_OVERHEAD_BYTES);
        reservation.resize(estimated_bytes)?;
        self.resize_ingress_after_pressure_seal(&mut memory, estimated_bytes, &table)?;
        memory.transfer_category(MemoryCategory::Prepared)?;

        let (durable_tx, durable_rx) = tokio::sync::oneshot::channel();
        let admitted = AdmittedAppend {
            batch_id: frame.batch_id,
            audit_event: frame.audit_event,
            rows,
            measured_wire_bytes: frame.measured_wire_bytes,
            admitted_bytes: reservation.bytes(),
            reservation,
            memory,
            tenant: frame.principal.tenant_id,
            table: binding.table_ref,
            queued_at: Instant::now(),
            durable_ack: Some(durable_tx),
        };
        let rows_accepted = u64::try_from(admitted.rows.num_rows()).unwrap_or(u64::MAX);
        let prepared = match self
            .persistence_cpu
            .submit(ScribePersistenceCpuOp::Preprocess(Box::new(admitted)))
            .await?
        {
            ScribePersistenceCpuResult::Prepared(value) => value,
            ScribePersistenceCpuResult::ParquetEncoded(_)
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
        record_accepted_frame(rows_accepted, append_started.elapsed());
        Ok(FrameAdmission {
            batch_id: frame.batch_id,
            rows_accepted,
            otlp_outcome,
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

    /// Grow an existing ingress reservation after one pressure seal and retry.
    ///
    /// The decode step raises the required ingress bytes from the pre-decode
    /// estimate to the measured Arrow size, so the reservation must grow. This
    /// mirrors [`Self::reserve_ingress_after_pressure_seal`] for the in-place
    /// resize path: on an ingress ceiling rejection it requests a
    /// shard-count-invariant pressure seal toward the low-water mark and retries
    /// the resize exactly once, while the cgroup 90% tripwire stays an immediate
    /// `IngestBusy` that a Scribe seal cannot relieve. A retry that still fails
    /// records the rejection labelled by the ceiling that tripped (D84, via
    /// [`super::record_scribe_ceiling_rejection`]) and returns `IngestBusy`,
    /// deferring to D71 client backoff.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup tripwire is engaged or
    /// when the single post-seal resize retry still exceeds the ingress ceiling.
    fn resize_ingress_after_pressure_seal(
        &self,
        memory: &mut crate::resources::ScribeMemoryLease,
        estimated_bytes: usize,
        table: &str,
    ) -> Result<(), ScribeError> {
        if memory.resize_ingress(estimated_bytes).is_ok() {
            return Ok(());
        }
        if self.memory.cgroup_tripwire_engaged() {
            super::record_scribe_ceiling_rejection(
                crate::scribe::memory::ScribeRejectionCeiling::CgroupBreaker,
            );
            return Err(ScribeError::IngestBusy {
                table: table.to_owned(),
            });
        }
        self.request_pressure_seal_toward_low_water();
        match memory.resize_ingress(estimated_bytes) {
            Ok(()) => return Ok(()),
            Err(_) => super::record_scribe_ceiling_rejection(
                if self.memory.ingress_sublimit_exceeded(estimated_bytes) {
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
    use super::{ScribeImpl, validate_decoded_request_size};
    use crate::catalog::TableRef;
    use crate::contracts::{ScribeAppend, ScribeError};
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
