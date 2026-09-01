//! The process-bound Iceberg storage adapter over this node's storage owner.
//!
//! Iceberg reaches object storage through its own [`Storage`] trait. Redux
//! satisfies that trait by delegating to the node's one [`BifrostStorage`]
//! owner rather than by resolving a second client from catalog properties, so
//! Iceberg metadata I/O, hot-footer reads, and Scribe publication all run
//! against the same configured backend under the same governance.
//!
//! The adapter is deliberately *process-bound*. Iceberg's trait objects are
//! `typetag`-serializable so a plan can carry its storage across a process
//! boundary; a live client handle cannot travel that way, and silently
//! reconstructing one on the far side would give a follower a backend identity
//! its leader never authorized. Both serialization directions therefore fail
//! with a stable local error.

use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt as _;
use futures_util::stream::{self, BoxStream};
use iceberg::io::{
    FileMetadata, FileRead, FileWrite, InputFile, ListEntry, OutputFile, Storage, StorageConfig,
    StorageFactory,
};
use iceberg::{Error as IcebergError, ErrorKind as IcebergErrorKind, Result as IcebergResult};
use opendal::EntryMode;
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::storage::{BifrostStorage, BifrostStorageError};

/// The one message every refused serialization of this adapter reports.
///
/// Stable and local by construction: a caller that sees it knows the plan it
/// tried to move carries a live process resource, not that a remote peer was
/// unreachable.
const NOT_PORTABLE: &str =
    "the Bifrost Iceberg storage adapter is bound to this process and cannot be serialized";

/// Maps one governed owner failure onto Iceberg's error type.
///
/// This is the adapter's only error responsibility: classification already
/// happened in the owner, which is the one place that sees the backend. A
/// missing object is preserved as `DataInvalid` because that is the only kind
/// Iceberg's own storage implementations use for it; every other closed owner
/// failure — admission, cancellation, the elapsed retry bound, a closed owner,
/// or a backend class — is `Unexpected`, which Iceberg treats as retryable at
/// its own layer.
fn owner_error(operation: &str, location: &str, error: &BifrostStorageError) -> IcebergError {
    let kind = if matches!(error, BifrostStorageError::NotFound { .. }) {
        IcebergErrorKind::DataInvalid
    } else {
        IcebergErrorKind::Unexpected
    };
    IcebergError::new(
        kind,
        format!("Bifrost storage failed to {operation} {location}: {error}"),
    )
}

/// Iceberg storage backed by this node's one Bifrost storage owner.
///
/// Cloning is cheap and shares the same owner and warehouse: Iceberg's
/// `new_input`/`new_output` need an owned `Arc<dyn Storage>`, and every clone
/// must remain the same backend identity.
#[derive(Debug, Clone)]
pub struct BifrostIcebergStorage {
    /// The node's storage owner; the sole route to the backend.
    storage: Arc<BifrostStorage>,
    /// Active warehouse URI. Every accepted location lives beneath it.
    warehouse: Arc<str>,
}

impl BifrostIcebergStorage {
    /// Binds Iceberg storage to one owner and one warehouse root.
    #[must_use]
    pub fn new(storage: Arc<BifrostStorage>, warehouse: &str) -> Self {
        Self {
            storage,
            warehouse: Arc::from(warehouse.trim_end_matches('/')),
        }
    }

