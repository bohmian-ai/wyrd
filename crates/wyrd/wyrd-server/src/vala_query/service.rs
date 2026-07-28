//! Typed LogicalPlan builders for each ValaQueryService method (M-07).
//!
//! Each function:
//!   1. Resolves the domain table provider(s) from the bifrost catalog.
//!   2. Registers them in a temporary session context to get a schema-aware DataFrame.
//!   3. Applies typed filters as bound `Expr` literals — never string interpolation.
//!   4. Projects away sensitive payload columns when the caller lacks the payload permission.
//!   5. Returns the `LogicalPlan` (pre-analysis). The `TenantPredicateRule` analyzer
//!      injects the tenant predicate during `run_plan_query`'s `execute_logical_plan` call.
//!
//! Note on trace-summary methods (QueryTraces, QueryRecentTraces): these return spans
//! plans filtered but NOT aggregated. The route handler performs the in-Rust GROUP-BY
//! aggregation after collecting batches. Stage 5 will push aggregation into the plan.
//!
//! Physical→API schema mapping:
//!   - Trace IDs are converted to fixed-binary literals for exact plan filtering;
//!     span IDs remain extraction-only fields.
//!   - String column name differences (start_time vs started_at, service_name vs service)
//!     are resolved in the extraction layer in routes.rs.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use datafusion::common::TableReference;
use datafusion::datasource::MemTable;
use datafusion::logical_expr::LogicalPlan;
use datafusion::prelude::{DataFrame, SessionContext, col, lit};
use datafusion::scalar::ScalarValue;
use uuid::Uuid;
use vala_bifrost::BifrostNamespace;
use vala_bifrost::error::BifrostError as EngineBifrostError;
use vala_bifrost::session::wyrd_session_context;
use vala_bifrost_redux::catalog::{
    BifrostCatalogError as ReduxCatalogError, TableRef as ReduxTableRef, TenantTableBinding,
};
use vala_bifrost_redux::namespaces::BifrostNamespace as ReduxNamespace;
use vala_bifrost_redux::provider::ReduxTableProvider;
use vala_bifrost_redux::scribe::seal_key::EventDay;
use vala_bifrost_redux::scribe::stream_identity::StreamIdentity;
use vala_bifrost_redux::scribe::tail_rpc::{FetchLiveTailRequest, FetchLiveTailService};
use vala_bifrost_redux::scribe::wal::WalLsn;
use vala_bifrost_redux::tables::builtin_table;
use wyrd_runtime::{Action, Permission, Resource};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    GetTraceRequest, MAX_QUERY_PAGE_SIZE, QueryAgentTracesRequest, QueryDriftRequest,
    QueryEvalRequest, QueryGenAiRequest, QueryLogsRequest, QueryMetricsRequest,
    QueryRecentTracesRequest, QueryTracesRequest,
};
use wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME;

use crate::AppState;
use crate::components::auth::Caller;

/// Default time window for `GetTrace` and listing queries (F-08: no unbounded scans).
pub(crate) const DEFAULT_WINDOW_DAYS: i64 = 7;

/// One durable staging row included in Oracle's table publication token.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
struct OracleFilePublication {
    /// Stable `vala.file_list` row identity.
    id: Uuid,
    /// Scribe node that owns the row's WAL sequence.
    node_id: Uuid,
    /// Scribe boot epoch that scopes the row's WAL sequence.
    writer_epoch: i64,
    /// First WAL position represented by the staged file.
    wal_lsn_min: i64,
    /// Last WAL position represented by the staged file.
    wal_lsn_max: i64,
    /// Whether Forge has claimed the row for compaction.
    compacted: bool,
    /// Iceberg snapshot that has published the row, when SQL stamping completed.
    committed_snapshot_id: Option<i64>,
}

/// One ordered Forge audit transition used to classify unresolved cutovers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
struct ForgeOperationTransition {
    /// Stable compaction operation shared by prepared and terminal transitions.
    operation_id: Uuid,
    /// Whether this row is a prepared transition instead of a terminal transition.
    is_prepared: bool,
}

/// Result of resolving ordered publication rows without assuming adjacent LSNs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OrderedPublication {
    /// Highest published WAL position for the queried pod-local stream.
    cutoff: WalLsn,
    /// Whether a partial cutover, invalid row, or published-after-unpublished gap exists.
    unresolved: bool,
}

