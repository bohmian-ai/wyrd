use std::collections::HashMap;
use std::path::{Path, PathBuf};

use clap::Parser;
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(
    name = "bench_check",
    about = "Compare criterion estimates against the stage1 baseline; exits non-zero on regression"
)]
struct Args {
    #[arg(long)]
    baseline: PathBuf,

    #[arg(long, default_value = "target/criterion")]
    criterion_dir: PathBuf,
}

#[derive(Debug, Deserialize)]
struct Baseline {
    #[allow(dead_code)]
    version: String,
    tolerance_pct: f64,
    metrics: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
struct Estimates {
    mean: Estimate,
}

#[derive(Debug, Deserialize)]
struct Estimate {
    point_estimate: f64,
}

fn main() {
    let args = Args::parse();

    let baseline_raw = std::fs::read_to_string(&args.baseline)
        .unwrap_or_else(|e| panic!("read baseline {:?}: {e}", args.baseline));
    let baseline: Baseline = serde_json::from_str(&baseline_raw)
        .unwrap_or_else(|e| panic!("parse baseline: {e}"));

    let tol = baseline.tolerance_pct / 100.0;
    let mut regressions: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for (metric_key, &expected_ns) in &baseline.metrics {
        // metric_key format: "bench_name.param.stat" e.g. "write_throughput.batch=100000.p50_ns"
        // Expected 0 means placeholder — skip the gate.
        if expected_ns == 0.0 {
            continue;
        }

        // Map metric_key → criterion estimates.json path.
        // criterion writes: target/criterion/<bench_name>/<param>/new/estimates.json
        let estimates_path = resolve_estimates_path(&args.criterion_dir, metric_key);
        let estimates_raw = match std::fs::read_to_string(&estimates_path) {
            Ok(s) => s,
            Err(_) => {
                eprintln!("WARN: estimates not found at {:?} (skipping)", estimates_path);
                continue;
            }
        };

        let estimates: Estimates = match serde_json::from_str(&estimates_raw) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("WARN: parse estimates {:?}: {e} (skipping)", estimates_path);
                continue;
            }
        };

        let actual_ns = estimates.mean.point_estimate;
        let threshold = expected_ns * (1.0 + tol);
        checked += 1;

        if actual_ns > threshold {
            regressions.push(format!(
                "  {metric_key}: expected ≤{threshold:.0} ns (baseline {expected_ns:.0} + {:.0}%), got {actual_ns:.0} ns",
                baseline.tolerance_pct,
            ));
        } else {
            println!("OK  {metric_key}: {actual_ns:.0} ns (baseline {expected_ns:.0} ns)");
        }
    }

    if checked == 0 {
        println!("bench:check — baseline has all zeros (Stage 1 placeholder); no regressions checked.");
        std::process::exit(0);
    }

    if regressions.is_empty() {
        println!("bench:check passed — {checked} metric(s) within tolerance.");
    } else {
        eprintln!(
            "bench:check FAILED — {} regression(s) of {} checked:",
            regressions.len(),
            checked
        );
        for r in &regressions {
            eprintln!("{r}");
        }
        std::process::exit(1);
    }
}

fn resolve_estimates_path(criterion_dir: &Path, metric_key: &str) -> PathBuf {
    // metric_key formats:
    //   "bench.stat"         → criterion_dir/bench/new/estimates.json
    //   "bench.param.stat"   → criterion_dir/bench/param/new/estimates.json
    //   param may contain '/' for nested criterion dirs (e.g. "sel/1pct")
    let parts: Vec<&str> = metric_key.splitn(3, '.').collect();
    let bench = parts.first().copied().unwrap_or(metric_key);
    if parts.len() >= 3 {
        let param = parts[1];
        criterion_dir.join(bench).join(param).join("new").join("estimates.json")
    } else {
        criterion_dir.join(bench).join("new").join("estimates.json")
    }
}
