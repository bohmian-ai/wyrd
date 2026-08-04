//! Canonical public full-cluster Bifrost benchmark adapter.

/// Run either the shortened real-cluster smoke or controlled reference owner.
///
/// # Errors
/// Returns a typed preflight, environment, cluster, report, or IO failure.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use wyrd_testing::bifrost::bench_cluster::{
        ClusterBenchmarkMode, parse_cluster_benchmark_args,
    };
    let command = parse_cluster_benchmark_args(std::env::args().skip(1))
        .map_err(|error| format!("invalid Bifrost benchmark arguments: {error}"))?;
    match command.mode {
        ClusterBenchmarkMode::Capacity => {
            wyrd_testing::bifrost::bench_cluster::run_capacity(command).await?;
        }
        ClusterBenchmarkMode::Qualification => {
            wyrd_testing::bifrost::bench_cluster::run_qualification(command).await?;
        }
    }
    Ok(())
}
