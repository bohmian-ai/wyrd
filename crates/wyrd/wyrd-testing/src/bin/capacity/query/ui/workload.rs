//! The `ui-sql` workload: the frozen original UI statements, run unchanged
//! over the deterministic journey in each residence.
//!
//! [`Workload::prepare`] loads the journey through OTLP, reconciles every
//! table's rows, resolves each statement's binds against the loaded rows and
//! records each typed answer as the reference. Every later residence must
//! answer every statement identically before its first window; every timed
//! query is checked against its reference.

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::task::JoinSet;
use wyrd_benchmark::Distribution;
use wyrd_client::Bifrost;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryParam, QueryTerminalOutcome};
use wyrd_testing::capacity::{Expected, Population, Window};
use wyrd_testing::experiment::{
    DIAGNOSTIC, ExperimentError, Phase, Reading, Repetition, Step, Validity, Workload,
};
use wyrd_testing::release_server::{LocalServer, PEAK_SAMPLE_INTERVAL_MS};

use super::Arm;
use super::answer::Answer;
use super::fixture::Journey;
use super::{CASE, Catalog, PAGES, RESIDENCES, Statement, observation, param, seed};
use crate::query::isolated::SUCCESS;
use crate::query::run::{self, Admission, Outcome, Sender, WRONG_RESULT, micros};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

use crate::query::telemetry::Telemetry;
use crate::query::{
    HOT_ENV, WRITE_FIXTURE_ROWS, client, learn_span_layout, platform, start, table_files,
};

/// How long seeded calls and verifications may take to become visible.
const VISIBLE_WAIT: Duration = Duration::from_secs(60);

/// Spans of the sample that teaches the learned arm's span layout.
const LEARNING_SPANS: i64 = 10_000;

/// Concurrent exporters loading the journey.
const LOADERS: usize = 16;

/// One catalog statement with its resolved, frozen binds and its reference
/// answer.
#[derive(Debug, Clone)]
pub struct Bound {
    /// The catalog statement.
    pub statement: Statement,
    /// The frozen binds.
    pub params: Vec<QueryParam>,
    /// The typed answer every residence must return.
    pub expected: Answer,
}

/// The `ui-sql` server, journey and frozen statements.
pub struct Ui {
    /// Where the server log is kept.
    logs: PathBuf,
    /// The journey loaded.
    journey: Journey,
    /// Whether the server learns a span layout before the journey loads.
    learned: bool,
    /// The running server, once prepared.
    server: Option<LocalServer>,
    /// The public query handle, once prepared.
    queries: Option<Arc<Bifrost>>,
    /// Every statement, bound and answered, once prepared.
    bound: Arc<Vec<Bound>>,
    /// Each of [`PAGES`]' statements, as indices into `bound`.
    pages: Arc<[Vec<usize>; 4]>,
    /// The residence the rows are in.
    placed: &'static str,
    /// Milliseconds of the first statement after the last placement.
    first_ms: f64,
    /// Spans held once seeding's gateway and verification spans joined the
    /// journey's.
    spans: i64,
}

impl Ui {
    /// An unprepared workload of `arm` keeping its server log under
    /// `logs`.
    pub fn new(logs: PathBuf, arm: Arm) -> Self {
        let journey = arm.journey;
        Self {
            logs,
            journey,
            learned: arm.learned,
            server: None,
            queries: None,
            bound: Arc::new(Vec::new()),
            pages: Arc::default(),
            placed: RESIDENCES[0],
            first_ms: 0.0,
            spans: journey.span_rows(),
        }
    }

    /// The prepared server and query handle.
    ///
    /// # Errors
    ///
    /// Returns a workload error before [`Workload::prepare`].
    fn prepared(&self) -> Result<(&LocalServer, Arc<Bifrost>), ExperimentError> {
        match (&self.server, &self.queries) {
            (Some(server), Some(queries)) => Ok((server, Arc::clone(queries))),
            _ => Err(ExperimentError::workload("the workload was not prepared")),
        }
    }

