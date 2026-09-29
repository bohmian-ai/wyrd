//! Redux-owned Iceberg SQL catalog construction.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::CatalogBuilder;
use iceberg::io::StorageFactory;
use iceberg_catalog_sql::{SqlBindStyle, SqlCatalog, SqlCatalogBuilder};

use crate::catalog::BifrostCatalogError;

/// Build the Iceberg SQL catalog used by Redux Gate, Forge, and Oracle.
///
/// # Errors
/// Returns [`BifrostCatalogError::Iceberg`] when the catalog cannot be loaded.
pub async fn build_catalog<S: std::hash::BuildHasher>(
    catalog_uri: &str,
    warehouse: &str,
    storage_factory: Arc<dyn StorageFactory>,
    storage_properties: HashMap<String, String, S>,
) -> Result<SqlCatalog, BifrostCatalogError> {
    sqlx_catalog::any::install_default_drivers();

    let mut properties = storage_properties;
    properties.insert(
        iceberg_catalog_sql::SQL_CATALOG_PROP_BIND_STYLE.to_owned(),
        SqlBindStyle::DollarNumeric.to_string(),
    );

    SqlCatalogBuilder::default()
        .with_storage_factory(storage_factory)
        .load(
            super::BIFROST_CATALOG_NAME,
            [
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_URI.to_owned(),
                    catalog_uri.to_owned(),
                ),
                (
                    iceberg_catalog_sql::SQL_CATALOG_PROP_WAREHOUSE.to_owned(),
                    warehouse.to_owned(),
                ),
            ]
            .into_iter()
            .chain(properties)
            .collect(),
        )
        .await
        .map_err(BifrostCatalogError::Iceberg)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use iceberg::io::MemoryStorageFactory;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;

    use super::build_catalog;

    /// Postgres `SSLRequest`: length 8, then the magic code 80877103.
    const SSL_REQUEST: [u8; 8] = [0, 0, 0, 8, 0x04, 0xd2, 0x16, 0x2f];

    /// Proves the catalog's own sqlx can negotiate TLS, so a production
    /// `sslmode=verify-full` catalog URI reaches the server instead of failing
    /// with `SQLx was built without TLS support`. A local listener stands in for
    /// Postgres: it records the `SSLRequest` and answers `N` (no TLS), which a
    /// TLS-capable client with `sslmode=require` must refuse.
    #[tokio::test]
    async fn catalog_sqlx_negotiates_tls() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("local addr").port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = [0_u8; 8];
            socket.read_exact(&mut request).await.expect("read request");
            socket.write_all(b"N").await.expect("refuse TLS");
            request
        });

        let Err(error) = build_catalog(
            &format!("postgres://wyrd:pw@127.0.0.1:{port}/wyrd?sslmode=require"),
            "memory://warehouse",
            Arc::new(MemoryStorageFactory),
            HashMap::<String, String>::new(),
        )
        .await
        else {
            panic!("a server without TLS must be refused under sslmode=require");
        };
        let error = error.to_string();

        assert!(
            !error.contains("without TLS support"),
            "the catalog's sqlx has no TLS backend: {error}"
        );
        assert_eq!(server.await.expect("server task"), SSL_REQUEST);
    }
}
