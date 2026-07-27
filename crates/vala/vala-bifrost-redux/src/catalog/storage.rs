//! Redux Iceberg storage factory and warehouse URI derivation.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::io::StorageFactory;
use iceberg_storage_opendal::OpenDalResolvingStorageFactory;
use wyrd_storage::settings::BackendConfig;

/// Build the Iceberg storage factory and backend-specific catalog properties.
#[must_use]
pub fn iceberg_storage_factory(
    backend: &BackendConfig,
) -> (Arc<dyn StorageFactory>, HashMap<String, String>) {
    let factory = Arc::new(OpenDalResolvingStorageFactory::new()) as Arc<dyn StorageFactory>;
    let mut properties = HashMap::new();

    match backend {
        BackendConfig::Local { .. } => {}
        BackendConfig::S3(config) => {
            if let Some(endpoint) = &config.endpoint_url {
                properties.insert("s3.endpoint".to_owned(), endpoint.clone());
            }
            if let Some(region) = &config.region {
                properties.insert("s3.region".to_owned(), region.clone());
            }
            if config.force_path_style {
                properties.insert("s3.path-style-access".to_owned(), "true".to_owned());
            }
        }
        BackendConfig::Gcs(config) => {
            if let Some(endpoint) = &config.endpoint_url {
                properties.insert("gcs.service.path".to_owned(), endpoint.clone());
            }
        }
        BackendConfig::Azure(config) => {
            properties.insert("adls.account-name".to_owned(), config.account.clone());
        }
    }

    (factory, properties)
}

/// Derive the base Iceberg warehouse URI for the active storage backend.
#[must_use]
pub fn warehouse_uri(backend: &BackendConfig) -> String {
    match backend {
        BackendConfig::Local { root } => format!("file://{}", root.display()),
        BackendConfig::S3(config) => format!("s3://{}", config.bucket),
        BackendConfig::Gcs(config) => format!("gs://{}", config.bucket),
        BackendConfig::Azure(config) => format!(
            "abfss://{}@{}.dfs.core.windows.net",
            config.container, config.account
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use wyrd_storage::settings::{AzureConfig, GcsConfig, S3Config};

    use super::*;

    #[test]
    fn warehouse_uri_matches_each_backend() {
        assert_eq!(
            warehouse_uri(&BackendConfig::Local {
                root: PathBuf::from("/var/lib/wyrd/warehouse"),
            }),
            "file:///var/lib/wyrd/warehouse"
        );
        assert_eq!(
            warehouse_uri(&BackendConfig::S3(S3Config {
                bucket: "tables".to_owned(),
                region: None,
                endpoint_url: None,
                force_path_style: false,
            })),
            "s3://tables"
        );
        assert_eq!(
            warehouse_uri(&BackendConfig::Gcs(GcsConfig {
                bucket: "tables".to_owned(),
                endpoint_url: None,
            })),
            "gs://tables"
        );
        assert_eq!(
            warehouse_uri(&BackendConfig::Azure(AzureConfig {
                account: "account".to_owned(),
                container: "tables".to_owned(),
                endpoint_url: None,
            })),
            "abfss://tables@account.dfs.core.windows.net"
        );
    }
}
