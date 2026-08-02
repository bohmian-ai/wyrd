//! Query-scoped live-tail fence acquisition, draining, and release.
//!
//! The owner discovers exact Scribe intervals, acquires them before audit, drains
//! bounded pages after audit, charges parent memory, and awaits release on every
//! ordinary success or failure path without spawning detached cleanup.

use super::*;

/// Maximum joined cleanup time for every fence after the query deadline has failed.
///
/// Cleanup receives a fresh private budget so an expired query deadline cannot
/// prevent first-polling a release. All sibling releases run concurrently under
/// this bound; cancellation of the owning query task can still stop outstanding
/// remote attempts, which remain idempotent and recoverable by the Scribe TTL.
const TAIL_FENCE_RELEASE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

/// Owns the dependencies and invariants for one query's bounded live-tail cut.
///
/// The owner keeps fence discovery, parent-memory accounting, telemetry,
/// deadline, freshness, and cancellation aligned across acquisition and drain.
/// It does not outlive the query attempt that constructed it.
pub(super) struct TailFenceDrainer<'a> {
    /// Directory used to resolve every discovered Scribe stream transport.
    pub(super) tails: &'a TailTransportDirectory,
    /// Parent-governed memory resources charged for decoded live batches.
    pub(super) memory: &'a OracleMemoryResources,
    /// Query telemetry retaining live-tail memory accounting.
    pub(super) telemetry: Arc<OracleTelemetry>,
    /// Immutable admission class applied to live-tail memory metrics.
    pub(super) query_class: QueryClass,
    /// Absolute deadline shared by fence acquisition and every page read.
    pub(super) deadline: Instant,
    /// Admission lifecycle cancellation shared with renewal and streaming.
    pub(super) cancellation: CancellationToken,
    /// Caller-selected strict or degraded live-source failure policy.
    pub(super) freshness: wyrd_spec::vala::api::FreshnessPolicy,
}

/// One metadata-only acquired fence retained until its post-audit drain.
pub(super) struct AcquiredTailFence {
    /// Canonical table receiving drained batches.
    pub(super) table: String,
    /// Transport that owns the retained interval.
    pub(super) transport: Arc<dyn TailReadTransport>,
    /// Immutable fence metadata returned by Scribe.
    pub(super) fence: wyrd_spec::vala::api::TailReadFence,
    /// Successful acquisition time used by the hold-duration histogram.
    pub(super) acquired_at: Instant,
    /// Whether every requested page completed before automatic release.
    pub(super) drain_succeeded: bool,
}

/// One planned metadata-only fence acquisition for a discovered Scribe stream.
pub(super) struct TailFenceAcquisition {
    /// Canonical table that will receive the retained live interval.
    pub(super) table: String,
    /// Transport selected for the exact Scribe stream identity.
    pub(super) transport: Arc<dyn TailReadTransport>,
    /// Validated private request pinning cursor, schema, and deadline metadata.
    pub(super) request: wyrd_spec::vala::api::AcquireTailFenceRequest,
}

/// Bounded live rows and their parent-governor reservations.
#[derive(Default)]
pub(super) struct DrainedTails {
    /// Per-table shallow Arrow batches.
    pub(super) batches: HashMap<String, Vec<RecordBatch>>,
    /// Reservations retained until the final query stream drops.
    pub(super) reservations: Vec<AccountedMemoryReservation>,
    /// Whether one requested live source was unavailable.
    pub(super) degraded: bool,
}

/// One live-tail interval drained and charged under the parent governor.
pub(super) struct DrainedTailFence {
    /// Canonical table receiving the interval.
    pub(super) table: String,
    /// Shallow live batches retained by this interval.
    pub(super) batches: Vec<RecordBatch>,
    /// Parent reservations retaining every decoded batch.
    pub(super) reservations: Vec<AccountedMemoryReservation>,
}

