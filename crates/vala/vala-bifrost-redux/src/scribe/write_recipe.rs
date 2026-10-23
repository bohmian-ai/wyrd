//! Seal-time resolution of the physical write recipe.
//!
//! Admission binds every batch to the complete recipe it was accepted under,
//! and the seal key carries it through WAL and replay, so a revision or Bloom
//! alteration committed after acceptance never reinterprets accepted rows and
//! WAL replay gets the same recipe as a live seal. Only rows admitted with no
//! registered declaration — the embedded engine seam — resolve the registered
//! control row here.

use std::sync::Arc;

use arrow::datatypes::Schema;
use vala_sql::OperatorPool;

use crate::catalog::layout::{BIFROST_PARTITION_SPEC_ID, PhysicalLayout};
use crate::catalog::{BifrostCatalog, LayoutRevision, TenantTableBinding};
use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;

/// Resolves the write recipe for the rows `binding` accepted under `seal_key`.
///
/// A key that carries its accepted recipe is written under exactly that
/// recipe with no catalog read. Otherwise, with a catalog owner the recipe is
/// the retained Iceberg revision the rows were accepted under
/// ([`BifrostCatalog::retained_layout`]); without one — the embedded engine
/// seam, whose tables are never revised — it is the registered control row,
/// which only describes the creation revision.
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
/// Returns [`ScribeError::Internal`] when the catalog or control read fails,
/// when the table has no control row, when the stored JSON is malformed, when
/// a non-creation revision has no catalog owner to resolve it, or when the
/// recipe no longer resolves against `schema`. The last case means the layout
/// and the physical schema have diverged, and encoding under either
/// interpretation would mislabel the output, so resolution fails closed.
///
/// Resolution deliberately narrows rather than demanding byte equality with the
/// stored form: the catalog canonicalizes over the table's full registered
/// schema, while a sealed generation only carries the managed columns its
/// payload mode stamps (`run_id` is native-only). See
/// [`PhysicalLayout::resolve_for_physical_schema`].
pub(crate) async fn resolve_write_recipe(
    operator: &OperatorPool,
    catalog: Option<&BifrostCatalog>,
    binding: &TenantTableBinding,
    seal_key: &SealKey,
    schema: &Schema,
) -> Result<Arc<PhysicalLayout>, ScribeError> {
    let fqn = binding.table_ref.fqn();
    let revision = seal_key.revision;
    let spec_id = seal_key.partition.spec_id();
    let stored = if let Some(recipe) = &seal_key.recipe {
        recipe.wire().clone()
    } else if let Some(catalog) = catalog {
        catalog
            .retained_layout(binding, revision, spec_id)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "write recipe for {fqn} revision {revision} is unavailable: {error}"
                ),
            })?
            .to_wire()
    } else {
        if revision != LayoutRevision::INITIAL || spec_id != BIFROST_PARTITION_SPEC_ID {
            return Err(ScribeError::Internal {
                detail: format!("write recipe for {fqn} revision {revision} needs a catalog owner"),
            });
        }
        let stored = vala_sql::queries::olap_catalog::registered_physical_layout(
            operator,
            binding.tenant,
            &fqn,
        )
        .await
        .map_err(|error| ScribeError::Internal {
            detail: format!("write recipe control read failed for {fqn}: {error}"),
        })?
        .ok_or_else(|| ScribeError::Internal {
            detail: format!("write recipe has no registered control row for {fqn}"),
        })?;
        serde_json::from_value(stored).map_err(|error| ScribeError::Internal {
            detail: format!("registered physical layout for {fqn} is malformed: {error}"),
        })?
    };
    PhysicalLayout::resolve_for_physical_schema(&fqn, schema, &stored)
        .map(Arc::new)
        .map_err(|error| ScribeError::Internal {
            detail: format!(
                "registered physical layout for {fqn} does not match the sealed schema: {error}"
            ),
        })
}
