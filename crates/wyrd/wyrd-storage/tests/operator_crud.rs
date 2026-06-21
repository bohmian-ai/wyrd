//! Cross-backend opendal `Operator` CRUD suite.
//!
//! [`run_operator_crud`] is the single body exercised against every backend.
//! Local always runs. S3/GCS/Azure each have two entrypoints that differ only
//! in how the `Operator` is built:
//!
//! * `*_emu` — gated on `WYRD_STORAGE_INTEGRATION_{S3,GCS,AZURE}=1`, targets the
//!   local docker emulator (`RustFS` / fake-gcs / Azurite). Runs on any CI.
//! * `*_cloud` — gated on `WYRD_STORAGE_CLOUD_{S3,GCS,AZURE}=1`, builds the
//!   production operator from `WYRD_STORAGE_*` env + ambient credentials. Runs
//!   only on merge-to-main CI.
//!
//! The assertions are identical across every backend and lane.

use opendal::{EntryMode, ErrorKind, Operator, services};
use wyrd_storage::factory::build_operator;
use wyrd_storage::settings::{AzureConfig, BackendConfig, GcsConfig, S3Config};

fn enabled(var: &str) -> bool {
    std::env::var(var).as_deref() == Ok("1")
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| default.to_owned())
}

fn unique_prefix() -> String {
    format!("test/{}", uuid::Uuid::now_v7())
}

/// Write/read/stat/list/delete round-trip plus a missing-key `NotFound`.
async fn run_operator_crud(op: &Operator) {
    let prefix = unique_prefix();

    op.write(&format!("{prefix}/a.bin"), b"aaa" as &[u8])
        .await
        .expect("write a");
    op.write(&format!("{prefix}/b.bin"), b"bbb" as &[u8])
        .await
        .expect("write b");
    op.write(&format!("{prefix}/c.bin"), b"ccc" as &[u8])
        .await
        .expect("write c");

    let meta = op.stat(&format!("{prefix}/a.bin")).await.expect("stat a");
    assert_eq!(meta.content_length(), 3);

    let buf = op.read(&format!("{prefix}/a.bin")).await.expect("read a");
    assert_eq!(buf.to_bytes().as_ref(), b"aaa");

    let entries = op
        .list_with(&format!("{prefix}/"))
        .recursive(true)
        .await
        .expect("list prefix");
    let file_count = entries
        .iter()
        .filter(|e| e.metadata().mode() == EntryMode::FILE)
        .count();
    assert_eq!(file_count, 3, "expected 3 files under prefix");

    op.delete(&format!("{prefix}/a.bin"))
        .await
        .expect("delete a");
    let err = op
        .stat(&format!("{prefix}/a.bin"))
        .await
        .expect_err("stat after delete");
    assert_eq!(err.kind(), ErrorKind::NotFound);

    let err = op
        .stat(&format!("{prefix}/never-written.bin"))
        .await
        .expect_err("stat missing");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

// ---- Operator builders -----------------------------------------------------

fn s3_emu_op() -> Operator {
    Operator::new(
        services::S3::default()
            .bucket(&env_or("WYRD_STORAGE_S3_BUCKET", "wyrd-storage-test"))
            .endpoint(&env_or("WYRD_S3_EMULATOR_ENDPOINT", "http://localhost:9000"))
            .region(&env_or("WYRD_STORAGE_S3_REGION", "us-east-1"))
            .access_key_id(&env_or("WYRD_S3_EMULATOR_ACCESS_KEY", "wyrd-test-key"))
            .secret_access_key(&env_or("WYRD_S3_EMULATOR_SECRET_KEY", "wyrd-test-secret")),
    )
    .expect("build S3 emulator operator")
    .finish()
}

fn gcs_emu_op() -> Operator {
    Operator::new(
        services::Gcs::default()
            .bucket(&env_or("WYRD_STORAGE_GCS_BUCKET", "wyrd-storage-test"))
            .endpoint(&env_or("WYRD_GCS_EMULATOR_HOST", "http://localhost:4443"))
            .skip_signature(),
    )
    .expect("build GCS emulator operator")
    .finish()
}

fn azure_emu_op() -> Operator {
    const AZURITE_KEY: &str = "Eby8vdM02xNOcqFlqUwJPLlmEtlCDXJ1OUzFT50uSRZ6IFsuFq2UVErCz4I6tq/K1SZFPTOtr/KBHBeksoGMGw==";
    let base = env_or("WYRD_AZURE_EMULATOR_ENDPOINT", "http://127.0.0.1:10000");
    Operator::new(
        services::Azblob::default()
            .endpoint(&format!("{base}/devstoreaccount1"))
            .account_name("devstoreaccount1")
            .account_key(AZURITE_KEY)
            .container(&env_or("WYRD_STORAGE_AZURE_CONTAINER", "wyrd-storage-test")),
    )
    .expect("build Azurite operator")
    .finish()
}

fn cloud_s3_op() -> Operator {
    build_operator(&BackendConfig::S3(S3Config {
        bucket: std::env::var("WYRD_STORAGE_S3_BUCKET").expect("WYRD_STORAGE_S3_BUCKET"),
        region: std::env::var("WYRD_STORAGE_S3_REGION").ok(),
        endpoint_url: std::env::var("WYRD_STORAGE_S3_ENDPOINT_URL").ok(),
        force_path_style: false,
    }))
    .expect("build cloud S3 operator")
}

fn cloud_gcs_op() -> Operator {
    build_operator(&BackendConfig::Gcs(GcsConfig {
        bucket: std::env::var("WYRD_STORAGE_GCS_BUCKET").expect("WYRD_STORAGE_GCS_BUCKET"),
    }))
    .expect("build cloud GCS operator")
}

fn cloud_azure_op() -> Operator {
    build_operator(&BackendConfig::Azure(AzureConfig {
        account: std::env::var("WYRD_STORAGE_AZURE_ACCOUNT").expect("WYRD_STORAGE_AZURE_ACCOUNT"),
        container: std::env::var("WYRD_STORAGE_AZURE_CONTAINER")
            .expect("WYRD_STORAGE_AZURE_CONTAINER"),
    }))
    .expect("build cloud Azure operator")
}

// ---- Entrypoints -----------------------------------------------------------

#[tokio::test]
async fn local_operator_crud() {
    let dir = tempfile::tempdir().expect("tempdir");
    let op = build_operator(&BackendConfig::Local {
        root: dir.path().to_path_buf(),
    })
    .expect("build local operator");
    run_operator_crud(&op).await;
}

#[tokio::test]
async fn s3_emu_operator_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_S3") {
        eprintln!("skipping S3 operator emulator test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return;
    }
    run_operator_crud(&s3_emu_op()).await;
}

