//! Ranged-read probe over a test server's Bifrost object store.
//!
//! Oracle's `DataFusion` scans read Parquet footers and pages through ranged
//! gets. Journeys that must hold a query at its first object read, or prove a
//! pruned object was never opened, observe those reads here instead of through
//! a production seam.

use std::fmt;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use object_store::path::Path;
use object_store::{
    CopyOptions, GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore,
    PutMultipartOptions, PutOptions, PutPayload, PutResult, RenameOptions, Result,
};

/// Delegating store that counts ranged reads and can stall the next one.
///
/// Every operation runs against the wrapped production store unchanged; only
/// ranged gets are counted, and at most one armed stall holds a read until
/// the journey releases it.
#[derive(Debug)]
pub struct ObjectReadProbe {
    /// The production store every call delegates to.
    inner: Arc<dyn ObjectStore>,
    /// Ranged gets observed since construction.
    ranged_reads: AtomicUsize,
    /// The stall the next ranged get takes, when one is armed.
    armed: Mutex<Option<Arc<ReadStall>>>,
}

impl ObjectReadProbe {
    /// Wraps the production store a test server's Bifrost owner was built with.
    #[must_use]
    pub fn new(inner: Arc<dyn ObjectStore>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            ranged_reads: AtomicUsize::new(0),
            armed: Mutex::new(None),
        })
    }

    /// Returns the ranged gets observed since construction.
    #[must_use]
    pub fn ranged_reads(&self) -> usize {
        self.ranged_reads.load(Ordering::Acquire)
    }

    /// Arms a fresh stall the next ranged get takes, replacing any unused one.
    ///
    /// A fresh stall per arming keeps a release issued against an earlier,
    /// never-reached stall from letting the next one through.
    ///
    /// # Panics
    /// Panics when the stall lock is poisoned.
    #[must_use]
    pub fn arm_stall(&self) -> Arc<ReadStall> {
        let stall = Arc::new(ReadStall::default());
        *self.armed.lock().expect("read-stall lock is not poisoned") = Some(Arc::clone(&stall));
        stall
    }

    /// Counts one ranged get and holds it at the armed stall, if any.
    ///
    /// # Panics
    /// Panics when the stall lock is poisoned.
    async fn observe_ranged_read(&self) {
        self.ranged_reads.fetch_add(1, Ordering::AcqRel);
        let stall = self
            .armed
            .lock()
            .expect("read-stall lock is not poisoned")
            .take();
        if let Some(stall) = stall {
            stall.hold().await;
        }
    }
}

impl fmt::Display for ObjectReadProbe {
    /// Names the probe and the store it wraps.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObjectReadProbe({})", self.inner)
    }
}

#[async_trait]
impl ObjectStore for ObjectReadProbe {
    /// Delegates a put unchanged.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> Result<PutResult> {
        self.inner.put_opts(location, payload, opts).await
    }

    /// Delegates a multipart upload unchanged.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOptions,
    ) -> Result<Box<dyn MultipartUpload>> {
        self.inner.put_multipart_opts(location, opts).await
    }

    /// Delegates a get, counting and possibly stalling it when it is ranged.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        if options.range.is_some() {
            self.observe_ranged_read().await;
        }
        self.inner.get_opts(location, options).await
    }

    /// Delegates a vectored ranged get as one counted, possibly stalled read.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn get_ranges(&self, location: &Path, ranges: &[Range<u64>]) -> Result<Vec<Bytes>> {
        self.observe_ranged_read().await;
        self.inner.get_ranges(location, ranges).await
    }

    /// Delegates deletes unchanged.
    fn delete_stream(
        &self,
        locations: BoxStream<'static, Result<Path>>,
    ) -> BoxStream<'static, Result<Path>> {
        self.inner.delete_stream(locations)
    }

    /// Delegates a listing unchanged.
    fn list(&self, prefix: Option<&Path>) -> BoxStream<'static, Result<ObjectMeta>> {
        self.inner.list(prefix)
    }

    /// Delegates an offset listing unchanged, keeping the store's ordering.
    fn list_with_offset(
        &self,
        prefix: Option<&Path>,
        offset: &Path,
    ) -> BoxStream<'static, Result<ObjectMeta>> {
        self.inner.list_with_offset(prefix, offset)
    }

    /// Delegates a delimited listing unchanged.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }

    /// Delegates a copy unchanged.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn copy_opts(&self, from: &Path, to: &Path, options: CopyOptions) -> Result<()> {
        self.inner.copy_opts(from, to, options).await
    }

    /// Delegates a rename unchanged, keeping the store's native rename.
    ///
    /// # Errors
    /// Returns the wrapped store's error.
    async fn rename_opts(&self, from: &Path, to: &Path, options: RenameOptions) -> Result<()> {
        self.inner.rename_opts(from, to, options).await
    }
}

/// One armed hold on a ranged read, reached once and released once.
#[derive(Debug, Default)]
pub struct ReadStall {
    /// Set once a ranged read is held here.
    reached: AtomicBool,
    /// Wakes journeys waiting for the hold.
    reached_ready: tokio::sync::Notify,
    /// Lets the held read continue; a release before the hold is kept.
    release: tokio::sync::Notify,
}

impl ReadStall {
    /// Waits until a ranged read is held at this stall.
    pub async fn wait_until_reached(&self) {
        loop {
            let notified = self.reached_ready.notified();
            if self.reached.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }

    /// Lets the held read, or the next one to arrive, continue.
    pub fn release(&self) {
        self.release.notify_one();
    }

    /// Marks this stall reached and waits for its release.
    async fn hold(&self) {
        self.reached.store(true, Ordering::Release);
        self.reached_ready.notify_waiters();
        self.release.notified().await;
    }
}
