//! Seeded, stratified Variant sample of one claim's staged runs.
//!
//! A claim's final objects all share one Variant layout, chosen before the
//! writer opens. [`ClaimVariantSample`] reads the claim's local, unshredded
//! runs twice: pass 1 reads only `principal_id` to count each writer's rows,
//! pass 2 reads `principal_id` and the Variant columns and offers every row to
//! the fork's [`VariantSampler`], which keeps a row when its seeded hash falls
//! under its stratum's Cochran rate. Only counters are held, never rows.
//!
//! The seed is the claim identity and a row's position is its position across
//! the runs in claim order, so a re-run claim picks the same sample.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use arrow::array::{AsArray, RecordBatch};
use arrow::datatypes::{DataType, Schema};
use iceberg::writer::file_writer::variant_shredding::{VariantLayout, VariantSampler};
use parquet::arrow::ProjectionMask;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use wyrd_spec::vala::PRINCIPAL_ID;

use crate::contracts::ScribeError;
use crate::scribe::assembly::StagingClaimId;
use crate::scribe::claim_assembly::MERGE_BATCH_ROWS;

/// One claim's runs and the seed that makes its Variant sample reproducible.
pub struct ClaimVariantSample<'a> {
    /// The claim's staged runs in claim order; row positions follow this order.
    runs: &'a [PathBuf],
    /// Logical schema every run carries.
    schema: &'a Schema,
    /// First eight bytes of the claim identity.
    seed: u64,
}

impl<'a> ClaimVariantSample<'a> {
    /// Plans the sample of `runs`, seeded by `claim`.
    #[must_use]
    pub fn new(claim: StagingClaimId, runs: &'a [PathBuf], schema: &'a Schema) -> Self {
        let mut seed = [0_u8; 8];
        seed.copy_from_slice(&claim.as_bytes()[..8]);
        Self {
            runs,
            schema,
            seed: u64::from_le_bytes(seed),
        }
    }

    /// Chooses the claim's Variant layout from its stratified sample.
    ///
    /// A schema without a Variant column returns the unshredded layout without
    /// reading a run. A schema without `principal_id` samples one stratum.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be opened or decoded,
    /// a `principal_id` is null or not a string, or a Variant column cannot be
    /// sampled.
    pub fn layout(&self) -> Result<VariantLayout, ScribeError> {
        let policy = crate::parquet::BIFROST_VARIANT_SHREDDING;
        if VariantSampler::new(self.schema, policy, self.seed, &[]).is_inert() {
            return Ok(VariantLayout::default());
        }
        let principal = self
            .schema
            .index_of(PRINCIPAL_ID)
            .ok()
            .map(|_| PRINCIPAL_ID);
        let mut strata: HashMap<String, usize> = HashMap::new();
        let mut stratum_rows: Vec<usize> = Vec::new();
        self.scan(principal.as_slice(), |batch| {
            match principal {
                Some(column) => {
                    for value in &principals(batch, column)? {
                        let value = required_principal(value)?;
                        let next = strata.len();
                        let stratum = *strata.entry(value.to_owned()).or_insert(next);
                        if stratum == stratum_rows.len() {
                            stratum_rows.push(0);
                        }
                        stratum_rows[stratum] += 1;
                    }
                }
                None => match stratum_rows.first_mut() {
                    Some(rows) => *rows += batch.num_rows(),
                    None => stratum_rows.push(batch.num_rows()),
                },
            }
            Ok(())
        })?;
        let mut sampler = VariantSampler::new(self.schema, policy, self.seed, &stratum_rows);
        let mut columns: Vec<String> = sampler.column_names().map(str::to_owned).collect();
        columns.extend(principal.map(str::to_owned));
        let mut position = 0_u64;
        self.scan(&columns, |batch| {
            let rows = match principal {
                Some(column) => principals(batch, column)?
                    .iter()
                    .map(|value| Ok(strata[required_principal(value)?]))
                    .collect::<Result<Vec<_>, ScribeError>>()?,
                None => vec![0; batch.num_rows()],
            };
            sampler
                .offer(batch, &rows, position)
                .map_err(|error| ScribeError::Internal {
                    detail: format!("sample the claim's Variant columns: {error}"),
                })?;
            position += rows.len() as u64;
            Ok(())
        })?;
        Ok(sampler.layout())
    }