impl TailFenceDrainer<'_> {
    /// Creates one query-attempt owner for live-tail acquisition and draining.
    #[must_use]
    pub(super) fn new<'a>(
        tails: &'a TailTransportDirectory,
        memory: &'a OracleMemoryResources,
        telemetry: Arc<OracleTelemetry>,
        query_class: QueryClass,
        deadline: Instant,
        cancellation: CancellationToken,
        freshness: wyrd_spec::vala::api::FreshnessPolicy,
    ) -> TailFenceDrainer<'a> {
        TailFenceDrainer {
            tails,
            memory,
            telemetry,
            query_class,
            deadline,
            cancellation,
            freshness,
        }
    }

    /// Acquires the exact last-sealed-to-live interval for each observed stream.
    ///
    /// Acquisition reads metadata only. Any partial failure releases every
    /// previously acquired fence before returning.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable for missing transports, invalid
    /// cursor/schema metadata, admission cancellation, expired deadlines, or
    /// Scribe rejection.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation stops outstanding acquisition work and returns
    /// query execution failure after releasing every completed fence.
    #[tracing::instrument(
        name = "bifrost.oracle.tail_fence",
        skip_all,
        fields(table_count = cuts.len())
    )]
    pub(super) async fn acquire(
        &self,
        cuts: &[PinnedSealedTable],
    ) -> Result<Vec<AcquiredTailFence>, BifrostError> {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let wire_deadline = chrono::Utc::now()
            + chrono::Duration::from_std(remaining).map_err(|_| BifrostError::QueryTimeout)?;
        let work = self.plan_acquisitions(cuts, wire_deadline)?;
        let mut results = Vec::with_capacity(work.len());
        let mut pending = FuturesUnordered::new();
        for acquisition in work {
            pending.push(self.acquire_fence(acquisition));
            if pending.len() == 8 {
                while let Some(result) = pending.next().await {
                    results.push(result);
                }
            }
        }
        while let Some(result) = pending.next().await {
            results.push(result);
        }
        let mut acquired = Vec::new();
        let mut failure = None;
        for result in results {
            match result {
                Ok(fence) => acquired.push(fence),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let Some(error) = failure {
            self.release_acquired(acquired).await;
            Err(error)
        } else {
            Ok(acquired)
        }
    }

    /// Releases every completed acquisition under one fresh joined cleanup budget.
    ///
    /// Every release future is installed and first-polled independently of the
    /// expired query deadline. A blocking or failed release cannot prevent a
    /// sibling attempt, and no release future survives this awaited method. The
    /// method intentionally reports failures through telemetry/logging because it
    /// is cleanup for an already-selected query error; confirmed sibling releases
    /// remain complete when another release fails or times out.
    ///
    /// # Cancellation
    ///
    /// Cancelling the owning task can interrupt outstanding remote releases after
    /// some siblings complete. Release is idempotent and Scribe's retained-fence
    /// TTL remains the crash/cancellation fallback rather than ordinary cleanup.
    pub(super) async fn release_acquired(&self, acquired: Vec<AcquiredTailFence>) {
        let mut releases = FuturesUnordered::new();
        for mut fence in acquired {
            releases.push(async move {
                self.release_one_with_timeout(&mut fence, TAIL_FENCE_RELEASE_CLEANUP_TIMEOUT)
                    .await
            });
        }
        while let Some(released) = releases.next().await {
            if !released {
                tracing::warn!("Oracle live-tail fence release was not confirmed");
            }
        }
    }

    /// Drains every acquired interval under page and parent-memory bounds.
    ///
    /// Every fence is released before this method returns, including all error
    /// paths. Strict mode fails on the first unavailable source; degraded mode
    /// records the unavailable live tier and retains complete drained sources.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable for strict drain failure, malformed
    /// cursor progress, deadline expiry, admission cancellation, or
    /// parent-memory exhaustion.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation stops outstanding page reads. Each active drain
    /// explicitly awaits its fence release before returning failure. Cancellation
    /// of the caller task itself can interrupt that awaited release after other
    /// intervals have completed; unconfirmed intervals remain recoverable by TTL.
    #[tracing::instrument(
        name = "bifrost.oracle.tail",
        skip_all,
        fields(fence_count = fences.len(), freshness = ?self.freshness)
    )]
    pub(super) async fn drain(
        &self,
        fences: Vec<AcquiredTailFence>,
    ) -> Result<DrainedTails, BifrostError> {
        let mut drained = DrainedTails::default();
        let results = futures_util::stream::iter(fences)
            .map(|acquired| self.drain_fence(acquired))
            .buffer_unordered(8)
            .collect::<Vec<_>>()
            .await;
        let mut failed_tables = HashSet::new();
        let mut strict_failure = None;
        for result in results {
            match result {
                Ok(interval) => {
                    drained
                        .batches
                        .entry(interval.table)
                        .or_default()
                        .extend(interval.batches);
                    drained.reservations.extend(interval.reservations);
                }
                Err((table, error)) => {
                    failed_tables.insert(table);
                    if strict_failure.is_none() {
                        strict_failure = Some(error);
                    }
                }
            }
        }
        if let Some(error) = strict_failure {
            if error == BifrostError::QueryTimeout
                || self.freshness == wyrd_spec::vala::api::FreshnessPolicy::Strict
            {
                return Err(error);
            }
            drained.degraded = true;
            for table in failed_tables {
                drained.batches.remove(&table);
            }
        }
        Ok(drained)
    }

    /// Plans one exact fence request for every sealed or independently live stream.
    ///
    /// The synchronous stage merges duplicate hot-file cursors, adds streams
    /// discovered without sealed files, and validates all private request
    /// metadata before any transport IO begins.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable when stream routing, cursor conversion,
    /// event-day parsing, or schema request construction fails.
    fn plan_acquisitions(
        &self,
        cuts: &[PinnedSealedTable],
        wire_deadline: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<TailFenceAcquisition>, BifrostError> {
        let mut work = Vec::new();
        for cut in cuts {
            let table = cut.binding.table_ref.fqn();
            let mut streams = std::collections::BTreeMap::<
                (uuid::Uuid, String, u64),
                (
                    wyrd_spec::vala::api::EventDay,
                    wyrd_spec::vala::api::TailCursor,
                    Arc<dyn TailReadTransport>,
                ),
            >::new();
            for file in &cut.hot_files {
                let event_day = wyrd_spec::vala::api::EventDay::new(
                    file.partition_day.format("%Y-%m-%d").to_string(),
                )
                .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let writer_epoch = u64::try_from(file.writer_epoch)
                    .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let wal_lsn = u64::try_from(file.wal_lsn_max)
                    .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
                let Some(transport) =
                    self.tails
                        .get_for_stream(&table, file.node_id, cut.binding.tenant)
                else {
                    return Err(BifrostError::QueryVisibilityUnavailable);
                };
                let key = (file.node_id, event_day.as_str().to_owned(), writer_epoch);
                let cursor = wyrd_spec::vala::api::TailCursor {
                    writer_epoch,
                    wal_lsn,
                    batch_id: uuid::Uuid::from_u128(u128::MAX),
                    row_ordinal: u32::MAX,
                };
                streams
                    .entry(key)
                    .and_modify(|(_, current, _)| {
                        if cursor.wal_lsn > current.wal_lsn {
                            *current = cursor.clone();
                        }
                    })
                    .or_insert((event_day, cursor, transport));
            }
            for route in self.tails.live_streams(&table, cut.binding.tenant) {
                let key = (
                    route.node_id,
                    route.event_day.as_str().to_owned(),
                    route.writer_epoch,
                );
                streams.entry(key).or_insert_with(|| {
                    (
                        route.event_day,
                        wyrd_spec::vala::api::TailCursor {
                            writer_epoch: route.writer_epoch,
                            wal_lsn: 0,
                            batch_id: uuid::Uuid::nil(),
                            row_ordinal: 0,
                        },
                        route.transport,
                    )
                });
            }
            if streams.is_empty() {
                return Err(BifrostError::QueryVisibilityUnavailable);
            }
            for (_, (event_day, exclusive_sealed, transport)) in streams {
                let request = tail_fence_request(cut, event_day, exclusive_sealed, wire_deadline)?;
                work.push(TailFenceAcquisition {
                    table: table.clone(),
                    transport,
                    request,
                });
            }
        }
        Ok(work)
    }

    /// Acquires one metadata-only Scribe fence from an owned transport request.
    ///
    /// Keeping the transport owned by this future makes the Oracle query future
    /// transport-safe without exposing a trait-object borrow through Gate's
    /// HTTP/gRPC handler futures.
    ///
    /// # Errors
    ///
    /// Returns visibility unavailable when Scribe rejects the interval, query
    /// timeout when the shared deadline expires, or execution failure when the
    /// admitted query is cancelled.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation wins without waiting for the transport timeout.
    async fn acquire_fence(
        &self,
        acquisition: TailFenceAcquisition,
    ) -> Result<AcquiredTailFence, BifrostError> {
        let TailFenceAcquisition {
            table,
            transport,
            request,
        } = acquisition;
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let acquisition = tokio::select! {
            () = self.cancellation.cancelled() => {
                return Err(BifrostError::QueryExecutionFailed);
            }
            result = tokio::time::timeout(remaining, transport.acquire_fence(request)) => result,
        };
        match acquisition {
            Ok(Ok(fence)) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "success"
                )
                .increment(1);
                Ok(AcquiredTailFence {
                    table,
                    transport,
                    fence,
                    acquired_at: Instant::now(),
                    drain_succeeded: false,
                })
            }
            Ok(Err(error)) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "failed"
                )
                .increment(1);
                tracing::warn!(error = %error, "Oracle live-tail fence acquisition failed");
                Err(BifrostError::QueryVisibilityUnavailable)
            }
            Err(_) => {
                metrics::counter!(
                    "bifrost_oracle_tail_fences_total",
                    "locality" => "local",
                    "outcome" => "failed"
                )
                .increment(1);
                Err(BifrostError::QueryTimeout)
            }
        }
    }

    /// Drains one retained fence and releases it on every completion path.
    ///
    /// # Errors
    ///
    /// Returns the canonical table plus timeout, cancellation, or visibility
    /// failure when a page, cursor, or parent-memory bound cannot be satisfied.
    ///
    /// # Cancellation
    ///
    /// Admission cancellation interrupts the active page read and then awaits the
    /// retained Scribe interval's release before returning. Cancellation of the
    /// caller task can interrupt that cleanup; pages already decoded remain local
    /// partial progress and are discarded with this failed interval.
    async fn drain_fence(
        &self,
        mut acquired: AcquiredTailFence,
    ) -> Result<DrainedTailFence, (String, BifrostError)> {
        let table = acquired.table.clone();
        let mut after = None;
        let mut batches = Vec::new();
        let mut reservations = Vec::new();
        loop {
            let Some(remaining) = self.deadline.checked_duration_since(Instant::now()) else {
                self.release_one(&mut acquired).await;
                return Err((table, BifrostError::QueryTimeout));
            };
            let page_started = Instant::now();
            let page_result = tokio::select! {
                () = self.cancellation.cancelled() => {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryExecutionFailed));
                }
                result = tokio::time::timeout(
                    remaining,
                    acquired.transport.read_page(wyrd_spec::vala::api::TailPageRequest {
                        fence_id: acquired.fence.fence_id,
                        after: after.clone(),
                        max_rows: 4_096,
                        max_encoded_bytes: 16 * 1024 * 1024,
                    }),
                ) => result,
            };
            let page_outcome = if matches!(&page_result, Ok(Ok(_))) {
                "success"
            } else {
                "failed"
            };
            metrics::counter!(
                "bifrost_oracle_tail_pages_total",
                "locality" => "local",
                "outcome" => page_outcome
            )
            .increment(1);
            metrics::histogram!(
                "bifrost_oracle_tail_page_seconds",
                "locality" => "local",
                "outcome" => page_outcome
            )
            .record(page_started.elapsed().as_secs_f64());
            let page = match page_result {
                Ok(Ok(page)) => page,
                Ok(Err(error)) => {
                    tracing::warn!(error = %error, "Oracle live-tail drain failed");
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryVisibilityUnavailable));
                }
                Err(_) => {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryTimeout));
                }
            };
            for batch in page.batches {
                let Ok(reservation) = self
                    .memory
                    .governor
                    .try_reserve_parent(batch.get_array_memory_size())
                else {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryVisibilityUnavailable));
                };
                reservations.push(self.telemetry.account_memory(
                    reservation,
                    self.query_class,
                    OracleMemoryKind::Tail,
                ));
                batches.push(batch.as_ref().clone());
            }
            after = page.next;
            if page.complete {
                break;
            }
            if after.is_none() {
                self.release_one(&mut acquired).await;
                return Err((table, BifrostError::QueryVisibilityUnavailable));
            }
        }
        let released = self.release_one(&mut acquired).await;
        if !released {
            tracing::warn!("Oracle live-tail fence release was not confirmed");
            return Err((table, BifrostError::QueryVisibilityUnavailable));
        }
        acquired.drain_succeeded = true;
        Ok(DrainedTailFence {
            table,
            batches,
            reservations,
        })
    }

    /// Awaits one fence release within the remaining query deadline.
    ///
    /// Returns `true` only for a transport-confirmed release. An expired deadline
    /// still first-polls the zero-budget timeout path; caller cancellation may
    /// interrupt the attempt, leaving Scribe TTL as the recovery boundary.
    async fn release_one(&self, acquired: &mut AcquiredTailFence) -> bool {
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .unwrap_or_default();
        self.release_one_with_timeout(acquired, remaining).await
    }

    /// Polls one release within the caller-owned lifecycle budget.
    ///
    /// The query drain path supplies its remaining query duration, while
    /// post-error aggregate cleanup supplies a fresh private bound. The method
    /// records confirmed completion on the fence so `Drop` reports truthful hold
    /// telemetry. Timeout, transport failure, or a negative response returns
    /// `false`; cancellation can interrupt before that result is recorded.
    async fn release_one_with_timeout(
        &self,
        acquired: &mut AcquiredTailFence,
        timeout: Duration,
    ) -> bool {
        let released = tokio::time::timeout(
            timeout,
            acquired
                .transport
                .release_fence_async(acquired.fence.fence_id),
        )
        .await
        .is_ok_and(|result| result.is_ok_and(|response| response.released));
        acquired.drain_succeeded = released;
        released
    }
}

