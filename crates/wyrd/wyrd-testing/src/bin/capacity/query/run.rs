//! The application side of the Bifrost experiments: public-client writes,
//! OTLP exports, and queries against the running server, and what each
//! closed loop observed.
//!
//! Every query is checked against its exact answer; every write resolves at
//! its durable acknowledgement. Server evidence for a window comes from the
//! shared [`wyrd_testing::capacity::Window`], never from this module.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use arrow::array::{AsArray, RecordBatch};
use arrow::datatypes::{DataType, Int64Type};
use num_traits::ToPrimitive;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use wyrd_client::bifrost::{BifrostClientError, BifrostOptions, TableConfig};
use wyrd_client::transport::trace::with_caller_trace;
use wyrd_client::{Bifrost, WyrdClient};
use wyrd_spec::trace::TraceContext;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryTerminalOutcome};
use wyrd_testing::capacity::request_trace;
use wyrd_testing::release_server::LocalServer;
use wyrd_tonic::otlp::logs_service::logs_service_client::LogsServiceClient;
use wyrd_tonic::otlp::trace_service::trace_service_client::TraceServiceClient;
use wyrd_tonic::prost::Message as _;
use wyrd_tonic::tonic::Request;
use wyrd_tonic::tonic::transport::Channel;

use crate::query::Result;
use crate::query::logs::{self, Logs};
use crate::query::telemetry::{self, Telemetry};
use crate::query::workload::{self, Case, Fixture, Rows};

/// Oracle's production queue places.
pub const QUEUE_PLACES: usize = 1_000;

/// Stable code of the queue-full refusal, the one admission refusal the
/// experiments declare, and only during deliberate overload.
pub const QUEUE_FULL: &str = "WYRD_VALA_429_QUERY_QUEUE_FULL";

/// Stable code a query whose rows differ from the exact answer counts under.
pub const WRONG_RESULT: &str = "wrong-result";

/// How long the queue may take to fill or to empty.
const QUEUE_SETTLE: Duration = Duration::from_secs(60);

/// How long the query sent into the full queue may take to be refused.
const OVERFLOW_WAIT: Duration = Duration::from_secs(30);

/// What one closed loop observed.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Latency of every successful request, microseconds.
    pub latencies_us: Vec<u64>,
    /// Failed requests by stable error code, `wrong-result`, or terminal.
    pub errors: BTreeMap<String, u64>,
    /// Rows durably acknowledged by a write loop.
    pub rows: u64,
    /// Encoded OTLP protobuf bytes of the acknowledged export requests,
    /// before transport framing and without compression; zero for native
    /// writes.
    pub wire_bytes: u64,
}

impl Outcome {
    /// Failed requests of every code.
    pub fn error_count(&self) -> u64 {
        self.errors.values().sum()
    }

    /// Folds another client's results into this one.
    fn merge(&mut self, other: Self) {
        self.latencies_us.extend(other.latencies_us);
        self.rows += other.rows;
        self.wire_bytes += other.wire_bytes;
        for (code, count) in other.errors {
            *self.errors.entry(code).or_default() += count;
        }
    }
}

/// The ingest path a write loop sends through: the public Bifrost writer or
/// one OTLP collector service.
#[derive(Clone)]
pub enum Sender {
    /// Native Arrow writes of `fixture` rows into `table`.
    Native {
        /// The registered public writer.
        writer: Arc<Bifrost>,
        /// The table written.
        table: &'static str,
        /// The rows' generator.
        fixture: Fixture,
    },
    /// Telemetry spans through the trace service into `vala.traces.spans`.
    Spans(Otlp, Telemetry),
    /// Bulky Kubernetes logs through the logs service into
    /// `vala.logs.records`.
    Logs(Otlp, Logs),
}

/// The server's OTLP gRPC collector, reached with the client's exchanged
/// bearer as an application's OpenTelemetry SDK exporter would.
#[derive(Clone)]
pub struct Otlp {
    /// The connected gRPC channel.
    channel: Channel,
    /// The client whose bearer every export presents.
    client: WyrdClient,
}

