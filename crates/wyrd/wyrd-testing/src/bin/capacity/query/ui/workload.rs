//! The `ui-sql` workload: the frozen original UI statements, run unchanged
//! over the deterministic journey in each residence.
//!
//! [`Workload::prepare`] loads the journey through OTLP, reconciles every
//! table's rows, resolves each statement's binds against the loaded rows and
//! records each typed answer as the reference. Every later residence must
//! answer every statement identically before its first window; every timed
//! query is checked against its reference.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use num_traits::ToPrimitive;
use tokio::task::JoinSet;
use wyrd_client::Bifrost;
use wyrd_spec::vala::api::{BifrostQueryRequest, QueryParam, QueryTerminalOutcome};
use wyrd_testing::capacity::{Expected, Population, Window};
use wyrd_testing::experiment::{ExperimentError, Phase, Reading, Repetition, Step, Workload};
use wyrd_testing::release_server::{LocalServer, PEAK_SAMPLE_INTERVAL_MS};

use super::answer::Answer;
use super::fixture::Journey;
use super::{CASE, Catalog, RESIDENCES, Statement, param};
use crate::query::isolated::SUCCESS;
use crate::query::run::{self, Admission, Outcome, Sender, WRONG_RESULT, micros};
use crate::query::{HOT_ENV, Residence, client, platform, start};

/// Concurrent exporters loading the journey.
const LOADERS: usize = 16;

/// How long the journey may take to publish once asked.
const SETTLE_TIMEOUT: Duration = Duration::from_secs(300);

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
    /// The running server, once prepared.
    server: Option<LocalServer>,
    /// The public query handle, once prepared.
    queries: Option<Arc<Bifrost>>,
    /// Every statement, bound and answered, once prepared.
    bound: Arc<Vec<Bound>>,
    /// The residence the rows are in.
    placed: &'static str,
    /// Milliseconds of the first statement after the last placement.
    first_ms: f64,
}

impl Ui {
    /// An unprepared workload of `journey` keeping its server log under
    /// `logs`.
    pub fn new(logs: PathBuf, journey: Journey) -> Self {
        Self {
            logs,
            journey,
            server: None,
            queries: None,
            bound: Arc::new(Vec::new()),
            placed: RESIDENCES[0],
            first_ms: 0.0,
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
    /// one, a failed control, rows left behind, or any differing answer.
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
            Residence::settle(server, SETTLE_TIMEOUT)
                .await
                .map_err(ExperimentError::workload)?;
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
            answer.matches(&bound.expected, &[]).map_err(|difference| {
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

    /// The statements one step runs: a page's family, or one query.
    fn selected(&self, step: &Step) -> Vec<Bound> {
        let page = step.text("page");
        let query = step.text("query");
        self.bound
            .iter()
            .filter(|bound| {
                page.is_some_and(|page| bound.statement.family == page)
                    || query.is_some_and(|query| bound.statement.id == query)
            })
            .cloned()
            .collect()
    }
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

/// Runs `bundle` once as its page does: the independent statements
/// concurrently, then those taking their results, each checked against its
/// reference, each in its own task under `admission`'s labels, which a
/// spawned task does not inherit; counts every statement the server
/// answered in `answered`.
///
/// # Errors
///
/// Returns the first failure's code, or [`WRONG_RESULT`].
async fn bundle(
    queries: &Arc<Bifrost>,
    bundle: &Arc<Vec<Bound>>,
    answered: &AtomicU64,
    admission: Admission,
) -> Result<(), String> {
    for dependent in [false, true] {
        let mut asked = JoinSet::new();
        for (index, bound) in bundle.iter().enumerate() {
            if bound.statement.depends_on.is_empty() == dependent {
                continue;
            }
            let (queries, bundle) = (Arc::clone(queries), Arc::clone(bundle));
            asked.spawn(async move {
                let bound = &bundle[index];
                admission
                    .labelled(ask(&queries, &bound.statement.sql, &bound.params))
                    .await
                    .map(|(answer, _)| answer.matches(&bound.expected, &[]).is_ok())
            });
        }
        let mut failure = None;
        while let Some(joined) = asked.join_next().await {
            let outcome = joined.unwrap_or_else(|_| Err("panicked".to_owned()));
            if outcome.is_ok() {
                answered.fetch_add(1, Ordering::Relaxed);
            }
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

/// The resolved binds of `statement`: its catalog binds, with every
/// dependent bind taken from the answers already recorded in `bound`.
///
/// # Errors
///
/// Returns a message for an unrepresentable bind or a missing dependency
/// answer.
fn resolve(statement: &Statement, bound: &[Bound]) -> Result<Vec<QueryParam>, String> {
    let mut params = statement
        .params
        .iter()
        .map(param)
        .collect::<Result<Vec<_>, _>>()?;
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
        for (table, rows) in [
            ("bifrost.traces.spans", self.journey.span_rows()),
            ("bifrost.logs.records", self.journey.log_rows()),
            ("bifrost.metrics.points", self.journey.point_rows()),
        ] {
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
        let catalog = Catalog::frozen().map_err(ExperimentError::workload)?;
        let mut bound: Vec<Bound> = Vec::new();
        let mut failed = Vec::new();
        for statement in catalog.queries {
            let params = resolve(&statement, &bound).map_err(ExperimentError::workload)?;
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
            bound.push(Bound {
                statement,
                params,
                expected,
            });
        }
        if !failed.is_empty() {
            return Err(ExperimentError::workload(format!(
                "every catalog statement must answer; failed: {}",
                failed.join(", ")
            )));
        }
        self.bound = Arc::new(bound);
        self.queries = Some(queries);
        Ok(())
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

    /// Runs one closed-loop window of `step`'s page or query in its
    /// residence, placing the rows there first, outside every window, when
    /// the residence changes.
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
        let selected = Arc::new(self.selected(step));
        if selected.is_empty() {
            return Err(ExperimentError::workload(format!(
                "step {} selects no statement",
                step.id
            )));
        }
        let (server, queries) = self.prepared()?;
        let timings = server.timings().await.map_err(ExperimentError::workload)?;
        let (opened, _) = Window::open(server)
            .await
            .map_err(ExperimentError::workload)?;
        let stop = Instant::now() + window;
        let admission = Admission::measured(stop, (CASE, phase.label()));
        let answered = Arc::new(AtomicU64::new(0));
        let work = async {
            let mut tasks = JoinSet::new();
            for _ in 0..clients {
                let (queries, selected, answered) = (
                    Arc::clone(&queries),
                    Arc::clone(&selected),
                    Arc::clone(&answered),
                );
                tasks.spawn(async move {
                    let mut outcome = Outcome::default();
                    while admission.open() {
                        let sent = Instant::now();
                        match bundle(&queries, &selected, &answered, admission).await {
                            Ok(()) => outcome.succeeded(&admission, sent, 0, 0),
                            Err(code) => *outcome.errors.entry(code).or_default() += 1,
                        }
                    }
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
        repetition.set(
            "queries",
            Reading::count(pages * selected.len().to_u64().unwrap_or(0)),
        );
        for (id, q) in [
            ("client_p50_ms", 0.5),
            ("client_p95_ms", 0.95),
            ("client_p99_ms", 0.99),
        ] {
            repetition.set(id, Reading::percentile(&outcome.latencies_us, q, 1e3));
        }
        repetition.set("first_request_ms", Reading::exact(self.first_ms));
        let [rows, bytes] = population.scanned_per_query(SUCCESS);
        repetition.set("scanned_rows_per_query", rows);
        repetition.set("scanned_bytes_per_query", bytes);
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
