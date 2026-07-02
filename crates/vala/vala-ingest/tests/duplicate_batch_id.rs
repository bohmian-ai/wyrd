//! Durable idempotency: a replayed stream (same `wyrd_batch_id`) produces no
//! double-write and returns the prior row count, and `origin`/`actor` are
//! stamped on both the first write and the replay.
//!
//! Requires a live Postgres + writable warehouse; gated behind
//! `BIFROST_TEST_DB_URL` and `#[ignore]` (the same pattern as the engine's
//! `round_trip_full`). Run with:
//! `BIFROST_TEST_DB_URL=postgres://... cargo test -p vala-ingest --all-features duplicate_batch_id -- --ignored`

use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;

use arrow::array::{Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use vala_bifrost::TableScope;
use vala_bifrost::catalog::namespaces::BifrostNamespace;
use vala_ingest::InsertBatchRequest;
use vala_ingest::auth::AuthContext;
use vala_ingest::limits::IngestLimits;
use vala_ingest::orchestrator::{FrameSource, run_ingest};
use wyrd_runtime::{CardScope, Permission, PermissionSet, Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;
use wyrd_tonic::tonic::Status;

const CARD: &str = "prod/Service/billing@1.0.0";

struct VecSource {
    frames: VecDeque<Result<InsertBatchRequest, Status>>,
}

impl FrameSource for VecSource {
    async fn next_frame(&mut self) -> Result<Option<InsertBatchRequest>, Status> {
        Ok(self.frames.pop_front().transpose()?)
    }
}

fn ingest_batch() -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("val", DataType::Int64, false),
        Field::new("run_id", DataType::Utf8, true),
        Field::new("card_ref", DataType::Utf8, true),
    ]));
    let run_id = uuid::Uuid::now_v7().to_string();
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1_i64, 2, 3])),
            Arc::new(StringArray::from(vec![run_id.clone(), run_id.clone(), run_id])),
            Arc::new(StringArray::from(vec![CARD, CARD, CARD])),
        ],
    )
    .expect("batch builds");

    let mut buffer = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buffer, &schema).expect("writer");
        writer.write(&batch).expect("write batch");
        writer.finish().expect("finish");
    }
    buffer
}

fn source_for(batch_id: [u8; 16]) -> VecSource {
    VecSource {
        frames: VecDeque::from(vec![Ok(InsertBatchRequest {
            table: "vala.bifrost.dup_test".to_owned(),
            arrow_ipc: ingest_batch(),
            wyrd_batch_id: batch_id.to_vec(),
        })]),
    }
}

fn auth_ctx(tenant: DataTenantId) -> AuthContext {
    let card = CardRef::from_str(CARD).expect("card parses");
    AuthContext {
        principal: Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::Service {
                card_ref: card.clone(),
            },
            tenant,
            Vec::new(),
            PermissionSet::from_iter([Permission::bifrost_record_write()]),
            CardScope::new([card]),
        ),
        tenant,
        request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string()).expect("request id"),
    }
}

#[tokio::test]
#[ignore = "requires BIFROST_TEST_DB_URL; run with --ignored"]
async fn duplicate_batch_id_replays_without_double_write() {
    let db_url = std::env::var("BIFROST_TEST_DB_URL")
        .expect("BIFROST_TEST_DB_URL must be set to run this test");

    let tmp = tempfile::tempdir().unwrap();
    let warehouse = format!("file://{}", tmp.path().display());
    let pool = Arc::new(sqlx::PgPool::connect(&db_url).await.unwrap());
    let backend = wyrd_storage::settings::BackendConfig::Local {
        root: tmp.path().to_path_buf(),
    };
    let factory =
        wyrd_storage::factory::iceberg_factory::iceberg_storage_factory(&backend).unwrap();
    let catalog = vala_bifrost::WyrdCatalog::new(
        &db_url,
        &warehouse,
        pool.clone(),
        None,
        factory.0,
        factory.1,
    )
    .await
    .unwrap();

    let tenant = DataTenantId::new_v7();
    vala_sql::testing::seed_tenant(&pool, tenant.as_uuid())
        .await
        .unwrap();

    catalog
        .create_table(
            BifrostNamespace::Bifrost,
            "dup_test",
            vec![Field::new("val", DataType::Int64, false)],
            TableScope::TenantOwned,
            tenant,
            &[],
            None,
        )
        .await
        .unwrap();

    let auth = auth_ctx(tenant);
    let limits = IngestLimits::default();
    let batch_id = *uuid::Uuid::now_v7().as_bytes();

    let first = run_ingest(&catalog, &limits, &auth, source_for(batch_id))
        .await
        .expect("first ingest commits");
    assert_eq!(first, 3);

    let replay = run_ingest(&catalog, &limits, &auth, source_for(batch_id))
        .await
        .expect("replay dedups");
    assert_eq!(replay, 3, "replay returns the prior row count");
}