impl Otlp {
    /// Connects to `client`'s gRPC endpoint.
    ///
    /// # Errors
    ///
    /// Returns the endpoint or connection failure.
    pub async fn connect(client: &WyrdClient) -> Result<Self> {
        let channel = Channel::from_shared(client.grpc_url().to_owned())?
            .connect()
            .await?;
        Ok(Self {
            channel,
            client: client.clone(),
        })
    }

    /// The `x-wyrd-access-token` value of the client's current bearer.
    ///
    /// # Errors
    ///
    /// Returns `bearer` when no bearer can be exchanged.
    async fn token(&self) -> std::result::Result<String, String> {
        let bearer = self
            .client
            .auth()
            .bearer()
            .await
            .map_err(|_| "bearer".to_owned())?;
        Ok(format!("Bearer {}", bearer.expose()))
    }
}

impl Sender {
    /// A native writer of `fixture` rows into `table`, registered through
    /// the public client.
    ///
    /// # Errors
    ///
    /// Returns the connection or registration failure.
    pub async fn native(
        client: &WyrdClient,
        table: &'static str,
        fixture: Fixture,
    ) -> Result<Self> {
        let writer = Bifrost::connect_with(
            client,
            BifrostOptions {
                table: Some(TableConfig::from_arrow(table, workload::schema())?),
                ..BifrostOptions::default()
            },
        )
        .await?;
        writer.register().await?;
        Ok(Self::Native {
            writer: Arc::new(writer),
            table,
            fixture,
        })
    }

    /// Rows per request: [`workload::REQUEST_ROWS`] for native writes, the
    /// SDK batch processors' export size for OTLP.
    #[must_use]
    pub const fn request_rows(&self) -> i64 {
        match self {
            Self::Native { .. } => workload::REQUEST_ROWS,
            Self::Spans(..) => telemetry::REQUEST_SPANS,
            Self::Logs(..) => logs::REQUEST_RECORDS,
        }
    }

    /// Sends rows `first..last` as a request continuing `trace`, so its
    /// server timing carries `trace`'s id, and resolves at their durable
    /// acknowledgement with the encoded OTLP bytes sent (zero for native).
    ///
    /// # Errors
    ///
    /// Returns the stable code the failure is counted by: the client error
    /// code, the gRPC status code, or `partial-success` when an export
    /// dropped rows.
    pub async fn send(
        &self,
        first: i64,
        last: i64,
        trace: &TraceContext,
    ) -> std::result::Result<u64, String> {
        match self {
            Self::Native {
                writer,
                table,
                fixture,
            } => {
                let batch = fixture
                    .batch(first, last)
                    .map_err(|_| "fixture".to_owned())?;
                with_caller_trace(Some(trace.clone()), writer.write_batch(table, &batch))
                    .await
                    .map_err(|error| code(&error))?;
                Ok(0)
            }
            Self::Spans(otlp, fixture) => {
                let body = fixture.request(first, last);
                let encoded = body.encoded_len();
                let token = otlp.token().await?;
                let response = TraceServiceClient::new(otlp.channel.clone())
                    .export(authorized(body, &token, trace)?)
                    .await
                    .map_err(|status| format!("{:?}", status.code()))?;
                let rejected = response
                    .into_inner()
                    .partial_success
                    .map_or(0, |partial| partial.rejected_spans);
                accepted(encoded, rejected)
            }
            Self::Logs(otlp, fixture) => {
                let body = fixture.request(first, last);
                let encoded = body.encoded_len();
                let token = otlp.token().await?;
                let response = LogsServiceClient::new(otlp.channel.clone())
                    .export(authorized(body, &token, trace)?)
                    .await
                    .map_err(|status| format!("{:?}", status.code()))?;
                let rejected = response
                    .into_inner()
                    .partial_success
                    .map_or(0, |partial| partial.rejected_log_records);
                accepted(encoded, rejected)
            }
        }
    }

