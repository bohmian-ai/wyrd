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
use std::task::Poll;

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

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures_util::Stream as _;

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::storage::{BifrostStorageConfig, BifrostStoragePolicy};

    /// Builds one list entry the gated wrapper is asked to expose.
    fn entry(path: &str) -> ListEntry {
        ListEntry {
            path: path.to_owned(),
            size: 1,
            last_modified_ms: None,
            is_dir: false,
        }
    }

    /// Counts every poll a wrapped backend listing actually receives.
    ///
    /// The count is the whole point: a wrapper that collects eagerly, or that
    /// keeps polling after a permit refusal, is indistinguishable from a
    /// correct one by its yielded items alone.
    fn counting_backend(
        entries: Vec<ListEntry>,
        polls: Arc<AtomicUsize>,
    ) -> BoxStream<'static, IcebergResult<ListEntry>> {
        let mut remaining = entries.into_iter();
        stream::poll_fn(move |_| {
            polls.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::Ready(remaining.next().map(Ok))
        })
        .boxed()
    }

    /// Proves a lazy listing gates every backend poll and every exposed item.
    ///
    /// Three fences are exercised because they refuse at different boundaries:
    /// a fence taken before the first poll must stop the backend from being
    /// polled at all, a fence taken while an item is already in hand must stop
    /// that item from reaching the caller, and a fence landing while the
    /// backend is parked on `Pending` must stop the resumed poll that a
    /// retained `next()` future would otherwise perform ungated. All three
    /// must terminate the wrapper after exactly one error rather than resuming
    /// on the next poll.
    #[tokio::test]
    async fn gated_list_stops_polling_and_yielding_after_fence() {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_mins(1);
        live_permit_exposes_every_entry(deadline).await;
        fence_before_the_first_poll_reaches_no_backend(deadline).await;
        fence_with_an_item_in_hand_refuses_that_item(deadline).await;
        fence_while_the_backend_is_parked_refuses_the_resumed_poll(deadline);
    }

    /// A live permit exposes every entry and polls the backend once per item
    /// plus the final exhaustion poll.
    ///
    /// # Panics
    ///
    /// Panics when an entry is refused or the backend poll count differs.
    async fn live_permit_exposes_every_entry(deadline: tokio::time::Instant) {
        let polls = Arc::new(AtomicUsize::new(0));
        let live = crate::oracle::reader_pins::ReaderIoPermit::new(
            CancellationToken::new(),
            CancellationToken::new(),
            deadline,
        );
        let mut stream = gate_list_stream(
            counting_backend(vec![entry("a"), entry("b")], Arc::clone(&polls)),
            live,
        );
        let mut exposed = Vec::new();
        while let Some(item) = stream.next().await {
            exposed.push(item.expect("a live permit exposes every entry").path);
        }
        assert_eq!(exposed, vec!["a".to_owned(), "b".to_owned()]);
        assert_eq!(polls.load(Ordering::SeqCst), 3);
    }

    /// A fence taken after the stream exists but before it is polled must
    /// reach the backend zero times.
    ///
    /// # Panics
    ///
    /// Panics when the listing is not refused, when the wrapper resumes, or
    /// when the backend is polled at all.
    async fn fence_before_the_first_poll_reaches_no_backend(deadline: tokio::time::Instant) {
        let polls = Arc::new(AtomicUsize::new(0));
        let epoch = CancellationToken::new();
        let mut stream = gate_list_stream(
            counting_backend(vec![entry("a"), entry("b")], Arc::clone(&polls)),
            crate::oracle::reader_pins::ReaderIoPermit::new(
                epoch.clone(),
                CancellationToken::new(),
                deadline,
            ),
        );
        epoch.cancel();
        assert!(
            stream
                .next()
                .await
                .expect("one permit error is yielded")
                .is_err(),
            "a fenced epoch refuses the listing rather than returning entries"
        );
        assert!(
            stream.next().await.is_none(),
            "the wrapper terminates after its one permit error"
        );
        assert_eq!(
            polls.load(Ordering::SeqCst),
            0,
            "no backend poll happens after the fence"
        );
    }

    /// A fence that lands while a backend item is already in hand must stop
    /// that item from reaching the caller, and must not poll again after.
    ///
    /// # Panics
    ///
    /// Panics when the entry is exposed, when the wrapper resumes, or when the
    /// backend is polled a second time.
    async fn fence_with_an_item_in_hand_refuses_that_item(deadline: tokio::time::Instant) {
        let polls = Arc::new(AtomicUsize::new(0));
        let epoch = CancellationToken::new();
        let fencing = epoch.clone();
        let counted = Arc::clone(&polls);
        let backend = stream::poll_fn(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            fencing.cancel();
            std::task::Poll::Ready(Some(Ok(entry("a"))))
        })
        .boxed();
        let mut stream = gate_list_stream(
            backend,
            crate::oracle::reader_pins::ReaderIoPermit::new(
                epoch,
                CancellationToken::new(),
                deadline,
            ),
        );
        assert!(
            stream
                .next()
                .await
                .expect("one permit error is yielded")
                .is_err(),
            "an entry read legally but fenced before exposure is refused, not returned"
        );
        assert!(stream.next().await.is_none());
        assert_eq!(
            polls.load(Ordering::SeqCst),
            1,
            "the fenced wrapper never polls the backend again"
        );
    }

    /// A backend parked on `Pending` must not be resumed after a fence.
    ///
    /// Polls are driven by hand because the property is about which poll
    /// reaches the backend, and an executor would hide that ordering behind
    /// wakeups. This is also why the case is synchronous.
    ///
    /// # Panics
    ///
    /// Panics when the resumed poll reaches the backend, when the fence is not
    /// reported as one permit error, or when the wrapper does not terminate.
    fn fence_while_the_backend_is_parked_refuses_the_resumed_poll(deadline: tokio::time::Instant) {
        let polls = Arc::new(AtomicUsize::new(0));
        let epoch = CancellationToken::new();
        let counted = Arc::clone(&polls);
        let backend = stream::poll_fn(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            std::task::Poll::<Option<IcebergResult<ListEntry>>>::Pending
        })
        .boxed();
        let mut stream = gate_list_stream(
            backend,
            crate::oracle::reader_pins::ReaderIoPermit::new(
                epoch.clone(),
                CancellationToken::new(),
                deadline,
            ),
        );
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(
            std::pin::Pin::new(&mut stream)
                .poll_next(&mut cx)
                .is_pending(),
            "a parked backend leaves the wrapper pending"
        );
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        epoch.cancel();
        let refused = std::pin::Pin::new(&mut stream).poll_next(&mut cx);
        assert!(
            matches!(refused, std::task::Poll::Ready(Some(Err(_)))),
            "a fence landing while the backend is parked refuses the resumed poll"
        );
        assert_eq!(
            polls.load(Ordering::SeqCst),
            1,
            "the resumed poll never reaches the backend"
        );
        assert!(matches!(
            std::pin::Pin::new(&mut stream).poll_next(&mut cx),
            std::task::Poll::Ready(None)
        ));
        assert_eq!(polls.load(Ordering::SeqCst), 1);
    }

    /// Builds an adapter over a real local backend rooted at `root`.
    ///
    /// A real handle rather than a stub: path authority is only meaningful
    /// against the operator the warehouse is actually rooted at, and a fake one
    /// would make every acceptance a statement about the fake.
    ///
    /// # Panics
    /// Panics when the signer or the default storage policy is invalid.
    fn adapter(root: &Path) -> (BifrostIcebergStorage, String) {
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
        (BifrostIcebergStorage::new(storage, &warehouse), warehouse)
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

/// The one message every refused write on the read-only query adapter reports.
///
/// Query-scoped storage exists to read a pinned snapshot. A write reaching it
/// is a construction mistake, not a permission failure, so it names the adapter
/// rather than suggesting the caller retry with different authority.
const READ_ONLY_QUERY_STORAGE: &str =
    "epoch-gated query storage is read-only and cannot write, delete, or open an output";

/// Maps a refused permit onto Iceberg's error type.
///
/// `FeatureUnsupported` is deliberate: the object is reachable and the request
/// is well formed, but this process no longer holds the authority that made
/// reading it safe, and Iceberg must not retry that at its own layer the way it
/// retries `Unexpected`.
fn permit_error(error: &wyrd_spec::vala::BifrostError) -> IcebergError {
    IcebergError::new(IcebergErrorKind::FeatureUnsupported, error.to_string())
}

/// Wraps one lazy listing so the permit gates every poll and every item.
///
/// A listing is the one storage operation whose work outlives the call that
/// created it: the backend stream is returned unread and polled later, by
/// whatever consumes it. Checking the permit only at construction would
/// therefore leave the entire enumeration ungated, so the permit is checked
/// immediately before each backend poll and again before each item — backend
/// errors included — is handed on.
///
/// A refusal yields exactly one error and then terminates: `done` latches, so
/// every later poll reports exhaustion without touching the backend. The
/// wrapper adds no buffering and collects nothing, so laziness and the
/// backend's own backpressure are unchanged.
///
/// The backend is polled directly rather than through a retained `next()`
/// future, because such a future outlives a `Pending` return: a fence landing
/// while the backend is parked would then be followed by a resumed backend
/// poll that no `begin_io` ever authorized. Polling the inner stream here
/// means every single backend poll is preceded by its own permit check.
fn gate_list_stream(
    entries: BoxStream<'static, IcebergResult<ListEntry>>,
    permit: crate::oracle::reader_pins::ReaderIoPermit,
) -> BoxStream<'static, IcebergResult<ListEntry>> {
    let mut entries = entries;
    let mut done = false;
    stream::poll_fn(move |cx| {
        if done {
            return Poll::Ready(None);
        }
        if let Err(error) = permit.begin_io() {
            done = true;
            return Poll::Ready(Some(Err(permit_error(&error))));
        }
        match entries.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => {
                done = true;
                Poll::Ready(None)
            }
            Poll::Ready(Some(item)) => {
                if let Err(error) = permit.expose_result() {
                    done = true;
                    return Poll::Ready(Some(Err(permit_error(&error))));
                }
                Poll::Ready(Some(item))
            }
        }
    })
    .boxed()
}

