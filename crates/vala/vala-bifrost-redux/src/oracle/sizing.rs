//! Sizes an Oracle query's partitions from the bytes it scans.
//!
//! The optimized logical plan names every table scan, the columns it
//! projects, and the filters that reach it. Before the physical plan exists,
//! each scan's provider estimates its bytes from Iceberg's file listing, the
//! cached footers' row-group statistics, and the live bytes Scribe reported
//! for the same scope; the sum picks the session's target partitions (see
//! [`crate::scan_sizing::OraclePartitioning::scan_partitions`]). A scan earns parallelism by
//! the data it reads, so a point lookup runs as one stream and a full scan
//! keeps every core.

use std::sync::Arc;

use datafusion::catalog::Session;
use datafusion::common::tree_node::{TreeNode as _, TreeNodeRecursion};
use datafusion::datasource::source_as_provider;
use datafusion::logical_expr::utils::split_conjunction;
use datafusion::logical_expr::{Expr, LogicalPlan, TableScan};

use super::exec::OracleTableProvider;
use crate::scan_sizing::ScanScope;

/// One Oracle table scan in an optimized logical plan.
struct OracleScan {
    /// The provider the scan reads.
    provider: Arc<OracleTableProvider>,
    /// Filters pushed into the scan, which Iceberg lists files with.
    pushed: Vec<Expr>,
    /// Columns the scan projects and every filter conjunct that reaches it,
    /// pushed or evaluated directly above it.
    scope: ScanScope,
}

/// Every Oracle table scan of one query's optimized logical plan.
pub(super) struct OracleScans {
    /// Scans in plan order; a table scanned twice appears twice.
    scans: Vec<OracleScan>,
}

impl OracleScans {
    /// Collects every Oracle table scan of `plan` with its scope.
    ///
    /// A `Filter` directly above a scan contributes its conjuncts: a predicate
    /// Iceberg cannot push stays there in the logical plan, yet the physical
    /// plan pushes it into the Parquet scan, where it prunes row groups.
    #[must_use]
    pub(super) fn collect(plan: &LogicalPlan) -> Self {
        let mut scans = Vec::new();
        // Pre-order: a filter is visited before the scan below it, which is
        // then already collected.
        let mut filtered: Vec<*const TableScan> = Vec::new();
        // The visitor never fails; `apply` returns `Result` for visitors that do.
        let _ = plan.apply(|node| {
            match node {
                LogicalPlan::Filter(filter) => {
                    if let LogicalPlan::TableScan(scan) = filter.input.as_ref() {
                        filtered.push(std::ptr::from_ref(scan));
                        scans.extend(Self::scan(scan, split_conjunction(&filter.predicate)));
                    }
                }
                LogicalPlan::TableScan(scan) if !filtered.contains(&std::ptr::from_ref(scan)) => {
                    scans.extend(Self::scan(scan, Vec::new()));
                }
                _ => {}
            }
            Ok(TreeNodeRecursion::Continue)
        });
        Self { scans }
    }

    /// Builds one scan's entry, or `None` when it reads no Oracle provider.
    fn scan(scan: &TableScan, above: Vec<&Expr>) -> Option<OracleScan> {
        let provider: Arc<dyn std::any::Any + Send + Sync> =
            source_as_provider(&scan.source).ok()?;
        let provider = provider.downcast::<OracleTableProvider>().ok()?;
        let columns = scan
            .projected_schema
            .fields()
            .iter()
            .map(|field| field.name().clone())
            .collect();
        let filters = scan
            .filters
            .iter()
            .flat_map(split_conjunction)
            .chain(above)
            .cloned()
            .collect();
        Some(OracleScan {
            provider,
            pushed: scan.filters.clone(),
            scope: ScanScope { columns, filters },
        })
    }

    /// Returns the scope a live listing of `table` reports bytes for.
    ///
    /// A table scanned once lists with that scan's scope. A table scanned more
    /// than once, or not at all, lists unscoped, which reports every retained
    /// byte: no single scope covers two scans.
    #[must_use]
    pub(super) fn listing_scope(&self, table: &str) -> ScanScope {
        let mut scans = self
            .scans
            .iter()
            .filter(|scan| scan.provider.table() == table);
        match (scans.next(), scans.next()) {
            (Some(scan), None) => scan.scope.clone(),
            _ => ScanScope::default(),
        }
    }

    /// Returns what every scan reads: bytes, and the row groups of their
    /// Parquet files.
    ///
    /// Each provider's live routes must already be set, so a scan counts its
    /// table's live bytes. A table scanned twice counts twice, as it is read
    /// twice.
    ///
    /// # Errors
    ///
    /// Returns a provider's `DataFusion` error when Iceberg cannot list its
    /// files.
    pub(super) async fn scan_size(
        &self,
        state: &dyn Session,
    ) -> datafusion::error::Result<crate::scan_sizing::ScanSize> {
        let mut total = crate::scan_sizing::ScanSize::default();
        for scan in &self.scans {
            let size = scan
                .provider
                .scan_size(state, &scan.pushed, &scan.scope)
                .await?;
            total = total.plus(size);
        }
        Ok(total)
    }
}