    /// Sends from `writers` concurrent closed-loop senders, each taking the
    /// next [`Self::request_rows`] rows from the shared `cursor`, until the
    /// cursor reaches `end` or `stop` is cancelled.
    ///
    /// The cursor only grows, so every request carries fresh identities and
    /// a later loop never repeats an earlier one's rows as a retry.
    pub async fn closed_loop(
        &self,
        cursor: &Arc<AtomicI64>,
        end: i64,
        writers: usize,
        stop: &CancellationToken,
    ) -> Outcome {
        let chunk = self.request_rows();
        let mut tasks = JoinSet::new();
        for _ in 0..writers {
            let (sender, cursor, stop) = (self.clone(), Arc::clone(cursor), stop.clone());
            tasks.spawn(async move {
                let mut outcome = Outcome::default();
                while !stop.is_cancelled() {
                    let first = cursor.fetch_add(chunk, Ordering::Relaxed);
                    if first >= end {
                        break;
                    }
                    let last = (first + chunk).min(end);
                    let sent = Instant::now();
                    match sender.send(first, last, &request_trace()).await {
                        Ok(wire) => {
                            outcome.latencies_us.push(micros(sent));
                            outcome.rows += (last - first).cast_unsigned();
                            outcome.wire_bytes += wire;
                        }
                        Err(code) => *outcome.errors.entry(code).or_default() += 1,
                    }
                }
                outcome
            });
        }
        join(tasks).await
    }
}

/// The encoded size of an acknowledged export, or `partial-success` when it
/// dropped rows.
fn accepted(encoded: usize, rejected: i64) -> std::result::Result<u64, String> {
    if rejected > 0 {
        Err("partial-success".to_owned())
    } else {
        Ok(encoded.to_u64().unwrap_or(u64::MAX))
    }
}

/// What the deliberate overload observed.
#[derive(Debug, Default)]
pub struct Overload {
    /// Highest `oracle_queries_queued` seen while filling.
    pub queued: usize,
    /// Whether the query sent into the full queue was refused with
    /// [`QUEUE_FULL`].
    pub overflow_refused: bool,
    /// What the overflow query observed: its error code, `answered`, or
    /// `queued` when it waited past [`OVERFLOW_WAIT`].
    pub overflow: String,
    /// Seconds for the queue to empty after the waiters disconnected, or
    /// `None` when it did not within [`QUEUE_SETTLE`].
    pub drained_seconds: Option<f64>,
}

/// The public query handle, the fixture under test, and its exact answers.
pub struct Bench {
    /// Public query handle every query client shares.
    queries: Arc<Bifrost>,
    /// Fixture size under test.
    fixture: Fixture,
    /// Exact answer of every aggregate variant, computed once.
    answers: Arc<BTreeMap<Case, Vec<Rows>>>,
}

impl Bench {
    /// Opens the query handle and computes the exact answers of `cases`.
    pub fn new(client: &WyrdClient, fixture: Fixture, cases: &[Case]) -> Self {
        let answers = cases
            .iter()
            .filter(|case| **case != Case::Selective)
            .map(|case| {
                let variants = (0..case.variants())
                    .map(|variant| case.expected(fixture, variant))
                    .collect();
                (*case, variants)
            })
            .collect();
        Self {
            queries: Arc::new(Bifrost::query_only(client)),
            fixture,
            answers: Arc::new(answers),
        }
    }

    /// The public query handle.
    pub fn queries(&self) -> Arc<Bifrost> {
        Arc::clone(&self.queries)
    }

    /// The statement and exact answer of `case` for `sequence`: selective
    /// answers are computed per id, aggregate answers come from the
    /// precomputed table.
    pub fn question(&self, case: Case, sequence: u64) -> (String, Rows) {
        let expected = match self.answers.get(&case) {
            Some(variants) => {
                variants[(sequence % case.variants()).to_usize().unwrap_or(0)].clone()
            }
            None => case.expected(self.fixture, sequence),
        };
        (case.sql(self.fixture, sequence), expected)
    }

    /// Runs `clients` closed-loop clients of `case` until `deadline`, each
    /// taking the next statement from the shared `sequence`.
    pub async fn drive(
        self: &Arc<Self>,
        case: Case,
        clients: usize,
        deadline: Instant,
        sequence: &Arc<AtomicU64>,
    ) -> Outcome {
        let mut tasks = JoinSet::new();
        for _ in 0..clients {
            let (bench, sequence) = (Arc::clone(self), Arc::clone(sequence));
            tasks.spawn(async move {
                let mut outcome = Outcome::default();
                while Instant::now() < deadline {
                    let next = sequence.fetch_add(1, Ordering::Relaxed);
                    let (sql, expected) = bench.question(case, next);
                    let sent = Instant::now();
                    match ask(&bench.queries, sql, &expected).await {
                        Ok(()) => outcome.latencies_us.push(micros(sent)),
                        Err(error) => *outcome.errors.entry(error).or_default() += 1,
                    }
                }
                outcome
            });
        }
        join(tasks).await
    }