#[tokio::test]
async fn s3_cloud_operator_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_S3") {
        eprintln!("skipping S3 operator cloud test; set WYRD_STORAGE_CLOUD_S3=1");
        return;
    }
    run_operator_crud(&cloud_s3_op()).await;
}

#[tokio::test]
async fn gcs_emu_operator_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_GCS") {
        eprintln!("skipping GCS operator emulator test; set WYRD_STORAGE_INTEGRATION_GCS=1");
        return;
    }
    run_operator_crud(&gcs_emu_op()).await;
}

#[tokio::test]
async fn gcs_cloud_operator_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_GCS") {
        eprintln!("skipping GCS operator cloud test; set WYRD_STORAGE_CLOUD_GCS=1");
        return;
    }
    run_operator_crud(&cloud_gcs_op()).await;
}

#[tokio::test]
async fn azure_emu_operator_crud() {
    if !enabled("WYRD_STORAGE_INTEGRATION_AZURE") {
        eprintln!("skipping Azure operator emulator test; set WYRD_STORAGE_INTEGRATION_AZURE=1");
        return;
    }
    run_operator_crud(&azure_emu_op()).await;
}

#[tokio::test]
async fn azure_cloud_operator_crud() {
    if !enabled("WYRD_STORAGE_CLOUD_AZURE") {
        eprintln!("skipping Azure operator cloud test; set WYRD_STORAGE_CLOUD_AZURE=1");
        return;
    }
    run_operator_crud(&cloud_azure_op()).await;
}
