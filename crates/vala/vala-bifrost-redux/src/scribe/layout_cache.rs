//! Process-local advisory cache of learned Variant layouts.
//!
//! A claim's sample chooses the Variant layout its final objects are shredded
//! with. [`VariantLayoutCache`] keeps the latest such layout per exact tenant,
//! table, and table version, so a shard opening a later generation of that
//! definition can retain its rows in the learned layout from the start. The
//! cache is advisory: a miss, an eviction, or a restart only means a new
//! generation stays canonical until a claim this process assembles teaches it
//! again. Nothing durable or coordinated depends on it.
//!
//! Each entry is charged to the Scribe governed root for as long as anything
//! holds it — the cache or a generation that pinned it — so eviction releases
//! only the cache's ownership while an active generation keeps its layout.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use arrow::array::RecordBatch;
use arrow::datatypes::SchemaRef;
use iceberg::writer::file_writer::variant_shredding::VariantLayout;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::ids::TableVersion;

use crate::catalog::TableRef;
use crate::contracts::ScribeError;
use crate::schema::SchemaFingerprint;

/// Most distinct table definitions the cache holds before evicting the oldest.
///
/// Each entry is a few schema-sized allocations; the bound keeps the map from
/// growing with the number of tables a process ever wrote, and every entry is
/// also charged to the governed root.
const VARIANT_LAYOUT_CACHE_ENTRIES: usize = 256;

/// Most bytes one shredded type node takes per slot: a 16-byte residual view,
/// a value of at most 16 bytes (a view or a `Decimal128`), and validity.
const SHREDDED_SLOT_BYTES: usize = 40;

/// Counts the nodes of `data_type` and reports whether any lies under a List.
fn type_nodes(data_type: &arrow::datatypes::DataType) -> (usize, bool) {
    use arrow::datatypes::DataType;
    match data_type {
        DataType::Struct(fields) => fields.iter().fold((1, false), |(nodes, listed), field| {
            let (child, child_listed) = type_nodes(field.data_type());
            (nodes.saturating_add(child), listed || child_listed)
        }),
        DataType::List(field)
        | DataType::LargeList(field)
        | DataType::ListView(field)
        | DataType::LargeListView(field)
        | DataType::FixedSizeList(field, _) => {
            let (child, _) = type_nodes(field.data_type());
            (child.saturating_add(1), true)
        }
        _ => (1, false),
    }
}

/// The exact table definition a learned layout applies to.
type LayoutKey = (DataTenantId, TableRef, TableVersion);

/// One learned Variant layout and the schemas it maps between.
///
/// Immutable once built and shared by reference: a generation pins the
/// `Arc` it opened with, so a later refresh never changes its layout.
#[derive(Debug)]
pub(crate) struct LearnedLayout {
    /// Layout chosen by a claim's sample.
    layout: VariantLayout,
    /// Logical schema the layout was learned for.
    logical: SchemaRef,
    /// Exact fingerprint of `logical`.
    logical_fingerprint: SchemaFingerprint,
    /// Physical schema rows take under the layout.
    physical: SchemaRef,
    /// Governed ownership of this entry's bytes, released with the last holder.
    _charge: crate::resources::ScribeMemoryLease,
}

impl LearnedLayout {
    /// Returns the logical schema the layout applies to.
    pub(crate) const fn logical(&self) -> &SchemaRef {
        &self.logical
    }

    /// Reports whether `schema` is exactly the logical schema this layout was learned for.
    pub(crate) fn applies_to(&self, schema: &SchemaRef) -> bool {
        SchemaFingerprint::from_arrow_schema_exact(schema) == self.logical_fingerprint
    }

    /// Returns an upper bound on the bytes shredding `rows` newly allocates.
    ///
    /// Only the Variant columns the layout rewrites are allocated again; every
    /// other column is shared with the output. A rewritten column's residual
    /// and typed string bytes are each at most its original bytes, and every
    /// node of its shredded type takes at most [`SHREDDED_SLOT_BYTES`] of
    /// views, fixed-width values, and validity per slot. A slot is a row, or —
    /// under a List — an element, of which there are at most as many as
    /// encoded bytes. The bound is reserved before conversion so the original
    /// and shredded copies never coexist unaccounted.
    pub(crate) fn conversion_bytes(&self, rows: &RecordBatch) -> usize {
        self.physical
            .fields()
            .iter()
            .zip(self.logical.fields())
            .zip(rows.columns())
            .filter(|((physical, logical), _)| physical.data_type() != logical.data_type())
            .map(|((physical, _), column)| {
                let bytes = column.get_array_memory_size();
                let (nodes, listed) = type_nodes(physical.data_type());
                let slots = if listed {
                    rows.num_rows().saturating_add(bytes)
                } else {
                    rows.num_rows()
                };
                bytes.saturating_mul(2).saturating_add(
                    nodes
                        .saturating_mul(SHREDDED_SLOT_BYTES)
                        .saturating_mul(slots),
                )
            })
            .fold(0, usize::saturating_add)
    }

