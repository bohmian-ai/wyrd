//! Query-scoped live-tail fence acquisition, draining, and release.
//!
//! The owner discovers exact Scribe intervals, acquires them before audit, drains
//! bounded pages after audit, charges parent memory, and awaits release on every
//! ordinary success or failure path without spawning detached cleanup.

use super::*;
use futures_util::stream::FuturesUnordered;

/// Decodes the exact time partition of one hot `vala.file_list` row.
///
/// The two durable columns are written only by Scribe from a canonical
/// [`crate::catalog::layout::TimePartition`], so a row that does not decode is
/// control-plane corruption and makes the query's visibility cut unusable.
///
/// # Errors
/// Returns [`BifrostError::QueryVisibilityUnavailable`] when the stored
/// granularity or start is not canonical.
pub(crate) fn hot_row_partition(
    row: &vala_sql::row_types::file_list::HotFileRow,
) -> Result<wyrd_spec::vala::api::TimePartitionWire, BifrostError> {
    crate::catalog::layout::TimePartition::from_durable_columns(
        &row.partition_granularity,
        row.partition_start,
    )
    .map(crate::catalog::layout::TimePartition::to_wire)
    .map_err(|_| BifrostError::QueryVisibilityUnavailable)
}

/// One query-scoped live-tail route discovered from an authoritative Scribe.
pub struct DiscoveredTailRoute {
    /// Exact time partition retained by the Scribe stream.
    pub time_partition: wyrd_spec::vala::api::TimePartitionWire,
    /// Exact node and writer epoch returned by discovery.
    pub stream: wyrd_spec::vala::api::TailStreamIdentity,
    /// Authorized transport to that exact Scribe incarnation.
    pub transport: Arc<dyn TailReadTransport>,
}

/// Query-scoped resolver for live Scribe streams.
#[async_trait::async_trait]
pub trait TailStreamDiscovery: Send + Sync {
    /// Refreshes authoritative membership and lists active streams for one binding.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryVisibilityUnavailable`] for stale membership,
    /// authorization, TLS, discovery, or audit failures.
    async fn discover(
        &self,
        binding: &wyrd_spec::vala::api::TenantTableBinding,
        query_id: uuid::Uuid,
        deadline: Instant,
    ) -> Result<Vec<DiscoveredTailRoute>, crate::scribe::tail_rpc::TailReadError>;

    /// Toggles a test-tier discovery outage without changing production behavior.
    #[cfg(feature = "test-support")]
    fn set_unavailable_for_test(&self, _unavailable: bool) {}
}

/// Maximum joined cleanup time for every fence after the query deadline has failed.
///
/// Cleanup receives a fresh private budget so an expired query deadline cannot
/// prevent first-polling a release. All sibling releases run concurrently under
/// this bound; cancellation of the owning query task can still stop outstanding
/// remote attempts, which remain idempotent and recoverable by the Scribe TTL.
const TAIL_FENCE_RELEASE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);

