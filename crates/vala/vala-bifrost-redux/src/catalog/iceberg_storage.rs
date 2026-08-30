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
use opendal::{EntryMode, Operator};
use serde::de::Error as _;
use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::storage::BifrostStorage;

/// The one message every refused serialization of this adapter reports.
///
/// Stable and local by construction: a caller that sees it knows the plan it
/// tried to move carries a live process resource, not that a remote peer was
/// unreachable.
const NOT_PORTABLE: &str =
    "the Bifrost Iceberg storage adapter is bound to this process and cannot be serialized";

/// Maps one backend failure onto Iceberg's error type.
///
/// `NotFound` is preserved as `DataInvalid` because that is the only kind
/// Iceberg's own storage implementations use for a missing object; everything
/// else is `Unexpected`, which Iceberg treats as retryable at its own layer.
fn backend_error(operation: &str, location: &str, error: &opendal::Error) -> IcebergError {
    let kind = if error.kind() == opendal::ErrorKind::NotFound {
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

    /// Returns the operator this adapter delegates every operation to.
    fn operator(&self) -> &Operator {
        self.storage.operator()
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
        self.operator()
            .exists(&key)
            .await
            .map_err(|error| backend_error("probe", path, &error))
    }

    async fn metadata(&self, path: &str) -> IcebergResult<FileMetadata> {
        let key = self.relativize(path)?;
        let stat = self
            .operator()
            .stat(&key)
            .await
            .map_err(|error| backend_error("stat", path, &error))?;
        Ok(FileMetadata {
            size: stat.content_length(),
        })
    }

    async fn read(&self, path: &str) -> IcebergResult<Bytes> {
        let key = self.relativize(path)?;
        self.operator()
            .read(&key)
            .await
            .map(|buffer| buffer.to_bytes())
            .map_err(|error| backend_error("read", path, &error))
    }

    async fn reader(&self, path: &str) -> IcebergResult<Box<dyn FileRead>> {
        let key = self.relativize(path)?;
        Ok(Box::new(BifrostFileRead {
            operator: self.operator().clone(),
            location: path.to_owned(),
            key,
        }))
    }

    async fn write(&self, path: &str, bs: Bytes) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.operator()
            .write(&key, bs)
            .await
            .map(|_| ())
            .map_err(|error| backend_error("write", path, &error))
    }

    async fn writer(&self, path: &str) -> IcebergResult<Box<dyn FileWrite>> {
        let key = self.relativize(path)?;
        let writer = self
            .operator()
            .writer(&key)
            .await
            .map_err(|error| backend_error("open a writer for", path, &error))?;
        Ok(Box::new(BifrostFileWrite {
            writer: Some(writer),
            location: path.to_owned(),
        }))
    }

    async fn delete(&self, path: &str) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.operator()
            .delete(&key)
            .await
            .map_err(|error| backend_error("delete", path, &error))
    }

    async fn delete_prefix(&self, path: &str) -> IcebergResult<()> {
        let key = self.relativize(path)?;
        self.operator()
            .delete_with(&key)
            .recursive(true)
            .await
            .map_err(|error| backend_error("delete the prefix", path, &error))
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
            .operator()
            .list_with(&prefix)
            .recursive(recursive)
            .await
            .map_err(|error| backend_error("list", path, &error))?;
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
/// cannot drift onto a different object than the one `reader` admitted.
#[derive(Debug)]
struct BifrostFileRead {
    /// The node's operator, cloned because Iceberg owns this reader.
    operator: Operator,
    /// Absolute location, retained only for error messages.
    location: String,
    /// Operator-relative key validated once when the reader was opened.
    key: String,
}

#[async_trait]
impl FileRead for BifrostFileRead {
    async fn read(&self, range: Range<u64>) -> IcebergResult<Bytes> {
        self.operator
            .read_with(&self.key)
            .range(range)
            .await
            .map(|buffer| buffer.to_bytes())
            .map_err(|error| backend_error("read a range of", &self.location, &error))
    }
}

/// A write-once writer over one validated warehouse object.
///
/// Deliberately carries no retry: a partially written object is not a read that
/// can simply be attempted again, and a repeated publication is how a duplicate
/// data file reaches a snapshot.
struct BifrostFileWrite {
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
            .finish()
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
            .write(bs)
            .await
            .map_err(|error| backend_error("write to", &self.location, &error))
    }

    async fn close(&mut self) -> IcebergResult<()> {
        let mut writer = self.writer.take().ok_or_else(|| {
            IcebergError::new(
                IcebergErrorKind::DataInvalid,
                format!("Bifrost storage already closed {}", self.location),
            )
        })?;
        writer
            .close()
            .await
            .map(|_| ())
            .map_err(|error| backend_error("close", &self.location, &error))
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
