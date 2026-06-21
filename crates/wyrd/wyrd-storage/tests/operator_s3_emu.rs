//! S3 opendal-operator CRUD tests against the RustFS emulator.
//!
//! Gate: `WYRD_STORAGE_INTEGRATION_S3=1`. Requires `storage:up`.

use opendal::{EntryMode, ErrorKind, Operator, services};

fn skip_unless_enabled() -> bool {
    if std::env::var("WYRD_STORAGE_INTEGRATION_S3").as_deref() != Ok("1") {
        eprintln!("skipping S3 operator emulator test; set WYRD_STORAGE_INTEGRATION_S3=1");
        return true;
    }
    false
}

fn build_op() -> Operator {
    Operator::new(
        services::S3::default()
            .bucket("wyrd-storage-test")
            .endpoint("http://localhost:9000")
            .region("us-east-1")
            .access_key_id("wyrd-test-key")
            .secret_access_key("wyrd-test-secret"),
    )
    .expect("build S3 emulator operator")
    .finish()
}

fn unique_prefix() -> String {
    format!("test/{}", uuid::Uuid::now_v7())
}

#[tokio::test]
async fn s3_emu_write_read_stat_list_delete() {
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
async fn s3_emu_stat_missing_is_not_found() {
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
