//! Promotion evidence — what Scribe records about one committed hot object.
//!
//! A published hot object is already described by its `vala.file_list` row, but
//! that row answers Oracle's question ("which files cover this range"), not the
//! catalog's ("what does this file contain"). Promotion into an Iceberg table
//! needs the second answer: the exact `DataFile` — partition value, record
//! count, per-column sizes, value/null/NaN counts, and lower/upper bounds — as
//! the writer that closed the footer computed it.
//!
//! [`ScribePublishedHotFileV1`] is that answer, persisted once, in the same
//! fenced transaction that publishes the row. It is derived from the closed
//! footer of the object Scribe actually sealed and is revalidated against the
//! uploaded bytes before the transaction runs, so a consumer never has to
//! reconstruct statistics from a file-list row or re-read the object to trust
//! them. It performs no catalog promotion itself; it is the evidence a promoter
//! reads.
//!
//! The record is a projection rather than a serialized `DataFile` because
//! [`iceberg::spec::DataFile`] has no serde representation of its own: its Avro
//! form needs a partition type and format version that only a loaded table
//! carries. Every field this projection keeps is one a `DataFile` round-trips
//! exactly, and [`ScribePublishedHotFileV1::data_file`] rebuilds it; anything a
//! `DataFile` could carry that Scribe never produces is refused at derivation
//! rather than silently dropped.

use std::collections::{BTreeMap, HashMap};

use iceberg::spec::{
    DataContentType, DataFile, DataFileBuilder, DataFileFormat, Datum, PrimitiveType, Struct,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::catalog::layout::{TimeGranularity, TimePartition};
use crate::contracts::ScribeError;

/// Version stamped into every promotion record this writer produces.
///
/// The record is unshipped, so there is exactly one version and a record that
/// names any other value is refused rather than upgraded.
pub const SCRIBE_PROMOTION_RECORD_VERSION: u16 = 1;

/// One column bound as the closed footer expressed it.
///
/// The primitive type travels with the bytes so a consumer can rebuild the
/// `Datum` without resolving the table schema first; binary single-value
/// serialization is type-directed, and bytes alone would be ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeColumnBoundV1 {
    /// Iceberg primitive type the bound bytes decode as.
    pub primitive_type: PrimitiveType,
    /// Lowercase hex of the Iceberg binary single-value serialization.
    pub value_hex: String,
}

impl ScribeColumnBoundV1 {
    /// Projects one datum into its persisted form.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the datum has no binary
    /// single-value serialization, which means the writer produced a bound the
    /// promotion record cannot carry losslessly.
    fn from_datum(datum: &Datum) -> Result<Self, ScribeError> {
        let bytes = datum.to_bytes().map_err(|error| ScribeError::Internal {
            detail: format!("promotion bound is not serializable: {error}"),
        })?;
        Ok(Self {
            primitive_type: datum.data_type().clone(),
            value_hex: hex::encode(bytes.as_ref()),
        })
    }

    /// Rebuilds the exact datum this bound was projected from.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the hex is malformed or the bytes
    /// do not decode as the recorded primitive type.
    fn to_datum(&self) -> Result<Datum, ScribeError> {
        let bytes = hex::decode(&self.value_hex).map_err(|error| ScribeError::Internal {
            detail: format!("promotion bound is not hex: {error}"),
        })?;
        Datum::try_from_bytes(&bytes, self.primitive_type.clone()).map_err(|error| {
            ScribeError::Internal {
                detail: format!("promotion bound does not decode as its type: {error}"),
            }
        })
    }
}

/// The exact partition value one published object belongs to.
///
/// Scribe partitions on one time transform, so the durable form is the same
/// granularity/boundary pair `vala.file_list` stores; the Iceberg literal is
/// derived from it rather than persisted, which keeps the record readable and
/// makes a non-canonical boundary unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribePartitionValueV1 {
    /// Durable lowercase granularity token (`hour` or `day`).
    pub granularity: String,
    /// Exact UTC partition start boundary.
    pub start_utc: chrono::DateTime<chrono::Utc>,
}

impl ScribePartitionValueV1 {
    /// Projects one physical partition into its persisted form.
    fn from_partition(partition: TimePartition) -> Self {
        Self {
            granularity: partition.granularity_str().to_owned(),
            start_utc: partition.start_utc(),
        }
    }

