//! The process-bound Iceberg storage adapter over this node's object store.
//!
//! Iceberg reaches object storage through its own [`Storage`] trait. Redux
//! satisfies that trait by delegating to the node's one Bifrost
//! [`ObjectStore`] rather than by resolving a second client from catalog
//! properties, so Iceberg metadata I/O, Oracle scans, Scribe uploads, and Forge
//! all run against the same configured backend. iceberg-rust ships no
//! `object_store` storage, so this is the one adapter between them.
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
use futures_util::stream::{self, BoxStream};
use futures_util::{StreamExt as _, TryStreamExt as _};
use iceberg::io::{
    FileMetadata, FileRead, FileWrite, InputFile, ListEntry, OutputFile, Storage, StorageConfig,
    StorageFactory,
};
use iceberg::{Error as IcebergError, ErrorKind as IcebergErrorKind, Result as IcebergResult};
use object_store::buffered::BufWriter;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectMeta, ObjectStore, ObjectStoreExt as _};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tokio::io::AsyncWriteExt as _;

use crate::storage::BifrostStorage;

/// The one message every refused serialization of this adapter reports.
///
/// Stable and local by construction: a caller that sees it knows the plan it
/// tried to move carries a live process resource, not that a remote peer was
/// unreachable.
const NOT_PORTABLE: &str =
    "the Bifrost Iceberg storage adapter is bound to this process and cannot be serialized";

/// Maps one object-store failure onto Iceberg's error type.
///
/// A missing object is `DataInvalid`, the only kind Iceberg's own storage
/// implementations use for it; every other failure is `Unexpected`. The
/// object-store error is kept as the typed source so a vanished pinned object
/// stays classifiable as stale above Iceberg and Parquet.
fn store_error(operation: &str, location: &str, error: object_store::Error) -> IcebergError {
    let kind = if matches!(error, object_store::Error::NotFound { .. }) {
        IcebergErrorKind::DataInvalid
    } else {
        IcebergErrorKind::Unexpected
    };
    IcebergError::new(
        kind,
        format!("Bifrost storage failed to {operation} {location}"),
    )
    .with_source(error)
}

