use iceberg::spec::{Schema, UnboundPartitionSpec};

use crate::error::BifrostError;
use crate::types::PartitionTransform;

pub fn build_partition_spec(
    iceberg_schema: &Schema,
    partition_columns: &[(String, PartitionTransform)],
) -> Result<UnboundPartitionSpec, BifrostError> {
    let mut builder = UnboundPartitionSpec::builder();
    for (col_name, transform) in partition_columns {
        let field = iceberg_schema
            .field_by_name(col_name)
            .ok_or_else(|| BifrostError::Internal(format!("partition column not found: {col_name}")))?;
        let iceberg_transform = transform.to_iceberg_transform();
        builder = builder
            .add_partition_field(field.id, col_name, iceberg_transform)
            .map_err(|e| BifrostError::Internal(e.to_string()))?;
    }
    Ok(builder.build())
}
