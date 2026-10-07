//! Per-file share of Variant bytes left in the residual `value`.
//!
//! Every final Scribe object and Forge output records, from its own footer,
//! how much of its non-null Variant data the layout failed to shred. A rising
//! share means layouts are missing keys. The metric's only label is the
//! closed writer role.

use arrow::datatypes::Schema;
use num_traits::ToPrimitive;
use parquet::file::metadata::ParquetMetaData;

/// Which Bifrost writer produced a measured file, the closed `writer` label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantWriter {
    /// A final Scribe hot object, laid out from its claim's sample.
    Scribe,
    /// A Forge rewrite output, laid out from its source footers.
    Forge,
}

impl VariantWriter {
    /// Returns the stable `writer` label for this role.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scribe => "scribe",
            Self::Forge => "forge",
        }
    }

    /// Records one written file's [`variant_residual_share`], when it has one.
    pub fn record(self, share: Option<f64>) {
        if let Some(share) = share {
            metrics::histogram!("bifrost_variant_residual_share", "writer" => self.as_str())
                .record(share);
        }
    }
}

/// Share of a file's Variant bytes held in residual `value` leaves.
///
/// Sums uncompressed leaf sizes over every top-level Variant column of
/// `logical`: the root `value` leaf against every leaf of the column except
/// `metadata`. Returns `None` when the file has no Variant bytes to measure.
#[must_use]
pub fn variant_residual_share(metadata: &ParquetMetaData, logical: &Schema) -> Option<f64> {
    let columns: Vec<&str> = logical
        .fields()
        .iter()
        .filter(|field| wyrd_types::variant::is_variant(field))
        .map(|field| field.name().as_str())
        .collect();
    let mut residual = 0_i64;
    let mut total = 0_i64;
    for chunk in metadata
        .row_groups()
        .iter()
        .flat_map(parquet::file::metadata::RowGroupMetaData::columns)
    {
        let path = chunk.column_path().parts();
        let (Some(root), Some(leaf)) = (path.first(), path.get(1)) else {
            continue;
        };
        if !columns.contains(&root.as_str()) || (path.len() == 2 && leaf == "metadata") {
            continue;
        }
        let bytes = chunk.uncompressed_size();
        total += bytes;
        if path.len() == 2 && leaf == "value" {
            residual += bytes;
        }
    }
    if total == 0 {
        return None;
    }
    Some(residual.to_f64()? / total.to_f64()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use arrow::array::{ArrayRef, RecordBatch, StringArray};
    use iceberg::writer::file_writer::variant_shredding::{VariantLayout, VariantSampler};
    use parquet::file::metadata::ParquetMetaDataReader;

    /// Writes `batch` shredded with `layout`'s Variant layout and returns its footer.
    fn footer(batch: &RecordBatch, layout: &VariantLayout) -> ParquetMetaData {
        let shredded = layout.shred(batch).expect("shred");
        let mut bytes = Vec::new();
        let mut writer = parquet::arrow::ArrowWriter::try_new(&mut bytes, shredded.schema(), None)
            .expect("writer");
        writer.write(&shredded).expect("write");
        writer.close().expect("close");
        ParquetMetaDataReader::new()
            .parse_and_finish(&bytes::Bytes::from(bytes))
            .expect("footer")
    }

    /// An unshredded column is all residual; a fully shredded one is mostly not.
    #[test]
    fn residual_share_measures_unshredded_bytes() {
        let json: ArrayRef = Arc::new(StringArray::from_iter_values(
            (0..1_000).map(|row| format!(r#"{{"a":{row},"b":"text {row}"}}"#)),
        ));
        let variant: ArrayRef = parquet_variant_compute::json_to_variant(&json)
            .expect("valid JSON")
            .into();
        let variant = arrow::compute::cast(&variant, &wyrd_types::variant::variant_storage_type())
            .expect("Variant storage");
        let schema = Arc::new(Schema::new(vec![wyrd_types::variant::variant_field(
            "v", true,
        )]));
        let batch = RecordBatch::try_new(Arc::clone(&schema), vec![variant]).expect("batch");

        let unshredded = footer(&batch, &VariantLayout::default());
        assert_eq!(variant_residual_share(&unshredded, &schema), Some(1.0));

        let policy = crate::parquet::BIFROST_VARIANT_SHREDDING;
        let mut sampler = VariantSampler::new(&schema, policy, 0, &[batch.num_rows()]);
        sampler
            .offer(&batch, &vec![0; batch.num_rows()], 0)
            .expect("sample");
        let shredded = footer(&batch, &sampler.layout());
        let share = variant_residual_share(&shredded, &schema).expect("Variant bytes");
        assert!(share < 0.5, "shredded keys leave the residual: {share}");

        let plain = Schema::new(vec![arrow::datatypes::Field::new(
            "v",
            arrow::datatypes::DataType::Utf8,
            true,
        )]);
        assert_eq!(variant_residual_share(&unshredded, &plain), None);
    }
}