    /// Converts an Iceberg location into the operator-relative key it names.
    ///
    /// This is the adapter's authority boundary. The operator is already rooted
    /// at the warehouse, so a key is only meaningful relative to it; anything
    /// outside the active warehouse names a different bucket, account, or
    /// filesystem, and there is no correct way to guess what the caller meant.
    /// Such a location is refused rather than reconstructed, because a
    /// reconstruction is exactly how a foreign path becomes a legitimate-looking
    /// read.
    ///
    /// # Errors
    /// Returns [`IcebergErrorKind::DataInvalid`] when the location carries a
    /// query or fragment, does not live under the active warehouse, is the
    /// warehouse root itself, or contains an empty or `.`/`..` path segment.
    fn relativize(&self, location: &str) -> IcebergResult<String> {
        let refuse = |reason: &str| {
            Err(IcebergError::new(
                IcebergErrorKind::DataInvalid,
                format!(
                    "Bifrost storage refuses {location}: {reason} (warehouse {})",
                    self.warehouse
                ),
            ))
        };
        if location.contains('?') || location.contains('#') {
            return refuse("a location carries a query or fragment");
        }
        let Some(relative) = location.strip_prefix(&*self.warehouse) else {
            return refuse("the location is outside the active warehouse");
        };
        let Some(relative) = relative.strip_prefix('/') else {
            return refuse("the location is the warehouse root itself");
        };
        if relative.is_empty() {
            return refuse("the location names no object");
        }
        if relative
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            return refuse("the location contains an empty or traversing path segment");
        }
        Ok(relative.to_owned())
    }

    /// Rebuilds the absolute location a relative key was listed under.
    ///
    /// Listing returns operator-relative keys, but Iceberg expects the same
    /// absolute form it passed in, so entries are re-prefixed with the exact
    /// warehouse this adapter is bound to.
    fn absolutize(warehouse: &str, relative: &str) -> String {
        format!("{warehouse}/{}", relative.trim_end_matches('/'))
    }
}

impl Serialize for BifrostIcebergStorage {
    /// Always refuses: this adapter holds a live process resource.
    ///
    /// # Errors
    /// Always returns a serializer error carrying [`NOT_PORTABLE`].
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(NOT_PORTABLE))
    }
}

impl<'de> Deserialize<'de> for BifrostIcebergStorage {
    /// Always refuses: reconstructing a backend client here would invent
    /// authority the sender never granted.
    ///
    /// # Errors
    /// Always returns a deserializer error carrying [`NOT_PORTABLE`].
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(D::Error::custom(NOT_PORTABLE))
    }
}

#[async_trait]
#[typetag::serde]
impl Storage for BifrostIcebergStorage {
    async fn exists(&self, path: &str) -> IcebergResult<bool> {
        let key = self.relativize(path)?;
        self.storage
            .exists(&key)
            .await
            .map_err(|error| owner_error("probe", path, &error))
    }

    async fn metadata(&self, path: &str) -> IcebergResult<FileMetadata> {
        let key = self.relativize(path)?;
        let size = self
            .storage
            .stat(&key)
            .await
            .map_err(|error| owner_error("stat", path, &error))?;
        Ok(FileMetadata { size })
    }

    async fn read(&self, path: &str) -> IcebergResult<Bytes> {
        let key = self.relativize(path)?;
        self.storage
            .read(&key)
            .await
            .map_err(|error| owner_error("read", path, &error))
    }

    async fn reader(&self, path: &str) -> IcebergResult<Box<dyn FileRead>> {
        let key = self.relativize(path)?;
        Ok(Box::new(BifrostFileRead {
            storage: Arc::clone(&self.storage),
            location: path.to_owned(),
            key,
        }))
    }

