//! Real-path proofs that each engine's benchmark measurement belongs to the
//! work it claims: every engine's own plan drives the release server through
//! the shared runner on a real clock, and independent references (the
//! declared window, a known driver spin, a known server scrape loop, the
//! client's own round trips, and the server's own row counter) bound what the
//! record keeps. Each check is a function a deliberately wrong substitution
//! must trip.
//!
//! Needs `mise run bench:build`, the repository Postgres wrapper, and a quiet
//! host; see each test for its command.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use wyrd_testing::experiment::{
    Baselines, Engine, ExperimentError, Level, Options, Phase, Record, Repetition, Status, Step,
    StepRecord, Timing, Verdict, Workload, run,
};
use wyrd_testing::release_server::{HostLoad, METRICS_PORT, Metrics};

use super::{line, repository};
use crate::levels::{self, Context, Plan};
use crate::{gateway, query, verification};

/// Each measured window.
const WINDOW: Duration = Duration::from_secs(3);

/// The window's stop fires on a millisecond timer before the stop scrape
/// starts; 50 ms covers that and scheduling on a quiet host.
const WINDOW_SLACK_SECONDS: f64 = 0.05;

/// The known CPU work the driver adds, on its own thread, in the second
/// repetition.
const SPIN: Duration = Duration::from_secs(1);

/// A thread spinning on a quiet host is charged at least this share of its
/// wall time; the spin starts a few milliseconds before the window opens.
const SPIN_FLOOR: f64 = 0.8;

/// Two 10 ms CPU accounting ticks, one at each end of a reading.
const TICKS_SECONDS: f64 = 0.02;

/// How much one process's CPU per unit of work may differ between two equal
/// one-client windows, as a share of it.
const DRIFT: f64 = 0.25;

/// Least share of a sequential `/metrics` scrape's round trip the server
/// spends rendering it on CPU: the exposition is rendered in memory, and
/// loopback transfer and the driver's read take the rest.
const SCRAPE_FLOOR: f64 = 0.3;

/// Bytes per MiB.
const MIB: f64 = 1_048_576.0;

/// The server's row counter, incremented once a memtable accepts rows of
/// any table.
const ROWS_INSERTED: &str = "bifrost_scribe_memtable_rows_inserted_total";

/// Items staged on the server's Scribe outbox and not yet written, such as
/// the audit row each permission decision stages after its request answered.
const OUTBOX_PENDING: &str = "outbox_pending";

/// How long the Scribe outbox may take to write what a window staged.
const SETTLE: Duration = Duration::from_secs(10);

/// How often a settling scrape polls the outbox.
const SETTLE_POLL: Duration = Duration::from_millis(10);

/// Computes on this thread for `length`.
pub fn burn(length: Duration) {
    let started = std::time::Instant::now();
    let mut state = 0_u64;
    while started.elapsed() < length {
        state = std::hint::black_box(
            state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1),
        );
    }
}

/// Scrapes the server's metrics listener, or `None` when it does not answer.
async fn scrape() -> Option<Metrics> {
    let url = format!("http://127.0.0.1:{METRICS_PORT}/metrics");
    let response = reqwest::get(url).await.ok()?.error_for_status().ok()?;
    Some(Metrics::parse(&response.text().await.ok()?))
}

