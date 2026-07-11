use iceberg::spec::{Schema, UnboundPartitionSpec};

use crate::error::BifrostError;
use crate::types::PartitionTransform;

pub fn build_partition_spec(
    iceberg_schema: &Schema,
    partition_columns: &[(String, PartitionTransform)],
) -> Result<UnboundPartitionSpec, BifrostError> {
    let mut builder = UnboundPartitionSpec::builder();
    for (col_name, transform) in partition_columns {
        let field = iceberg_schema.field_by_name(col_name).ok_or_else(|| {
            BifrostError::Internal(format!("partition column not found: {col_name}"))
        })?;
        let iceberg_transform = transform.to_iceberg_transform();
        // Iceberg disallows a non-identity partition field name that matches a schema
        // column name. Use a suffixed name for derived transforms.
        let partition_name = match transform {
            PartitionTransform::Identity => col_name.clone(),
            PartitionTransform::Day => format!("{col_name}_day"),
            PartitionTransform::Month => format!("{col_name}_month"),
            PartitionTransform::Year => format!("{col_name}_year"),
            PartitionTransform::Truncate(w) => format!("{col_name}_trunc{w}"),
            PartitionTransform::Bucket(n) => format!("{col_name}_bucket{n}"),
        };
        builder = builder
            .add_partition_field(field.id, &partition_name, iceberg_transform)
            .map_err(|e| BifrostError::Internal(e.to_string()))?;
    }
    Ok(builder.build())
}

#[cfg(test)]
mod tests {
    use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, TimeUnit};

    use super::*;

    fn ts_schema() -> Schema {
        let arrow = ArrowSchema::new(vec![Field::new(
            "ts",
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            false,
        )]);
        iceberg::arrow::arrow_schema_to_schema_auto_assign_ids(&arrow).expect("test schema builds")
    }

    #[test]
    fn partition_field_names_use_transform_suffix() {
        let schema = ts_schema();
        let spec = build_partition_spec(&schema, &[("ts".to_owned(), PartitionTransform::Day)])
            .expect("day partition spec builds");
        assert_eq!(spec.fields()[0].name, "ts_day");
    }

    #[test]
    fn identity_partition_keeps_column_name() {
        let schema = ts_schema();
        let spec =
            build_partition_spec(&schema, &[("ts".to_owned(), PartitionTransform::Identity)])
                .expect("identity partition spec builds");
        assert_eq!(spec.fields()[0].name, "ts");
    }

    #[test]
    fn month_partition_uses_month_suffix() {
        let schema = ts_schema();
        let spec = build_partition_spec(&schema, &[("ts".to_owned(), PartitionTransform::Month)])
            .expect("month partition spec builds");
        assert_eq!(spec.fields()[0].name, "ts_month");
    }

    #[test]
    fn truncate_partition_uses_trunc_suffix() {
        let schema = ts_schema();
        let spec = build_partition_spec(
            &schema,
            &[("ts".to_owned(), PartitionTransform::Truncate(16))],
        )
        .expect("truncate partition spec builds");
        assert_eq!(spec.fields()[0].name, "ts_trunc16");
    }

    #[test]
    fn bucket_partition_uses_bucket_suffix() {
        let schema = ts_schema();
        let spec =
            build_partition_spec(&schema, &[("ts".to_owned(), PartitionTransform::Bucket(8))])
                .expect("bucket partition spec builds");
        assert_eq!(spec.fields()[0].name, "ts_bucket8");
    }
}