    async fn write(&self, path: &str, bs: Bytes) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.storage
            .write_once(&key, bs)
            .await
            .map_err(|error| owner_error("write", path, &error))
    }

    async fn writer(&self, path: &str) -> IcebergResult<Box<dyn FileWrite>> {
        let key = self.relativize(path)?;
        let writer = self
            .storage
            .open_writer_once(&key)
            .await
            .map_err(|error| owner_error("open a writer for", path, &error))?;
        Ok(Box::new(BifrostFileWrite {
            storage: Arc::clone(&self.storage),
            writer: Some(writer),
            location: path.to_owned(),
        }))
    }

    async fn delete(&self, path: &str) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.storage
            .delete_once(&key)
            .await
            .map_err(|error| owner_error("delete", path, &error))
    }

    async fn delete_prefix(&self, path: &str) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.storage
            .delete_prefix_once(&key)
            .await
            .map_err(|error| owner_error("delete the prefix", path, &error))
    }

    async fn delete_stream(&self, mut paths: BoxStream<'static, String>) -> IcebergResult<()> {
        while let Some(path) = paths.next().await {
            self.delete(&path).await?;
        }
        Ok(())
    }

    async fn list(
        &self,
        path: &str,
        recursive: bool,
    ) -> IcebergResult<BoxStream<'static, IcebergResult<ListEntry>>> {
        let key = self.relativize(path)?;
        let prefix = format!("{}/", key.trim_end_matches('/'));
        let entries = self
            .storage
            .list(&prefix, recursive)
            .await
            .map_err(|error| owner_error("list", path, &error))?;
        let warehouse = Arc::clone(&self.warehouse);
        Ok(stream::iter(entries.into_iter().map(move |entry| {
            let metadata = entry.metadata();
            Ok(ListEntry {
                path: Self::absolutize(&warehouse, entry.path()),
                size: metadata.content_length(),
                last_modified_ms: metadata
                    .last_modified()
                    .map(|modified| modified.into_inner().as_millisecond()),
                is_dir: metadata.mode() == EntryMode::DIR,
            })
        }))
        .boxed())
    }

    fn new_input(&self, path: &str) -> IcebergResult<InputFile> {
        self.relativize(path)?;
        Ok(InputFile::new(Arc::new(self.clone()), path.to_owned()))
    }

    fn new_output(&self, path: &str) -> IcebergResult<OutputFile> {
        self.relativize(path)?;
        Ok(OutputFile::new(Arc::new(self.clone()), path.to_owned()))
    }
}

/// A ranged reader over one validated warehouse object.
///
/// Holds the already-relativized key so a range read repeats no validation and
/// cannot drift onto a different object than the one `reader` admitted, and the
/// node's storage owner rather than a raw operator so a range read issued long
/// after the reader was opened is still admitted, bounded, and refused by the
/// same owner as every other Iceberg operation.
#[derive(Debug)]
struct BifrostFileRead {
    /// The node's storage owner; the sole route to the backend.
    storage: Arc<BifrostStorage>,
    /// Absolute location, retained only for error messages.
    location: String,
    /// Owner-relative key validated once when the reader was opened.
    key: String,
}

#[async_trait]
impl FileRead for BifrostFileRead {
    async fn read(&self, range: Range<u64>) -> IcebergResult<Bytes> {
        self.storage
            .read_range(&self.key, range)
            .await
            .map_err(|error| owner_error("read a range of", &self.location, &error))
    }
}

/// A write-once writer over one validated warehouse object.
///
/// Deliberately carries no retry: a partially written object is not a read that
/// can simply be attempted again, and a repeated publication is how a duplicate
/// data file reaches a snapshot.
struct BifrostFileWrite {
    /// The node's storage owner that admits every append and the close.
    storage: Arc<BifrostStorage>,
    /// The open writer, taken on close so a second close is an error.
    writer: Option<opendal::Writer>,
    /// Absolute location, retained only for error messages.
    location: String,
}

impl std::fmt::Debug for BifrostFileWrite {
    /// Names the object and whether the writer is still open.
    ///
    /// Hand-written because an `OpenDAL` writer is not `Debug`, and Iceberg's
    /// `FileWrite` requires the bound.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BifrostFileWrite")
            .field("location", &self.location)
            .field("open", &self.writer.is_some())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl FileWrite for BifrostFileWrite {
    async fn write(&mut self, bs: Bytes) -> IcebergResult<()> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            IcebergError::new(
                IcebergErrorKind::DataInvalid,
                format!(
                    "Bifrost storage cannot write to the closed {}",
                    self.location
                ),
            )
        })?;
        self.storage
            .writer_write_once(writer, bs)
            .await
            .map_err(|error| owner_error("write to", &self.location, &error))
    }

    async fn close(&mut self) -> IcebergResult<()> {
        let mut writer = self.writer.take().ok_or_else(|| {
            IcebergError::new(
                IcebergErrorKind::DataInvalid,
                format!("Bifrost storage already closed {}", self.location),
            )
        })?;
        self.storage
            .writer_close_once(&mut writer)
            .await
            .map_err(|error| owner_error("close", &self.location, &error))
    }
}

