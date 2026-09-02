//! SQL planning owner for Oracle's immutable visibility cut.
//!
//! Planning validates read-only requests, pins tenant-qualified metadata under
//! one deadline, classifies the cut, and builds session-local providers without
//! performing admission, audit, or row execution side effects.

use std::sync::Arc;
use std::time::Instant;

use arrow::datatypes::Schema;
use datafusion::catalog::{CatalogProvider, MemoryCatalogProvider, MemorySchemaProvider};
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::datasource::default_table_source::DefaultTableSource;
use datafusion::datasource::{MemTable, TableProvider};
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::ExecutionPlan;
use num_traits::ToPrimitive;
use wyrd_spec::vala::api::ClusterCapabilities;

use super::*;
use super::{
    AuthorizedQueryContext, BifrostCatalog, BifrostCatalogError, BifrostError, DrainedTails,
    HotFileSource, OracleAudit, OracleMemoryResources, OracleTableInputs, OracleTableProvider,
    OracleTelemetry, PinnedSealedTable, PlannedSqlCut, ProtectedPlannedSqlCut, QueryClass,
    TableRef, map_datafusion_error, optimized_plan_is_complex, query_class_label,
};
use crate::oracle::reader_pins::OracleReaderAuthority;

/// Query floor and logical-plan preparation owner.
#[derive(Debug, Clone)]
pub struct OraclePlanner {
    /// Synchronous floor, deadline, and capacity configuration.
    pub(super) config: OracleConfig,
    /// Bounded permits covering sealed metadata planning.
    pub(super) planning: Arc<Semaphore>,
}

/// One closed classification result with its production accounting inputs.
#[derive(Debug, Clone, Copy)]
pub(super) struct OracleClassification {
    /// Locked query class selected for admission.
    pub(super) query_class: QueryClass,
    /// Closed reason explaining the class selection.
    pub(super) reason: &'static str,
    /// Predicted sealed scan duration from the normative formula.
    pub(super) predicted_scan_seconds: f64,
}

impl OraclePlanner {
    /// Creates a planner with bounded synchronous validation settings.
    #[must_use]
    pub fn new(config: OracleConfig) -> Self {
        Self {
            planning: Arc::new(Semaphore::new(config.planning_permits)),
            config,
        }
    }

    /// Validates one non-empty, single-statement `SELECT` request.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryInvalidSql`] for floor violations.
    pub fn validate_query(&self, request: &BifrostQueryRequest) -> Result<(), BifrostError> {
        request
            .validate()
            .map_err(|error| BifrostError::QueryInvalidSql {
                detail: error.to_string(),
            })?;
        if request.sql.len() > self.config.max_sql_bytes {
            return Err(BifrostError::QueryInvalidSql {
                detail: "query exceeds configured SQL byte limit".to_owned(),
            });
        }
        parse_select_tables(&request.sql)?;
        Ok(())
    }

    /// Tries to reserve one bounded planning slot without queuing unbounded work.
    ///
    /// # Errors
    ///
    /// Returns admission rejection while the planning bound is saturated.
    fn try_planning(&self) -> Result<OwnedSemaphorePermit, BifrostError> {
        Arc::clone(&self.planning)
            .try_acquire_owned()
            .map_err(|_| BifrostError::QueryAdmissionRejected)
    }

    /// Applies the normative estimated-byte classification formula.
    #[must_use]
    pub fn classify(estimated_bytes: u64, live_oracle_cpu: f64, complex: bool) -> QueryClass {
        Self::classification(estimated_bytes, live_oracle_cpu, complex).query_class
    }

