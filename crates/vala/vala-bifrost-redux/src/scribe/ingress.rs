//! Bounded pre-ACK preparation for Scribe requests.

use super::ScribeImpl;
use crate::contracts::{FrameAdmission, IngressPayload, ScribeError, ScribeIngressFrame};
use crate::scribe::material_plan::{MaterialPlan, ScribeIngressPlanner};
use crate::scribe::memory::MemoryCategory;
use crate::scribe::preprocess::{
    AdmittedAppend, AdmittedRows, NativeAdmittedRows, PreparedAppend, prepare_append,
};
use crate::tables::AuditLogTable;

use std::sync::Arc;
use std::time::Instant;

use iceberg::spec::Schema;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;

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
/// The system owner is admitted only for the server's internal audit
/// publication: the platform audit principal writing the retained audit log.
///
/// # Errors
///
/// Returns [`ScribeError::InvalidFrame`] for tenant mismatch or any other
/// system-owner frame, and [`ScribeError::PayloadTooLarge`] above the fixed
/// request ceiling.
fn validate_logical_transport_frame(
    frame: &ScribeIngressFrame,
    request_limit_bytes: usize,
) -> Result<(), ScribeError> {
    let system_audit_publication = frame.principal.id == PLATFORM_AUDIT_PRINCIPAL
        && AuditLogTable::admits_system_owner(&frame.table);
    if frame.authenticated_tenant != frame.principal.tenant_id
        || (frame.authenticated_tenant == wyrd_spec::DataTenantId::SYSTEM_OWNER
            && !system_audit_publication)
    {
        return Err(ScribeError::InvalidFrame);
    }
    if frame.measured_wire_bytes > request_limit_bytes {
        return Err(ScribeError::PayloadTooLarge {
            bytes: frame.measured_wire_bytes,
            limit: request_limit_bytes,
        });
    }
    Ok(())
}