/// Read-only Iceberg storage that no operation escapes without a live permit.
///
/// Every reachable read checks the permit immediately before the backend call
/// and again after it completes, before anything is returned. The second check
/// is the one that matters for correctness: without it, a read that started
/// legally could still hand bytes from a snapshot to a query whose epoch lost
/// authority while the read was in flight, and Forge may already have deleted
/// what those bytes describe.
///
/// Writes are not gated, they are refused. This adapter is built per query from
/// a pinned immutable cut; there is no correct write through it, so admitting
/// one under a valid permit would be worse than rejecting it.
#[derive(Debug, Clone)]
pub struct EpochGatedIcebergStorage {
    /// Ungated adapter every authorized operation delegates to.
    inner: BifrostIcebergStorage,
    /// Proof that this query's epoch still authorizes snapshot-dependent IO.
    permit: crate::oracle::reader_pins::ReaderIoPermit,
}

impl EpochGatedIcebergStorage {
    /// Binds one query's permit to this node's storage adapter.
    #[must_use]
    pub fn new(
        inner: BifrostIcebergStorage,
        permit: crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Self {
        Self { inner, permit }
    }
}

impl Serialize for EpochGatedIcebergStorage {
    /// Always refuses: the permit is authority local to this process and epoch.
    ///
    /// # Errors
    /// Always returns a serializer error carrying [`NOT_PORTABLE`].
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(NOT_PORTABLE))
    }
}

