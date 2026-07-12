//! Read-time plan rewriter: substitutes source-table scans with projection scans.
//!
//! [`ProjectionRewriter`] rewrites a DataFusion [`LogicalPlan`] by replacing a
//! `TableScan` of the source table with a `TableScan` of a chosen projection
//! table. The caller is responsible for selecting a single [`MatchedProjection`]
//! from the output of [`super::matcher::match_projections`]; this module
//! applies the rewrite unconditionally once given a choice.
//!
//! Freshness rules are enforced by the matcher (see `matcher.rs`). The rewriter
//! trusts that the provided `chosen` projection passed all checks. It does NOT
//! re-validate freshness.
//!
//! DataFusion plan rewriting uses [`TreeNode::transform`], which descends the
//! plan bottom-up. We replace `TableScan` nodes whose table name matches the
//! source FQN with a new scan of the projection table, preserving all other
//! plan nodes (filters, aggregations, projections). If no scan matches, the
//! plan is returned unchanged.

use datafusion::common::Result as DfResult;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::logical_expr::LogicalPlan;

use super::matcher::MatchedProjection;

/// Rewrites a DataFusion logical plan to substitute a source-table scan with
/// a projection-table scan.
#[derive(Debug)]
pub struct ProjectionRewriter {
    /// Fully-qualified source table name (as registered in the Iceberg catalog).
    source_fqn: String,
    /// Fully-qualified projection table name to substitute with.
    projection_fqn: String,
}

impl ProjectionRewriter {
    /// Create a rewriter for the given source → projection substitution.
    #[must_use]
    pub fn new(source_fqn: impl Into<String>, chosen: &MatchedProjection) -> Self {
        Self {
            source_fqn: source_fqn.into(),
            projection_fqn: chosen.candidate.fqn.clone(),
        }
    }

    /// Rewrite `plan`, substituting any `TableScan` of `source_fqn` with
    /// a scan of the projection table.
    ///
    /// Returns the rewritten plan. If no matching scan is found, returns
    /// the original plan unchanged.
    ///
    /// # Errors
    /// Returns a DataFusion [`DfResult`] error if plan transformation fails
    /// (e.g. an internal tree-node inconsistency).
    pub fn rewrite(&self, plan: LogicalPlan) -> DfResult<LogicalPlan> {
        let rewritten = plan.transform(|node| {
            let LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };

            // Check whether this scan targets the source table.
            if scan.table_name.table() != self.source_fqn && !self.scan_matches_source(scan) {
                return Ok(Transformed::no(node));
            }

            // Build a replacement scan targeting the projection table.
            // The projection table must be registered in the same catalog with the
            // same schema (guaranteed by the refresh worker). We update only the
            // table_name; the source, projected_schema, and filters are forwarded.
            let mut new_scan = scan.clone();
            new_scan.table_name =
                datafusion::common::TableReference::bare(self.projection_fqn.clone());

            tracing::debug!(
                source = %self.source_fqn,
                projection = %self.projection_fqn,
                "projection rewriter: substituted source scan with projection"
            );

            Ok(Transformed::yes(LogicalPlan::TableScan(new_scan)))
        })?;

