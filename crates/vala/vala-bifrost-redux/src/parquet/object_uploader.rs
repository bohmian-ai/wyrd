//! Bounded, content-verifying Parquet object upload shared by data-plane roles.

use std::path::Path;
use std::time::Instant;

use opendal::{Buffer, ErrorKind, Operator};
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt as _;

#[cfg(test)]
use std::sync::Arc;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Deterministic object-store identity selected by the publishing owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParquetObjectIdentity(String);

impl ParquetObjectIdentity {
    /// Validates an object key before it reaches the storage boundary.
    ///
    /// # Errors
    ///
    /// Returns [`ParquetUploadError::InvalidIdentity`] for empty, absolute, or
    /// traversal-bearing keys.
    pub fn new(key: impl Into<String>) -> Result<Self, ParquetUploadError> {
        let key = key.into();
        if key.is_empty()
            || key.starts_with('/')
            || key.ends_with('/')
            || key.contains('\\')
            || key.chars().any(char::is_control)
            || key
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(ParquetUploadError::InvalidIdentity);
        }
        Ok(Self(key))
    }

    /// Returns the validated backend-relative object key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Closed producer role used for bounded upload telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BifrostUploadRole {
    /// Scribe immutable persistence.
    Scribe,
    /// Forge compaction publication.
    Forge,
}

/// Verified result returned only after remote content convergence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedParquetObject {
    /// Deterministic remote identity.
    pub identity: ParquetObjectIdentity,
    /// SHA-256 digest verified from remote bytes.
    pub sha256: [u8; 32],
    /// Exact verified remote length.
    pub length: u64,
}