/// Scrapes once the server's Scribe outbox holds nothing pending, so every
/// row a window's requests staged is in the memtable counters, or `None`
/// when the server does not answer or the outbox does not settle within
/// [`SETTLE`].
async fn settled_scrape() -> Option<Metrics> {
    let deadline = Instant::now() + SETTLE;
    loop {
        let metrics = scrape().await?;
        if metrics.sum(OUTBOX_PENDING, &["outbox=\"scribe\""]) == 0.0 {
            return Some(metrics);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(SETTLE_POLL).await;
    }
}

/// Scrapes `/metrics` one request after another for `length` and returns
/// the seconds the requests were in flight.
async fn scrape_loop(length: Duration) -> f64 {
    let url = format!("http://127.0.0.1:{METRICS_PORT}/metrics");
    let client = reqwest::Client::new();
    let end = Instant::now() + length;
    let mut busy = Duration::ZERO;
    while Instant::now() < end {
        let sent = Instant::now();
        if let Ok(response) = client.get(&url).send().await {
            let _ = response.bytes().await;
        }
        busy += sent.elapsed();
    }
    busy.as_secs_f64()
}

/// What the wrapper saw around one measured call.
#[derive(Debug, Clone)]
struct Observed {
    /// The step measured.
    step: String,
    /// The phase measured.
    phase: Phase,
    /// Rows the server's memtables accepted from just before to just after
    /// the call, when both scrapes answered.
    rows_inserted: Option<f64>,
    /// Seconds the scrape loop kept requests in flight, when it ran.
    scraping: Option<f64>,
}

/// The engine's real workload with the controls of each one-client
/// repetition: none in the first, [`SPIN`] of driver CPU in the second,
/// and a sequential server scrape loop over the window in the third.
struct Controlled {
    /// The engine's production workload.
    inner: Box<dyn Workload>,
    /// Every measured call's observation, in order.
    observed: Arc<Mutex<Vec<Observed>>>,
}

/// Whether `step` runs one client.
fn one_client(step: &str) -> bool {
    step.rsplit(['-', '.']).next() == Some("c1")
}

#[async_trait]
impl Workload for Controlled {
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        self.inner.prepare().await
    }

    fn processes(&self) -> Vec<u32> {
        self.inner.processes()
    }

    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let controlled = one_client(&step.id);
        let before = settled_scrape().await;
        let spin = (controlled && phase == Phase::Repetition(1))
            .then(|| tokio::task::spawn_blocking(|| burn(SPIN)));
        let scraping = (controlled && phase == Phase::Repetition(2))
            .then(|| tokio::spawn(scrape_loop(window)));
        let repetition = self.inner.measure(step, phase, window).await;
        if let Some(spin) = spin {
            spin.await.map_err(ExperimentError::workload)?;
        }
        let scraping = match scraping {
            Some(scraping) => Some(scraping.await.map_err(ExperimentError::workload)?),
            None => None,
        };
        let after = settled_scrape().await;
        let rows_inserted = before
            .zip(after)
            .map(|(before, after)| after.sum(ROWS_INSERTED, &[]) - before.sum(ROWS_INSERTED, &[]));
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Observed {
                step: step.id.clone(),
                phase,
                rows_inserted,
                scraping,
            });
        repetition
    }

    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.inner.finish().await
    }
}

/// One engine run through the shared runner, read back from its published
/// record, with the wrapper's observations.
struct Measured {
    /// The record read back from disk.
    record: Record,
    /// Every measured call's observation.
    observed: Vec<Observed>,
    /// Where the timing evidence was kept.
    artifacts: PathBuf,
}

/// The DBA's per-database setup `with-test-postgres.sh` performs on `wyrd`.
const DATABASE_SETUP: &str =
    include_str!("../../../../../../../scripts/postgres/test-database-setup.sql");

/// Recreates the wrapper's `wyrd` database empty, owned by the platform login
/// and with the DBA's setup, as `with-test-postgres.sh` hands it over, so each
/// release server migrates and serves a fresh database as one CLI run does.
///
/// # Panics
///
/// Panics when the wrapper's admin URL is unset or the database cannot be
/// dropped, created, or set up.
async fn fresh_database() {
    let admin =
        std::env::var("WYRD_TEST_DATABASE_ADMIN_URL").expect("the Postgres wrapper's admin URL");
    let mut maintenance = url::Url::parse(&admin).expect("admin URL");
    maintenance.set_path("/postgres");
    let pool = sqlx::PgPool::connect(maintenance.as_str())
        .await
        .expect("maintenance database");
    for statement in [
        "DROP DATABASE IF EXISTS wyrd WITH (FORCE)",
        "CREATE DATABASE wyrd OWNER wyrd_platform",
    ] {
        sqlx::raw_sql(statement)
            .execute(&pool)
            .await
            .expect(statement);
    }
    pool.close().await;
    let pool = sqlx::PgPool::connect(&admin)
        .await
        .expect("fresh wyrd database");
    sqlx::raw_sql(DATABASE_SETUP)
        .execute(&pool)
        .await
        .expect("database setup");
    pool.close().await;
}