/// Coherence token spanning Oracle's Iceberg-provider and hot-tail load.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OracleFence {
    /// Live-tail exclusion point derived from the local stream's published prefix.
    cutoff: WalLsn,
    /// Complete ordered table publication state observed in one SQL snapshot.
    publication_rows: Vec<OracleFilePublication>,
    /// Whether the ordered rows contain a publication gap or partial state.
    unresolved_publication: bool,
    /// Prepared Forge operations whose latest durable transition is not terminal.
    unresolved_operations: Vec<Uuid>,
}

impl OracleFence {
    /// Derive one fence token from ordered SQL rows and Forge transitions.
    ///
    /// The cutoff is local to `stream`, while publication-gap validation covers
    /// every stream in the physical table. Numeric adjacency is intentionally
    /// ignored because one WAL stream can interleave multiple tables and shards.
    #[must_use]
    fn derive(
        publication_rows: Vec<OracleFilePublication>,
        transitions: &[ForgeOperationTransition],
        stream: StreamIdentity,
    ) -> Self {
        let publication = ordered_publication(&publication_rows, stream);
        Self {
            cutoff: publication.cutoff,
            publication_rows,
            unresolved_publication: publication.unresolved,
            unresolved_operations: unresolved_prepared_operations(transitions),
        }
    }

    /// Reject a token that cannot prove one complete Iceberg-plus-hot snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] for a publication gap, a partial SQL
    /// stamp, or a prepared Forge operation without a matching latest terminal
    /// transition.
    fn require_resolved(&self) -> Result<(), WyrdError> {
        if !self.unresolved_publication && self.unresolved_operations.is_empty() {
            return Ok(());
        }
        Err(WyrdError::Internal {
            message: "Oracle snapshot is awaiting Forge publication".to_owned(),
            details: serde_json::json!({
                "unresolved_publication": self.unresolved_publication,
                "unresolved_operations": self.unresolved_operations,
            }),
        })
    }

    /// Require the post-provider token to match this pre-provider token exactly.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when any table publication row, cutoff,
    /// or unresolved-operation classification changed while the provider loaded.
    fn require_unchanged(&self, after: &Self) -> Result<(), WyrdError> {
        if self == after {
            return Ok(());
        }
        Err(WyrdError::Internal {
            message: "Oracle snapshot and publication fence changed during provider load"
                .to_owned(),
            details: serde_json::json!({
                "before_cutoff": self.cutoff.as_u64(),
                "after_cutoff": after.cutoff.as_u64(),
            }),
        })
    }
}

/// Owns one table-scoped Oracle snapshot workflow and its live Scribe reader.
struct OracleSnapshot<'a> {
    /// Server state containing SQL, Redux catalog, and Scribe dependencies.
    state: &'a AppState,
    /// Authenticated tenant whose physical table is being queried.
    tenant: DataTenantId,
    /// Tenant-qualified physical table identity used by SQL and Scribe.
    binding: TenantTableBinding,
    /// Logical Redux table used to load the Iceberg provider.
    table: ReduxTableRef,
    /// Pod-local Scribe reader whose stream defines the live-tail cutoff.
    tail: Option<FetchLiveTailService>,
    /// First event day included in the bounded hot snapshot.
    start_day: EventDay,
    /// Last event day included in the bounded hot snapshot.
    end_day: EventDay,
}

impl<'a> OracleSnapshot<'a> {
    /// Build the snapshot owner for one authenticated table and bounded window.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the physical binding is invalid or
    /// the configured Scribe cannot construct its pod-local tail service.
    fn new(
        state: &'a AppState,
        table: &ReduxTableRef,
        tenant: DataTenantId,
        since: Option<DateTime<Utc>>,
        until: Option<DateTime<Utc>>,
    ) -> Result<Self, WyrdError> {
        let binding = TenantTableBinding::resolve((tenant, table.clone())).map_err(|error| {
            WyrdError::Internal {
                message: "invalid tenant table binding".to_owned(),
                details: serde_json::json!({ "detail": error.to_string() }),
            }
        })?;
        let tail = state
            .bifrost_ingest
            .as_ref()
            .map(|runtime| runtime.scribe().tail_service())
            .transpose()
            .map_err(|error| WyrdError::Internal {
                message: "Scribe hot-read service unavailable".to_owned(),
                details: serde_json::json!({ "detail": error.to_string() }),
            })?;
        let now = Utc::now();
        let start = since.unwrap_or_else(|| now - Duration::days(DEFAULT_WINDOW_DAYS));
        let end = until.unwrap_or(now);
        Ok(Self {
            state,
            tenant,
            binding,
            table: table.clone(),
            tail,
            start_day: EventDay::from_timestamp(start),
            end_day: EventDay::from_timestamp(end),
        })
    }