    /// Rebuilds the exact partition this value was projected from.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the granularity token is unknown
    /// or the boundary is not canonical for it.
    fn to_partition(&self) -> Result<TimePartition, ScribeError> {
        let granularity = TimeGranularity::from_str_token(&self.granularity).ok_or_else(|| {
            ScribeError::Internal {
                detail: format!(
                    "promotion partition granularity is unknown: {}",
                    self.granularity
                ),
            }
        })?;
        TimePartition::new(granularity, self.start_utc).map_err(|error| ScribeError::Internal {
            detail: format!("promotion partition boundary is not canonical: {error}"),
        })
    }
}

/// The serializable projection of one Iceberg `DataFile`.
///
/// Maps are `BTreeMap` so the JSON a given object produces is byte-identical on
/// every restart and replay; `HashMap` iteration order would make an otherwise
/// deterministic record compare unequal to itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribeDataFileV1 {
    /// Rows the object contains.
    pub record_count: u64,
    /// Exact sealed object size in bytes.
    pub file_size_in_bytes: u64,
    /// Compressed on-disk size per Iceberg field id.
    pub column_sizes: BTreeMap<i32, u64>,
    /// Values, including nulls and NaNs, per Iceberg field id.
    pub value_counts: BTreeMap<i32, u64>,
    /// Null values per Iceberg field id.
    pub null_value_counts: BTreeMap<i32, u64>,
    /// NaN values per Iceberg field id.
    pub nan_value_counts: BTreeMap<i32, u64>,
    /// Lower bound per Iceberg field id.
    pub lower_bounds: BTreeMap<i32, ScribeColumnBoundV1>,
    /// Upper bound per Iceberg field id.
    pub upper_bounds: BTreeMap<i32, ScribeColumnBoundV1>,
    /// Ascending row-group offsets within the object.
    pub split_offsets: Vec<i64>,
}

impl ScribeDataFileV1 {
    /// Projects one closed-footer Iceberg `DataFile` into its stored form.
    ///
    /// Only the fields Scribe's own writer can produce survive the projection,
    /// and every map is ordered so that re-deriving the record from the same
    /// footer encodes byte-identically across restarts and replays.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the file is not a Parquet data
    /// file, omits row-group split offsets, carries delete-file or encryption
    /// state Scribe never writes, or has a bound with no binary serialization.
    pub fn from_data_file(data_file: &DataFile) -> Result<Self, ScribeError> {
        if data_file.content_type() != DataContentType::Data
            || data_file.file_format() != DataFileFormat::Parquet
        {
            return Err(ScribeError::Internal {
                detail: "promotion evidence requires a Parquet data file".to_owned(),
            });
        }
        if data_file.equality_ids().is_some()
            || data_file.key_metadata().is_some()
            || data_file.referenced_data_file().is_some()
            || data_file.content_offset().is_some()
            || data_file.content_size_in_bytes().is_some()
        {
            return Err(ScribeError::Internal {
                detail: "promotion evidence carries state Scribe never writes".to_owned(),
            });
        }
        let split_offsets = data_file
            .split_offsets()
            .ok_or_else(|| ScribeError::Internal {
                detail: "promotion evidence omits row-group split offsets".to_owned(),
            })?
            .to_vec();
        Ok(Self {
            record_count: data_file.record_count(),
            file_size_in_bytes: data_file.file_size_in_bytes(),
            column_sizes: sorted_counts(data_file.column_sizes()),
            value_counts: sorted_counts(data_file.value_counts()),
            null_value_counts: sorted_counts(data_file.null_value_counts()),
            nan_value_counts: sorted_counts(data_file.nan_value_counts()),
            lower_bounds: sorted_bounds(data_file.lower_bounds())?,
            upper_bounds: sorted_bounds(data_file.upper_bounds())?,
            split_offsets,
        })
    }
}

/// Identity a published object carries independently of its statistics.
///
/// Borrowed at derivation so the caller states the publication facts once; the
/// record owns them afterward.
pub struct PublishedHotFileIdentity<'a> {
    /// Tenant that owns the rows.
    pub data_tenant_id: Uuid,
    /// Logical namespace stored on the `file_list` row.
    pub namespace: &'a str,
    /// Logical table name stored on the `file_list` row.
    pub table_name: &'a str,
    /// Identity of the `file_list` row this record is bound to.
    pub file_list_id: Uuid,
    /// Object key the sealed artifact publishes under.
    pub object_key: &'a str,
    /// Lowercase SHA-256 of the sealed object bytes.
    pub file_checksum: &'a str,
    /// Physical partition the object belongs to.
    pub partition: TimePartition,
    /// Iceberg partition-spec identity the object was written under.
    pub partition_spec_id: i32,
    /// Iceberg sort-order identity the object was written under.
    pub sort_order_id: i32,
}

