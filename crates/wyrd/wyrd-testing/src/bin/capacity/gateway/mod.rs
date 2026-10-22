//! AI gateway overhead benchmark.
//!
//! Reads as the production journey it measures. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) on the whole host; the report title carries the host
//! fingerprint and build identity, so compare only reports from matching
//! hosts. The tenant administrator configures `openai_compatible`
//! deployments against a local upstream, an explicit capture policy, and one
//! machine principal per client; every call is raw HTTP/1.1 keep-alive
//! `POST /v1/chat/completions` with a `wyrd_sk_` key, timed until the
//! complete answer is validated. All evidence comes from client timings,
//! `/metrics`, and the OS's readings of the server process.
//!
//! `gateway-overhead` runs [`Case`]s, each selectable with `--case 1,2,3`,
//! at each client count of its matrix that `--clients 1,4,16,64` keeps:
//!
//! 1. `chat`: about 1 KiB in, a 1 KiB buffered answer, metadata capture.
//! 2. `chat-stream`: the same request streamed, 32 data events then
//!    `[DONE]`, metadata capture.
//! 3. `chat-capture`: 64 KiB in and out, request and response payload
//!    capture, at 16 and 64 clients.
//!
//! The upstream answers instantly, so each row judges the server's own
//! overhead (arrival to handoff, less the provider exchange) p99 strictly
//! under 1 ms at every handoff point, beside client roundtrip percentiles,
//! capture integrity, a bounded drain, and a loss-free shutdown.
//!
//! Run through `mise run bench:gateway:overhead`. Every window is recorded
//! with `perf` under `profiles/` unless `--no-profile`, and prints the
//! runtime component Pareto, the CPU Pareto by crate, and the off-CPU stacks
//! as it ends. The report, raw samples (`samples.jsonl`), and server log
//! land in `target/gateway-overhead/`; each row prints as it is judged.
//! Exits nonzero when any row fails.

mod capacity;
mod envelope;
mod report;
mod run;
mod workload;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use capacity::{KEYS_PER_TENANT, Load, MAX_IN_FLIGHT, PACING, RATES, Slo, TENANTS};
use envelope::{CAPACITY_HEADER, envelope};
use report::{OVERHEAD_HEADER, Report, Shutdown, Verdict};
use run::{Bench, Profiler, WARMUP, WINDOW, settle};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use workload::{CapturePolicy, Shape, Tenant};
use wyrd_testing::capacity::{binary_identity, install_tracing, release_binary};
use wyrd_testing::gateway_capture::{GatewayCapture, PacedUpstream};
use wyrd_testing::release_server::{LocalServer, Placement};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Every client count any case runs at.
const CLIENTS: [usize; 4] = [1, 4, 16, 64];

/// Where the overhead report, samples, profiles, and server log land.
const OUTPUT: &str = "target/gateway-overhead";

/// Where the capacity report, samples, profiles, and server log land.
const CAPACITY_OUTPUT: &str = "target/gateway-capacity";

/// Data events of every streamed answer.
const STREAM_CHUNKS: usize = 32;

/// Request content, and buffered answer content, of the small cases.
const SMALL_BYTES: usize = 1024;

/// Request and answer content of the payload-capture case.
const LARGE_BYTES: usize = 64 * 1024;

/// One overhead case of the matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    /// Buffered 1 KiB chat.
    Chat,
    /// Streamed 1 KiB chat.
    ChatStream,
    /// Buffered 64 KiB chat with payload capture.
    ChatCapture,
}

impl Case {
    /// Every case, in run order.
    const ALL: [Self; 3] = [Self::Chat, Self::ChatStream, Self::ChatCapture];

    /// The case `--case` names by number.
    fn numbered(item: &str) -> Option<Self> {
        match item {
            "1" => Some(Self::Chat),
            "2" => Some(Self::ChatStream),
            "3" => Some(Self::ChatCapture),
            _ => None,
        }
    }