/// Runs `engine`'s smoke-scale plan at `level` from `rest` through the
/// shared runner with the wrapper's controls, three [`WINDOW`]s per L1 step,
/// publishes the record, and reads it back. Each run starts from a
/// [`fresh_database`].
///
/// # Panics
///
/// Panics when the plan cannot be declared, the record does not read back
/// unchanged, or the smoke is not PASS.
async fn measured(
    engine: Engine,
    level: Level,
    rest: &[&str],
    directory: &std::path::Path,
) -> Measured {
    fresh_database().await;
    let root = repository();
    let mut arguments = line(rest);
    arguments.extend(["--smoke".to_owned(), "--allow-dirty".to_owned()]);
    let context = Context {
        level,
        arguments: &arguments,
        root: &root,
    };
    let base = levels::base(engine, &context).expect("base declaration");
    let Plan {
        mut declaration,
        workload,
    } = match engine {
        Engine::Bifrost => query::plan(&context, base),
        Engine::Verification => verification::plan(&context, base),
        Engine::Gateway => gateway::plan(&context, base),
    }
    .expect("plan");
    levels::smoke(&mut declaration);
    if level == Level::One {
        declaration.timing = Timing {
            warmup_seconds: 1,
            repetition_seconds: WINDOW.as_secs(),
            repetitions: 3,
            budget_seconds: 1_200,
            minimum_seconds: 0,
        };
    }
    let observed = Arc::new(Mutex::new(Vec::new()));
    let mut workload = Controlled {
        inner: workload,
        observed: Arc::clone(&observed),
    };
    let artifacts = directory.join(format!("{}-{}", engine.name(), declaration.experiment));
    let options = Options {
        artifacts: artifacts.clone(),
        profiler: None,
        interrupt: CancellationToken::new(),
    };
    let record = run(
        declaration.seal(),
        &mut workload,
        HostLoad::sample,
        &options,
    )
    .await;
    let path = record.publish(directory).expect("publish");
    let read = Record::read(&path).expect("read back");
    assert_eq!(read.render(), record.render());
    assert_eq!(read.verdict, Verdict::Pass, "{}", read.render());
    let observed = observed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    Measured {
        record: read,
        observed,
        artifacts,
    }
}

/// Whether `observed` lies in its reference `[low, high]`, naming `what`
/// when it does not.
fn within(what: &str, observed: f64, low: f64, high: f64) -> Result<(), String> {
    if (low..=high).contains(&observed) {
        Ok(())
    } else {
        Err(format!(
            "{what} read {observed}, outside its reference [{low}, {high}]"
        ))
    }
}

/// A repetition's present value of `id`.
fn value(repetition: &Repetition, id: &str) -> Result<f64, String> {
    repetition
        .reading(id)
        .value
        .ok_or_else(|| format!("`{id}` is missing: {:?}", repetition.reading(id)))
}

/// The engine's names for a window's completed work and its completions
/// after the stop.
const fn work_names(engine: Engine) -> (&'static str, &'static str) {
    match engine {
        Engine::Verification => ("verifications", "verifications_after_window"),
        Engine::Bifrost | Engine::Gateway => ("requests", "late_requests"),
    }
}

/// The window lasted the declared [`WINDOW`] on the real clock.
fn window_matches(repetition: &Repetition) -> Result<(), String> {
    let window = WINDOW.as_secs_f64();
    within(
        "window seconds",
        repetition.seconds,
        window,
        window + WINDOW_SLACK_SECONDS,
    )
}

