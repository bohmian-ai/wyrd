//! Promotion of already-published Scribe hot objects into the Iceberg table.
//!
//! Promotion is the one Forge route that produces no new data object. A Scribe
//! writer has already sealed an immutable Parquet object, verified it, and
//! recorded its exact `DataFile` projection as durable promotion evidence on
//! the `vala.file_list` row. Promotion revalidates that evidence against the
//! object that exists, fast-appends the writer's own `DataFile` values
//! unchanged, and then settles SQL and audit so Oracle switches the row from
//! the hot source to the promoted source exactly once.
//!
//! Nothing in this module selects, groups, packs, rewrites, or re-derives
//! geometry: doing any of those would make the promoted file differ from the
//! object Scribe published and break the exactness the route exists to
//! preserve.

use std::collections::BTreeSet;

use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;
use tracing::Instrument as _;
use uuid::Uuid;
use vala_sql::queries::file_list::HotFileCatalog;
use vala_sql::queries::forge_operations::ForgeOperations;
use vala_sql::row_types::forge_operations::{ForgeOperationFamily, ForgeOperationTransition};
use wyrd_spec::vala::api::{
    AuditDetail, ForgePromotedFile, ForgePromotedFileSetDigest, ForgeScribePromotionPhase,
    StoragePath,
};

use super::Forge;
use super::compact::ForgeGroupKey;
use super::error::ForgeError;
use super::lease::ForgeLease;
use crate::catalog::TenantTableBinding;
use crate::scribe::promotion::ScribePublishedHotFileV1;

/// Canonical `kind` discriminator carried by promotion task parameters.
pub(super) const SCRIBE_PROMOTION_PARAMETER_KIND: &str = "scribe_promotion";

/// The one Iceberg branch Bifrost publishes tenant table data to.
pub(super) const PROMOTION_BRANCH: &str = "main";

/// One table's complete promotion demand plus the volume it will publish.
///
/// The byte total is deliberately outside [`ScribePromotionPlan`]: the plan is
/// the durable value that must round-trip through task parameters unchanged,
/// and promoted volume is a planning estimate, not part of the promoted-file
/// identity the digest binds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScribePromotionDemand {
    /// The exact ordered, digest-bound group this table owes.
    pub(super) plan: ScribePromotionPlan,
    /// Sum of the encoded sizes of every promoted object.
    pub(super) total_bytes: u64,
    /// Each promoted object's writer evidence, aligned with `plan.files()`.
    ///
    /// The records are the only source of the `DataFile` values promotion
    /// appends. They are carried alongside the plan rather than re-read later
    /// so the group a worker revalidates and the group it appends are provably
    /// the same read.
    pub(super) records: Vec<ScribePublishedHotFileV1>,
}

/// One ordered, digest-bound group of hot objects promoted as a single unit.
///
/// The plan is a pure durable value: it owns the branch it publishes to, the
/// exact ordered files it promotes, and the digest that binds them. Its
/// ordering is the durable production order the demand read returned, and the
/// digest is computed over that order, so a plan that survives enqueue,
/// restart, and takeover proves it still names the same file set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ScribePromotionPlan {
    /// Iceberg branch the promotion commit targets.
    branch: String,
    /// Ordered promoted files, one per hot object.
    files: Vec<ForgePromotedFile>,
    /// Digest binding the exact ordered file set.
    digest: ForgePromotedFileSetDigest,
}

