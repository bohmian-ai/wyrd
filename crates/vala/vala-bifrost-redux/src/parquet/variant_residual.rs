//! Per-file share of Variant bytes left in the residual `value`.
//!
//! Every final Scribe object and Forge output records, from its own footer,
//! how much of its non-null Variant data the layout failed to shred. A rising
//! share means layouts are missing keys. The metric's only label is the
//! closed writer role.

use arrow::datatypes::Schema;
use iceberg::writer::file_writer::variant_shredding::variant_leaves;
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
/// Sums uncompressed leaf sizes over every Variant field of `logical`, at
/// any Struct or List depth, located in the footer by the fork's
/// [`variant_leaves`]: each field's root `value` leaf against every leaf of
/// the field except `metadata`. Returns `None` when the file has no Variant
/// bytes to measure or its schema cannot be read.
#[must_use]
pub fn variant_residual_share(metadata: &ParquetMetaData, logical: &Schema) -> Option<f64> {
    let variants = variant_leaves(logical, metadata).ok()?;
    let mut residual = 0_i64;
    let mut total = 0_i64;
    for group in metadata.row_groups() {
        for variant in &variants {
            for leaf in variant.all.clone() {
                if variant.metadata == Some(leaf) {
                    continue;
                }
                let bytes = group.column(leaf).uncompressed_size();
                total += bytes;
                if variant.value == Some(leaf) {
                    residual += bytes;
                }
            }
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

    /// An unshredded column is all residual and a fully shredded one mostly
    /// not, whether the Variant is a column or inside a List of Structs.
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

        let attributes = wyrd_types::variant::variant_field("attributes", true);
        let element = arrow::datatypes::Field::new(
            "element",
            arrow::datatypes::DataType::Struct(vec![attributes.clone()].into()),
            true,
        );
        let events = arrow::array::ListArray::new(
            Arc::new(element.clone()),
            arrow::buffer::OffsetBuffer::from_lengths(vec![1; batch.num_rows()]),
            Arc::new(arrow::array::StructArray::new(
                vec![attributes].into(),
                vec![Arc::clone(batch.column(0))],
                None,
            )),
            None,
        );
        let nested = Arc::new(Schema::new(vec![arrow::datatypes::Field::new(
            "events",
            arrow::datatypes::DataType::List(Arc::new(element)),
            true,
        )]));
        let nested_batch =
            RecordBatch::try_new(Arc::clone(&nested), vec![Arc::new(events)]).expect("batch");
        let unshredded_nested = footer(&nested_batch, &VariantLayout::default());
        assert_eq!(
            variant_residual_share(&unshredded_nested, &nested),
            Some(1.0)
        );
        let mut sampler = VariantSampler::new(&nested, policy, 0, &[nested_batch.num_rows()]);
        sampler
            .offer(&nested_batch, &vec![0; nested_batch.num_rows()], 0)
            .expect("sample");
        let shredded_nested = footer(&nested_batch, &sampler.layout());
        let nested_share =
            variant_residual_share(&shredded_nested, &nested).expect("Variant bytes");
        assert!(
            nested_share < 0.5,
            "nested keys leave the residual: {nested_share}"
        );

        let plain = Schema::new(vec![arrow::datatypes::Field::new(
            "v",
            arrow::datatypes::DataType::Utf8,
            true,
        )]);
        assert_eq!(variant_residual_share(&unshredded, &plain), None);
    }
}