/// The server's median lies inside the client's own median round trip: each
/// server observation is one client call's server part, so the server's
/// order statistics never exceed the client's.
fn client_bounds_server(repetition: &Repetition) -> Result<(), String> {
    let server = repetition.reading("p50_ms");
    let lower = server
        .above
        .or(server.value)
        .ok_or_else(|| format!("server p50 is missing: {server:?}"))?;
    let client = value(repetition, "client_p50_ms")?;
    within("server p50 lower bound", lower, 0.0, client)
}

/// Every completed call, in the window or after it, is one recorded server
/// observation of its case and phase.
fn population_reconciles(engine: Engine, repetition: &Repetition) -> Result<(), String> {
    let (work, late) = work_names(engine);
    let calls = value(repetition, work)? + value(repetition, late)?;
    let mut boundaries = std::collections::BTreeMap::<&str, u64>::new();
    for cohort in &repetition.reconciliation {
        *boundaries.entry(cohort.boundary.as_str()).or_default() += cohort.recorded;
    }
    if boundaries.is_empty() {
        return Err("no server timing boundary was reconciled".to_owned());
    }
    boundaries.into_iter().try_for_each(|(boundary, recorded)| {
        within(
            &format!("`{boundary}` recorded observations"),
            recorded as f64,
            calls,
            calls,
        )
    })
}

/// The rows an error-free write window acknowledged, in the window and
/// after it, plus the one audit row each authorized request's permission
/// decision stages, are the rows the server's memtables accepted around it
/// once its outbox settled. A failed request still stages its audit row, so
/// a window with errors has no exact reference.
fn rows_match_the_server(repetition: &Repetition, inserted: f64) -> Result<(), String> {
    within("write errors", value(repetition, "errors")?, 0.0, 0.0)?;
    let rows = value(repetition, "rows")?
        + value(repetition, "late_rows")?
        + value(repetition, "requests")?
        + value(repetition, "late_requests")?;
    within("server rows inserted", inserted, rows, rows)
}

/// The spun repetition's driver CPU exceeds the plain one's by the known
/// spin and, where the server's CPU is `proportional` to its work, the
/// server's CPU per unit of work does not absorb it.
fn spin_is_driver_cpu(
    engine: Engine,
    proportional: bool,
    plain: &Repetition,
    spun: &Repetition,
) -> Result<(), String> {
    let driver = |repetition: &Repetition| {
        repetition
            .driver_cpu_seconds
            .ok_or_else(|| "driver CPU is missing".to_owned())
    };
    let spin = SPIN.as_secs_f64();
    within(
        "spin driver CPU",
        driver(spun)? - driver(plain)?,
        spin * SPIN_FLOOR,
        spin + TICKS_SECONDS + DRIFT * driver(plain)?,
    )?;
    if !proportional {
        return Ok(());
    }
    let (work, _) = work_names(engine);
    let per_work = |repetition: &Repetition| -> Result<f64, String> {
        let cpu = repetition
            .server_cpu_seconds
            .ok_or_else(|| "server CPU is missing".to_owned())?;
        Ok(cpu / value(repetition, work)?)
    };
    let plain = per_work(plain)?;
    within(
        "spun server CPU per work",
        per_work(spun)?,
        plain * (1.0 - DRIFT),
        plain * (1.0 + DRIFT),
    )
}

/// The scraped repetition's server CPU beyond what its work costs at the
/// plain rate covers the known share of the scrape loop's round trips, and
/// its driver CPU grows by less than that.
fn scrape_is_server_cpu(
    engine: Engine,
    plain: &Repetition,
    scraped: &Repetition,
    scraping: f64,
) -> Result<(), String> {
    let (work, _) = work_names(engine);
    let extra = |cpu: fn(&Repetition) -> Option<f64>, what: &str| -> Result<f64, String> {
        let at = |repetition: &Repetition| {
            cpu(repetition).ok_or_else(|| format!("{what} CPU is missing"))
        };
        let per_work = at(plain)? / value(plain, work)?;
        Ok(at(scraped)? - per_work * value(scraped, work)?)
    };
    let server = extra(|repetition| repetition.server_cpu_seconds, "server")?;
    let driver = extra(|repetition| repetition.driver_cpu_seconds, "driver")?;
    within(
        "scrape server CPU",
        server,
        SCRAPE_FLOOR * scraping,
        f64::MAX,
    )?;
    within("scrape driver CPU", driver, f64::MIN, server)
}

