//! Backend SDK and opendal operator construction.

#[cfg(feature = "cloud")]
pub mod azure;
#[cfg(feature = "cloud")]
pub mod gcs;
pub mod local;
#[cfg(feature = "cloud")]
pub mod s3;

use std::time::Duration;

use crate::error::StorageError;
use crate::settings::BackendConfig;
use crate::signer::BackendSigner;
use opendal::Operator;
use opendal::layers::HttpClientLayer;
use opendal::raw::HttpClient;
use wyrd_spec::storage::StorageBackendKind;

/// Idle-connection lifetime for a self-hosted or emulated object store
/// reached through a configured endpoint.
///
/// Such servers close idle keep-alive connections on their own schedule
/// (`RustFS` after about five seconds). A request written to a pooled
/// connection just as the server closes it fails with "connection closed
/// before message completed", and Wyrd deliberately never retries storage
/// writes behind the caller, so the pool drops idle connections well before
/// the shortest known server timeout.
const ENDPOINT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(2);

/// Idle-connection lifetime for AWS S3, which closes idle keep-alive
/// connections after about 20 seconds.
#[cfg(feature = "cloud")]
const S3_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(15);

/// Idle-connection lifetime for managed GCS and Azure Blob Storage.
///
/// Both front ends keep idle connections for minutes (GCS about 610 seconds,
/// Azure 10 minutes), so the bound is the network path instead: 60 seconds
/// stays under the Azure outbound NAT's fixed 4-minute idle drop and the AWS
/// NAT gateway's 350 seconds while keeping connections hot between bursts.
#[cfg(feature = "cloud")]
const MANAGED_POOL_IDLE_TIMEOUT: Duration = Duration::from_mins(1);

/// Choose how long the operator's HTTP client may reuse an idle connection.
///
/// Connections stay pooled as long as the storage server is known to keep
/// them, so steady Bifrost traffic reuses warm TLS connections, and are
/// dropped before the server closes them so no request races that close.
/// Any configured endpoint names a server whose timeout Wyrd does not know,
/// so it takes the shortest bound.
fn pool_idle_timeout(backend: &BackendConfig) -> Duration {
    match backend {
        BackendConfig::Local { .. } => ENDPOINT_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::S3(c) if c.endpoint_url.is_none() => S3_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::Gcs(c) if c.endpoint_url.is_none() => MANAGED_POOL_IDLE_TIMEOUT,
        #[cfg(feature = "cloud")]
        BackendConfig::Azure(c) if c.endpoint_url.is_none() => MANAGED_POOL_IDLE_TIMEOUT,
        _ => ENDPOINT_POOL_IDLE_TIMEOUT,
    }
}

#[cfg(not(feature = "cloud"))]
fn cloud_disabled(backend: &BackendConfig) -> StorageError {
    let backend_kind = match backend {
        BackendConfig::Local { .. } => StorageBackendKind::Local,
        BackendConfig::S3(_) => StorageBackendKind::S3,
        BackendConfig::Gcs(_) => StorageBackendKind::Gcs,
        BackendConfig::Azure(_) => StorageBackendKind::Azure,
    };
    StorageError::Backend {
        backend: backend_kind,
        op: "build_signer",
        message: "cloud storage backends are not compiled in this build; rebuild with the `cloud` feature".to_owned(),
    }
}

/// Refuse a GCS or Azure endpoint override in a build without `emulator`.
///
/// The production GCS and Azure signers address only the public provider, so
/// honoring `WYRD_STORAGE_ENDPOINT_URL` for the data plane while signing
/// against the real provider would split one backend across two services.
#[cfg(all(feature = "cloud", not(feature = "emulator")))]
fn endpoint_requires_emulator(backend: StorageBackendKind) -> StorageError {
    StorageError::Backend {
        backend,
        op: "build_signer",
        message: "WYRD_STORAGE_ENDPOINT_URL is supported for this backend only in builds with the `emulator` feature".to_owned(),
    }
}

/// Build the active backend signer in a cloud-enabled build.
///
/// Cloud backends may perform SDK setup and boot probing, so this variant
/// awaits the backend-specific construction IO. Local construction remains
/// synchronous inside the same future.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process. Other storage errors report backend SDK
/// construction or boot-probe failures. Cancellation can interrupt a remote
/// probe without constructing a signer; it makes no durable storage changes.
#[cfg(feature = "cloud")]
pub async fn build_signer(backend: &BackendConfig) -> Result<BackendSigner, StorageError> {
    wyrd_tls::install_crypto_provider()?;
    match backend {
        BackendConfig::Local { root } => {
            Ok(BackendSigner::Local(local::build_signer(root.clone())?))
        }
        BackendConfig::S3(config) => Ok(BackendSigner::Cloud(Box::new(
            crate::cloud::CloudSigner::S3(s3::build_signer(config).await?),
        ))),
        BackendConfig::Gcs(config) => Ok(BackendSigner::Cloud(Box::new(
            crate::cloud::CloudSigner::Gcs(gcs::build_signer(config).await?),
        ))),
        BackendConfig::Azure(config) => Ok(BackendSigner::Cloud(Box::new(
            crate::cloud::CloudSigner::Azure(azure::build_signer(config).await?),
        ))),
    }
}

