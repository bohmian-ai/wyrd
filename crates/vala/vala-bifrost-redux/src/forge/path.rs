//! Strict conversion between Iceberg catalog URIs and Forge object keys.

use crate::catalog::TenantTableBinding;

use super::error::ForgeError;

/// Verify that an Iceberg table location is exactly this binding's table root
/// in this node's warehouse.
///
/// The catalog places every table at `{warehouse}/{object_prefix}`, so any
/// other location belongs to another binding or another store.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the binding prefix is empty or the
/// location is not exactly `{warehouse}/{object_prefix}`.
pub(super) fn validate_table_location(
    binding: &TenantTableBinding,
    table_location: &str,
    warehouse: &str,
) -> Result<(), ForgeError> {
    let prefix = binding.object_prefix.trim_matches('/');
    let expected = format!("{}/{prefix}", warehouse.trim_end_matches('/'));
    if prefix.is_empty() || table_location.trim_end_matches('/') != expected {
        return Err(ForgeError::Invariant {
            detail: format!(
                "catalog table location does not exactly match the binding and warehouse: {table_location}"
            ),
        });
    }
    Ok(())
}

/// Convert one catalog path into the exact relative object key under `binding`.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when `table_location` is not owned by the
/// binding or `path` cannot be represented as a validated object key below it.
pub(super) fn catalog_path_to_object_key(
    table_location: &str,
    binding: &TenantTableBinding,
    warehouse: &str,
    path: &str,
) -> Result<String, ForgeError> {
    validate_table_location(binding, table_location, warehouse)?;
    if let Some(path) = binding.validate_object_path(path) {
        return Ok(path);
    }
    let location = table_location.trim_end_matches('/');
    let relative =
        path.strip_prefix(&format!("{location}/"))
            .ok_or_else(|| ForgeError::Invariant {
                detail: format!("catalog path escaped table binding: {path}"),
            })?;
    let key = format!("{}/{relative}", binding.object_prefix.trim_end_matches('/'));
    binding
        .validate_object_path(&key)
        .ok_or_else(|| ForgeError::Invariant {
            detail: format!("catalog path escaped table binding: {path}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::table_ref::TableRef;
    use crate::namespaces::BifrostNamespace;
    use wyrd_spec::DataTenantId;

    /// Build one valid binding whose prefix is distinct from sibling tables.
    fn binding() -> TenantTableBinding {
        TenantTableBinding::resolve((
            DataTenantId::new_v7(),
            TableRef::new(BifrostNamespace::Traces, "spans"),
        ))
        .expect("test binding resolves")
    }

    /// Warehouse URI the path contract tests place tables under.
    const WAREHOUSE: &str = "s3://tables";

    /// Proves URI and relative forms map to one validated key while confusing
    /// prefixes, traversal, and a foreign warehouse fail closed.
    #[test]
    fn manifest_candidate_rejects_cross_binding_path() {
        let binding = binding();
        let location = format!("{WAREHOUSE}/{}", binding.object_prefix);
        let catalog_path = format!("{location}/data/part-000.parquet");
        let expected = format!("{}/data/part-000.parquet", binding.object_prefix);
        let map = |location: &str, path: &str| {
            catalog_path_to_object_key(location, &binding, WAREHOUSE, path)
        };

        assert_eq!(
            map(&location, &catalog_path).expect("catalog URI maps"),
            expected
        );
        assert_eq!(
            map(&location, &expected).expect("relative key maps"),
            expected
        );
        assert!(
            map(
                &location,
                &format!(
                    "{WAREHOUSE}/{}-other/data/part.parquet",
                    binding.object_prefix
                )
            )
            .is_err()
        );
        assert!(
            map(
                &location,
                &format!(
                    "{WAREHOUSE}/{}/../other/part.parquet",
                    binding.object_prefix
                )
            )
            .is_err()
        );
        assert!(
            map(
                &format!("s3://foreign/{}", binding.object_prefix),
                &catalog_path
            )
            .is_err()
        );
        assert!(
            map(
                &format!("{WAREHOUSE}//{}", binding.object_prefix),
                &catalog_path
            )
            .is_err(),
            "an extra separator changes the exact table root"
        );
    }

    /// Proves a local warehouse's `file://` table root maps through the same contract.
    #[test]
    fn shared_path_contract_accepts_file_scheme_round_trip() {
        let binding = binding();
        let warehouse = "file:///var/lib/wyrd/warehouse";
        let location = format!("{warehouse}/{}", binding.object_prefix);
        let catalog_path = format!("{location}/data/part-000.parquet");
        let expected = format!("{}/data/part-000.parquet", binding.object_prefix);

        assert_eq!(
            catalog_path_to_object_key(&location, &binding, warehouse, &catalog_path)
                .expect("file catalog URI maps"),
            expected
        );
    }
}