    /// Reads `columns` of every run in claim order and visits each batch.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when a run cannot be opened or decoded,
    /// or any error `visit` returns.
    fn scan(
        &self,
        columns: &[impl AsRef<str>],
        mut visit: impl FnMut(&RecordBatch) -> Result<(), ScribeError>,
    ) -> Result<(), ScribeError> {
        for run in self.runs {
            for batch in self.run_reader(run, columns)? {
                visit(&batch.map_err(|error| ScribeError::Internal {
                    detail: format!("decode the staged run for Variant sampling: {error}"),
                })?)?;
            }
        }
        Ok(())
    }

    /// Opens one run projected to the named top-level columns.
    ///
    /// Staged runs are unshredded, so each top-level Arrow field is one Parquet
    /// root column at the same index.
    ///
    /// # Errors
    ///
    /// Returns [`ScribeError::Internal`] when the run cannot be opened or its
    /// reader cannot be built.
    fn run_reader(
        &self,
        run: &Path,
        columns: &[impl AsRef<str>],
    ) -> Result<parquet::arrow::arrow_reader::ParquetRecordBatchReader, ScribeError> {
        let file = std::fs::File::open(run).map_err(|error| ScribeError::Internal {
            detail: format!("open the staged run for Variant sampling: {error}"),
        })?;
        let builder = ParquetRecordBatchReaderBuilder::try_new(file).map_err(|error| {
            ScribeError::Internal {
                detail: format!("build the staged run Variant sampling reader: {error}"),
            }
        })?;
        let roots = columns
            .iter()
            .filter_map(|name| self.schema.index_of(name.as_ref()).ok());
        let mask = ProjectionMask::roots(builder.parquet_schema(), roots);
        builder
            .with_projection(mask)
            .with_batch_size(MERGE_BATCH_ROWS)
            .build()
            .map_err(|error| ScribeError::Internal {
                detail: format!("start the staged run Variant sampling read: {error}"),
            })
    }
}

/// Returns `batch`'s `principal_id` column as UTF-8 strings.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the column is absent or cannot be
/// cast to UTF-8.
fn principals(batch: &RecordBatch, column: &str) -> Result<arrow::array::StringArray, ScribeError> {
    let Some(values) = batch.column_by_name(column) else {
        return Err(ScribeError::Internal {
            detail: format!("staged run lost its `{column}` column"),
        });
    };
    arrow::compute::cast(values, &DataType::Utf8)
        .map(|values| values.as_string::<i32>().clone())
        .map_err(|error| ScribeError::Internal {
            detail: format!("read `{column}` for Variant sampling: {error}"),
        })
}