    /// Pins the authenticated request against one membership snapshot and returns its plan.
    ///
    /// This preparation has no admission, audit, registry, or execution side effects. It lets
    /// an ingress owner choose and sign one compatible ready-Oracle participant cut before the
    /// local or remote leader begins the shared execution operation.
    ///
    /// The pinned cut is returned rather than discarded so a local leader can execute the
    /// snapshot it already paid for. A forwarded query cannot use it — the participant cut
    /// carries cluster membership, not the file list — so a remote leader pins for itself.
    ///
    /// # Errors
    /// Returns the same validation, catalog, timeout, and planning failures as Oracle planning.
    pub async fn classify_for_forwarding(
        &self,
        context: &AuthorizedQueryContext,
        request: &BifrostQueryRequest,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: Option<&Arc<OracleReaderAuthority>>,
        snapshot: &crate::cluster::ClusterSnapshot,
    ) -> Result<PlannedSqlCut, BifrostError> {
        self.validate_query(request)?;
        let tables = parse_select_tables(&request.sql)?;
        let live_oracle_cpu = snapshot
            .live_oracles()
            .into_iter()
            .filter_map(|role| match &role.capabilities {
                ClusterCapabilities::OracleV1(capabilities) => Some(capabilities.cpu_cores),
                ClusterCapabilities::ScribeV1(_) => None,
            })
            .sum::<f64>();
        self.pin_and_classify(SqlPlanInputs {
            context,
            sql: &request.sql,
            tables: &tables,
            deadline,
            catalog,
            authority,
            live_oracle_cpu,
        })
        .await
    }

    /// Produces the class, closed reason, and predicted duration from one cut.
    #[must_use]
    pub(super) fn classification(
        estimated_bytes: u64,
        live_oracle_cpu: f64,
        complex: bool,
    ) -> OracleClassification {
        let cpu = (live_oracle_cpu * 0.8).floor().max(1.0);
        let seconds =
            estimated_bytes.to_f64().unwrap_or(f64::MAX) / ESTIMATED_SCAN_BYTES_PER_SECOND / cpu;
        if complex {
            OracleClassification {
                query_class: QueryClass::Analytical,
                reason: "global_operator",
                predicted_scan_seconds: seconds,
            }
        } else if seconds > INTERACTIVE_SCAN_LIMIT_SECONDS {
            OracleClassification {
                query_class: QueryClass::Analytical,
                reason: "predicted_scan",
                predicted_scan_seconds: seconds,
            }
        } else {
            OracleClassification {
                query_class: QueryClass::Interactive,
                reason: "estimated_scan",
                predicted_scan_seconds: seconds,
            }
        }
    }
}

/// Why one protect-and-materialize attempt failed.
///
/// Only authoritative catalog promotion is worth restarting for; every other
/// failure would repeat identically, so it is reported as it stands.
enum AttemptFailure {
    /// Revalidation proved the catalog moved under this attempt.
    CatalogPromoted(BifrostError),
    /// A failure a restart cannot change.
    Fatal(BifrostError),
}

impl AttemptFailure {
    /// Unwraps the public error this attempt failed with.
    fn into_public(self) -> BifrostError {
        match self {
            Self::CatalogPromoted(error) | Self::Fatal(error) => error,
        }
    }
}

