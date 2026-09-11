//! Strict conversion between Iceberg catalog URIs and Forge object keys.

use crate::catalog::TenantTableBinding;

use super::error::ForgeError;

/// Verify that an Iceberg table location belongs to this exact binding and store.
///
/// # Errors
///
/// Returns [`ForgeError::Invariant`] when the location is empty, the binding
/// prefix is empty, the location uses the prefix as anything other than its
/// final path segment, or its scheme/authority differs from `store`.
pub(super) fn validate_table_location(
    binding: &TenantTableBinding,
    table_location: &str,
    store: &opendal::Operator,
) -> Result<(), ForgeError> {
    let location = table_location.trim_end_matches('/');
    let prefix = binding.object_prefix.trim_matches('/');
    let marker = format!("/{prefix}");
    if !location.ends_with(&marker) {
        return Err(ForgeError::Invariant {
            detail: format!(
                "catalog table location does not match tenant binding: location={table_location}"
            ),
        });
    }
    if location.is_empty() || prefix.is_empty() {
        return Err(ForgeError::Invariant {
            detail: format!(
                "catalog table location does not match tenant binding: location={table_location}"
            ),
        });
    }
    let info = store.info();
    let expected_scheme = match info.scheme() {
        "fs" => "file",
        scheme => scheme,
    };
    let expected_location = if expected_scheme == "file" {
        String::new()
    } else {
        format!("{expected_scheme}://{}/{prefix}", info.name())
    };
    let location_matches_store = if expected_scheme == "file" {
        location
            .strip_prefix("file://")
            .is_some_and(|path| !path.ends_with(&format!("//{prefix}")))
    } else {
        location == expected_location
    };
    if !location_matches_store {
        return Err(ForgeError::Invariant {
            detail: format!(
                "catalog table location does not exactly match the binding and staging store: {table_location}"
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
    store: &opendal::Operator,
    path: &str,
) -> Result<String, ForgeError> {
    validate_table_location(binding, table_location, store)?;
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

    /// Create an in-memory store so scheme and authority checks use live metadata.
    fn memory_store() -> opendal::Operator {
        opendal::Operator::new(opendal::services::Memory::default())
            .expect("memory backend initializes")
            .finish()
    }

    /// Create a filesystem store to cover the authority-free `file:///` form.
    fn filesystem_store(root: &std::path::Path) -> opendal::Operator {
        let service = opendal::services::Fs::default()
            .root(root.to_str().expect("temporary path is valid UTF-8"));
        opendal::Operator::new(service)
            .expect("filesystem backend initializes")
            .finish()
    }

    /// Proves URI and relative forms map to one validated key while confusing
    /// prefixes, traversal, and a foreign store authority fail closed.
    #[test]
    fn manifest_candidate_rejects_cross_binding_path() {
        let binding = binding();
        let store = memory_store();
        let authority = store.info().name();
        let location = format!("memory://{authority}/{}", binding.object_prefix);
        let catalog_path = format!("{location}/data/part-000.parquet");
        let expected = format!("{}/data/part-000.parquet", binding.object_prefix);

        assert_eq!(
            catalog_path_to_object_key(&location, &binding, &store, &catalog_path)
                .expect("catalog URI maps"),
            expected
        );
        assert_eq!(
            catalog_path_to_object_key(&location, &binding, &store, &expected)
                .expect("relative key maps"),
            expected
        );
        assert!(
            catalog_path_to_object_key(
                &location,
                &binding,
                &store,
                &format!(
                    "memory://{authority}/{}-other/data/part.parquet",
                    binding.object_prefix
                ),
            )
            .is_err()
        );
        assert!(
            catalog_path_to_object_key(
                &location,
                &binding,
                &store,
                &format!(
                    "memory://{authority}/{}/../other/part.parquet",
                    binding.object_prefix
                ),
            )
            .is_err()
        );
        assert!(
            catalog_path_to_object_key(
                &format!("memory://foreign/{}", binding.object_prefix),
                &binding,
                &store,
                &catalog_path,
            )
            .is_err()
        );
        assert!(
            catalog_path_to_object_key(
                &format!("memory://{authority}//{}", binding.object_prefix),
                &binding,
                &store,
                &catalog_path,
            )
            .is_err(),
            "an extra separator changes the exact table root"
        );
    }

    /// Proves the authority-free filesystem URI maps through the same contract.
    #[test]
    fn shared_path_contract_accepts_file_scheme_round_trip() {
        let binding = binding();
        let root = tempfile::tempdir().expect("temporary root initializes");
        let store = filesystem_store(root.path());
        let location = format!("file:///{}", binding.object_prefix);
        let catalog_path = format!("{location}/data/part-000.parquet");
        let expected = format!("{}/data/part-000.parquet", binding.object_prefix);

        assert_eq!(
            catalog_path_to_object_key(&location, &binding, &store, &catalog_path)
                .expect("file catalog URI maps"),
            expected
        );
    }
}