impl<'de> Deserialize<'de> for EpochGatedIcebergStorage {
    /// Always refuses: a reconstructed permit would authorize nothing real.
    ///
    /// # Errors
    /// Always returns a deserializer error carrying [`NOT_PORTABLE`].
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(D::Error::custom(NOT_PORTABLE))
    }
}

#[async_trait]
#[typetag::serde]
impl Storage for EpochGatedIcebergStorage {
    async fn exists(&self, path: &str) -> IcebergResult<bool> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let found = self.inner.exists(path).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(found)
    }

    async fn metadata(&self, path: &str) -> IcebergResult<FileMetadata> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let metadata = self.inner.metadata(path).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(metadata)
    }

    async fn read(&self, path: &str) -> IcebergResult<Bytes> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let bytes = self.inner.read(path).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(bytes)
    }

    async fn reader(&self, path: &str) -> IcebergResult<Box<dyn FileRead>> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let inner = self.inner.reader(path).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(Box::new(EpochGatedFileRead {
            inner,
            permit: self.permit.clone(),
        }))
    }

    async fn write(&self, _path: &str, _bs: Bytes) -> IcebergResult<()> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }

    async fn writer(&self, _path: &str) -> IcebergResult<Box<dyn FileWrite>> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }

    async fn delete(&self, _path: &str) -> IcebergResult<()> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }

    async fn delete_prefix(&self, _path: &str) -> IcebergResult<()> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }

    async fn delete_stream(&self, _paths: BoxStream<'static, String>) -> IcebergResult<()> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }

    async fn list(
        &self,
        path: &str,
        recursive: bool,
    ) -> IcebergResult<BoxStream<'static, IcebergResult<ListEntry>>> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let entries = self.inner.list(path, recursive).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(gate_list_stream(entries, self.permit.clone()))
    }

    fn new_input(&self, path: &str) -> IcebergResult<InputFile> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        self.inner.new_input(path)?;
        Ok(InputFile::new(Arc::new(self.clone()), path.to_owned()))
    }

    fn new_output(&self, _path: &str) -> IcebergResult<OutputFile> {
        Err(IcebergError::new(
            IcebergErrorKind::FeatureUnsupported,
            READ_ONLY_QUERY_STORAGE,
        ))
    }
}