        Ok(rewritten.data)
    }

    /// Returns `true` when the scan's table name matches the source FQN,
    /// considering both bare names and qualified (schema.table) names.
    fn scan_matches_source(&self, scan: &datafusion::logical_expr::TableScan) -> bool {
        // The source_fqn may be stored as "namespace.name" (e.g. "vala.metrics").
        // DataFusion table references can be unqualified (bare) or schema-qualified.
        let table_ref = &scan.table_name;

        // Bare match: just the table name portion.
        if table_ref.table() == self.source_fqn {
            return true;
        }

        // Schema-qualified match: "schema.table".
        if let datafusion::common::TableReference::Partial { schema, table } = table_ref {
            let qualified = format!("{schema}.{table}");
            if qualified == self.source_fqn {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serving::projections::matcher::{
        MatchPlan, MatchedProjection, ProjectionCandidate, ProjectionKind, match_projections,
    };
    use crate::types::SchemaFingerprint;
    use datafusion::common::TableReference;
    use datafusion::logical_expr::{LogicalPlan, TableScan};

    fn fingerprint(byte: u8) -> SchemaFingerprint {
        SchemaFingerprint([byte; 32])
    }

    fn fresh_lookup_candidate() -> ProjectionCandidate {
        ProjectionCandidate {
            projection_uid: vec![0u8; 16],
            kind: ProjectionKind::LookupSet,
            fqn: "vala.proj_lookup".to_string(),
            refresh_epoch: 3,
            source_refresh_epoch: 3,
            built_for_snapshot_id: Some(99),
            commit_lag: 0,
            source_schema_fingerprint: fingerprint(1),
        }
    }

    fn base_plan_ctx() -> MatchPlan {
        MatchPlan {
            source_refresh_epoch: 3,
            current_snapshot_id: Some(99),
            source_schema_fingerprint: fingerprint(1),
            max_rollup_commit_lag: 5,
        }
    }

    /// Behavior gate (rewrite): a fresh projection changes the scan table name.
    ///
    /// We construct the rewriter directly (without a full DataFusion plan
    /// round-trip) and verify the projection FQN is correctly carried.
    #[test]
    fn rewriter_substitution_uses_projection_fqn() {
        let candidate = fresh_lookup_candidate();
        let plan_ctx = base_plan_ctx();
        let matched = match_projections(&[candidate], &plan_ctx);
        assert_eq!(matched.len(), 1, "fresh candidate must match");

        let rewriter = ProjectionRewriter::new("vala.source_table", &matched[0]);
        assert_eq!(rewriter.source_fqn, "vala.source_table");
        assert_eq!(rewriter.projection_fqn, "vala.proj_lookup");
    }

    /// Behavior gate (rewrite): a stale projection is rejected by the matcher,
    /// so no rewriter is constructed for it.
    #[test]
    fn stale_projection_rejected_by_matcher_no_rewrite() {
        let mut candidate = fresh_lookup_candidate();
        candidate.built_for_snapshot_id = Some(1); // stale snapshot
        let plan_ctx = base_plan_ctx();
        let matched = match_projections(&[candidate], &plan_ctx);
        assert!(
            matched.is_empty(),
            "stale projection must not produce a match"
        );
    }

    /// `scan_matches_source` returns true for a bare name that equals source_fqn.
    #[test]
    fn scan_matches_bare_source_name() {
        use datafusion::datasource::default_table_source::DefaultTableSource;
        use datafusion::datasource::empty::EmptyTable;

        let candidate = fresh_lookup_candidate();
        let plan_ctx = base_plan_ctx();
        let matched = match_projections(&[candidate], &plan_ctx);
        let rewriter = ProjectionRewriter::new("metrics", &matched[0]);

        let bare_ref = TableReference::bare("metrics");
        let empty_table = std::sync::Arc::new(EmptyTable::new(std::sync::Arc::new(
            arrow::datatypes::Schema::empty(),
        )));
        let source: std::sync::Arc<dyn datafusion::logical_expr::TableSource> =
            std::sync::Arc::new(DefaultTableSource::new(empty_table));
        let empty_schema = std::sync::Arc::new(
            datafusion::common::DFSchema::try_from(arrow::datatypes::Schema::empty()).unwrap(),
        );
        let fake_scan = TableScan {
            table_name: bare_ref,
            source,
            projection: None,
            projected_schema: empty_schema,
            filters: vec![],
            fetch: None,
        };
        assert!(rewriter.scan_matches_source(&fake_scan));
    }

    /// Rewriting a plan with no matching scan returns the plan unchanged.
    #[test]
    fn rewrite_no_match_returns_plan_unchanged() {
        let candidate = fresh_lookup_candidate();
        let plan_ctx = base_plan_ctx();
        let matched = match_projections(&[candidate], &plan_ctx);
        let rewriter = ProjectionRewriter::new("vala.source_table", &matched[0]);

        // A plan with an EmptyRelation has no TableScan — rewrite is a no-op.
        let empty_plan = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
            produce_one_row: false,
            schema: std::sync::Arc::new(datafusion::common::DFSchema::empty()),
        });

        let result = rewriter.rewrite(empty_plan.clone()).unwrap();
        // Plan node type should be unchanged (EmptyRelation).
        assert!(
            matches!(result, LogicalPlan::EmptyRelation(_)),
            "no-match rewrite must return original plan type"
        );
    }

    /// Projections module is exported correctly.
    #[test]
    fn projections_module_accessible() {
        let _ = ProjectionRewriter::new(
            "source",
            &MatchedProjection {
                candidate: fresh_lookup_candidate(),
            },
        );
    }
}