    /// The report's name of the case, its number first.
    const fn name(self) -> &'static str {
        match self {
            Self::Chat => "1 chat",
            Self::ChatStream => "2 chat-stream",
            Self::ChatCapture => "3 chat-capture",
        }
    }

    /// The client counts the case runs at.
    const fn clients(self) -> &'static [usize] {
        match self {
            Self::Chat | Self::ChatStream => &CLIENTS,
            Self::ChatCapture => &[16, 64],
        }
    }

    /// The upstream path prefix whose deployment answers it.
    const fn prefix(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::ChatStream => "stream",
            Self::ChatCapture => "capture",
        }
    }

    /// What its tenant captures.
    const fn capture(self) -> CapturePolicy {
        match self {
            Self::Chat | Self::ChatStream => CapturePolicy::Metadata,
            Self::ChatCapture => CapturePolicy::Payload,
        }
    }

    /// What each of its calls asks for and expects back.
    fn shape(self) -> Shape {
        let (stream, request_bytes, expected) = match self {
            Self::Chat => (false, SMALL_BYTES, SMALL_BYTES),
            Self::ChatStream => (true, SMALL_BYTES, STREAM_CHUNKS),
            Self::ChatCapture => (false, LARGE_BYTES, LARGE_BYTES),
        };
        Shape {
            model: format!("bench-{}/gpt-4o", self.prefix()),
            stream,
            request_bytes,
            expected,
        }
    }
}

/// The overhead steps one command line selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overhead {
    /// Each selected `(case, clients)` step, in run order.
    steps: Vec<(Case, usize)>,
    /// Whether every window is profiled.
    profile: bool,
}

impl Overhead {
    /// Reads `--case`, `--clients`, and `--no-profile` from `arguments`, the
    /// command line with the subcommand removed. The steps are each selected
    /// case's matrix intersected with the selected clients; both default to
    /// all.
    ///
    /// # Errors
    ///
    /// Returns a message for an unknown argument, an invalid or duplicate
    /// value, or a selection with no step.
    pub fn parse(arguments: &[String]) -> std::result::Result<Self, String> {
        crate::flags::known(arguments, &["--case", "--clients"], &["--no-profile"])?;
        let cases = crate::flags::list(arguments, "--case", "cases 1, 2, and 3", Case::numbered)?
            .unwrap_or_else(|| Case::ALL.to_vec());
        let clients = crate::flags::clients(arguments, &CLIENTS)?;
        let steps: Vec<(Case, usize)> = cases
            .iter()
            .flat_map(|case| {
                case.clients()
                    .iter()
                    .filter(|count| clients.contains(count))
                    .map(|count| (*case, *count))
            })
            .collect();
        if steps.is_empty() {
            return Err(format!(
                "no overhead step runs: --clients {clients:?} is outside every selected case's matrix"
            ));
        }
        Ok(Self {
            steps,
            profile: !arguments.iter().any(|argument| argument == "--no-profile"),
        })
    }

    /// Runs every step against one fresh server, writing evidence under
    /// `output`, and returns whether every row passed.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, provisioning, or evidence failure that
    /// stopped it; a step's failure becomes its FAIL row instead.
    async fn benchmark(&self, output: &Path) -> Result<bool> {
        fresh(output)?;
        let binary = release_binary()?;
        let server = LocalServer::start(&binary, &["bench"], &[], Placement::Host).await?;
        let [setup] = server.tenants() else {
            return Err("setup provisioned one tenant".into());
        };
        let upstream = upstream().await;
        let keys = self
            .steps
            .iter()
            .map(|(_, clients)| *clients)
            .max()
            .unwrap_or(1);
        let tenant = Tenant::provision(setup, &upstream.uri(), keys).await?;
        let profiles = self.profile.then(|| output.join("profiles"));
        let bench = Bench::new(&server, &tenant, profiles);
        let title = format!(
            "Gateway overhead: {}, binary {}, {}s warmup + {}s windows, {}",
            server.describe(),
            binary_identity(&binary)?,
            WARMUP.as_secs(),
            WINDOW.as_secs(),
            if self.profile {
                "profiled at 999 Hz"
            } else {
                "not profiled"
            },
        );
        let mut report = Report::new(title, OVERHEAD_HEADER, server.memory_limit_bytes());
        for &(case, clients) in &self.steps {
            let name = format!("{} {clients}", case.name());
            let step = match tenant.capture(case.capture()).await {
                Ok(()) => bench.run(&name, &case.shape(), clients).await,
                Err(error) => Err(error),
            };
            match step {
                Ok(step) => report.overhead(&step),
                Err(error) => report.failed(&name, error.to_string()),
            }
            report.write_to(output)?;
        }
        stop(server, output, &mut report).await
    }
}