/// Failure modes of bounded verified upload.
#[derive(Debug, thiserror::Error)]
pub enum ParquetUploadError {
    /// The owner supplied an unsafe or ambiguous key.
    #[error("invalid Parquet object identity")]
    InvalidIdentity,
    /// The caller did not provide governed transfer storage.
    #[error("upload chunk must not be empty")]
    EmptyChunk,
    /// Local staged content could not be read.
    #[error("local staged Parquet IO failed: {0}")]
    LocalIo(#[from] std::io::Error),
    /// Object storage did not converge to the expected bytes.
    #[error("Parquet object storage operation failed: {0}")]
    ObjectStore(#[from] opendal::Error),
    /// Remote bytes contradict the manifest-fixed identity.
    #[error("remote Parquet content contradicts expected SHA-256 or length")]
    ContentContradiction,
}

/// Shared uploader that owns PUT/read-back convergence but no deletion authority.
pub struct BifrostParquetUploader {
    /// Durable backend used for both upload attempts and authoritative read-back.
    operator: Operator,
    /// Deterministic ambiguity seams used only by the uploader contract test.
    #[cfg(test)]
    faults: UploadFaults,
}

/// One-shot storage outcomes that exercise convergence without a mock uploader.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
struct UploadFaults {
    /// Fails before the first remote mutation so read-back observes absence.
    fail_before_put: Arc<AtomicBool>,
    /// Fails after close so read-back must recognize the completed object.
    fail_after_close: Arc<AtomicBool>,
    /// Largest OpenDAL-owned write or read payload observed by the real backend.
    max_backend_payload_bytes: Arc<AtomicUsize>,
    /// Number of conditional creates completed by the real backend.
    successful_creates: Arc<AtomicUsize>,
}

impl BifrostParquetUploader {
    /// Creates an uploader over the configured durable object store.
    #[must_use]
    pub fn new(operator: Operator) -> Self {
        Self {
            operator,
            #[cfg(test)]
            faults: UploadFaults::default(),
        }
    }

    /// Arms one absence-first PUT attempt for deterministic convergence proof.
    #[cfg(test)]
    fn fail_next_put_before_mutation(&self) {
        self.faults.fail_before_put.store(true, Ordering::Release);
    }

    /// Arms one close-ambiguous PUT after the remote bytes are complete.
    #[cfg(test)]
    fn fail_next_put_after_close(&self) {
        self.faults.fail_after_close.store(true, Ordering::Release);
    }

    /// Returns the largest backend-owned payload created by a real upload or read-back.
    #[cfg(test)]
    fn max_backend_payload_bytes(&self) -> usize {
        self.faults
            .max_backend_payload_bytes
            .load(Ordering::Acquire)
    }

    /// Returns the number of conditional object creations completed by this uploader.
    #[cfg(test)]
    fn successful_creates(&self) -> usize {
        self.faults.successful_creates.load(Ordering::Acquire)
    }

    /// Records one unavoidable OpenDAL-owned payload after bounding its length.
    #[cfg(test)]
    fn record_backend_payload(&self, bytes: usize) {
        self.faults
            .max_backend_payload_bytes
            .fetch_max(bytes, Ordering::AcqRel);
    }

    /// Streams a local file through a caller-governed chunk and verifies remote bytes.
    ///
    /// A failed or ambiguous PUT is resolved by read-back. Absence is retried once;
    /// contradictory content fails closed. `OpenDAL` requires one owned payload for
    /// each write or range read; every such payload is bounded by `chunk.len()`,
    /// and producer admission charges both the retained caller chunk and that
    /// backend-owned buffer. This owner never deletes remote bytes.
    ///
    /// # Errors
    ///
    /// Returns an IO/storage error, [`ParquetUploadError::EmptyChunk`], or
    /// [`ParquetUploadError::ContentContradiction`] when remote identity differs.
    pub async fn upload_file_verified(
        &self,
        identity: &ParquetObjectIdentity,
        source: &Path,
        expected_sha256: [u8; 32],
        expected_len: u64,
        chunk: &mut [u8],
        role: BifrostUploadRole,
    ) -> Result<VerifiedParquetObject, ParquetUploadError> {
        if chunk.is_empty() {
            return Err(ParquetUploadError::EmptyChunk);
        }
        let started = Instant::now();
        let result = self
            .converge(identity, source, expected_sha256, expected_len, chunk)
            .await;
        let outcome = match &result {
            Ok(_) => "verified",
            Err(ParquetUploadError::ContentContradiction) => "contradiction",
            Err(_) => "error",
        };
        metrics::counter!(
            "bifrost_parquet_upload_outcomes_total",
            "role" => role_label(role),
            "outcome" => outcome
        )
        .increment(1);
        metrics::histogram!(
            "bifrost_parquet_upload_duration_seconds",
            "role" => role_label(role),
            "outcome" => outcome
        )
        .record(started.elapsed().as_secs_f64());
        if let Ok(verified) = &result {
            metrics::counter!("bifrost_parquet_upload_bytes", "role" => role_label(role))
                .increment(verified.length);
        }
        result
    }

    /// Attempts an atomic manifest-fixed create and resolves outcomes by read-back.
    ///
    /// # Errors
    ///
    /// Returns local IO or object-store failures when convergence cannot be
    /// established, and [`ParquetUploadError::ContentContradiction`] when remote
    /// length or content differs from the manifest-fixed expectation.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave the deterministic remote key absent, partially
    /// written, or complete. A retry is safe because it verifies the same fixed
    /// length and digest and never deletes remote content.
    async fn converge(
        &self,
        identity: &ParquetObjectIdentity,
        source: &Path,
        expected_sha256: [u8; 32],
        expected_len: u64,
        chunk: &mut [u8],
    ) -> Result<VerifiedParquetObject, ParquetUploadError> {
        let mut last_error = None;
        for attempt in 0..2 {
            if let Err(error) = self.put_file(identity, source, chunk).await {
                last_error = Some(error);
            }
            match self
                .verify_remote(identity, expected_sha256, expected_len, chunk)
                .await
            {
                Ok(verified) => return Ok(verified),
                Err(error @ ParquetUploadError::ContentContradiction) => return Err(error),
                Err(ParquetUploadError::ObjectStore(error))
                    if error.kind() == ErrorKind::NotFound && attempt == 0 =>
                {
                    // A failed conditional create can race object removal. The bounded
                    // retry remains conditional, so it cannot overwrite a later winner.
                }
                Err(error) if attempt == 0 => last_error = Some(error),
                Err(error) => return Err(last_error.unwrap_or(error)),
            }
        }
        Err(last_error.expect("two convergence attempts always record a terminal error"))
    }

    /// Streams one atomic local-file create through the caller-governed chunk.
    ///
    /// `OpenDAL` takes one owned [`Buffer`] per write. The unavoidable copy is
    /// bounded by the caller chunk and is the second transfer buffer charged by
    /// Scribe/Forge producer admission.
    ///
    /// # Errors
    ///
    /// Returns [`ParquetUploadError`] when opening or reading the local stage,
    /// opening or writing the remote writer, or closing the PUT fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation can leave an ambiguous remote object at the deterministic
    /// key. The caller must resolve that state through read-back convergence.
    async fn put_file(
        &self,
        identity: &ParquetObjectIdentity,
        source: &Path,
        chunk: &mut [u8],
    ) -> Result<(), ParquetUploadError> {
        #[cfg(test)]
        if self.faults.fail_before_put.swap(false, Ordering::AcqRel) {
            return Err(
                opendal::Error::new(ErrorKind::Unexpected, "injected PUT absence")
                    .set_temporary()
                    .into(),
            );
        }
        let mut file = File::open(source).await?;
        let mut writer = self
            .operator
            .writer_with(identity.as_str())
            .if_not_exists(true)
            .await?;
        loop {
            let read = file.read(chunk).await?;
            if read == 0 {
                break;
            }
            let owned = Buffer::from(chunk[..read].to_vec());
            #[cfg(test)]
            self.record_backend_payload(owned.len());
            writer.write(owned).await?;
        }
        writer.close().await?;
        #[cfg(test)]
        self.faults
            .successful_creates
            .fetch_add(1, Ordering::AcqRel);
        #[cfg(test)]
        if self.faults.fail_after_close.swap(false, Ordering::AcqRel) {
            return Err(
                opendal::Error::new(ErrorKind::Unexpected, "injected ambiguous close")
                    .set_temporary()
                    .into(),
            );
        }
        Ok(())
    }

    /// Reads bounded remote ranges and validates content identity.
    ///
    /// `OpenDAL` returns an owned range buffer rather than filling caller memory.
    /// The caller chunk therefore provides the authoritative range bound while
    /// producer admission charges both the retained chunk and one backend buffer.
    ///
    /// # Errors
    ///
    /// Returns an object-store error when metadata or range reads fail, and
    /// [`ParquetUploadError::ContentContradiction`] for any length, progress, or
    /// digest mismatch.
    ///
    /// # Cancellation
    ///
    /// Cancellation has no remote side effect; a later verification can restart
    /// from offset zero using the same expected identity.
    async fn verify_remote(
        &self,
        identity: &ParquetObjectIdentity,
        expected_sha256: [u8; 32],
        expected_len: u64,
        chunk: &mut [u8],
    ) -> Result<VerifiedParquetObject, ParquetUploadError> {
        let metadata = self.operator.stat(identity.as_str()).await?;
        if metadata.content_length() != expected_len {
            return Err(ParquetUploadError::ContentContradiction);
        }
        let reader = self.operator.reader(identity.as_str()).await?;
        let mut digest = Sha256::new();
        let mut offset = 0_u64;
        while offset < expected_len {
            let end = offset.saturating_add(chunk.len() as u64).min(expected_len);
            let remote = reader.read(offset..end).await?;
            if remote.is_empty() {
                return Err(ParquetUploadError::ContentContradiction);
            }
            if remote.len() > chunk.len() {
                return Err(ParquetUploadError::ContentContradiction);
            }
            let remote_len = remote.len();
            #[cfg(test)]
            self.record_backend_payload(remote_len);
            for bytes in remote {
                digest.update(&bytes);
            }
            offset = offset
                .checked_add(remote_len as u64)
                .ok_or(ParquetUploadError::ContentContradiction)?;
        }
        let actual_sha256: [u8; 32] = digest.finalize().into();
        if offset != expected_len || actual_sha256 != expected_sha256 {
            return Err(ParquetUploadError::ContentContradiction);
        }
        Ok(VerifiedParquetObject {
            identity: identity.clone(),
            sha256: actual_sha256,
            length: offset,
        })
    }
}

/// Maps the closed role enum to its bounded telemetry label.
const fn role_label(role: BifrostUploadRole) -> &'static str {
    match role {
        BifrostUploadRole::Scribe => "scribe",
        BifrostUploadRole::Forge => "forge",
    }
}

#[cfg(test)]
mod tests {
    use opendal::services::Memory;