impl ScribePromotionPlan {
    /// Builds one plan from an ordered, non-empty, duplicate-free file set.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the branch is empty, the file set
    /// is empty, or two entries share a file identity or canonical path — any
    /// of which would let one object be promoted twice within one commit.
    pub(super) fn new(branch: &str, files: Vec<ForgePromotedFile>) -> Result<Self, ForgeError> {
        if branch.trim().is_empty() || files.is_empty() {
            return Err(ForgeError::Invariant {
                detail: "Scribe promotion requires a branch and at least one file".to_owned(),
            });
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for file in &files {
            if !ids.insert(file.file_id()) || !paths.insert(file.path().as_str().to_owned()) {
                return Err(ForgeError::Invariant {
                    detail: "Scribe promotion group repeats a file identity or path".to_owned(),
                });
            }
        }
        let digest = ForgePromotedFileSetDigest::compute(&files);
        Ok(Self {
            branch: branch.to_owned(),
            files,
            digest,
        })
    }

    /// Borrows the ordered promoted files in their durable promotion order.
    pub(super) fn files(&self) -> &[ForgePromotedFile] {
        &self.files
    }

    /// Borrows the Iceberg branch this promotion commits to.
    pub(super) fn branch(&self) -> &str {
        &self.branch
    }

    /// Borrows the digest binding this exact ordered file set.
    pub(super) const fn digest(&self) -> &ForgePromotedFileSetDigest {
        &self.digest
    }

    /// Collects the durable `file_list` identities this plan settles.
    pub(super) fn file_ids(&self) -> Vec<Uuid> {
        self.files.iter().map(ForgePromotedFile::file_id).collect()
    }

    /// Collects the canonical logical paths this plan promotes.
    pub(super) fn paths(&self) -> Vec<StoragePath> {
        self.files.iter().map(|file| file.path().clone()).collect()
    }

    /// Encodes the plan as the durable task parameters persisted with the task.
    ///
    /// The parallel arrays are aligned by position and the digest travels with
    /// them, so a worker that decodes the parameters can prove the group it is
    /// about to promote is byte-for-byte the group the scheduler planned.
    pub(super) fn to_parameters(&self) -> Value {
        serde_json::json!({
            "kind": SCRIBE_PROMOTION_PARAMETER_KIND,
            "branch": self.branch,
            "file_ids": self
                .files
                .iter()
                .map(|file| file.file_id().hyphenated().to_string())
                .collect::<Vec<_>>(),
            "paths": self
                .files
                .iter()
                .map(|file| file.path().as_str().to_owned())
                .collect::<Vec<_>>(),
            "checksums": self
                .files
                .iter()
                .map(|file| file.checksum().to_owned())
                .collect::<Vec<_>>(),
            "promoted_file_set_digest": self.digest.as_str(),
        })
    }

    /// Decodes durable promotion parameters back into an exact plan.
    ///
    /// Decoding is the worker's first fail-closed boundary: a malformed shape,
    /// a misaligned array, an unparsable field, or a digest that does not
    /// reproduce from the decoded files means the persisted parameters no
    /// longer describe a promotable group, and no catalog work may follow.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Invariant`] when the key set is not exactly the
    /// six declared fields, the `kind` is not `scribe_promotion`, the branch is
    /// not [`PROMOTION_BRANCH`], a required field is not the expected type, the
    /// three arrays differ in length, a field fails its own validation, or the
    /// recomputed digest differs from the persisted one.
    pub(super) fn from_parameters(parameters: &Map<String, Value>) -> Result<Self, ForgeError> {
        fn invariant(detail: &str) -> ForgeError {
            ForgeError::Invariant {
                detail: format!("Scribe promotion parameters {detail}"),
            }
        }
        fn strings(
            parameters: &Map<String, Value>,
            field: &str,
        ) -> Result<Vec<String>, ForgeError> {
            parameters
                .get(field)
                .and_then(Value::as_array)
                .ok_or_else(|| invariant(&format!("lack the {field} array")))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invariant(&format!("hold a non-string {field} entry")))
                })
                .collect()
        }
        // The parameter object is closed. A digest computed from the fields
        // this decoder reads cannot speak for a field it does not read, so an
        // unknown key is an undecodable shape rather than harmless noise.
        const DECLARED_KEYS: [&str; 6] = [
            "kind",
            "branch",
            "file_ids",
            "paths",
            "checksums",
            "promoted_file_set_digest",
        ];
        if parameters.len() != DECLARED_KEYS.len()
            || parameters
                .keys()
                .any(|key| !DECLARED_KEYS.contains(&key.as_str()))
        {
            return Err(invariant("are not the exact declared six-field set"));
        }
        if parameters.get("kind").and_then(Value::as_str) != Some(SCRIBE_PROMOTION_PARAMETER_KIND) {
            return Err(invariant("do not name the promotion kind"));
        }
        let branch = parameters
            .get("branch")
            .and_then(Value::as_str)
            .ok_or_else(|| invariant("lack a branch"))?;
        // The digest proves which files are promoted, never where. Forge owns
        // exactly one branch, so any other destination is a demand this worker
        // must not act on however self-consistent it looks.
        if branch != PROMOTION_BRANCH {
            return Err(invariant("name a branch Forge does not promote to"));
        }
        let file_ids = strings(parameters, "file_ids")?;
        let paths = strings(parameters, "paths")?;
        let checksums = strings(parameters, "checksums")?;
        if file_ids.len() != paths.len() || file_ids.len() != checksums.len() {
            return Err(invariant("hold misaligned file, path, and checksum arrays"));
        }
        let persisted = parameters
            .get("promoted_file_set_digest")
            .and_then(Value::as_str)
            .ok_or_else(|| invariant("lack the promoted file set digest"))?;
        let mut files = Vec::with_capacity(file_ids.len());
        for ((raw_id, raw_path), checksum) in file_ids.iter().zip(&paths).zip(&checksums) {
            let file_id =
                Uuid::parse_str(raw_id).map_err(|_| invariant("hold an unparsable file id"))?;
            let path = StoragePath::new(raw_path.as_str())
                .map_err(|_| invariant("hold a non-canonical logical path"))?;
            let file = ForgePromotedFile::new(file_id, path, checksum.as_str())
                .map_err(|_| invariant("hold an invalid promoted file"))?;
            files.push(file);
        }
        let plan = Self::new(branch, files)?;
        if plan.digest.as_str() != persisted {
            return Err(invariant(
                "carry a digest that does not reproduce from their own files",
            ));
        }
        Ok(plan)
    }
}