    /// Moves every row to `residence` through the benchmark control, checks
    /// it arrived, and requires every statement to answer exactly as before.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a residence earlier than the current
    /// one, a failed control, a stage moving fewer rows than the journey
    /// holds, a publish of no claims, or any differing answer.
    async fn place(&mut self, residence: &'static str) -> Result<(), ExperimentError> {
        let order = |name: &str| RESIDENCES.iter().position(|known| *known == name);
        if order(residence) < order(self.placed) {
            return Err(ExperimentError::workload(format!(
                "rows in {} cannot return to {residence}",
                self.placed
            )));
        }
        let (server, queries) = self.prepared()?;
        let platform = platform(server).await.map_err(ExperimentError::workload)?;
        if residence == "staged" {
            let staged = platform
                .stage_scribe()
                .await
                .map_err(ExperimentError::workload)?;
            let rows =
                self.journey.span_rows() + self.journey.log_rows() + self.journey.point_rows();
            if staged < rows.cast_unsigned() {
                return Err(ExperimentError::workload(format!(
                    "staging moved {staged} of the journey's {rows} rows"
                )));
            }
        } else if residence == "published" {
            platform
                .publish_scribe()
                .await
                .map_err(ExperimentError::workload)?;
            for (table, rows) in self.tables() {
                let published = published_rows(server, table).map_err(ExperimentError::workload)?;
                if published != rows {
                    return Err(ExperimentError::workload(format!(
                        "publishing left {published} of the journey's {rows} {table} rows in Parquet"
                    )));
                }
            }
        }
        let sent = Instant::now();
        let mut first = None;
        for bound in self.bound.iter() {
            let (answer, _) = ask(&queries, &bound.statement.sql, &bound.params)
                .await
                .map_err(|code| {
                    ExperimentError::workload(format!(
                        "{} in {residence} failed with {code}",
                        bound.statement.id
                    ))
                })?;
            first.get_or_insert_with(|| sent.elapsed().as_secs_f64() * 1e3);
            answer
                .matches(&bound.expected, bound.statement.order())
                .map_err(|difference| {
                    ExperimentError::workload(format!(
                        "{} in {residence} differs from memtable: {difference}",
                        bound.statement.id
                    ))
                })?;
        }
        self.first_ms = first.unwrap_or_default();
        self.placed = residence;
        Ok(())
    }

    /// Seeds the gateway calls and verifications ([`seed::seed`]), waits
    /// until every one is visible to `queries`, then counts the spans the
    /// journey and seeding hold together, returning the seeded window.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a seeding failure, rows not all visible
    /// within [`VISIBLE_WAIT`], or fewer spans than the journey and one
    /// attempt per call.
    async fn seed(
        &mut self,
        client: &wyrd_client::WyrdClient,
        queries: &Arc<Bifrost>,
    ) -> Result<[QueryParam; 2], ExperimentError> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the server is not started"))?;
        let [setup] = server.tenants() else {
            return Err(ExperimentError::workload("setup provisioned one tenant"));
        };
        let window = seed::seed(setup, client, &self.logs)
            .await
            .map_err(ExperimentError::workload)?;
        let count = async |table: &str| {
            let (answer, _) = ask(queries, &format!("SELECT count(*) AS n FROM {table}"), &[])
                .await
                .map_err(ExperimentError::workload)?;
            Ok::<_, ExperimentError>(
                answer
                    .column("n")
                    .first()
                    .copied()
                    .flatten()
                    .and_then(|n| n.parse::<i64>().ok())
                    .unwrap_or_default(),
            )
        };
        let deadline = Instant::now() + VISIBLE_WAIT;
        loop {
            let calls = count("bifrost.gateway.calls").await?;
            let results = count("bifrost.verification.results").await?;
            if calls == i64::from(seed::CALLS)
                && results == i64::from(seed::DIRECT + seed::SUBMITTED)
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err(ExperimentError::workload(format!(
                    "after {VISIBLE_WAIT:?} {calls} of {} calls and {results} of {} results are visible",
                    seed::CALLS,
                    seed::DIRECT + seed::SUBMITTED
                )));
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        self.spans = count("bifrost.traces.spans").await?;
        let floor = self.journey.span_rows() + i64::from(seed::CALLS);
        if self.spans < floor {
            return Err(ExperimentError::workload(format!(
                "{} spans after seeding, under the journey's and one per call {floor}",
                self.spans
            )));
        }
        Ok(window)
    }

