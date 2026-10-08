//! The synchronous projection of [`crate::bifrost::Bifrost`], for callers with no
//! runtime of their own.
//!
//! Every method drives the async client on the shared Wyrd runtime. The async
//! client is the implementation, not a parallel one, so the two can never
//! disagree about what a method does — only about who drives it. This mirrors
//! `reqwest::blocking`, which is already in the dependency tree and sets the
//! reader's expectation for the pairing.

use crate::WyrdClient;
use arrow::record_batch::RecordBatch;
use wyrd_queue::QueueConfig;
use wyrd_spec::request_id::RequestId;
#[cfg(feature = "internal")]
use wyrd_spec::vala::api::BifrostQueryRequest;
use wyrd_spec::vala::api::{
    BifrostTableDescription, CancelRunningQueryResponse, QueryParam, RegisterOutcome,
    RunningQuerySummary,
};

use crate::bifrost::facade::QueryResult;
use crate::bifrost::query::{BifrostClientError, QueryResultStream};
#[cfg(feature = "internal")]
use crate::bifrost::query::{CollectedQueryLimits, CollectedQueryResult};
use crate::bifrost::table::{Correlation, TableConfig};

/// The synchronous [`crate::bifrost::Bifrost`].
///
/// # Panics
///
/// Calling any method from inside an async context panics, as
/// `reqwest::blocking` does: blocking a runtime worker on the same runtime it
/// is driving deadlocks, and a panic naming the mistake is better than a hang.
pub struct Bifrost {
    /// The async client every method blocks on.
    inner: crate::bifrost::Bifrost,
}

impl Bifrost {
    /// Build a client with no arguments, resolving its own transport.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::from_env`].
    pub fn from_env() -> Result<Self, BifrostClientError> {
        Ok(Self {
            inner: block_on(crate::bifrost::Bifrost::from_env())?,
        })
    }

    /// Build a query-only client over an explicitly supplied transport.
    ///
    /// # Arguments
    /// * `client` - The authenticated client whose credential and connection pools
    ///   both planes share; it is borrowed, not owned.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::connect`].
    pub fn connect(client: &WyrdClient) -> Result<Self, BifrostClientError> {
        Ok(Self {
            inner: block_on(crate::bifrost::Bifrost::connect(client))?,
        })
    }

    /// Build a client already bound to `table` for writes.
    ///
    /// # Arguments
    /// * `client` - The authenticated client whose transport both planes share.
    /// * `table` - The table bound as the initial write target.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::connect`].
    pub fn connect_with_table(
        client: &WyrdClient,
        table: TableConfig,
    ) -> Result<Self, BifrostClientError> {
        Ok(Self {
            inner: block_on(crate::bifrost::Bifrost::connect_with_table(client, table))?,
        })
    }

    /// Build a client with explicit producer tuning.
    ///
    /// # Arguments
    /// * `client` - The authenticated client whose transport both planes share.
    /// * `table` - The initial write target, or `None` to bind one later.
    /// * `config` - Producer tuning, validated before the ingest channel is dialled.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::connect`].
    pub fn connect_with_config(
        client: &WyrdClient,
        table: Option<TableConfig>,
        config: QueueConfig,
    ) -> Result<Self, BifrostClientError> {
        Ok(Self {
            inner: block_on(crate::bifrost::Bifrost::connect_with_config(
                client, table, config,
            ))?,
        })
    }

    /// Create the active table, returning whether it was created or matched.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::register`].
    pub fn register(&self) -> Result<RegisterOutcome, BifrostClientError> {
        block_on(self.inner.register())
    }

    /// Bind `table` as the write target, returning the previous binding.
    ///
    /// # Arguments
    /// * `table` - The table to bind as the new write target.
    pub fn use_table(&self, table: TableConfig) -> Option<TableConfig> {
        self.inner.use_table(table)
    }