/// Build the active backend signer in a build without cloud support.
///
/// The returned immediately-ready future preserves the awaitable factory API
/// without creating an asynchronous state machine for local construction or a
/// deterministic cloud-capability error.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process, a storage error when local signer construction
/// fails, or a cloud-capability error when a cloud backend is selected without
/// the `cloud` feature.
#[cfg(not(feature = "cloud"))]
pub fn build_signer(
    backend: &BackendConfig,
) -> std::future::Ready<Result<BackendSigner, StorageError>> {
    let result = match wyrd_tls::install_crypto_provider() {
        Ok(()) => match backend {
            BackendConfig::Local { root } => {
                local::build_signer(root.clone()).map(BackendSigner::Local)
            }
            BackendConfig::S3(_) | BackendConfig::Gcs(_) | BackendConfig::Azure(_) => {
                Err(cloud_disabled(backend))
            }
        },
        Err(error) => Err(StorageError::from(error)),
    };
    std::future::ready(result)
}

/// Finish an `OpenDAL` operator over the operator-owned HTTP client.
///
/// Every operator gets its own client whose idle pool lasts `pool_idle`
/// (see [`pool_idle_timeout`]) instead of `OpenDAL`'s process-global default,
/// whose 90-second idle pool outlives some servers' keep-alive and hands out
/// connections the server is closing.
///
/// # Errors
/// Returns [`StorageError::Backend`] when the HTTP client or the operator
/// cannot be constructed.
fn finish_op<B: opendal::Builder>(
    builder: B,
    backend: StorageBackendKind,
    pool_idle: Duration,
) -> Result<Operator, StorageError> {
    let backend_error = |message: String| StorageError::Backend {
        backend,
        op: "build_operator",
        message,
    };
    let client = reqwest::Client::builder()
        .pool_idle_timeout(pool_idle)
        .build()
        .map_err(|e| backend_error(e.to_string()))?;
    Ok(Operator::new(builder)
        .map_err(|e| backend_error(e.to_string()))?
        .layer(HttpClientLayer::new(HttpClient::with(client)))
        .finish())
}

/// Build the opendal `Operator` for the selected backend.
///
/// # Errors
/// Returns [`StorageError::CryptoProvider`] when another Rustls provider
/// already owns the process, or a storage error when operator construction
/// fails.
pub fn build_operator(backend: &BackendConfig) -> Result<Operator, StorageError> {
    wyrd_tls::install_crypto_provider()?;
    let pool_idle = pool_idle_timeout(backend);
    match backend {
        BackendConfig::Local { root } => finish_op(
            local::fs_service(root),
            StorageBackendKind::Local,
            pool_idle,
        ),
        #[cfg(feature = "cloud")]
        BackendConfig::S3(c) => finish_op(s3::s3_service(c), StorageBackendKind::S3, pool_idle),
        #[cfg(feature = "cloud")]
        BackendConfig::Gcs(c) => finish_op(gcs::gcs_service(c), StorageBackendKind::Gcs, pool_idle),
        #[cfg(feature = "cloud")]
        BackendConfig::Azure(c) => finish_op(
            azure::azblob_service(c),
            StorageBackendKind::Azure,
            pool_idle,
        ),
        #[cfg(not(feature = "cloud"))]
        BackendConfig::S3(_) => Err(cloud_disabled(backend)),
        #[cfg(not(feature = "cloud"))]
        BackendConfig::Gcs(_) => Err(cloud_disabled(backend)),
        #[cfg(not(feature = "cloud"))]
        BackendConfig::Azure(_) => Err(cloud_disabled(backend)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "cloud")]
    use crate::settings::{AzureConfig, GcsConfig, S3Config};
    use opendal::ErrorKind;

    #[tokio::test]
    async fn fs_operator_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = BackendConfig::Local {
            root: dir.path().to_path_buf(),
        };
        let op = build_operator(&backend).expect("build local operator");

        op.write("probe.txt", b"hello" as &[u8])
            .await
            .expect("write");
        let meta = op.stat("probe.txt").await.expect("stat");
        assert_eq!(meta.content_length(), 5);
        let buf = op.read("probe.txt").await.expect("read");
        assert_eq!(buf.to_bytes().as_ref(), b"hello");
        op.delete("probe.txt").await.expect("delete");
    }

    #[tokio::test]
    async fn fs_stat_missing_is_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = BackendConfig::Local {
            root: dir.path().to_path_buf(),
        };
        let op = build_operator(&backend).expect("build local operator");

        let err = op
            .stat("does-not-exist.txt")
            .await
            .expect_err("should be NotFound");
        assert_eq!(err.kind(), ErrorKind::NotFound);
    }

    /// Managed cloud stores keep warm connections for as long as each
    /// provider holds them; any configured endpoint takes the short bound
    /// because its server's keep-alive is unknown.
    ///
    /// # Panics
    /// Panics when any backend selects an idle bound other than the one its
    /// provider or configured endpoint requires.
    #[cfg(feature = "cloud")]
    #[test]
    fn pool_idle_timeout_matches_the_storage_server() {
        let endpoint = Some("http://127.0.0.1:9000".to_owned());
        let s3 = |endpoint_url| {
            BackendConfig::S3(S3Config {
                bucket: "b".to_owned(),
                region: None,
                endpoint_url,
            })
        };
        let gcs = |endpoint_url| {
            BackendConfig::Gcs(GcsConfig {
                bucket: "b".to_owned(),
                endpoint_url,
            })
        };
        let azure = |endpoint_url| {
            BackendConfig::Azure(AzureConfig {
                account: "a".to_owned(),
                container: "c".to_owned(),
                endpoint_url,
            })
        };

        assert_eq!(pool_idle_timeout(&s3(None)), S3_POOL_IDLE_TIMEOUT);
        assert_eq!(pool_idle_timeout(&gcs(None)), MANAGED_POOL_IDLE_TIMEOUT);
        assert_eq!(pool_idle_timeout(&azure(None)), MANAGED_POOL_IDLE_TIMEOUT);
        for backend in [s3(endpoint.clone()), gcs(endpoint.clone()), azure(endpoint)] {
            assert_eq!(pool_idle_timeout(&backend), ENDPOINT_POOL_IDLE_TIMEOUT);
        }
    }
}