    /// Loads [`LEARNING_SPANS`] spans of the `hot` sample, dated far
    /// outside the journey's hour and with their own trace ids, through
    /// `otlp` and publishes them so the server learns its span layout
    /// before the journey loads; the span count then includes them.
    ///
    /// # Errors
    ///
    /// Returns a workload error for a short or failed load, or a publish
    /// that taught no layout.
    async fn learn(&mut self, otlp: &run::Otlp) -> Result<(), ExperimentError> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| ExperimentError::workload("the server is not started"))?;
        let sample = Sender::Spans(
            otlp.clone(),
            Telemetry {
                spans: WRITE_FIXTURE_ROWS,
                wide: false,
            },
        );
        let loaded = sample
            .closed_loop(
                &Arc::new(AtomicI64::new(WRITE_FIXTURE_ROWS)),
                WRITE_FIXTURE_ROWS + LEARNING_SPANS,
                LOADERS,
                Admission::default(),
            )
            .await;
        if loaded.error_count() > 0 || loaded.rows != LEARNING_SPANS.cast_unsigned() {
            return Err(ExperimentError::workload(format!(
                "the layout sample loaded {} of {LEARNING_SPANS} spans with errors {:?}",
                loaded.rows, loaded.errors
            )));
        }
        learn_span_layout(server)
            .await
            .map_err(ExperimentError::workload)?;
        self.spans += LEARNING_SPANS;
        Ok(())
    }

    /// Each table the journey writes and the rows it holds there.
    fn tables(&self) -> [(&'static str, i64); 3] {
        [
            ("bifrost.traces.spans", self.spans),
            ("bifrost.logs.records", self.journey.log_rows()),
            ("bifrost.metrics.points", self.journey.point_rows()),
        ]
    }
}