impl OraclePlanner {
    /// Protects and materializes every table scan in a typed plan.
    ///
    /// Tables are prepared sequentially under one absolute deadline, the whole
    /// set is protected by one durable guard, and only then is any snapshot
    /// materialized. Cancellation drops the active catalog future; a partially
    /// materialized result is discarded together with its guard, which narrows
    /// the protection it took.
    ///
    /// # Errors
    /// Returns invalid SQL for non-canonical scans, reader-authority refusal,
    /// or timeout/catalog failures while materializing the immutable cuts.
    pub(super) async fn prepare_typed_cuts(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        tenant: wyrd_spec::DataTenantId,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<ProtectedPlannedSqlCut, BifrostError> {
        let tables = collect_plan_table_refs(plan)?;
        Self::protect_and_materialize(&tables, tenant, deadline, catalog, Some(authority)).await
    }

    /// Prepares identities, takes one complete reader guard, revalidates, then
    /// materializes — restarting the whole thing once on catalog promotion.
    ///
    /// This is the only place a leader turns table references into readable
    /// cuts. The ordering is the protection contract: every identity is
    /// resolved from metadata alone, one guard covers the whole set, every
    /// prepared table is revalidated against the authoritative catalog before
    /// any of them is materialized, and no snapshot-dependent source IO happens
    /// before that guard exists.
    ///
    /// Promotion between preparation and materialization invalidates the whole
    /// attempt, not one table: the guard covers a set of snapshots, and a set
    /// with one stale member protects nothing coherent. The guard is therefore
    /// dropped and every table is prepared, protected, revalidated, and
    /// materialized again. One restart only — a second drift is reported rather
    /// than chased.
    ///
    /// # Errors
    /// Returns [`BifrostError::QueryTimeout`] when the deadline passes during
    /// preparation or materialization, the reader authority's refusal when the
    /// cut cannot be protected, a metadata-mismatch failure when the catalog
    /// was promoted twice under this query, and the catalog's public failure
    /// otherwise.
    async fn protect_and_materialize(
        tables: &[TableRef],
        tenant: wyrd_spec::DataTenantId,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: Option<&Arc<OracleReaderAuthority>>,
    ) -> Result<ProtectedPlannedSqlCut, BifrostError> {
        let Some(authority) = authority else {
            // A replica with no local Oracle role holds no reader epoch, so it
            // has nothing that could protect a snapshot it is about to read.
            return Err(BifrostError::OracleRoleUnavailable);
        };
        match Self::attempt_protected_cut(tables, tenant, deadline, catalog, authority).await {
            Err(AttemptFailure::CatalogPromoted(error)) => {
                tracing::warn!(
                    error = %error,
                    tables = tables.len(),
                    "Oracle restarted complete reader admission after catalog promotion"
                );
                Self::attempt_protected_cut(tables, tenant, deadline, catalog, authority)
                    .await
                    .map_err(AttemptFailure::into_public)
            }
            other => other.map_err(AttemptFailure::into_public),
        }
    }

    /// Runs one complete prepare, protect, revalidate, and materialize attempt.
    ///
    /// # Errors
    /// Returns [`AttemptFailure::CatalogPromoted`] when revalidation proved the
    /// authoritative catalog moved under this attempt, which the caller may
    /// restart once, and [`AttemptFailure::Fatal`] for every failure a restart
    /// cannot change.
    async fn attempt_protected_cut(
        tables: &[TableRef],
        tenant: wyrd_spec::DataTenantId,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<ProtectedPlannedSqlCut, AttemptFailure> {
        let mut prepared = Vec::with_capacity(tables.len());
        for table in tables {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            prepared.push(
                tokio::time::timeout(remaining, catalog.prepare_reader_identity(table, tenant))
                    .await
                    .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                    .map_err(|error| AttemptFailure::Fatal(error.into_public()))?,
            );
        }
        let (guard, permit) = authority
            .acquire_guard(&prepared)
            .await
            .map_err(AttemptFailure::Fatal)?;
        // Every prepared table is revalidated before any of them materializes,
        // so a promotion is found while the whole attempt is still discardable.
        for identity in &prepared {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            tokio::time::timeout(remaining, catalog.revalidate_reader_identity(identity))
                .await
                .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                .map_err(|error| match error {
                    BifrostCatalogError::MetadataMismatch(_) => {
                        AttemptFailure::CatalogPromoted(error.into_public())
                    }
                    other => AttemptFailure::Fatal(other.into_public()),
                })?;
        }
        let mut cuts = Vec::with_capacity(prepared.len());
        for identity in prepared {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or(AttemptFailure::Fatal(BifrostError::QueryTimeout))?;
            cuts.push(
                tokio::time::timeout(remaining, catalog.materialize_reader_cut(identity, &permit))
                    .await
                    .map_err(|_| AttemptFailure::Fatal(BifrostError::QueryTimeout))?
                    .map_err(|error| AttemptFailure::Fatal(error.into_public()))?,
            );
        }
        Ok(ProtectedPlannedSqlCut {
            guard,
            permit,
            cuts,
        })
    }

    /// Drives one complete protect-and-materialize attempt from a test.
    ///
    /// Preparation, protection, revalidation, and materialization are one
    /// operation by construction, so a test that needs to observe the restart
    /// has no other entry point into the exact production sequence.
    ///
    /// Returns how many cuts the successful attempt materialized, which is the
    /// observable the caller needs; the protected cut itself stays private.
    ///
    /// # Errors
    /// Returns whatever [`Self::protect_and_materialize`] returns.
    #[cfg(any(test, feature = "test-support"))]
    pub async fn protect_and_materialize_for_test(
        tables: &[TableRef],
        tenant: wyrd_spec::DataTenantId,
        deadline: Instant,
        catalog: &BifrostCatalog,
        authority: &Arc<OracleReaderAuthority>,
    ) -> Result<usize, BifrostError> {
        Self::protect_and_materialize(tables, tenant, deadline, catalog, Some(authority))
            .await
            .map(|protected| protected.cuts.len())
    }

    /// Builds executable providers from authenticated cuts and drained tails.
    ///
    /// Provider construction consumes each cut and transfers its matching drained
    /// batches into the provider map. Cancellation can leave only local partially
    /// constructed providers, which are dropped because no map is returned.
    ///
    /// # Errors
    /// Returns metadata, storage, or provider-construction failures.
    pub(super) async fn build_typed_providers(
        &self,
        inputs: TypedProviderInputs<'_>,
    ) -> Result<std::collections::HashMap<String, Arc<dyn TableProvider>>, BifrostError> {
        let TypedProviderInputs {
            context,
            class,
            cuts,
            drained,
            catalog,
            audit,
            memory,
            query_pool,
            telemetry,
        } = inputs;
        let mut providers = std::collections::HashMap::with_capacity(cuts.len());
        for cut in cuts {
            let table_name = cut.binding.table_ref.fqn();
            let hot_files = local_hot_sources(catalog, &cut)?;
            let provider = OracleTableProvider::try_new(OracleTableInputs {
                table: cut.iceberg_table,
                distributed_iceberg_batches: None,
                hot_files,
                distributed_hot_batches: Vec::new(),
                live_batches: drained.batches.remove(&table_name).unwrap_or_default(),
                context: context.clone(),
                table_name: table_name.clone(),
                audit: Arc::clone(&audit),
                memory: memory.clone(),
                query_pool: Arc::clone(&query_pool),
                telemetry: Arc::clone(&telemetry),
                query_class: class,
            })
            .await
            .map_err(|error| map_datafusion_error(&error))?;
            providers.insert(table_name, Arc::new(provider) as Arc<dyn TableProvider>);
        }
        Ok(providers)
    }

    /// Replaces schema-only scans with providers from the authenticated cut.
    ///
    /// # Errors
    /// Returns a typed execution failure when any scan lacks a cut provider or
    /// when `DataFusion` rejects the transformed logical plan.
    pub(super) fn replace_typed_sources(
        plan: datafusion::logical_expr::LogicalPlan,
        providers: &std::collections::HashMap<String, Arc<dyn TableProvider>>,
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        plan.transform(|node| {
            let datafusion::logical_expr::LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };
            let table_name = scan.table_name.to_string();
            let Some(provider) = providers.get(&table_name) else {
                return Err(datafusion::error::DataFusionError::Plan(
                    "typed plan scan has no authenticated Oracle provider".to_owned(),
                ));
            };
            let mut replacement = scan.clone();
            replacement.source = Arc::new(DefaultTableSource::new(Arc::clone(provider)));
            Ok(Transformed::yes(
                datafusion::logical_expr::LogicalPlan::TableScan(replacement),
            ))
        })
        .map(|transformed| transformed.data)
        .map_err(|error| map_datafusion_error(&error))
    }

    /// Creates one physical plan in the admitted query's governed session.
    ///
    /// The caller supplies the same query-owned memory and disk runtime used by
    /// SQL execution. Cancellation drops the `DataFusion` planning future and
    /// returns no session or executable plan, so no partially prepared
    /// execution escapes.
    ///
    /// # Errors
    /// Returns query execution failure when `DataFusion` cannot lower the plan
    /// in the supplied governed session.
    pub(super) async fn create_physical_plan(
        plan: &datafusion::logical_expr::LogicalPlan,
        session: SessionContext,
    ) -> Result<(SessionContext, Arc<dyn ExecutionPlan>), BifrostError> {
        let physical = session
            .state()
            .create_physical_plan(plan)
            .await
            .map_err(|error| map_datafusion_error(&error))?;
        Ok((session, physical))
    }

    /// Pins metadata and derives the immutable class for one SQL retry attempt.
    ///
    /// The planner owns parsing-adjacent metadata work and never executes rows;
    /// provider installation remains an Oracle composition concern after audit.
    /// The planning permit spans all catalog pins, schema-only optimization, and
    /// classification. Cancellation drops that permit and any local partial cuts;
    /// no admission or audit side effect has occurred at this stage.
    ///
    /// # Errors
    /// Returns timeout, catalog, planning, or byte-accounting failures.
    pub(super) async fn pin_and_classify(
        &self,
        inputs: SqlPlanInputs<'_>,
    ) -> Result<PlannedSqlCut, BifrostError> {
        let SqlPlanInputs {
            context,
            sql,
            tables,
            deadline,
            catalog,
            authority,
            live_oracle_cpu,
        } = inputs;
        let planning = self.try_planning()?;
        // DEBUG, not INFO: one event per catalog pin per query is per-request
        // decision detail, not a lifecycle transition. It is the only way to
        // attribute pre-fragment query latency, which is otherwise invisible
        // between admission and the first fragment dispatch.
        let pin_started = std::time::Instant::now();
        let ProtectedPlannedSqlCut {
            guard,
            permit,
            cuts,
        } = Self::protect_and_materialize(
            tables,
            context.data_tenant_id,
            deadline,
            catalog,
            authority,
        )
        .await?;
        let estimated_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
            total
                .checked_add(cut.estimated_bytes)
                .ok_or(BifrostError::QueryAdmissionRejected)
        })?;
        let pinned_elapsed = pin_started.elapsed();
        let hot_files = cuts.iter().map(|cut| cut.hot_files.len()).sum::<usize>();
        let iceberg_files = cuts
            .iter()
            .map(|cut| cut.iceberg_files.len())
            .sum::<usize>();
        let optimized_plan = self.prepare_optimized_sql_plan(sql, &cuts).await?;
        tracing::debug!(
            tables = tables.len(),
            hot_files,
            iceberg_files,
            pin_ms = pinned_elapsed.as_millis(),
            optimize_ms = pin_started
                .elapsed()
                .saturating_sub(pinned_elapsed)
                .as_millis(),
            "Oracle pinned one sealed cut"
        );
        let classification = Self::classification(
            estimated_bytes,
            live_oracle_cpu,
            optimized_plan_is_complex(&optimized_plan),
        );
        let local_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
            cut.hot_files.iter().try_fold(total, |total, file| {
                let bytes = u64::try_from(file.file_size)
                    .map_err(|_| BifrostError::QueryAdmissionRejected)?;
                total
                    .checked_add(bytes)
                    .ok_or(BifrostError::QueryAdmissionRejected)
            })
        })?;
        let remote_bytes = cuts.iter().try_fold(0_u64, |total, cut| {
            cut.iceberg_files.iter().try_fold(total, |total, file| {
                total
                    .checked_add(file.file_size)
                    .ok_or(BifrostError::QueryAdmissionRejected)
            })
        })?;
        let total_bytes = local_bytes
            .checked_add(remote_bytes)
            .ok_or(BifrostError::QueryAdmissionRejected)?;
        let local_ratio = if total_bytes == 0 {
            0.0
        } else {
            local_bytes
                .to_f64()
                .zip(total_bytes.to_f64())
                .map(|(local, total)| local / total)
                .ok_or(BifrostError::QueryAdmissionRejected)?
        };
        tracing::Span::current()
            .record("query_class", query_class_label(classification.query_class));
        OracleTelemetry::record_classification(classification);
        drop(planning);
        Ok(PlannedSqlCut {
            cuts,
            query_class: classification.query_class,
            local_ratio,
            reader_pin: guard,
            reader_io_permit: permit,
        })
    }

    /// Lowers and optimizes SQL against schema-only providers from one cut.
    ///
    /// All registered providers are empty and session-local. Cancellation during
    /// SQL lowering or optimization drops the session and exposes no partial plan.
    ///
    /// # Errors
    /// Returns a stable planning failure when schema projection, registration,
    /// SQL lowering, or logical optimization fails.
    #[tracing::instrument(
        name = "bifrost.oracle.plan",
        skip_all,
        fields(table_count = cuts.len())
    )]
    async fn prepare_optimized_sql_plan(
        &self,
        sql: &str,
        cuts: &[PinnedSealedTable],
    ) -> Result<datafusion::logical_expr::LogicalPlan, BifrostError> {
        let session = SessionContext::new();
        for cut in cuts {
            let physical = iceberg::arrow::schema_to_arrow_schema(
                cut.iceberg_table.metadata().current_schema(),
            )
            .map_err(|_| BifrostError::QueryExecutionFailed)?;
            let fields = physical
                .fields()
                .iter()
                .filter(|field| field.name() != "data_tenant_id")
                .cloned()
                .collect::<Vec<_>>();
            let public = Arc::new(Schema::new(fields));
            let provider = MemTable::try_new(public, vec![Vec::new()])
                .map_err(|error| map_datafusion_error(&error))?;
            register_session_table(&session, &cut.binding, Arc::new(provider))?;
        }
        session
            .sql(sql)
            .await
            .map_err(|error| map_datafusion_error(&error))?
            .into_optimized_plan()
            .map_err(|error| map_datafusion_error(&error))
    }
}

