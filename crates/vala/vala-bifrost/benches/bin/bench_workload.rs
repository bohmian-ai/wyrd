use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use serde::Serialize;
use tokio::runtime::Runtime;
use vala_bifrost::catalog::WyrdCatalog;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_bifrost::types::TableScope;
use wyrd_spec::ids::DataTenantId;
use wyrd_storage::factory::iceberg_factory::iceberg_storage_factory;
use wyrd_storage::settings::BackendConfig;

#[derive(Debug, Clone, ValueEnum)]
enum WorkloadKind {
    Selective,
    Broad,
    HighCardinality,
    Rollup,
}

#[derive(Parser, Debug)]
#[command(name = "bench_workload", about = "vala-bifrost large-workload bench")]
struct Args {
    #[arg(long, default_value_t = 10_000_000)]
    rows: u64,

    #[arg(long, default_value_t = 65_536)]
    batch_size: usize,

    #[arg(long, default_value = "selective")]
    workload: WorkloadKind,

    #[arg(long)]
    postgres_url: String,

    #[arg(long, default_value = "local")]
    object_store: String,

    #[arg(long)]
    output: PathBuf,
}

#[derive(Debug, Serialize)]
struct WorkloadReport {
    workload: String,
    rows: u64,
    batch_size: usize,
    files_total: usize,
    files_pruned: usize,
    bytes_total: u64,
    bytes_scanned: u64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    iceberg_snapshots: usize,
    commit_count: usize,
}

fn main() {
    let args = Args::parse();
    let rt = Runtime::new().unwrap();
    rt.block_on(run(args));
}

async fn run(args: Args) {
    let tmp = tempfile::tempdir().unwrap();
    let warehouse = format!("file://{}", tmp.path().display());

    let pool = Arc::new(
        sqlx::PgPool::connect(&args.postgres_url)
            .await
            .expect("connect to bench db"),
    );
    vala_sql::testing::migrate_for_test(&pool)
        .await
        .expect("migrate bench db");

    let catalog_uri = vala_sql::testing::catalog_uri(&pool);
    let backend = BackendConfig::Local {
        root: tmp.path().to_path_buf(),
    };
    let (factory, props) = iceberg_storage_factory(&backend).unwrap();

    let catalog = WyrdCatalog::new(&catalog_uri, &warehouse, pool.clone(), factory, props)
        .await
        .unwrap();

    let ns = BifrostNamespace::Bifrost;
    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&pool, tenant.as_uuid())
        .await
        .expect("seed bench tenant");

    catalog
        .create_table(
            ns,
            "bench_wl",
            vec![
                arrow::datatypes::Field::new("id", arrow::datatypes::DataType::Int64, false),
                arrow::datatypes::Field::new("payload", arrow::datatypes::DataType::Utf8, false),
            ],
            TableScope::TenantOwned,
            tenant,
            &[],
        )
        .await
        .unwrap();

    let batch_count = usize::try_from(args.rows)
        .expect("rows fits usize")
        .div_ceil(args.batch_size);
    let mut commit_count = 0;
    let mut durations_ms = Vec::with_capacity(batch_count);

    for i in 0..batch_count {
        let n = if i + 1 == batch_count {
            let rem = usize::try_from(args.rows).expect("rows fits usize") % args.batch_size;
            if rem == 0 { args.batch_size } else { rem }
        } else {
            args.batch_size
        };

        let batch = make_batch(n);
        let writer = catalog
            .writer(ns, "bench_wl", TableScope::TenantOwned, tenant)
            .await
            .unwrap();
        writer.write(batch).await.unwrap();

        let t0 = std::time::Instant::now();
        writer.flush().await.unwrap();
        durations_ms.push(t0.elapsed().as_secs_f64() * 1_000.0);
        commit_count += 1;
    }

    durations_ms.sort_by(f64::total_cmp);
    let p50_ms = percentile(&durations_ms, 0.50);
    let p95_ms = percentile(&durations_ms, 0.95);
    let p99_ms = percentile(&durations_ms, 0.99);

    let report = WorkloadReport {
        workload: format!("{:?}", args.workload).to_lowercase(),
        rows: args.rows,
        batch_size: args.batch_size,
        files_total: commit_count,
        files_pruned: 0,
        bytes_total: 0,
        bytes_scanned: 0,
        p50_ms,
        p95_ms,
        p99_ms,
        iceberg_snapshots: commit_count,
        commit_count,
    };

    let json = serde_json::to_string_pretty(&report).unwrap();
    std::fs::write(&args.output, &json).unwrap();
    println!("{json}");
}

fn make_batch(n: usize) -> arrow::record_batch::RecordBatch {
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;

    // User fields only — the write path server-stamps every system column at flush.
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("payload", DataType::Utf8, false),
    ]));

    let ids: Int64Array = (0..i64::try_from(n).expect("n fits i64")).collect();
    let payloads: StringArray = (0..n).map(|i| Some(format!("p{i}"))).collect();

    arrow::record_batch::RecordBatch::try_new(schema, vec![Arc::new(ids), Arc::new(payloads)])
        .unwrap()
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let idx = ((sorted.len() as f64 * p) as usize).min(sorted.len() - 1);
    sorted[idx]
}