    /// Load the Redux Iceberg provider and hot tail under one stable fence.
    ///
    /// The method reads an actual SQL token before loading the hot batches and
    /// Iceberg provider, then reads the token again. Either unresolved token or
    /// any token change fails closed. Once the provider is built and the token
    /// is stable, later commits are outside this query snapshot and are safe.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when SQL, Scribe, catalog, provider
    /// construction, or fence validation fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation can stop the read between stages. No durable state changes,
    /// and the read-only SQL transaction rolls back when dropped.
    async fn load_provider(&self) -> Result<Option<ReduxTableProvider>, WyrdError> {
        let before = self.load_fence().await?;
        if let Some(fence) = &before {
            fence.require_resolved()?;
        }
        let cutoff = before.as_ref().map_or(WalLsn::ZERO, |fence| fence.cutoff);
        let hot_batches = self.fetch_hot_batches_after(cutoff).await?;
        let catalog = self
            .state
            .bifrost_redux
            .as_deref()
            .ok_or_else(|| WyrdError::Internal {
                message: "Redux Bifrost catalog is not configured".to_owned(),
                details: serde_json::Value::Null,
            })?;
        let provider = match catalog
            .provider_with_hot_batches(&self.table, self.tenant, hot_batches)
            .await
        {
            Ok(provider) => Some(provider),
            Err(ReduxCatalogError::TableNotFound(_)) => None,
            Err(error) => {
                return Err(WyrdError::Internal {
                    message: "bifrost provider error".to_owned(),
                    details: serde_json::json!({ "detail": error.to_string() }),
                });
            }
        };
        let after = self.load_fence().await?;
        if let Some(fence) = &after {
            fence.require_resolved()?;
        }
        match (&before, &after) {
            (Some(before), Some(after)) => before.require_unchanged(after)?,
            (None, None) => {}
            _ => {
                return Err(WyrdError::Internal {
                    message: "Oracle Scribe availability changed during provider load".to_owned(),
                    details: serde_json::Value::Null,
                });
            }
        }
        Ok(provider)
    }