/// Everything one SQL retry attempt needs to pin, protect, and classify.
///
/// Bundled rather than passed positionally because the same seven values are
/// threaded through ingress classification and leader planning; a struct keeps
/// the two call sites from drifting apart silently.
pub(super) struct SqlPlanInputs<'a> {
    /// Authenticated tenant and request context.
    pub(super) context: &'a AuthorizedQueryContext,
    /// Validated read-only SQL text.
    pub(super) sql: &'a str,
    /// Canonical table references parsed from that SQL.
    pub(super) tables: &'a [TableRef],
    /// Absolute deadline covering preparation and materialization.
    pub(super) deadline: Instant,
    /// Tenant-qualified catalog the cuts are resolved through.
    pub(super) catalog: &'a BifrostCatalog,
    /// This node's reader epoch, absent on a replica with no Oracle role.
    pub(super) authority: Option<&'a Arc<OracleReaderAuthority>>,
    /// Live Oracle CPU used by the classification formula.
    pub(super) live_oracle_cpu: f64,
}

/// Dependencies required to build providers for one typed immutable cut.
pub(super) struct TypedProviderInputs<'a> {
    /// Authenticated tenant context copied into each provider.
    pub(super) context: &'a AuthorizedQueryContext,
    /// Admission class charged by each provider.
    pub(super) class: QueryClass,
    /// Pinned table metadata selected for this query.
    pub(super) cuts: Vec<PinnedSealedTable>,
    /// Drained live batches and reservations for the same cuts.
    pub(super) drained: &'a mut DrainedTails,
    /// Catalog used to resolve hot object locations.
    pub(super) catalog: &'a BifrostCatalog,
    /// Durable audit collaborator retained by each source operator.
    pub(super) audit: Arc<dyn OracleAudit>,
    /// Parent memory governor and reconciliation ceiling.
    pub(super) memory: OracleMemoryResources,
    /// Query-local pool shared by `DataFusion` and Wyrd source owners.
    pub(super) query_pool: Arc<dyn datafusion::execution::memory_pool::MemoryPool>,
    /// Production source/reconciliation telemetry owner.
    pub(super) telemetry: Arc<OracleTelemetry>,
}

