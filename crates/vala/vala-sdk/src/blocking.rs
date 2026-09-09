//! The synchronous projection of [`crate::Bifrost`], for callers with no
//! runtime of their own.
//!
//! Every method drives the async client on the shared Wyrd runtime. The async
//! client is the implementation, not a parallel one, so the two can never
//! disagree about what a method does — only about who drives it. This mirrors
//! `reqwest::blocking`, which is already in the dependency tree and sets the
//! reader's expectation for the pairing.

use arrow::record_batch::RecordBatch;
use wyrd_client::WyrdClient;
use wyrd_queue::QueueConfig;
use wyrd_spec::vala::api::{BifrostTableDescription, RegisterOutcome};

use crate::bifrost::QueryResult;
use crate::query::{QueryClient, QueryResultStream, ValaSdkError};
use crate::table::{Correlation, TableConfig};

/// The synchronous [`crate::Bifrost`].
///
/// # Panics
///
/// Calling any method from inside an async context panics, as
/// `reqwest::blocking` does: blocking a runtime worker on the same runtime it
/// is driving deadlocks, and a panic naming the mistake is better than a hang.
pub struct Bifrost {
    /// The async client every method blocks on.
    inner: crate::Bifrost,
}

impl Bifrost {
    /// Build a client with no arguments, resolving its own transport.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::from_env`].
    pub fn from_env() -> Result<Self, ValaSdkError> {
        Ok(Self {
            inner: block_on(crate::Bifrost::from_env())?,
        })
    }

    /// Build a query-only client over an explicitly supplied transport.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::connect`].
    pub fn connect(client: &WyrdClient) -> Result<Self, ValaSdkError> {
        Ok(Self {
            inner: block_on(crate::Bifrost::connect(client))?,
        })
    }

    /// Build a client already bound to `table` for writes.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::connect`].
    pub fn connect_with_table(
        client: &WyrdClient,
        table: TableConfig,
    ) -> Result<Self, ValaSdkError> {
        Ok(Self {
            inner: block_on(crate::Bifrost::connect_with_table(client, table))?,
        })
    }

    /// Build a client with explicit producer tuning.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::connect`].
    pub fn connect_with_config(
        client: &WyrdClient,
        table: Option<TableConfig>,
        config: QueueConfig,
    ) -> Result<Self, ValaSdkError> {
        Ok(Self {
            inner: block_on(crate::Bifrost::connect_with_config(client, table, config))?,
        })
    }

    /// Create the active table, returning whether it was created or matched.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::register`].
    pub fn register(&self) -> Result<RegisterOutcome, ValaSdkError> {
        block_on(self.inner.register())
    }

    /// Bind `table` as the write target, returning the previous binding.
    pub fn use_table(&self, table: TableConfig) -> Option<TableConfig> {
        self.inner.use_table(table)
    }

    /// Bind an already-registered table by name.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::use_table_by_name`].
    pub fn use_table_by_name(&self, fqn: &str) -> Result<(), ValaSdkError> {
        block_on(self.inner.use_table_by_name(fqn))
    }

    /// The active write binding, if any.
    #[must_use]
    pub fn table(&self) -> Option<TableConfig> {
        self.inner.table()
    }

    /// Enqueue one JSON row into the active table.
    ///
    /// Already synchronous on the async client, so this forwards rather than
    /// blocking: enqueueing is a bounded channel send, not IO.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::insert`].
    pub fn insert(&self, row: Vec<u8>, correlation: Correlation) -> Result<(), ValaSdkError> {
        self.inner.insert(row, correlation)
    }

    /// Flush every pooled producer and wait for each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::flush`].
    pub fn flush(&self) -> Result<(), ValaSdkError> {
        block_on(self.inner.flush())
    }

    /// Drain every producer and stop its background task.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::shutdown`].
    pub fn shutdown(&self) -> Result<(), ValaSdkError> {
        block_on(self.inner.shutdown())
    }

    /// Run one SQL SELECT and collect every batch.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::sql`].
    pub fn sql(&self, query: &str) -> Result<QueryResult, ValaSdkError> {
        block_on(self.inner.sql(query))
    }

    /// Run one SQL SELECT and iterate its batches as they arrive.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::stream`].
    pub fn stream(&self, query: &str) -> Result<BlockingQueryStream, ValaSdkError> {
        Ok(BlockingQueryStream {
            inner: block_on(self.inner.stream(query))?,
        })
    }

    /// Read one registered table's server-owned description.
    ///
    /// # Errors
    ///
    /// As [`crate::Bifrost::describe`].
    pub fn describe(&self, fqn: &str) -> Result<BifrostTableDescription, ValaSdkError> {
        block_on(self.inner.describe(fqn))
    }

    /// Escape hatch to the query plane's lifecycle surface: running, status,
    /// cancel, and the raw request form `sql` and `stream` wrap.
    ///
    /// The async client's own [`QueryClient`], not a second implementation: its
    /// methods are futures a caller drives on whatever runtime it already has,
    /// so the blocking facade adds no advanced-query behavior of its own.
    #[must_use]
    pub fn query_client(&self) -> &QueryClient {
        self.inner.query_client()
    }

    /// The async client underneath, for a caller that acquires a runtime later.
    #[must_use]
    pub fn into_async(self) -> crate::Bifrost {
        self.inner
    }
}

/// A blocking iterator over one query's Arrow batches.
///
/// Each `next` drives one asynchronous batch read to completion on the shared
/// runtime, so the terminal-frame requirement and cancellation semantics are
/// exactly the async stream's — the iterator only changes who waits.
pub struct BlockingQueryStream {
    /// The async stream every `next` drives one step of.
    inner: QueryResultStream,
}

impl BlockingQueryStream {
    /// The validated terminal frame, present only after the stream completes.
    #[must_use]
    pub fn terminal(&self) -> Option<&wyrd_spec::vala::api::QueryTerminalFrame> {
        self.inner.terminal()
    }
}

impl Iterator for BlockingQueryStream {
    type Item = Result<RecordBatch, ValaSdkError>;

    /// Read the next batch, ending on the validated terminal.
    ///
    /// A stream that ends without its required terminal yields the
    /// incomplete-stream error rather than `None`, so a truncated response is
    /// never mistaken for an empty result.
    fn next(&mut self) -> Option<Self::Item> {
        match block_on(self.inner.next_batch()) {
            Ok(Some(batch)) => Some(Ok(batch)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

/// Drive one future to completion on the shared Wyrd runtime.
///
/// # Panics
///
/// Panics when called from inside an async context, which would block a
/// runtime worker on the runtime it is driving.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    wyrd_runtime::runtime().block_on(future)
}
