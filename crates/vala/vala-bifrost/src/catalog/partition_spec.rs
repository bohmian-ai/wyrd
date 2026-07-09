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
