use std::sync::Arc;
use wyrd_storage::{BackendSigner, StorageHandle};

const ENV_KEYS: &[&str] = &[
    "WYRD_STORAGE_BACKEND",
    "WYRD_STORAGE_REQUIRE_ENCRYPTION",
    "WYRD_STORAGE_PRESIGN_TTL_SECS",
    "WYRD_STORAGE_PART_SIZE_BYTES",
    "WYRD_STORAGE_LOCAL_ROOT",
    "WYRD_PUBLIC_BASE_URL",
];

#[tokio::test]
async fn builds_local_handle_from_settings() {
    let (handle, _root) = Box::pin(local_handle()).await;

    assert!(matches!(handle.signer(), BackendSigner::Local(_)));
    assert_eq!(
        handle.backend(),
        wyrd_spec::storage::StorageBackendKind::Local
    );
    assert_eq!(handle.presign_ttl_secs(), 600);
    assert_eq!(handle.default_part_size_bytes(), 16 * 1024 * 1024);
    assert_eq!(handle.public_base_url(), Some("https://wyrd.test"));
}

async fn local_handle() -> (Arc<StorageHandle>, tempfile::TempDir) {
    let root = tempfile::tempdir().expect("temp dir");
    let vars = vec![
        ("WYRD_STORAGE_BACKEND", Some("local".to_owned())),
        (
            "WYRD_STORAGE_LOCAL_ROOT",
            Some(root.path().display().to_string()),
        ),
        ("WYRD_PUBLIC_BASE_URL", Some("https://wyrd.test".to_owned())),
    ];
    let provided = vars.iter().map(|(key, _)| *key).collect::<Vec<_>>();
    let mut all = ENV_KEYS
        .iter()
        .filter(|key| !provided.contains(key))
        .map(|key| (*key, None))
        .collect::<Vec<(&'static str, Option<String>)>>();
    all.extend(vars);

    let handle = Box::pin(temp_env::async_with_vars(all, async {
        let settings = wyrd_storage::settings::from_env().expect("settings parse");
        StorageHandle::from_settings(settings)
            .await
            .expect("local handle")
    }))
    .await;

    (handle, root)
}