/// The capacity windows one command line selects.
#[derive(Debug, Clone, PartialEq)]
pub struct Capacity {
    /// Selected cases, in run order.
    loads: Vec<Load>,
    /// Offered aggregate rates, ascending.
    rates: Vec<f64>,
    /// The declared SLOs.
    slo: Slo,
    /// Whether every window is profiled.
    profile: bool,
}

impl Capacity {
    /// Reads `--case`, `--rates`, the `--max-*-ms` SLOs, and `--no-profile`
    /// from `arguments`, the command line with the subcommand removed.
    /// Cases default to all and rates to [`RATES`]; rates run ascending.
    ///
    /// # Errors
    ///
    /// Returns a message for an unknown argument (including `--clients`),
    /// an invalid or duplicate value, or a missing SLO a selected case needs.
    pub fn parse(arguments: &[String]) -> std::result::Result<Self, String> {
        crate::flags::known(
            arguments,
            &[
                "--case",
                "--rates",
                "--max-answer-ms",
                "--max-first-token-ms",
                "--max-chunk-lag-ms",
                "--max-stream-ms",
            ],
            &["--no-profile"],
        )?;
        let loads = crate::flags::list(arguments, "--case", "cases 1, 2, and 3", Load::numbered)?
            .unwrap_or_else(|| Load::ALL.to_vec());
        let mut rates = crate::flags::list(arguments, "--rates", "positive requests/s", |item| {
            item.parse::<f64>()
                .ok()
                .filter(|rate| rate.is_finite() && *rate > 0.0)
        })?
        .unwrap_or_else(|| RATES.to_vec());
        rates.sort_by(f64::total_cmp);
        Ok(Self {
            slo: Slo::parse(arguments, &loads)?,
            loads,
            rates,
            profile: !arguments.iter().any(|argument| argument == "--no-profile"),
        })
    }

    /// Runs every selected case's ascending rate sweep, stopping a case at
    /// its first non-sustainable rate, then its overload sequence after the
    /// highest sustainable one, against one fresh four-tenant server, and
    /// returns whether the command passed.
    ///
    /// # Errors
    ///
    /// Returns a server, upstream, provisioning, or evidence failure that
    /// stopped it; a window's failure becomes its FAIL row instead.
    async fn benchmark(&self, output: &Path) -> Result<bool> {
        fresh(output)?;
        let binary = release_binary()?;
        let server = LocalServer::start(&binary, &TENANTS, &[], Placement::Host).await?;
        let upstream = Arc::new(PacedUpstream::start(PACING, MAX_IN_FLIGHT).await?);
        let mut tenants = Vec::with_capacity(TENANTS.len());
        for setup in server.tenants() {
            let tenant = Tenant::provision(setup, &upstream.uri(), KEYS_PER_TENANT).await?;
            tenant.capture(CapturePolicy::Metadata).await?;
            tenants.push(tenant);
        }
        let profiles = self.profile.then(|| output.join("profiles"));
        let bench = capacity::Bench::new(
            &server,
            &tenants,
            Arc::clone(&upstream),
            workload::http()?,
            Profiler::new(&server, profiles),
        );
        let title = format!(
            "Gateway capacity: {}, binary {}, SLO p99 bounds {:?} s, upstream {PACING:?}, {}s warmup + {}s windows, {KEYS_PER_TENANT} warmed keys per tenant, {MAX_IN_FLIGHT} in-flight bound, arrivals over {} ms late missed, {}",
            server.describe(),
            binary_identity(&binary)?,
            self.slo,
            capacity::WARMUP.as_secs(),
            capacity::WINDOW.as_secs(),
            capacity::MISS_AFTER.as_millis(),
            if self.profile {
                "profiled at 999 Hz"
            } else {
                "not profiled"
            },
        );
        let mut report = Report::new(title, CAPACITY_HEADER, server.memory_limit_bytes());
        for &load in &self.loads {
            let mut tested = Vec::new();
            for &rate in &self.rates {
                match bench.rate(load, rate).await {
                    Ok(measured) => tested.push((rate, report.rate(load, &measured, &self.slo))),
                    Err(error) => {
                        report.failed(&format!("{} {rate}/s", load.name()), error.to_string());
                        tested.push((rate, Verdict::Invalid));
                    }
                }
                report.write_to(output)?;
                if tested
                    .last()
                    .is_some_and(|(_, verdict)| *verdict != Verdict::Sustainable)
                {
                    break;
                }
            }
            if tested.len() < self.rates.len() {
                report.note(format!(
                    "{}: rates above the first failing rate were not run",
                    load.name()
                ));
            }
            let (envelope, found) = envelope(load, &tested);
            report.note(envelope);
            let highest = tested
                .iter()
                .filter(|(_, verdict)| *verdict == Verdict::Sustainable)
                .map(|(rate, _)| *rate)
                .next_back();
            match highest {
                Some(rate) => match bench.overload(load, rate).await {
                    Ok((high, low)) => report.overload(load, &high, &low, &self.slo),
                    Err(error) => {
                        report.failed(&format!("{} overload", load.name()), error.to_string());
                    }
                },
                None if !found => report.failed(
                    &format!("{} envelope", load.name()),
                    "no measured sustainable rate".to_owned(),
                ),
                None => {}
            }
            report.write_to(output)?;
        }
        stop(server, output, &mut report).await
    }
}

