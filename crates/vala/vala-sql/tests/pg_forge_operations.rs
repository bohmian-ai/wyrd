mod pg_tests {
    //! SQL integration tests for `vala.forge_operation_state`.
    //!
    //! Covers the migration schema, advisory-lock serialization, transition
    //! matrix, idempotent replay, `Reset -> Prepared` reopen, cap-bound reads
    //! with overflow sentinel, RLS isolation, and constant-open-set query
    //! planning with large terminal history.
    //! Run via `mise run test:sql`.

    mod forge_operations {
        //! Exercises the concrete Forge SQL owner against real PostgreSQL.
        //!
        //! The tests use tenant transactions for public workflows and the
        //! migrator fixture only to inspect catalogs or arrange deliberately
        //! corrupt projection state that ordinary writers cannot create.

        use sqlx::types::Uuid;
        use sqlx::{AssertSqlSafe, PgPool};
        use wyrd_dev_fixtures::pg::PgFixture;
        use wyrd_spec::DataTenantId;
        use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
        use wyrd_spec::request_id::RequestId;
        use wyrd_spec::vala::api::{
            AuditDetail, AuditEvent, AuditOutcome, ForgeIcebergRewritePhase, ForgeOrphanGcPhase,
            ForgeSnapshotExpirePhase, StoragePath,
        };

        use vala_sql::queries::forge_operations::ForgeOperations;
        use vala_sql::row_types::forge_operations::{
            ForgeClaimTable, ForgeExpirationAuthority, ForgeExpirationPreparation,
            ForgeExpirationResetOutcome, ForgeExpirationResetRequest, ForgeExpirationSettlement,
            ForgeExpirationSettlementRequest, ForgeOperationFamily, ForgeOperationTransition,
        };
        use vala_sql::row_types::forge_tasks::ForgeTaskEvidence;
        use vala_sql::{SqlError, TenantConn};

        // -----------------------------------------------------------------------
        // Fixture and helpers
        // -----------------------------------------------------------------------

        /// Fixture handles shared by one isolated Forge SQL test.
        struct TestFixtures {
            /// Isolated database and seeded tenant owner.
            fixture: PgFixture,
            /// Migrator-role pool for schema and plan assertions.
            superuser: PgPool,
        }

        /// Starts an isolated migrated Postgres fixture for one Forge SQL test.
        ///
        /// # Panics
        ///
        /// Panics when PostgreSQL setup or role-backed pool creation fails.
        async fn setup() -> TestFixtures {
            let fixture = PgFixture::start().await.expect("fixture");
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            TestFixtures { fixture, superuser }
        }

        /// Returns the primary logical Forge resource used by the tests.
        fn resource() -> &'static str {
            "tenant_a.ns.tbl"
        }

        /// Builds an audit event with the supplied operation and detail.
        fn event(operation: &str, resource: &str, detail: Option<AuditDetail>) -> AuditEvent {
            let mut event = AuditEvent::new(
                RequestId::now_v7(),
                None,
                operation.to_owned(),
                resource.to_owned(),
                None,
                PrincipalId::new(Uuid::now_v7()),
                PrincipalKindTag::User,
                "bifrost.forge".to_owned(),
                AuditOutcome::Allowed,
            );
            if let Some(d) = detail {
                event = event.with_detail(d);
            }
            event
        }

        /// Runs one Prepared transition and commits successful work.
        ///
        /// # Errors
        ///
        /// Returns the Forge transition error without committing when the
        /// public owner rejects the event or PostgreSQL IO fails.
        ///
        /// # Panics
        ///
        /// Panics when the fixture cannot acquire or commit its tenant
        /// transaction, or when the fixed resource cannot construct the owner.
        async fn append_prepared(
            pool: &PgPool,
            tenant: DataTenantId,
            resource: &str,
            family: ForgeOperationFamily,
            event: &AuditEvent,
        ) -> Result<ForgeOperationTransition, SqlError> {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for prepared transition");
            let ops = ForgeOperations::new(resource, family).expect("valid Forge resource");
            let result = match event.detail.as_ref() {
                Some(detail) => {
                    ops.append_prepared(&mut conn, &event.operation, detail)
                        .await
                }
                None => Err(SqlError::Conflict {
                    detail: "Forge transitions require typed detail".to_owned(),
                }),
            };
            if result.is_ok() {
                conn.commit().await.expect("prepared transition commit");
            }
            result
        }

        /// Runs one terminal transition and commits successful work.
        ///
        /// # Errors
        ///
        /// Returns the Forge transition error without committing when the
        /// public owner rejects the event or PostgreSQL IO fails.
        ///
        /// # Panics
        ///
        /// Panics when the fixture cannot acquire or commit its tenant
        /// transaction, or when the fixed resource cannot construct the owner.
        async fn append_terminal(
            pool: &PgPool,
            tenant: DataTenantId,
            resource: &str,
            family: ForgeOperationFamily,
            event: &AuditEvent,
        ) -> Result<ForgeOperationTransition, SqlError> {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for terminal transition");
            let ops = ForgeOperations::new(resource, family).expect("valid Forge resource");
            let result = match event.detail.as_ref() {
                Some(detail) => {
                    ops.append_terminal(&mut conn, &event.operation, detail)
                        .await
                }
                None => Err(SqlError::Conflict {
                    detail: "Forge transitions require typed detail".to_owned(),
                }),
            };
            if result.is_ok() {
                conn.commit().await.expect("terminal transition commit");
            }
            result
        }

        /// Counts projection rows for one tenant through a tenant transaction.
        ///
        /// # Panics
        ///
        /// Panics when the fixture cannot acquire, query, or commit the tenant
        /// transaction.
        async fn count_state(pool: &PgPool, tenant: DataTenantId) -> i64 {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for state count");
            let row: (i64,) = sqlx::query_as(
                "SELECT count(*) FROM vala.forge_operation_state WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("state count query");
            conn.commit().await.expect("state count commit");
            row.0
        }

        /// Counts audit rows for one tenant through a tenant transaction.
        ///
        /// # Panics
        ///
        /// Panics when the fixture cannot acquire, query, or commit the tenant
        /// transaction.
        async fn count_audit(pool: &PgPool, tenant: DataTenantId) -> i64 {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for audit count");
            let row: (i64,) =
                sqlx::query_as("SELECT count(*) FROM vala.audit_staging WHERE data_tenant_id = $1")
                    .bind(tenant.as_uuid())
                    .fetch_one(&mut **conn.transaction())
                    .await
                    .expect("audit count query");
            conn.commit().await.expect("audit count commit");
            row.0
        }

        /// Minimal persisted state used to assert transition and rollback effects.
        #[derive(Debug, PartialEq, Eq)]
        struct StateSnapshot {
            /// Current closed phase stored by the projection.
            phase: String,
        }

        /// Reads one operation's persisted phase.
        ///
        /// # Panics
        ///
        /// Panics when the tenant transaction or exact-row query fails.
        async fn state_snapshot(
            pool: &PgPool,
            tenant: DataTenantId,
            family: ForgeOperationFamily,
            operation_id: Uuid,
        ) -> StateSnapshot {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for state snapshot");
            let row: (String,) = sqlx::query_as(
                r#"
                SELECT phase
                  FROM vala.forge_operation_state
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND resource = $1
                   AND family = $2
                   AND operation_id = $3
                "#,
            )
            .bind(resource())
            .bind(family.as_str())
            .bind(operation_id)
            .fetch_one(&mut **conn.transaction())
            .await
            .expect("state snapshot query");
            conn.commit().await.expect("state snapshot commit");
            StateSnapshot { phase: row.0 }
        }

        /// Extracts the deterministic operation identity from a Forge detail.
        ///
        /// # Panics
        ///
        /// Panics when `detail` is not one of the four Forge detail variants.
        fn forge_operation_id(detail: &AuditDetail) -> Uuid {
            match detail {
                AuditDetail::ForgeScribePromotion { operation_id, .. }
                | AuditDetail::ForgeIcebergRewrite { operation_id, .. }
                | AuditDetail::ForgeSnapshotExpire { operation_id, .. }
                | AuditDetail::ForgeOrphanGc { operation_id, .. } => *operation_id,
                _ => panic!("expected Forge audit detail"),
            }
        }

        // -----------------------------------------------------------------------
        // Schema
        // -----------------------------------------------------------------------

        /// Verifies the complete ordered PostgreSQL catalog contract.
        ///
        /// # Panics
        ///
        /// Panics when fixture/catalog access fails or any column, primary-key,
        /// check, index, RLS policy, or grant differs from the migration.
        #[tokio::test]
        async fn migration_schema_is_exact() {
            let TestFixtures {
                fixture: _fixture,
                superuser,
            } = setup().await;

            let columns: Vec<(String, String, String)> = sqlx::query_as(
                r#"
                SELECT column_name, data_type, is_nullable
                FROM information_schema.columns
                WHERE table_schema = 'vala' AND table_name = 'forge_operation_state'
                ORDER BY ordinal_position
                "#,
            )
            .fetch_all(&superuser)
            .await
            .expect("column query");

            let expected = [
                ("data_tenant_id", "uuid", "NO"),
                ("resource", "text", "NO"),
                ("family", "text", "NO"),
                ("operation_id", "uuid", "NO"),
                ("phase", "text", "NO"),
                ("prepared_detail", "jsonb", "NO"),
                ("current_detail", "jsonb", "NO"),
                ("prepared_at", "timestamp with time zone", "NO"),
                ("updated_at", "timestamp with time zone", "NO"),
            ];

            assert_eq!(columns.len(), expected.len(), "column count matches schema");
            for ((col_name, col_type, nullable), (exp_name, exp_type, exp_nullable)) in
                columns.iter().zip(expected.iter())
            {
                assert_eq!(col_name, exp_name, "column name");
                assert_eq!(col_type, exp_type, "column type for {col_name}");
                assert_eq!(nullable, exp_nullable, "nullability for {col_name}");
            }

            let pk_columns: Vec<(String,)> = sqlx::query_as(
                r#"
                SELECT attribute.attname
                  FROM pg_constraint AS catalog_constraint
                  JOIN pg_class AS relation ON relation.oid = catalog_constraint.conrelid
                  JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
                  CROSS JOIN LATERAL unnest(catalog_constraint.conkey)
                      WITH ORDINALITY AS key(attnum, ordinal)
                  JOIN pg_attribute AS attribute
                    ON attribute.attrelid = relation.oid
                   AND attribute.attnum = key.attnum
                 WHERE namespace.nspname = 'vala'
                   AND relation.relname = 'forge_operation_state'
                   AND catalog_constraint.contype = 'p'
                 ORDER BY key.ordinal
                "#,
            )
            .fetch_all(&superuser)
            .await
            .expect("pk query");
            assert_eq!(
                pk_columns,
                vec![
                    ("data_tenant_id".to_owned(),),
                    ("resource".to_owned(),),
                    ("family".to_owned(),),
                    ("operation_id".to_owned(),),
                ],
                "primary-key columns and order"
            );

            let checks: Vec<(String, String)> = sqlx::query_as(
                r#"
                SELECT catalog_constraint.conname,
                       pg_get_constraintdef(catalog_constraint.oid, false)
                  FROM pg_constraint AS catalog_constraint
                  JOIN pg_class AS relation ON relation.oid = catalog_constraint.conrelid
                  JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
                 WHERE namespace.nspname = 'vala'
                   AND relation.relname = 'forge_operation_state'
                   AND catalog_constraint.contype = 'c'
                 ORDER BY catalog_constraint.conname
                "#,
            )
            .fetch_all(&superuser)
            .await
            .expect("check constraint query");
            assert_eq!(
                checks,
                vec![
                    (
                        "forge_operation_state_family_check".to_owned(),
                        "CHECK ((family = ANY (ARRAY['scribe_promotion'::text, 'iceberg_rewrite'::text, 'snapshot_expire'::text, 'orphan_gc'::text])))".to_owned(),
                    ),
                    (
                        "forge_operation_state_phase_check".to_owned(),
                        "CHECK ((phase = ANY (ARRAY['prepared'::text, 'committed'::text, 'recovered'::text, 'reset'::text])))".to_owned(),
                    ),
                ],
                "check names and exact definitions"
            );

            let partial_index: (String,) = sqlx::query_as(
                r#"
                SELECT indexdef
                  FROM pg_indexes
                 WHERE schemaname = 'vala'
                   AND tablename = 'forge_operation_state'
                   AND indexname = 'forge_operation_state_open'
                "#,
            )
            .fetch_one(&superuser)
            .await
            .expect("index query");
            assert_eq!(
                partial_index.0,
                "CREATE INDEX forge_operation_state_open ON vala.forge_operation_state USING btree (data_tenant_id, resource, family, prepared_at, operation_id) WHERE (phase = 'prepared'::text)",
                "exact partial-index definition and predicate"
            );

            // RLS enabled and forced
            let rls: (bool, bool) = sqlx::query_as(
                r#"
                SELECT relrowsecurity, relforcerowsecurity
                FROM pg_class
                WHERE relname = 'forge_operation_state'
                  AND relnamespace = (
                      SELECT oid FROM pg_namespace WHERE nspname = 'vala'
                  )
                "#,
            )
            .fetch_one(&superuser)
            .await
            .expect("rls query");
            assert!(rls.0, "RLS is enabled");
            assert!(rls.1, "RLS is forced");

            let policies: Vec<(String, String, String, String, String)> = sqlx::query_as(
                r#"
                SELECT policyname, permissive, cmd, qual, with_check
                  FROM pg_policies
                 WHERE schemaname = 'vala'
                   AND tablename = 'forge_operation_state'
                 ORDER BY policyname
                "#,
            )
            .fetch_all(&superuser)
            .await
            .expect("policy query");
            assert_eq!(
                policies,
                vec![(
                    "tenant_isolation".to_owned(),
                    "PERMISSIVE".to_owned(),
                    "ALL".to_owned(),
                    "(data_tenant_id = wyrd.current_tenant())".to_owned(),
                    "(data_tenant_id = wyrd.current_tenant())".to_owned(),
                )],
                "exact RLS policy mode, command, USING, and WITH CHECK"
            );

            let grants: Vec<(String, String)> = sqlx::query_as(
                r#"
                SELECT grantee, privilege_type
                  FROM information_schema.table_privileges
                 WHERE table_schema = 'vala'
                   AND table_name = 'forge_operation_state'
                   AND grantee IN ('wyrd_app', 'wyrd_platform_admin')
                 ORDER BY grantee, privilege_type
                "#,
            )
            .fetch_all(&superuser)
            .await
            .expect("grant query");
            assert_eq!(
                grants,
                vec![
                    ("wyrd_app".to_owned(), "INSERT".to_owned()),
                    ("wyrd_app".to_owned(), "SELECT".to_owned()),
                    ("wyrd_app".to_owned(), "UPDATE".to_owned()),
                    ("wyrd_platform_admin".to_owned(), "INSERT".to_owned()),
                    ("wyrd_platform_admin".to_owned(), "SELECT".to_owned()),
                    ("wyrd_platform_admin".to_owned(), "UPDATE".to_owned()),
                ],
                "complete role/privilege set"
            );
        }

        // -----------------------------------------------------------------------
        // Iceberg rewrite family
        // -----------------------------------------------------------------------

        /// Builds one Iceberg-rewrite detail for the fixed test resource.
        ///
        /// The production route stamps a distinct `operation_id` per rewrite
        /// *attempt*, because a retried attempt produces different output
        /// objects and a Prepared detail is immutable. Building the detail from
        /// that identity here keeps the durable proofs below aligned with what
        /// the publication owner actually writes.
        ///
        /// # Panics
        ///
        /// Panics when the fixed partition instant or object paths are invalid.
        fn rewrite_detail(
            operation_id: Uuid,
            phase: ForgeIcebergRewritePhase,
            committed_snapshot_id: Option<i64>,
            output: &str,
        ) -> AuditDetail {
            AuditDetail::ForgeIcebergRewrite {
                operation_id,
                phase,
                group: resource().to_owned(),
                base_snapshot_id: 100,
                committed_snapshot_id,
                partition_spec_id: 3,
                time_partition: wyrd_spec::vala::api::TimePartitionWire::new(
                    wyrd_spec::vala::api::TimeGranularityWire::Day,
                    chrono::DateTime::from_timestamp(1_756_684_800, 0).expect("fixture instant"),
                )
                .expect("fixture instant is an exact day boundary"),
                target_file_size_bytes: 1024,
                input_paths: vec![StoragePath::new("table/live-a.parquet").expect("valid path")],
                output_paths: vec![StoragePath::new(output).expect("valid path")],
            }
        }

        /// Verifies every terminal the Iceberg-rewrite route can persist.
        ///
        /// The production publication owner settles a rewrite three ways and
        /// only three ways: `Committed` when the catalog answered, `Recovered`
        /// when a successor found its predecessor's own snapshot on the table,
        /// and `Reset` when a refusal proved nothing landed. All three are
        /// asserted against one resource, each under its own attempt-scoped
        /// operation identity, because that is exactly the durable shape one
        /// table accumulates across a retried publication.
        ///
        /// # Panics
        ///
        /// Panics when setup, typed detail construction, transitions, or exact
        /// persisted phase/sequence/cardinality assertions fail.
        #[tokio::test]
        async fn iceberg_rewrite_settles_every_terminal_under_attempt_identities() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let family = ForgeOperationFamily::IcebergRewrite;

            let terminals = [
                (
                    ForgeIcebergRewritePhase::Committed,
                    "forge.iceberg_rewrite.committed",
                    Some(101),
                    "committed",
                ),
                (
                    ForgeIcebergRewritePhase::Recovered,
                    "forge.iceberg_rewrite.recovered",
                    Some(102),
                    "recovered",
                ),
                (
                    ForgeIcebergRewritePhase::Reset,
                    "forge.iceberg_rewrite.reset",
                    None,
                    "reset",
                ),
            ];

            for (index, (phase, operation, committed_snapshot_id, persisted)) in
                terminals.into_iter().enumerate()
            {
                let operation_id = Uuid::now_v7();
                let output = format!("table/rewrite-{index}.parquet");
                let prepared_event = event(
                    "forge.iceberg_rewrite.prepared",
                    resource(),
                    Some(rewrite_detail(
                        operation_id,
                        ForgeIcebergRewritePhase::Prepared,
                        None,
                        &output,
                    )),
                );
                let () = match append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("iceberg prepared")
                {
                    ForgeOperationTransition::Applied => (),
                    other => panic!("expected prepared application, got {other:?}"),
                };
                assert_eq!(
                    forge_operation_id(prepared_event.detail.as_ref().expect("prepared detail")),
                    operation_id,
                    "the persisted operation identity is the attempt's own"
                );

                let terminal_event = event(
                    operation,
                    resource(),
                    Some(rewrite_detail(
                        operation_id,
                        phase,
                        committed_snapshot_id,
                        &output,
                    )),
                );
                let () = match append_terminal(pool, tenant, resource(), family, &terminal_event)
                    .await
                    .expect("iceberg terminal")
                {
                    ForgeOperationTransition::Applied => (),
                    other => panic!("expected terminal application, got {other:?}"),
                };

                assert_eq!(
                    state_snapshot(pool, tenant, family, operation_id).await,
                    StateSnapshot {
                        phase: persisted.to_owned()
                    }
                );
            }

            assert_eq!(
                count_state(pool, tenant).await,
                3,
                "one resource carries one durable row per attempt identity"
            );
            assert_eq!(count_audit(pool, tenant).await, 6);
        }

        // -----------------------------------------------------------------------
        // Snapshot expire family
        // -----------------------------------------------------------------------

        /// Verifies snapshot-expiry Prepared-to-Committed persistence.
        ///
        /// # Panics
        ///
        /// Panics when setup, typed detail construction, transitions, or exact
        /// persisted phase/sequence/cardinality assertions fail.
        #[tokio::test]
        async fn snapshot_expire_prepare_and_commit() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let family = ForgeOperationFamily::SnapshotExpire;

            let detail = AuditDetail::ForgeSnapshotExpire {
                operation_id: Uuid::now_v7(),
                phase: ForgeSnapshotExpirePhase::Prepared,
                group: resource().to_owned(),
                base_metadata_location: StoragePath::new("table/iceberg/metadata/00001-a.json")
                    .expect("valid path"),
                current_snapshot_id: Some(42),
                retained_ref_heads: vec![42, 43],
                cutoff_ms: 1_700_000_000_000i64,
                selected_snapshot_ids: vec![1, 2, 3],
            };
            let prepared_event = event("forge.snapshot_expire.prepared", resource(), Some(detail));

            let () = match append_prepared(pool, tenant, resource(), family, &prepared_event)
                .await
                .expect("snapshot_expire prepared")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected prepared application, got {other:?}"),
            };
            let prepared_detail = prepared_event.detail.as_ref().expect("prepared detail");
            let operation_id = forge_operation_id(prepared_detail);
            let committed_detail = match prepared_detail {
                AuditDetail::ForgeSnapshotExpire {
                    base_metadata_location,
                    current_snapshot_id,
                    retained_ref_heads,
                    cutoff_ms,
                    selected_snapshot_ids,
                    ..
                } => AuditDetail::ForgeSnapshotExpire {
                    operation_id,
                    phase: ForgeSnapshotExpirePhase::Committed,
                    group: resource().to_owned(),
                    base_metadata_location: base_metadata_location.clone(),
                    current_snapshot_id: *current_snapshot_id,
                    retained_ref_heads: retained_ref_heads.clone(),
                    cutoff_ms: *cutoff_ms,
                    selected_snapshot_ids: selected_snapshot_ids.clone(),
                },
                _ => panic!("expected snapshot-expiry detail"),
            };
            let committed_event = event(
                "forge.snapshot_expire.committed",
                resource(),
                Some(committed_detail),
            );
            let () = match append_terminal(pool, tenant, resource(), family, &committed_event)
                .await
                .expect("snapshot_expire committed")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected terminal application, got {other:?}"),
            };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned()
                }
            );
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        // -----------------------------------------------------------------------
        // Orphan GC family
        // -----------------------------------------------------------------------

        /// Verifies orphan-GC Prepared-to-Committed persistence.
        ///
        /// # Panics
        ///
        /// Panics when setup, typed detail construction, transitions, or exact
        /// persisted phase/sequence/cardinality assertions fail.
        #[tokio::test]
        async fn orphan_gc_prepare_and_commit() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let family = ForgeOperationFamily::OrphanGc;

            let detail = AuditDetail::ForgeOrphanGc {
                operation_id: Uuid::now_v7(),
                phase: ForgeOrphanGcPhase::Prepared,
                group: resource().to_owned(),
                candidate_paths: vec![
                    StoragePath::new("table/orphans/a.parquet").expect("valid path"),
                ],
                deleted_paths: vec![],
                skipped_paths: vec![],
            };
            let prepared_event = event("forge.orphan_gc.prepared", resource(), Some(detail));

            let () = match append_prepared(pool, tenant, resource(), family, &prepared_event)
                .await
                .expect("orphan_gc prepared")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected prepared application, got {other:?}"),
            };
            let prepared_detail = prepared_event.detail.as_ref().expect("prepared detail");
            let operation_id = forge_operation_id(prepared_detail);
            let committed_detail = match prepared_detail {
                AuditDetail::ForgeOrphanGc {
                    candidate_paths, ..
                } => AuditDetail::ForgeOrphanGc {
                    operation_id,
                    phase: ForgeOrphanGcPhase::Committed,
                    group: resource().to_owned(),
                    candidate_paths: candidate_paths.clone(),
                    deleted_paths: candidate_paths.clone(),
                    skipped_paths: vec![],
                },
                _ => panic!("expected orphan-GC detail"),
            };
            let committed_event = event(
                "forge.orphan_gc.committed",
                resource(),
                Some(committed_detail),
            );
            let () = match append_terminal(pool, tenant, resource(), family, &committed_event)
                .await
                .expect("orphan_gc committed")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected terminal application, got {other:?}"),
            };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned()
                }
            );
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        // -----------------------------------------------------------------------
        // Invalid operations
        // -----------------------------------------------------------------------

        /// Verifies Reset is rejected for expiry and orphan-GC families without writes.
        ///
        /// # Panics
        ///
        /// Panics when setup, Prepared transitions, or exact conflict and
        /// cardinality assertions fail.
        #[tokio::test]
        async fn reset_is_conflict_for_snapshot_expire_and_orphan_gc() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let cases = [
                (
                    ForgeOperationFamily::SnapshotExpire,
                    "forge.snapshot_expire.prepared",
                    "forge.snapshot_expire.reset",
                    AuditDetail::ForgeSnapshotExpire {
                        operation_id: Uuid::now_v7(),
                        phase: ForgeSnapshotExpirePhase::Prepared,
                        group: resource().to_owned(),
                        base_metadata_location: StoragePath::new(
                            "table/iceberg/metadata/reset-expiry.json",
                        )
                        .expect("valid path"),
                        current_snapshot_id: Some(42),
                        retained_ref_heads: vec![42],
                        cutoff_ms: 1_700_000_000_000,
                        selected_snapshot_ids: vec![1],
                    },
                    "Reset transition is not valid for family snapshot_expire",
                ),
                (
                    ForgeOperationFamily::OrphanGc,
                    "forge.orphan_gc.prepared",
                    "forge.orphan_gc.reset",
                    AuditDetail::ForgeOrphanGc {
                        operation_id: Uuid::now_v7(),
                        phase: ForgeOrphanGcPhase::Prepared,
                        group: resource().to_owned(),
                        candidate_paths: vec![
                            StoragePath::new("table/orphans/reset.parquet").expect("valid path"),
                        ],
                        deleted_paths: vec![],
                        skipped_paths: vec![],
                    },
                    "Reset transition is not valid for family orphan_gc",
                ),
            ];

            for (family, prepared_operation, reset_operation, detail, expected) in cases {
                let prepared_event = event(prepared_operation, resource(), Some(detail.clone()));
                append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("family prepared");
                let reset_event = event(reset_operation, resource(), Some(detail));

                let result = append_terminal(pool, tenant, resource(), family, &reset_event).await;

                assert!(matches!(
                    result,
                    Err(SqlError::Conflict { ref detail }) if detail == expected
                ));
            }
            assert_eq!(count_state(pool, tenant).await, 2);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        // -----------------------------------------------------------------------
        // Scribe promotion family
        // -----------------------------------------------------------------------

        /// Builds one Scribe-promotion detail for the fixed test resource.
        ///
        /// # Panics
        /// Panics when the fixed promoted-file tuple or path is invalid.
        fn promotion_detail(
            operation_id: Uuid,
            phase: wyrd_spec::vala::api::ForgeScribePromotionPhase,
            committed_snapshot_id: Option<i64>,
            checksum: &str,
        ) -> AuditDetail {
            let promoted = [wyrd_spec::vala::api::ForgePromotedFile::new(
                Uuid::from_u128(5),
                StoragePath::new("table/hot-a.parquet").expect("valid path"),
                checksum,
            )
            .expect("promoted file")];
            AuditDetail::ForgeScribePromotion {
                operation_id,
                phase,
                group: resource().to_owned(),
                base_snapshot_id: 200,
                committed_snapshot_id,
                input_file_ids: vec![Uuid::from_u128(5)],
                input_paths: vec![StoragePath::new("table/hot-a.parquet").expect("valid path")],
                promoted_file_set_digest: wyrd_spec::vala::api::ForgePromotedFileSetDigest::compute(
                    &promoted,
                ),
            }
        }

        /// Verifies that the Scribe-promotion family transitions under the same
        /// fence and transactional audit contract as every other Forge family.
        ///
        /// A Prepared row persists with its prepared audit sequence, the
        /// Committed transition persists the promotion snapshot and its own
        /// terminal sequence, and each transition contributes exactly one audit
        /// row. The negative arms prove the fence is real rather than
        /// incidental: a promotion detail presented under the wrong family, a
        /// terminal event whose digest no longer matches the prepared file set,
        /// and an event carrying no audit detail at all each fail without
        /// leaving durable state behind.
        ///
        /// # Panics
        ///
        /// Panics when setup, typed detail construction, transitions, or exact
        /// persisted phase, sequence, digest, and cardinality assertions fail.
        #[tokio::test]
        async fn scribe_promotion_operation_transitions_are_fenced_and_audited() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let family = ForgeOperationFamily::ScribePromotion;

            assert_eq!(family.as_str(), "scribe_promotion");
            assert_eq!(family.operation_prefix(), "forge.scribe_promotion");
            assert_eq!(family.expected_detail_kind(), "forge_scribe_promotion");

            let operation_id = Uuid::now_v7();
            let prepared_detail = promotion_detail(
                operation_id,
                wyrd_spec::vala::api::ForgeScribePromotionPhase::Prepared,
                None,
                "aa11",
            );
            let prepared_event = event(
                "forge.scribe_promotion.prepared",
                resource(),
                Some(prepared_detail.clone()),
            );
            let () = match append_prepared(pool, tenant, resource(), family, &prepared_event)
                .await
                .expect("promotion prepared")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected prepared application, got {other:?}"),
            };
            assert_eq!(forge_operation_id(&prepared_detail), operation_id);

            // A promotion detail presented under another family is refused
            // before any durable effect.
            let mismatched = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::IcebergRewrite,
                &prepared_event,
            )
            .await;
            assert!(
                matches!(mismatched, Err(SqlError::Conflict { .. })),
                "promotion detail must not transition another family, got {mismatched:?}"
            );

            // A terminal event with no detail cannot settle the operation.
            let detailless = event("forge.scribe_promotion.committed", resource(), None);
            assert!(
                matches!(
                    append_terminal(pool, tenant, resource(), family, &detailless).await,
                    Err(SqlError::Conflict { .. })
                ),
                "a terminal transition without audit detail must fail closed"
            );

            let committed_event = event(
                "forge.scribe_promotion.committed",
                resource(),
                Some(promotion_detail(
                    operation_id,
                    wyrd_spec::vala::api::ForgeScribePromotionPhase::Committed,
                    Some(201),
                    "aa11",
                )),
            );
            let () = match append_terminal(pool, tenant, resource(), family, &committed_event)
                .await
                .expect("promotion committed")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected terminal application, got {other:?}"),
            };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned()
                }
            );

            // A replay whose promoted file set digests differently is not the
            // same settlement and must not be accepted as idempotent.
            let redigested = event(
                "forge.scribe_promotion.committed",
                resource(),
                Some(promotion_detail(
                    operation_id,
                    wyrd_spec::vala::api::ForgeScribePromotionPhase::Committed,
                    Some(201),
                    "bb22",
                )),
            );
            assert!(
                matches!(
                    append_terminal(pool, tenant, resource(), family, &redigested).await,
                    Err(SqlError::Conflict { .. })
                ),
                "a differing promoted-file-set digest must not replay as idempotent"
            );

            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        // -----------------------------------------------------------------------
        // Self-contained operation state
        // -----------------------------------------------------------------------

        /// Seeds one `vala.forge_operation_state` row directly, with no
        /// `vala.audit_staging` row at either referenced sequence.
        ///
        /// Production always appends the audit event in the same transaction as
        /// the transition, but audit delivery rows are subject to their own
        /// retention lifecycle. This helper reproduces the state a Forge worker
        /// legitimately restarts into once a delivered prepared audit row has
        /// aged out of the outbox, which no public writer can otherwise create.
        ///
        /// # Panics
        ///
        /// Panics when the tenant transaction, insert, or commit fails, or when
        /// either detail cannot be serialized.
        async fn seed_state_row(
            pool: &PgPool,
            tenant: DataTenantId,
            family: ForgeOperationFamily,
            operation_id: Uuid,
            phase: &str,
            prepared_detail: &AuditDetail,
            current_detail: &AuditDetail,
        ) {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for seeded projection row");
            sqlx::query(
                r#"
                INSERT INTO vala.forge_operation_state
                    (data_tenant_id, resource, family, operation_id, phase,
                     prepared_detail, current_detail,
                     prepared_at, updated_at)
                VALUES (wyrd.current_tenant(), $1, $2, $3, $4,
                        $5::jsonb, $6::jsonb, now(), now())
                "#,
            )
            .bind(resource())
            .bind(family.as_str())
            .bind(operation_id)
            .bind(phase)
            .bind(serde_json::to_string(prepared_detail).expect("serialize seeded prepared detail"))
            .bind(serde_json::to_string(current_detail).expect("serialize seeded current detail"))
            .execute(&mut **conn.transaction())
            .await
            .expect("seeded projection insert");
            conn.commit().await.expect("seeded projection commit");
        }

        /// Seeds one outbox-less prepared `OrphanGc` row and settles it.
        ///
        /// Orphan collection is the one destructive family whose recovery
        /// authority is reached only through this projection: an orphan batch
        /// leaves no catalog trace, so a reader that needed the prepared audit
        /// row to still exist would lose the batch whenever delivery was
        /// relayed or pruned away. This proves the family lists, settles, and
        /// snapshots on its own durable state, and that its terminal
        /// settlement still appends exactly one audit event atomically.
        ///
        /// # Panics
        ///
        /// Panics when seeding, listing, settlement, or any phase, sequence, or
        /// cardinality assertion fails.
        async fn assert_orphan_gc_recovery_is_outbox_independent(
            pool: &PgPool,
            tenant: DataTenantId,
            audits_before: i64,
        ) {
            let operation_id = Uuid::now_v7();
            let candidate_paths =
                vec![StoragePath::new("table/orphans/outbox-free.parquet").expect("valid path")];
            let prepared_detail = AuditDetail::ForgeOrphanGc {
                operation_id,
                phase: ForgeOrphanGcPhase::Prepared,
                group: resource().to_owned(),
                candidate_paths: candidate_paths.clone(),
                deleted_paths: vec![],
                skipped_paths: vec![],
            };
            seed_state_row(
                pool,
                tenant,
                ForgeOperationFamily::OrphanGc,
                operation_id,
                "prepared",
                &prepared_detail,
                &prepared_detail,
            )
            .await;

            let open = list_open(pool, tenant, ForgeOperationFamily::OrphanGc)
                .await
                .expect("orphan-GC recovery must not require an audit delivery row");
            assert!(!open.overflowed, "single seeded open orphan-GC operation");
            assert_eq!(open.operations.len(), 1, "exactly one open orphan-GC batch");
            assert_eq!(open.operations[0].operation_id, operation_id);
            assert_eq!(open.operations[0].prepared_detail, prepared_detail);

            let recovered_event = event(
                "forge.orphan_gc.recovered",
                resource(),
                Some(AuditDetail::ForgeOrphanGc {
                    operation_id,
                    phase: ForgeOrphanGcPhase::Recovered,
                    group: resource().to_owned(),
                    candidate_paths: candidate_paths.clone(),
                    deleted_paths: candidate_paths,
                    skipped_paths: vec![],
                }),
            );
            let () = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::OrphanGc,
                &recovered_event,
            )
            .await
            .expect("orphan-GC settlement must not require an audit delivery row")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected terminal application, got {other:?}"),
            };
            assert_eq!(
                count_audit(pool, tenant).await,
                audits_before + 1,
                "orphan-GC settlement appends exactly one audit event"
            );
            assert_eq!(
                state_snapshot(pool, tenant, ForgeOperationFamily::OrphanGc, operation_id).await,
                StateSnapshot {
                    phase: "recovered".to_owned()
                }
            );
            assert!(
                list_open(pool, tenant, ForgeOperationFamily::OrphanGc)
                    .await
                    .expect("open listing")
                    .operations
                    .is_empty(),
                "a settled batch is no longer open recovery work"
            );
        }

        /// Keyset paging over open operations is complete and scope-isolated.
        ///
        /// Reconciliation must observe every open operation exactly once, so
        /// the page boundary is walked with the cursor the previous page ended
        /// on rather than an offset — an offset silently repeats or skips a row
        /// whenever a concurrent owner settles one mid-walk. Two rows are
        /// deliberately seeded with the same `prepared_at`, because that is the
        /// case a timestamp-only cursor loses. The listing is also confined to
        /// its own family: an open operation of a different family sharing the
        /// resource must never appear in the page or shift its ordering.
        ///
        /// # Panics
        ///
        /// Panics when setup fails, when the walk repeats, skips, or reorders a
        /// row, when overflow is misreported, or when a foreign-family
        /// operation leaks into the page.
        #[tokio::test]
        async fn open_operation_keyset_pagination_is_complete_and_isolated() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            /// Seeds one Prepared orphan-GC row and returns its id.
            async fn seed_open(
                pool: &PgPool,
                tenant: DataTenantId,
                family: ForgeOperationFamily,
            ) -> Uuid {
                let operation_id = Uuid::now_v7();
                let detail = match family {
                    ForgeOperationFamily::OrphanGc => AuditDetail::ForgeOrphanGc {
                        operation_id,
                        phase: ForgeOrphanGcPhase::Prepared,
                        group: resource().to_owned(),
                        candidate_paths: vec![
                            StoragePath::new("table/orphans/a.parquet").expect("valid path"),
                        ],
                        deleted_paths: vec![],
                        skipped_paths: vec![],
                    },
                    _ => {
                        expire_detail(operation_id, ForgeSnapshotExpirePhase::Prepared, resource())
                    }
                };
                seed_state_row(
                    pool,
                    tenant,
                    family,
                    operation_id,
                    "prepared",
                    &detail,
                    &detail,
                )
                .await;
                operation_id
            }

            for _ in 0..5 {
                seed_open(pool, tenant, ForgeOperationFamily::OrphanGc).await;
            }
            // A different family on the same resource must be invisible here.
            let foreign = seed_open(pool, tenant, ForgeOperationFamily::SnapshotExpire).await;

            // Two rows share one `prepared_at`, so the cursor must carry the
            // operation id to make progress across that boundary.
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for the collision update");
            sqlx::query(
                r#"
                UPDATE vala.forge_operation_state
                   SET prepared_at = (
                        SELECT MIN(prepared_at)
                          FROM vala.forge_operation_state
                         WHERE family = 'orphan_gc')
                 WHERE family = 'orphan_gc'
                "#,
            )
            .execute(&mut **conn.transaction())
            .await
            .expect("collapse the prepared timestamps");
            conn.commit().await.expect("collision update commits");

            let ops = ForgeOperations::new(resource(), ForgeOperationFamily::OrphanGc)
                .expect("valid Forge resource");
            let mut walked: Vec<Uuid> = Vec::new();
            let mut cursor = None;
            let mut pages = 0;
            loop {
                let mut conn = TenantConn::acquire(pool, tenant)
                    .await
                    .expect("tenant connection for one page");
                let page = ops
                    .list_open(&mut conn, 2, cursor)
                    .await
                    .expect("bounded open listing");
                conn.commit().await.expect("page commit");
                pages += 1;
                assert!(pages <= 4, "a cursor that does not advance would loop here");
                for row in &page.operations {
                    walked.push(row.operation_id);
                }
                let Some(last) = page.operations.last() else {
                    assert!(!page.overflowed, "an empty page cannot overflow");
                    break;
                };
                cursor = Some((last.prepared_at, last.operation_id));
                if !page.overflowed {
                    break;
                }
            }

            assert_eq!(
                walked.len(),
                5,
                "every open operation is visited exactly once"
            );
            let unique: std::collections::BTreeSet<Uuid> = walked.iter().copied().collect();
            assert_eq!(unique.len(), 5, "no operation is returned twice");
            assert!(
                !walked.contains(&foreign),
                "a different family never leaks into this family's page"
            );
            let mut sorted = walked.clone();
            sorted.sort_unstable();
            assert_eq!(
                walked, sorted,
                "with one shared timestamp the walk is ordered by operation id"
            );
            assert_eq!(
                pages, 3,
                "five open operations at a cap of two are exactly three pages"
            );

            // The overflow flag is the walk's only stopping condition, so it
            // must be exact at the boundary rather than merely eventually
            // false: a page holding every remaining row has not overflowed even
            // though the reader asked for one more than it can return.
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for the boundary page");
            let exact = ops
                .list_open(&mut conn, 5, None)
                .await
                .expect("a page that holds every open operation");
            assert_eq!(exact.operations.len(), 5);
            assert!(
                !exact.overflowed,
                "a page that returned every open operation has nothing past it"
            );
            let under = ops
                .list_open(&mut conn, 4, None)
                .await
                .expect("a page one short of the open set");
            assert_eq!(under.operations.len(), 4);
            assert!(
                under.overflowed,
                "a page one short of the open set reports what it could not return"
            );
            assert!(
                matches!(
                    ops.list_open(&mut conn, 0, None).await,
                    Err(SqlError::Conflict { .. })
                ),
                "a zero cap is refused rather than silently returning nothing"
            );
            conn.commit().await.expect("boundary read commit");
        }

        /// Builds one snapshot-expiry detail for the fixed test resource.
        ///
        /// # Panics
        ///
        /// Panics when the fixed metadata location is not a valid storage path.
        fn expire_detail(
            operation_id: Uuid,
            phase: ForgeSnapshotExpirePhase,
            group: &str,
        ) -> AuditDetail {
            AuditDetail::ForgeSnapshotExpire {
                operation_id,
                phase,
                group: group.to_owned(),
                base_metadata_location: StoragePath::new(
                    "table/iceberg/metadata/00007-self-contained.json",
                )
                .expect("valid path"),
                current_snapshot_id: Some(77),
                retained_ref_heads: vec![77],
                cutoff_ms: 1_700_000_000_000,
                selected_snapshot_ids: vec![11, 12],
            }
        }

        /// Lists open operations for one family through a tenant transaction.
        ///
        /// # Panics
        ///
        /// Panics when the tenant transaction cannot be acquired or committed,
        /// or when the fixed resource cannot construct the owner.
        async fn list_open(
            pool: &PgPool,
            tenant: DataTenantId,
            family: ForgeOperationFamily,
        ) -> Result<vala_sql::row_types::forge_operations::OpenForgeOperationPage, SqlError>
        {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for open listing");
            let ops = ForgeOperations::new(resource(), family).expect("valid Forge resource");
            let result = ops.list_open(&mut conn, 8, None).await;
            conn.commit().await.expect("open listing commit");
            result
        }

        /// Lists Reset operations for one family through a tenant transaction.
        ///
        /// # Panics
        ///
        /// Panics when the tenant transaction cannot be acquired or committed,
        /// or when the fixed resource cannot construct the owner.
        async fn list_reset(
            pool: &PgPool,
            tenant: DataTenantId,
            family: ForgeOperationFamily,
        ) -> Result<vala_sql::row_types::forge_operations::OpenForgeOperationPage, SqlError>
        {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for reset listing");
            let ops = ForgeOperations::new(resource(), family).expect("valid Forge resource");
            let result = ops.list_reset(&mut conn, 8).await;
            conn.commit().await.expect("reset listing commit");
            result
        }

        /// Proves Forge operation recovery reads only its own state projection.
        ///
        /// `vala.forge_operation_state` is the sole Forge recovery authority:
        /// it stores the complete typed prepared and current details plus the
        /// audit sequences those transitions produced. `vala.audit_staging` is a
        /// delivery table with its own retention, so requiring one of its rows
        /// to still be present before a worker may read back its own prepared
        /// operation would make recovery depend on audit delivery rather than
        /// on Forge's own durable state.
        ///
        /// The seeded rows carry complete valid state and sequence references
        /// with no outbox row at either sequence. Reads, Prepared replay, and
        /// terminal settlement must all succeed on that state alone, terminal
        /// settlement must still append exactly one audit event atomically, and
        /// contradictory stored state must still fail closed.
        ///
        /// # Panics
        ///
        /// Panics when setup, seeding, reads, transitions, or exact phase,
        /// sequence, cardinality, and refusal assertions fail.
        #[tokio::test]
        async fn self_contained_state_reads_and_replays_do_not_require_prepared_outbox_row() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            // A Prepared snapshot-expiry operation whose prepared audit row is
            // no longer in the outbox.
            let expire_id = Uuid::now_v7();
            let prepared_detail =
                expire_detail(expire_id, ForgeSnapshotExpirePhase::Prepared, resource());
            seed_state_row(
                pool,
                tenant,
                ForgeOperationFamily::SnapshotExpire,
                expire_id,
                "prepared",
                &prepared_detail,
                &prepared_detail,
            )
            .await;

            // A terminal Reset rewrite operation whose audit rows are likewise
            // absent.
            let reset_id = Uuid::now_v7();
            let reset_prepared_detail = rewrite_detail(
                reset_id,
                ForgeIcebergRewritePhase::Prepared,
                None,
                "table/rewrite-self-contained.parquet",
            );
            let reset_current_detail = rewrite_detail(
                reset_id,
                ForgeIcebergRewritePhase::Reset,
                None,
                "table/rewrite-self-contained.parquet",
            );
            seed_state_row(
                pool,
                tenant,
                ForgeOperationFamily::IcebergRewrite,
                reset_id,
                "reset",
                &reset_prepared_detail,
                &reset_current_detail,
            )
            .await;

            assert_eq!(
                count_audit(pool, tenant).await,
                0,
                "no audit delivery row backs either seeded operation"
            );

            let open = list_open(pool, tenant, ForgeOperationFamily::SnapshotExpire)
                .await
                .expect("open listing must not require an audit delivery row");
            assert!(!open.overflowed, "single seeded open operation");
            assert_eq!(open.operations.len(), 1, "exactly one open operation");
            assert_eq!(open.operations[0].operation_id, expire_id);
            assert_eq!(
                open.operations[0].phase,
                vala_sql::row_types::forge_operations::ForgeOperationPhase::Prepared
            );
            assert_eq!(open.operations[0].prepared_detail, prepared_detail);

            let reset = list_reset(pool, tenant, ForgeOperationFamily::IcebergRewrite)
                .await
                .expect("reset listing must not require an audit delivery row");
            assert!(!reset.overflowed, "single seeded reset operation");
            assert_eq!(reset.operations.len(), 1, "exactly one reset operation");
            assert_eq!(reset.operations[0].operation_id, reset_id);
            assert_eq!(
                reset.operations[0].phase,
                vala_sql::row_types::forge_operations::ForgeOperationPhase::Reset
            );

            // Identical Prepared replay returns the stored sequence and writes
            // nothing.
            let replay = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::SnapshotExpire,
                &event(
                    "forge.snapshot_expire.prepared",
                    resource(),
                    Some(prepared_detail.clone()),
                ),
            )
            .await
            .expect("Prepared replay must not require an audit delivery row");
            assert!(
                matches!(replay, ForgeOperationTransition::AlreadyApplied),
                "Prepared replay returns the stored prepared sequence, got {replay:?}"
            );
            assert_eq!(
                count_audit(pool, tenant).await,
                0,
                "an idempotent Prepared replay appends no audit"
            );

            // Terminal settlement still appends exactly one audit event.
            let committed_event = event(
                "forge.snapshot_expire.committed",
                resource(),
                Some(expire_detail(
                    expire_id,
                    ForgeSnapshotExpirePhase::Committed,
                    resource(),
                )),
            );
            let () = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::SnapshotExpire,
                &committed_event,
            )
            .await
            .expect("terminal settlement must not require an audit delivery row")
            {
                ForgeOperationTransition::Applied => (),
                other => panic!("expected terminal application, got {other:?}"),
            };
            assert_eq!(
                count_audit(pool, tenant).await,
                1,
                "terminal settlement appends exactly one audit event"
            );
            assert_eq!(
                state_snapshot(
                    pool,
                    tenant,
                    ForgeOperationFamily::SnapshotExpire,
                    expire_id
                )
                .await,
                StateSnapshot {
                    phase: "committed".to_owned()
                }
            );

            // Terminal replay is idempotent and appends nothing further.
            let terminal_replay = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::SnapshotExpire,
                &committed_event,
            )
            .await
            .expect("idempotent terminal replay");
            assert!(
                matches!(terminal_replay, ForgeOperationTransition::AlreadyApplied),
                "terminal replay returns the stored terminal sequence, got {terminal_replay:?}"
            );
            assert_eq!(
                count_audit(pool, tenant).await,
                1,
                "an idempotent terminal replay appends no audit"
            );

            assert_orphan_gc_recovery_is_outbox_independent(pool, tenant, 1).await;

            // Contradictory stored state still fails closed before any caller
            // receives recovery authority.
            let mismatched_id = Uuid::now_v7();
            let mismatched_detail = expire_detail(
                mismatched_id,
                ForgeSnapshotExpirePhase::Prepared,
                "tenant_a.ns.other",
            );
            seed_state_row(
                pool,
                tenant,
                ForgeOperationFamily::SnapshotExpire,
                mismatched_id,
                "prepared",
                &mismatched_detail,
                &mismatched_detail,
            )
            .await;
            assert!(
                matches!(
                    list_open(pool, tenant, ForgeOperationFamily::SnapshotExpire).await,
                    Err(SqlError::InvariantViolation { .. })
                ),
                "state whose detail identity contradicts its row must fail closed"
            );
        }

        // -------------------------------------------------------------------
        // Serialized snapshot expiration claims
        // -------------------------------------------------------------------

        /// Fixed table identity every expiration test claims against.
        fn claim_table(table_uuid: Uuid) -> ForgeClaimTable {
            ForgeClaimTable {
                table_uid: [7_u8; 16],
                catalog_name: "wyrd-redux".to_owned(),
                namespace_name: "vala.bifrost".to_owned(),
                table_name: "tbl".to_owned(),
                table_uuid,
            }
        }

        /// Seeds the registered table, its maintenance-authority row, one live
        /// lease, and one running snapshot-expiry task owned by `authority`.
        ///
        /// # Panics
        ///
        /// Panics when any seeding statement fails.
        async fn seed_expiration_arrangement(
            superuser: &PgPool,
            tenant: DataTenantId,
            authority: &ForgeExpirationAuthority,
            table: &ForgeClaimTable,
        ) {
            sqlx::query("INSERT INTO vala.bifrost_tables (data_tenant_id,table_uid,fqn,fingerprint,physical_layout) VALUES ($1,$2,'vala.bifrost.tbl',decode(repeat('00',32),'hex'),'{}'::jsonb)")
                .bind(tenant.as_uuid())
                .bind(table.table_uid.as_slice())
                .execute(superuser)
                .await
                .expect("seed bifrost table");
            sqlx::query("INSERT INTO vala.bifrost_table_maintenance_authority (data_tenant_id,catalog_name,namespace_name,table_name,table_uid) VALUES ($1,$2,$3,$4,$5)")
                .bind(tenant.as_uuid())
                .bind(&table.catalog_name)
                .bind(&table.namespace_name)
                .bind(&table.table_name)
                .bind(table.table_uid.as_slice())
                .execute(superuser)
                .await
                .expect("seed maintenance authority");
            sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
                .bind(&authority.lease_key)
                .bind(authority.worker_id)
                .bind(authority.lease_fencing_token)
                .execute(superuser)
                .await
                .expect("seed lease");
            sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry',77,'{}'::jsonb,decode(repeat('00',32),'hex'),1,1,'running',$6,$7,now()+interval '10 minutes',77,1,now())")
                .bind(authority.task_id)
                .bind(tenant.as_uuid())
                .bind(&table.catalog_name)
                .bind(&table.namespace_name)
                .bind(&table.table_name)
                .bind(authority.attempt_id)
                .bind(authority.worker_id)
                .execute(superuser)
                .await
                .expect("seed running task");
        }

        /// Builds Prepared-phase task evidence with no committed publication yet.
        fn prepared_evidence() -> ForgeTaskEvidence {
            ForgeTaskEvidence {
                prepared_candidate_index: None,
                version: 1,
                committed_snapshot_id: None,
                committed_metadata_location: None,
                committed_metadata_digest: None,
                cleanup_candidates: Vec::new(),
                deleted_candidate_count: 0,
            }
        }

        /// Builds settled evidence naming the exact committed publication.
        fn settled_evidence() -> ForgeTaskEvidence {
            ForgeTaskEvidence {
                prepared_candidate_index: None,
                version: 1,
                committed_snapshot_id: Some(88),
                committed_metadata_location: Some(
                    "table/iceberg/metadata/00008-settled.json".to_owned(),
                ),
                committed_metadata_digest: Some("a".repeat(64)),
                cleanup_candidates: Vec::new(),
                deleted_candidate_count: 0,
            }
        }

        /// Counts this operation's unresolved claim rows.
        ///
        /// # Panics
        ///
        /// Panics when the count query fails.
        async fn count_claims(superuser: &PgPool, operation_id: Uuid) -> i64 {
            sqlx::query_scalar(
                "SELECT count(*) FROM vala.forge_snapshot_expiration_claims WHERE operation_id=$1",
            )
            .bind(operation_id)
            .fetch_one(superuser)
            .await
            .expect("claim count")
        }

        /// Reads one Forge task's current state string.
        ///
        /// # Panics
        ///
        /// Panics when the state query fails.
        async fn task_state_of(superuser: &PgPool, task_id: Uuid) -> String {
            sqlx::query_scalar("SELECT state FROM vala.forge_tasks WHERE task_id=$1")
                .bind(task_id)
                .fetch_one(superuser)
                .await
                .expect("task state")
        }

        /// Every durable fact a refused expiration transition must leave alone.
        ///
        /// Captured as one tuple so a corruption case can compare the complete
        /// state before and after the refusal instead of asserting each fact
        /// separately: total claim rows, the operation phase, the task state,
        /// the tenant's highest planning-demand generation, and the audit
        /// chain length.
        ///
        /// # Panics
        ///
        /// Panics when any inspection query fails.
        async fn durable_expiration_state(
            superuser: &PgPool,
            tenant: DataTenantId,
            operation_id: Uuid,
            task_id: Uuid,
        ) -> (i64, Option<String>, String, i64, i64) {
            let claims: i64 =
                sqlx::query_scalar("SELECT count(*) FROM vala.forge_snapshot_expiration_claims")
                    .fetch_one(superuser)
                    .await
                    .expect("total claim rows");
            let phase: Option<String> = sqlx::query_scalar(
                "SELECT phase FROM vala.forge_operation_state WHERE operation_id=$1",
            )
            .bind(operation_id)
            .fetch_optional(superuser)
            .await
            .expect("operation phase");
            let demand: i64 = sqlx::query_scalar(
                "SELECT COALESCE(max(generation),-1) FROM vala.forge_planning_demands WHERE data_tenant_id=$1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(superuser)
            .await
            .expect("planning demand generation");
            (
                claims,
                phase,
                task_state_of(superuser, task_id).await,
                demand,
                count_audit(superuser, tenant).await,
            )
        }

        /// Proves the claim table matches its migration and that preparation,
        /// reset, and settlement are each one atomic, replayable transaction.
        ///
        /// # Panics
        ///
        /// Panics on any schema, lifecycle, atomicity, or replay mismatch.
        #[tokio::test]
        async fn snapshot_expiration_claim_schema_and_atomic_lifecycle_match_migration() {
            let TestFixtures { fixture, superuser } = setup().await;
            let tenant = fixture.data_tenant_id();
            let operator = fixture.operator_pool();

            // --- schema is exactly what the migration declares -------------
            let columns: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT column_name,data_type,is_nullable FROM information_schema.columns WHERE table_schema='vala' AND table_name='forge_snapshot_expiration_claims' ORDER BY ordinal_position",
            )
            .fetch_all(&superuser)
            .await
            .expect("claim columns");
            let expected = [
                ("data_tenant_id", "uuid", "NO"),
                ("resource", "text", "NO"),
                ("family", "text", "NO"),
                ("operation_id", "uuid", "NO"),
                ("snapshot_id", "bigint", "NO"),
                ("task_id", "uuid", "NO"),
                ("attempt_id", "uuid", "NO"),
                ("worker_id", "uuid", "NO"),
                ("lease_key", "text", "NO"),
                ("lease_fencing_token", "bigint", "NO"),
                ("table_uid", "bytea", "NO"),
                ("catalog_name", "text", "NO"),
                ("namespace_name", "text", "NO"),
                ("table_name", "text", "NO"),
                ("table_uuid", "uuid", "NO"),
            ];
            assert_eq!(columns.len(), expected.len(), "claim column count");
            for ((name, kind, nullable), (want_name, want_kind, want_nullable)) in
                columns.iter().zip(expected.iter())
            {
                assert_eq!(name, want_name, "claim column name");
                assert_eq!(kind, want_kind, "claim column type for {name}");
                assert_eq!(nullable, want_nullable, "claim nullability for {name}");
            }

            let indexes: Vec<(String,)> = sqlx::query_as(
                "SELECT indexname FROM pg_indexes WHERE schemaname='vala' AND tablename='forge_snapshot_expiration_claims' ORDER BY indexname",
            )
            .fetch_all(&superuser)
            .await
            .expect("claim indexes");
            let index_names: Vec<&str> = indexes.iter().map(|row| row.0.as_str()).collect();
            assert!(
                index_names.contains(&"forge_snapshot_expiration_claims_admission")
                    && index_names.contains(&"forge_snapshot_expiration_claims_task"),
                "admission and task-bound indexes exist: {index_names:?}"
            );

            let (rls_enabled, rls_forced): (bool, bool) = sqlx::query_as(
                "SELECT relrowsecurity,relforcerowsecurity FROM pg_class WHERE oid='vala.forge_snapshot_expiration_claims'::regclass",
            )
            .fetch_one(&superuser)
            .await
            .expect("rls flags");
            assert!(rls_enabled && rls_forced, "claims force row level security");

            let grants: Vec<(String, String)> = sqlx::query_as(
                "SELECT grantee,privilege_type FROM information_schema.role_table_grants WHERE table_schema='vala' AND table_name='forge_snapshot_expiration_claims' AND grantee IN ('wyrd_app','wyrd_platform_admin') ORDER BY grantee,privilege_type",
            )
            .fetch_all(&superuser)
            .await
            .expect("claim grants");
            let granted: Vec<(&str, &str)> = grants
                .iter()
                .map(|row| (row.0.as_str(), row.1.as_str()))
                .collect();
            assert_eq!(
                granted,
                vec![
                    ("wyrd_app", "SELECT"),
                    ("wyrd_platform_admin", "DELETE"),
                    ("wyrd_platform_admin", "INSERT"),
                    ("wyrd_platform_admin", "SELECT"),
                ],
                "claims are immutable: no role holds UPDATE"
            );

            // --- preparation is one atomic transaction ---------------------
            let authority = ForgeExpirationAuthority {
                task_id: Uuid::now_v7(),
                attempt_id: Uuid::now_v7(),
                worker_id: Uuid::now_v7(),
                lease_key: "forge:tenant_a:vala.bifrost:tbl".to_owned(),
                lease_fencing_token: 9,
            };
            let table = claim_table(Uuid::now_v7());
            seed_expiration_arrangement(&superuser, tenant, &authority, &table).await;

            let operation_id = Uuid::now_v7();
            let detail =
                expire_detail(operation_id, ForgeSnapshotExpirePhase::Prepared, resource());
            let evidence = prepared_evidence();
            let ops = ForgeOperations::new(resource(), ForgeOperationFamily::SnapshotExpire)
                .expect("valid Forge resource");
            let preparation = ForgeExpirationPreparation {
                authority: &authority,
                table: &table,
                evidence: &evidence,
                operation: "forge.snapshot_expire.prepared",
                detail: &detail,
            };

            let applied = ops
                .prepare_snapshot_expiration(operator, tenant, &preparation)
                .await
                .expect("preparation applies");
            assert!(
                matches!(applied, ForgeOperationTransition::Applied { .. }),
                "first preparation applies: {applied:?}"
            );
            assert_eq!(
                count_claims(&superuser, operation_id).await,
                2,
                "one claim per selected snapshot"
            );
            assert_eq!(
                task_state_of(&superuser, authority.task_id).await,
                "prepared",
                "task advanced with the claims"
            );
            assert_eq!(
                count_audit(&superuser, tenant).await,
                2,
                "preparation audits the operation and the task exactly once each"
            );

            // Replaying the identical preparation writes nothing.
            let replay = ops
                .prepare_snapshot_expiration(operator, tenant, &preparation)
                .await
                .expect("preparation replay");
            assert!(
                matches!(replay, ForgeOperationTransition::AlreadyApplied { .. }),
                "identical preparation replay is idempotent: {replay:?}"
            );
            assert_eq!(
                count_audit(&superuser, tenant).await,
                2,
                "replay appends no audit"
            );

            // A second table-local operation cannot claim the same snapshot.
            let rival_id = Uuid::now_v7();
            let rival_detail =
                expire_detail(rival_id, ForgeSnapshotExpirePhase::Prepared, resource());
            let rival = ForgeExpirationPreparation {
                authority: &authority,
                table: &table,
                evidence: &evidence,
                operation: "forge.snapshot_expire.prepared",
                detail: &rival_detail,
            };
            assert!(
                ops.prepare_snapshot_expiration(operator, tenant, &rival)
                    .await
                    .is_err(),
                "a rival operation cannot claim an already-claimed snapshot"
            );
            assert_eq!(
                count_claims(&superuser, rival_id).await,
                0,
                "the refused rival left no claim behind"
            );

            // --- reset releases the selection without a public phase -------
            let reset_request = ForgeExpirationResetRequest {
                authority: &authority,
                table: &table,
                detail: &detail,
            };
            let reset = ops
                .reset_snapshot_expiration(operator, tenant, &reset_request)
                .await
                .expect("reset applies");
            let ForgeExpirationResetOutcome::Applied { demand_generation } = reset else {
                panic!("first reset applies: {reset:?}");
            };
            assert!(demand_generation > 0, "reset advanced planning demand");
            assert_eq!(
                count_claims(&superuser, operation_id).await,
                0,
                "reset released every claim"
            );
            assert_eq!(
                task_state_of(&superuser, authority.task_id).await,
                "cancelled",
                "reset cancelled the task in the same transaction"
            );
            let phase: String = sqlx::query_scalar(
                "SELECT phase FROM vala.forge_operation_state WHERE operation_id=$1",
            )
            .bind(operation_id)
            .fetch_one(&superuser)
            .await
            .expect("reset phase");
            assert_eq!(
                phase, "reset",
                "reset is durable in the column, not the detail"
            );

            let reset_replay = ops
                .reset_snapshot_expiration(operator, tenant, &reset_request)
                .await
                .expect("reset replay");
            assert_eq!(
                reset_replay,
                ForgeExpirationResetOutcome::AlreadyApplied,
                "identical reset replay writes nothing"
            );
            assert_eq!(
                count_audit(&superuser, tenant).await,
                4,
                "reset audits the operation and the task exactly once each"
            );

            // --- settlement resolves a fresh preparation -------------------
            let settle_authority = ForgeExpirationAuthority {
                task_id: Uuid::now_v7(),
                attempt_id: Uuid::now_v7(),
                worker_id: Uuid::now_v7(),
                lease_key: "forge:tenant_a:vala.bifrost:tbl:settle".to_owned(),
                lease_fencing_token: 10,
            };
            sqlx::query("INSERT INTO vala.maintenance_leases (lease_key,owner,fencing_token,expires_at,heartbeat_at) VALUES ($1,$2,$3,now()+interval '10 minutes',now())")
                .bind(&settle_authority.lease_key)
                .bind(settle_authority.worker_id)
                .bind(settle_authority.lease_fencing_token)
                .execute(&superuser)
                .await
                .expect("seed settle lease");
            sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry',78,'{}'::jsonb,decode(repeat('11',32),'hex'),1,1,'running',$6,$7,now()+interval '10 minutes',78,1,now())")
                .bind(settle_authority.task_id)
                .bind(tenant.as_uuid())
                .bind(&table.catalog_name)
                .bind(&table.namespace_name)
                .bind(&table.table_name)
                .bind(settle_authority.attempt_id)
                .bind(settle_authority.worker_id)
                .execute(&superuser)
                .await
                .expect("seed settle task");

            let settle_operation = Uuid::now_v7();
            let settle_detail = expire_detail(
                settle_operation,
                ForgeSnapshotExpirePhase::Prepared,
                resource(),
            );
            ops.prepare_snapshot_expiration(
                operator,
                tenant,
                &ForgeExpirationPreparation {
                    authority: &settle_authority,
                    table: &table,
                    evidence: &evidence,
                    operation: "forge.snapshot_expire.prepared",
                    detail: &settle_detail,
                },
            )
            .await
            .expect("settle preparation applies");

            let committed_detail = expire_detail(
                settle_operation,
                ForgeSnapshotExpirePhase::Committed,
                resource(),
            );
            let final_evidence = settled_evidence();
            let settlement = ForgeExpirationSettlementRequest {
                authority: &settle_authority,
                table: &table,
                settlement: ForgeExpirationSettlement::Committed,
                evidence: &final_evidence,
                operation: "forge.snapshot_expire.committed",
                detail: &committed_detail,
            };
            // --- every claim row must exactly reproduce the preparation ----
            // Claim rows are immutable historical evidence. A takeover changes
            // current execution authority, never preparation identity, so any
            // divergence must refuse reconciliation input, reset, and
            // settlement without touching claims, operation state, the task,
            // planning demand, or the audit chain.
            let settle_reset_request = ForgeExpirationResetRequest {
                authority: &settle_authority,
                table: &table,
                detail: &settle_detail,
            };

            // The claim table's foreign keys mean a corrupted row must still
            // reference real state, so each identity case gets a decoy the
            // validator is required to reject anyway.
            let stranger = Uuid::now_v7();
            let decoy_state = "INSERT INTO vala.forge_operation_state (data_tenant_id,resource,family,operation_id,phase,prepared_detail,current_detail,prepared_at,updated_at) VALUES ($1,$2,'snapshot_expire',$3,'prepared',$4::jsonb,$4::jsonb,now(),now())";
            for (decoy_resource, decoy_operation) in [
                ("tenant_a.ns.other", settle_operation),
                (resource(), stranger),
            ] {
                let decoy_detail = expire_detail(
                    decoy_operation,
                    ForgeSnapshotExpirePhase::Prepared,
                    decoy_resource,
                );
                sqlx::query(decoy_state)
                    .bind(tenant.as_uuid())
                    .bind(decoy_resource)
                    .bind(decoy_operation)
                    .bind(serde_json::to_string(&decoy_detail).expect("serialize decoy detail"))
                    .execute(&superuser)
                    .await
                    .expect("seed decoy operation state");
            }
            sqlx::query("INSERT INTO vala.bifrost_tables (data_tenant_id,table_uid,fqn,fingerprint,physical_layout) VALUES ($1,decode(repeat('aa',16),'hex'),'vala.bifrost.other',decode(repeat('22',32),'hex'),'{}'::jsonb)")
                .bind(tenant.as_uuid())
                .execute(&superuser)
                .await
                .expect("seed decoy bifrost table");
            sqlx::query("INSERT INTO vala.bifrost_table_maintenance_authority (data_tenant_id,catalog_name,namespace_name,table_name,table_uid) VALUES ($1,'wyrd-redux','vala.bifrost','other',decode(repeat('aa',16),'hex'))")
                .bind(tenant.as_uuid())
                .execute(&superuser)
                .await
                .expect("seed decoy maintenance authority");
            sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,state,ready_at) VALUES ($1,$2,'wyrd-redux','vala.bifrost','other','snapshot_expiry',79,'{}'::jsonb,decode(repeat('22',32),'hex'),1,1,'ready',now())")
                .bind(stranger)
                .bind(tenant.as_uuid())
                .execute(&superuser)
                .await
                .expect("seed decoy task");

            let all_rows = format!("WHERE operation_id='{settle_operation}'");
            let one_row = format!("WHERE operation_id='{settle_operation}' AND snapshot_id=11");
            let corruptions: Vec<(&str, String, String)> = vec![
                (
                    "resource",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET resource='tenant_a.ns.other' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET resource='{}' WHERE operation_id='{settle_operation}'",
                        resource()
                    ),
                ),
                (
                    "operation identity",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET operation_id='{stranger}' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET operation_id='{settle_operation}' WHERE operation_id='{stranger}'"
                    ),
                ),
                (
                    "table uid",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_uid=decode(repeat('aa',16),'hex') {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_uid=decode(repeat('07',16),'hex') {all_rows}"
                    ),
                ),
                (
                    "table name",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_name='other' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_name='{}' {all_rows}",
                        table.table_name
                    ),
                ),
                (
                    "table uuid",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_uuid='{stranger}' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET table_uuid='{}' {all_rows}",
                        table.table_uuid
                    ),
                ),
                (
                    "original task",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET task_id='{stranger}' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET task_id='{}' {all_rows}",
                        settle_authority.task_id
                    ),
                ),
                (
                    "original attempt",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET attempt_id='{stranger}' {all_rows}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET attempt_id='{}' {all_rows}",
                        settle_authority.attempt_id
                    ),
                ),
                (
                    "historical worker disagreement",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET worker_id='{stranger}' {one_row}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET worker_id='{}' {all_rows}",
                        settle_authority.worker_id
                    ),
                ),
                (
                    "lease key disagreement",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET lease_key='forge:tenant_a:other' {one_row}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET lease_key='{}' {all_rows}",
                        settle_authority.lease_key
                    ),
                ),
                (
                    "lease fence disagreement",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET lease_fencing_token=99 {one_row}"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET lease_fencing_token={} {all_rows}",
                        settle_authority.lease_fencing_token
                    ),
                ),
                (
                    "selected snapshot membership",
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET snapshot_id=13 WHERE operation_id='{settle_operation}' AND snapshot_id=12"
                    ),
                    format!(
                        "UPDATE vala.forge_snapshot_expiration_claims SET snapshot_id=12 WHERE operation_id='{settle_operation}' AND snapshot_id=13"
                    ),
                ),
                (
                    "selected snapshot cardinality",
                    format!(
                        "INSERT INTO vala.forge_snapshot_expiration_claims SELECT data_tenant_id,resource,family,operation_id,13,task_id,attempt_id,worker_id,lease_key,lease_fencing_token,table_uid,catalog_name,namespace_name,table_name,table_uuid FROM vala.forge_snapshot_expiration_claims {one_row}"
                    ),
                    format!(
                        "DELETE FROM vala.forge_snapshot_expiration_claims WHERE operation_id='{settle_operation}' AND snapshot_id=13"
                    ),
                ),
            ];

            for (label, corrupt, restore) in &corruptions {
                sqlx::query(AssertSqlSafe(corrupt.as_str()))
                    .execute(&superuser)
                    .await
                    .unwrap_or_else(|error| panic!("corrupt {label}: {error}"));
                let before = durable_expiration_state(
                    &superuser,
                    tenant,
                    settle_operation,
                    settle_authority.task_id,
                )
                .await;

                let claims = ops
                    .claims_for_task(operator, tenant, settle_authority.task_id)
                    .await
                    .unwrap_or_else(|error| panic!("claim lookup for {label}: {error}"));
                assert!(
                    ops.require_claim_identity(
                        &claims,
                        settle_authority.task_id,
                        settle_authority.attempt_id,
                        &table,
                        &settle_detail,
                    )
                    .is_err(),
                    "reconciliation input must refuse a corrupted {label}"
                );
                assert!(
                    ops.reset_snapshot_expiration(operator, tenant, &settle_reset_request)
                        .await
                        .is_err(),
                    "reset must refuse a corrupted {label}"
                );
                assert!(
                    ops.settle_snapshot_expiration(operator, tenant, &settlement)
                        .await
                        .is_err(),
                    "settlement must refuse a corrupted {label}"
                );

                assert_eq!(
                    durable_expiration_state(
                        &superuser,
                        tenant,
                        settle_operation,
                        settle_authority.task_id,
                    )
                    .await,
                    before,
                    "a refused {label} leaves claims, operation, task, demand, and audit unchanged"
                );

                sqlx::query(AssertSqlSafe(restore.as_str()))
                    .execute(&superuser)
                    .await
                    .unwrap_or_else(|error| panic!("restore {label}: {error}"));
            }

            // A takeover's current authority is never compared with the
            // historical worker, lease, and fence the rows agree on.
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE vala.forge_snapshot_expiration_claims SET worker_id='{stranger}',lease_key='forge:tenant_a:historic',lease_fencing_token=3 {all_rows}"
            )))
            .execute(&superuser)
            .await
            .expect("rewrite historical authority consistently");
            let historical = ops
                .claims_for_task(operator, tenant, settle_authority.task_id)
                .await
                .expect("claim lookup after historical rewrite");
            ops.require_claim_identity(
                &historical,
                settle_authority.task_id,
                settle_authority.attempt_id,
                &table,
                &settle_detail,
            )
            .expect("a consistent historical authority is accepted under takeover");
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE vala.forge_snapshot_expiration_claims SET worker_id='{}',lease_key='{}',lease_fencing_token={} {all_rows}",
                settle_authority.worker_id,
                settle_authority.lease_key,
                settle_authority.lease_fencing_token
            )))
            .execute(&superuser)
            .await
            .expect("restore historical authority");

            let settled = ops
                .settle_snapshot_expiration(operator, tenant, &settlement)
                .await
                .expect("settlement applies");
            assert!(
                matches!(settled, ForgeOperationTransition::Applied { .. }),
                "first settlement applies: {settled:?}"
            );
            assert_eq!(
                count_claims(&superuser, settle_operation).await,
                0,
                "settlement released every claim"
            );
            assert_eq!(
                task_state_of(&superuser, settle_authority.task_id).await,
                "succeeded",
                "settlement succeeded the task in the same transaction"
            );

            let settle_replay = ops
                .settle_snapshot_expiration(operator, tenant, &settlement)
                .await
                .expect("settlement replay");
            assert!(
                matches!(
                    settle_replay,
                    ForgeOperationTransition::AlreadyApplied { .. }
                ),
                "identical settlement replay writes nothing: {settle_replay:?}"
            );

            // Reconciliation locates the operation through task-bound claims.
            assert!(
                ops.claims_for_task(operator, tenant, settle_authority.task_id)
                    .await
                    .expect("task claim lookup")
                    .is_empty(),
                "a resolved task holds no unresolved claims"
            );
        }
    }
}