    /// Read one RLS-bound statement snapshot of publication and Forge state.
    ///
    /// Both ordered aggregates are scalar subqueries in the same SQL statement,
    /// so PostgreSQL's READ COMMITTED semantics expose one MVCC snapshot without
    /// changing the transaction mode after tenant binding.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the tenant connection cannot be
    /// acquired, SQL rows cannot be decoded, or commit fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the read-only transaction without durable effects.
    async fn load_fence(&self) -> Result<Option<OracleFence>, WyrdError> {
        let Some(tail) = &self.tail else {
            return Ok(None);
        };
        let mut conn = self
            .state
            .postgres
            .vala()
            .tenant_conn(self.tenant)
            .await
            .map_err(oracle_fence_error)?;
        let resource = format!(
            "bifrost://{}/{}/{}",
            self.tenant, self.binding.logical_namespace, self.binding.table_name
        );
        let (sqlx::types::Json(publication_rows), sqlx::types::Json(transitions)): (
            sqlx::types::Json<Vec<OracleFilePublication>>,
            sqlx::types::Json<Vec<ForgeOperationTransition>>,
        ) = sqlx::query_as(
            "SELECT
                COALESCE(
                    (
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'id', id,
                                'node_id', node_id,
                                'writer_epoch', writer_epoch,
                                'wal_lsn_min', wal_lsn_min,
                                'wal_lsn_max', wal_lsn_max,
                                'compacted', compacted,
                                'committed_snapshot_id', committed_snapshot_id
                            )
                            ORDER BY node_id, writer_epoch, wal_lsn_min, wal_lsn_max, id
                        )
                        FROM vala.file_list
                        WHERE data_tenant_id = $1
                          AND namespace = $2
                          AND table_name = $3
                    ),
                    '[]'::jsonb
                ) AS publication_rows,
                COALESCE(
                    (
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'operation_id', detail::jsonb ->> 'operation_id',
                                'is_prepared', operation = 'forge.file_compact.prepared'
                            )
                            ORDER BY seq
                        )
                        FROM vala.audit_outbox
                        WHERE data_tenant_id = $1
                          AND resource = $4
                          AND operation IN (
                              'forge.file_compact.prepared',
                              'forge.file_compact.committed',
                              'forge.file_compact.recovered',
                              'forge.file_compact.reset'
                          )
                    ),
                    '[]'::jsonb
                ) AS transitions",
        )
        .bind(self.tenant.as_uuid())
        .bind(&self.binding.logical_namespace)
        .bind(&self.binding.table_name)
        .bind(resource)
        .fetch_one(&mut **conn.transaction())
        .await
        .map_err(oracle_fence_error)?;
        conn.commit().await.map_err(oracle_fence_error)?;
        Ok(Some(OracleFence::derive(
            publication_rows,
            &transitions,
            tail.stream(),
        )))
    }

    /// Fetch direct Arrow handles newer than the supplied published cutoff.
    ///
    /// # Errors
    ///
    /// Returns [`WyrdError::Internal`] when the pod-local Scribe snapshot fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation abandons the in-memory snapshot request without durable
    /// effects.
    async fn fetch_hot_batches_after(
        &self,
        cutoff: WalLsn,
    ) -> Result<Vec<arrow::record_batch::RecordBatch>, WyrdError> {
        let Some(tail) = &self.tail else {
            return Ok(Vec::new());
        };
        if self.start_day > self.end_day {
            return Ok(Vec::new());
        }
        let request = FetchLiveTailRequest {
            binding: self.binding.clone(),
            target_stream: tail.stream(),
            start_day: self.start_day,
            end_day: self.end_day,
            after_lsn: cutoff,
            required_columns: Vec::new(),
        };
        tail.fetch_hot_batches(request)
            .await
            .map(|batches| batches.into_iter().map(|batch| batch.rows).collect())
            .map_err(|error| WyrdError::Internal {
                message: "Scribe hot-read failed".to_owned(),
                details: serde_json::json!({ "detail": error.to_string() }),
            })
    }
}

/// Resolve the published prefix for every ordered stream and the local cutoff.
///
/// A normal unpublished tail blocks later rows in only its own stream. Numeric
/// gaps are accepted because WAL positions can belong to other tables or
/// shards. A published row after that barrier, a partial snapshot stamp, or an
/// invalid WAL range marks the table unresolved.
#[must_use]
fn ordered_publication(
    rows: &[OracleFilePublication],
    local_stream: StreamIdentity,
) -> OrderedPublication {
    let mut blocked_by_stream = BTreeMap::<(Uuid, i64), bool>::new();
    let mut cutoff = WalLsn::ZERO;
    let mut unresolved = false;
    for row in rows {
        let stream = (row.node_id, row.writer_epoch);
        let blocked = blocked_by_stream.entry(stream).or_default();
        let valid_lsn = row.wal_lsn_min >= 0
            && row.wal_lsn_max >= row.wal_lsn_min
            && u64::try_from(row.wal_lsn_max).is_ok();
        match (row.compacted, row.committed_snapshot_id) {
            (true, Some(_)) if valid_lsn => {
                if *blocked {
                    unresolved = true;
                } else if row.node_id == local_stream.node_id.as_uuid()
                    && row.writer_epoch == local_stream.writer_epoch.as_i64()
                {
                    let published_lsn = u64::try_from(row.wal_lsn_max)
                        .expect("validated non-negative WAL LSN must fit u64");
                    cutoff = WalLsn::new(cutoff.as_u64().max(published_lsn));
                }
            }
            (false, None) if valid_lsn => {
                // A durable staging row is not in Iceberg yet. Oracle only
                // reads Iceberg plus this pod's live tail, so the row cannot
                // be proven present in the candidate snapshot. Fail closed
                // instead of returning a partial result when the row has
                // already retired from the tail or belongs to another pod.
                *blocked = true;
                unresolved = true;
            }
            _ => {
                *blocked = true;
                unresolved = true;
            }
        }
    }
    OrderedPublication { cutoff, unresolved }
}

