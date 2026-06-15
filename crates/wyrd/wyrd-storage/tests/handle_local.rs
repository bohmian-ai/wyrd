use std::sync::Arc;
use wyrd_spec::DataTenantId;
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
    let (handle, _root) = local_handle().await;

    assert!(matches!(handle.signer(), BackendSigner::Local(_)));
    assert_eq!(
        handle.backend(),
        wyrd_spec::storage::StorageBackendKind::Local
    );
    assert_eq!(handle.presign_ttl_secs(), 600);
    assert_eq!(handle.default_part_size_bytes(), 16 * 1024 * 1024);
    assert_eq!(handle.public_base_url(), Some("https://wyrd.test"));
    let store_a = handle.object_store();
    let store_b = handle.object_store();
    assert!(Arc::ptr_eq(&store_a, &store_b));
}

#[tokio::test]
async fn object_store_for_accepts_local_tenant_uri() {
    let (handle, _root) = local_handle().await;
    let tenant = DataTenantId::new_v7();
    let uri = format!(
        "file://localhost/{tenant}/cards/{}/x.parquet",
        uuid::Uuid::now_v7()
    );

    let store = handle
        .object_store_for(&uri, tenant)
        .expect("tenant uri should resolve to shared object store");

    assert!(Arc::ptr_eq(&store, &handle.object_store()));
}

#[tokio::test]
async fn object_store_for_rejects_foreign_tenant_uri() {
    let (handle, _root) = local_handle().await;
    let caller = DataTenantId::new_v7();
    let foreign = DataTenantId::new_v7();
    let uri = format!(
        "file://localhost/{foreign}/cards/{}/x.parquet",
        uuid::Uuid::now_v7()
    );

    let error = match handle.object_store_for(&uri, caller) {
        Ok(_) => panic!("foreign tenant uri should be rejected"),
        Err(error) => error,
    };

    assert!(matches!(
        error,
        wyrd_storage::StorageError::TenantPrefixForeign { .. }
    ));
}

#[tokio::test]
async fn object_store_for_returns_shared_arc_for_same_tenant() {
    let (handle, _root) = local_handle().await;
    let tenant = DataTenantId::new_v7();
    let uri_a = format!(
        "file://localhost/{tenant}/cards/{}/a.parquet",
        uuid::Uuid::now_v7()
    );
    let uri_b = format!(
        "file://localhost/{tenant}/cards/{}/b.parquet",
        uuid::Uuid::now_v7()
    );

    let store_a = handle
        .object_store_for(&uri_a, tenant)
        .expect("first tenant uri should resolve");
    let store_b = handle
        .object_store_for(&uri_b, tenant)
        .expect("second tenant uri should resolve");

    assert!(Arc::ptr_eq(&store_a, &store_b));
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

    let handle = temp_env::async_with_vars(all, async {
        let settings = wyrd_storage::settings::from_env().expect("settings parse");
        StorageHandle::from_settings(settings)
            .await
            .expect("local handle")
    })
    .await;

    (handle, root)
}