    /// Fills Oracle's queue and sends one more query.
    ///
    /// Holders first occupy every execution slot with a streaming scan they
    /// stop reading; waiters then queue until `oracle_queries_queued` reaches
    /// [`QUEUE_PLACES`]; one more query must be refused as queue-full.
    /// Disconnecting the waiters must empty the queue.
    ///
    /// # Errors
    ///
    /// Returns scrape failures.
    pub async fn overload_queue(&self, server: &LocalServer) -> Result<Overload> {
        let slots = server
            .metrics()
            .await?
            .sum("bifrost_oracle_local_slot_units", &["kind=\"limit\""])
            .to_usize()
            .unwrap_or(0);
        let release = CancellationToken::new();
        let mut holders = JoinSet::new();
        for _ in 0..slots {
            let (queries, release) = (Arc::clone(&self.queries), release.clone());
            holders.spawn(async move {
                let request = BifrostQueryRequest {
                    sql: format!("SELECT event_id FROM {}", workload::TABLE),
                    deadline_ms: None,
                    params: Vec::new(),
                };
                if let Ok(mut stream) = queries.query(&request).await {
                    let _ = stream.next_batch().await;
                    release.cancelled().await;
                }
            });
        }
        // A waiter that reaches a free slot before the holders take them all
        // is answered and never queues, so seat every holder first.
        let seating = Instant::now();
        while seating.elapsed() < QUEUE_SETTLE {
            let used = server
                .metrics()
                .await?
                .sum("bifrost_oracle_local_slot_units", &["kind=\"used\""])
                .to_usize()
                .unwrap_or(0);
            if used >= slots {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut waiters = JoinSet::new();
        for sequence in 0..QUEUE_PLACES as u64 {
            let (queries, (sql, expected)) = (
                Arc::clone(&self.queries),
                self.question(Case::Selective, sequence),
            );
            waiters.spawn(async move { ask(&queries, sql, &expected).await });
        }
        let mut overload = Overload::default();
        let filling = Instant::now();
        while overload.queued < QUEUE_PLACES && filling.elapsed() < QUEUE_SETTLE {
            tokio::time::sleep(Duration::from_millis(100)).await;
            overload.queued = overload.queued.max(queued(server).await?);
        }
        let (sql, expected) = self.question(Case::Selective, u64::MAX);
        overload.overflow =
            match tokio::time::timeout(OVERFLOW_WAIT, ask(&self.queries, sql, &expected)).await {
                Ok(Ok(())) => "answered".to_owned(),
                Ok(Err(error)) => error,
                Err(_) => "queued".to_owned(),
            };
        overload.overflow_refused = overload.overflow == QUEUE_FULL;
        waiters.abort_all();
        while waiters.join_next().await.is_some() {}
        let draining = Instant::now();
        while draining.elapsed() < QUEUE_SETTLE {
            if queued(server).await? == 0 {
                overload.drained_seconds = Some(draining.elapsed().as_secs_f64());
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        release.cancel();
        while holders.join_next().await.is_some() {}
        Ok(overload)
    }
}

/// Queries Oracle reports waiting.
///
/// # Errors
///
/// Returns scrape failures.
async fn queued(server: &LocalServer) -> Result<usize> {
    Ok(server
        .metrics()
        .await?
        .sum("oracle_queries_queued", &[])
        .to_usize()
        .unwrap_or(0))
}

/// Sends one query with the server's default deadline, reads it to its
/// terminal, and compares its rows with `expected`.
///
/// # Errors
///
/// Returns the stable error code, the failed terminal's code, or
/// [`WRONG_RESULT`].
pub async fn ask(
    queries: &Bifrost,
    sql: String,
    expected: &Rows,
) -> std::result::Result<(), String> {
    let request = BifrostQueryRequest {
        sql,
        deadline_ms: None,
        params: Vec::new(),
    };
    let mut stream = queries
        .query(&request)
        .await
        .map_err(|error| code(&error))?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await.map_err(|error| code(&error))? {
        rows.extend(int_rows(&batch).map_err(|_| "non-integer column".to_owned())?);
    }
    match stream.terminal() {
        Some(terminal) if terminal.outcome == QueryTerminalOutcome::Success => {
            if rows == *expected {
                Ok(())
            } else {
                Err(WRONG_RESULT.to_owned())
            }
        }
        Some(terminal) => Err(match &terminal.error {
            Some(error) => format!("{:?}", error.code),
            None => format!("{:?}", terminal.outcome),
        }),
        None => Err("no-terminal".to_owned()),
    }
}

/// The stable code a client error projects to.
fn code(error: &BifrostClientError) -> String {
    tracing::debug!(%error, "benchmark request failed");
    wyrd_spec::error::WyrdError::from(error).code().to_owned()
}

/// Casts every column of `batch` to `i64` and returns its rows.
///
/// # Errors
///
/// Returns the cast error for a column that is not integral.
fn int_rows(batch: &RecordBatch) -> std::result::Result<Rows, arrow::error::ArrowError> {
    let columns = batch
        .columns()
        .iter()
        .map(|column| arrow::compute::cast(column, &DataType::Int64))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let columns: Vec<_> = columns
        .iter()
        .map(AsArray::as_primitive::<Int64Type>)
        .collect();
    Ok((0..batch.num_rows())
        .map(|row| columns.iter().map(|column| column.value(row)).collect())
        .collect())
}

/// Waits for every task and merges their results; a panicked task counts as
/// one `panicked` error.
async fn join(mut tasks: JoinSet<Outcome>) -> Outcome {
    let mut total = Outcome::default();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(outcome) => total.merge(outcome),
            Err(_) => *total.errors.entry("panicked".to_owned()).or_default() += 1,
        }
    }
    total
}

/// Microseconds since `sent`.
pub fn micros(sent: Instant) -> u64 {
    u64::try_from(sent.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Wraps `body` in a gRPC request carrying the `x-wyrd-access-token` bearer
/// and `trace` as its W3C `traceparent`.
///
/// # Errors
///
/// Returns `bearer` or `traceparent` when either is not a valid metadata
/// value.
fn authorized<T>(
    body: T,
    token: &str,
    trace: &TraceContext,
) -> std::result::Result<Request<T>, String> {
    let mut request = Request::new(body);
    let metadata = request.metadata_mut();
    metadata.insert(
        "x-wyrd-access-token",
        token.parse().map_err(|_| "bearer".to_owned())?,
    );
    metadata.insert(
        "traceparent",
        trace
            .to_traceparent()
            .parse()
            .map_err(|_| "traceparent".to_owned())?,
    );
    Ok(request)
}

#[cfg(test)]
mod tests {
    use super::{Outcome, accepted};

    /// Merged client outcomes keep every sample, row, wire byte, and error
    /// code, and an export that dropped rows is a `partial-success` failure.
    ///
    /// # Panics
    ///
    /// Panics when a merge loses evidence or a partial export is accepted.
    #[test]
    fn outcomes_merge_and_partial_exports_fail() {
        let mut total = Outcome {
            latencies_us: vec![1, 2],
            errors: [("wrong-result".to_owned(), 1)].into(),
            rows: 10,
            wire_bytes: 100,
        };
        total.merge(Outcome {
            latencies_us: vec![3],
            errors: [
                ("wrong-result".to_owned(), 2),
                ("Unavailable".to_owned(), 1),
            ]
            .into(),
            rows: 5,
            wire_bytes: 50,
        });
        assert_eq!(total.latencies_us, [1, 2, 3]);
        assert_eq!(
            (total.rows, total.wire_bytes, total.error_count()),
            (15, 150, 4)
        );
        assert_eq!(accepted(64, 0), Ok(64));
        assert_eq!(accepted(64, 1), Err("partial-success".to_owned()));
    }
}