/// Names the first persisted metric that disagrees with the object's own footer.
///
/// Returning the field rather than a boolean is what makes a refusal
/// actionable: the operator learns which dimension of the evidence is wrong,
/// which is the difference between "this object cannot be promoted" and a
/// diagnosis.
fn first_metric_disagreement(
    physical: &crate::scribe::promotion::ScribeDataFileV1,
    claimed: &crate::scribe::promotion::ScribeDataFileV1,
) -> Option<&'static str> {
    if physical.record_count != claimed.record_count {
        return Some("record count");
    }
    if physical.file_size_in_bytes != claimed.file_size_in_bytes {
        return Some("file size");
    }
    if physical.column_sizes != claimed.column_sizes {
        return Some("column sizes");
    }
    if physical.value_counts != claimed.value_counts {
        return Some("value counts");
    }
    if physical.null_value_counts != claimed.null_value_counts {
        return Some("null value counts");
    }
    if physical.nan_value_counts != claimed.nan_value_counts {
        return Some("NaN value counts");
    }
    if physical.lower_bounds != claimed.lower_bounds {
        return Some("lower bounds");
    }
    if physical.upper_bounds != claimed.upper_bounds {
        return Some("upper bounds");
    }
    if physical.split_offsets != claimed.split_offsets {
        return Some("split offsets");
    }
    None
}

/// Reads one event-time bound of the object's own footer as epoch microseconds.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the bound is absent or is not the
/// timestamp literal the partitioned column must produce, either of which means
/// the object cannot be placed in a time partition at all.
fn event_time_bound_micros(
    bounds: &std::collections::HashMap<i32, iceberg::spec::Datum>,
    field_id: i32,
    edge: &str,
) -> Result<i64, ForgeError> {
    let datum = bounds.get(&field_id).ok_or_else(|| ForgeError::Invariant {
        detail: format!("promoted object footer carries no {edge} event-time bound"),
    })?;
    match datum.literal() {
        iceberg::spec::PrimitiveLiteral::Long(micros) => Ok(*micros),
        other => Err(ForgeError::Invariant {
            detail: format!(
                "promoted object {edge} event-time bound is not a timestamp: {other:?}"
            ),
        }),
    }
}

/// Proves one immutable object physically agrees with the evidence promoting it.
///
/// The checks run cheapest-and-most-decisive first: table policy identity needs
/// no IO at all, the footer decode needs one bounded parse, and the partition
/// containment check needs the projection the decode already produced. Every
/// one of them fails before the caller reaches the catalog.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the record names a partition spec or
/// sort order the table no longer carries, the footer cannot be decoded under
/// the writer's bounds, the footer's schema fingerprint differs from the
/// record's, any persisted metric disagrees with the footer, or the object's own
/// event-time bounds fall outside the partition the record claims.
fn validate_promoted_object(
    record: &ScribePublishedHotFileV1,
    bytes: &bytes::Bytes,
    file_size: u64,
    table: &iceberg::table::Table,
) -> Result<(), ForgeError> {
    let metadata = table.metadata();
    if record.partition_spec_id != metadata.default_partition_spec_id() {
        return Err(ForgeError::Invariant {
            detail: format!(
                "promotion evidence names partition spec {} but the table now writes {}",
                record.partition_spec_id,
                metadata.default_partition_spec_id()
            ),
        });
    }
    let table_sort_order = i32::try_from(metadata.default_sort_order().order_id).map_err(|_| {
        ForgeError::Invariant {
            detail: "table sort order id is not representable".to_owned(),
        }
    })?;
    if record.sort_order_id != table_sort_order {
        return Err(ForgeError::Invariant {
            detail: format!(
                "promotion evidence names sort order {} but the table now writes {table_sort_order}",
                record.sort_order_id
            ),
        });
    }

    let schema = metadata.current_schema();
    let footer = crate::parquet::PromotedObjectFooter::decode(
        bytes,
        &record.object_key,
        std::sync::Arc::clone(schema),
        file_size,
    )
    .map_err(|detail| ForgeError::Invariant { detail })?;
    if footer.schema_fingerprint() != record.schema_fingerprint {
        return Err(ForgeError::Invariant {
            detail: format!(
                "promoted object {} was sealed against another schema than its evidence claims",
                record.object_key
            ),
        });
    }
    let physical = footer
        .metrics()
        .map_err(|detail| ForgeError::Invariant { detail })?;
    if let Some(field) = first_metric_disagreement(&physical, &record.data_file) {
        return Err(ForgeError::Invariant {
            detail: format!(
                "promoted object {} disagrees with its evidence on {field}",
                record.object_key
            ),
        });
    }

    let partition = crate::catalog::TimePartition::from_durable_columns(
        &record.partition.granularity,
        record.partition.start_utc,
    )
    .map_err(|error| ForgeError::Invariant {
        detail: format!("promotion evidence names a non-canonical partition: {error}"),
    })?;
    let field_id = schema
        .field_by_name(wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME)
        .ok_or_else(|| ForgeError::Invariant {
            detail: "table schema carries no event-time column to partition on".to_owned(),
        })?
        .id;
    let lowest = event_time_bound_micros(footer.data_file().lower_bounds(), field_id, "lower")?;
    let highest = event_time_bound_micros(footer.data_file().upper_bounds(), field_id, "upper")?;
    let start = partition.start_utc().timestamp_micros();
    let end = partition.end_utc().timestamp_micros();
    if lowest < start || highest >= end {
        return Err(ForgeError::Invariant {
            detail: format!(
                "promoted object {} holds event times outside the partition its evidence claims",
                record.object_key
            ),
        });
    }
    Ok(())
}

