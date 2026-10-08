//! Seal-time resolution of the registered physical write recipe.
//!
//! Bifrost follows the table-format write-task convention: a writer resolves
//! schema, partitioning, sort order, and footer properties from the registered
//! table once per write task, and row batches never carry layout with them.
//! That keeps a re-registration authoritative for the next write instead of
//! leaving each in-flight generation pinned to the layout that was current when
//! its rows arrived, and it gives WAL replay the same recipe as a live seal.
//!
//! Resolution is unconditional rather than cached. One indexed control-row read
//! per seal is negligible next to Parquet encoding and object upload, and it
//! removes any window in which a stale recipe could label an artifact
//! `bifrost-writer-v2` while writing a different physical layout.

use std::sync::Arc;

use arrow::datatypes::Schema;
use vala_sql::OperatorPool;

use crate::catalog::TenantTableBinding;
use crate::catalog::layout::PhysicalLayout;
use crate::contracts::ScribeError;

/// Resolves the canonical write recipe registered for `binding`.
///
/// `operator` is the Scribe operator lane's control capability. The registry
/// read itself belongs to the `vala-sql` `olap_catalog` owner, which names the
/// tenant explicitly because that lane serves every tenant's staged work.
///
/// `schema` is the frozen generation's physical Arrow schema — the exact schema
/// the encoder will write — so re-resolving against it guarantees every sort
/// and Bloom column named by the recipe is present in the artifact.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the control read fails, when the
/// table has no control row, when the stored JSON is malformed, or when the
/// stored declaration no longer resolves against `schema`. The last case means
/// the registered layout and the physical schema have diverged, and encoding
/// under either interpretation would mislabel the output, so resolution fails
/// closed.
///
/// Resolution deliberately narrows rather than demanding byte equality with the
/// stored form: the catalog canonicalizes over the table's full registered
/// schema, while a sealed generation only carries the managed columns its
/// payload mode stamps (`run_id` is native-only). See
/// [`PhysicalLayout::resolve_for_physical_schema`].
pub(crate) async fn resolve_write_recipe(
    operator: &OperatorPool,
    binding: &TenantTableBinding,
    schema: &Schema,
) -> Result<Arc<PhysicalLayout>, ScribeError> {
    let fqn = binding.table_ref.fqn();
    let stored =
        vala_sql::queries::olap_catalog::registered_physical_layout(operator, binding.tenant, &fqn)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!("write recipe control read failed for {fqn}: {error}"),
            })?
            .ok_or_else(|| ScribeError::Internal {
                detail: format!("write recipe has no registered control row for {fqn}"),
            })?;
    let stored: wyrd_spec::vala::api::PhysicalLayoutWire =
        serde_json::from_value(stored).map_err(|error| ScribeError::Internal {
            detail: format!("registered physical layout for {fqn} is malformed: {error}"),
        })?;
    PhysicalLayout::resolve_for_physical_schema(&fqn, schema, &stored)
        .map(Arc::new)
        .map_err(|error| ScribeError::Internal {
            detail: format!(
                "registered physical layout for {fqn} does not match the sealed schema: {error}"
            ),
        })
}