    use super::*;

    /// Builds the real `OpenDAL` memory backend used by uploader contract tests.
    fn memory_operator() -> Operator {
        Operator::new(Memory::default())
            .expect("memory backend configuration")
            .finish()
    }

    /// Proves unsafe or normalization-dependent keys never reach `OpenDAL`.
    #[test]
    fn deterministic_identity_validation_rejects_ambiguous_keys() {
        for key in ["", "/a", "a/", "a//b", "a/./b", "a/../b", "a\\b"] {
            assert!(matches!(
                ParquetObjectIdentity::new(key),
                Err(ParquetUploadError::InvalidIdentity)
            ));
        }
        assert!(ParquetObjectIdentity::new("tenant/table/file.parquet").is_ok());
    }

    /// Proves telemetry role cardinality is closed to the two durable producers.
    #[test]
    fn upload_roles_are_closed_to_scribe_and_forge() {
        assert_eq!(role_label(BifrostUploadRole::Scribe), "scribe");
        assert_eq!(role_label(BifrostUploadRole::Forge), "forge");
    }

    /// Proves bounded convergence, contradiction detection, and absent delete authority.
    #[tokio::test]
    async fn shared_uploader_resolves_success_timeout_absence_and_digest_contradiction_without_remote_delete()
     {
        let operator = memory_operator();
        let uploader = BifrostParquetUploader::new(operator.clone());
        let identity = ParquetObjectIdentity::new("scribe/tenant/file.parquet").expect("identity");
        let content = b"larger than the caller transfer chunk";
        let directory = tempfile::tempdir().expect("temporary stage");
        let source = directory.path().join("stage.parquet");
        tokio::fs::write(&source, content)
            .await
            .expect("stage write");
        let expected_sha256 = Sha256::digest(content).into();
        let mut chunk = [0_u8; 5];
        uploader.fail_next_put_before_mutation();
        let result = uploader
            .upload_file_verified(
                &identity,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Scribe,
            )
            .await
            .expect("verified upload");
        assert_eq!(result.length, content.len() as u64);
        assert_eq!(result.sha256, expected_sha256);

        let timeout_identity =
            ParquetObjectIdentity::new("forge/tenant/ambiguous.parquet").expect("identity");
        uploader.fail_next_put_after_close();
        let ambiguous = uploader
            .upload_file_verified(
                &timeout_identity,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Forge,
            )
            .await
            .expect("close ambiguity converges by full read-back");
        assert_eq!(ambiguous.sha256, expected_sha256);
        assert_eq!(ambiguous.length, content.len() as u64);

        tokio::fs::write(&source, b"different")
            .await
            .expect("replace stage");
        let unchanged = uploader
            .upload_file_verified(
                &identity,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Forge,
            )
            .await
            .expect("existing immutable object wins over changed local source");
        assert_eq!(unchanged.sha256, expected_sha256);
        assert_eq!(
            operator
                .read(identity.as_str())
                .await
                .expect("original remote bytes")
                .to_vec(),
            content
        );

        let contradiction_identity =
            ParquetObjectIdentity::new("scribe/tenant/contradiction.parquet").expect("identity");
        operator
            .write(contradiction_identity.as_str(), b"different".to_vec())
            .await
            .expect("seed conflicting remote object");
        let error = uploader
            .upload_file_verified(
                &contradiction_identity,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Forge,
            )
            .await
            .expect_err("contradiction fails closed");
        assert!(matches!(error, ParquetUploadError::ContentContradiction));
    }

