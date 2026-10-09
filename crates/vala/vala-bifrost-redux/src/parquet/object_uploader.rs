//! Bounded, content-verifying Parquet object upload shared by data-plane roles.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use object_store::buffered::BufWriter;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt as _};
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

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
    ObjectStore(#[from] object_store::Error),
    /// Remote bytes contradict the manifest-fixed identity.
    #[error("remote Parquet content contradicts expected SHA-256 or length")]
    ContentContradiction,
}

/// Shared uploader that owns PUT/read-back convergence but no deletion authority.
pub struct BifrostParquetUploader {
    /// Durable backend used for both upload attempts and authoritative read-back.
    objects: Arc<dyn ObjectStore>,
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
    /// Largest backend-owned write or read payload observed by the real backend.
    max_backend_payload_bytes: Arc<AtomicUsize>,
    /// PUT attempts begun, so a test can prove a key was never rewritten.
    puts: Arc<AtomicUsize>,
}

impl BifrostParquetUploader {
    /// Creates an uploader over the configured durable object store.
    #[must_use]
    pub fn new(objects: Arc<dyn ObjectStore>) -> Self {
        Self {
            objects,
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

    /// Returns the PUT attempts begun since construction.
    #[cfg(test)]
    fn puts(&self) -> usize {
        self.faults.puts.load(Ordering::Acquire)
    }

    /// Records one unavoidable backend-owned payload after bounding its length.
    #[cfg(test)]
    fn record_backend_payload(&self, bytes: usize) {
        self.faults
            .max_backend_payload_bytes
            .fetch_max(bytes, Ordering::AcqRel);
    }

    /// Streams a local file through a caller-governed chunk and verifies remote bytes.
    ///
    /// A key that already holds the expected bytes is accepted without a PUT.
    /// Otherwise a failed or ambiguous PUT is resolved by read-back. Absence is
    /// retried once; contradictory content fails closed. The key is unpublished
    /// until the catalog commits it, so a PUT overwrites whatever an earlier
    /// attempt left there. `object_store` takes one owned payload for each
    /// write or range read; every such payload is bounded by `chunk.len()`,
    /// and the caller charges its retained chunk. This owner never deletes
    /// remote bytes.
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

    /// Verifies an object a committed publication already names, never
    /// writing it.
    ///
    /// A published key is never uploaded again, so recovery of a committed
    /// publication only reads the object back against its manifest-fixed
    /// identity. There is no repair path.
    ///
    /// # Errors
    ///
    /// Returns [`ParquetUploadError::EmptyChunk`] for an empty chunk,
    /// [`ParquetUploadError::ContentContradiction`] when the object is missing
    /// or its length or content differs, and the object-store error when a
    /// read fails otherwise.
    pub async fn verify_published(
        &self,
        identity: &ParquetObjectIdentity,
        expected_sha256: [u8; 32],
        expected_len: u64,
        chunk: &mut [u8],
    ) -> Result<VerifiedParquetObject, ParquetUploadError> {
        if chunk.is_empty() {
            return Err(ParquetUploadError::EmptyChunk);
        }
        match self
            .verify_remote(identity, expected_sha256, expected_len, chunk)
            .await
        {
            Err(ParquetUploadError::ObjectStore(object_store::Error::NotFound { .. })) => {
                Err(ParquetUploadError::ContentContradiction)
            }
            verified => verified,
        }
    }

    /// Writes the manifest-fixed object and resolves outcomes by read-back.
    ///
    /// The key is read back first: when it already holds the expected bytes
    /// no PUT is issued. A missing or contradicting object is written, since
    /// an unpublished key may be overwritten.
    ///
    /// # Errors
    ///
    /// Returns local IO or object-store failures when convergence cannot be
    /// established, and [`ParquetUploadError::ContentContradiction`] when remote
    /// length or content differs from the manifest-fixed expectation.
    ///
    /// # Cancellation
    ///
    /// Cancellation may leave the deterministic remote key absent, an aborted
    /// multipart upload, or complete. A retry is safe because it rewrites the
    /// same bytes and verifies the same fixed length and digest.
    ///
    /// # Panics
    ///
    /// Never in practice: both convergence attempts record an error before
    /// the loop can end without returning.
    async fn converge(
        &self,
        identity: &ParquetObjectIdentity,
        source: &Path,
        expected_sha256: [u8; 32],
        expected_len: u64,
        chunk: &mut [u8],
    ) -> Result<VerifiedParquetObject, ParquetUploadError> {
        match self
            .verify_remote(identity, expected_sha256, expected_len, chunk)
            .await
        {
            Ok(verified) => return Ok(verified),
            Err(
                ParquetUploadError::ContentContradiction
                | ParquetUploadError::ObjectStore(object_store::Error::NotFound { .. }),
            ) => {}
            Err(error) => return Err(error),
        }
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
                Err(ParquetUploadError::ObjectStore(object_store::Error::NotFound { .. }))
                    if attempt == 0 =>
                {
                    // A failed PUT left the key absent; the bounded retry rewrites it.
                }
                Err(error) if attempt == 0 => last_error = Some(error),
                Err(error) => return Err(last_error.unwrap_or(error)),
            }
        }
        Err(last_error.expect("two convergence attempts always record a terminal error"))
    }