/// Return operations whose latest ordered audit transition remains prepared.
#[must_use]
fn unresolved_prepared_operations(transitions: &[ForgeOperationTransition]) -> Vec<Uuid> {
    let mut latest = BTreeMap::new();
    for transition in transitions {
        latest.insert(transition.operation_id, transition.is_prepared);
    }
    latest
        .into_iter()
        .filter_map(|(operation_id, is_prepared)| is_prepared.then_some(operation_id))
        .collect()
}

/// Map one SQL fence failure to the fail-closed Oracle error.
fn oracle_fence_error(error: impl std::fmt::Display) -> WyrdError {
    WyrdError::Internal {
        message: "Oracle publication fence lookup failed".to_owned(),
        details: serde_json::json!({ "detail": error.to_string() }),
    }
}

/// Pure regression tests for ordered publication and fence-token invariants.
#[cfg(test)]
mod oracle_fence_tests {
    use super::{
        ForgeOperationTransition, OracleFence, OracleFilePublication, ordered_publication,
        unresolved_prepared_operations,
    };
    use uuid::Uuid;
    use vala_bifrost_redux::scribe::stream_identity::{NodeId, StreamIdentity, WriterEpoch};
    use vala_bifrost_redux::scribe::wal::WalLsn;

    /// Build one deterministic stream identity for pure fence tests.
    fn stream(node: Uuid, epoch: i64) -> StreamIdentity {
        StreamIdentity::new(NodeId::new(node), WriterEpoch::new(epoch))
    }

    /// Build one publication row with deterministic identity and state.
    fn row(
        id: u128,
        node_id: Uuid,
        epoch: i64,
        min: i64,
        max: i64,
        compacted: bool,
        snapshot: Option<i64>,
    ) -> OracleFilePublication {
        OracleFilePublication {
            id: Uuid::from_u128(id),
            node_id,
            writer_epoch: epoch,
            wal_lsn_min: min,
            wal_lsn_max: max,
            compacted,
            committed_snapshot_id: snapshot,
        }
    }

    /// Published rows advance through numeric WAL gaps until the first unpublished row.
    ///
    /// Any durable uncompacted row marks the table unresolved because this
    /// Oracle path does not load staging Parquet into its provider snapshot.
    #[test]
    fn ordered_prefix_ignores_numeric_adjacency_and_stops_at_unpublished_tail() {
        let node = Uuid::from_u128(1);
        let rows = vec![
            row(1, node, 7, 1, 3, true, Some(10)),
            row(2, node, 7, 9, 12, true, Some(11)),
            row(3, node, 7, 20, 24, false, None),
        ];
        let publication = ordered_publication(&rows, stream(node, 7));
        assert_eq!(publication.cutoff.as_u64(), 12);
        assert!(publication.unresolved);
    }

    /// A published row after an earlier unpublished table row fails closed.
    #[test]
    fn ordered_publication_rejects_published_row_after_gap() {
        let node = Uuid::from_u128(2);
        let rows = vec![
            row(1, node, 9, 1, 3, false, None),
            row(2, node, 9, 10, 14, true, Some(12)),
        ];
        let publication = ordered_publication(&rows, stream(node, 9));
        assert_eq!(publication.cutoff, WalLsn::ZERO);
        assert!(publication.unresolved);
    }

    /// Latest-transition classification treats a retried prepared operation as unresolved.
    #[test]
    fn latest_prepared_transition_is_unresolved_after_an_older_terminal() {
        let operation_id = Uuid::from_u128(20);
        let transitions = [
            ForgeOperationTransition {
                operation_id,
                is_prepared: true,
            },
            ForgeOperationTransition {
                operation_id,
                is_prepared: false,
            },
            ForgeOperationTransition {
                operation_id,
                is_prepared: true,
            },
        ];
        assert_eq!(
            unresolved_prepared_operations(&transitions),
            vec![operation_id]
        );
    }

    /// Any publication-state change produces a different coherence token.
    #[test]
    fn fence_token_change_fails_coherence_validation() {
        let node = Uuid::from_u128(3);
        let before = OracleFence::derive(
            vec![row(1, node, 4, 1, 3, true, Some(10))],
            &[],
            stream(node, 4),
        );
        let after = OracleFence::derive(
            vec![
                row(1, node, 4, 1, 3, true, Some(10)),
                row(2, node, 4, 8, 9, false, None),
            ],
            &[],
            stream(node, 4),
        );
        assert!(before.require_unchanged(&after).is_err());
    }
}