/// Iceberg storage backed by this node's one Bifrost object store.
///
/// Cloning is cheap and shares the same owner and warehouse: Iceberg's
/// `new_input`/`new_output` need an owned `Arc<dyn Storage>`, and every clone
/// must remain the same backend identity.
#[derive(Debug, Clone)]
pub struct BifrostIcebergStorage {
    /// The node's storage owner, whose store is the sole route to the backend.
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
    /// Two spellings reach this boundary and both are already warehouse-local.
    /// Iceberg metadata carries absolute locations, which must live under the
    /// active warehouse. Scribe registers a promoted object by its operator key,
    /// which names no authority at all: it has no scheme and no leading `/`, so
    /// it cannot denote a different backend, and it is passed through unchanged
    /// rather than re-prefixed. Everything else — a foreign scheme, a
    /// filesystem-absolute path, a traversing segment — is refused.
    ///
    /// # Errors
    /// Returns [`IcebergErrorKind::DataInvalid`] when the location carries a
    /// query or fragment, names an authority outside the active warehouse, is
    /// filesystem-absolute, is the warehouse root itself, or contains an empty
    /// or `.`/`..` path segment.
    pub(crate) fn relativize(&self, location: &str) -> IcebergResult<String> {
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
        let relative = match location.strip_prefix(&*self.warehouse) {
            Some(under_warehouse) => match under_warehouse.strip_prefix('/') {
                Some(relative) => relative,
                None => return refuse("the location is the warehouse root itself"),
            },
            None if location.contains("://") => {
                return refuse("the location is outside the active warehouse");
            }
            None if location.starts_with('/') => {
                return refuse("the location is outside the active warehouse");
            }
            None => location,
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

    /// Borrows the storage owner whose store every read and write of this
    /// adapter goes through, and which holds the node's Parquet footer cache.
    #[must_use]
    pub(crate) fn storage(&self) -> &Arc<BifrostStorage> {
        &self.storage
    }

    /// Returns the store and validated object path for one Iceberg location.
    ///
    /// # Errors
    /// Returns the [`Self::relativize`] refusal.
    fn object(&self, location: &str) -> IcebergResult<(&Arc<dyn ObjectStore>, ObjectPath)> {
        let key = self.relativize(location)?;
        Ok((self.storage.objects(), ObjectPath::from(key.as_str())))
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
        let (store, key) = self.object(path)?;
        match store.head(&key).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(store_error("probe", path, error)),
        }
    }

    async fn metadata(&self, path: &str) -> IcebergResult<FileMetadata> {
        let (store, key) = self.object(path)?;
        let meta = store
            .head(&key)
            .await
            .map_err(|error| store_error("stat", path, error))?;
        Ok(FileMetadata { size: meta.size })
    }

    async fn read(&self, path: &str) -> IcebergResult<Bytes> {
        let (store, key) = self.object(path)?;
        store
            .get(&key)
            .await
            .map_err(|error| store_error("read", path, error))?
            .bytes()
            .await
            .map_err(|error| store_error("read", path, error))
    }

    async fn reader(&self, path: &str) -> IcebergResult<Box<dyn FileRead>> {
        let (store, key) = self.object(path)?;
        Ok(Box::new(BifrostFileRead {
            store: Arc::clone(store),
            location: path.to_owned(),
            key,
        }))
    }

    async fn write(&self, path: &str, bs: Bytes) -> IcebergResult<()> {
        let (store, key) = self.object(path)?;
        store
            .put(&key, bs.into())
            .await
            .map(|_| ())
            .map_err(|error| store_error("write", path, error))
    }

    async fn writer(&self, path: &str) -> IcebergResult<Box<dyn FileWrite>> {
        let (store, key) = self.object(path)?;
        Ok(Box::new(BifrostFileWrite {
            writer: Some(BufWriter::new(Arc::clone(store), key)),
            location: path.to_owned(),
        }))
    }

    async fn delete(&self, path: &str) -> IcebergResult<()> {
        let (store, key) = self.object(path)?;
        store
            .delete(&key)
            .await
            .map_err(|error| store_error("delete", path, error))
    }

    async fn delete_prefix(&self, path: &str) -> IcebergResult<()> {
        let (store, key) = self.object(path)?;
        let locations = store.list(Some(&key)).map_ok(|meta| meta.location).boxed();
        store
            .delete_stream(locations)
            .try_for_each(|_| std::future::ready(Ok(())))
            .await
            .map_err(|error| store_error("delete the prefix", path, error))
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
        let (store, key) = self.object(path)?;
        let warehouse = Arc::clone(&self.warehouse);
        let entry = move |meta: &ObjectMeta, is_dir: bool| ListEntry {
            path: Self::absolutize(&warehouse, meta.location.as_ref()),
            size: meta.size,
            last_modified_ms: Some(meta.last_modified.timestamp_millis()),
            is_dir,
        };
        if recursive {
            let location = path.to_owned();
            return Ok(store
                .list(Some(&key))
                .map(move |meta| match meta {
                    Ok(meta) => Ok(entry(&meta, false)),
                    Err(error) => Err(store_error("list", &location, error)),
                })
                .boxed());
        }
        let listed = store
            .list_with_delimiter(Some(&key))
            .await
            .map_err(|error| store_error("list", path, error))?;
        let warehouse = Arc::clone(&self.warehouse);
        let directories = listed
            .common_prefixes
            .into_iter()
            .map(move |prefix| ListEntry {
                path: Self::absolutize(&warehouse, prefix.as_ref()),
                size: 0,
                last_modified_ms: None,
                is_dir: true,
            });
        let files = listed
            .objects
            .iter()
            .map(|meta| entry(meta, false))
            .collect::<Vec<_>>();
        Ok(stream::iter(files.into_iter().chain(directories).map(Ok)).boxed())
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
/// Holds the already-validated path so a range read repeats no validation and
/// cannot drift onto a different object than the one `reader` admitted.
#[derive(Debug)]
struct BifrostFileRead {
    /// The node's object store.
    store: Arc<dyn ObjectStore>,
    /// Absolute location, retained only for error messages.
    location: String,
    /// Store path validated once when the reader was opened.
    key: ObjectPath,
}

#[async_trait]
impl FileRead for BifrostFileRead {
    async fn read(&self, range: Range<u64>) -> IcebergResult<Bytes> {
        self.store
            .get_range(&self.key, range)
            .await
            .map_err(|error| store_error("read a range of", &self.location, error))
    }
}

/// A buffered writer over one validated warehouse object.
///
/// `object_store`'s [`BufWriter`] issues one `put` for a small object and a
/// multipart upload for a large one; the object exists only once `close`
/// completes.
struct BifrostFileWrite {
    /// The open writer, taken on close so a second close is an error.
    writer: Option<BufWriter>,
    /// Absolute location, retained only for error messages.
    location: String,
}

impl std::fmt::Debug for BifrostFileWrite {
    /// Names the object and whether the writer is still open.
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
        writer
            .put(bs)
            .await
            .map_err(|error| store_error("write to", &self.location, error))
    }

    async fn close(&mut self) -> IcebergResult<()> {
        let mut writer = self.writer.take().ok_or_else(|| {
            IcebergError::new(
                IcebergErrorKind::DataInvalid,
                format!("Bifrost storage already closed {}", self.location),
            )
        })?;
        writer.shutdown().await.map_err(|error| {
            IcebergError::new(
                IcebergErrorKind::Unexpected,
                format!("Bifrost storage failed to close {}", self.location),
            )
            .with_source(error)
        })
    }
}

/// Builds Iceberg storage bound to this process's object store.
///
/// Iceberg asks the factory for storage per catalog configuration; this one
/// ignores that configuration entirely, because the backend is already decided
/// by the node's object store and letting catalog properties select a second
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

    /// Builds an adapter over a real local store rooted at `root`.
    ///
    /// A real store rather than a stub: path authority is only meaningful
    /// against the store the warehouse is actually rooted at, and a fake one
    /// would make every acceptance a statement about the fake.
    ///
    /// # Panics
    /// Panics when the fixture store cannot be built.
    fn adapter(root: &Path) -> (BifrostIcebergStorage, String) {
        let warehouse = format!("file://{}", root.display());
        (
            BifrostIcebergStorage::new(BifrostStorage::for_test(root, false), &warehouse),
            warehouse,
        )
    }

    /// A streamed write becomes one object, listing reports it under its
    /// absolute location (and its directory when not recursive), and a prefix
    /// delete removes everything beneath it.
    ///
    /// # Panics
    /// Panics when the written bytes, a listing, or the prefix delete differ.
    #[tokio::test]
    async fn written_objects_list_and_delete_by_prefix() {
        let root = tempfile::tempdir().expect("warehouse root");
        let (storage, warehouse) = adapter(root.path());
        let streamed = format!("{warehouse}/datasets/tenant/run/a.parquet");
        let mut writer = storage.writer(&streamed).await.expect("a writer opens");
        writer
            .write(Bytes::from_static(b"ro"))
            .await
            .expect("first chunk");
        writer
            .write(Bytes::from_static(b"ws"))
            .await
            .expect("second chunk");
        writer.close().await.expect("the writer closes");
        writer.close().await.expect_err("a second close is refused");
        assert_eq!(
            storage.read(&streamed).await.expect("reads back"),
            Bytes::from_static(b"rows")
        );
        let sibling = format!("{warehouse}/datasets/tenant/b.parquet");
        storage
            .write(&sibling, Bytes::from_static(b"x"))
            .await
            .expect("a sibling writes");

        let prefix = format!("{warehouse}/datasets/tenant");
        let mut recursive = Storage::list(&storage, &prefix, true)
            .await
            .expect("lists")
            .map(|entry| entry.expect("entry").path)
            .collect::<Vec<_>>()
            .await;
        recursive.sort();
        assert_eq!(recursive, vec![sibling.clone(), streamed.clone()]);
        let shallow = Storage::list(&storage, &prefix, false)
            .await
            .expect("lists")
            .map(|entry| entry.expect("entry"))
            .collect::<Vec<_>>()
            .await;
        assert!(
            shallow
                .iter()
                .any(|entry| entry.path == sibling && !entry.is_dir)
        );
        assert!(
            shallow
                .iter()
                .any(|entry| entry.path == format!("{prefix}/run") && entry.is_dir)
        );

        storage.delete_prefix(&prefix).await.expect("prefix delete");
        assert!(!storage.exists(&streamed).await.expect("probes"));
        assert!(!storage.exists(&sibling).await.expect("probes"));
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