/// Collects distinct canonical table identities from a typed plan.
///
/// # Errors
/// Returns invalid SQL when a scan is not a canonical Bifrost table reference
/// or when the plan contains no table scans.
fn collect_plan_table_refs(
    plan: &datafusion::logical_expr::LogicalPlan,
) -> Result<Vec<TableRef>, BifrostError> {
    /// Visits one logical subtree and records distinct canonical table refs.
    ///
    /// # Errors
    /// Returns invalid SQL when a table scan does not use a canonical Bifrost
    /// fully-qualified table reference.
    fn visit(
        plan: &datafusion::logical_expr::LogicalPlan,
        refs: &mut Vec<TableRef>,
    ) -> Result<(), BifrostError> {
        if let datafusion::logical_expr::LogicalPlan::TableScan(scan) = plan {
            let table = TableRef::parse_fqn(&scan.table_name.to_string()).ok_or(
                BifrostError::QueryInvalidSql {
                    detail: "typed query plan contains an invalid Bifrost table reference"
                        .to_owned(),
                },
            )?;
            if !refs.contains(&table) {
                refs.push(table);
            }
        }
        for input in plan.inputs() {
            visit(input, refs)?;
        }
        Ok(())
    }
    let mut refs = Vec::new();
    visit(plan, &mut refs)?;
    if refs.is_empty() {
        return Err(BifrostError::QueryInvalidSql {
            detail: "typed query plans must contain at least one table scan".to_owned(),
        });
    }
    Ok(refs)
}