/// Derives the exclusive live-tail cursor from every sealed row in the pinned cut.
///
/// The complete manifest is intentional: rows already represented by the pinned
/// Iceberg snapshot no longer need a hot-Parquet scan, but still fence the live
/// WAL interval. The maximum represented LSN makes the live source disjoint from
/// both sealed physical sources.
///
/// # Errors
///
/// Returns visibility unavailable when a matching persisted LSN is negative.
fn sealed_watermark(
    manifest: &[vala_sql::row_types::file_list::HotFileRow],
    node_id: uuid::Uuid,
    writer_epoch: u64,
    time_partition: wyrd_spec::vala::api::TimePartitionWire,
) -> Result<wyrd_spec::vala::api::TailCursor, BifrostError> {
    let wal_lsn = manifest
        .iter()
        .filter(|file| {
            file.node_id == node_id
                && u64::try_from(file.writer_epoch).ok() == Some(writer_epoch)
                && hot_row_partition(file).is_ok_and(|partition| partition == time_partition)
        })
        .map(|file| {
            u64::try_from(file.wal_lsn_max).map_err(|_| BifrostError::QueryVisibilityUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max();
    Ok(match wal_lsn {
        Some(wal_lsn) => wyrd_spec::vala::api::TailCursor {
            writer_epoch,
            wal_lsn,
            batch_id: uuid::Uuid::from_u128(u128::MAX),
            row_ordinal: u32::MAX,
        },
        None => wyrd_spec::vala::api::TailCursor {
            writer_epoch,
            wal_lsn: 0,
            batch_id: uuid::Uuid::nil(),
            row_ordinal: 0,
        },
    })
}

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
    /// Query-local pool shared with `DataFusion` and retained live batches.
    pub(super) query_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// Query telemetry retaining live-tail memory accounting.
    pub(super) telemetry: Arc<OracleTelemetry>,
    /// Immutable admission class applied to live-tail memory metrics.
    pub(super) query_class: QueryClass,
    /// Absolute deadline shared by fence acquisition and every page read.
    pub(super) deadline: Instant,
    /// Admission lifecycle cancellation shared with streaming.
    pub(super) cancellation: CancellationToken,
    /// Caller-selected strict or degraded live-source failure policy.
    pub(super) freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Query identity bound into private Scribe-tail tickets.
    pub(super) query_id: uuid::Uuid,
    /// Server-owned domain-separated ticket signer.
    pub(super) ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Authoritative role membership used for each query discovery cut.
    pub(super) cluster: Option<Arc<crate::cluster::ClusterRegistry>>,
    /// Production query-scoped stream resolver; absent only in unit fixtures.
    pub(super) discovery: Option<Arc<dyn TailStreamDiscovery>>,
}

/// One metadata-only acquired fence retained until its post-audit drain.
pub(super) struct AcquiredTailFence {
    /// Canonical table receiving drained batches.
    pub(super) table: String,
    /// Transport that owns the retained interval.
    pub(super) transport: Arc<dyn TailReadTransport>,
    /// Immutable fence metadata returned by Scribe.
    pub(super) fence: wyrd_spec::vala::api::TailReadFence,
    /// Reusable exact-fence capability returned by the tail authority.
    pub(super) capability: Vec<u8>,
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
    /// Single-use signed ticket for this exact query/table/node/epoch scope.
    pub(super) ticket: Vec<u8>,
}

/// Bounded live rows and their parent-governor reservations.
#[derive(Default)]
pub(super) struct DrainedTails {
    /// Per-table shallow Arrow batches.
    pub(super) batches: HashMap<String, Vec<RecordBatch>>,
    /// Reservations retained until the final query stream drops.
    pub(super) reservations: Vec<AccountedMemoryReservation>,
    /// Exact Scribe follower sources replacing leader-local tail materialization.
    pub(super) follower_sources: Vec<ScribeFollowerSource>,
    /// Whether one requested live source was unavailable.
    pub(super) degraded: bool,
}

/// One authenticated Scribe source selected from the immutable live-tail cut.
pub(super) struct ScribeFollowerSource {
    /// Canonical table receiving this disjoint live source.
    pub(super) table: String,
    /// Exact Scribe participant selected by the acquired stream fence.
    pub(super) node_id: wyrd_spec::vala::api::NodeId,
    /// Follower assignment consumed by the existing `FetchLiveTailService`.
    pub(super) assignment: wyrd_spec::vala::api::FollowerScanAssignment,
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

/// Query-scoped dependencies and limits used by [`TailFenceDrainer`].
pub(super) struct TailFenceDrainerConfig {
    /// Query telemetry retaining live-tail memory accounting.
    pub(super) telemetry: Arc<OracleTelemetry>,
    /// Query-local pool charged for every decoded live batch.
    pub(super) query_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// Immutable admission class applied to live-tail memory metrics.
    pub(super) query_class: QueryClass,
    /// Absolute deadline shared by fence acquisition and every page read.
    pub(super) deadline: Instant,
    /// Admission lifecycle cancellation shared with streaming.
    pub(super) cancellation: CancellationToken,
    /// Caller-selected strict or degraded live-source failure policy.
    pub(super) freshness: wyrd_spec::vala::api::FreshnessPolicy,
    /// Query identity bound into private Scribe-tail tickets.
    pub(super) query_id: uuid::Uuid,
    /// Server-owned domain-separated ticket signer.
    pub(super) ticket_minter: Option<Arc<dyn crate::scribe::tail_rpc::TailTicketMinter>>,
    /// Authoritative role membership used for each query discovery cut.
    pub(super) cluster: Option<Arc<crate::cluster::ClusterRegistry>>,
    /// Production query-scoped stream resolver; absent only in unit fixtures.
    pub(super) discovery: Option<Arc<dyn TailStreamDiscovery>>,
}

impl TailFenceDrainer<'_> {
    /// Returns the currently live Scribe node identities for route fencing.
    fn live_scribe_nodes(&self) -> Option<std::collections::HashSet<uuid::Uuid>> {
        self.cluster.as_ref().and_then(|cluster| {
            let nodes = cluster
                .snapshot()
                .live_scribes()
                .into_iter()
                .map(|lease| lease.key.node_id.as_uuid())
                .collect::<std::collections::HashSet<_>>();
            (!nodes.is_empty()).then_some(nodes)
        })
    }

    /// Converts a pinned catalog binding to the private discovery wire shape.
    ///
    /// # Errors
    /// Returns visibility unavailable when the catalog namespace is malformed.
    fn wire_binding(
        cut: &PinnedSealedTable,
    ) -> Result<wyrd_spec::vala::api::TenantTableBinding, BifrostError> {
        Ok(wyrd_spec::vala::api::TenantTableBinding {
            tenant_id: cut.binding.tenant,
            namespace: cut
                .binding
                .logical_namespace
                .strip_prefix("vala.")
                .ok_or(BifrostError::QueryVisibilityUnavailable)?
                .to_owned(),
            table: cut.binding.table_name.clone(),
        })
    }

    /// Mints one query-scoped ticket for an exact Scribe stream.
    ///
    /// # Errors
    /// Returns visibility unavailable when the configured signer rejects the
    /// claims. Unit fixtures without a signer use an empty ticket.
    fn mint_tail_ticket(
        &self,
        tenant_id: wyrd_spec::DataTenantId,
        canonical_table: &str,
        node_id: uuid::Uuid,
        writer_epoch: u64,
        deadline: chrono::DateTime<chrono::Utc>,
        audience: crate::scribe::tail_rpc::TailTicketAudience,
    ) -> Result<Vec<u8>, BifrostError> {
        self.ticket_minter
            .as_ref()
            .map(|minter| {
                minter.mint_tail_ticket(&crate::scribe::tail_rpc::TailTicketClaims {
                    query_id: self.query_id,
                    tenant_id,
                    canonical_table: canonical_table.to_owned(),
                    node_id,
                    writer_epoch,
                    deadline,
                    audience,
                    nonce: uuid::Uuid::now_v7().as_bytes().to_vec(),
                })
            })
            .transpose()
            .map_err(|_| BifrostError::QueryVisibilityUnavailable)
            .map(Option::unwrap_or_default)
    }

    /// Creates one query-attempt owner for live-tail acquisition and draining.
    #[must_use]
    pub(super) fn new<'a>(
        tails: &'a TailTransportDirectory,
        memory: &'a OracleMemoryResources,
        config: TailFenceDrainerConfig,
    ) -> TailFenceDrainer<'a> {
        TailFenceDrainer {
            tails,
            memory,
            query_pool: config.query_pool,
            telemetry: config.telemetry,
            query_class: config.query_class,
            deadline: config.deadline,
            cancellation: config.cancellation,
            freshness: config.freshness,
            query_id: config.query_id,
            ticket_minter: config.ticket_minter,
            cluster: config.cluster,
            discovery: config.discovery,
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
        if let Some(cluster) = &self.cluster {
            cluster
                .refresh_snapshot()
                .await
                .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
        }
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let wire_deadline = chrono::Utc::now()
            + chrono::Duration::from_std(remaining).map_err(|_| BifrostError::QueryTimeout)?;
        let work = if let Some(discovery) = &self.discovery {
            self.plan_discovered_acquisitions(cuts, wire_deadline, discovery)
                .await?
        } else {
            self.plan_acquisitions(cuts, wire_deadline)?
        };
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
        let live_nodes = self.live_scribe_nodes();
        for cut in cuts {
            let table = cut.binding.table_ref.fqn();
            let mut streams = std::collections::BTreeMap::<
                (uuid::Uuid, wyrd_spec::vala::api::TimePartitionWire, u64),
                (
                    wyrd_spec::vala::api::TimePartitionWire,
                    wyrd_spec::vala::api::TailCursor,
                    Arc<dyn TailReadTransport>,
                ),
            >::new();
            for file in &cut.sealed_manifest {
                let time_partition = hot_row_partition(file)?;
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
                let key = (file.node_id, time_partition, writer_epoch);
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
                    .or_insert((time_partition, cursor, transport));
            }
            for route in self.tails.live_streams(&table, cut.binding.tenant) {
                if live_nodes
                    .as_ref()
                    .is_some_and(|nodes| !nodes.contains(&route.node_id))
                {
                    continue;
                }
                let key = (route.node_id, route.time_partition, route.writer_epoch);
                streams.entry(key).or_insert_with(|| {
                    (
                        route.time_partition,
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
            for ((stream_node_id, _, _), (time_partition, exclusive_sealed, transport)) in streams {
                let request = tail_fence_request(
                    cut,
                    time_partition,
                    exclusive_sealed,
                    wire_deadline,
                    self.query_id,
                )?;
                let ticket = self.mint_tail_ticket(
                    cut.binding.tenant,
                    &table,
                    stream_node_id,
                    request.exclusive_sealed.writer_epoch,
                    wire_deadline,
                    crate::scribe::tail_rpc::TailTicketAudience::Acquire,
                )?;
                work.push(TailFenceAcquisition {
                    table: table.clone(),
                    transport,
                    request,
                    ticket,
                });
            }
        }
        Ok(work)
    }

    /// Builds acquisitions from the fresh query-scoped discovery result.
    async fn plan_discovered_acquisitions(
        &self,
        cuts: &[PinnedSealedTable],
        wire_deadline: chrono::DateTime<chrono::Utc>,
        discovery: &Arc<dyn TailStreamDiscovery>,
    ) -> Result<Vec<TailFenceAcquisition>, BifrostError> {
        let mut work = Vec::new();
        for cut in cuts {
            let binding = Self::wire_binding(cut)?;
            let routes = match discovery
                .discover(&binding, self.query_id, self.deadline)
                .await
            {
                Ok(routes) => routes,
                Err(crate::scribe::tail_rpc::TailReadError::State { detail })
                    if detail.contains("stale") || detail.contains("epoch") =>
                {
                    // A lease can rotate between the registry cut and the private
                    // list RPC.  Allow exactly one fresh resolver cut; never
                    // silently degrade to an older directory route.
                    match discovery
                        .discover(&binding, self.query_id, self.deadline)
                        .await
                    {
                        Ok(routes) => routes,
                        Err(error) => {
                            tracing::warn!(
                                query_id = %self.query_id,
                                table = %binding.table,
                                error = ?error,
                                "query-scoped Scribe tail rediscovery failed"
                            );
                            return Err(BifrostError::QueryVisibilityUnavailable);
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        query_id = %self.query_id,
                        table = %binding.table,
                        error = ?error,
                        "query-scoped Scribe tail discovery failed"
                    );
                    return Err(BifrostError::QueryVisibilityUnavailable);
                }
            };
            for route in routes {
                let exclusive = sealed_watermark(
                    &cut.sealed_manifest,
                    route.stream.node_id.as_uuid(),
                    route.stream.writer_epoch,
                    route.time_partition,
                )?;
                let request = tail_fence_request(
                    cut,
                    route.time_partition,
                    exclusive,
                    wire_deadline,
                    self.query_id,
                )?;
                let ticket = self.mint_tail_ticket(
                    cut.binding.tenant,
                    &cut.binding.table_ref.fqn(),
                    route.stream.node_id.as_uuid(),
                    route.stream.writer_epoch,
                    wire_deadline,
                    crate::scribe::tail_rpc::TailTicketAudience::Acquire,
                )?;
                work.push(TailFenceAcquisition {
                    table: cut.binding.table_ref.fqn(),
                    transport: route.transport,
                    request,
                    ticket,
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
            ticket,
        } = acquisition;
        let remaining = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or(BifrostError::QueryTimeout)?;
        let acquisition = tokio::select! {
            () = self.cancellation.cancelled() => {
                return Err(BifrostError::QueryExecutionFailed);
            }
            result = tokio::time::timeout(remaining, transport.acquire_fence_with_capability(request, ticket)) => result,
        };
        match acquisition {
            Ok(Ok(lease)) => {
                let fence = lease.fence;
                let capability = lease.capability;
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
                    capability,
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
                    acquired.transport.read_page_with_capability(
                        wyrd_spec::vala::api::TailPageRequest {
                            query_id: self.query_id,
                            fence_id: acquired.fence.fence_id,
                            after: after.clone(),
                            max_rows: 4_096,
                            max_encoded_bytes: 16 * 1024 * 1024,
                        },
                        acquired.capability.clone(),
                    ),
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
                let Ok(reservation) = self.memory.resources.try_split_query_memory(
                    &self.query_pool,
                    "oracle-live-tail",
                    batch.get_array_memory_size(),
                ) else {
                    self.release_one(&mut acquired).await;
                    return Err((table, BifrostError::QueryVisibilityUnavailable));
                };
                reservations.push(self.telemetry.account_query_memory(
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
            acquired.transport.release_fence_with_capability(
                self.query_id,
                acquired.fence.fence_id,
                acquired.capability.clone(),
            ),
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
    time_partition: wyrd_spec::vala::api::TimePartitionWire,
    exclusive_sealed: wyrd_spec::vala::api::TailCursor,
    deadline: chrono::DateTime<chrono::Utc>,
    query_id: uuid::Uuid,
) -> Result<wyrd_spec::vala::api::AcquireTailFenceRequest, BifrostError> {
    let arrow_schema =
        iceberg::arrow::schema_to_arrow_schema(cut.iceberg_table.metadata().current_schema())
            .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    let normalized_fields = arrow_schema
        .fields()
        .iter()
        .map(|field| {
            let data_type = match field.data_type() {
                DataType::Timestamp(unit, Some(timezone)) if timezone.as_ref() == "+00:00" => {
                    DataType::Timestamp(*unit, Some("UTC".into()))
                }
                data_type => data_type.clone(),
            };
            Field::new(field.name(), data_type, field.is_nullable())
        })
        .collect::<Vec<_>>();
    let normalized_schema = Schema::new(normalized_fields);
    let fingerprint = crate::contracts::projected_source_schema_fingerprint(&normalized_schema);
    let fingerprint = wyrd_spec::vala::api::SchemaFingerprint::new(hex::encode(fingerprint.0))
        .map_err(|_| BifrostError::QueryVisibilityUnavailable)?;
    Ok(wyrd_spec::vala::api::AcquireTailFenceRequest {
        query_id,
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
        time_partition,
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

    /// Proves Iceberg-represented sealed rows still fence the exclusive live interval.
    #[test]
    fn sealed_watermark_makes_live_tail_disjoint() {
        let node_id = uuid::Uuid::now_v7();
        let operation = uuid::Uuid::now_v7();
        let row = vala_sql::row_types::file_list::HotFileRow {
            id: uuid::Uuid::now_v7(),
            data_tenant_id: uuid::Uuid::now_v7(),
            namespace: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
            file_path: "events/sealed.parquet".to_owned(),
            file_ordinal: 0,
            file_checksum: None,
            file_size: 1,
            row_count: 1,
            min_event_time: None,
            max_event_time: None,
            partition_granularity: "day".to_owned(),
            partition_start: crate::test_support::day_partition(2026, 8, 12).start_utc(),
            compacted: true,
            committed_snapshot_id: Some(91),
            forge_publication_operation_id: Some(operation),
            node_id,
            writer_epoch: 4,
            wal_lsn_min: 10,
            wal_lsn_max: 20,
            created_at: chrono::Utc::now(),
        };

        let cursor = sealed_watermark(
            &[row],
            node_id,
            4,
            crate::test_support::day_partition(2026, 8, 12).to_wire(),
        )
        .expect("represented sealed lineage must produce a live-tail watermark");

        assert_eq!(cursor.writer_epoch, 4);
        assert_eq!(cursor.wal_lsn, 20);
        assert_eq!(cursor.batch_id, uuid::Uuid::from_u128(u128::MAX));
        assert_eq!(cursor.row_ordinal, u32::MAX);
    }

    /// Hourly sealed subtraction is exact: no row is scanned twice and none is
    /// skipped, and a partition of the wrong granularity contributes nothing.
    ///
    /// The live cut is `(watermark, ...]` over the sealed manifest, so the
    /// watermark must be the maximum represented LSN of exactly the rows in
    /// the requested partition. Three hours of sealed rows prove there is no
    /// bleed across adjacent hours, and a same-instant `Day` partition proves
    /// granularity participates in identity rather than only the start.
    #[test]
    fn hourly_tail_cut_has_no_overlap_or_omission() {
        let node_id = uuid::Uuid::now_v7();
        let tenant = uuid::Uuid::now_v7();
        let sealed_row = |partition: crate::catalog::layout::TimePartition,
                          wal_lsn_min: i64,
                          wal_lsn_max: i64| {
            vala_sql::row_types::file_list::HotFileRow {
                id: uuid::Uuid::now_v7(),
                data_tenant_id: tenant,
                namespace: "vala.bifrost".to_owned(),
                table_name: "events".to_owned(),
                file_path: format!("events/sealed-{wal_lsn_max}.parquet"),
                file_ordinal: 0,
                file_checksum: None,
                file_size: 1,
                row_count: 1,
                min_event_time: None,
                max_event_time: None,
                partition_granularity: partition.granularity_str().to_owned(),
                partition_start: partition.start_utc(),
                compacted: true,
                committed_snapshot_id: Some(91),
                forge_publication_operation_id: Some(uuid::Uuid::now_v7()),
                node_id,
                writer_epoch: 4,
                wal_lsn_min,
                wal_lsn_max,
                created_at: chrono::Utc::now(),
            }
        };

        let thirteen = crate::test_support::hour_partition(2026, 8, 12, 13);
        let fourteen = crate::test_support::hour_partition(2026, 8, 12, 14);
        let fifteen = crate::test_support::hour_partition(2026, 8, 12, 15);
        let manifest = vec![
            sealed_row(thirteen, 1, 10),
            sealed_row(fourteen, 11, 20),
            sealed_row(fourteen, 21, 30),
            sealed_row(fifteen, 31, 40),
        ];

        // Each hour's watermark is its own maximum represented LSN: the
        // preceding hour does not raise it (no omission) and the following
        // hour does not lower it (no overlap).
        for (partition, expected_lsn) in [(thirteen, 10), (fourteen, 30), (fifteen, 40)] {
            let cursor = sealed_watermark(&manifest, node_id, 4, partition.to_wire())
                .expect("hourly sealed lineage produces a watermark");
            assert_eq!(
                cursor.wal_lsn,
                expected_lsn,
                "hour {} must fence exactly its own represented LSNs",
                partition.start_utc()
            );
            assert_eq!(cursor.writer_epoch, 4);
        }

        // The union of the per-hour intervals covers the whole manifest
        // exactly once: consecutive watermarks are strictly increasing and the
        // final one represents every sealed row.
        let watermarks = [thirteen, fourteen, fifteen]
            .into_iter()
            .map(|partition| {
                sealed_watermark(&manifest, node_id, 4, partition.to_wire())
                    .expect("hourly watermark")
                    .wal_lsn
            })
            .collect::<Vec<_>>();
        assert!(
            watermarks.windows(2).all(|pair| pair[0] < pair[1]),
            "hourly watermarks must be strictly increasing"
        );
        assert_eq!(
            watermarks.last().copied(),
            manifest
                .iter()
                .map(|row| u64::try_from(row.wal_lsn_max).expect("fixture LSN"))
                .max(),
            "the final hour must represent every sealed row"
        );

        // A Day partition starting at the same instant as an Hour partition is
        // a different identity, so it selects nothing and the live cut opens
        // at the start of the stream rather than silently reusing hourly rows.
        let day = crate::test_support::day_partition(2026, 8, 12);
        let wrong_granularity = sealed_watermark(&manifest, node_id, 4, day.to_wire())
            .expect("a granularity mismatch is empty, not an error");
        assert_eq!(wrong_granularity.wal_lsn, 0);
        assert_eq!(wrong_granularity.batch_id, uuid::Uuid::nil());
        assert_eq!(wrong_granularity.row_ordinal, 0);

        // A different writer epoch is likewise a different lineage.
        let other_epoch = sealed_watermark(&manifest, node_id, 5, fourteen.to_wire())
            .expect("an epoch mismatch is empty, not an error");
        assert_eq!(other_epoch.wal_lsn, 0);
    }

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
            TailFenceDrainerConfig {
                telemetry: Arc::new(OracleTelemetry::new(Arc::new(OracleSlotManager::new(1, 1)))),
                query_pool: crate::resources::bounded_memory_pool(1024 * 1024 * 1024),
                query_class: QueryClass::Interactive,
                deadline: Instant::now() + Duration::from_secs(1),
                cancellation: CancellationToken::new(),
                freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
                query_id: uuid::Uuid::nil(),
                ticket_minter: None,
                cluster: None,
                discovery: None,
            },
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
                time_partition: crate::test_support::day_partition(2026, 8, 2).to_wire(),
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
            capability: Vec::new(),
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
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
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
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
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
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
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
            resources: crate::resources::BifrostRuntimeResources::composed_for_test(
                1024 * 1024 * 1024,
                1024 * 1024 * 1024,
                [crate::resources::BifrostRole::Oracle],
            )
            .oracle()
            .expect("composition must enable the Oracle capability"),
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