/// Operator tests that bind a local socket and serve HTTP.
///
/// They need a live listener, so they live in `pg_tests`, which the fast
/// family lanes skip, and run only through their exact focused invocation.
#[cfg(all(test, feature = "cloud"))]
mod pg_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use opendal::ErrorKind;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;

    use super::*;
    use crate::settings::S3Config;

    /// Serves one S3 `HEAD` per connection with `404`, then closes the
    /// connection as soon as a second request arrives on it without answering,
    /// the way an object store closing an idle keep-alive connection races a
    /// reused one. Returns the endpoint and the accepted-connection counter.
    ///
    /// # Panics
    /// Panics when the loopback listener cannot bind or report its address.
    async fn closing_keep_alive_server() -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let endpoint = format!("http://{}", listener.local_addr().expect("local addr"));
        let accepted = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&accepted);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                counter.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut chunk = [0_u8; 1024];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => request.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let reply = b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n";
                    if socket.write_all(reply).await.is_err() {
                        return;
                    }
                    // Any further request on this connection is dropped unanswered.
                    let _ = socket.read(&mut chunk).await;
                });
            }
        });
        (endpoint, accepted)
    }

    /// A request after an idle gap longer than [`ENDPOINT_POOL_IDLE_TIMEOUT`] opens a
    /// fresh connection instead of reusing one the server is closing, so the
    /// second `stat` still reports `NotFound` rather than a transport error.
    ///
    /// # Panics
    /// Panics when the listener or operator cannot be built, when either
    /// `stat` does not fail with `NotFound`, or when the server did not accept
    /// exactly two connections.
    #[tokio::test]
    async fn idle_connection_is_not_reused_after_the_pool_timeout() {
        let (endpoint, accepted) = closing_keep_alive_server().await;
        let backend = BackendConfig::S3(S3Config {
            bucket: "probe".to_owned(),
            region: Some("us-east-1".to_owned()),
            endpoint_url: Some(endpoint),
        });
        temp_env::async_with_vars(
            [
                ("AWS_ACCESS_KEY_ID", Some("test-key")),
                ("AWS_SECRET_ACCESS_KEY", Some("test-secret")),
                ("AWS_EC2_METADATA_DISABLED", Some("true")),
            ],
            async {
                let op = build_operator(&backend).expect("build s3 operator");
                let first = op.stat("_wyrd/health.sentinel").await.expect_err("404");
                assert_eq!(first.kind(), ErrorKind::NotFound);
                tokio::time::sleep(ENDPOINT_POOL_IDLE_TIMEOUT + Duration::from_millis(500)).await;
                let second = op.stat("_wyrd/health.sentinel").await.expect_err("404");
                assert_eq!(second.kind(), ErrorKind::NotFound, "{second}");
            },
        )
        .await;
        assert_eq!(accepted.load(Ordering::SeqCst), 2);
    }
}