/// What a reader observed about the uploaded object, for revalidation.
///
/// The uploader already re-stats and re-digests the remote object, so this is
/// the evidence it produced rather than a second read; binding the record to it
/// is what makes "the record describes the bytes that exist" a checked claim
/// instead of an assumption.
pub struct ObservedHotObject<'a> {
    /// Object key the verified upload wrote.
    pub object_key: &'a str,
    /// Byte length the object store reported.
    pub file_size: u64,
    /// Lowercase SHA-256 the verifying read computed.
    pub file_checksum: &'a str,
}

/// One committed Scribe hot object's complete promotion evidence.
///
/// Persisted as `vala.file_list.promotion_record` in the fenced publication
/// transaction, one record per row, immutable afterward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScribePublishedHotFileV1 {
    /// Record version; always [`SCRIBE_PROMOTION_RECORD_VERSION`].
    pub version: u16,
    /// Tenant that owns the rows.
    pub data_tenant_id: Uuid,
    /// Logical namespace stored on the `file_list` row.
    pub namespace: String,
    /// Logical table name stored on the `file_list` row.
    pub table_name: String,
    /// Identity of the `file_list` row this record is bound to.
    pub file_list_id: Uuid,
    /// Object key of the published immutable object.
    pub object_key: String,
    /// Lowercase SHA-256 of the sealed object bytes.
    pub file_checksum: String,
    /// Iceberg partition-spec identity the object was written under.
    pub partition_spec_id: i32,
    /// Iceberg sort-order identity the object was written under.
    pub sort_order_id: i32,
    /// Physical partition value the object belongs to.
    pub partition: ScribePartitionValueV1,
    /// Iceberg-ready projection of the object's `DataFile`.
    pub data_file: ScribeDataFileV1,
}

impl ScribePublishedHotFileV1 {
    /// Binds writer-derived footer metrics to one object's publication identity.
    ///
    /// `data_file` must be the projection the sealing writer took from the
    /// footer it closed, not a reconstruction: everything this record promises
    /// about column metrics is exactly what that footer carried. Partition,
    /// spec, and sort identity come from `identity` rather than from the
    /// writer because they are publication facts — the writer seals an object
    /// before anything decides which fenced row will name it.
    pub fn from_metrics(
        identity: &PublishedHotFileIdentity<'_>,
        data_file: ScribeDataFileV1,
    ) -> Self {
        Self {
            version: SCRIBE_PROMOTION_RECORD_VERSION,
            data_tenant_id: identity.data_tenant_id,
            namespace: identity.namespace.to_owned(),
            table_name: identity.table_name.to_owned(),
            file_list_id: identity.file_list_id,
            object_key: identity.object_key.to_owned(),
            file_checksum: identity.file_checksum.to_owned(),
            partition_spec_id: identity.partition_spec_id,
            sort_order_id: identity.sort_order_id,
            partition: ScribePartitionValueV1::from_partition(identity.partition),
            data_file,
        }
    }

    /// Rebuilds the exact Iceberg `DataFile` this record was derived from.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the record names an unknown
    /// version, its partition is not canonical, a bound does not decode, or the
    /// builder refuses the assembled file.
    pub fn data_file(&self) -> Result<DataFile, ScribeError> {
        self.validate_version()?;
        let partition = self.partition.to_partition()?;
        let mut builder = DataFileBuilder::default();
        builder
            .content(DataContentType::Data)
            .file_path(self.object_key.clone())
            .file_format(DataFileFormat::Parquet)
            .partition(Struct::from_iter([Some(
                partition.iceberg_partition_literal(),
            )]))
            .record_count(self.data_file.record_count)
            .file_size_in_bytes(self.data_file.file_size_in_bytes)
            .column_sizes(counts_map(&self.data_file.column_sizes))
            .value_counts(counts_map(&self.data_file.value_counts))
            .null_value_counts(counts_map(&self.data_file.null_value_counts))
            .nan_value_counts(counts_map(&self.data_file.nan_value_counts))
            .lower_bounds(bounds_map(&self.data_file.lower_bounds)?)
            .upper_bounds(bounds_map(&self.data_file.upper_bounds)?)
            .split_offsets(Some(self.data_file.split_offsets.clone()))
            .partition_spec_id(self.partition_spec_id)
            .sort_order_id(self.sort_order_id);
        builder.build().map_err(|error| ScribeError::Internal {
            detail: format!("promotion evidence does not rebuild a data file: {error}"),
        })
    }