/// The rows in `table`'s published Parquet files under `server`'s store,
/// from each file's footer.
///
/// # Errors
///
/// Returns the first file that cannot be opened or whose footer cannot be
/// read.
fn published_rows(server: &LocalServer, table: &str) -> Result<i64, String> {
    table_files(&server.storage_dir(), table)
        .iter()
        .try_fold(0, |rows, path| {
            let file = std::fs::File::open(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let builder = ParquetRecordBatchReaderBuilder::try_new(file)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            Ok(rows + builder.metadata().file_metadata().num_rows())
        })
}

/// Sends one statement with `params`, reads it to its terminal, and returns
/// its typed answer and the microseconds until its first batch, zero when it
/// returned none.
///
/// # Errors
///
/// Returns the stable error code, the failed or degraded terminal's
/// outcome, a terminal warning, `no-terminal`, or `no-schema`.
pub async fn ask(
    queries: &Bifrost,
    sql: &str,
    params: &[QueryParam],
) -> Result<(Answer, u64), String> {
    let request = BifrostQueryRequest {
        sql: sql.to_owned(),
        params: params.to_vec(),
        deadline_ms: None,
    };
    let sent = Instant::now();
    let mut stream = queries
        .query(&request)
        .await
        .map_err(|error| run::code(&error))?;
    let mut batches = Vec::new();
    let mut first = 0;
    while let Some(batch) = stream
        .next_batch()
        .await
        .map_err(|error| run::code(&error))?
    {
        if batches.is_empty() {
            first = micros(sent);
        }
        batches.push(batch);
    }
    match stream.terminal() {
        Some(terminal) if terminal.outcome != QueryTerminalOutcome::Success => {
            return Err(format!("{:?}", terminal.outcome));
        }
        Some(terminal) if !terminal.warnings.is_empty() => {
            return Err(format!("warning {:?}", terminal.warnings));
        }
        Some(_) => {}
        None => return Err("no-terminal".to_owned()),
    }
    let schema = stream.schema().cloned().ok_or("no-schema")?;
    let answer = Answer::from_batches(&schema, &batches).map_err(|error| error.to_string())?;
    Ok((answer, first))
}

/// Runs `page`, indices into `bound`, once as the page does: the
/// independent statements concurrently, then those taking their results,
/// each checked against its reference, each in its own task under
/// `admission`'s labels, which a spawned task does not inherit. Counts every
/// statement the server answered in `answered` and adds each one's
/// nanoseconds from sent to terminal to its entry of `statements`.
///
/// # Errors
///
/// Returns the first failure's code, or [`WRONG_RESULT`].
async fn bundle(
    queries: &Arc<Bifrost>,
    bound: &Arc<Vec<Bound>>,
    page: &[usize],
    answered: &AtomicU64,
    admission: Admission,
    statements: &mut [Vec<u64>],
) -> Result<(), String> {
    for dependent in [false, true] {
        let mut asked = JoinSet::new();
        for &index in page {
            if bound[index].statement.depends_on.is_empty() == dependent {
                continue;
            }
            let (queries, bound) = (Arc::clone(queries), Arc::clone(bound));
            asked.spawn(async move {
                let bound = &bound[index];
                let sent = Instant::now();
                let outcome = admission
                    .labelled(ask(&queries, &bound.statement.sql, &bound.params))
                    .await
                    .map(|(answer, _)| {
                        answer
                            .matches(&bound.expected, bound.statement.order())
                            .is_ok()
                    });
                (index, nanos(sent), outcome)
            });
        }
        let mut failure = None;
        while let Some(joined) = asked.join_next().await {
            let outcome = match joined {
                Ok((index, elapsed, outcome)) => {
                    if outcome.is_ok() {
                        answered.fetch_add(1, Ordering::Relaxed);
                        statements[index].push(elapsed);
                    }
                    outcome
                }
                Err(_) => Err("panicked".to_owned()),
            };
            match outcome {
                Ok(true) => {}
                Ok(false) => {
                    failure.get_or_insert_with(|| WRONG_RESULT.to_owned());
                }
                Err(code) => {
                    failure.get_or_insert(code);
                }
            }
        }
        if let Some(failure) = failure {
            return Err(failure);
        }
    }
    Ok(())
}

/// Nanoseconds since `sent`.
fn nanos(sent: Instant) -> u64 {
    u64::try_from(sent.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

/// Client nanoseconds of one step's pages and statements, every client's
/// merged.
#[derive(Debug, Default)]
struct Latencies {
    /// Each of [`PAGES`]' complete pages.
    pages: [Vec<u64>; 4],
    /// Each bound statement's answers, by its index.
    statements: Vec<Vec<u64>>,
}

impl Latencies {
    /// Adds one client's samples.
    fn merge(&mut self, other: Self) {
        for (all, more) in self.pages.iter_mut().zip(other.pages) {
            all.extend(more);
        }
        self.statements
            .resize_with(other.statements.len(), Vec::new);
        for (all, more) in self.statements.iter_mut().zip(other.statements) {
            all.extend(more);
        }
    }
}

/// Sets diagnostic `observation`'s readings from `samples` nanoseconds,
/// with each percentile whose samples are under `validity`'s floor for it
/// replaced by an inconclusive reading that names the shortfall.
fn diagnose(repetition: &mut Repetition, observation: &str, samples: &[u64], validity: &Validity) {
    repetition.diagnose(observation, Distribution::of_durations(samples), false);
    let count = samples.len() as u64;
    for (stat, floor) in [
        ("p95_us", validity.p95_samples),
        ("p99_us", validity.p99_samples),
    ] {
        if count < floor {
            repetition.set(
                &format!("{DIAGNOSTIC}{observation}.{stat}"),
                Reading::missing(format!(
                    "inconclusive: {count} samples, under the {floor}-sample floor"
                )),
            );
        }
    }
}

/// The resolved binds of `statement`: its catalog binds, with every
/// dependent bind taken from the answers already recorded in `bound`.
///
/// # Errors
///
/// Returns a message for an unrepresentable bind or a missing dependency
/// answer.
fn resolve(
    statement: &Statement,
    bound: &[Bound],
    window: &[QueryParam; 2],
) -> Result<Vec<QueryParam>, String> {
    let mut params = statement
        .params
        .iter()
        .map(param)
        .collect::<Result<Vec<_>, _>>()?;
    if seed::seeded(&statement.id) {
        params[..2].clone_from_slice(window);
    }
    let answer = |id: &str| {
        bound
            .iter()
            .find(|bound| bound.statement.id == id)
            .map(|bound| &bound.expected)
            .ok_or_else(|| format!("{} needs {id}", statement.id))
    };
    let cell = |answer: &Answer, row: usize, column: &str| {
        answer
            .column(column)
            .get(row)
            .copied()
            .flatten()
            .map(|cell| QueryParam::String(cell.to_owned()))
            .ok_or_else(|| format!("{} needs row {row} of {column}", statement.id))
    };
    match statement.id.as_str() {
        "trace-counts" => {
            let list = answer("trace-list")?;
            params[2] = cell(list, 0, "trace_id")?;
            params[3] = cell(list, 1, "trace_id")?;
        }
        "trace-page-after" | "trace-page-before" => {
            let list = answer("trace-list")?;
            let row = list.rows.len().saturating_sub(1).min(49);
            params[2] = cell(list, row, "start_time_unix_nano")?;
            params[3] = cell(list, row, "trace_id")?;
            params[4] = cell(list, row, "span_id")?;
        }
        "trace-waterfall" => params[0] = cell(answer("trace-list")?, 0, "trace_id")?,
        "span-detail" => {
            let list = answer("trace-list")?;
            params[0] = cell(list, 0, "trace_id")?;
            params[1] = cell(list, 0, "span_id")?;
        }
        "log-detail" => {
            let list = answer("logs-list")?;
            params[0] = cell(list, 0, "time_unix_nano")?;
            params[1] = cell(list, 0, "observed_time_unix_nano")?;
            params[2] = cell(list, 0, "wyrd_request_id")?;
        }
        "metrics-labels" => params[3] = cell(answer("metrics-apart-chart")?, 0, "series")?,
        _ => {}
    }
    Ok(params)
}

#[async_trait]
impl Workload for Ui {
    /// Starts the server under the `hot` settings, exports the journey,
    /// reconciles each table's rows, then binds and answers every catalog
    /// statement in catalog order, keeping each answer as its reference.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an uncreatable log directory, a server,
    /// export or reconciliation failure, or any catalog statement that
    /// fails; every statement is still tried first, so the error names all
    /// of them.
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        std::fs::create_dir_all(&self.logs).map_err(ExperimentError::workload)?;
        let server = start(&HOT_ENV).await.map_err(ExperimentError::workload)?;
        let client = client(&server).map_err(ExperimentError::workload);
        self.server = Some(server);
        let client = client?;
        let otlp = run::Otlp::connect(&client)
            .await
            .map_err(ExperimentError::workload)?;
        if self.learned {
            self.learn(&otlp).await?;
        }
        let exports = self.journey.exports();
        let loaded = Sender::Ui(otlp, self.journey)
            .closed_loop(
                &Arc::new(AtomicI64::new(0)),
                exports,
                LOADERS,
                Admission::default(),
            )
            .await;
        if loaded.error_count() > 0 || loaded.rows != exports.cast_unsigned() {
            return Err(ExperimentError::workload(format!(
                "the journey exported {} of {exports} requests with errors {:?}",
                loaded.rows, loaded.errors
            )));
        }
        let queries = Arc::new(Bifrost::query_only(&client));
        for (table, rows) in self.tables() {
            let (counted, _) = ask(&queries, &format!("SELECT count(*) AS n FROM {table}"), &[])
                .await
                .map_err(ExperimentError::workload)?;
            let counted = counted
                .column("n")
                .first()
                .copied()
                .flatten()
                .map(str::to_owned);
            if counted != Some(rows.to_string()) {
                return Err(ExperimentError::workload(format!(
                    "{table} holds {counted:?} rows, the journey exported {rows}"
                )));
            }
        }
        let window = self.seed(&client, &queries).await?;
        let catalog = Catalog::frozen().map_err(ExperimentError::workload)?;
        let mut bound: Vec<Bound> = Vec::new();
        let mut failed = Vec::new();
        for statement in catalog.queries {
            let params = resolve(&statement, &bound, &window).map_err(ExperimentError::workload)?;
            let expected = match ask(&queries, &statement.sql, &params).await {
                Ok((answer, _)) => answer,
                Err(code) => {
                    tracing::warn!(id = %statement.id, %code, "ui-sql reference failed");
                    failed.push(format!("{} ({code})", statement.id));
                    continue;
                }
            };
            tracing::info!(
                id = %statement.id,
                rows = expected.rows.len(),
                columns = ?expected.columns.iter().map(|column| format!("{}:{}", column.name, column.data_type)).collect::<Vec<_>>(),
                first = ?expected.rows.first(),
                "ui-sql reference"
            );
            if let Err(problem) = seed::check(&statement.id, &expected, self.journey) {
                failed.push(problem);
            }
            bound.push(Bound {
                statement,
                params,
                expected,
            });
        }
        if !failed.is_empty() {
            return Err(ExperimentError::workload(format!(
                "every catalog statement must answer what the fixture holds; failed: {}",
                failed.join(", ")
            )));
        }
        self.pages = Arc::new(PAGES.map(|page| {
            (0..bound.len())
                .filter(|index| bound[*index].statement.family == page)
                .collect()
        }));
        self.bound = Arc::new(bound);
        self.queries = Some(queries);
        Ok(())
    }
    /// Reads the server's outstanding publication and maintenance work
    /// ([`LocalServer::outstanding`]) before teardown; none before the
    /// server started.
    async fn outstanding(
        &mut self,
    ) -> std::result::Result<Option<serde_json::Value>, ExperimentError> {
        match &self.server {
            Some(server) => server
                .outstanding()
                .await
                .map(Some)
                .map_err(ExperimentError::workload),
            None => Ok(None),
        }
    }

    /// The server's PID, once it runs.
    fn processes(&self) -> Vec<u32> {
        self.server
            .as_ref()
            .and_then(LocalServer::pid)
            .into_iter()
            .collect()
    }

    fn heap_profile_url(&self) -> Option<String> {
        self.server.as_ref().map(LocalServer::heap_profile_url)
    }

    /// Runs one closed-loop window of `step`'s clients in its residence,
    /// each cycling through every page from its own offset, placing the
    /// rows there first, outside every window, when the residence changes;
    /// reads the pooled server and client latency and each page's and page
    /// statement's client latency.
    ///
    /// # Errors
    ///
    /// Returns a workload error for an unprepared workload, an undeclared
    /// step, a failed placement, or a scrape failure.
    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let residence = step
            .text("residence")
            .and_then(|name| RESIDENCES.into_iter().find(|known| *known == name))
            .ok_or_else(|| {
                ExperimentError::workload(format!("step {} names no residence", step.id))
            })?;
        if residence != self.placed {
            self.place(residence).await?;
        }
        let clients = step
            .number("clients")
            .and_then(|clients| usize::try_from(clients).ok())
            .ok_or_else(|| {
                ExperimentError::workload(format!("step {} declares no clients", step.id))
            })?;
        let (server, queries) = self.prepared()?;
        let timings = server.timings().await.map_err(ExperimentError::workload)?;
        let (opened, _) = Window::open(server)
            .await
            .map_err(ExperimentError::workload)?;
        let stop = Instant::now() + window;
        let admission = Admission::measured(stop, (CASE, phase.label()));
        let answered = Arc::new(AtomicU64::new(0));
        let latencies = Arc::new(Mutex::new(Latencies::default()));
        let work = async {
            let mut tasks = JoinSet::new();
            for client in 0..clients {
                let (queries, bound, pages, answered, latencies) = (
                    Arc::clone(&queries),
                    Arc::clone(&self.bound),
                    Arc::clone(&self.pages),
                    Arc::clone(&answered),
                    Arc::clone(&latencies),
                );
                tasks.spawn(async move {
                    let mut outcome = Outcome::default();
                    let mut own = Latencies {
                        statements: vec![Vec::new(); bound.len()],
                        ..Latencies::default()
                    };
                    for page in (client..).map(|turn| turn % PAGES.len()) {
                        if !admission.open() {
                            break;
                        }
                        let sent = Instant::now();
                        match bundle(
                            &queries,
                            &bound,
                            &pages[page],
                            &answered,
                            admission,
                            &mut own.statements,
                        )
                        .await
                        {
                            Ok(()) => {
                                own.pages[page].push(nanos(sent));
                                outcome.succeeded(&admission, sent, 0, 0);
                            }
                            Err(code) => *outcome.errors.entry(code).or_default() += 1,
                        }
                    }
                    latencies
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .merge(own);
                    outcome
                });
            }
            run::join(tasks).await
        };
        let (outcome, usage, _) = opened
            .measure(server, stop, work)
            .await
            .map_err(ExperimentError::workload)?;
        let mut repetition = Repetition {
            seconds: usage.seconds,
            processes: vec!["wyrd-server".to_owned()],
            server_cpu_seconds: usage.server_cpu_seconds,
            peak_memory_bytes: usage.peak_memory_bytes,
            opening_memory_bytes: usage.opening_memory_bytes,
            memory_interval_ms: PEAK_SAMPLE_INTERVAL_MS,
            driver_cpu_seconds: usage.driver_cpu_seconds,
            ..Repetition::default()
        };
        let cohort = Expected {
            boundary: "bifrost.query",
            outcome: SUCCESS,
            case: CASE,
            phase: phase.label(),
            count: answered.load(Ordering::Relaxed),
        };
        let population = Population::collect(server, &timings, &[cohort]).await;
        population.validate(&mut repetition, "measured");
        population.percentiles(
            &mut repetition,
            "bifrost.query",
            SUCCESS,
            false,
            &[("p50_ms", 0.5), ("p95_ms", 0.95), ("p99_ms", 0.99)],
        );
        repetition.set(
            "server_failures",
            Reading::count(population.others("bifrost.query", SUCCESS)),
        );
        let wrong = outcome.errors.get(WRONG_RESULT).copied().unwrap_or(0);
        let pages = outcome.measured();
        repetition.set("requests", Reading::count(pages));
        repetition.set("late_requests", Reading::count(outcome.late));
        repetition.set("errors", Reading::count(outcome.error_count()));
        repetition.set("wrong_results", Reading::count(wrong));
        repetition.set("queries", Reading::count(answered.load(Ordering::Relaxed)));
        for (id, q) in [
            ("client_p50_ms", 0.5),
            ("client_p95_ms", 0.95),
            ("client_p99_ms", 0.99),
        ] {
            repetition.set(id, Reading::percentile(&outcome.latencies_us, q, 1e3));
        }
        repetition.set("first_request_ms", Reading::exact(self.first_ms));
        let latencies = std::mem::take(
            &mut *latencies
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        let validity = Validity::default();
        for (page, samples) in PAGES.iter().zip(&latencies.pages) {
            diagnose(
                &mut repetition,
                &observation("page", page),
                samples,
                &validity,
            );
        }
        for index in self.pages.iter().flatten() {
            diagnose(
                &mut repetition,
                &observation("statement", &self.bound[*index].statement.id),
                latencies
                    .statements
                    .get(*index)
                    .map_or(&[][..], Vec::as_slice),
                &validity,
            );
        }
        let [rows, bytes] = population.scanned_per_query(SUCCESS);
        repetition.set("scanned_rows_per_query", rows);
        repetition.set("scanned_bytes_per_query", bytes);
        let queries = answered.load(Ordering::Relaxed);
        for (id, counter) in [
            ("memtable_rows_per_query", LIVE_ROWS_EXAMINED),
            ("memtable_bytes_per_query", LIVE_BYTES_EXAMINED),
        ] {
            repetition.set(
                id,
                examined_per_query(population.counter(counter, &[]), queries),
            );
        }
        Ok(repetition)
    }

    /// Stops the server and keeps its log.
    ///
    /// # Errors
    ///
    /// Returns a workload error when the server exits unsuccessfully or its
    /// log cannot be copied.
    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.queries = None;
        let Some(server) = self.server.take() else {
            return Ok(());
        };
        server
            .stop(&self.logs.join("server.log"))
            .await
            .map(|_| ())
            .map_err(ExperimentError::workload)
    }
}

/// Scribe's benchmark count of the captured memtable rows its live reads
/// examined, before any row filter.
pub const LIVE_ROWS_EXAMINED: &str = "bifrost_scribe_live_rows_examined_total";

/// Scribe's benchmark count of the decoded Arrow bytes of the columns its
/// live reads examined in those rows.
pub const LIVE_BYTES_EXAMINED: &str = "bifrost_scribe_live_bytes_examined_total";

/// The mean of one window's `examined` counter movement over its `queries`
/// answered queries, missing when the server never registered the counter or
/// no query was answered: absence is not a zero scan.
fn examined_per_query(examined: Option<u64>, queries: u64) -> Reading {
    match examined {
        _ if queries == 0 => Reading::missing("no query was answered"),
        None => Reading::missing("the server registered no live memtable scan counter"),
        Some(examined) => Reading::exact(examined as f64 / queries as f64).with_samples(queries),
    }
}

#[cfg(test)]
mod tests {
    use wyrd_testing::experiment::{Repetition, Validity};

    /// A page or statement percentile under its sample floor is
    /// inconclusive, never a number, while its count and median stay.
    ///
    /// # Panics
    ///
    /// Panics when a short percentile reads as a value.
    #[test]
    fn short_percentiles_are_inconclusive() {
        let validity = Validity::default();
        let mut repetition = Repetition::default();
        super::diagnose(&mut repetition, "page.logs", &[1_000_000; 199], &validity);
        let reading = |stat: &str| repetition.reading(&format!("diagnostic.page.logs.{stat}"));
        assert_eq!(reading("count").value, Some(199.0));
        assert!(reading("p50_us").value.is_some());
        let p95 = reading("p95_us");
        assert_eq!(p95.value, None);
        assert_eq!(
            p95.reason.as_deref(),
            Some("inconclusive: 199 samples, under the 200-sample floor")
        );
        let mut repetition = Repetition::default();
        super::diagnose(&mut repetition, "page.logs", &[1_000_000; 200], &validity);
        let reading = |stat: &str| repetition.reading(&format!("diagnostic.page.logs.{stat}"));
        assert!(reading("p95_us").value.is_some());
        assert_eq!(
            reading("p99_us").reason.as_deref(),
            Some("inconclusive: 200 samples, under the 1000-sample floor")
        );
    }

    /// A window's examined memtable rows or bytes read per answered query,
    /// and missing, never zero, when the server never registered the counter
    /// or no query was answered.
    ///
    /// # Panics
    ///
    /// Panics when a reading is not the counter's mean per query or an
    /// absent counter or query reads as a number.
    #[test]
    fn memtable_scans_read_per_answered_query() {
        let reading = super::examined_per_query(Some(30), 4);
        assert_eq!(reading.value, Some(7.5));
        assert_eq!(reading.samples, Some(4));
        assert_eq!(super::examined_per_query(Some(0), 4).value, Some(0.0));
        assert_eq!(super::examined_per_query(None, 4).value, None);
        assert_eq!(super::examined_per_query(Some(30), 0).value, None);
    }
}