    /// Bind an already-registered table by name.
    ///
    /// # Arguments
    /// * `fqn` - The registered table's `<namespace>.<name>`.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::use_table_by_name`].
    pub fn use_table_by_name(&self, fqn: &str) -> Result<(), BifrostClientError> {
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
    /// # Arguments
    /// * `row` - One JSON-encoded row whose fields match the active table's schema.
    /// * `correlation` - The card and run identities stamped onto the row.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::insert`].
    pub fn insert(&self, row: Vec<u8>, correlation: Correlation) -> Result<(), BifrostClientError> {
        self.inner.insert(row, correlation)
    }

    /// Flush every pooled producer and wait for each durable acknowledgement.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::flush`].
    pub fn flush(&self) -> Result<(), BifrostClientError> {
        block_on(self.inner.flush())
    }

    /// Drain every producer and stop its background task.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::shutdown`].
    pub fn shutdown(&self) -> Result<(), BifrostClientError> {
        block_on(self.inner.shutdown())
    }

    /// Run one SQL SELECT and collect every batch.
    ///
    /// # Arguments
    /// * `query` - The SQL SELECT text.
    /// * `params` - Positional bind values; `params[i]` binds `$(i + 1)`.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::sql`].
    pub fn sql(
        &self,
        query: &str,
        params: &[QueryParam],
    ) -> Result<QueryResult, BifrostClientError> {
        block_on(self.inner.sql(query, params))
    }

    /// Run one SQL SELECT and deserialize every row into `T`.
    ///
    /// # Arguments
    /// * `query` - The SQL SELECT text.
    /// * `params` - Positional bind values; `params[i]` binds `$(i + 1)`.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::sql_as`].
    pub fn sql_as<T: serde::de::DeserializeOwned>(
        &self,
        query: &str,
        params: &[QueryParam],
    ) -> Result<Vec<T>, BifrostClientError> {
        block_on(self.inner.sql_as(query, params))
    }

    /// Run one SQL SELECT and iterate its batches as they arrive.
    ///
    /// # Arguments
    /// * `query` - The SQL SELECT text.
    /// * `params` - Positional bind values.
    /// * `deadline` - The server-side query deadline, or `None` for the
    ///   server default.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::stream`].
    pub fn stream(
        &self,
        query: &str,
        params: &[QueryParam],
        deadline: Option<std::time::Duration>,
    ) -> Result<BlockingQueryStream, BifrostClientError> {
        Ok(BlockingQueryStream {
            inner: block_on(self.inner.stream(query, params, deadline))?,
        })
    }

    /// Read one registered table's server-owned description.
    ///
    /// # Arguments
    /// * `namespace` - The table's namespace.
    /// * `name` - The table's name within `namespace`.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::describe_table`].
    pub fn describe_table(
        &self,
        namespace: &str,
        name: &str,
    ) -> Result<BifrostTableDescription, BifrostClientError> {
        block_on(self.inner.describe_table(namespace, name))
    }

    /// Start one query from a complete request and iterate its batches.
    ///
    /// # Arguments
    /// * `request` - The complete query request: SQL, bind values, and deadline.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::query`].
    #[cfg(feature = "internal")]
    pub fn query(
        &self,
        request: &BifrostQueryRequest,
    ) -> Result<BlockingQueryStream, BifrostClientError> {
        Ok(BlockingQueryStream {
            inner: block_on(self.inner.query(request))?,
        })
    }

    /// Run one query and collect it within explicit limits.
    ///
    /// # Arguments
    /// * `request` - The complete query request: SQL, bind values, and deadline.
    /// * `limits` - The row and encoded-byte ceilings the collected result must fit.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::collect_bounded`].
    #[cfg(feature = "internal")]
    pub fn collect_bounded(
        &self,
        request: &BifrostQueryRequest,
        limits: CollectedQueryLimits,
    ) -> Result<CollectedQueryResult, BifrostClientError> {
        block_on(self.inner.collect_bounded(request, limits))
    }

    /// List the active queries visible to the authenticated tenant.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::running`].
    pub fn running(&self) -> Result<Vec<RunningQuerySummary>, BifrostClientError> {
        block_on(self.inner.running())
    }

    /// Get one active query visible to the authenticated tenant.
    ///
    /// # Arguments
    /// * `request_id` - The id of the active query to look up.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::status`].
    pub fn status(
        &self,
        request_id: &RequestId,
    ) -> Result<RunningQuerySummary, BifrostClientError> {
        block_on(self.inner.status(request_id))
    }

    /// Request server-side cancellation of one active query.
    ///
    /// # Arguments
    /// * `request_id` - The id of the active query to cancel.
    ///
    /// # Errors
    ///
    /// As [`crate::bifrost::Bifrost::cancel`].
    pub fn cancel(
        &self,
        request_id: &RequestId,
    ) -> Result<CancelRunningQueryResponse, BifrostClientError> {
        block_on(self.inner.cancel(request_id))
    }

    /// The async client underneath, for a caller that acquires a runtime later.
    #[must_use]
    #[cfg(feature = "internal")]
    pub fn into_async(self) -> crate::bifrost::Bifrost {
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
    type Item = Result<RecordBatch, BifrostClientError>;

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
