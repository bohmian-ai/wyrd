mod pg_tests {
    //! Durable idempotency: a replayed stream (same `wyrd_batch_id`) produces no
    //! double-write. Both the first ingest and its replay return the batch row
    //! count, and the durable `vala.olap_commits` anchor holds exactly one
    //! committed row for the batch — a second append would surface as a second
    //! committed anchor, so the row count alone is not a sufficient assertion.
    //!
    //! Runs against embedded Postgres via `PgFixture` (no external database, no
    //! `#[ignore]`, no env gating); part of the SQL-backed test lane
    //! (`mise run test:sql`).

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
    use wyrd_dev_fixtures::pg::PgFixture;
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
                Arc::new(StringArray::from(vec![
                    run_id.clone(),
                    run_id.clone(),
                    run_id,
                ])),
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
                frame_sequence: 0,
            })]),
        }
    }

    fn auth_ctx(tenant: DataTenantId) -> AuthContext {
        let mut card = CardRef::from_str(CARD).expect("card parses");
        // Stamp a UID so stamp_correlation_columns can resolve card_uid from the
        // per-row card_ref column — requires uid to be present (M-11 fail-closed).
        card.uid = Some(wyrd_spec::ids::CardUid::from_uuid(uuid::Uuid::now_v7()).expect("v7 uid"));
        AuthContext {
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::Service {
                    card_ref: card.clone(),
                    card_ref_scope: CardRefScope::own(&card),
                },
                tenant,
                Vec::new(),
                PermissionSet::from_iter([Permission::bifrost_record_write()]),
            ),
            tenant,
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string()).expect("request id"),
        }
    }

    #[tokio::test]
    async fn duplicate_batch_id_replays_without_double_write() {
        let fixture = PgFixture::start().await.expect("fixture");
        let catalog_uri = fixture.catalog_uri();
        let pool = Arc::new(fixture.app_pool().clone());

        let tmp = tempfile::tempdir().unwrap();
        let backend = wyrd_storage::settings::BackendConfig::Local {
            root: tmp.path().to_path_buf(),
        };
        let catalog = vala_bifrost::WyrdCatalog::new(&catalog_uri, &backend, pool.clone(), None)
            .await
            .unwrap();

        let tenant = DataTenantId::new_v7();
        fixture
            .seed_additional_tenant_with_uuid(
                tenant,
                &format!("test-{}", tenant.as_uuid().simple()),
            )
            .await
            .unwrap();

        catalog
            .create_table(vala_bifrost::catalog::CreateTableRequest {
                ns: BifrostNamespace::Bifrost,

                name: "dup_test",

                user_fields: vec![Field::new("val", DataType::Int64, false)],

                scope: TableScope::TenantOwned,

                tenant,

                partition_columns: &[],

                audit: None,
            })
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

        // Durable single-write: the replay must not create a second commit
        // anchor. Count committed rows for this batch on the BYPASSRLS admin
        // pool — a double-append would surface here as a second committed row,
        // which the returned row count alone would not catch.
        let committed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM vala.olap_commits WHERE batch_id = $1 AND state = 'committed'",
        )
        .bind(batch_id.as_slice())
        .fetch_one(fixture.platform_admin_pool())
        .await
        .expect("count committed anchors");
        assert_eq!(
            committed, 1,
            "exactly one committed olap_commits anchor for the replayed batch"
        );
    }
}
