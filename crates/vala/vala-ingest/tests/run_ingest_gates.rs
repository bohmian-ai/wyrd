//! Gate tests for `run_ingest`: RBAC denial and system-table write denial.
//!
//! Both gates fire before the catalog is accessed (RBAC before `collect_frames`,
//! system-table after `collect_frames` but before `catalog.writer`), so neither
//! test requires a pre-created table — only a valid catalog instance.
//!
//! Requires a live Postgres + writable warehouse; gated behind
//! `BIFROST_TEST_DB_URL` and `#[ignore]`. Run with:
//! `BIFROST_TEST_DB_URL=postgres://... cargo test -p vala-ingest --all-features run_ingest_gates -- --ignored`

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use vala_ingest::InsertBatchRequest;
use vala_ingest::auth::AuthContext;
use vala_ingest::error::IngestError;
use vala_ingest::limits::IngestLimits;
use vala_ingest::orchestrator::{FrameSource, run_ingest};
use wyrd_runtime::{Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::{CardRef, CardRefScope};
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::Status;

const CARD: &str = "prod/Service/billing@1.0.0";

struct VecSource {
    frames: VecDeque<Result<InsertBatchRequest, Status>>,
}

impl FrameSource for VecSource {
    async fn next_frame(&mut self) -> Result<Option<InsertBatchRequest>, Status> {
        self.frames.pop_front().transpose()
    }
}

fn minimal_ipc_batch() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("val", DataType::Int64, false),
        Field::new("card_ref", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64])),
            Arc::new(StringArray::from(vec![CARD])),
        ],
    )
    .expect("batch builds");
    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, &schema).expect("writer");
        writer.write(&batch).expect("write");
        writer.finish().expect("finish");
    }
    buffer
}

fn source(table: &str) -> VecSource {
    VecSource {
        frames: VecDeque::from(vec![Ok(InsertBatchRequest {
            table: table.to_owned(),
            arrow_ipc: minimal_ipc_batch(),
            wyrd_batch_id: uuid::Uuid::now_v7().as_bytes().to_vec(),
        })]),
    }
}

fn auth_with_permissions(tenant: DataTenantId, perms: PermissionSet) -> AuthContext {
    let card = CardRef::from_str(CARD).expect("card parses");
    AuthContext {
        principal: Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: card.clone(),
                card_ref_scope: CardRefScope::own(&card),
            },
            tenant,
            Vec::new(),
            perms,
        ),
        tenant,
        request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string()).expect("request id"),
    }
}

async fn catalog_for(db_url: &str) -> vala_bifrost::WyrdCatalog {
    let tmp = tempfile::tempdir().unwrap();
    let warehouse = format!("file://{}", tmp.path().display());
    let pool = Arc::new(sqlx::PgPool::connect(db_url).await.unwrap());
    let backend = wyrd_storage::settings::BackendConfig::Local {
        root: tmp.path().to_path_buf(),
    };
    let factory =
        wyrd_storage::factory::iceberg_factory::iceberg_storage_factory(&backend).unwrap();
    vala_bifrost::WyrdCatalog::new(db_url, &warehouse, pool, None, factory.0, factory.1)
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires BIFROST_TEST_DB_URL; run with --ignored"]
async fn run_ingest_rbac_denial_rejects_principal_without_record_write() {
    let db_url = std::env::var("BIFROST_TEST_DB_URL")
        .expect("BIFROST_TEST_DB_URL must be set to run this test");
    let catalog = catalog_for(&db_url).await;
    let tenant = DataTenantId::new_v7();
    let auth = auth_with_permissions(tenant, PermissionSet::new());
    let limits = IngestLimits::default();

    let err = run_ingest(&catalog, &limits, &auth, source("vala.bifrost.gate_test"))
        .await
        .expect_err("principal without bifrost_record_write is rejected");
    assert!(
        matches!(err, IngestError::RbacDenied { .. }),
        "expected RbacDenied, got: {err:?}"
    );
    assert_eq!(err.wyrd_code(), "WYRD_PERMISSION_403_DENIED_RBAC");
}

#[tokio::test]
#[ignore = "requires BIFROST_TEST_DB_URL; run with --ignored"]
async fn run_ingest_system_table_write_denied() {
    let db_url = std::env::var("BIFROST_TEST_DB_URL")
        .expect("BIFROST_TEST_DB_URL must be set to run this test");
    let catalog = catalog_for(&db_url).await;
    let tenant = DataTenantId::new_v7();
    let auth = auth_with_permissions(
        tenant,
        PermissionSet::from_iter([Permission::bifrost_record_write()]),
    );
    let limits = IngestLimits::default();

    let err = run_ingest(&catalog, &limits, &auth, source("vala.system.audit_log"))
        .await
        .expect_err("write to vala.system.* is denied");
    assert!(
        matches!(err, IngestError::SystemTableWriteDenied { .. }),
        "expected SystemTableWriteDenied, got: {err:?}"
    );
    assert_eq!(err.wyrd_code(), "WYRD_VALA_403_BIFROST_SYSTEM_TABLE_WRITE");
}