/// Every check one engine's measured step keeps, over its three controlled
/// repetitions and the wrapper's observations of them. The server-CPU
/// controls compare windows at the plain window's CPU per unit of work, so
/// they run only where the server's CPU is `proportional` to its work.
fn step_holds(
    engine: Engine,
    proportional: bool,
    step: &StepRecord,
    observed: &[Observed],
) -> Result<(), String> {
    let at = |phase: Phase| {
        observed
            .iter()
            .find(|seen| seen.step == step.id && seen.phase == phase)
    };
    for (index, repetition) in step.repetitions.iter().enumerate() {
        let name = |error: String| format!("{} repetition {index}: {error}", step.id);
        window_matches(repetition).map_err(name)?;
        client_bounds_server(repetition).map_err(name)?;
        population_reconciles(engine, repetition).map_err(name)?;
        if repetition.reading("late_rows").is_present() {
            let phase = Phase::Repetition(u8::try_from(index).unwrap_or(u8::MAX));
            let inserted = at(phase)
                .and_then(|seen| seen.rows_inserted)
                .ok_or_else(|| name("no settled row counter was read".to_owned()))?;
            rows_match_the_server(repetition, inserted).map_err(name)?;
            let wire = repetition.reading("wire_bytes");
            if step.id.starts_with("native") {
                if wire.value.is_some() {
                    return Err(name("native writes report OTLP wire bytes".to_owned()));
                }
            } else {
                let wire = value(repetition, "wire_bytes").map_err(name)?;
                let retained = value(repetition, "retained_mib").map_err(name)? * MIB;
                if wire <= 0.0 || retained <= 0.0 || (wire - retained).abs() < 1.0 {
                    return Err(name(format!(
                        "wire bytes {wire} and retained Arrow bytes {retained} are not two sources"
                    )));
                }
            }
        }
    }
    if one_client(&step.id) {
        let [plain, spun, scraped] = step.repetitions.as_slice() else {
            return Err(format!("{}: three repetitions", step.id));
        };
        spin_is_driver_cpu(engine, proportional, plain, spun).map_err(|error| {
            let state = |repetition: &Repetition| {
                [
                    "published_files",
                    "memtable_backlog_mib",
                    "staged_backlog_mib",
                ]
                .map(|name| repetition.reading(name).value)
            };
            format!(
                "{}: {error} (server CPU {:?}, {:?}, {:?} s; published files, memtable and staged backlog MiB {:?}, {:?}, {:?})",
                step.id,
                plain.server_cpu_seconds,
                spun.server_cpu_seconds,
                scraped.server_cpu_seconds,
                state(plain),
                state(spun),
                state(scraped),
            )
        })?;
        let scraping = at(Phase::Repetition(2))
            .and_then(|seen| seen.scraping)
            .ok_or_else(|| format!("{}: the scrape loop did not run", step.id))?;
        if proportional {
            scrape_is_server_cpu(engine, plain, scraped, scraping)
                .map_err(|error| format!("{}: {error}", step.id))?;
        }
    }
    Ok(())
}