/// A ranged reader that re-checks the permit around every range.
///
/// A Parquet scan opens one reader and then issues many ranged reads over the
/// life of a query, so checking only at open would leave the longest-lived
/// route to object bytes ungated for the rest of the query.
pub struct EpochGatedFileRead {
    /// Ungated reader every authorized range delegates to.
    inner: Box<dyn FileRead>,
    /// Proof that this query's epoch still authorizes snapshot-dependent IO.
    permit: crate::oracle::reader_pins::ReaderIoPermit,
}

impl std::fmt::Debug for EpochGatedFileRead {
    /// Prints the wrapper without the object identity the reader holds.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpochGatedFileRead")
            .field("permit", &self.permit)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl FileRead for EpochGatedFileRead {
    async fn read(&self, range: Range<u64>) -> IcebergResult<Bytes> {
        self.permit.begin_io().map_err(|e| permit_error(&e))?;
        let bytes = self.inner.read(range).await?;
        self.permit.expose_result().map_err(|e| permit_error(&e))?;
        Ok(bytes)
    }
}

/// Builds epoch-gated storage for exactly one query.
///
/// Iceberg asks the factory per catalog configuration and ignores it here for
/// the same reason [`BifrostIcebergStorageFactory`] does: the backend is
/// already decided. What this factory adds is that every instance it hands out
/// carries the same query's permit, so a table built through it cannot acquire
/// an ungated route to storage partway down its own metadata tree.
#[derive(Debug, Clone)]
pub struct EpochGatedIcebergStorageFactory {
    /// The gated storage every built instance shares.
    storage: EpochGatedIcebergStorage,
}

impl EpochGatedIcebergStorageFactory {
    /// Binds a factory to one owner, one warehouse root, and one permit.
    #[must_use]
    pub fn new(
        storage: Arc<BifrostStorage>,
        warehouse: &str,
        permit: crate::oracle::reader_pins::ReaderIoPermit,
    ) -> Self {
        Self {
            storage: EpochGatedIcebergStorage::new(
                BifrostIcebergStorage::new(storage, warehouse),
                permit,
            ),
        }
    }
}

impl Serialize for EpochGatedIcebergStorageFactory {
    /// Always refuses: the permit is authority local to this process and epoch.
    ///
    /// # Errors
    /// Always returns a serializer error carrying [`NOT_PORTABLE`].
    fn serialize<S: Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom(NOT_PORTABLE))
    }
}

impl<'de> Deserialize<'de> for EpochGatedIcebergStorageFactory {
    /// Always refuses: a reconstructed permit would authorize nothing real.
    ///
    /// # Errors
    /// Always returns a deserializer error carrying [`NOT_PORTABLE`].
    fn deserialize<D: Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(D::Error::custom(NOT_PORTABLE))
    }
}

#[typetag::serde]
impl StorageFactory for EpochGatedIcebergStorageFactory {
    fn build(&self, _config: &StorageConfig) -> IcebergResult<Arc<dyn Storage>> {
        Ok(Arc::new(self.storage.clone()))
    }
}