/// Unwraps one server-injected `principal_id`, which is never null.
///
/// # Errors
///
/// Returns [`ScribeError::Internal`] when the value is null.
fn required_principal(value: Option<&str>) -> Result<&str, ScribeError> {
    value.ok_or_else(|| ScribeError::Internal {
        detail: format!("staged run holds a null `{PRINCIPAL_ID}`"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, StringArray};
    use arrow::datatypes::Field;

    /// Logical schema of the fixture runs: `principal_id` and Variant `v`.
    fn sample_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(PRINCIPAL_ID, DataType::Utf8, false),
            wyrd_types::variant::variant_field("v", true),
        ]))
    }

    /// Writes one unshredded staged run holding `rows` of `(principal, JSON)`.
    fn write_run(dir: &Path, name: &str, rows: &[(String, String)]) -> PathBuf {
        let principals: ArrayRef = Arc::new(StringArray::from_iter_values(
            rows.iter().map(|(principal, _)| principal.as_str()),
        ));
        let json: ArrayRef = Arc::new(StringArray::from_iter_values(
            rows.iter().map(|(_, json)| json.as_str()),
        ));
        let variant: ArrayRef = parquet_variant_compute::json_to_variant(&json)
            .expect("valid JSON")
            .into();
        let variant = arrow::compute::cast(&variant, &wyrd_types::variant::variant_storage_type())
            .expect("Variant storage casts to the stored shape");
        let batch =
            RecordBatch::try_new(sample_schema(), vec![principals, variant]).expect("run batch");
        let path = dir.join(name);
        let mut writer = parquet::arrow::ArrowWriter::try_new(
            std::fs::File::create(&path).expect("run file"),
            sample_schema(),
            None,
        )
        .expect("run writer");
        writer.write(&batch).expect("write run");
        writer.close().expect("close run");
        path
    }

    /// `count` rows for `principal`, each holding `json`.
    fn rows(principal: &str, count: usize, json: &str) -> Vec<(String, String)> {
        (0..count)
            .map(|_| (principal.to_owned(), json.to_owned()))
            .collect()
    }

    /// The fields the sampled layout shreds in column `v`.
    fn shredded(runs: &[PathBuf]) -> Vec<String> {
        let schema = sample_schema();
        let claim = StagingClaimId::from_hex(&"2a".repeat(32)).expect("claim id");
        let layout = ClaimVariantSample::new(claim, runs, &schema)
            .layout()
            .expect("claim sample");
        let physical = layout.physical_schema(&schema).expect("physical schema");
        let DataType::Struct(storage) = physical.field_with_name("v").expect("v").data_type()
        else {
            return Vec::new();
        };
        match storage
            .find("typed_value")
            .map(|(_, field)| field.data_type())
        {
            Some(DataType::Struct(fields)) => fields.iter().map(|f| f.name().clone()).collect(),
            _ => Vec::new(),
        }
    }

    /// Bifrost's policy samples 4,147 rows from a large stratum and nearly all of a small one.
    #[test]
    fn bifrost_policy_follows_cochran() {
        let policy = crate::parquet::BIFROST_VARIANT_SHREDDING;
        assert_eq!(policy.stratum_sample_size(1_000_000_000), 4_147);
        assert_eq!(policy.stratum_sample_size(100), 98);
        assert_eq!(policy.stratum_sample_size(30), 30);
    }

    /// A key common in one writer's rows and rare overall is shredded.
    ///
    /// The busy writer has 20,000 rows without `rare`; `small` holds `rare` in
    /// all 40 of its rows (0.2% of the claim), which is its own stratum.
    #[test]
    fn writer_with_its_own_stratum_keeps_its_key() {
        let dir = tempfile::tempdir().expect("runs");
        let busy = write_run(
            dir.path(),
            "busy.parquet",
            &rows("busy", 20_000, r#"{"common":1}"#),
        );
        let small = write_run(
            dir.path(),
            "small.parquet",
            &rows("small", 40, r#"{"rare":1}"#),
        );
        assert_eq!(shredded(&[busy, small]), ["common", "rare"]);
    }

    /// A writer under 30 rows is merged into the shared stratum and still counts.
    ///
    /// `tiny` holds `rare` in 10 rows and `quiet` has 20 rows without it, so
    /// the shared stratum holds `rare` in a third of its rows while the claim
    /// holds it in 0.05%.
    #[test]
    fn writers_in_the_shared_stratum_keep_their_key() {
        let dir = tempfile::tempdir().expect("runs");
        let busy = write_run(
            dir.path(),
            "busy.parquet",
            &rows("busy", 20_000, r#"{"common":1}"#),
        );
        let mut few = rows("tiny", 10, r#"{"rare":1}"#);
        few.extend(rows("quiet", 20, r#"{"common":1}"#));
        let few = write_run(dir.path(), "few.parquet", &few);
        assert_eq!(shredded(&[busy, few]), ["common", "rare"]);
    }

    /// A schema with no Variant column samples nothing and stays unshredded.
    #[test]
    fn no_variant_column_reads_no_run() {
        let schema = Schema::new(vec![Field::new(PRINCIPAL_ID, DataType::Utf8, false)]);
        let claim = StagingClaimId::from_hex(&"2a".repeat(32)).expect("claim id");
        let missing = [PathBuf::from("/nonexistent/run.parquet")];
        let layout = ClaimVariantSample::new(claim, &missing, &schema)
            .layout()
            .expect("no run is opened");
        assert_eq!(layout, VariantLayout::default());
    }
}
