use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use vala_bifrost::WyrdCatalog;
use vala_bifrost_redux::contracts::{FrameAdmission, Scribe, ScribeError, ScribeIngressFrame};
use vala_ingest::auth::AuthContext;
use vala_ingest::error::IngestError;
use vala_ingest::limits::IngestLimits;
use vala_ingest::orchestrator::run_ingest_frame_to_scribe;
use wyrd_dev_fixtures::pg::PgFixture;
use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::wyrd::v1::InsertBatchRequest;

struct FailingScribe;

#[async_trait]
impl Scribe for FailingScribe {
    async fn ingest_frame(
        &self,
        _frame: ScribeIngressFrame,
    ) -> Result<FrameAdmission, ScribeError> {
        panic!("Scribe must not be called by a rejected frame gate")
    }
}

fn auth(tenant: DataTenantId, permissions: PermissionSet) -> AuthContext {
    AuthContext {
        principal: Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::User,
            tenant,
            Vec::new(),
            permissions,
        ),
        tenant,
        request_id: RequestId::now_v7(),
    }
}

fn frame(table: &str) -> InsertBatchRequest {
    InsertBatchRequest {
        table: table.to_owned(),
        arrow_ipc: Bytes::new().to_vec(),
        wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
        frame_sequence: 0,
    }
}

async fn catalog(fixture: &PgFixture) -> (WyrdCatalog, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("catalog object-store root");
    let backend = wyrd_storage::settings::BackendConfig::Local {
        root: temp.path().to_path_buf(),
    };
    let catalog = WyrdCatalog::new(
        &fixture.catalog_uri(),
        &backend,
        Arc::new(fixture.app_pool().clone()),
        None,
    )
    .await
    .expect("catalog");
    (catalog, temp)
}

#[tokio::test]
#[ignore = "requires WYRD_DATABASE_URL and WYRD_DATABASE_MIGRATOR_PASSWORD"]
async fn frame_gate_denies_rbac_before_catalog_or_scribe() {
    let fixture = PgFixture::start().await.expect("embedded postgres");
    let (catalog, _temp) = catalog(&fixture).await;
    let tenant = DataTenantId::new_v7();
    let error = run_ingest_frame_to_scribe(
        &catalog,
        &FailingScribe,
        &IngestLimits::default(),
        &auth(tenant, PermissionSet::new()),
        frame("vala.bifrost.gate_test"),
        0,
    )
    .await
    .expect_err("missing bifrost write permission");
    assert!(matches!(error, IngestError::RbacDenied { .. }));
    assert_eq!(error.wyrd_code(), "WYRD_PERMISSION_403_DENIED_RBAC");
}

#[tokio::test]
#[ignore = "requires WYRD_DATABASE_URL and WYRD_DATABASE_MIGRATOR_PASSWORD"]
async fn frame_gate_denies_system_table_before_catalog_or_scribe() {
    let fixture = PgFixture::start().await.expect("embedded postgres");
    let (catalog, _temp) = catalog(&fixture).await;
    let tenant = DataTenantId::new_v7();
    let error = run_ingest_frame_to_scribe(
        &catalog,
        &FailingScribe,
        &IngestLimits::default(),
        &auth(
            tenant,
            PermissionSet::from_iter([Permission::bifrost_record_write()]),
        ),
        frame("vala.system.audit_log"),
        0,
    )
    .await
    .expect_err("system table write");
    assert!(matches!(error, IngestError::SystemTableWriteDenied { .. }));
    assert_eq!(
        error.wyrd_code(),
        "WYRD_VALA_403_BIFROST_SYSTEM_TABLE_WRITE"
    );
}