/// Settles `server`, stops it, retains its log under `output`, adds the
/// shutdown row, writes the report, and returns whether every row passed.
///
/// # Errors
///
/// Returns the report write failure.
async fn stop(server: LocalServer, output: &Path, report: &mut Report) -> Result<bool> {
    let settled = quiescent(&server).await;
    let log = output.join("server.log");
    let stopped = server.stop(&log).map_err(Into::into);
    report.shutdown(&Shutdown {
        stopped,
        settled,
        log: std::fs::read_to_string(&log).map_err(Into::into),
    });
    report.write_to(output)?;
    Ok(report.passed())
}

/// Replaces `output` with an empty directory, leaving every other mode's
/// evidence alone.
///
/// # Errors
///
/// Returns the filesystem failure.
fn fresh(output: &Path) -> Result<()> {
    if output.exists() {
        std::fs::remove_dir_all(output)?;
    }
    std::fs::create_dir_all(output)?;
    Ok(())
}

/// Whether `server` settled with nothing active or pending and no outbox
/// item ever lost other than shed at an outbox limit, which saturated rows
/// already report, before it is stopped.
///
/// # Errors
///
/// Returns scrape failures.
async fn quiescent(server: &LocalServer) -> Result<bool> {
    if settle(server).await?.is_none() {
        return Ok(false);
    }
    let metrics = server.metrics().await?;
    let lost = metrics.sum("outbox_events_lost_total", &[]);
    Ok(lost - metrics.sum("outbox_events_lost_total", &["reason=\"shed\""]) == 0.0)
}

/// The instant overhead upstream: one route per [`Case::prefix`], answering
/// its case's buffered answer or whole stream, recording no requests so its
/// memory stays bounded.
async fn upstream() -> MockServer {
    let upstream = MockServer::builder()
        .disable_request_recording()
        .start()
        .await;
    let answers = [
        (
            "chat",
            ResponseTemplate::new(200).set_body_json(GatewayCapture::sized_completion(SMALL_BYTES)),
        ),
        (
            "stream",
            ResponseTemplate::new(200).set_body_raw(
                GatewayCapture::chat_events("chatcmpl-bench", STREAM_CHUNKS),
                "text/event-stream",
            ),
        ),
        (
            "capture",
            ResponseTemplate::new(200).set_body_json(GatewayCapture::sized_completion(LARGE_BYTES)),
        ),
    ];
    for (prefix, answer) in answers {
        Mock::given(method("POST"))
            .and(path(format!("/{prefix}/v1/chat/completions")))
            .respond_with(answer)
            .mount(&upstream)
            .await;
    }
    upstream
}