    /// Proves one conditional creator wins and a concurrent loser verifies its bytes.
    ///
    /// # Panics
    ///
    /// Panics if either uploader cannot converge on the same immutable remote object.
    #[tokio::test]
    async fn concurrent_same_key_uploads_create_once_and_verify_the_winner() {
        let operator = memory_operator();
        let uploader = Arc::new(BifrostParquetUploader::new(operator.clone()));
        let identity =
            ParquetObjectIdentity::new("scribe/tenant/concurrent.parquet").expect("identity");
        let content = b"concurrent immutable Parquet bytes";
        let directory = tempfile::tempdir().expect("temporary stage");
        let source = directory.path().join("concurrent.parquet");
        tokio::fs::write(&source, content)
            .await
            .expect("stage write");
        let expected_sha256 = Sha256::digest(content).into();
        let mut first_chunk = [0_u8; 5];
        let mut second_chunk = [0_u8; 7];
        let first = uploader.upload_file_verified(
            &identity,
            &source,
            expected_sha256,
            content.len() as u64,
            &mut first_chunk,
            BifrostUploadRole::Scribe,
        );
        let second = uploader.upload_file_verified(
            &identity,
            &source,
            expected_sha256,
            content.len() as u64,
            &mut second_chunk,
            BifrostUploadRole::Forge,
        );
        let (first, second) = tokio::join!(first, second);
        assert_eq!(first.expect("first converges").sha256, expected_sha256);
        assert_eq!(second.expect("second converges").sha256, expected_sha256);
        assert_eq!(uploader.successful_creates(), 1);
        assert_eq!(
            operator
                .read(identity.as_str())
                .await
                .expect("remote bytes")
                .to_vec(),
            content
        );
    }