impl Drop for AcquiredTailFence {
    /// Records hold duration without starting detached cleanup work.
    ///
    /// A fence not marked successfully released is recorded as failed and relies
    /// on the Scribe retention TTL; dropping never performs transport IO.
    fn drop(&mut self) {
        let outcome = if self.drain_succeeded {
            "success"
        } else {
            "failed"
        };
        metrics::histogram!(
            "bifrost_oracle_tail_fence_hold_seconds",
            "locality" => "local",
            "outcome" => outcome
        )
        .record(self.acquired_at.elapsed().as_secs_f64());
    }
}

/// Builds one validated metadata-only tail-fence request from a pinned sealed cut.
///
/// # Errors
///
/// Returns visibility unavailable for missing sealed cursor metadata, schema
/// conversion failure, or invalid private wire values.
fn tail_fence_request(
    cut: &PinnedSealedTable,
    event_day: wyrd_spec::vala::api::EventDay,
    exclusive_sealed: wyrd_spec::vala::api::TailCursor,
    deadline: chrono::DateTime<chrono::Utc>,
) -> Result<wyrd_spec::vala::api::AcquireTailFenceRequest, BifrostError> {
    let arrow_schema =
        iceberg::arrow::schema_to_arrow_schema(cut.iceberg_table.metadata().current_schema())
            .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    let fingerprint = crate::contracts::projected_source_schema_fingerprint(&arrow_schema);
    let fingerprint = wyrd_spec::vala::api::SchemaFingerprint::new(hex::encode(fingerprint.0))
        .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    Ok(wyrd_spec::vala::api::AcquireTailFenceRequest {
        binding: wyrd_spec::vala::api::TenantTableBinding {
            tenant_id: cut.binding.tenant,
            namespace: cut
                .binding
                .logical_namespace
                .strip_prefix("vala.")
                .ok_or(BifrostError::QueryVisibilityUnavailable)?
                .to_owned(),
            table: cut.binding.table_name.clone(),
        },
        event_day,
        exclusive_sealed,
        deadline,
        schema_fingerprint: fingerprint,
        tail_protocol_version: TAIL_PROTOCOL_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;

    use super::*;
    use crate::scribe::tail_rpc::{FenceRelease, LocalTailPage, TailReadError};

    /// Deterministic remote-style transport for fence cleanup and drain lifecycle tests.
    struct TailLifecycleTransport {
        /// Whether page reads fail before producing any rows.
        fail_read: bool,
        /// Whether release returns a transport failure after being polled.
        fail_release: bool,
        /// Whether release remains pending until its caller-owned timeout expires.
        block_release: bool,
        /// Number of page-read attempts observed by the transport.
        reads: AtomicUsize,
        /// Number of release futures first-polled by the owner.
        release_polls: AtomicUsize,
    }

    #[async_trait]
    impl TailReadTransport for TailLifecycleTransport {
        /// Rejects acquisition because these focused tests begin with an owned fence.
        ///
        /// # Errors
        ///
        /// Always returns a deterministic transport-state error.
        async fn acquire_fence(
            &self,
            _request: wyrd_spec::vala::api::AcquireTailFenceRequest,
        ) -> Result<wyrd_spec::vala::api::TailReadFence, TailReadError> {
            Err(TailReadError::State {
                detail: "test owns the acquired fence".to_owned(),
            })
        }

        /// Returns one empty complete page or the configured remote read failure.
        ///
        /// # Errors
        ///
        /// Returns a deterministic transport-state error when `fail_read` is set.
        async fn read_page(
            &self,
            _request: wyrd_spec::vala::api::TailPageRequest,
        ) -> Result<LocalTailPage, TailReadError> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.fail_read {
                return Err(TailReadError::State {
                    detail: "injected page failure".to_owned(),
                });
            }
            Ok(LocalTailPage {
                batches: Vec::new(),
                next: None,
                complete: true,
            })
        }

        /// Rejects the synchronous release path so tests prove the awaited async path.
        ///
        /// # Errors
        ///
        /// Always returns a deterministic transport-state error.
        fn release_fence(
            &self,
            _fence_id: wyrd_spec::vala::api::TailFenceId,
        ) -> Result<FenceRelease, TailReadError> {
            Err(TailReadError::State {
                detail: "test requires async release".to_owned(),
            })
        }

        /// Records the first poll, then succeeds, fails, or remains pending as configured.
        ///
        /// # Errors
        ///
        /// Returns a deterministic transport-state error when `fail_release` is set.
        /// Cancellation while pending records the first poll but no completion.
        async fn release_fence_async(
            &self,
            _fence_id: wyrd_spec::vala::api::TailFenceId,
        ) -> Result<FenceRelease, TailReadError> {
            self.release_polls.fetch_add(1, Ordering::SeqCst);
            if self.block_release {
                std::future::pending().await
            } else if self.fail_release {
                Err(TailReadError::State {
                    detail: "injected release failure".to_owned(),
                })
            } else {
                Ok(FenceRelease { released: true })
            }
        }
    }

    /// Builds the minimal query-scoped owner needed to exercise fence lifecycle helpers.
    fn drainer<'a>(
        tails: &'a TailTransportDirectory,
        memory: &'a OracleMemoryResources,
    ) -> TailFenceDrainer<'a> {
        TailFenceDrainer::new(
            tails,
            memory,
            Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1)))),
            QueryClass::Interactive,
            Instant::now() + Duration::from_secs(1),
            CancellationToken::new(),
            wyrd_spec::vala::api::FreshnessPolicy::Strict,
        )
    }

    /// Creates one valid acquired fence around the supplied remote transport.
    fn acquired(transport: Arc<dyn TailReadTransport>) -> AcquiredTailFence {
        let cursor = wyrd_spec::vala::api::TailCursor {
            writer_epoch: 1,
            wal_lsn: 0,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        };
        AcquiredTailFence {
            table: "vala.bifrost.spans".to_owned(),
            transport,
            fence: wyrd_spec::vala::api::TailReadFence {
                fence_id: wyrd_spec::vala::api::TailFenceId::new(uuid::Uuid::now_v7()),
                binding: wyrd_spec::vala::api::TenantTableBinding {
                    tenant_id: DataTenantId::new_v7(),
                    namespace: "vala.bifrost".to_owned(),
                    table: "spans".to_owned(),
                },
                event_day: wyrd_spec::vala::api::EventDay::new("2026-08-02")
                    .expect("test event day is valid"),
                stream: wyrd_spec::vala::api::TailStreamIdentity {
                    node_id: wyrd_spec::vala::api::NodeId::new(uuid::Uuid::now_v7()),
                    writer_epoch: 1,
                },
                exclusive_sealed: cursor.clone(),
                inclusive_live: cursor,
                schema_fingerprint: wyrd_spec::vala::api::SchemaFingerprint::new("schema")
                    .expect("test fingerprint is valid"),
                tail_protocol_version: TAIL_PROTOCOL_VERSION,
                expires_at: chrono::Utc::now() + chrono::Duration::seconds(1),
            },
            acquired_at: Instant::now(),
            drain_succeeded: false,
        }
    }

    /// A complete remote drain awaits exactly one release and marks the fence successful.
    #[tokio::test]
    async fn remote_drain_releases_fence_before_success() {
        let transport = Arc::new(TailLifecycleTransport {
            fail_read: false,
            fail_release: false,
            block_release: false,
            reads: AtomicUsize::new(0),
            release_polls: AtomicUsize::new(0),
        });
        let tails = TailTransportDirectory::default();
        let memory = OracleMemoryResources {
            governor: BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("test memory governor"),
            reconciliation_limit_bytes: 1024,
        };
        let drained = drainer(&tails, &memory)
            .drain_fence(acquired(
                Arc::clone(&transport) as Arc<dyn TailReadTransport>
            ))
            .await
            .expect("complete remote page and release succeed");
        assert!(drained.batches.is_empty());
        assert_eq!(transport.reads.load(Ordering::SeqCst), 1);
        assert_eq!(transport.release_polls.load(Ordering::SeqCst), 1);
    }

    /// A remote page error still awaits release before returning visibility failure.
    #[tokio::test]
    async fn remote_drain_error_releases_fence_before_failure() {
        let transport = Arc::new(TailLifecycleTransport {
            fail_read: true,
            fail_release: false,
            block_release: false,
            reads: AtomicUsize::new(0),
            release_polls: AtomicUsize::new(0),
        });
        let tails = TailTransportDirectory::default();
        let memory = OracleMemoryResources {
            governor: BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("test memory governor"),
            reconciliation_limit_bytes: 1024,
        };
        let result = drainer(&tails, &memory)
            .drain_fence(acquired(
                Arc::clone(&transport) as Arc<dyn TailReadTransport>
            ))
            .await;
        let Err(error) = result else {
            panic!("remote page failure must reject the interval");
        };
        assert_eq!(error.1, BifrostError::QueryVisibilityUnavailable);
        assert_eq!(transport.reads.load(Ordering::SeqCst), 1);
        assert_eq!(transport.release_polls.load(Ordering::SeqCst), 1);
    }

    /// A remote release error is observed once and never retried from fence drop.
    #[tokio::test]
    async fn remote_release_error_is_observed_without_drop_retry() {
        let transport = Arc::new(TailLifecycleTransport {
            fail_read: false,
            fail_release: true,
            block_release: false,
            reads: AtomicUsize::new(0),
            release_polls: AtomicUsize::new(0),
        });
        let tails = TailTransportDirectory::default();
        let memory = OracleMemoryResources {
            governor: BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("test memory governor"),
            reconciliation_limit_bytes: 1024,
        };
        let mut fence = acquired(Arc::clone(&transport) as Arc<dyn TailReadTransport>);
        assert!(
            !drainer(&tails, &memory)
                .release_one_with_timeout(&mut fence, Duration::from_millis(50))
                .await
        );
        drop(fence);
        tokio::task::yield_now().await;
        assert_eq!(transport.release_polls.load(Ordering::SeqCst), 1);
    }

    /// A blocked remote release is first-polled and bounded by the caller timeout.
    #[tokio::test]
    async fn remote_release_timeout_first_polls_without_detached_cleanup() {
        let transport = Arc::new(TailLifecycleTransport {
            fail_read: false,
            fail_release: false,
            block_release: true,
            reads: AtomicUsize::new(0),
            release_polls: AtomicUsize::new(0),
        });
        let tails = TailTransportDirectory::default();
        let memory = OracleMemoryResources {
            governor: BifrostMemoryGovernor::new(512 * 1024 * 1024).expect("test memory governor"),
            reconciliation_limit_bytes: 1024,
        };
        let mut fence = acquired(Arc::clone(&transport) as Arc<dyn TailReadTransport>);
        assert!(
            !drainer(&tails, &memory)
                .release_one_with_timeout(&mut fence, Duration::from_millis(10))
                .await
        );
        assert_eq!(transport.release_polls.load(Ordering::SeqCst), 1);
        drop(fence);
        tokio::task::yield_now().await;
        assert_eq!(transport.release_polls.load(Ordering::SeqCst), 1);
    }
}