/// Each engine's every measured L1 case, through its real plan, server,
/// transport, instrumentation, resource window, conversion, and record,
/// keeps what independent references say: windows last the declared time,
/// the client's round trip bounds the server's time, every call is one
/// recorded server observation at each timing boundary, the server's
/// memtables accepted every acknowledged write row plus one audit row per
/// request, OTLP wire and retained Arrow bytes come from two
/// sources while native writes carry no wire bytes, a known driver spin is
/// driver CPU the server's does not absorb, and a known server scrape loop
/// is server CPU the driver's does not take, the server sides wherever its
/// CPU follows its window's own work. Wrong windows, processes,
/// cases, and boundaries substituted into the same checks trip them.
///
/// `RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::engine_measurements_keep_boundaries_processes_and_windows)'`
/// after `mise run bench:build`, on a quiet host.
///
/// # Panics
///
/// Panics when a reading falls outside its reference or a substitution
/// passes a check.
#[tokio::test]
#[ignore = "needs the release server from `mise run bench:build`, the Postgres wrapper and a quiet host"]
async fn engine_measurements_keep_boundaries_processes_and_windows() {
    let directory = tempfile::tempdir().expect("directory");
    let runs = [
        (Engine::Bifrost, vec!["writes-hot"]),
        (Engine::Bifrost, vec!["writes-parquet"]),
        (Engine::Bifrost, vec!["hot"]),
        (Engine::Bifrost, vec!["parquet"]),
        (Engine::Verification, vec![]),
        (Engine::Gateway, vec![]),
    ];
    let mut failures = Vec::new();
    let mut kept = Vec::new();
    for (engine, rest) in runs {
        let run = measured(engine, Level::One, &rest, directory.path()).await;
        // A writes-parquet window pays for the staging and publication it
        // causes, so its server CPU is not proportional to its own requests;
        // writes-hot proves the same server-process accounting.
        let proportional = run.record.experiment != "writes-parquet";
        // A derived step (a peak or scaling summary) measures no window of
        // its own; its measured inputs carry every check.
        for step in run
            .record
            .steps
            .iter()
            .filter(|step| step.status == Status::Measured)
        {
            match step_holds(engine, proportional, step, &run.observed) {
                Ok(()) => println!("{} {}: holds", run.record.experiment, step.id),
                Err(error) => failures.push(format!("{}: {error}", run.record.experiment)),
            }
            let timings = run
                .artifacts
                .join(format!("timings/{}/repetition-1.json", step.id));
            if !timings.is_file() {
                let kept: Vec<_> = std::fs::read_dir(run.artifacts.join("timings"))
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|entry| entry.file_name())
                    .collect();
                failures.push(format!("{} not kept; kept {kept:?}", timings.display()));
            }
        }
        kept.push((engine, run));
    }
    assert!(failures.is_empty(), "{failures:#?}");

    for (engine, run) in &kept {
        let steps = &run.record.steps;
        let one = steps
            .iter()
            .find(|step| one_client(&step.id))
            .expect("a one-client case");
        let [plain, spun, _] = one.repetitions.as_slice() else {
            panic!("three repetitions");
        };
        let mut wrong = plain.clone();
        wrong.seconds *= 1e3;
        assert!(window_matches(&wrong).is_err(), "a window read in ms");
        let mut swapped = spun.clone();
        std::mem::swap(
            &mut swapped.driver_cpu_seconds,
            &mut swapped.server_cpu_seconds,
        );
        assert!(
            spin_is_driver_cpu(*engine, true, plain, &swapped).is_err(),
            "server and driver processes swapped"
        );
        let mut inner = plain.clone();
        let client = value(plain, "client_p50_ms").expect("client p50");
        inner.set(
            "client_p50_ms",
            wyrd_testing::experiment::Reading::exact(client / 1e3),
        );
        assert!(
            client_bounds_server(&inner).is_err(),
            "a boundary inside the server's"
        );
        let many = steps
            .iter()
            .filter(|step| step.status == Status::Measured && !one_client(&step.id))
            .find_map(|step| step.repetitions.first())
            .expect("a many-client case");
        let mut crossed = many.clone();
        crossed.reconciliation = plain.reconciliation.clone();
        assert!(
            population_reconciles(*engine, &crossed).is_err(),
            "another case's population"
        );
        let scraping = run
            .observed
            .iter()
            .find(|seen| seen.step == one.id && seen.phase == Phase::Repetition(2))
            .and_then(|seen| seen.scraping)
            .expect("scrape loop");
        assert!(
            scrape_is_server_cpu(*engine, plain, plain, scraping).is_err(),
            "a window without the scrape loop"
        );
    }
}