    /// Checks the record against the object the verified upload observed.
    ///
    /// Publication runs this before the fenced transaction: a record that does
    /// not describe the bytes that now exist must not become durable evidence,
    /// and refusing here leaves the members staged and the WAL authoritative.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the version is unknown or the
    /// object key, size, or checksum contradicts the observed object.
    pub fn validate_object(&self, observed: &ObservedHotObject<'_>) -> Result<(), ScribeError> {
        self.validate_version()?;
        if self.object_key != observed.object_key
            || self.data_file.file_size_in_bytes != observed.file_size
            || self.file_checksum != observed.file_checksum
        {
            return Err(ScribeError::Internal {
                detail: format!(
                    "promotion evidence contradicts the published object {}",
                    observed.object_key
                ),
            });
        }
        Ok(())
    }

    /// Encodes the record as the JSON value persisted in `promotion_record`.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the record cannot be encoded,
    /// which would mean a field escaped the representable projection.
    pub fn to_json(&self) -> Result<serde_json::Value, ScribeError> {
        serde_json::to_value(self).map_err(|error| ScribeError::Internal {
            detail: format!("promotion evidence does not encode: {error}"),
        })
    }

    /// Decodes one persisted `promotion_record` value.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the value is not a promotion
    /// record or names an unknown version.
    pub fn from_json(value: &serde_json::Value) -> Result<Self, ScribeError> {
        let record: Self =
            serde_json::from_value(value.clone()).map_err(|error| ScribeError::Internal {
                detail: format!("promotion evidence does not decode: {error}"),
            })?;
        record.validate_version()?;
        Ok(record)
    }

    /// Refuses any record that is not this writer's one shipped version.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] for any other version.
    fn validate_version(&self) -> Result<(), ScribeError> {
        if self.version != SCRIBE_PROMOTION_RECORD_VERSION {
            return Err(ScribeError::Internal {
                detail: format!("promotion evidence names unknown version {}", self.version),
            });
        }
        Ok(())
    }
}

/// Orders one field-id count map for deterministic encoding.
fn sorted_counts(counts: &HashMap<i32, u64>) -> BTreeMap<i32, u64> {
    counts.iter().map(|(id, count)| (*id, *count)).collect()
}

/// Rebuilds the writer-facing count map from its ordered form.
fn counts_map(counts: &BTreeMap<i32, u64>) -> HashMap<i32, u64> {
    counts.iter().map(|(id, count)| (*id, *count)).collect()
}

/// Orders and projects one field-id bound map for deterministic encoding.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a bound has no binary serialization.
fn sorted_bounds(
    bounds: &HashMap<i32, Datum>,
) -> Result<BTreeMap<i32, ScribeColumnBoundV1>, ScribeError> {
    bounds
        .iter()
        .map(|(id, datum)| Ok((*id, ScribeColumnBoundV1::from_datum(datum)?)))
        .collect()
}