/// Builds Iceberg storage bound to this process's storage owner.
///
/// Iceberg asks the factory for storage per catalog configuration; this one
/// ignores that configuration entirely, because the backend is already decided
/// by the node's storage owner and letting catalog properties select a second
/// one is precisely the resolution behavior this adapter replaces.
#[derive(Debug, Clone)]
pub struct BifrostIcebergStorageFactory {
    /// The storage every built instance shares.
    storage: BifrostIcebergStorage,
}

impl BifrostIcebergStorageFactory {
    /// Binds a factory to one owner and one warehouse root.
    #[must_use]
    pub fn new(storage: Arc<BifrostStorage>, warehouse: &str) -> Self {
        Self {
            storage: BifrostIcebergStorage::new(storage, warehouse),
        }
    }
}

impl Serialize for BifrostIcebergStorageFactory {
    /// Always refuses: this adapter holds a live process resource.
    ///
    /// # Errors
    /// Always returns a serializer error carrying [`NOT_PORTABLE`].
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(NOT_PORTABLE))
    }
}

impl<'de> Deserialize<'de> for BifrostIcebergStorageFactory {
    /// Always refuses: reconstructing a backend client here would invent
    /// authority the sender never granted.
    ///
    /// # Errors
    /// Always returns a deserializer error carrying [`NOT_PORTABLE`].
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(D::Error::custom(NOT_PORTABLE))
    }
}