#[derive(Clone, Copy)]
struct TableProviderScope<'a> {
    tenant: wyrd_spec::ids::DataTenantId,
    fqn: &'a str,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
}

/// Ceiling on records collected before pagination (mirrors MAX_QUERY_PAGE_SIZE).
pub(crate) fn effective_limit(requested: Option<u32>) -> u32 {
    requested
        .unwrap_or(MAX_QUERY_PAGE_SIZE)
        .min(MAX_QUERY_PAGE_SIZE)
}

/// Load one Redux provider through the coherent Oracle snapshot workflow.
///
/// This narrow module adapter keeps existing query-service callers from
/// constructing or observing the private [`OracleSnapshot`] owner.
///
/// # Errors
///
/// Returns [`WyrdError::Internal`] when the SQL fence, hot Scribe snapshot,
/// Iceberg provider, or post-load coherence validation fails.
///
/// # Cancellation
///
/// Cancellation abandons read-only SQL and provider work without durable
/// effects.
pub(crate) async fn load_redux_provider_with_fence(
    state: &AppState,
    table: &ReduxTableRef,
    tenant: DataTenantId,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<Option<ReduxTableProvider>, WyrdError> {
    OracleSnapshot::new(state, table, tenant, since, until)?
        .load_provider()
        .await
}

/// Register one coherent Redux domain-table provider in `ctx`.
///
/// A missing physical table receives the built-in empty schema when available;
/// otherwise registration is skipped and DataFusion planning surfaces a
/// referenced-table error.
///
/// # Errors
///
/// Returns [`WyrdError`] when namespace resolution, Oracle snapshot loading, or
/// DataFusion registration fails.
///
/// # Cancellation
///
/// Cancellation abandons read-only SQL, Scribe, Iceberg, and DataFusion setup
/// without durable effects.
async fn attach_redux_table_provider(
    ctx: &SessionContext,
    state: &AppState,
    ns: BifrostNamespace,
    table_name: &str,
    scope: TableProviderScope<'_>,
) -> Result<(), WyrdError> {
    let segment = ns.as_str().strip_prefix("vala.").unwrap_or(ns.as_str());
    let redux_ns =
        ReduxNamespace::from_domain_namespace(segment).ok_or_else(|| WyrdError::Internal {
            message: "unknown Redux Bifrost namespace".to_owned(),
            details: serde_json::json!({ "namespace": ns.as_str() }),
        })?;
    let table = ReduxTableRef::new(redux_ns, table_name);
    match load_redux_provider_with_fence(state, &table, scope.tenant, scope.since, scope.until)
        .await?
    {
        Some(provider) => {
            ctx.register_table(TableReference::bare(scope.fqn), Arc::new(provider))
                .map_err(|e| WyrdError::Internal {
                    message: "failed to register domain table".to_owned(),
                    details: serde_json::json!({ "detail": e.to_string() }),
                })?;
        }
        None => {
            let Some(definition) = builtin_table(segment, table_name) else {
                return Ok(());
            };
            let empty = MemTable::try_new((definition.schema)(), vec![vec![]]).map_err(df_err)?;
            ctx.register_table(TableReference::bare(scope.fqn), Arc::new(empty))
                .map_err(df_err)?;
        }
    }
    Ok(())
}

async fn attach_table_provider(
    ctx: &SessionContext,
    state: &AppState,
    ns: BifrostNamespace,
    table_name: &str,
    scope: TableProviderScope<'_>,
) -> Result<(), WyrdError> {
    if state.bifrost_redux.is_some() {
        return attach_redux_table_provider(ctx, state, ns, table_name, scope).await;
    }

    match state.bifrost.provider(ns, table_name, scope.tenant).await {
        Ok(provider) => {
            ctx.register_table(TableReference::bare(scope.fqn), Arc::new(provider))
                .map_err(|e| WyrdError::Internal {
                    message: "failed to register domain table".to_owned(),
                    details: serde_json::json!({ "detail": e.to_string() }),
                })?;
        }
        Err(EngineBifrostError::TableNotFound(_)) => {}
        Err(e) => {
            return Err(WyrdError::Internal {
                message: "bifrost provider error".to_owned(),
                details: serde_json::json!({ "detail": e.to_string() }),
            });
        }
    }
    Ok(())
}

/// Apply a mandatory `wyrd_event_time` window filter. `since` defaults to
/// `now - DEFAULT_WINDOW_DAYS` when absent (F-08).
fn apply_window(
    df: DataFrame,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
) -> Result<DataFrame, WyrdError> {
    let effective_since = since.unwrap_or_else(|| Utc::now() - Duration::days(DEFAULT_WINDOW_DAYS));
    let ts_since = ScalarValue::TimestampMicrosecond(
        Some(effective_since.timestamp_micros()),
        Some(Arc::from("UTC")),
    );
    let df = df
        .filter(col(WYRD_EVENT_TIME).gt_eq(lit(ts_since)))
        .map_err(df_err)?;
    if let Some(until_ts) = until {
        let ts_until = ScalarValue::TimestampMicrosecond(
            Some(until_ts.timestamp_micros()),
            Some(Arc::from("UTC")),
        );
        return df
            .filter(col(WYRD_EVENT_TIME).lt(lit(ts_until)))
            .map_err(df_err);
    }
    Ok(df)
}

fn df_err(e: datafusion::error::DataFusionError) -> WyrdError {
    WyrdError::Internal {
        message: "plan construction error".to_owned(),
        details: serde_json::json!({ "detail": e.to_string() }),
    }
}

fn opt_filter_str(
    df: DataFrame,
    column: &str,
    value: &Option<String>,
) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).eq(lit(v.clone()))).map_err(df_err),
        None => Ok(df),
    }
}