/// Takes the transport-decode owner a canonical payload may already hold.
///
/// Gate materializes canonical batches inside the capacity it reserved before
/// decoding, so that child moves here and is grown into Scribe's one complete
/// root. Native IPC, and canonical batches from engine-internal producers, have
/// no such reservation and acquire their root through ordinary admission.
fn take_transport_decode_owner(
    payload: &mut IngressPayload,
) -> Option<crate::contracts::OtlpDecodeOwner> {
    match payload {
        IngressPayload::Canonical(canonical) => canonical.owner.take(),
        IngressPayload::ArrowIpc(_) => None,
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
    /// Server-owned built-in whose declared physical contract the decode must
    /// preserve, including its correlation policy.
    ///
    /// `None` for a dynamic table, which keeps the default envelope.
    definition: Option<&'static crate::tables::BuiltinTableDefinition>,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    registered_schema: Option<Arc<Schema>>,
}

/// The registered contract Scribe resolves for one logical frame before it
/// plans any material.
///
/// Both halves come from the same control row: the schema the caller must
/// match, and the canonical layout that fixes how the rows are partitioned.
struct LogicalFrameContract {
    /// Catalog fingerprint required of the caller-owned source schema.
    expected_schema_fingerprint: crate::schema::fingerprint::SchemaFingerprint,
    /// Registered partition granularity every slice of this frame is bucketed to.
    partition_granularity: crate::catalog::TimeGranularity,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    registered_schema: Option<Arc<Schema>>,
}

/// Root admission state established before any scalable materialization.
struct RootAdmission {
    /// Catalog fingerprint required of the caller-owned source schema.
    expected_schema_fingerprint: crate::schema::fingerprint::SchemaFingerprint,
    /// Registered partition granularity applied to every prepared slice.
    partition_granularity: crate::catalog::TimeGranularity,
    /// Registered Iceberg schema whose field ids every stamped batch carries.
    ///
    /// `None` only for the embedded engine seam, which has no catalog owner
    /// and writes objects that no registered table promotes.
    registered_schema: Option<Arc<Schema>>,
    /// One authoritative receipt time shared by planning and projection.
    receipt_micros: i64,
    /// Complete immutable source-derived material plan.
    material_plan: MaterialPlan,
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
    /// The catalog owner is authoritative for both halves of the contract: a
    /// caller-supplied fingerprint is honored as an override, but the partition
    /// granularity always comes from the registered canonical layout so no
    /// ingest path can invent one. Only the embedded engine seam, which has no
    /// catalog owner, falls back to the default hourly granularity, and it must
    /// supply its own fingerprint.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError`] when no logical-ingress catalog exists and the
    /// frame carries no fingerprint, when built-in provisioning or the
    /// registration lookup fails, or when the registered layout is undecodable.
    async fn resolve_logical_frame(
        &self,
        frame: &ScribeIngressFrame,
    ) -> Result<LogicalFrameContract, ScribeError> {
        let Some(catalog) = self.catalog.as_ref() else {
            let expected_schema_fingerprint =
                frame
                    .expected_schema_fingerprint
                    .ok_or_else(|| ScribeError::Internal {
                        detail: "Scribe logical ingress requires its catalog owner".to_owned(),
                    })?;
            return Ok(LogicalFrameContract {
                expected_schema_fingerprint,
                partition_granularity: crate::catalog::TimeGranularity::Hour,
                registered_schema: None,
            });
        };
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
        let (registered_fingerprint, layout) = catalog
            .table_registration(&frame.table, frame.authenticated_tenant)
            .await
            .map_err(scribe_catalog_error)?;
        let registered_schema = catalog
            .registered_schema(&frame.table, frame.authenticated_tenant)
            .await
            .map_err(scribe_catalog_error)?;
        Ok(LogicalFrameContract {
            expected_schema_fingerprint: frame
                .expected_schema_fingerprint
                .unwrap_or(registered_fingerprint),
            partition_granularity: crate::catalog::TimeGranularity::from_wire(
                layout.partition_granularity,
            ),
            registered_schema: Some(registered_schema),
        })
    }

    /// Resolves the registered UID Gate authorizes one record write against.
    ///
    /// The registration lookup runs first so the common case costs one control
    /// row. Only a missing built-in destination is provisioned, through the same
    /// [`crate::catalog::BifrostCatalog::ensure_builtin`] owner ingest uses.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when Scribe has no catalog owner,
    /// [`ScribeError::TableNotFound`] for an unregistered caller table, and the
    /// mapped catalog failure otherwise.
    pub(super) async fn resolve_write_table_uid(
        &self,
        tenant: wyrd_spec::ids::DataTenantId,
        table: &crate::catalog::TableRef,
    ) -> Result<crate::catalog::TableUid, ScribeError> {
        let catalog = self.catalog.as_ref().ok_or_else(|| ScribeError::Internal {
            detail: "Scribe write authorization requires its catalog owner".to_owned(),
        })?;
        match catalog.table_uid(table, tenant).await {
            Err(crate::catalog::BifrostCatalogError::TableNotFound(fqn)) => {
                let definition = crate::tables::builtin_table(
                    table
                        .namespace
                        .as_str()
                        .strip_prefix("vala.")
                        .unwrap_or_default(),
                    &table.name,
                )
                .ok_or(ScribeError::TableNotFound { table: fqn })?;
                catalog
                    .ensure_builtin(tenant, definition)
                    .await
                    .map_err(scribe_catalog_error)
            }
            resolved => resolved.map_err(scribe_catalog_error),
        }
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
    ) -> Result<MaterialPlan, ScribeError> {
        let planner = ScribeIngressPlanner::new(self.ingest_limits);
        let plan = match &frame.payload {
            IngressPayload::ArrowIpc(bytes) => planner.plan_native(bytes)?,
            IngressPayload::Canonical(canonical) => {
                let plan = planner.plan_canonical(&canonical.batches, frame.measured_wire_bytes)?;
                validate_decoded_request_size(
                    plan.current_material_bytes,
                    plan.request_len,
                    self.decoded_request_limit(),
                )?;
                plan
            }
        };
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
    /// Native Arrow retains its preflight descriptors and decodes one source at
    /// a time on the persistence lane. Canonical batches are already
    /// materialized, so they are validated and stamped on the bounded ingress
    /// CPU lane before admission completes.
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
            definition,
            registered_schema,
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
                    definition,
                    registered_schema,
                    expanded_limit_bytes: self.ingest_limits.expanded_bytes(),
                })))
            }
            IngressPayload::Canonical(canonical) => {
                let rows = self
                    .ingress_cpu
                    .decode(
                        IngressPayload::Canonical(canonical),
                        crate::scribe::execution_lanes::IngressDecodeInputs {
                            principal,
                            expected_schema_fingerprint,
                            request_id,
                            receipt_micros,
                            window: event_time_window,
                            definition,
                            registered_schema,
                        },
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

    /// Validates, plans, and charges the bytes one logical frame already holds.
    ///
    /// A transport-decode owner is transferred when present; otherwise Scribe
    /// opens an empty lease on the shared root. The lease is then sized to the
    /// materialized Arrow the payload holds now. Future decode, WAL, and
    /// persistence output is charged only when it is materialized. Physical
    /// binding and shard attachment happen after the charge is admitted.
    ///
    /// # Errors
    ///
    /// Returns the stable transport, catalog, material, memory, admission, or
    /// shard-attachment refusal before row materialization begins.
    async fn admit_transport_frame(
        &self,
        frame: &mut ScribeIngressFrame,
    ) -> Result<RootAdmission, ScribeError> {
        validate_logical_transport_frame(frame, self.ingest_limits.otlp.request_bytes)?;
        let decode_owner = take_transport_decode_owner(&mut frame.payload);
        let LogicalFrameContract {
            expected_schema_fingerprint,
            partition_granularity,
            registered_schema,
        } = self.resolve_logical_frame(frame).await?;
        let receipt_micros = self.admission_instant(frame.principal.tenant_id).await?;
        let binding_facts =
            crate::catalog::TenantTableBinding::facts(&frame.authenticated_tenant, &frame.table)
                .map_err(|_| ScribeError::InvalidFrame)?;
        let material_plan = self.plan_transport_payload(frame)?;
        let held_bytes = material_plan.held_material_bytes;
        let mut memory = match decode_owner {
            Some(owner) => owner.complete(),
            None => self.memory.try_reserve_ingress(MemoryCategory::Raw, 0)?,
        };
        self.resize_after_pressure_seal(&mut memory, held_bytes, &frame.table.name)?;
        let binding = Self::construct_physical_binding(binding_facts, frame.principal.tenant_id)?;
        let table = binding.table_ref.fqn();
        let shard = self.shards.lane_for(
            frame.principal.tenant_id,
            &binding.table_ref,
            frame.batch_id,
        );
        let reservation = self.admission.try_reserve(table)?;
        memory.attach_shard(shard)?;
        #[cfg(any(test, feature = "test-support"))]
        self.pause_admitted_ingest_for_test().await;
        Ok(RootAdmission {
            expected_schema_fingerprint,
            partition_granularity,
            registered_schema,
            receipt_micros,
            material_plan,
            memory,
            binding,
            reservation,
        })
    }

    /// Reads the one ingestion instant a transport frame is admitted under.
    ///
    /// `PostgreSQL` is the clock of record: the instant is read once per batch,
    /// stamped on every row as `wyrd_ingested_at`, used as `wyrd_event_time`
    /// for rows that supply none, anchors the event-time window, and is stored
    /// on the batch-commit fence. An engine without control `PostgreSQL` has no
    /// durable state to agree with and reads the system clock. A test-support
    /// owner adds the offset installed by `shift_receipt_clock_for_test`.
    ///
    /// # Errors
    ///
    /// Returns the `PostgreSQL` acquisition or read error, or an internal error
    /// when the system clock is unreadable.
    async fn admission_instant(&self, tenant: DataTenantId) -> Result<i64, ScribeError> {
        let micros = match &self.control_postgres {
            Some(postgres) => {
                let mut conn = postgres.tenant_conn(tenant).await?;
                let instant =
                    vala_sql::queries::scribe_batch_commits::ingest_instant(&mut conn).await?;
                conn.commit().await?;
                instant.timestamp_micros()
            }
            None => crate::scribe::execution_lanes::current_receipt_micros()?,
        };
        #[cfg(any(test, feature = "test-support"))]
        let micros = micros.saturating_add(
            self.receipt_offset_micros_for_test
                .load(std::sync::atomic::Ordering::Acquire),
        );
        Ok(micros)
    }

    /// Transfers an admitted memory lease through decode into prepared ownership.
    ///
    /// # Errors
    ///
    /// Returns the resource owner's typed category-transition failure.
    fn mark_memory_prepared(
        memory: &mut crate::resources::ScribeMemoryLease,
    ) -> Result<(), ScribeError> {
        memory.transfer_category(MemoryCategory::Decode)?;
        memory.transfer_category(MemoryCategory::Prepared)
    }

    /// Resolve the server-owned built-in one logical table names, when it is one.
    ///
    /// Every built-in resolves here, not only the canonical signal tables: the
    /// definition is what carries the table's declared correlation policy and
    /// past-event-time exemption, and a table that appends no correlation
    /// envelope must be stamped as such or its physical object would carry
    /// columns its registered schema does not. Every built-in's value validator
    /// runs from it, and the narrower canonical contract — the ledger order and
    /// the canonical physical fingerprint — is keyed off `canonical_fields` at
    /// its own sites. A dynamic table resolves to
    /// `None` and keeps the default envelope.
    fn builtin_definition(
        table: &crate::catalog::TableRef,
    ) -> Option<&'static crate::tables::BuiltinTableDefinition> {
        crate::tables::builtin_table(
            table
                .namespace
                .as_str()
                .strip_prefix("vala.")
                .unwrap_or_default(),
            &table.name,
        )
    }

    /// Preprocess one admitted append on the ingress CPU lane.
    ///
    /// Preprocessing is pre-ACK request work, so it shares the decode lane
    /// rather than the persistence lane, where a long staging encode or claim
    /// merge would hold every write behind it.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the ingress queue is saturated
    /// and any preprocessing error from [`prepare_append`].
    async fn preprocess(&self, admitted: AdmittedAppend) -> Result<PreparedAppend, ScribeError> {
        self.ingress_cpu.run(move || prepare_append(admitted)).await
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
        let RootAdmission {
            expected_schema_fingerprint,
            partition_granularity,
            registered_schema,
            receipt_micros,
            material_plan,
            mut memory,
            binding,
            reservation,
        } = self.admit_transport_frame(&mut frame).await?;
        let tenant = frame.principal.tenant_id;
        let builtin_definition = Self::builtin_definition(&frame.table);
        let request_id = uuid::Uuid::parse_str(frame.request_id.as_str()).map_err(|error| {
            ScribeError::Internal {
                detail: format!("Scribe request identity is not a UUID: {error}"),
            }
        })?;
        let rows = self
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
                    definition: builtin_definition,
                    registered_schema,
                },
            )
            .await?;
        Self::mark_memory_prepared(&mut memory)?;

        let (durable_tx, durable_rx) = tokio::sync::oneshot::channel();
        let admitted = AdmittedAppend {
            batch_id: frame.batch_id,
            request_id,
            ingested_at_micros: receipt_micros,
            rows,
            measured_wire_bytes: frame.measured_wire_bytes,
            reservation,
            memory,
            tenant,
            table: binding.table_ref,
            partition_granularity,
            queued_at: Instant::now(),
            durable_ack: Some(durable_tx),
        };
        let mut prepared = self.preprocess(admitted).await?;
        self.charge_prepared(&mut prepared)?;
        self.shards
            .try_send(prepared)
            .inspect_err(|_| super::record_scribe_rejection("queue"))?;
        let completion = durable_rx.await.map_err(|_| ScribeError::Internal {
            detail: "shard owner dropped durable batch completion".to_owned(),
        })??;
        metrics::histogram!("bifrost_scribe_ack_seconds")
            .record(append_started.elapsed().as_secs_f64());
        Ok(FrameAdmission {
            batch_id: frame.batch_id,
            rows_accepted: completion.rows,
            receipt_micros,
            first_commit: completion.first_commit,
        })
    }

    /// Charges the bytes one prepared append actually holds before WAL/ACK.
    ///
    /// Preprocessing materialized the memtable slices and their WAL records;
    /// the admitted lease grows or shrinks to exactly that retained set. A
    /// refusal drops the append and its owners before any WAL or ACK.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the shared cap cannot cover the
    /// prepared bytes after one pressure seal, and an internal error when the
    /// prepared append lost its lease or root accounting is poisoned.
    fn charge_prepared(&self, prepared: &mut PreparedAppend) -> Result<(), ScribeError> {
        let bytes = prepared.prepared_bytes;
        let table = prepared.table.name.clone();
        let memory = prepared
            .memory
            .as_mut()
            .ok_or_else(|| ScribeError::Internal {
                detail: "prepared append lost its memory lease".to_owned(),
            })?;
        self.resize_after_pressure_seal(memory, bytes, &table)
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
        self.memory.ingress_limit_bytes()
    }

    /// Moves the receipt instant of every later transport frame `ahead`.
    ///
    /// Lets a bounded journey replay an already-committed batch on a later
    /// receipt day without waiting for one. Frames already admitted keep their
    /// receipt; the shift replaces, rather than accumulates, any earlier one.
    ///
    /// # Panics
    ///
    /// Panics when `ahead` does not fit in signed microseconds.
    #[cfg(any(test, feature = "test-support"))]
    pub fn shift_receipt_clock_for_test(&self, ahead: std::time::Duration) {
        let micros = i64::try_from(ahead.as_micros()).expect("receipt shift fits i64 micros");
        self.receipt_offset_micros_for_test
            .store(micros, std::sync::atomic::Ordering::Release);
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

    /// Resizes one ingress lease, retrying once after a coordinated pressure seal.
    ///
    /// This is the admission-side realization of the D83 flush-first contract:
    /// when the shared root refuses the resize, request a shard-count-invariant
    /// pressure seal toward the low-water mark
    /// ([`ScribeImpl::request_pressure_seal_toward_low_water`]), then retry
    /// exactly once. An engaged cgroup tripwire is a container-level limit a
    /// Scribe seal cannot relieve, so it returns `IngestBusy` without a retry.
    /// A retry that still fails records the rejection labelled by the ceiling
    /// that tripped (D84, via [`super::record_scribe_ceiling_rejection`]) and
    /// returns `IngestBusy`, deferring to D71 client backoff. There is no
    /// busy-wait: freezing and persistence proceed asynchronously between the
    /// seal request and retry. A refused resize leaves the lease unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::IngestBusy`] when the cgroup tripwire is engaged or
    /// when the single post-seal retry still cannot fit under the shared cap,
    /// and an internal error when root accounting is poisoned.
    pub(super) fn resize_after_pressure_seal(
        &self,
        lease: &mut crate::resources::ScribeMemoryLease,
        bytes: usize,
        table: &str,
    ) -> Result<(), ScribeError> {
        if lease.resize_ingress(bytes).is_ok() {
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
        match lease.resize_ingress(bytes) {
            Ok(()) => Ok(()),
            Err(ScribeError::IngestBusy { .. }) => {
                let growth = bytes.saturating_sub(lease.bytes());
                super::record_scribe_ceiling_rejection(
                    if self.memory.ingress_sublimit_exceeded(growth) {
                        crate::scribe::memory::ScribeRejectionCeiling::IngressSublimit
                    } else {
                        crate::scribe::memory::ScribeRejectionCeiling::BifrostParent
                    },
                );
                Err(ScribeError::IngestBusy {
                    table: table.to_owned(),
                })
            }
            Err(error) => Err(error),
        }
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
            stall.enter();
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

#[cfg(test)]
mod tests {
    use super::{
        ScribeImpl, take_transport_decode_owner, validate_decoded_request_size,
        validate_logical_transport_frame,
    };
    use crate::catalog::TableRef;
    use crate::contracts::{
        CanonicalIngress, IngressPayload, Scribe, ScribeError, ScribeIngressFrame,
        projected_source_schema_fingerprint,
    };
    use crate::namespaces::BifrostNamespace;
    use crate::scribe::memory::MemoryCategory;
    use arrow::array::{StringArray, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use arrow::record_batch::RecordBatch;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use wyrd_runtime::{PermissionSet, Principal, PrincipalKind};
    use wyrd_spec::DataTenantId;
    use wyrd_spec::auth::PLATFORM_AUDIT_PRINCIPAL;
    use wyrd_spec::auth::PrincipalId;
    use wyrd_spec::request_id::RequestId;

    /// The Scribe ingress boundary carries no OTLP signal payload (S4).
    ///
    /// `IngressPayload` is closed over exactly the native transport frame and
    /// Gate-validated canonical batches. Neither variant can name a typed OTLP
    /// request, so Scribe cannot reacquire signal mapping authority without a
    /// contract change that fails this test.
    #[test]
    fn scribe_boundary_contains_no_otlp_signal_payload() {
        let mut native = IngressPayload::ArrowIpc(bytes::Bytes::new());
        assert!(take_transport_decode_owner(&mut native).is_none());
        let mut canonical = IngressPayload::Canonical(CanonicalIngress::unreserved(Vec::new()));
        assert!(take_transport_decode_owner(&mut canonical).is_none());
        let boundary =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/contracts.rs"))
                .expect("read the Scribe ingress contract");
        let payload = boundary
            .split("pub enum IngressPayload {")
            .nth(1)
            .and_then(|rest| rest.split_once('}'))
            .expect("locate the IngressPayload variants")
            .0;
        assert!(
            !payload.contains("Otlp"),
            "IngressPayload regained an OTLP signal variant: {payload}"
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
    async fn live_ingress_pressure_seals_retries_once_then_returns_ingest_busy() {
        let scribe = ScribeImpl::new();
        let ceiling = scribe.memory.ingress_limit_bytes();

        // Pin ingress at its ceiling: no bucket exists to seal, so the retry
        // after the pressure-seal request cannot fit and must reject.
        let pinned = scribe
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, ceiling)
            .expect("pin ingress at the ceiling");
        let mut lease = scribe
            .memory
            .try_reserve_ingress(MemoryCategory::Raw, 0)
            .expect("an empty lease never exceeds the cap");
        assert!(
            matches!(
                scribe.resize_after_pressure_seal(&mut lease, 1, "table"),
                Err(ScribeError::IngestBusy { .. })
            ),
            "a backlogged ceiling must reject after the seal request"
        );
        assert_eq!(
            lease.bytes(),
            0,
            "a refused resize leaves the lease unchanged"
        );

        // Release the pin so capacity is available; the same path now admits.
        drop(pinned);
        scribe
            .resize_after_pressure_seal(&mut lease, ceiling / 2, "table")
            .expect("admits once ingress capacity is available");
        drop(lease);

        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }

    /// Builds one already-projected ingest frame whose single `value` cell is
    /// exactly `value`.
    ///
    /// The decoded-size gate measures the projected Arrow payload rather than
    /// the wire frame, so the caller controls the measured size purely through
    /// the string it passes; `measured_wire_bytes` stays zero to keep the wire
    /// bound out of the proof. The fingerprint is computed from the same schema
    /// the batch carries, so the frame clears schema validation and reaches the
    /// size gate.
    fn decoded_size_frame(
        tenant: DataTenantId,
        principal: &Principal,
        value: String,
    ) -> ScribeIngressFrame {
        let schema = Arc::new(Schema::new(vec![
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(
                    TimeUnit::Microsecond,
                    Some(iceberg::arrow::UTC_TIME_ZONE.into()),
                ),
                false,
            ),
            Field::new("value", DataType::Utf8, false),
        ]));
        let rows = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(
                    TimestampMicrosecondArray::from(vec![chrono::Utc::now().timestamp_micros()])
                        .with_timezone(iceberg::arrow::UTC_TIME_ZONE),
                ),
                Arc::new(StringArray::from(vec![value])),
            ],
        )
        .expect("decoded-size fixture batch");
        let request_id = RequestId::now_v7();
        ScribeIngressFrame {
            authenticated_tenant: tenant,
            principal: principal.clone(),
            table: TableRef::new(BifrostNamespace::Bifrost, "decoded_size_bound"),
            expected_schema_fingerprint: Some(projected_source_schema_fingerprint(schema.as_ref())),
            request_id: request_id.clone(),
            batch_id: uuid::Uuid::now_v7(),
            measured_wire_bytes: 0,
            payload: IngressPayload::Canonical(CanonicalIngress::unreserved(vec![rows])),
        }
    }

    /// An OTLP decode lease transfers into Scribe and is resized to the bytes
    /// Scribe holds, so decode, projection, and memtable are charged once.
    ///
    /// A decode owner much larger than the tiny projected batch is carried by
    /// the frame. After the durable ACK, no transport, decode, or prepared
    /// category charge remains, and the only Scribe bytes left are the
    /// memtable's own active ownership — the decode charge was neither added
    /// to nor left behind the memtable charge.
    ///
    /// # Panics
    ///
    /// Panics when the owner cannot be reserved, the write is refused, or a
    /// category other than memtable ownership still holds bytes after ACK.
    #[tokio::test]
    async fn decode_to_memtable_transfers_one_charge() {
        let scribe = ScribeImpl::new();
        let tenant = DataTenantId::new(uuid::Uuid::now_v7()).expect("random tenant is valid");
        let principal = Principal {
            id: PrincipalId::new(uuid::Uuid::now_v7()),
            kind: PrincipalKind::User,
            tenant_id: tenant,
            roles: Vec::new(),
            effective_permissions: PermissionSet::new(),
            credential_id: None,
        };
        let baseline = scribe.memory_snapshot().scribe_total_bytes;
        let decode_bytes = 4 * 1024 * 1024;
        let mut frame = decoded_size_frame(tenant, &principal, "v".repeat(1024));
        let IngressPayload::Canonical(canonical) = &mut frame.payload else {
            unreachable!("the fixture frame is canonical");
        };
        canonical.owner = Some(crate::contracts::OtlpDecodeOwner {
            memory: scribe
                .memory
                .try_reserve_ingress(MemoryCategory::Decode, decode_bytes)
                .expect("decode owner fits the embedded root"),
        });
        assert_eq!(
            scribe.memory_snapshot().scribe_total_bytes,
            baseline + decode_bytes
        );

        let admission = Scribe::ingest_frame(&scribe, frame)
            .await
            .expect("the canonical write is admitted");
        assert_eq!(admission.rows_accepted, 1);

        let after = scribe.memory_snapshot();
        for category in [
            MemoryCategory::Raw,
            MemoryCategory::Decode,
            MemoryCategory::Prepared,
        ] {
            assert_eq!(
                after.categories[category as usize], 0,
                "{category:?} still holds a pre-ACK charge"
            );
        }
        let memtable = after.categories[MemoryCategory::Active as usize]
            + after.categories[MemoryCategory::Immutable as usize];
        assert!(memtable > 0, "the ACKed rows are charged to the memtable");
        assert!(
            memtable < decode_bytes,
            "the decode charge was transferred and resized, not retained"
        );
        assert_eq!(after.scribe_total_bytes, baseline + memtable);
        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }

    /// The system owner passes transport validation only as the platform audit
    /// principal writing the retained audit log; caller tables and any other
    /// principal stay refused.
    ///
    /// # Panics
    ///
    /// Panics when an admission decision differs from the audit-only exception.
    #[test]
    fn system_owner_frames_admit_only_internal_audit_publication() {
        let system = DataTenantId::SYSTEM_OWNER;
        let publisher = Principal::new(
            PLATFORM_AUDIT_PRINCIPAL,
            PrincipalKind::User,
            system,
            Vec::new(),
            PermissionSet::new(),
        );
        let audit_log = TableRef::new(BifrostNamespace::Audit, "audit_log");

        let mut frame = decoded_size_frame(system, &publisher, "x".to_owned());
        assert!(matches!(
            validate_logical_transport_frame(&frame, usize::MAX),
            Err(ScribeError::InvalidFrame)
        ));
        frame.table = audit_log.clone();
        validate_logical_transport_frame(&frame, usize::MAX)
            .expect("internal audit publication admits the system owner");

        let mut caller = decoded_size_frame(
            system,
            &Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                system,
                Vec::new(),
                PermissionSet::new(),
            ),
            "x".to_owned(),
        );
        caller.table = audit_log;
        assert!(matches!(
            validate_logical_transport_frame(&caller, usize::MAX),
            Err(ScribeError::InvalidFrame)
        ));
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
            credential_id: None,
        };
        let make_frame = |value: String| decoded_size_frame(tenant, &principal, value);
        let admission_before = scribe.admission_snapshot();
        let memory_before = scribe.memory_snapshot();
        let wal_before = crate::scribe::wal::wal_file_bytes_for_test(scribe.wal.base_dir());
        let stats_before = scribe
            .memtable_stats()
            .expect("pre-rejection memtable stats");
        let error = Scribe::ingest_frame(&scribe, make_frame("x".repeat(limit)))
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
        assert_eq!(
            crate::scribe::wal::wal_file_bytes_for_test(scribe.wal.base_dir()),
            wal_before
        );
        assert_eq!(
            scribe
                .memtable_stats()
                .expect("post-rejection memtable stats"),
            stats_before
        );
        let admission = Scribe::ingest_frame(&scribe, make_frame("small".to_owned()))
            .await
            .expect("sub-limit request remains durably admissible");
        assert_eq!(admission.rows_accepted, 1);
        assert!(crate::scribe::wal::wal_file_bytes_for_test(scribe.wal.base_dir()) > wal_before);
        scribe
            .shutdown(Instant::now() + Duration::from_secs(1))
            .await;
    }
}
