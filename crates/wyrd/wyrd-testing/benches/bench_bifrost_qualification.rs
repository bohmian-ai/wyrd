//! Bifrost qualification-run driver binary.
//!
//! Drives one budgeted qualification run end to end: provision the run's shared
//! resources once, calibrate throughput, resolve the live Oracle memory budget,
//! select the D70 shape, run every family serially over the shared resources,
//! and write the pre-seal qualification manifest. Unlike the per-family bench
//! binaries it selects no single workload or topology; it owns the whole run and
//! prints the written manifest path. The `bench:bifrost:qualification` lane
//! invokes it.

use wyrd_testing::bifrost::bench_qualification_driver::{
    QualificationDriverArgs, execute_qualification_run,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let argv: Vec<String> = std::env::args().collect();
    let command_line = argv.join(" ");
    let args = QualificationDriverArgs::parse(argv.into_iter().skip(1))?;
    let manifest_path = execute_qualification_run(args, vec![command_line]).await?;
    println!("{}", manifest_path.display());
    Ok(())
}