    /// Proves upload cannot allocate without caller-owned governed transfer memory.
    #[tokio::test]
    async fn caller_owned_chunk_must_be_nonempty() {
        let uploader = BifrostParquetUploader::new(memory_operator());
        let identity = ParquetObjectIdentity::new("scribe/file.parquet").expect("identity");
        let error = uploader
            .upload_file_verified(
                &identity,
                Path::new("unused"),
                Sha256::digest([]).into(),
                0,
                &mut [],
                BifrostUploadRole::Scribe,
            )
            .await
            .expect_err("empty chunk refused");
        assert!(matches!(error, ParquetUploadError::EmptyChunk));
    }

    /// The real `OpenDAL` writer and reader never own more than one governed chunk.
    ///
    /// # Panics
    ///
    /// Panics if a write/range-read payload exceeds the caller bound or the
    /// producer projection omits either simultaneously retained transfer buffer.
    #[tokio::test]
    async fn shared_uploader_bounds_backend_owned_payloads_by_governed_chunk() {
        let uploader = BifrostParquetUploader::new(memory_operator());
        let identity =
            ParquetObjectIdentity::new("scribe/tenant/bounded.parquet").expect("identity");
        let content = b"backend payloads span several governed chunks";
        let directory = tempfile::tempdir().expect("temporary stage");
        let source = directory.path().join("bounded.parquet");
        tokio::fs::write(&source, content)
            .await
            .expect("stage write");
        let mut chunk = [0_u8; 7];
        uploader
            .upload_file_verified(
                &identity,
                &source,
                Sha256::digest(content).into(),
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Scribe,
            )
            .await
            .expect("bounded upload");
        assert_eq!(uploader.max_backend_payload_bytes(), chunk.len());
        let candidate = 11_usize;
        assert_eq!(
            crate::scribe::memory::parquet_candidate_incremental_bytes(candidate)
                .expect("workspace projection"),
            candidate * 2
                + crate::scribe::memory::PARQUET_FOOTER_CHILD_BYTES
                + crate::scribe::memory::PARQUET_TRANSFER_BUFFER_BYTES * 2
        );
    }
}
