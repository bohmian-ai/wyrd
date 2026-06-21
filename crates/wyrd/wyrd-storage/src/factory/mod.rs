//! Backend SDK and opendal operator construction.

pub mod azure;
pub mod gcs;
pub mod local;
pub mod s3;

use crate::error::StorageError;
use crate::settings::BackendConfig;
use crate::signer::BackendSigner;
use opendal::Operator;
use wyrd_spec::storage::StorageBackendKind;

/// Build the active backend signer.
///
/// # Errors
/// Returns a storage error when SDK construction or boot probing fails.
pub async fn build_signer(backend: &BackendConfig) -> Result<BackendSigner, StorageError> {
    match backend {
        BackendConfig::Local { root } => {
            Ok(BackendSigner::Local(local::build_signer(root.clone())?))
        }
        BackendConfig::S3(config) => Ok(BackendSigner::S3(s3::build_signer(config).await?)),
        BackendConfig::Gcs(config) => Ok(BackendSigner::Gcs(gcs::build_signer(config).await?)),
        BackendConfig::Azure(config) => {
            Ok(BackendSigner::Azure(azure::build_signer(config).await?))
        }
    }
}

fn finish_op<B: opendal::Builder>(
    builder: B,
    backend: StorageBackendKind,
) -> Result<Operator, StorageError> {
    Ok(Operator::new(builder)
        .map_err(|e| StorageError::Backend {
            backend,
            op: "build_operator",
            message: e.to_string(),
        })?
        .finish())
}

/// Build the opendal `Operator` for the selected backend.
///
/// # Errors
/// Returns a storage error when operator construction fails.
pub fn build_operator(backend: &BackendConfig) -> Result<Operator, StorageError> {
    match backend {
        BackendConfig::Local { root } => finish_op(local::fs_service(root), StorageBackendKind::Local),
        BackendConfig::S3(c) => finish_op(s3::s3_service(c), StorageBackendKind::S3),
        BackendConfig::Gcs(c) => finish_op(gcs::gcs_service(c), StorageBackendKind::Gcs),
        BackendConfig::Azure(c) => finish_op(azure::azblob_service(c), StorageBackendKind::Azure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