#[typetag::serde]
impl StorageFactory for BifrostIcebergStorageFactory {
    fn build(&self, _config: &StorageConfig) -> IcebergResult<Arc<dyn Storage>> {
        Ok(Arc::new(self.storage.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::storage::{BifrostStorageConfig, BifrostStoragePolicy};

    /// Builds an adapter over a real local backend rooted at `root`.
    ///
    /// A real handle rather than a stub: path authority is only meaningful
    /// against the operator the warehouse is actually rooted at, and a fake one
    /// would make every acceptance a statement about the fake.
    ///
    /// # Panics
    /// Panics when the signer or the default storage policy is invalid.
    fn adapter(root: &Path) -> (BifrostIcebergStorage, String) {
        let (storage, warehouse, _owner) = adapter_with_owner(root);
        (storage, warehouse)
    }

    /// Builds an adapter and also hands back the owner it is bound to.
    ///
    /// Lifecycle assertions need both halves: the adapter is the surface
    /// Iceberg calls and the owner is the thing that closes, and the whole
    /// point of the governed adapter is that closing the second one stops the
    /// first one.
    ///
    /// # Panics
    /// Panics when the signer or the default storage policy is invalid.
    fn adapter_with_owner(root: &Path) -> (BifrostIcebergStorage, String, Arc<BifrostStorage>) {
        let signer = wyrd_storage::signer::BackendSigner::Local(
            wyrd_storage::local::LocalSigner::new(root.to_path_buf()).expect("local signer"),
        );
        let storage = Arc::new(BifrostStorage::new(
            Arc::new(wyrd_storage::handle::StorageHandle::new(signer)),
            BifrostStoragePolicy::resolve(
                BifrostStorageConfig::default(),
                u64::from(u32::MAX),
                false,
            )
            .expect("the default storage policy is valid"),
            None,
        ));
        let warehouse = format!("file://{}", root.display());
        (
            BifrostIcebergStorage::new(Arc::clone(&storage), &warehouse),
            warehouse,
            storage,
        )
    }

    /// Closing the node's storage owner stops every Iceberg object operation.
    ///
    /// This is the whole claim of routing the adapter through the owner. If any
    /// `Storage` method still reached `OpenDAL` directly, a closed owner would
    /// be a statement about the metadata cache alone and a process could finish
    /// shutting down while Iceberg reads, writes, and deletes were still in
    /// flight against the backend. Every method is asserted, including the
    /// already-open reader and writer handles, because refusing only the
    /// entry points still leaves a live route to the backend.
    ///
    /// # Panics
    /// Panics when any operation is served after the owner is closed, or when
    /// the pre-close round trip fails.
    #[tokio::test]
    async fn iceberg_reads_refuse_after_owner_close() {
        let root = tempfile::tempdir().expect("warehouse root");
        let (storage, warehouse, owner) = adapter_with_owner(root.path());
        let live = format!("{warehouse}/datasets/tenant/live.parquet");
        storage
            .write(&live, Bytes::from_static(b"rows"))
            .await
            .expect("the open owner admits a write");
        let reader = match storage.reader(&live).await {
            Ok(reader) => reader,
            Err(error) => panic!("the open owner admits a reader: {error}"),
        };
        let mut writer = match storage
            .writer(&format!("{warehouse}/datasets/tenant/staged.parquet"))
            .await
        {
            Ok(writer) => writer,
            Err(error) => panic!("the open owner admits a writer: {error}"),
        };

        assert!(
            owner.close(std::time::Instant::now()).await,
            "the owner settles"
        );

        assert!(
            storage.exists(&live).await.is_err(),
            "a closed owner admits no existence probe"
        );
        assert!(
            storage.metadata(&live).await.is_err(),
            "a closed owner admits no stat"
        );
        storage
            .read(&live)
            .await
            .expect_err("a closed owner admits no read");
        reader
            .read(0..2)
            .await
            .expect_err("a closed owner admits no ranged read through an open reader");
        assert!(
            storage
                .list(&format!("{warehouse}/datasets"), true)
                .await
                .is_err(),
            "a closed owner admits no list"
        );
        storage
            .write(&live, Bytes::from_static(b"more"))
            .await
            .expect_err("a closed owner admits no write");
        assert!(
            storage
                .writer(&format!("{warehouse}/datasets/tenant/refused.parquet"))
                .await
                .is_err(),
            "a closed owner opens no writer"
        );
        writer
            .write(Bytes::from_static(b"more"))
            .await
            .expect_err("a closed owner admits no writer append");
        writer
            .close()
            .await
            .expect_err("a closed owner admits no writer close");
        storage
            .delete(&live)
            .await
            .expect_err("a closed owner admits no delete");
        storage
            .delete_prefix(&format!("{warehouse}/datasets"))
            .await
            .expect_err("a closed owner admits no recursive delete");

        let snapshot = owner.telemetry_snapshot();
        assert!(
            snapshot.request_terminal(vala_storage_outcome()) >= 11,
            "every refused Iceberg operation must publish a governed closed terminal"
        );
        assert_eq!(snapshot.request_starts(), snapshot.request_terminals());
        assert_eq!(snapshot.active_requests(), 0);
        assert_eq!(snapshot.anomalies(), 0);
    }

    /// Names the closed-owner request terminal the refusals above publish.
    const fn vala_storage_outcome() -> crate::storage::StorageRequestOutcome {
        crate::storage::StorageRequestOutcome::Closed
    }

    /// The adapter admits only objects inside the warehouse it is bound to, and
    /// round-trips one that is.
    ///
    /// Path authority is the adapter's entire security contribution: it holds a
    /// credentialed client for one warehouse, so any location it accepts is a
    /// location that client will actually read or write. A foreign authority,
    /// a query or fragment, and a traversing segment are all refused instead of
    /// normalized, because normalization is what turns a path the caller was
    /// never entitled to into one this client happily serves.
    ///
    /// # Panics
    /// Panics when an admitted round trip fails or a refused location is
    /// accepted.
    #[tokio::test]
    async fn the_adapter_serves_only_locations_inside_its_own_warehouse() {
        let root = tempfile::tempdir().expect("warehouse root");
        let (storage, warehouse) = adapter(root.path());

        let inside = format!("{warehouse}/datasets/tenant/a.parquet");
        storage
            .write(&inside, Bytes::from_static(b"rows"))
            .await
            .expect("a location inside the warehouse is served");
        assert_eq!(
            storage
                .read(&inside)
                .await
                .expect("the written object reads"),
            Bytes::from_static(b"rows")
        );
        assert_eq!(
            storage
                .metadata(&inside)
                .await
                .expect("the written object stats")
                .size,
            4
        );
        assert!(storage.exists(&inside).await.expect("existence probes"));
        let reader = storage.reader(&inside).await.expect("a reader opens");
        assert_eq!(
            reader.read(1..3).await.expect("a range reads"),
            Bytes::from_static(b"ow")
        );

        for refused in [
            "s3://other-bucket/datasets/a.parquet".to_owned(),
            "file:///elsewhere/a.parquet".to_owned(),
            format!("{warehouse}/../escape.parquet"),
            format!("{warehouse}/datasets/../../escape.parquet"),
            format!("{warehouse}//datasets/a.parquet"),
            format!("{warehouse}/datasets/a.parquet?versionId=2"),
            format!("{warehouse}/datasets/a.parquet#footer"),
            warehouse.clone(),
        ] {
            assert!(
                storage.read(&refused).await.is_err(),
                "{refused} must not be served"
            );
            assert!(
                storage.new_input(&refused).is_err(),
                "{refused} must not become an input file"
            );
            assert!(
                storage.new_output(&refused).is_err(),
                "{refused} must not become an output file"
            );
        }
    }

    /// Neither the adapter nor its factory can leave or re-enter this process.
    ///
    /// Iceberg's storage trait objects are `typetag`-portable, so without this
    /// refusal a distributed plan would serialize a live backend client and the
    /// far side would silently rebuild one — a backend identity the sender
    /// never authorized. Both directions are asserted for both types because
    /// only refusing one of them still leaves a way through.
    ///
    /// # Panics
    /// Panics when either direction of either type succeeds.
    #[test]
    fn the_adapter_and_its_factory_refuse_to_cross_a_process_boundary() {
        let root = tempfile::tempdir().expect("warehouse root");
        let (storage, warehouse) = adapter(root.path());
        let factory = BifrostIcebergStorageFactory {
            storage: storage.clone(),
        };

        let serialized = serde_json::to_string(&storage).expect_err("storage must not serialize");
        assert!(serialized.to_string().contains(NOT_PORTABLE));
        let serialized =
            serde_json::to_string(&factory).expect_err("the factory must not serialize");
        assert!(serialized.to_string().contains(NOT_PORTABLE));

        let deserialized = serde_json::from_str::<BifrostIcebergStorage>("{}")
            .expect_err("storage must not deserialize");
        assert!(deserialized.to_string().contains(NOT_PORTABLE));
        let deserialized = serde_json::from_str::<BifrostIcebergStorageFactory>("{}")
            .expect_err("the factory must not deserialize");
        assert!(deserialized.to_string().contains(NOT_PORTABLE));

        // The trait-object forms are the ones a plan actually carries.
        let boxed: Box<dyn Storage> = Box::new(storage);
        assert!(serde_json::to_string(&boxed).is_err());
        let boxed: Box<dyn StorageFactory> = Box::new(factory);
        assert!(serde_json::to_string(&boxed).is_err());
        assert!(warehouse.starts_with("file://"));
    }
}
