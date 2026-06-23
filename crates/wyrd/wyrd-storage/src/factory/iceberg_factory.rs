use std::collections::HashMap;
use std::sync::Arc;

use iceberg::io::StorageFactory;
use iceberg_storage_opendal::OpenDalResolvingStorageFactory;

use crate::error::StorageError;
use crate::settings::BackendConfig;

/// Iceberg storage factory and catalog properties returned by backend constructors.
pub type IcebergStorageResult =
    Result<(Arc<dyn StorageFactory>, HashMap<String, String>), StorageError>;

/// Build an Iceberg `StorageFactory` and extra catalog properties for the active backend.
///
/// # Errors
/// Returns a `StorageError` if the factory cannot be constructed.
pub fn iceberg_storage_factory(backend: &BackendConfig) -> IcebergStorageResult {
    let factory = Arc::new(OpenDalResolvingStorageFactory::new()) as Arc<dyn StorageFactory>;
    let mut props = HashMap::new();

    match backend {
        BackendConfig::Local { .. } => {}
        BackendConfig::S3(c) => {
            if let Some(endpoint) = &c.endpoint_url {
                props.insert("s3.endpoint".to_string(), endpoint.clone());
            }
            if let Some(region) = &c.region {
                props.insert("s3.region".to_string(), region.clone());
            }
            if c.force_path_style {
                props.insert("s3.path-style-access".to_string(), "true".to_string());
            }
        }
        BackendConfig::Gcs(c) => {
            if let Some(endpoint) = &c.endpoint_url {
                props.insert("gcs.service.path".to_string(), endpoint.clone());
            }
        }
        BackendConfig::Azure(c) => {
            props.insert("adls.account-name".to_string(), c.account.clone());
        }
    }

    Ok((factory, props))
}