/// Runs the overhead benchmark. `arguments` is the process command line
/// with the `gateway-overhead` subcommand removed.
pub(crate) async fn overhead(arguments: Vec<String>) -> ExitCode {
    install_tracing();
    let outcome = match Overhead::parse(&arguments) {
        Ok(overhead) => overhead.benchmark(&PathBuf::from(OUTPUT)).await,
        Err(error) => Err(error.into()),
    };
    exit("overhead", outcome)
}

/// Runs the capacity benchmark. `arguments` is the process command line
/// with the `gateway-capacity` subcommand removed.
pub(crate) async fn capacity(arguments: Vec<String>) -> ExitCode {
    install_tracing();
    let outcome = match Capacity::parse(&arguments) {
        Ok(capacity) => capacity.benchmark(&PathBuf::from(CAPACITY_OUTPUT)).await,
        Err(error) => Err(error.into()),
    };
    exit("capacity", outcome)
}

/// The exit code of `mode`'s `outcome`, printing a harness failure.
fn exit(mode: &str, outcome: Result<bool>) -> ExitCode {
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("gateway {mode} benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::sync::Semaphore;
    use tokio::time::Instant;

    use super::capacity::{Load, Measured, Schedule, Slo, open_loop};
    use super::envelope::CAPACITY_HEADER;
    use super::report::{OVERHEAD_HEADER, Report};
    use super::run::Step;
    use super::{CAPACITY_OUTPUT, Capacity, Case, OUTPUT, Overhead, fresh};

    /// A command line of `arguments` after the program name.
    fn line(arguments: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(arguments.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// The default overhead selection is all ten profiled steps; `--case`
    /// and `--clients` keep only their intersection with each case's
    /// matrix, and invalid, unknown, or empty selections fail.
    ///
    /// # Panics
    ///
    /// Panics when a selection parses the wrong way.
    #[test]
    fn case_and_client_selection_preserves_matrix() {
        let all = Overhead::parse(&line(&[])).expect("the default selection");
        assert!(all.profile);
        assert_eq!(all.steps.len(), 10);
        assert_eq!(
            all.steps
                .iter()
                .filter(|(case, _)| *case == Case::ChatCapture)
                .count(),
            2
        );
        let quick = Overhead::parse(&line(&["--case", "1", "--clients", "16", "--no-profile"]))
            .expect("one step");
        assert_eq!(quick.steps, vec![(Case::Chat, 16)]);
        assert!(!quick.profile);
        let wide = Overhead::parse(&line(&["--clients", "64"])).expect("three steps");
        assert_eq!(
            wide.steps,
            vec![
                (Case::Chat, 64),
                (Case::ChatStream, 64),
                (Case::ChatCapture, 64)
            ]
        );
        for refused in [
            &["--case", "3", "--clients", "1"][..],
            &["--case", "4"],
            &["--case", "1,1"],
            &["--clients", "8"],
            &["--rate", "50"],
            &["--case"],
        ] {
            assert!(Overhead::parse(&line(refused)).is_err(), "{refused:?}");
        }
        let shape = Case::ChatCapture.shape();
        assert_eq!(
            (shape.request_bytes, shape.expected, shape.stream),
            (65_536, 65_536, false)
        );
        assert_eq!(Case::ChatStream.shape().expected, 32);
    }

    /// The capacity SLO flags of the release runs.
    const SLOS: [&str; 8] = [
        "--max-answer-ms",
        "200",
        "--max-first-token-ms",
        "350",
        "--max-chunk-lag-ms",
        "50",
        "--max-stream-ms",
        "1100",
    ];

    /// Capacity parses cases, ascending rates, and every SLO a selected mode
    /// needs, refusing `--clients`; open-loop arrivals keep the offered rate
    /// whatever responses do, count a stalled driver's late arrivals missed
    /// instead of bursting them, count arrivals over the in-flight bound
    /// capped, and spread contention 70/10/10/10 with both modes per tenant.
    ///
    /// # Panics
    ///
    /// Panics when a selection, schedule, or driver count is wrong.
    #[test]
    fn capacity_arrivals_preserve_offered_rate_and_slo_inputs() {
        let all = Capacity::parse(&line(&SLOS)).expect("the default selection");
        assert_eq!(all.loads, Load::ALL.to_vec());
        assert_eq!(all.rates, vec![50.0, 100.0, 250.0, 500.0, 1000.0]);
        assert_eq!(all.slo.answer, Some(0.2));
        assert_eq!(all.slo.stream, Some(1.1));
        let buffered = Capacity::parse(&line(&[
            "--case",
            "1",
            "--rates",
            "250,50",
            "--max-answer-ms",
            "200",
        ]))
        .expect("a buffered selection needs only its bound");
        assert_eq!(buffered.rates, vec![50.0, 250.0]);
        assert_eq!(
            buffered.slo,
            Slo {
                answer: Some(0.2),
                ..Slo::default()
            }
        );
        for refused in [
            &["--case", "2", "--max-answer-ms", "200"][..],
            &["--case", "1"],
            &["--case", "1", "--max-answer-ms", "0"],
            &["--case", "1", "--max-answer-ms", "inf"],
            &["--case", "1", "--max-answer-ms", "200", "--rates", "50,50"],
            &["--case", "1", "--max-answer-ms", "200", "--rates", "-5"],
            &["--case", "1", "--max-answer-ms", "200", "--clients", "4"],
            &["--case", "4", "--max-answer-ms", "200"],
        ] {
            assert!(Capacity::parse(&line(refused)).is_err(), "{refused:?}");
        }

        let planned = Schedule::new(vec![
            (100.0, Duration::from_secs(1)),
            (50.0, Duration::from_secs(2)),
        ])
        .planned();
        assert_eq!(planned.len(), 200);
        assert_eq!(planned[100].offset, Duration::from_secs(1));
        assert_eq!(planned[101].offset, Duration::from_millis(1_020));

        let contention: Vec<(usize, bool)> = (0..1_000)
            .map(|index| Load::TenantContention.arrival(index))
            .collect();
        for (tenant, share) in [(0, 700), (1, 100), (2, 100), (3, 100)] {
            let mine: Vec<bool> = contention
                .iter()
                .filter(|(owner, _)| *owner == tenant)
                .map(|(_, stream)| *stream)
                .collect();
            assert_eq!(mine.len(), share, "tenant {tenant}");
            assert_eq!(
                mine.iter().filter(|stream| **stream).count(),
                share / 2,
                "tenant {tenant}"
            );
        }

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .expect("a paused test runtime");
        runtime.block_on(async {
            // Responses far slower than the arrival interval keep the rate.
            let schedule = Schedule::new(vec![(100.0, Duration::from_secs(1))]);
            let slow = || async { tokio::time::sleep(Duration::from_secs(5)).await };
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(1_000)),
                1_000,
                |_| slow(),
            )
            .await;
            assert_eq!(
                (driven.scheduled, driven.sent, driven.missed, driven.capped),
                (100, 100, 0, 0)
            );
            assert_eq!(driven.peak_in_flight, 100);
            assert!(driven.lateness_us.iter().all(|late| *late == 0));
            // Arrivals beyond the in-flight bound are capped, not queued.
            let schedule = Schedule::new(vec![(100.0, Duration::from_millis(100))]);
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(2)),
                2,
                |_| slow(),
            )
            .await;
            assert_eq!((driven.scheduled, driven.sent, driven.capped), (10, 2, 8));
        });
        // A driver stall misses the arrivals it overran instead of bursting
        // them later; every arrival is accounted for.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a test runtime");
        runtime.block_on(async {
            let schedule = Schedule::new(vec![(1_000.0, Duration::from_millis(100))]);
            let driven = open_loop(
                &schedule,
                Instant::now(),
                Arc::new(Semaphore::new(1_000)),
                1_000,
                |planned| {
                    if planned.index == 10 {
                        std::thread::sleep(Duration::from_millis(60));
                    }
                    async {}
                },
            )
            .await;
            assert_eq!(driven.scheduled, 100);
            assert!(driven.missed >= 30, "{driven:?}");
            assert_eq!(driven.sent + driven.missed + driven.capped, 100);
            assert!(driven.lateness_us.iter().all(|late| *late <= 20_000));
        });
    }

    /// Both modes profile by default and not with `--no-profile`, write
    /// separate output directories that replace only their own, and record
    /// the same raw roundtrip fields and verdicts whether profiled or not.
    ///
    /// # Panics
    ///
    /// Panics when a mode's flags, output, samples, or verdicts differ.
    #[test]
    fn benchmark_modes_preserve_profiles_and_roundtrip_evidence() {
        assert!(Overhead::parse(&line(&[])).expect("overhead").profile);
        assert!(
            !Overhead::parse(&line(&["--no-profile"]))
                .expect("overhead")
                .profile
        );
        assert!(Capacity::parse(&line(&SLOS)).expect("capacity").profile);
        let mut quiet = SLOS.to_vec();
        quiet.push("--no-profile");
        assert!(!Capacity::parse(&line(&quiet)).expect("capacity").profile);

        assert_ne!(OUTPUT, CAPACITY_OUTPUT);
        let root = tempfile::tempdir().expect("a scratch directory");
        let (overhead, capacity) = (root.path().join("overhead"), root.path().join("capacity"));
        std::fs::create_dir_all(&capacity).expect("capacity output");
        std::fs::write(capacity.join("report.md"), "kept").expect("capacity evidence");
        std::fs::create_dir_all(&overhead).expect("overhead output");
        std::fs::write(overhead.join("stale"), "old").expect("stale overhead evidence");
        fresh(&overhead).expect("a fresh overhead output");
        assert!(!overhead.join("stale").exists());
        assert!(capacity.join("report.md").exists());

        let rendered = |profiled: bool| {
            let mut step = Step {
                name: "1 chat 16".to_owned(),
                clients: 16,
                ..Step::default()
            };
            step.samples.arrivals = 2;
            step.samples.completion_us = vec![300, 400];
            step.profile_gaps = profiled.then(Vec::new);
            let mut measured = Measured {
                name: "1 buffered 50/s".to_owned(),
                rate: 50.0,
                seconds: 60.0,
                ..Measured::default()
            };
            measured.profile_gaps = profiled.then(Vec::new);
            let directory = tempfile::tempdir().expect("a report directory");
            let mut report = Report::new("overhead".to_owned(), OVERHEAD_HEADER, 64 << 30);
            report.overhead(&step);
            report
                .write_to(directory.path())
                .expect("the overhead report");
            let mut lines =
                std::fs::read_to_string(directory.path().join("samples.jsonl")).expect("samples");
            let mut verdicts =
                std::fs::read_to_string(directory.path().join("report.md")).expect("report");
            let mut report = Report::new("capacity".to_owned(), CAPACITY_HEADER, 64 << 30);
            let slo = Slo {
                answer: Some(0.2),
                ..Slo::default()
            };
            report.rate(Load::Buffered, &measured, &slo);
            report
                .write_to(directory.path())
                .expect("the capacity report");
            lines.push_str(
                &std::fs::read_to_string(directory.path().join("samples.jsonl")).expect("samples"),
            );
            verdicts.push_str(
                &std::fs::read_to_string(directory.path().join("report.md")).expect("report"),
            );
            let keys: Vec<Vec<String>> = lines
                .lines()
                .map(|line| {
                    let value: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
                    assert_eq!(value["profiled"], profiled);
                    value
                        .as_object()
                        .expect("an object")
                        .keys()
                        .cloned()
                        .collect()
                })
                .collect();
            let verdicts: Vec<String> = verdicts
                .lines()
                .filter(|line| line.starts_with("| 1 "))
                .map(|line| line.rsplit(" | ").nth(1).unwrap_or_default().to_owned())
                .collect();
            (keys, verdicts)
        };
        let (profiled_keys, profiled_verdicts) = rendered(true);
        let (quiet_keys, quiet_verdicts) = rendered(false);
        assert_eq!(profiled_keys, quiet_keys);
        assert!(profiled_keys[0].contains(&"completion_us".to_owned()));
        assert!(profiled_keys[1].contains(&"calls".to_owned()));
        assert_eq!(profiled_verdicts, quiet_verdicts);
        assert_eq!(profiled_verdicts.len(), 5, "{profiled_verdicts:?}");
    }
}