    /// Shreds logical `rows` into this layout's physical schema.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when `rows` is not in the layout's
    /// logical schema or Arrow refuses to shred a Variant column.
    pub(crate) fn shred(&self, rows: &RecordBatch) -> Result<RecordBatch, ScribeError> {
        if !self.applies_to(rows.schema_ref()) {
            return Err(ScribeError::Internal {
                detail:
                    "rows are not in the logical schema their generation's layout was learned for"
                        .to_owned(),
            });
        }
        self.layout
            .shred(rows)
            .map_err(|error| ScribeError::Internal {
                detail: format!("shred rows into their generation's Variant layout: {error}"),
            })
    }
}

/// Bounded, governed map from table definition to its latest learned layout.
#[derive(Debug, Default)]
pub(crate) struct VariantLayoutCache {
    /// Entries in install order, oldest first; at most one per key.
    entries: Mutex<VecDeque<(LayoutKey, Arc<LearnedLayout>)>>,
}

impl VariantLayoutCache {
    /// Returns the layout learned for exactly `tenant`, `table`, and `version`, if any.
    ///
    /// A poisoned lock reads as a miss: the cache is advisory and a miss keeps
    /// the generation canonical.
    pub(crate) fn get(
        &self,
        tenant: DataTenantId,
        table: &TableRef,
        version: TableVersion,
    ) -> Option<Arc<LearnedLayout>> {
        let entries = self.entries.lock().ok()?;
        entries
            .iter()
            .find(|((entry_tenant, entry_table, entry_version), _)| {
                *entry_tenant == tenant && entry_table == table && *entry_version == version
            })
            .map(|(_, layout)| Arc::clone(layout))
    }

    /// Replaces the hint for one table definition with the layout a claim just learned.
    ///
    /// Called on successful claim assembly, in completion order, so the most
    /// recently completed claim wins; an equivalent hint from a retried claim
    /// is harmless. An unshredded layout removes the hint. When the governed
    /// root cannot charge the entry, or the layout cannot be applied to
    /// `logical`, the hint is dropped rather than failing assembly. Only
    /// generations opened later see the new hint.
    pub(crate) fn install(
        &self,
        resources: &crate::resources::ScribeResources,
        key: (DataTenantId, TableRef, TableVersion),
        logical: SchemaRef,
        layout: VariantLayout,
    ) {
        let learned = if layout.is_unshredded() {
            None
        } else {
            match Self::learned(resources, logical, layout) {
                Ok(learned) => Some(learned),
                Err(error) => {
                    tracing::warn!(error = %error, "Variant layout hint not cached");
                    None
                }
            }
        };
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.retain(|(entry, _)| *entry != key);
        if let Some(learned) = learned {
            entries.push_back((key, Arc::new(learned)));
            while entries.len() > VARIANT_LAYOUT_CACHE_ENTRIES {
                entries.pop_front();
            }
        }
    }

    /// Builds and charges one entry.
    ///
    /// The charge is the two schemas' Arrow-reported sizes; the layout itself
    /// holds one data type per shredded field, no larger than the physical
    /// schema, so it is counted as that schema again.
    ///
    /// # Errors
    ///
    /// Returns the shredding error when `layout` cannot be applied to
    /// `logical`, or the root's refusal when it cannot hold the entry.
    fn learned(
        resources: &crate::resources::ScribeResources,
        logical: SchemaRef,
        layout: VariantLayout,
    ) -> Result<LearnedLayout, ScribeError> {
        let physical = layout
            .physical_schema(&logical)
            .map_err(|error| ScribeError::Internal {
                detail: format!("derive the learned Variant layout's physical schema: {error}"),
            })?;
        let bytes = logical
            .fields()
            .size()
            .saturating_add(physical.fields().size().saturating_mul(2));
        let charge = resources
            .try_reserve_maintenance(crate::scribe::memory::MemoryCategory::Metadata, bytes)?;
        Ok(LearnedLayout {
            layout,
            logical_fingerprint: SchemaFingerprint::from_arrow_schema_exact(&logical),
            logical,
            physical,
            _charge: charge,
        })
    }
}