/// Reads and validates the exact promotion demand one table currently owes.
///
/// Every eligible `file_list` row must carry decodable
/// [`ScribePublishedHotFileV1`] evidence bound to the same tenant, table, row
/// identity, path, and checksum the row itself records. A row that fails any
/// of those checks invalidates the whole group rather than shrinking it: a
/// partial promotion would publish a file set whose digest no longer matches
/// the demand the scheduler observed.
///
/// # Errors
///
/// Returns [`ForgeError::Sql`] when the tenant-scoped read fails and
/// [`ForgeError::Invariant`] when a row's promotion evidence is absent,
/// undecodable, or contradicts the durable row it is bound to.
pub(super) async fn read_promotion_demand(
    conn: &mut wyrd_sql::TenantConn<'_>,
    binding: &TenantTableBinding,
    branch: &str,
    only: Option<&BTreeSet<Uuid>>,
) -> Result<Option<ScribePromotionDemand>, ForgeError> {
    let rows = HotFileCatalog::new(
        &binding.table_ref.namespace.to_string(),
        &binding.table_ref.name,
    )
    .list_promotable(conn)
    .await
    .map_err(ForgeError::Sql)?;
    let rows: Vec<_> = match only {
        // Revalidation asks about the group it planned. Hot objects Scribe
        // published after planning are new demand for the next pass, not a
        // contradiction of this plan, so they are excluded here rather than
        // allowed to fail the digest comparison.
        Some(ids) => rows
            .into_iter()
            .filter(|row| ids.contains(&row.id))
            .collect(),
        None => rows,
    };
    if rows.is_empty() {
        return Ok(None);
    }
    let mut files = Vec::with_capacity(rows.len());
    let mut records = Vec::with_capacity(rows.len());
    let mut total_bytes = 0_u64;
    for row in &rows {
        total_bytes = total_bytes.saturating_add(u64::try_from(row.file_size).map_err(|_| {
            ForgeError::Invariant {
                detail: format!("hot object {} records a negative size", row.file_path),
            }
        })?);
        let record =
            ScribePublishedHotFileV1::from_json(&row.promotion_record).map_err(|error| {
                ForgeError::Invariant {
                    detail: format!(
                        "hot object {} carries undecodable promotion evidence: {error}",
                        row.file_path
                    ),
                }
            })?;
        if record.file_list_id != row.id
            || record.object_key != row.file_path
            || record.file_checksum != row.file_checksum
            || record.data_tenant_id != Uuid::from(binding.tenant)
            || record.namespace != binding.table_ref.namespace.to_string()
            || record.table_name != binding.table_ref.name
        {
            return Err(ForgeError::Invariant {
                detail: format!(
                    "hot object {} carries promotion evidence bound to another row",
                    row.file_path
                ),
            });
        }
        let path =
            StoragePath::new(row.file_path.as_str()).map_err(|error| ForgeError::Invariant {
                detail: format!("hot object path is not audit-safe: {error}"),
            })?;
        let file =
            ForgePromotedFile::new(row.id, path, row.file_checksum.as_str()).map_err(|error| {
                ForgeError::Invariant {
                    detail: format!("hot object cannot be promoted: {error}"),
                }
            })?;
        files.push(file);
        records.push(record);
    }
    ScribePromotionPlan::new(branch, files).map(|plan| {
        Some(ScribePromotionDemand {
            plan,
            total_bytes,
            records,
        })
    })
}

/// The complete input to one promotion fast-append.
///
/// The identities travel together because they are only meaningful together:
/// the snapshot summary must carry the task and operation that produced this
/// exact file set, and the attempt only labels the telemetry span. Grouping
/// them also keeps the call site honest — a caller cannot silently transpose
/// two `Uuid` arguments.
pub(super) struct ForgePromotionCommit<'a> {
    /// Table state the append is built against.
    pub(super) table: &'a iceberg::table::Table,
    /// Writer-owned `DataFile` values appended unchanged.
    pub(super) data_files: Vec<iceberg::spec::DataFile>,
    /// Durable task that owns this promotion.
    pub(super) task_id: Uuid,
    /// Attempt used only to label the commit span.
    pub(super) attempt_id: Uuid,
    /// Operation identity recovery and Oracle match on.
    pub(super) operation_id: Uuid,
}

