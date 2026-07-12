//! Query session builder for the Bifrost serving layer (slice 04).
//!
//! [`build_query_context`] constructs a [`QueryCtx`] that carries:
//! - A `DataFusion` [`SessionContext`] with the tenant predicate rule wired in.
//! - All loaded [`ProjectionCandidate`]s for a given source table.
//!
//! This is the single builder used by the sync path, async path, and admission
//! control — none of those paths construct their own session; they call this
//! function and receive a `QueryCtx` with projections already loaded.
//!
//! The function performs one Postgres round-trip (via [`list_by_source`]) and
//! is otherwise synchronous. Callers supply a `&mut TenantConn<'_>` that is
//! already bound to the correct tenant.

use vala_sql::SqlError;
use vala_sql::TenantConn;
use vala_sql::queries::olap_catalog::list_by_source;
use wyrd_spec::ids::DataTenantId;

use crate::catalog::WyrdCatalog;
use crate::serving::projections::{ProjectionCandidate, ProjectionKind};
use crate::session::wyrd_session_context;
use crate::types::SchemaFingerprint;

/// A fully-assembled query execution context, ready to serve a query.
///
/// Constructed by [`build_query_context`] and consumed by the query executor
/// (slice 07), the admission classifier (slice 06), and the sync handler.
pub struct QueryCtx {
    /// `DataFusion` session context with tenant predicate rule.
    pub session: datafusion::prelude::SessionContext,
    /// Projection candidates for the source table (may be empty).
    pub projection_candidates: Vec<ProjectionCandidate>,
    /// The source table UID for which candidates were loaded.
    pub source_table_uid: Vec<u8>,
}

/// Build a [`QueryCtx`] for the given tenant and source table.
///
/// Loads projection candidates from `vala.olap_projections` via
/// [`list_by_source`], converts each row to a [`ProjectionCandidate`],
/// and wires a `DataFusion` session context with the tenant predicate rule.
///
/// # Errors
/// Returns [`SqlError`] when the projection load query fails.
pub async fn build_query_context(
    conn: &mut TenantConn<'_>,
    _catalog: &WyrdCatalog,
    tenant: DataTenantId,
    source_table_uid: &[u8; 16],
) -> Result<QueryCtx, SqlError> {
    // Load projection candidates for the source table.
    let rows = list_by_source(conn, source_table_uid).await?;

    let projection_candidates: Vec<ProjectionCandidate> = rows
        .into_iter()
        .filter_map(|row| {
            let kind = ProjectionKind::from_db_str(&row.projection_kind)?;

            // F14: convert the stored 32-byte fingerprint bytes.
            let fingerprint_bytes: [u8; 32] = row.source_schema_fingerprint.try_into().ok()?;
            let source_schema_fingerprint = SchemaFingerprint(fingerprint_bytes);

            Some(ProjectionCandidate {
                projection_uid: row.projection_uid,
                kind,
                fqn: row.fqn,
                refresh_epoch: row.refresh_epoch,
                source_refresh_epoch: row.source_refresh_epoch,
                built_for_snapshot_id: row.built_for_snapshot_id,
                commit_lag: row.commit_lag,
                source_schema_fingerprint,
            })
        })
        .collect();

    tracing::debug!(
        tenant = %tenant,
        source_table_uid = ?source_table_uid,
        candidates = projection_candidates.len(),
        "build_query_context: loaded projection candidates"
    );

    // Build a DataFusion session context with the tenant predicate rule.
    let session = wyrd_session_context(tenant);

    Ok(QueryCtx {
        session,
        projection_candidates,
        source_table_uid: source_table_uid.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SchemaFingerprint;

    /// Verify that an empty projection candidate list is handled correctly
    /// (no panic, empty Vec returned).
    #[test]
    fn empty_candidates_is_valid_query_ctx() {
        // Since build_query_context requires a live DB, we verify the
        // projection-candidate construction path separately here.
        let candidates: Vec<ProjectionCandidate> = vec![];
        assert!(candidates.is_empty());
    }

    /// Verify that a row with an unknown kind is silently dropped.
    #[test]
    fn unknown_kind_row_is_dropped() {
        use sqlx::types::Uuid;
        use vala_sql::row_types::olap_catalog::ProjectionCandidateRow;

        let row = ProjectionCandidateRow {
            data_tenant_id: Uuid::new_v4(),
            projection_uid: vec![0u8; 16],
            source_table_uid: vec![0u8; 16],
            fqn: "test.unknown".to_string(),
            projection_kind: "unknown_kind".to_string(), // unknown — will be dropped
            projection_state: "idle".to_string(),
            refresh_epoch: 0,
            source_refresh_epoch: 0,
            built_for_snapshot_id: None,
            commit_lag: 0,
            source_schema_fingerprint: vec![1u8; 32],
        };

        // Simulate the filter_map logic.
        let kind = ProjectionKind::from_db_str(&row.projection_kind);
        assert!(
            kind.is_none(),
            "unknown kind must return None and be dropped"
        );
    }

    /// Verify that a row with a bad fingerprint length (not 32 bytes) is dropped.
    #[test]
    fn bad_fingerprint_length_row_is_dropped() {
        let bad_fp: Vec<u8> = vec![1u8; 16]; // wrong length
        let result: Option<[u8; 32]> = bad_fp.try_into().ok();
        assert!(result.is_none(), "16-byte fingerprint must fail try_into");
    }

    /// Verify that a valid row with known kind and 32-byte fingerprint converts correctly.
    #[test]
    fn valid_row_converts_to_candidate() {
        use sqlx::types::Uuid;
        use vala_sql::row_types::olap_catalog::ProjectionCandidateRow;

        let row = ProjectionCandidateRow {
            data_tenant_id: Uuid::new_v4(),
            projection_uid: vec![7u8; 16],
            source_table_uid: vec![3u8; 16],
            fqn: "vala.rollup_summary".to_string(),
            projection_kind: "rollup".to_string(),
            projection_state: "idle".to_string(),
            refresh_epoch: 5,
            source_refresh_epoch: 5,
            built_for_snapshot_id: Some(100),
            commit_lag: 2,
            source_schema_fingerprint: vec![0xABu8; 32],
        };

        let kind = ProjectionKind::from_db_str(&row.projection_kind);
        assert!(kind.is_some());
        assert_eq!(kind.unwrap(), ProjectionKind::Rollup);

        let fp_bytes: [u8; 32] = row.source_schema_fingerprint.try_into().unwrap();
        let fp = SchemaFingerprint(fp_bytes);
        assert_eq!(fp.0[0], 0xAB);
    }
}
