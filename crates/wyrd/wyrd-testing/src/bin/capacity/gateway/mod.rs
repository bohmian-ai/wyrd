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

mod report;
mod run;
mod workload;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use report::{Report, Shutdown};
use run::{Bench, WARMUP, WINDOW, settle};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use workload::{CapturePolicy, Shape, Tenant};
use wyrd_testing::capacity::{binary_identity, install_tracing, release_binary};
use wyrd_testing::gateway_capture::GatewayCapture;
use wyrd_testing::release_server::{LocalServer, Placement};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Every client count any case runs at.
const CLIENTS: [usize; 4] = [1, 4, 16, 64];

/// Where the overhead report, samples, profiles, and server log land.
const OUTPUT: &str = "target/gateway-overhead";

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
        let mut report = Report::new(title, server.memory_limit_bytes());
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
/// item ever lost, before it is stopped.
///
/// # Errors
///
/// Returns scrape failures.
async fn quiescent(server: &LocalServer) -> Result<bool> {
    if settle(server).await?.is_none() {
        return Ok(false);
    }
    Ok(server.metrics().await?.sum("outbox_events_lost_total", &[]) == 0.0)
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
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("gateway overhead benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Case, Overhead};

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
}
