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

        use sqlx::PgPool;
        use sqlx::types::Uuid;
        use wyrd_dev_fixtures::pg::PgFixture;
        use wyrd_spec::DataTenantId;
        use wyrd_spec::auth::{PrincipalId, PrincipalKindTag};
        use wyrd_spec::request_id::RequestId;
        use wyrd_spec::vala::api::{
            AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod,
            ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, StoragePath,
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
                AuthMethod::Internal,
                "bifrost.forge".to_owned(),
                AuditDecision::Allow,
                AuditResult::Success,
                "redacted".to_owned(),
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
            let result = ops.append_prepared(&mut conn, event).await;
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
            let result = ops.append_terminal(&mut conn, event).await;
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
                sqlx::query_as("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1")
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
            /// Prepared evidence sequence retained by the projection.
            prepared_audit_seq: i64,
            /// Terminal evidence sequence retained after a closing transition.
            terminal_audit_seq: Option<i64>,
        }

        /// Reads one operation's persisted phase and evidence sequences.
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
            let row: (String, i64, Option<i64>) = sqlx::query_as(
                r#"
                SELECT phase, prepared_audit_seq, terminal_audit_seq
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
            StateSnapshot {
                phase: row.0,
                prepared_audit_seq: row.1,
                terminal_audit_seq: row.2,
            }
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
                ("prepared_audit_seq", "bigint", "NO"),
                ("terminal_audit_seq", "bigint", "YES"),
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
                        "forge_operation_state_check".to_owned(),
                        "CHECK ((((phase = 'prepared'::text) AND (terminal_audit_seq IS NULL)) OR ((phase <> 'prepared'::text) AND (terminal_audit_seq IS NOT NULL))))".to_owned(),
                    ),
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
                let prepared_seq =
                    match append_prepared(pool, tenant, resource(), family, &prepared_event)
                        .await
                        .expect("iceberg prepared")
                    {
                        ForgeOperationTransition::Applied { audit_seq } => audit_seq,
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
                let terminal_seq =
                    match append_terminal(pool, tenant, resource(), family, &terminal_event)
                        .await
                        .expect("iceberg terminal")
                    {
                        ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                        other => panic!("expected terminal application, got {other:?}"),
                    };

                assert_eq!(
                    state_snapshot(pool, tenant, family, operation_id).await,
                    StateSnapshot {
                        phase: persisted.to_owned(),
                        prepared_audit_seq: prepared_seq,
                        terminal_audit_seq: Some(terminal_seq),
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

            let prepared_seq =
                match append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("snapshot_expire prepared")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
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
            let terminal_seq =
                match append_terminal(pool, tenant, resource(), family, &committed_event)
                    .await
                    .expect("snapshot_expire committed")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                    other => panic!("expected terminal application, got {other:?}"),
                };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned(),
                    prepared_audit_seq: prepared_seq,
                    terminal_audit_seq: Some(terminal_seq),
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

            let prepared_seq =
                match append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("orphan_gc prepared")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
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
            let terminal_seq =
                match append_terminal(pool, tenant, resource(), family, &committed_event)
                    .await
                    .expect("orphan_gc committed")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                    other => panic!("expected terminal application, got {other:?}"),
                };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned(),
                    prepared_audit_seq: prepared_seq,
                    terminal_audit_seq: Some(terminal_seq),
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
            let prepared_seq =
                match append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("promotion prepared")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
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
            let terminal_seq =
                match append_terminal(pool, tenant, resource(), family, &committed_event)
                    .await
                    .expect("promotion committed")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                    other => panic!("expected terminal application, got {other:?}"),
                };

            assert_eq!(
                state_snapshot(pool, tenant, family, operation_id).await,
                StateSnapshot {
                    phase: "committed".to_owned(),
                    prepared_audit_seq: prepared_seq,
                    terminal_audit_seq: Some(terminal_seq),
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
        /// `vala.audit_outbox` row at either referenced sequence.
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
        #[expect(
            clippy::too_many_arguments,
            reason = "one seeded projection row is exactly these columns"
        )]
        async fn seed_state_row(
            pool: &PgPool,
            tenant: DataTenantId,
            family: ForgeOperationFamily,
            operation_id: Uuid,
            phase: &str,
            prepared_detail: &AuditDetail,
            current_detail: &AuditDetail,
            prepared_audit_seq: i64,
            terminal_audit_seq: Option<i64>,
        ) {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for seeded projection row");
            sqlx::query(
                r#"
                INSERT INTO vala.forge_operation_state
                    (data_tenant_id, resource, family, operation_id, phase,
                     prepared_detail, current_detail,
                     prepared_audit_seq, terminal_audit_seq,
                     prepared_at, updated_at)
                VALUES (wyrd.current_tenant(), $1, $2, $3, $4,
                        $5::jsonb, $6::jsonb, $7, $8, now(), now())
                "#,
            )
            .bind(resource())
            .bind(family.as_str())
            .bind(operation_id)
            .bind(phase)
            .bind(serde_json::to_string(prepared_detail).expect("serialize seeded prepared detail"))
            .bind(serde_json::to_string(current_detail).expect("serialize seeded current detail"))
            .bind(prepared_audit_seq)
            .bind(terminal_audit_seq)
            .execute(&mut **conn.transaction())
            .await
            .expect("seeded projection insert");
            conn.commit().await.expect("seeded projection commit");
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
            let result = ops.list_open(&mut conn, 8).await;
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
        /// audit sequences those transitions produced. `vala.audit_outbox` is a
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
            let prepared_seq = 4_242_i64;
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
                prepared_seq,
                None,
            )
            .await;

            // A terminal Reset rewrite operation whose audit rows are likewise
            // absent.
            let reset_id = Uuid::now_v7();
            let reset_prepared_seq = 5_150_i64;
            let reset_terminal_seq = 5_151_i64;
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
                reset_prepared_seq,
                Some(reset_terminal_seq),
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
            assert_eq!(open.operations[0].prepared_audit_seq, prepared_seq);
            assert_eq!(open.operations[0].terminal_audit_seq, None);
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
            assert_eq!(reset.operations[0].prepared_audit_seq, reset_prepared_seq);
            assert_eq!(
                reset.operations[0].terminal_audit_seq,
                Some(reset_terminal_seq)
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
                matches!(
                    replay,
                    ForgeOperationTransition::AlreadyApplied { audit_seq } if audit_seq == prepared_seq
                ),
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
            let terminal_seq = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::SnapshotExpire,
                &committed_event,
            )
            .await
            .expect("terminal settlement must not require an audit delivery row")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
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
                    phase: "committed".to_owned(),
                    prepared_audit_seq: prepared_seq,
                    terminal_audit_seq: Some(terminal_seq),
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
                matches!(
                    terminal_replay,
                    ForgeOperationTransition::AlreadyApplied { audit_seq } if audit_seq == terminal_seq
                ),
                "terminal replay returns the stored terminal sequence, got {terminal_replay:?}"
            );
            assert_eq!(
                count_audit(pool, tenant).await,
                1,
                "an idempotent terminal replay appends no audit"
            );

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
                9_001,
                None,
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
            sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry','ordinary',77,'{}'::jsonb,decode(repeat('00',32),'hex'),1,1,1,1,1,1,'running',$6,$7,now()+interval '10 minutes',77,1,now())")
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
            let prepared_event = event(
                "forge.snapshot_expire.prepared",
                resource(),
                Some(detail.clone()),
            );
            let task_prepared = event(
                "forge.task.prepared",
                &format!("forge-task:{}", authority.task_id),
                None,
            );
            let evidence = prepared_evidence();
            let ops = ForgeOperations::new(resource(), ForgeOperationFamily::SnapshotExpire)
                .expect("valid Forge resource");
            let preparation = ForgeExpirationPreparation {
                authority: &authority,
                table: &table,
                evidence: &evidence,
                operation_event: &prepared_event,
                task_event: &task_prepared,
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
            let rival_event = event(
                "forge.snapshot_expire.prepared",
                resource(),
                Some(rival_detail),
            );
            let rival = ForgeExpirationPreparation {
                authority: &authority,
                table: &table,
                evidence: &evidence,
                operation_event: &rival_event,
                task_event: &task_prepared,
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
            let reset_event = event(
                "forge.snapshot_expire.reset",
                resource(),
                Some(detail.clone()),
            );
            let reset_event = AuditEvent {
                result: AuditResult::Failure,
                ..reset_event
            };
            let task_cancelled = event(
                "forge.task.cancelled",
                &format!("forge-task:{}", authority.task_id),
                None,
            );
            let reset_request = ForgeExpirationResetRequest {
                authority: &authority,
                table: &table,
                operation_event: &reset_event,
                task_event: &task_cancelled,
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
            sqlx::query("INSERT INTO vala.forge_tasks (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,large_task_ceiling_bytes,state,attempt_id,claimed_by,claim_expires_at,watermark_snapshot_id,watermark_timestamp_ms,ready_at) VALUES ($1,$2,$3,$4,$5,'snapshot_expiry','ordinary',78,'{}'::jsonb,decode(repeat('11',32),'hex'),1,1,1,1,1,1,'running',$6,$7,now()+interval '10 minutes',78,1,now())")
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
            let settle_prepared = event(
                "forge.snapshot_expire.prepared",
                resource(),
                Some(settle_detail),
            );
            let settle_task_prepared = event(
                "forge.task.prepared",
                &format!("forge-task:{}", settle_authority.task_id),
                None,
            );
            ops.prepare_snapshot_expiration(
                operator,
                tenant,
                &ForgeExpirationPreparation {
                    authority: &settle_authority,
                    table: &table,
                    evidence: &evidence,
                    operation_event: &settle_prepared,
                    task_event: &settle_task_prepared,
                },
            )
            .await
            .expect("settle preparation applies");

            let committed_detail = expire_detail(
                settle_operation,
                ForgeSnapshotExpirePhase::Committed,
                resource(),
            );
            let committed_event = event(
                "forge.snapshot_expire.committed",
                resource(),
                Some(committed_detail),
            );
            let task_succeeded = event(
                "forge.task.succeeded",
                &format!("forge-task:{}", settle_authority.task_id),
                None,
            );
            let final_evidence = settled_evidence();
            let settlement = ForgeExpirationSettlementRequest {
                authority: &settle_authority,
                table: &table,
                settlement: ForgeExpirationSettlement::Committed,
                evidence: &final_evidence,
                operation_event: &committed_event,
                task_event: &task_succeeded,
            };
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