/// One promotion audit transition and the publication it settles with.
///
/// The phase, the snapshot it claims, and the plan it claims it for are one
/// decision: a terminal phase carrying no snapshot means "nothing landed", and
/// a committed snapshot without its plan cannot settle a row. Passing them as
/// one value keeps the two halves from drifting apart at a call site.
pub(super) struct ForgePromotionSettlement<'a> {
    /// Exact ordered group this settlement is about.
    pub(super) plan: &'a ScribePromotionPlan,
    /// Durable phase this transition records.
    pub(super) phase: ForgeScribePromotionPhase,
    /// Operation identity shared by every phase of one promotion.
    pub(super) operation_id: Uuid,
    /// Snapshot the promotion was planned against.
    pub(super) base_snapshot_id: i64,
    /// Snapshot that now represents the group, when one landed.
    pub(super) committed_snapshot_id: Option<i64>,
}


/// What a claimed promotion plan still means against durable state.
///
/// A promotion task is planned against the rows one scheduler pass observed.
/// Between that pass and the worker's claim, a sibling task can commit the same
/// group and settle those rows, because the Iceberg commit and the SQL
/// settlement are two steps and a plan can be cut between them. The claimed
/// task must then distinguish "my group is still owed" from "my group already
/// landed", and only the first is work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromotionPlanStatus {
    /// Every planned row is still unsettled, so the plan is exactly owed.
    Current,
    /// Some or all planned rows already landed under another operation.
    ///
    /// The claim is stale rather than wrong: the effect it would perform has
    /// been performed. Its caller cancels it as superseded and leaves any
    /// remaining demand to the next planning pass.
    Superseded,
}

impl Forge {
    /// Classifies one claimed promotion plan against its own durable rows.
    ///
    /// This runs before the Prepared transition, because Prepared is a promise
    /// that this exact group will be appended and a superseded group must never
    /// make that promise. The question asked is deliberately narrow: what
    /// happened to *these* `file_list` identities? Rows that are still
    /// unsettled mean the plan is owed unchanged. Rows carrying a committed
    /// snapshot mean a sibling promotion already appended them, which is proven
    /// by finding their paths alive in the table's current snapshot before the
    /// claim is retired — settlement alone is a SQL claim, and the catalog is
    /// the authority on whether the data is actually there.
    ///
    /// Partial settlement is also [`PromotionPlanStatus::Superseded`]: this
    /// plan's digest binds the whole ordered group, so it can no longer be
    /// appended as planned, and the unsettled remainder is ordinary demand for
    /// the next planning pass rather than something to salvage here.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the tenant-scoped read fails,
    /// [`ForgeError::Catalog`] when the current snapshot's manifests cannot be
    /// read, and [`ForgeError::Reconciliation`] for the two states that are not
    /// stale but lost: a planned row that disappeared without durable
    /// settlement, and a settled row whose path is absent from the current
    /// snapshot.
    pub(super) async fn classify_planned_promotion(
        &self,
        binding: &TenantTableBinding,
        plan: &ScribePromotionPlan,
        table: &iceberg::table::Table,
    ) -> Result<PromotionPlanStatus, ForgeError> {
        let planned = plan.file_ids();
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let rows = HotFileCatalog::new(
            &binding.table_ref.namespace.to_string(),
            &binding.table_ref.name,
        )
        .planned_settlement(&mut conn, &planned)
        .await
        .map_err(ForgeError::Sql)?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        if rows.len() != planned.len() {
            let present: BTreeSet<Uuid> = rows.iter().map(|row| row.id).collect();
            let missing: Vec<Uuid> = planned
                .iter()
                .filter(|id| !present.contains(id))
                .copied()
                .collect();
            return Err(ForgeError::Reconciliation {
                detail: format!(
                    "planned Scribe promotion rows disappeared without durable settlement: {missing:?}"
                ),
            });
        }
        let settled: Vec<&vala_sql::queries::file_list::PlannedHotFileRow> = rows
            .iter()
            .filter(|row| row.committed_snapshot_id.is_some())
            .collect();
        if settled.is_empty() {
            return Ok(PromotionPlanStatus::Current);
        }
        let live = self.live_data_object_keys(binding, table).await?;
        for row in settled {
            if !live.contains(&row.file_path) {
                return Err(ForgeError::Reconciliation {
                    detail: format!(
                        "planned Scribe promotion row {} records committed snapshot {:?} but its path is absent from the current snapshot",
                        row.file_path, row.committed_snapshot_id
                    ),
                });
            }
        }
        Ok(PromotionPlanStatus::Superseded)
    }

