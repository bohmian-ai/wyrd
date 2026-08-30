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
use uuid::Uuid;
use vala_sql::queries::file_list::HotFileCatalog;
use wyrd_spec::vala::api::{ForgePromotedFile, ForgePromotedFileSetDigest, StoragePath};

use super::error::ForgeError;
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
    /// Returns [`ForgeError::Invariant`] when the `kind` is not
    /// `scribe_promotion`, a required field is absent or not the expected
    /// type, the three arrays differ in length, a field fails its own
    /// validation, or the recomputed digest differs from the persisted one.
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
        if parameters.get("kind").and_then(Value::as_str) != Some(SCRIBE_PROMOTION_PARAMETER_KIND) {
            return Err(invariant("do not name the promotion kind"));
        }
        let branch = parameters
            .get("branch")
            .and_then(Value::as_str)
            .ok_or_else(|| invariant("lack a branch"))?;
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
) -> Result<Option<ScribePromotionDemand>, ForgeError> {
    let rows = HotFileCatalog::new(
        &binding.table_ref.namespace.to_string(),
        &binding.table_ref.name,
    )
    .list_promotable(conn)
    .await
    .map_err(ForgeError::Sql)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut files = Vec::with_capacity(rows.len());
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
    }
    ScribePromotionPlan::new(branch, files)
        .map(|plan| Some(ScribePromotionDemand { plan, total_bytes }))
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
    }
}
