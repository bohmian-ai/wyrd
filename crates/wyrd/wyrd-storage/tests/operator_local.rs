use opendal::{EntryMode, ErrorKind};
use wyrd_storage::factory::build_operator;
use wyrd_storage::settings::BackendConfig;

fn unique_prefix() -> String {
    format!("test/{}", uuid::Uuid::now_v7())
}

#[tokio::test]
async fn local_write_read_stat_list_delete() {
    let dir = tempfile::tempdir().expect("tempdir");
    let op = build_operator(&BackendConfig::Local {
        root: dir.path().to_path_buf(),
    })
    .expect("build local operator");
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
async fn local_stat_missing_is_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let op = build_operator(&BackendConfig::Local {
        root: dir.path().to_path_buf(),
    })
    .expect("build local operator");
    let err = op
        .stat("never-written.bin")
        .await
        .expect_err("stat missing");
    assert_eq!(err.kind(), ErrorKind::NotFound);
}
