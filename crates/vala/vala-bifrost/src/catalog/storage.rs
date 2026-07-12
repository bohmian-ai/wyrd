//! Iceberg `StorageFactory` and warehouse-URI derivation for the active backend.
//!
//! Both are pure functions of [`BackendConfig`] and are consumed only by
//! [`super::WyrdCatalog::new`]. They live here — with the iceberg cone — rather
//! than in `wyrd-storage`, which owns artifact blob storage, not the Iceberg
//! warehouse concept.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::io::StorageFactory;
use iceberg_storage_opendal::OpenDalResolvingStorageFactory;
use wyrd_storage::settings::BackendConfig;

/// Build an Iceberg `StorageFactory` and extra catalog properties for `backend`.
///
/// The factory is an `OpenDalResolvingStorageFactory` that auto-detects the URL
/// scheme and reads credentials from the ambient environment (IRSA, workload
/// identity, instance profile). The property map carries any backend-specific
/// hints (endpoint, region, account) that the Iceberg catalog embeds in table
/// metadata.
pub fn iceberg_storage_factory(
    backend: &BackendConfig,
) -> (Arc<dyn StorageFactory>, HashMap<String, String>) {
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

    (factory, props)
}

/// Derive the Iceberg warehouse base URI from the active backend configuration.
///
/// This is the single source of truth for where the catalog roots table
/// locations; the catalog appends `{namespace}/{name}` itself, so this returns
/// only the base (no trailing slash, no table path).
///
/// - `Local { root }` → `file://{root}`
/// - `S3` → `s3://{bucket}`
/// - `Gcs` → `gs://{bucket}`
/// - `Azure` → `abfss://{container}@{account}.dfs.core.windows.net`
pub fn warehouse_uri(backend: &BackendConfig) -> String {
    match backend {
        BackendConfig::Local { root } => format!("file://{}", root.display()),
        BackendConfig::S3(c) => format!("s3://{}", c.bucket),
        BackendConfig::Gcs(c) => format!("gs://{}", c.bucket),
        BackendConfig::Azure(c) => {
            format!("abfss://{}@{}.dfs.core.windows.net", c.container, c.account)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::warehouse_uri;
    use std::path::PathBuf;
    use wyrd_storage::settings::{AzureConfig, BackendConfig, GcsConfig, S3Config};

    #[test]
    fn local_warehouse_uri_is_file_scheme() {
        let backend = BackendConfig::Local {
            root: PathBuf::from("/var/lib/wyrd/warehouse"),
        };
        assert_eq!(warehouse_uri(&backend), "file:///var/lib/wyrd/warehouse");
    }

    #[test]
    fn s3_warehouse_uri_is_bucket_base() {
        let backend = BackendConfig::S3(S3Config {
            bucket: "wyrd-tables".to_owned(),
            region: Some("us-east-1".to_owned()),
            endpoint_url: None,
            force_path_style: false,
        });
        assert_eq!(warehouse_uri(&backend), "s3://wyrd-tables");
    }

    #[test]
    fn gcs_warehouse_uri_is_bucket_base() {
        let backend = BackendConfig::Gcs(GcsConfig {
            bucket: "wyrd-tables".to_owned(),
            endpoint_url: None,
        });
        assert_eq!(warehouse_uri(&backend), "gs://wyrd-tables");
    }

    #[test]
    fn azure_warehouse_uri_is_abfss_base() {
        let backend = BackendConfig::Azure(AzureConfig {
            account: "acct".to_owned(),
            container: "tables".to_owned(),
            endpoint_url: None,
        });
        assert_eq!(
            warehouse_uri(&backend),
            "abfss://tables@acct.dfs.core.windows.net"
        );
    }
}