fn opt_filter_u32(df: DataFrame, column: &str, value: Option<u32>) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).gt_eq(lit(v as i64))).map_err(df_err),
        None => Ok(df),
    }
}

fn opt_filter_i32(df: DataFrame, column: &str, value: Option<i32>) -> Result<DataFrame, WyrdError> {
    match value {
        Some(v) => df.filter(col(column).gt_eq(lit(v))).map_err(df_err),
        None => Ok(df),
    }
}

fn has_perm(caller: &Caller, resource: Resource) -> bool {
    caller
        .principal
        .effective_permissions
        .contains(&Permission {
            resource,
            action: Action::Read,
        })
}

/// Build the `LogicalPlan` for `GetTrace` (spans filtered to one trace_id + window).
/// Payload `attributes` column is projected away unless the caller holds
/// `BifrostTracePayload:Read`.
pub async fn build_get_trace_plan(
    state: &AppState,
    caller: &Caller,
    req: &GetTraceRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        TableProviderScope {
            tenant,
            fqn: "vala.traces.spans",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let trace_id = hex::decode(&req.trace_id).map_err(|error| WyrdError::Validation {
        message: "trace_id must be a hexadecimal value".to_owned(),
        details: serde_json::json!({ "trace_id": req.trace_id, "detail": error.to_string() }),
    })?;
    if trace_id.len() != 16 {
        return Err(WyrdError::Validation {
            message: "trace_id must contain exactly 16 bytes".to_owned(),
            details: serde_json::json!({ "trace_id": req.trace_id, "bytes": trace_id.len() }),
        });
    }
    let df = df
        .filter(col("trace_id").eq(lit(ScalarValue::FixedSizeBinary(16, Some(trace_id)))))
        .map_err(df_err)?;
    let df = if !has_perm(caller, Resource::BifrostTracePayload) {
        df.drop_columns(&["attributes"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryTraces` (filtered spans; aggregation deferred to handler).
pub async fn build_query_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        TableProviderScope {
            tenant,
            fqn: "vala.traces.spans",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // service_name is the physical column; matches req.service filter name
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_str(df, "status", &req.status)?;
    let df = opt_filter_str(df, "name", &req.name)?;
    let df = opt_filter_u32(df, "duration_ms", req.min_duration_ms)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryRecentTraces` (ordered recent spans;
/// handler groups into TraceSummaryRow).
pub async fn build_query_recent_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryRecentTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Traces,
        "spans",
        TableProviderScope {
            tenant,
            fqn: "vala.traces.spans",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.traces.spans"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "service_name", &req.service)?;
    let df = opt_filter_str(df, "status", &req.status)?;
    let df = opt_filter_u32(df, "duration_ms", req.min_duration_ms)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryGenAi`.
/// Payload columns (`input_messages`, `output_messages`) are projected away unless
/// the caller holds `BifrostGenAiPayload:Read`.
pub async fn build_query_genai_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryGenAiRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::GenAi,
        "messages",
        TableProviderScope {
            tenant,
            fqn: "vala.genai.messages",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.genai.messages"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "conversation_id", &req.conversation_id)?;
    let df = opt_filter_str(df, "request_model", &req.model)?;
    let df = opt_filter_str(df, "provider_name", &req.provider)?;
    let df = if !has_perm(caller, Resource::BifrostGenAiPayload) {
        df.drop_columns(&["input_messages", "output_messages"])
            .map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryEval` (uses `vala.eval.assertions` for per-metric rows).
pub async fn build_query_eval_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryEvalRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    // assertions table has assertion_name (metric), score_value (score), eval_ref (eval_id)
    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Eval,
        "assertions",
        TableProviderScope {
            tenant,
            fqn: "vala.eval.assertions",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.eval.assertions"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // eval_ref maps to EvalRow.eval_id in the extraction layer
    let df = opt_filter_str(df, "eval_ref", &req.eval_id)?;
    // run_id is a correlation column appended by the observation policy
    let df = opt_filter_str(df, "run_id", &req.run_id)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryDrift`.
pub async fn build_query_drift_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryDriftRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Drift,
        "observations",
        TableProviderScope {
            tenant,
            fqn: "vala.drift.observations",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.drift.observations"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    // series maps to DriftRow.feature in the extraction layer
    let df = opt_filter_str(df, "series", &req.feature)?;
    // run_id is the correlation column
    let df = opt_filter_str(df, "run_id", &req.run_id)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryMetrics`.
pub async fn build_query_metrics_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryMetricsRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Metrics,
        "points",
        TableProviderScope {
            tenant,
            fqn: "vala.metrics.points",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.metrics.points"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "metric_name", &req.metric_name)?;
    let df = opt_filter_str(df, "metric_type", &req.metric_type)?;

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryLogs`. Payload columns (`body`, `attributes`)
/// are projected away unless the caller holds `BifrostLogPayload:Read`.
pub async fn build_query_logs_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryLogsRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Logs,
        "records",
        TableProviderScope {
            tenant,
            fqn: "vala.logs.records",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.logs.records"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_i32(df, "severity_number", req.severity_number_min)?;
    // trace_id in logs is FixedSizeBinary(16); string filter deferred to Stage 5
    // event_name is Utf8 — safe to filter directly
    let df = opt_filter_str(df, "event_name", &req.event_name)?;
    let df = if !has_perm(caller, Resource::BifrostLogPayload) {
        df.drop_columns(&["body", "attributes"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}

/// Build the `LogicalPlan` for `QueryAgentTraces`. Payload column (`messages`)
/// is projected away unless the caller holds `BifrostAgentTracePayload:Read`.
pub async fn build_query_agent_traces_plan(
    state: &AppState,
    caller: &Caller,
    req: &QueryAgentTracesRequest,
) -> Result<LogicalPlan, WyrdError> {
    let tenant = caller.data_tenant_id;
    let ctx = wyrd_session_context(tenant);

    attach_table_provider(
        &ctx,
        state,
        BifrostNamespace::Dev,
        "agent_traces",
        TableProviderScope {
            tenant,
            fqn: "vala.dev.agent_traces",
            since: req.window.since,
            until: req.window.until,
        },
    )
    .await?;

    let df = ctx
        .table(TableReference::bare("vala.dev.agent_traces"))
        .await
        .map_err(df_err)?;
    let df = apply_window(df, req.window.since, req.window.until)?;
    let df = opt_filter_str(df, "dev_session_id", &req.dev_session_id)?;
    let df = opt_filter_str(df, "repo", &req.repo)?;
    let df = opt_filter_str(df, "commit_sha", &req.commit_sha)?;
    let df = opt_filter_str(df, "branch", &req.branch)?;
    let df = opt_filter_str(df, "run_id", &req.run_id)?;
    let df = if !has_perm(caller, Resource::BifrostAgentTracePayload) {
        df.drop_columns(&["messages", "tool_io"]).map_err(df_err)?
    } else {
        df
    };

    Ok(df.logical_plan().clone())
}
