//! GCS opendal-operator CRUD tests against the fake-gcs-server emulator.
//!
//! Gate: `WYRD_STORAGE_INTEGRATION_GCS=1`. Requires `storage:up`.

use opendal::{EntryMode, ErrorKind, Operator, services};

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_GCS").as_deref() != Ok("1") {
        eprintln!("skipping GCS operator emulator test; set WYRD_STORAGE_INTEGRATION_GCS=1");
        return true;
    }
    false
}

fn emulator_host() -> String {
    std::env::var("WYRD_GCS_EMULATOR_HOST")
        .unwrap_or_else(|_| "http://localhost:4443".to_owned())
}

fn build_op() -> Operator {
    Operator::new(
        services::Gcs::default()
            .bucket("wyrd-storage-test")
            .endpoint(&emulator_host())
            .skip_signature(),
    )
    .expect("build GCS emulator operator")
    .finish()
}

fn unique_prefix() -> String {
    format!("test/{}", uuid::Uuid::now_v7())
}

#[tokio::test]
async fn gcs_emu_write_read_stat_list_delete() {
    if skip_unless_enabled() {
        return;
    }
    let op = build_op();
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
}

#[tokio::test]
async fn gcs_emu_stat_missing_is_not_found() {
    if skip_unless_enabled() {
        return;
    }
    let op = build_op();
    let err = op
        .stat(&format!("missing/{}", uuid::Uuid::now_v7()))
        .await
        .expect_err("stat missing");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}
