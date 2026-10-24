//! Seal-time resolution of the physical write recipe.
//!
//! Admission binds every batch to the table version it was accepted under,
//! and the seal key carries it through WAL and replay, so a definition edit
//! committed after acceptance never reinterprets accepted rows and WAL replay
//! gets the same recipe as a live seal. Only rows admitted with no registered
//! declaration — the embedded engine seam — resolve the registered control row
//! here.

use std::sync::Arc;

use arrow::datatypes::Schema;
use vala_sql::OperatorPool;
use wyrd_spec::vala::ids::TableVersion;

use crate::catalog::layout::{BIFROST_PARTITION_SPEC_ID, BIFROST_SORT_ORDER_ID, PhysicalLayout};
use crate::catalog::{BifrostCatalog, TenantTableBinding};
use crate::contracts::ScribeError;
use crate::scribe::seal_key::SealKey;

/// The physical recipe one table version writes accepted rows under.
#[derive(Debug, Clone)]
pub(crate) struct WriteRecipe {
    /// Layout narrowed to the sealed physical schema.
    pub layout: Arc<PhysicalLayout>,
    /// Iceberg sort-order id the version writes and claims files under.
    pub sort_order_id: i32,
}

/// Resolves the write recipe for the rows `binding` accepted under `seal_key`.
///
/// With a catalog owner the recipe is the definition of the table version the
/// rows were accepted under ([`BifrostCatalog::retained_definition`]); without
/// one — the embedded engine seam, whose tables are never edited — it is the
/// registered control row, which only describes the creation version.
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
/// the version is not retained or its partition spec is not the batch's, when
/// a non-creation version has no catalog owner to resolve it, or when the
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
) -> Result<WriteRecipe, ScribeError> {
    let fqn = binding.table_ref.fqn();
    let table_version = seal_key.table_version;
    let spec_id = seal_key.partition.spec_id();
    let (stored, sort_order_id) = if let Some(catalog) = catalog {
        let definition = catalog
            .retained_definition(binding, table_version)
            .await
            .map_err(|error| ScribeError::Internal {
                detail: format!(
                    "write recipe for {fqn} version {table_version} is unavailable: {error}"
                ),
            })?;
        if definition.spec_id != spec_id {
            return Err(ScribeError::Internal {
                detail: format!(
                    "{fqn} version {table_version} partitions under spec {}, not {spec_id}",
                    definition.spec_id
                ),
            });
        }
        (definition.layout.to_wire(), definition.sort_order_id)
    } else {
        if table_version != TableVersion::INITIAL || spec_id != BIFROST_PARTITION_SPEC_ID {
            return Err(ScribeError::Internal {
                detail: format!(
                    "write recipe for {fqn} version {table_version} needs a catalog owner"
                ),
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
        let stored = serde_json::from_value(stored).map_err(|error| ScribeError::Internal {
            detail: format!("registered physical layout for {fqn} is malformed: {error}"),
        })?;
        (stored, BIFROST_SORT_ORDER_ID)
    };
    PhysicalLayout::resolve_for_physical_schema(&fqn, schema, &stored)
        .map(|layout| WriteRecipe {
            layout: Arc::new(layout),
            sort_order_id,
        })
        .map_err(|error| ScribeError::Internal {
            detail: format!(
                "registered physical layout for {fqn} does not match the sealed schema: {error}"
            ),
        })
}
