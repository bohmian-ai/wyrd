//! Verification runtime and engine overhead benchmark (REQ-015, AC-009).
//!
//! Reads as the production journey it measures. An operator starts one
//! release `wyrd-server` through the local-development guide (`migrate`,
//! serve, `setup`) on the whole host, Linux or macOS; the report title carries
//! the host fingerprint, so compare only reports from matching hosts. One
//! tenant registers an assertion Verifier, a Model, and one Service per
//! closed-loop client, and each client is its own application loading its
//! pulled Service bundle under that Service's own Card-bound key, so every
//! call is the public Rust client's `observe().verify` or
//! `observe().submit` exactly as production makes it. All evidence comes from
//! client timings, `/metrics`, and the OS's readings of the server process.
//!
//! The two AC-009 cases, each selectable with `--case 1,2`, run at each of
//! [`CLIENTS`] closed-loop clients, or those `--clients 1,4,16,64` selects:
//!
//! 1. Direct `verify`: judged on the server's verification overhead (the
//!    request's arrival until its result is staged, less the Verifier's own
//!    blocks) p99 strictly under 1 ms; reports the Verifier's time, the
//!    phases, and what the client adds.
//! 2. Background `submit`: reported only: submit time, queue wait, the queued
//!    run's overhead and Verifier time, trigger to terminal, throughput, and
//!    the drain.
//!
//! Run through `mise run bench:verification:capacity`. Every window of the
//! server is recorded with `perf` (a frame-pointer build) under `profiles/`
//! and prints the runtime's component Pareto, the CPU Pareto by crate, and
//! the off-CPU stacks as it ends. The report, raw latencies
//! (`samples.jsonl`), and server log land in `target/verification-capacity/`;
//! each row prints as it is judged. Exits nonzero when any row fails.

mod report;
mod run;
mod workload;

use std::path::PathBuf;
use std::process::ExitCode;

use report::Report;
use run::{Bench, Op, WINDOW};
use workload::Tenant;
use wyrd_testing::capacity::{install_tracing, release_binary};
use wyrd_testing::release_server::{LocalServer, Placement};

/// Error type of every benchmark step: the binary only reports it.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Closed-loop client points of each case, one window each.
const CLIENTS: [usize; 4] = [1, 4, 16, 64];

/// Where the report, samples, profiles, and server log land.
const OUTPUT: &str = "target/verification-capacity";

/// Runs the selected `cases` at each of `clients` against one fresh server
/// and returns whether every row passed.
///
/// # Errors
///
/// Returns a server, client, capture, or evidence failure that stopped it.
async fn benchmark(cases: &[u8], clients: &[usize]) -> Result<bool> {
    let output = PathBuf::from(OUTPUT);
    if output.exists() {
        std::fs::remove_dir_all(&output)?;
    }
    std::fs::create_dir_all(&output)?;
    let server = LocalServer::start(&release_binary()?, &["bench"], &[], Placement::Host).await?;
    let [setup] = server.tenants() else {
        return Err("setup provisioned one tenant".into());
    };
    let applications = clients.iter().copied().max().unwrap_or(0);
    let tenant = Tenant::provision(setup, &output.join("tenants"), applications).await?;
    let bench = Bench::new(&server, tenant, output.join("profiles"));
    let label = format!(
        "{}, {}s windows, 2 KiB contexts, one Service key per client",
        server.describe(),
        WINDOW.as_secs()
    );
    let mut report = Report::new(label, server.memory_limit_bytes());
    if cases.contains(&1) {
        for &clients in clients {
            let step = format!("1 direct verify {clients}");
            let measured = bench.run(&step, Op::Verify, clients).await?;
            report.direct(&step, &measured);
        }
    }
    if cases.contains(&2) {
        for &clients in clients {
            let step = format!("2 background submit {clients}");
            let measured = bench.run(&step, Op::Submit, clients).await?;
            report.queued(&step, &measured);
        }
    }
    report.shutdown(&server.stop(&output.join("server.log")).map_err(Into::into));
    report.write_to(&output)?;
    Ok(report.passed())
}

/// Parses `--case 1,2`, defaulting to both cases.
///
/// # Errors
///
/// Returns an error naming a value that is not case 1 or 2.
fn parse_cases(arguments: &[String]) -> Result<Vec<u8>> {
    let Some(list) = arguments
        .windows(2)
        .find(|pair| pair[0] == "--case")
        .map(|pair| pair[1].as_str())
    else {
        return Ok(vec![1, 2]);
    };
    list.split(',')
        .map(|case| match case.trim().parse::<u8>() {
            Ok(number @ 1..=2) => Ok(number),
            _ => Err(format!("--case takes cases 1 and 2, got `{case}`").into()),
        })
        .collect()
}

/// Runs the benchmark. `arguments` is the process command line with the
/// `verification-capacity` subcommand removed.
pub(crate) async fn main(arguments: Vec<String>) -> ExitCode {
    install_tracing();
    let selected = parse_cases(&arguments).and_then(|cases| {
        let clients = crate::flags::clients(&arguments, &CLIENTS)?;
        Ok((cases, clients))
    });
    let outcome = match selected {
        Ok((cases, clients)) => benchmark(&cases, &clients).await,
        Err(error) => Err(error),
    };
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("verification capacity benchmark failed: {error}");
            ExitCode::FAILURE
        }
    }
}