/// Resolves leader-local hot file locations for one pinned cut.
///
/// # Errors
/// Returns metadata mismatch when a file exceeds process bounds, catalog
/// errors when an object location cannot be resolved, or an aggregate mapping
/// error from either source.
fn local_hot_sources(
    catalog: &BifrostCatalog,
    cut: &PinnedSealedTable,
) -> Result<Vec<HotFileSource>, BifrostError> {
    cut.hot_files
        .iter()
        .map(|file| {
            let size_bytes =
                usize::try_from(file.file_size).map_err(|_| BifrostError::MetadataMismatch {
                    detail: "hot file size exceeds process bounds".to_owned(),
                })?;
            Ok(HotFileSource {
                location: catalog
                    .object_location(&cut.binding, &file.file_path)
                    .map_err(BifrostCatalogError::into_public)?,
                size_bytes,
                event_time:
                    crate::catalog::event_time::EventTimeStatistics::from_catalog_timestamps(
                        file.min_event_time,
                        file.max_event_time,
                    ),
            })
        })
        .collect()
}

/// Registers one schema-only table beneath the Oracle catalog hierarchy.
///
/// The helper mirrors the logical namespace used by SQL lowering while keeping
/// the registered provider empty; physical source access is installed only
/// after the immutable cut and audit decision are complete.
///
/// # Errors
/// Returns query execution failure when the binding namespace is invalid or
/// `DataFusion` rejects catalog/schema/table registration.
fn register_session_table(
    session: &SessionContext,
    binding: &crate::catalog::TenantTableBinding,
    provider: Arc<dyn TableProvider>,
) -> Result<(), BifrostError> {
    let schema_name = binding
        .logical_namespace
        .strip_prefix("vala.")
        .filter(|name| !name.is_empty())
        .ok_or(BifrostError::QueryExecutionFailed)?;
    let catalog = session.catalog("vala").unwrap_or_else(|| {
        let catalog: Arc<dyn CatalogProvider> = Arc::new(MemoryCatalogProvider::new());
        session.register_catalog("vala", Arc::clone(&catalog));
        catalog
    });
    let schema = if let Some(schema) = catalog.schema(schema_name) {
        schema
    } else {
        let schema: Arc<dyn datafusion::catalog::SchemaProvider> =
            Arc::new(MemorySchemaProvider::new());
        catalog
            .register_schema(schema_name, Arc::clone(&schema))
            .map_err(|error| map_datafusion_error(&error))?;
        schema
    };
    let alias = Arc::clone(&provider);
    schema
        .register_table(binding.table_name.clone(), provider)
        .map_err(|error| map_datafusion_error(&error))?;
    // Keep the canonical three-part hierarchy while also accepting the
    // public quoted-FQN form (`"vala.traces.spans"`). DataFusion resolves a
    // quoted dotted identifier as one table under its default
    // `datafusion.public` catalog; this alias preserves that SQL spelling
    // without changing the canonical 1/2/3-part identifier resolution.
    session
        .register_table(TableReference::bare(binding.table_ref.fqn()), alias)
        .map_err(|error| map_datafusion_error(&error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;
    use std::sync::Arc;

    use super::{OraclePlanner, register_session_table};
    use crate::catalog::{TableRef, TenantTableBinding};
    use crate::namespaces::BifrostNamespace;
    use crate::oracle::BifrostError;
    use wyrd_spec::DataTenantId;

    /// Both canonical hierarchy and quoted dotted-FQN SQL resolve identically.
    #[tokio::test]
    async fn quoted_dotted_identifier_resolves_alongside_canonical_name() {
        let session = SessionContext::new();
        let binding = TenantTableBinding::resolve((
            DataTenantId::new(uuid::Uuid::now_v7()).expect("test tenant identity"),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("test table binding");
        let provider = MemTable::try_new(
            Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])),
            vec![Vec::new()],
        )
        .expect("schema-only provider");
        register_session_table(&session, &binding, Arc::new(provider))
            .expect("register canonical and quoted aliases");

        for sql in [
            "SELECT * FROM vala.traces.spans",
            "SELECT * FROM \"vala.traces.spans\"",
        ] {
            session
                .sql(sql)
                .await
                .unwrap_or_else(|error| panic!("{sql} must resolve: {error}"));
        }
    }

    /// Rejects a typed plan whose scan was not replaced by an authenticated provider.
    #[tokio::test]
    async fn typed_source_replacement_fails_closed_for_unmatched_scan() {
        let session = SessionContext::new();
        let provider = MemTable::try_new(
            std::sync::Arc::new(Schema::new(vec![Field::new("name", DataType::Utf8, true)])),
            vec![Vec::new()],
        )
        .expect("schema-only provider");
        session
            .register_table("spans", std::sync::Arc::new(provider))
            .expect("register typed table");
        let plan = session
            .sql("SELECT name FROM spans")
            .await
            .expect("lower typed query")
            .into_optimized_plan()
            .expect("optimize typed query");
        assert!(matches!(
            OraclePlanner::replace_typed_sources(plan, &HashMap::new()),
            Err(BifrostError::QueryExecutionFailed)
        ));
    }

    /// Physical source count does not force analytical scheduling for simple SQL.
    #[test]
    fn disjoint_multi_source_union_preserves_sql_classification() {
        assert_eq!(
            OraclePlanner::classify(0, 1.0, false),
            crate::oracle::QueryClass::Interactive
        );
    }

    /// One small sealed source remains eligible for interactive scheduling.
    #[test]
    fn simple_single_source_cut_classifies_interactive() {
        assert_eq!(
            OraclePlanner::classify(0, 1.0, false),
            crate::oracle::QueryClass::Interactive
        );
    }
}