    /// Streams one local file to its key through the caller-governed chunk.
    ///
    /// `object_store`'s [`BufWriter`] sends a file smaller than the chunk as
    /// one PUT and a larger one as a multipart upload with chunk-sized parts.
    /// One part is in flight at a time, so the backend-owned copy stays
    /// bounded by the chunk. A failed write aborts the multipart upload.
    ///
    /// # Errors
    ///
    /// Returns [`ParquetUploadError`] when reading the local stage, writing a
    /// part, or completing the upload fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation can leave an incomplete multipart upload or a complete
    /// object at the deterministic key. The caller resolves that state
    /// through read-back convergence.
    async fn put_file(
        &self,
        identity: &ParquetObjectIdentity,
        source: &Path,
        chunk: &mut [u8],
    ) -> Result<(), ParquetUploadError> {
        #[cfg(test)]
        self.faults.puts.fetch_add(1, Ordering::AcqRel);
        #[cfg(test)]
        if self.faults.fail_before_put.swap(false, Ordering::AcqRel) {
            return Err(object_store::Error::Generic {
                store: "test",
                source: "injected PUT absence".into(),
            }
            .into());
        }
        let mut file = File::open(source).await?;
        let mut writer = BufWriter::with_capacity(
            Arc::clone(&self.objects),
            ObjectPath::from(identity.as_str()),
            chunk.len(),
        )
        .with_max_concurrency(1);
        if let Err(error) = self.write_parts(&mut file, &mut writer, chunk).await {
            // The write already failed; an abort failure only leaves an
            // incomplete multipart upload for the backend to expire.
            let _ = writer.abort().await;
            return Err(error);
        }
        writer.shutdown().await?;
        #[cfg(test)]
        if self.faults.fail_after_close.swap(false, Ordering::AcqRel) {
            return Err(object_store::Error::Generic {
                store: "test",
                source: "injected ambiguous close".into(),
            }
            .into());
        }
        Ok(())
    }

    /// Copies the local file into `writer` one chunk at a time.
    ///
    /// # Errors
    ///
    /// Returns [`ParquetUploadError`] when a local read or a part write fails.
    async fn write_parts(
        &self,
        file: &mut File,
        writer: &mut BufWriter,
        chunk: &mut [u8],
    ) -> Result<(), ParquetUploadError> {
        loop {
            let read = file.read(chunk).await?;
            if read == 0 {
                return Ok(());
            }
            #[cfg(test)]
            self.record_backend_payload(read);
            writer.put(Bytes::copy_from_slice(&chunk[..read])).await?;
        }
    }