/// Sequential `/metrics` scrape loops the perturbed run keeps over each of
/// its windows: renders on the server's own runtime that compete with the
/// measured requests for its workers and CPU without being its work.
const STORM: usize = 32;

/// The engine's real workload with [`STORM`] scrape loops over every window
/// when `storm` holds, and untouched otherwise.
struct Stormed {
    /// The engine's production workload.
    inner: Box<dyn Workload>,
    /// Whether every window runs the scrape loops.
    storm: bool,
}

#[async_trait]
impl Workload for Stormed {
    async fn prepare(&mut self) -> Result<(), ExperimentError> {
        self.inner.prepare().await
    }

    fn processes(&self) -> Vec<u32> {
        self.inner.processes()
    }

    async fn measure(
        &mut self,
        step: &Step,
        phase: Phase,
        window: Duration,
    ) -> Result<Repetition, ExperimentError> {
        let loops: Vec<_> = (0..if self.storm { STORM } else { 0 })
            .map(|_| tokio::spawn(scrape_loop(window)))
            .collect();
        let repetition = self.inner.measure(step, phase, window).await;
        for scraping in loops {
            scraping.await.map_err(ExperimentError::workload)?;
        }
        repetition
    }

    async fn finish(&mut self) -> Result<(), ExperimentError> {
        self.inner.finish().await
    }
}

/// Runs `engine`'s ordinary full L1 plan from `rest` with `--qualify-baseline`
/// when `qualify` holds, its declaration trimmed to measured step `step`,
/// the criteria judging it narrowed to it, and its references, compared
/// against `baselines` as a CLI run compares against its committed file,
/// through the shared runner from a [`fresh_database`]; publishes the
/// record under `directory` and reads it back with its path.
///
/// # Panics
///
/// Panics when the plan cannot be declared, does not declare `step`, refuses
/// itself, or its record does not read back unchanged.
async fn judged(
    engine: Engine,
    rest: &[&str],
    step: &str,
    qualify: bool,
    baselines: &Baselines,
    storm: bool,
    directory: &std::path::Path,
) -> (String, Record) {
    fresh_database().await;
    let root = repository();
    let mut arguments = line(rest);
    if qualify {
        arguments.push("--qualify-baseline".to_owned());
    }
    let context = Context {
        level: Level::One,
        arguments: &arguments,
        root: &root,
    };
    let base = levels::base(engine, &context).expect("base declaration");
    let Plan {
        mut declaration,
        workload,
    } = match engine {
        Engine::Bifrost => query::plan(&context, base),
        Engine::Verification => verification::plan(&context, base),
        Engine::Gateway => gateway::plan(&context, base),
    }
    .expect("plan");
    assert!(
        declaration.steps.iter().any(|declared| declared.id == step),
        "{step} is not declared"
    );
    declaration.steps.retain(|declared| declared.id == step);
    declaration.criteria.retain_mut(|criterion| {
        criterion.steps.retain(|judged| judged == step);
        !criterion.steps.is_empty()
    });
    declaration
        .references
        .retain(|reference| reference.step == step);
    declaration.compare(baselines);
    assert_eq!(declaration.refusals(), Vec::new(), "{step} refuses itself");
    let mut workload = Stormed {
        inner: workload,
        storm,
    };
    let options = Options {
        artifacts: directory.join(format!(
            "{}-{}-{}",
            engine.name(),
            declaration.experiment,
            if storm { "storm" } else { "quiet" }
        )),
        profiler: None,
        interrupt: CancellationToken::new(),
    };
    let record = run(
        declaration.seal(),
        &mut workload,
        HostLoad::sample,
        &options,
    )
    .await;
    let path = record.publish(directory).expect("publish");
    let read = Record::read(&path).expect("read back");
    assert_eq!(read.render(), record.render());
    (path.display().to_string(), read)
}