/// Rebuilds the writer-facing bound map from its ordered form.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when a bound does not decode.
fn bounds_map(
    bounds: &BTreeMap<i32, ScribeColumnBoundV1>,
) -> Result<HashMap<i32, Datum>, ScribeError> {
    bounds
        .iter()
        .map(|(id, bound)| Ok((*id, bound.to_datum()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::catalog::layout::{
        BIFROST_PARTITION_SPEC_ID, BIFROST_SORT_ORDER_ID, TimeGranularity,
    };

    /// Builds one record over a two-column object with real bounds.
    ///
    /// The bounds are the interesting part: they are the only fields whose JSON
    /// form is type-directed bytes rather than a number, so a round trip that
    /// keeps them is a round trip that keeps everything.
    fn record() -> ScribePublishedHotFileV1 {
        let partition = TimePartition::new(TimeGranularity::Day, chrono::DateTime::UNIX_EPOCH)
            .expect("canonical fixture partition");
        ScribePublishedHotFileV1::from_metrics(
            &PublishedHotFileIdentity {
                data_tenant_id: Uuid::now_v7(),
                namespace: "vala.traces",
                table_name: "spans",
                file_list_id: Uuid::now_v7(),
                object_key: "tenant/table/day=1970-01-01/scribe-0-00000.parquet",
                file_checksum: &"ab".repeat(32),
                partition,
                partition_spec_id: BIFROST_PARTITION_SPEC_ID,
                sort_order_id: BIFROST_SORT_ORDER_ID,
            },
            ScribeDataFileV1 {
                record_count: 4,
                file_size_in_bytes: 2048,
                column_sizes: BTreeMap::from([(1, 128), (2, 256)]),
                value_counts: BTreeMap::from([(1, 4), (2, 4)]),
                null_value_counts: BTreeMap::from([(1, 0), (2, 0)]),
                nan_value_counts: BTreeMap::new(),
                lower_bounds: BTreeMap::from([
                    (
                        1,
                        ScribeColumnBoundV1::from_datum(&Datum::long(10)).expect("lower bound"),
                    ),
                    (
                        2,
                        ScribeColumnBoundV1::from_datum(&Datum::string("alpha"))
                            .expect("lower bound"),
                    ),
                ]),
                upper_bounds: BTreeMap::from([
                    (
                        1,
                        ScribeColumnBoundV1::from_datum(&Datum::long(40)).expect("upper bound"),
                    ),
                    (
                        2,
                        ScribeColumnBoundV1::from_datum(&Datum::string("omega"))
                            .expect("upper bound"),
                    ),
                ]),
                split_offsets: vec![4],
            },
        )
    }

    /// The persisted JSON round-trips to an identical record and encodes the
    /// same bytes twice, so a restart re-deriving it cannot drift.
    ///
    /// # Panics
    ///
    /// Panics when the decoded record differs or the encoding is not stable.
    #[test]
    fn promotion_record_round_trips_and_encodes_deterministically() {
        let original = record();
        let encoded = original.to_json().expect("record encodes");
        let decoded = ScribePublishedHotFileV1::from_json(&encoded).expect("record decodes");
        assert_eq!(decoded, original);
        assert_eq!(
            serde_json::to_string(&encoded).expect("stable encoding"),
            serde_json::to_string(&decoded.to_json().expect("re-encode")).expect("stable encoding"),
        );
    }

    /// The record rebuilds the exact Iceberg `DataFile` a promoter needs,
    /// including its partition value and typed bounds.
    ///
    /// # Panics
    ///
    /// Panics when a rebuilt field does not match the recorded one.
    #[test]
    fn promotion_record_rebuilds_its_iceberg_data_file() {
        let original = record();
        let rebuilt = original.data_file().expect("data file rebuilds");
        assert_eq!(rebuilt.record_count(), original.data_file.record_count);
        assert_eq!(
            rebuilt.file_size_in_bytes(),
            original.data_file.file_size_in_bytes
        );
        assert_eq!(rebuilt.file_path(), original.object_key);
        assert_eq!(rebuilt.partition_spec_id(), BIFROST_PARTITION_SPEC_ID);
        assert_eq!(rebuilt.sort_order_id(), Some(BIFROST_SORT_ORDER_ID));
        assert_eq!(
            rebuilt.lower_bounds().get(&1),
            Some(&Datum::long(10)),
            "typed bounds survive the projection"
        );
        assert_eq!(
            rebuilt.upper_bounds().get(&2),
            Some(&Datum::string("omega"))
        );
        assert_eq!(
            rebuilt.split_offsets(),
            Some(original.data_file.split_offsets.as_slice())
        );
    }

    /// A record that contradicts the object the verified upload observed is
    /// refused, and the matching record is accepted.
    ///
    /// # Panics
    ///
    /// Panics when a contradicting key, size, or digest is accepted, or when
    /// the agreeing observation is refused.
    #[test]
    fn promotion_record_refuses_an_object_it_does_not_describe() {
        let record = record();
        let agreeing = ObservedHotObject {
            object_key: &record.object_key,
            file_size: record.data_file.file_size_in_bytes,
            file_checksum: &record.file_checksum,
        };
        record.validate_object(&agreeing).expect("agreeing object");
        for contradiction in [
            ObservedHotObject {
                object_key: "tenant/table/day=1970-01-01/scribe-0-00001.parquet",
                ..agreeing
            },
            ObservedHotObject {
                file_size: agreeing.file_size + 1,
                ..agreeing
            },
            ObservedHotObject {
                file_checksum: &"ef".repeat(32),
                ..agreeing
            },
        ] {
            record
                .validate_object(&contradiction)
                .expect_err("contradicting object must be refused");
        }
    }

    /// A stored record naming any other version is refused rather than read as
    /// this version, because there is exactly one shipped shape.
    ///
    /// # Panics
    ///
    /// Panics when an unknown version decodes or validates.
    #[test]
    fn promotion_record_refuses_an_unknown_version() {
        let mut encoded = record().to_json().expect("record encodes");
        encoded["version"] = serde_json::json!(2);
        ScribePublishedHotFileV1::from_json(&encoded).expect_err("unknown version must be refused");
    }
}
