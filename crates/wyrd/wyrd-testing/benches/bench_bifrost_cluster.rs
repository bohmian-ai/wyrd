//! Canonical public full-cluster Bifrost benchmark adapter.

/// Run either the shortened real-cluster smoke or controlled reference owner.
///
/// # Errors
/// Returns a typed preflight, environment, cluster, report, or IO failure.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if std::env::var_os("WYRD_BIFROST_CLUSTER_SMOKE").is_some() {
        wyrd_testing::bifrost::bench_cluster::run_smoke().await?;
        return Ok(());
    }
    wyrd_testing::bifrost::bench_cluster::run_reference().await?;
    Ok(())
}