/// The judgment of reference `metric` on `step` in `record`, with its
/// verdict.
fn reference_verdict(record: &Record, step: &str, metric: &str) -> Option<Verdict> {
    let id = format!("reference.{}.{step}.{metric}", record.experiment);
    record
        .steps
        .iter()
        .flat_map(|judged| &judged.criteria)
        .find(|judgment| judgment.criterion == id)
        .and_then(|judgment| judgment.verdict)
}

/// Each engine's ordinary full L1 measurement reaches the normal reference
/// judge: two clean `--qualify-baseline` suites of one measured case
/// promote same-method references, an unperturbed normal run against them
/// PASSes every reference, and the same run with [`STORM`] server scrape
/// loops over each window FAILs a reference by the noise-aware regression
/// rule, exit code 1, in a record read back from disk; across the engines
/// latency, server CPU per work, and throughput per vCPU each regress. No reading is edited and
/// no limit changes; the perturbed record stays valid.
///
/// `RUSTFLAGS='-C force-frame-pointers=yes' CARGO_PROFILE_RELEASE_DEBUG=line-tables-only mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked --release --target-dir target/profiling -p wyrd-testing --bin capacity --run-ignored=only --no-capture -E 'test(=tests::pg_tests::measured_regressions_reach_normal_reference_judgment)'`
/// after `mise run bench:build`, from a clean committed tree on a quiet
/// host.
///
/// # Panics
///
/// Panics when a qualification suite cannot establish references, the
/// control fails a reference, a perturbed run fails none, or a dimension
/// regresses in no engine.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the release server from `mise run bench:build`, the Postgres wrapper, a clean committed tree and a quiet host"]
async fn measured_regressions_reach_normal_reference_judgment() {
    let directory = tempfile::tempdir().expect("directory");
    let cases = [
        (
            Engine::Bifrost,
            vec!["writes-hot"],
            "native-write-c1",
            ["p95_ms", "cpu_us_per_row", "rows_per_s_per_vcpu"],
        ),
        (
            Engine::Verification,
            vec![],
            "direct-verify-c1",
            [
                "overhead_p99_ms",
                "cpu_us_per_verification",
                "verifications_per_s_per_vcpu",
            ],
        ),
        (
            Engine::Gateway,
            vec![],
            "chat.c1",
            [
                "overhead_p99_ms",
                "cpu_us_per_request",
                "requests_per_s_per_vcpu",
            ],
        ),
    ];
    let none = Baselines::default();
    let mut failed = [false; 3];
    for (engine, rest, step, dimensions) in cases {
        let mut suites = Vec::new();
        for _ in 0..2 {
            let suite = judged(engine, &rest, step, true, &none, false, directory.path()).await;
            assert_eq!(
                suite.1.qualification_refusals(),
                Vec::<String>::new(),
                "{}",
                suite.1.render()
            );
            suites.push(suite);
        }
        let promoted = Baselines::promote(&suites).expect("references promote");
        let control = judged(
            engine,
            &rest,
            step,
            false,
            &promoted,
            false,
            directory.path(),
        )
        .await;
        assert_eq!(control.1.verdict, Verdict::Pass, "{}", control.1.render());
        let regressed = judged(
            engine,
            &rest,
            step,
            false,
            &promoted,
            true,
            directory.path(),
        )
        .await;
        let record = &regressed.1;
        println!("{}\n{}", regressed.0, record.render());
        assert_eq!(record.verdict, Verdict::Fail, "{}", record.render());
        assert_eq!(record.exit_code(), 1);
        let mut regressed = false;
        for (dimension, metric) in dimensions.into_iter().enumerate() {
            assert_eq!(
                reference_verdict(&control.1, step, metric),
                Some(Verdict::Pass),
                "{step} control {metric}"
            );
            if reference_verdict(record, step, metric) == Some(Verdict::Fail) {
                failed[dimension] = true;
                regressed = true;
            }
        }
        assert!(
            regressed,
            "{step}: no reference regressed: {}",
            record.render()
        );
    }
    assert_eq!(
        failed, [true; 3],
        "latency, CPU per work and throughput per vCPU each regress in some engine"
    );
}