    /// Collects the object keys of every live data file in the current snapshot.
    ///
    /// Promotion settlement is only believable if the catalog agrees, so the
    /// comparison is made against the paths the table actually serves. Catalog
    /// paths are normalized into the tenant's object keys, which is the form
    /// `vala.file_list` records, so the two sides are comparable without
    /// guessing at prefixes. An empty current snapshot yields an empty set,
    /// which correctly makes any claimed settlement unprovable.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Catalog`] when the manifest list or a manifest
    /// cannot be read, and [`ForgeError::Invariant`] when a catalog path does
    /// not normalize into this binding's object prefix.
    async fn live_data_object_keys(
        &self,
        binding: &TenantTableBinding,
        table: &iceberg::table::Table,
    ) -> Result<BTreeSet<String>, ForgeError> {
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Ok(BTreeSet::new());
        };
        let table_location = table.metadata().location();
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .map_err(ForgeError::Catalog)?;
        let mut live = BTreeSet::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .map_err(ForgeError::Catalog)?;
            for entry in manifest
                .entries()
                .iter()
                .filter(|entry| entry.is_alive())
                .filter(|entry| entry.content_type() == iceberg::spec::DataContentType::Data)
            {
                live.insert(super::path::catalog_path_to_object_key(
                    table_location,
                    binding,
                    &self.core.staging,
                    entry.file_path(),
                )?);
            }
        }
        Ok(live)
    }

    /// Revalidates one prepared promotion group against the objects that exist.
    ///
    /// Revalidation is what makes the append safe to perform unchanged: the
    /// durable demand is re-read under the worker's own fence, checked against
    /// the plan the scheduler persisted, and then each object is checked
    /// against its own evidence — physically. The order matters. The durable
    /// demand is settled first because a diverged demand makes every later
    /// check meaningless; then the object's identity (key, size, checksum);
    /// then the object's own footer, re-derived into the same `DataFile`
    /// projection Scribe recorded and compared field for field; then the
    /// identity the table's current policy requires (schema fingerprint,
    /// partition spec, sort order); and finally the partition value, which is
    /// checked against the object's own event-time bounds so a record cannot
    /// claim a window its rows do not fall in.
    ///
    /// A size-and-checksum check alone would accept every one of those
    /// contradictions: the bytes are unchanged in all of them, and only the
    /// evidence about the bytes is wrong. Only the object bytes are read; no
    /// data object is written, and the appended `DataFile` is still the value
    /// Scribe recorded rather than one recomputed here.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::Sql`] when the demand cannot be re-read,
    /// [`ForgeError::Reconciliation`] when the durable demand no longer matches
    /// the prepared plan, [`ForgeError::ObjectStore`] when an object cannot be
    /// read, and [`ForgeError::Invariant`] when an object contradicts its own
    /// promotion evidence, disagrees with its own footer, names identity the
    /// table no longer carries, or its `DataFile` cannot be rebuilt.
    pub(super) async fn revalidate_promotion(
        &self,
        binding: &TenantTableBinding,
        plan: &ScribePromotionPlan,
        table: &iceberg::table::Table,
    ) -> Result<Vec<iceberg::spec::DataFile>, ForgeError> {
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        let planned: BTreeSet<Uuid> = plan.file_ids().into_iter().collect();
        let demand = read_promotion_demand(&mut conn, binding, plan.branch(), Some(&planned))
            .await?;
        conn.commit().await.map_err(ForgeError::Sql)?;
        let demand = demand.ok_or_else(|| ForgeError::Reconciliation {
            detail: "prepared Scribe promotion has no durable demand left".to_owned(),
        })?;
        if &demand.plan != plan {
            return Err(ForgeError::Reconciliation {
                detail: "durable Scribe promotion demand diverged from the prepared plan"
                    .to_owned(),
            });
        }
        let mut data_files = Vec::with_capacity(demand.records.len());
        for record in &demand.records {
            let bytes = self
                .core
                .object_store
                .read(&record.object_key)
                .await
                .map_err(ForgeError::ObjectStore)?;
            let observed_checksum = {
                use sha2::Digest as _;
                hex::encode(sha2::Sha256::digest(bytes.to_bytes()))
            };
            let file_size = u64::try_from(bytes.len()).map_err(|_| ForgeError::Invariant {
                detail: format!("hot object {} size exceeds u64", record.object_key),
            })?;
            record
                .validate_object(&crate::scribe::promotion::ObservedHotObject {
                    object_key: &record.object_key,
                    file_size,
                    file_checksum: &observed_checksum,
                })
                .map_err(|error| ForgeError::Invariant {
                    detail: format!("hot object failed promotion revalidation: {error}"),
                })?;
            validate_promoted_object(record, &bytes.to_bytes(), file_size, table)?;
            data_files.push(record.data_file().map_err(|error| ForgeError::Invariant {
                detail: format!("promotion evidence does not rebuild a data file: {error}"),
            })?);
        }
        Ok(data_files)
    }

    /// Fast-appends one revalidated promotion group under the publication fence.
    ///
    /// The commit carries the workflow, task, and operation identities as
    /// snapshot summary properties. The workflow name is what keeps rewrite
    /// reconciliation off these snapshots: it scans every retained snapshot on
    /// the table and would otherwise read a promotion's bare operation identity
    /// as a malformed rewrite identity rather than as another workflow's. Those two properties are the entire basis of recovery: a
    /// worker that loses acceptance can prove the commit landed by finding its
    /// own task identity on a retained snapshot, and Oracle closes the
    /// catalog-to-SQL window by matching the operation identity on the snapshot
    /// it pinned. Duplicate checking stays on, so a replayed append of an
    /// already-promoted object is refused by the catalog rather than creating a
    /// second reference to one file.
    ///
    /// Cancellation and timeout are raced against the in-flight commit and are
    /// never read as proof of rejection: both surface as
    /// [`ForgeError::Reconciliation`] so the caller's Prepared operation stays
    /// open for evidence-based recovery.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when the lease cannot cover the commit
    /// window, [`ForgeError::Catalog`] when the catalog refuses the append, and
    /// [`ForgeError::Reconciliation`] when acceptance becomes unknown.
    ///
    /// # Cancellation
    ///
    /// Cancellation after submission leaves acceptance unknown and is reported
    /// as a reconciliation error, never as a clean stop.
    pub(super) async fn commit_promotion(
        &self,
        lease: &mut ForgeLease,
        commit: ForgePromotionCommit<'_>,
        stop: &CancellationToken,
    ) -> Result<iceberg::table::Table, ForgeError> {
        let ForgePromotionCommit {
            table,
            data_files,
            task_id,
            attempt_id,
            operation_id,
        } = commit;
        let span = super::publication::catalog_commit_span(
            "scribe_promotion",
            Some((task_id, attempt_id)),
        );
        if !lease.renew(&self.core.operator_pool).await?
            || !lease.commit_window_fits(self.core.config.commit_window())
        {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let transaction = iceberg::transaction::Transaction::new(table);
        let action = transaction
            .fast_append()
            .with_check_duplicate(true)
            .add_data_files(data_files)
            .set_snapshot_properties(std::collections::HashMap::from([
                ("forge.workflow".to_owned(), "scribe-promotion".to_owned()),
                ("forge.task_id".to_owned(), task_id.to_string()),
                ("forge.operation_id".to_owned(), operation_id.to_string()),
            ]));
        let transaction = iceberg::transaction::ApplyTransactionAction::apply(action, transaction)
            .map_err(ForgeError::Catalog)?;
        lease.require_fence(&self.core.operator_pool).await?;
        if !lease.commit_window_fits(self.core.config.commit_window()) {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        if stop.is_cancelled() {
            return Err(ForgeError::Shutdown);
        }
        let timeout = self.core.config.iceberg_total_retry_timeout;
        let catalog = self.core.catalog.as_ref();
        let outcome = async move {
            let commit = transaction.commit(catalog);
            tokio::pin!(commit);
            tokio::select! {
                response = tokio::time::timeout(timeout, &mut commit) => match response {
                    Ok(Ok(committed)) => Ok(committed),
                    Ok(Err(error)) => Err(ForgeError::Catalog(error)),
                    Err(_) => Err(ForgeError::Reconciliation {
                        detail: "Scribe promotion commit timed out with unknown acceptance"
                            .to_owned(),
                    }),
                },
                () = stop.cancelled() => Err(ForgeError::Reconciliation {
                    detail: "Scribe promotion commit was cancelled with unknown acceptance"
                        .to_owned(),
                }),
            }
        }
        .instrument(span.clone())
        .await;
        span.record(
            "result",
            if outcome.is_ok() {
                "committed"
            } else {
                "failed"
            },
        );
        outcome
    }

    /// Settles one promotion's audit transition and hot-row publication together.
    ///
    /// Audit evidence and the `file_list` publication columns become durable in
    /// the same fenced tenant transaction, so the catalog-to-SQL window can end
    /// in exactly one of two states: neither is written and Oracle keeps
    /// scanning the objects as hot, or both are written and Oracle reads them
    /// from the promoted snapshot. There is no interval in which a row is
    /// counted twice or not at all.
    ///
    /// The settlement is idempotent by construction: the operation-state
    /// transition collapses a repeated append, and the publication update
    /// matches rows that are either unpublished or already published by this
    /// exact operation. A takeover that repeats a settled promotion therefore
    /// commits the same durable state rather than a second one.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::FenceLost`] when the lease cannot be renewed or no
    /// longer holds inside the transaction, and [`ForgeError::Sql`] when the
    /// tenant transaction, audit append, publication update, or commit fails.
    pub(super) async fn settle_promotion(
        &self,
        lease: &mut ForgeLease,
        binding: &TenantTableBinding,
        settlement: ForgePromotionSettlement<'_>,
    ) -> Result<(), ForgeError> {
        let ForgePromotionSettlement {
            plan,
            phase,
            operation_id,
            base_snapshot_id,
            committed_snapshot_id,
        } = settlement;
        if !lease.renew(&self.core.operator_pool).await? {
            return Err(ForgeError::FenceLost {
                lease_key: lease.lease_key.clone(),
            });
        }
        let resource = ForgeGroupKey::table_audit_resource(binding.tenant, &binding.table_ref);
        let detail = AuditDetail::ForgeScribePromotion {
            operation_id,
            phase,
            group: resource.clone(),
            base_snapshot_id,
            committed_snapshot_id,
            input_file_ids: plan.file_ids(),
            input_paths: plan.paths(),
            promoted_file_set_digest: plan.digest().clone(),
        };
        let operation = format!(
            "{}.{}",
            ForgeOperationFamily::ScribePromotion.operation_prefix(),
            phase_suffix(phase)
        );
        let mut conn = self
            .core
            .vala
            .tenant_conn(binding.tenant)
            .await
            .map_err(ForgeError::Sql)?;
        lease.require_fence(&self.core.operator_pool).await?;
        let operations = ForgeOperations::new(&resource, ForgeOperationFamily::ScribePromotion)
            .map_err(ForgeError::Sql)?;
        let transition = if phase == ForgeScribePromotionPhase::Prepared {
            operations
                .append_prepared(&mut conn, &operation, &detail)
                .await
        } else {
            operations
                .append_terminal(&mut conn, &operation, &detail)
                .await
        }
        .map_err(ForgeError::Sql)?;
        match transition {
            ForgeOperationTransition::Applied | ForgeOperationTransition::AlreadyApplied => {}
        }
        if let Some(snapshot_id) = committed_snapshot_id {
            HotFileCatalog::new(
                &binding.table_ref.namespace.to_string(),
                &binding.table_ref.name,
            )
            .settle_promoted(&mut conn, &plan.file_ids(), snapshot_id, operation_id)
            .await
            .map_err(ForgeError::Sql)?;
        }
        lease.assert_transaction_fence(&mut conn).await?;
        conn.commit().await.map_err(ForgeError::Sql)
    }
}

/// Returns the operation-name suffix for one promotion phase.
///
/// The suffix completes the `forge.scribe_promotion` operation prefix, so the
/// durable operation name and the audit detail's phase can never disagree.
const fn phase_suffix(phase: ForgeScribePromotionPhase) -> &'static str {
    match phase {
        ForgeScribePromotionPhase::Prepared => "prepared",
        ForgeScribePromotionPhase::Committed => "committed",
        ForgeScribePromotionPhase::Recovered => "recovered",
        ForgeScribePromotionPhase::Reset => "reset",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one deterministic promoted file for parameter round-trip proofs.
    fn file(seed: u128, name: &str) -> ForgePromotedFile {
        ForgePromotedFile::new(
            Uuid::from_u128(seed),
            StoragePath::new(format!("events/{name}.parquet")).expect("canonical path"),
            "a".repeat(64),
        )
        .expect("valid promoted file")
    }

    /// Durable parameters round-trip exactly and reject every altered shape.
    ///
    /// # Panics
    ///
    /// Panics when the canonical plan does not round-trip or an altered
    /// parameter shape is accepted.
    #[test]
    fn promotion_parameters_round_trip_and_reject_altered_shapes() {
        let plan = ScribePromotionPlan::new("main", vec![file(1, "a"), file(2, "b")])
            .expect("canonical plan");
        let parameters = plan.to_parameters();
        let object = parameters.as_object().expect("parameters are an object");
        assert_eq!(
            ScribePromotionPlan::from_parameters(object).expect("round trip"),
            plan
        );

        for mutate in [
            |object: &mut Map<String, Value>| {
                object.insert("kind".to_owned(), Value::from("promotion_v2"));
            },
            // An unknown key is a parameter shape this decoder does not
            // understand. Ignoring it would let a writer smuggle a directive
            // past a digest that only covers the fields the decoder reads.
            |object: &mut Map<String, Value>| {
                object.insert("expire_snapshots".to_owned(), Value::from(true));
            },
            // A foreign branch with a self-consistent digest: the digest proves
            // the file set, never the destination, so only an exact branch
            // check keeps a promotion off a ref Forge does not own.
            |object: &mut Map<String, Value>| {
                let plan = ScribePromotionPlan::new("audit", vec![file(1, "a"), file(2, "b")])
                    .expect("canonical plan on another branch");
                *object = plan
                    .to_parameters()
                    .as_object()
                    .expect("parameters are an object")
                    .clone();
            },
            |object: &mut Map<String, Value>| {
                object.insert("checksums".to_owned(), serde_json::json!(["a".repeat(64)]));
            },
            |object: &mut Map<String, Value>| {
                object.insert(
                    "promoted_file_set_digest".to_owned(),
                    Value::from(format!("sha256:{}", "0".repeat(64))),
                );
            },
            |object: &mut Map<String, Value>| {
                let reversed = object
                    .get("file_ids")
                    .and_then(Value::as_array)
                    .expect("file ids")
                    .iter()
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>();
                object.insert("file_ids".to_owned(), Value::from(reversed));
            },
        ] {
            let mut altered = object.clone();
            mutate(&mut altered);
            assert!(
                ScribePromotionPlan::from_parameters(&altered).is_err(),
                "an altered promotion parameter shape must be refused"
            );
        }

        // Every declared key is load-bearing: dropping any one of them leaves
        // parameters that cannot be proven to describe the planned group.
        for field in [
            "kind",
            "branch",
            "file_ids",
            "paths",
            "checksums",
            "promoted_file_set_digest",
        ] {
            let mut absent = object.clone();
            absent.remove(field);
            assert!(
                ScribePromotionPlan::from_parameters(&absent).is_err(),
                "promotion parameters missing {field} must be refused"
            );
        }
    }
}