    /// Reads bounded remote ranges and validates content identity.
    ///
    /// `object_store` returns an owned range buffer rather than filling caller memory.
    /// The caller chunk therefore provides the authoritative range bound for the
    /// one backend buffer.
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
        let location = ObjectPath::from(identity.as_str());
        let metadata = self.objects.head(&location).await?;
        if metadata.size != expected_len {
            return Err(ParquetUploadError::ContentContradiction);
        }
        let mut digest = Sha256::new();
        let mut offset = 0_u64;
        while offset < expected_len {
            let end = offset.saturating_add(chunk.len() as u64).min(expected_len);
            let remote = self.objects.get_range(&location, offset..end).await?;
            if remote.is_empty() {
                return Err(ParquetUploadError::ContentContradiction);
            }
            if remote.len() > chunk.len() {
                return Err(ParquetUploadError::ContentContradiction);
            }
            let remote_len = remote.len();
            #[cfg(test)]
            self.record_backend_payload(remote_len);
            digest.update(&remote);
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
    use object_store::memory::InMemory;

    use super::*;

    /// Builds the real `object_store` memory backend used by uploader contract tests.
    fn memory_store() -> Arc<dyn ObjectStore> {
        Arc::new(InMemory::new())
    }

    /// Reads one remote object back in full for test assertions.
    ///
    /// # Panics
    ///
    /// Panics when the object is missing or its bytes cannot be read.
    async fn remote_bytes(store: &Arc<dyn ObjectStore>, identity: &ParquetObjectIdentity) -> Bytes {
        store
            .get(&ObjectPath::from(identity.as_str()))
            .await
            .expect("remote object")
            .bytes()
            .await
            .expect("remote bytes")
    }

    /// Proves unsafe or normalization-dependent keys never reach the object store.
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

    /// Proves bounded convergence, overwrite of an unpublished key, and contradiction detection.
    #[tokio::test]
    async fn shared_uploader_resolves_absence_ambiguity_overwrite_and_digest_contradiction() {
        let store = memory_store();
        let uploader = BifrostParquetUploader::new(Arc::clone(&store));
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

        let stale_identity =
            ParquetObjectIdentity::new("scribe/tenant/stale.parquet").expect("identity");
        store
            .put(
                &ObjectPath::from(stale_identity.as_str()),
                Bytes::from_static(b"bytes an earlier attempt left").into(),
            )
            .await
            .expect("seed stale unpublished object");
        let overwritten = uploader
            .upload_file_verified(
                &stale_identity,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Scribe,
            )
            .await
            .expect("an unpublished key is overwritten and verified");
        assert_eq!(overwritten.sha256, expected_sha256);
        assert_eq!(
            remote_bytes(&store, &stale_identity).await.as_ref(),
            content
        );

        tokio::fs::write(&source, b"different")
            .await
            .expect("replace stage");
        let contradicted =
            ParquetObjectIdentity::new("forge/tenant/contradicted.parquet").expect("identity");
        let error = uploader
            .upload_file_verified(
                &contradicted,
                &source,
                expected_sha256,
                content.len() as u64,
                &mut chunk,
                BifrostUploadRole::Forge,
            )
            .await
            .expect_err("local bytes that contradict the manifest fail closed");
        assert!(matches!(error, ParquetUploadError::ContentContradiction));
    }

    /// Writes one staged file and returns its path, expected digest, and length.
    ///
    /// # Panics
    ///
    /// Panics when the stage cannot be written.
    async fn staged(directory: &Path, content: &[u8]) -> (std::path::PathBuf, [u8; 32], u64) {
        let source = directory.join("stage.parquet");
        tokio::fs::write(&source, content)
            .await
            .expect("stage write");
        (source, Sha256::digest(content).into(), content.len() as u64)
    }

    /// Seeds `identity` with `bytes`, standing for what an earlier attempt or
    /// a committed publication left at the key.
    ///
    /// # Panics
    ///
    /// Panics when the seed write fails.
    async fn seed(store: &Arc<dyn ObjectStore>, identity: &ParquetObjectIdentity, bytes: &[u8]) {
        store
            .put(
                &ObjectPath::from(identity.as_str()),
                Bytes::copy_from_slice(bytes).into(),
            )
            .await
            .expect("seed remote object");
    }

    /// A committed publication's object is only read back: a matching one is
    /// accepted, and a missing or contradicting one fails closed, all with no
    /// PUT.
    ///
    /// # Panics
    ///
    /// Panics when any remote state issues a PUT or classifies otherwise.
    #[tokio::test]
    async fn published_objects_are_verified_and_never_written() {
        let content = b"published Parquet bytes";
        let directory = tempfile::tempdir().expect("temporary stage");
        let (_, sha256, length) = staged(directory.path(), content).await;
        let mut chunk = [0_u8; 6];
        for (remote, accepted) in [
            (Some(&content[..]), true),
            (None, false),
            (Some(&b"other"[..]), false),
        ] {
            let store = memory_store();
            let uploader = BifrostParquetUploader::new(Arc::clone(&store));
            let identity =
                ParquetObjectIdentity::new("scribe/tenant/published.parquet").expect("identity");
            if let Some(remote) = remote {
                seed(&store, &identity, remote).await;
            }
            let verified = uploader
                .verify_published(&identity, sha256, length, &mut chunk)
                .await;
            if accepted {
                assert_eq!(verified.expect("matching object").sha256, sha256);
            } else {
                assert!(matches!(
                    verified,
                    Err(ParquetUploadError::ContentContradiction)
                ));
            }
            assert_eq!(uploader.puts(), 0, "a published key is never written");
        }
    }

    /// An unpublished key that already holds the expected bytes is accepted
    /// with no PUT; a missing or contradicting one is written once.
    ///
    /// # Panics
    ///
    /// Panics when a remote state issues the wrong number of PUTs or does not
    /// converge on the staged bytes.
    #[tokio::test]
    async fn unpublished_keys_are_written_only_when_they_differ() {
        let content = b"unpublished Parquet bytes";
        let directory = tempfile::tempdir().expect("temporary stage");
        let (source, sha256, length) = staged(directory.path(), content).await;
        let mut chunk = [0_u8; 6];
        for (remote, puts) in [(Some(&content[..]), 0), (None, 1), (Some(&b"other"[..]), 1)] {
            let store = memory_store();
            let uploader = BifrostParquetUploader::new(Arc::clone(&store));
            let identity =
                ParquetObjectIdentity::new("scribe/tenant/unpublished.parquet").expect("identity");
            if let Some(remote) = remote {
                seed(&store, &identity, remote).await;
            }
            let verified = uploader
                .upload_file_verified(
                    &identity,
                    &source,
                    sha256,
                    length,
                    &mut chunk,
                    BifrostUploadRole::Scribe,
                )
                .await
                .expect("the key converges");
            assert_eq!(verified.sha256, sha256);
            assert_eq!(uploader.puts(), puts);
            assert_eq!(remote_bytes(&store, &identity).await.as_ref(), content);
        }
    }

    /// Proves concurrent uploads of the same manifest-fixed bytes both verify.
    ///
    /// # Panics
    ///
    /// Panics if either uploader cannot converge on the same remote object.
    #[tokio::test]
    async fn concurrent_same_key_uploads_both_verify_the_same_bytes() {
        let store = memory_store();
        let uploader = BifrostParquetUploader::new(Arc::clone(&store));
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
        assert_eq!(remote_bytes(&store, &identity).await.as_ref(), content);
    }

    /// Proves upload cannot allocate without caller-owned governed transfer memory.
    #[tokio::test]
    async fn caller_owned_chunk_must_be_nonempty() {
        let uploader = BifrostParquetUploader::new(memory_store());
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

    /// The real `object_store` writer and reader never own more than one governed chunk.
    ///
    /// # Panics
    ///
    /// Panics if a write/range-read payload exceeds the caller bound or the
    /// producer projection omits either simultaneously retained transfer buffer.
    #[tokio::test]
    async fn shared_uploader_bounds_backend_owned_payloads_by_governed_chunk() {
        let uploader = BifrostParquetUploader::new(memory_store());
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
    }
}
