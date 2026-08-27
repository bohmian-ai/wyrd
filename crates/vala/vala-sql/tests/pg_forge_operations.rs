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
            AuditDecision, AuditDetail, AuditEvent, AuditResult, AuthMethod, ForgeCompactionPhase,
            ForgeIcebergRewritePhase, ForgeOrphanGcPhase, ForgeSnapshotExpirePhase, StoragePath,
        };

        use vala_sql::queries::forge_operations::ForgeOperations;
        use vala_sql::queries::forge_tasks::FAIR_CLAIM_SQL;
        use vala_sql::row_types::forge_operations::{
            ForgeOperationFamily, ForgeOperationTransition,
        };
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
            /// Second tenant used for RLS isolation checks.
            tenant_b: DataTenantId,
        }

        /// Starts an isolated Postgres fixture with a second tenant.
        ///
        /// # Panics
        ///
        /// Panics when PostgreSQL setup, role-backed pool creation, or tenant
        /// seeding fails.
        async fn setup() -> TestFixtures {
            let fixture = PgFixture::start().await.expect("fixture");
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let tenant_b = DataTenantId::new_v7();
            fixture
                .seed_additional_tenant_with_uuid(
                    tenant_b,
                    &format!("test-{}", tenant_b.as_uuid().simple()),
                )
                .await
                .expect("seed second tenant");
            TestFixtures {
                fixture,
                superuser,
                tenant_b,
            }
        }

        /// Returns the primary logical Forge resource used by the tests.
        fn resource() -> &'static str {
            "tenant_a.ns.tbl"
        }

        /// Returns a second logical resource used for scope isolation.
        fn resource_b() -> &'static str {
            "tenant_b.ns.tbl"
        }

        /// Builds one typed compaction detail for a resource and phase.
        ///
        /// # Panics
        ///
        /// Panics only if the fixed test storage paths violate `StoragePath`.
        fn compaction_detail(phase: ForgeCompactionPhase, resource: &str) -> AuditDetail {
            AuditDetail::ForgeCompaction {
                operation_id: Uuid::now_v7(),
                phase,
                group: resource.to_owned(),
                input_file_ids: vec![Uuid::now_v7(), Uuid::now_v7()],
                input_paths: vec![
                    StoragePath::new("table/a.parquet").expect("valid path"),
                    StoragePath::new("table/b.parquet").expect("valid path"),
                ],
                output_paths: vec![StoragePath::new("table/c.parquet").expect("valid path")],
                snapshot_id: None,
            }
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

        /// Builds a Prepared compaction event with a fresh operation ID.
        ///
        /// # Panics
        ///
        /// Panics only if the fixed compaction storage paths are invalid.
        fn compact_prepared_event(resource: &str) -> AuditEvent {
            let detail = compaction_detail(ForgeCompactionPhase::Prepared, resource);
            event("forge.file_compact.prepared", resource, Some(detail))
        }

        /// Builds a Committed event preserving a Prepared operation identity.
        ///
        /// # Panics
        ///
        /// Panics when `prepared_detail` is not a compaction detail.
        fn compact_committed_event(prepared_detail: &AuditDetail, resource: &str) -> AuditEvent {
            let detail = match prepared_detail {
                AuditDetail::ForgeCompaction {
                    operation_id,
                    input_file_ids,
                    input_paths,
                    output_paths,
                    ..
                } => AuditDetail::ForgeCompaction {
                    operation_id: *operation_id,
                    phase: ForgeCompactionPhase::Committed,
                    group: resource.to_owned(),
                    input_file_ids: input_file_ids.clone(),
                    input_paths: input_paths.clone(),
                    output_paths: output_paths.clone(),
                    snapshot_id: Some(42),
                },
                _ => panic!("expected ForgeCompaction"),
            };
            event("forge.file_compact.committed", resource, Some(detail))
        }

        /// Builds a Recovered event preserving a Prepared operation identity.
        ///
        /// # Panics
        ///
        /// Panics when `prepared_detail` is not a compaction detail.
        fn compact_recovered_event(prepared_detail: &AuditDetail, resource: &str) -> AuditEvent {
            let detail = match prepared_detail {
                AuditDetail::ForgeCompaction {
                    operation_id,
                    input_file_ids,
                    input_paths,
                    output_paths,
                    ..
                } => AuditDetail::ForgeCompaction {
                    operation_id: *operation_id,
                    phase: ForgeCompactionPhase::Recovered,
                    group: resource.to_owned(),
                    input_file_ids: input_file_ids.clone(),
                    input_paths: input_paths.clone(),
                    output_paths: output_paths.clone(),
                    snapshot_id: Some(42),
                },
                _ => panic!("expected ForgeCompaction"),
            };
            event("forge.file_compact.recovered", resource, Some(detail))
        }

        /// Builds a Reset event preserving a Prepared operation identity.
        ///
        /// # Panics
        ///
        /// Panics when `prepared_detail` is not a compaction detail.
        fn compact_reset_event(prepared_detail: &AuditDetail, resource: &str) -> AuditEvent {
            let detail = match prepared_detail {
                AuditDetail::ForgeCompaction {
                    operation_id,
                    input_file_ids,
                    input_paths,
                    output_paths,
                    ..
                } => AuditDetail::ForgeCompaction {
                    operation_id: *operation_id,
                    phase: ForgeCompactionPhase::Reset,
                    group: resource.to_owned(),
                    input_file_ids: input_file_ids.clone(),
                    input_paths: input_paths.clone(),
                    output_paths: output_paths.clone(),
                    snapshot_id: Some(42),
                },
                _ => panic!("expected ForgeCompaction"),
            };
            event("forge.file_compact.reset", resource, Some(detail))
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

        /// Reads one bounded open-operation page and commits the read transaction.
        ///
        /// # Errors
        ///
        /// Returns the bounded-read error from the public Forge owner.
        ///
        /// # Panics
        ///
        /// Panics when the fixture cannot acquire or commit its tenant
        /// transaction, or when the fixed resource cannot construct the owner.
        async fn list_open(
            pool: &PgPool,
            tenant: DataTenantId,
            resource: &str,
            family: ForgeOperationFamily,
            cap: usize,
        ) -> Result<vala_sql::row_types::forge_operations::OpenForgeOperationPage, SqlError>
        {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant connection for open read");
            let ops = ForgeOperations::new(resource, family).expect("valid Forge resource");
            let result = ops.list_open(&mut conn, cap).await;
            conn.commit().await.expect("open read commit");
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
                AuditDetail::ForgeCompaction { operation_id, .. }
                | AuditDetail::ForgeIcebergRewrite { operation_id, .. }
                | AuditDetail::ForgeSnapshotExpire { operation_id, .. }
                | AuditDetail::ForgeOrphanGc { operation_id, .. } => *operation_id,
                _ => panic!("expected Forge audit detail"),
            }
        }

        /// Inserts one deliberately corrupt Prepared projection through the admin pool.
        ///
        /// The helper never mutates append-only audit evidence. Its caller
        /// chooses the prepared sequence so tests can point at absent or
        /// inconsistent evidence.
        ///
        /// # Panics
        ///
        /// Panics when detail serialization or the admin insert fails.
        async fn insert_prepared_projection(
            superuser: &PgPool,
            tenant: DataTenantId,
            resource: &str,
            family: ForgeOperationFamily,
            detail: &AuditDetail,
            prepared_audit_seq: i64,
        ) {
            let detail_json = serde_json::to_value(detail).expect("Forge detail JSON");
            sqlx::query(
                r#"
                INSERT INTO vala.forge_operation_state
                    (data_tenant_id, resource, family, operation_id, phase,
                     prepared_detail, current_detail, prepared_audit_seq,
                     terminal_audit_seq, prepared_at, updated_at)
                VALUES ($1, $2, $3, $4, 'prepared',
                        $5::jsonb, $5::jsonb, $6, NULL, now(), now())
                "#,
            )
            .bind(tenant.as_uuid())
            .bind(resource)
            .bind(family.as_str())
            .bind(forge_operation_id(detail))
            .bind(detail_json)
            .bind(prepared_audit_seq)
            .execute(superuser)
            .await
            .expect("insert corrupt prepared projection");
        }

        /// Redirects one test-owned projection to another prepared evidence sequence.
        ///
        /// # Panics
        ///
        /// Panics when the admin update does not affect exactly one row.
        async fn redirect_prepared_evidence(
            superuser: &PgPool,
            tenant: DataTenantId,
            operation_id: Uuid,
            prepared_audit_seq: i64,
        ) {
            let result = sqlx::query(
                r#"
                UPDATE vala.forge_operation_state
                   SET prepared_audit_seq = $3
                 WHERE data_tenant_id = $1
                   AND operation_id = $2
                "#,
            )
            .bind(tenant.as_uuid())
            .bind(operation_id)
            .bind(prepared_audit_seq)
            .execute(superuser)
            .await
            .expect("redirect prepared evidence");
            assert_eq!(result.rows_affected(), 1, "one projection redirected");
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
                ..
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
                        "CHECK ((family = ANY (ARRAY['staging_fold'::text, 'iceberg_rewrite'::text, 'snapshot_expire'::text, 'orphan_gc'::text])))".to_owned(),
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
        // Prepared evidence corruption
        // -----------------------------------------------------------------------

        /// Verifies a transition classifies absent prepared evidence as corruption.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, corruption arrangement, or the exact
        /// state/audit assertions fail.
        #[tokio::test]
        async fn missing_prepared_audit_is_invariant_violation_for_transition() {
            let TestFixtures {
                fixture, superuser, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared_detail = compaction_detail(ForgeCompactionPhase::Prepared, resource());
            let missing_seq = 9_000_000_001_i64;
            insert_prepared_projection(
                &superuser,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_detail,
                missing_seq,
            )
            .await;
            let before = state_snapshot(
                pool,
                tenant,
                ForgeOperationFamily::StagingFold,
                forge_operation_id(&prepared_detail),
            )
            .await;
            let terminal = compact_committed_event(&prepared_detail, resource());

            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &terminal,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::InvariantViolation { ref detail })
                    if detail == "missing prepared audit operation at seq 9000000001"
            ));
            assert_eq!(count_audit(pool, tenant).await, 0, "no audit appended");
            assert_eq!(
                state_snapshot(
                    pool,
                    tenant,
                    ForgeOperationFamily::StagingFold,
                    forge_operation_id(&prepared_detail),
                )
                .await,
                before,
                "corrupt state remains unchanged"
            );
        }

        /// Verifies the bounded read classifies absent prepared evidence as corruption.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, corruption arrangement, or exact error
        /// and cardinality assertions fail.
        #[tokio::test]
        async fn missing_prepared_audit_is_invariant_violation_for_list_open() {
            let TestFixtures {
                fixture, superuser, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared_detail = compaction_detail(ForgeCompactionPhase::Prepared, resource());
            insert_prepared_projection(
                &superuser,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_detail,
                9_000_000_002,
            )
            .await;

            let result = list_open(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::InvariantViolation { ref detail })
                    if detail == "missing prepared audit operation at seq 9000000002"
            ));
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 0);
        }

        /// Verifies a projection pointing at terminal evidence fails operation parity.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, public transitions, admin arrangement, or
        /// exact no-write assertions fail.
        #[tokio::test]
        async fn mismatched_prepared_audit_operation_is_invariant_violation() {
            let TestFixtures {
                fixture, superuser, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let target = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &target,
            )
            .await
            .expect("target prepared");
            let evidence = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &evidence,
            )
            .await
            .expect("evidence prepared");
            let evidence_terminal = compact_committed_event(
                evidence.detail.as_ref().expect("evidence detail"),
                resource(),
            );
            let terminal_seq = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &evidence_terminal,
            )
            .await
            .expect("evidence committed")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                other => panic!("expected evidence application, got {other:?}"),
            };
            let target_id = forge_operation_id(target.detail.as_ref().expect("target detail"));
            redirect_prepared_evidence(&superuser, tenant, target_id, terminal_seq).await;
            let before =
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id).await;
            let audit_before = count_audit(pool, tenant).await;
            let terminal =
                compact_committed_event(target.detail.as_ref().expect("target detail"), resource());

            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &terminal,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::InvariantViolation { ref detail })
                    if detail == "prepared audit operation mismatch: expected forge.file_compact.prepared, got forge.file_compact.committed"
            ));
            assert_eq!(count_audit(pool, tenant).await, audit_before);
            assert_eq!(
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id,).await,
                before
            );
        }

        /// Verifies a projection pointing at another resource fails resource parity.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, public transitions, admin arrangement, or
        /// exact no-write assertions fail.
        #[tokio::test]
        async fn mismatched_prepared_audit_resource_is_invariant_violation() {
            let TestFixtures {
                fixture, superuser, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let target = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &target,
            )
            .await
            .expect("target prepared");
            let evidence = compact_prepared_event(resource_b());
            let evidence_seq = match append_prepared(
                pool,
                tenant,
                resource_b(),
                ForgeOperationFamily::StagingFold,
                &evidence,
            )
            .await
            .expect("other-resource evidence prepared")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                other => panic!("expected evidence application, got {other:?}"),
            };
            let target_id = forge_operation_id(target.detail.as_ref().expect("target detail"));
            redirect_prepared_evidence(&superuser, tenant, target_id, evidence_seq).await;
            let before =
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id).await;
            let audit_before = count_audit(pool, tenant).await;
            let terminal =
                compact_committed_event(target.detail.as_ref().expect("target detail"), resource());

            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &terminal,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::InvariantViolation { ref detail })
                    if detail == "prepared audit resource mismatch: expected tenant_a.ns.tbl, got tenant_b.ns.tbl"
            ));
            assert_eq!(count_audit(pool, tenant).await, audit_before);
            assert_eq!(
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id,).await,
                before
            );
        }

        /// Verifies a projection pointing at another Prepared detail fails parity.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, public transitions, admin arrangement, or
        /// exact no-write assertions fail.
        #[tokio::test]
        async fn mismatched_prepared_audit_detail_is_invariant_violation() {
            let TestFixtures {
                fixture, superuser, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let target = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &target,
            )
            .await
            .expect("target prepared");
            let evidence = compact_prepared_event(resource());
            let evidence_seq = match append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &evidence,
            )
            .await
            .expect("other-detail evidence prepared")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                other => panic!("expected evidence application, got {other:?}"),
            };
            let target_id = forge_operation_id(target.detail.as_ref().expect("target detail"));
            redirect_prepared_evidence(&superuser, tenant, target_id, evidence_seq).await;
            let before =
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id).await;
            let audit_before = count_audit(pool, tenant).await;
            let terminal =
                compact_committed_event(target.detail.as_ref().expect("target detail"), resource());

            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &terminal,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::InvariantViolation { ref detail })
                    if detail == "prepared state detail and joined audit detail do not match"
            ));
            assert_eq!(count_audit(pool, tenant).await, audit_before);
            assert_eq!(
                state_snapshot(pool, tenant, ForgeOperationFamily::StagingFold, target_id,).await,
                before
            );
        }

        // -----------------------------------------------------------------------
        // First Prepared
        // -----------------------------------------------------------------------

        /// Verifies the first Prepared transition creates audit and state.
        ///
        /// # Panics
        ///
        /// Panics when setup, transition execution, or state assertions fail.
        #[tokio::test]
        async fn first_prepared_applies_and_returns_applied() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let event = compact_prepared_event(resource());

            let result = append_prepared(
                pool,
                fixture.data_tenant_id(),
                resource(),
                ForgeOperationFamily::StagingFold,
                &event,
            )
            .await
            .expect("first prepared");

            match result {
                ForgeOperationTransition::Applied { audit_seq } => {
                    assert!(audit_seq > 0, "seq must be positive");
                }
                _ => panic!("expected Applied, got {result:?}"),
            }

            assert_eq!(count_state(pool, fixture.data_tenant_id()).await, 1);
        }

        /// Verifies identical Prepared replay is idempotent.
        ///
        /// # Panics
        ///
        /// Panics when setup, either transition, or replay assertions fail.
        #[tokio::test]
        async fn identical_prepared_replay_is_idempotent() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let event = compact_prepared_event(resource());

            let first = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &event,
            )
            .await
            .expect("first prepared");

            let second = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &event,
            )
            .await
            .expect("second prepared");

            match (first, second) {
                (
                    ForgeOperationTransition::Applied { audit_seq: seq1 },
                    ForgeOperationTransition::AlreadyApplied { audit_seq: seq2 },
                ) => {
                    assert_eq!(
                        seq1, seq2,
                        "AlreadyApplied must return the first prepared seq"
                    );
                }
                _ => panic!("expected Applied followed by AlreadyApplied"),
            }

            // Only one state row, one Prepared audit.
            assert_eq!(count_state(pool, tenant).await, 1);
        }

        /// Verifies a same-ID Prepared event with changed canonical detail conflicts.
        ///
        /// # Panics
        ///
        /// Panics when setup, fixture detail extraction, or exact no-write
        /// assertions fail.
        #[tokio::test]
        async fn changed_prepared_detail_for_same_operation_is_conflict_without_write() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared,
            )
            .await
            .expect("first prepared");
            let operation_id =
                forge_operation_id(prepared.detail.as_ref().expect("prepared detail"));
            let changed = AuditDetail::ForgeCompaction {
                operation_id,
                phase: ForgeCompactionPhase::Prepared,
                group: resource().to_owned(),
                input_file_ids: vec![Uuid::now_v7()],
                input_paths: vec![StoragePath::new("table/changed.parquet").expect("valid path")],
                output_paths: vec![
                    StoragePath::new("table/changed-output.parquet").expect("valid path"),
                ],
                snapshot_id: None,
            };
            let collision = event("forge.file_compact.prepared", resource(), Some(changed));

            let result = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &collision,
            )
            .await;

            assert!(matches!(
                result,
                Err(SqlError::Conflict { ref detail })
                    if detail == "prepared detail does not match stored prepared detail"
            ));
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 1);
        }

        // -----------------------------------------------------------------------
        // Terminal transitions
        // -----------------------------------------------------------------------

        /// Verifies staging-fold Prepared-to-Committed state and audit persistence.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, either transition, or exact persisted
        /// phase/sequence/cardinality assertions fail.
        #[tokio::test]
        async fn prepared_then_committed_flow() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let prepared_event = compact_prepared_event(resource());
            let prepared = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("prepared");

            let prepared_seq = match prepared {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                _ => panic!("expected Applied"),
            };

            let detail = prepared_event.detail.as_ref().expect("detail");
            let committed_event = compact_committed_event(detail, resource());
            let terminal = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await
            .expect("committed");

            let terminal_seq = match terminal {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                _ => panic!("expected Applied"),
            };
            assert!(terminal_seq > prepared_seq, "terminal seq > prepared seq");
            let operation_id =
                forge_operation_id(prepared_event.detail.as_ref().expect("prepared detail"));
            assert_eq!(
                state_snapshot(
                    pool,
                    tenant,
                    ForgeOperationFamily::StagingFold,
                    operation_id,
                )
                .await,
                StateSnapshot {
                    phase: "committed".to_owned(),
                    prepared_audit_seq: prepared_seq,
                    terminal_audit_seq: Some(terminal_seq),
                }
            );
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        /// Verifies identical terminal replay is idempotent.
        ///
        /// # Panics
        ///
        /// Panics when fixture setup, transitions, or replay sequence assertions fail.
        #[tokio::test]
        async fn same_terminal_replay_is_idempotent() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("prepared");

            let detail = prepared_event.detail.as_ref().expect("detail");
            let committed_event = compact_committed_event(detail, resource());

            let first = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await
            .expect("first terminal");
            let second = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await
            .expect("second terminal");

            match (first, second) {
                (
                    ForgeOperationTransition::Applied { audit_seq: s1 },
                    ForgeOperationTransition::AlreadyApplied { audit_seq: s2 },
                ) => {
                    assert_eq!(s1, s2, "AlreadyApplied returns the same terminal seq");
                }
                _ => panic!("expected Applied then AlreadyApplied"),
            }
        }

        /// Verifies Prepared replay after Committed returns the terminal sequence.
        ///
        /// # Panics
        ///
        /// Panics when setup, transitions, or exact no-write assertions fail.
        #[tokio::test]
        async fn prepared_replay_after_committed_is_already_applied_without_write() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("prepared");
            let committed_event = compact_committed_event(
                prepared_event.detail.as_ref().expect("prepared detail"),
                resource(),
            );
            let terminal_seq = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await
            .expect("committed")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                other => panic!("expected committed application, got {other:?}"),
            };

            let replay = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("Prepared replay after Committed");

            assert!(matches!(
                replay,
                ForgeOperationTransition::AlreadyApplied { audit_seq }
                    if audit_seq == terminal_seq
            ));
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        /// Verifies Prepared replay after Recovered returns the terminal sequence.
        ///
        /// # Panics
        ///
        /// Panics when setup, transitions, or exact no-write assertions fail.
        #[tokio::test]
        async fn prepared_replay_after_recovered_is_already_applied_without_write() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("prepared");
            let recovered_event = compact_recovered_event(
                prepared_event.detail.as_ref().expect("prepared detail"),
                resource(),
            );
            let terminal_seq = match append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &recovered_event,
            )
            .await
            .expect("recovered")
            {
                ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                other => panic!("expected recovered application, got {other:?}"),
            };

            let replay = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("Prepared replay after Recovered");

            assert!(matches!(
                replay,
                ForgeOperationTransition::AlreadyApplied { audit_seq }
                    if audit_seq == terminal_seq
            ));
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        /// Verifies terminal transitions require an existing Prepared row.
        ///
        /// # Panics
        ///
        /// Panics when setup or the exact conflict assertion fails.
        #[tokio::test]
        async fn terminal_without_prepared_is_conflict() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let detail = compact_prepared_event(resource());
            let committed_event =
                compact_committed_event(detail.detail.as_ref().expect("detail"), resource());

            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await;

            assert!(result.is_err(), "terminal without prepared must fail");
            match result {
                Err(SqlError::Conflict { .. }) => {} // expected
                Err(other) => panic!("expected Conflict, got {other:?}"),
                Ok(_) => panic!("expected error"),
            }
        }

        // -----------------------------------------------------------------------
        // Reset is terminal; retry requires a new generation.
        // -----------------------------------------------------------------------

        /// Verifies Reset rejects resurrection while a new generation may retry.
        ///
        /// # Panics
        ///
        /// Panics when setup, any transition, or exact state/audit assertions fail.
        #[tokio::test]
        async fn reset_retry_requires_new_operation_generation() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            // Prepare -> Reset
            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("first prepared");

            let detail = prepared_event.detail.as_ref().expect("detail");
            let reset_event = compact_reset_event(detail, resource());
            append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &reset_event,
            )
            .await
            .expect("reset");

            // Retry the exact original Prepared detail and operation ID.
            let new_prepared_event = event(
                "forge.file_compact.prepared",
                resource(),
                Some(detail.clone()),
            );
            let reopened = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &new_prepared_event,
            )
            .await;
            assert!(reopened.is_err(), "Reset generation cannot reopen");

            let fresh_prepared = compact_prepared_event(resource());
            assert_ne!(
                fresh_prepared.detail, new_prepared_event.detail,
                "retry receives a fresh operation and output generation"
            );
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &fresh_prepared,
            )
            .await
            .expect("fresh generation prepared");

            assert_eq!(count_state(pool, tenant).await, 2);
            assert_eq!(count_audit(pool, tenant).await, 3);
        }

        /// Verifies a changed detail cannot reopen a reset operation.
        ///
        /// # Panics
        ///
        /// Panics when setup, fixed path construction, transition setup, or
        /// rejection assertions fail.
        #[tokio::test]
        async fn changed_detail_after_reset_rolls_back() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("first prepared");

            let detail = prepared_event.detail.as_ref().expect("detail");
            let reset_event = compact_reset_event(detail, resource());
            append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &reset_event,
            )
            .await
            .expect("reset");

            // Now try a Prepared with a DIFFERENT detail (changed input_file_ids)
            let changed_detail = AuditDetail::ForgeCompaction {
                operation_id: match detail {
                    AuditDetail::ForgeCompaction { operation_id, .. } => *operation_id,
                    _ => unreachable!(),
                },
                phase: ForgeCompactionPhase::Prepared,
                group: resource().to_owned(),
                input_file_ids: vec![Uuid::now_v7()], // different from original
                input_paths: vec![StoragePath::new("table/a.parquet").expect("valid path")],
                output_paths: vec![StoragePath::new("table/c.parquet").expect("valid path")],
                snapshot_id: None,
            };
            let changed_event = event(
                "forge.file_compact.prepared",
                resource(),
                Some(changed_detail),
            );

            let result = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &changed_event,
            )
            .await;

            assert!(
                result.is_err(),
                "changed detail after Reset must be rejected"
            );
        }

        // -----------------------------------------------------------------------
        // list_open: cap and overflow
        // -----------------------------------------------------------------------

        /// Verifies a page smaller than the cap returns no overflow sentinel.
        ///
        /// # Panics
        ///
        /// Panics when setup, Prepared transitions, or page assertions fail.
        #[tokio::test]
        async fn list_open_returns_exact_cap_without_overflow() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            // Insert 5 open operations with different operation_ids
            for _ in 0..5 {
                let event = compact_prepared_event(resource());
                append_prepared(
                    pool,
                    tenant,
                    resource(),
                    ForgeOperationFamily::StagingFold,
                    &event,
                )
                .await
                .expect("prepared");
            }

            let page = list_open(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await
            .expect("list_open");
            assert_eq!(page.operations.len(), 5, "all 5 open operations returned");
            assert!(!page.overflowed, "no overflow with cap=10 for 5 operations");
        }

        /// Verifies list_open returns a cap-sized page with overflow metadata.
        ///
        /// # Panics
        ///
        /// Panics when setup, Prepared transitions, or page assertions fail.
        #[tokio::test]
        async fn list_open_reports_overflow_and_returns_cap() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            for _ in 0..5 {
                let event = compact_prepared_event(resource());
                append_prepared(
                    pool,
                    tenant,
                    resource(),
                    ForgeOperationFamily::StagingFold,
                    &event,
                )
                .await
                .expect("prepared");
            }

            let page = list_open(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                3,
            )
            .await
            .expect("list_open");
            assert_eq!(page.operations.len(), 3, "cap=3, 5 open -> 3 returned");
            assert!(page.overflowed, "overflow must be true when more than cap");
        }

        /// Verifies zero-cap reads fail with a conflict.
        ///
        /// # Panics
        ///
        /// Panics when setup, transaction lifecycle, or conflict assertions fail.
        #[tokio::test]
        async fn list_open_zero_cap_returns_conflict() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
            let ops = ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold).unwrap();
            let result = ops.list_open(&mut conn, 0).await;
            conn.commit().await.unwrap();

            assert!(
                matches!(result, Err(SqlError::Conflict { .. })),
                "cap=0 must return Conflict"
            );
        }

        /// Verifies an empty projection returns an empty bounded page.
        ///
        /// # Panics
        ///
        /// Panics when setup, bounded read, or empty-page assertions fail.
        #[tokio::test]
        async fn list_open_empty_returns_empty_page() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let page = list_open(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await
            .expect("list_open on empty table");
            assert!(page.operations.is_empty(), "no open ops");
            assert!(!page.overflowed, "no overflow on empty");
        }

        // -----------------------------------------------------------------------
        // RLS isolation
        // -----------------------------------------------------------------------

        /// Verifies tenant RLS hides another tenant's projection rows.
        ///
        /// # Panics
        ///
        /// Panics when setup, transitions, bounded reads, or isolation assertions fail.
        #[tokio::test]
        async fn rls_blocks_cross_tenant_access() {
            let TestFixtures {
                fixture, tenant_b, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant_a = fixture.data_tenant_id();

            let event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant_a,
                resource(),
                ForgeOperationFamily::StagingFold,
                &event,
            )
            .await
            .expect("prepared for tenant A");

            // Tenant B should see no open operations for tenant A's resource.
            let page = list_open(
                pool,
                tenant_b,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await
            .expect("list_open for tenant B");
            assert!(
                page.operations.is_empty(),
                "tenant B must not see tenant A's open operations"
            );
        }

        /// Verifies RLS rejects inserting a row for another current tenant.
        ///
        /// # Panics
        ///
        /// Panics when setup, fixture detail extraction, transaction
        /// acquisition, or RLS assertions fail.
        #[tokio::test]
        async fn rls_rejects_cross_tenant_insert() {
            let TestFixtures {
                fixture, tenant_b, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant_a = fixture.data_tenant_id();

            let event = compact_prepared_event(resource());
            let mut conn = TenantConn::acquire(pool, tenant_b).await.unwrap();
            let detail = event.detail.as_ref().expect("prepared detail");
            let detail_json = serde_json::to_string(detail).expect("detail json");
            let operation_id = match detail {
                AuditDetail::ForgeCompaction { operation_id, .. } => *operation_id,
                _ => panic!("expected compaction detail"),
            };
            let result = sqlx::query(
                r#"
                INSERT INTO vala.forge_operation_state
                    (data_tenant_id, resource, family, operation_id, phase,
                     prepared_detail, current_detail, prepared_audit_seq,
                     terminal_audit_seq, prepared_at, updated_at)
                VALUES ($1, $2, 'staging_fold', $3, 'prepared',
                        $4::jsonb, $4::jsonb, 1, NULL, now(), now())
                "#,
            )
            .bind(tenant_a.as_uuid())
            .bind(resource())
            .bind(operation_id)
            .bind(detail_json)
            .execute(&mut **conn.transaction())
            .await;

            assert!(
                result.is_err(),
                "cross-tenant insert must be rejected by RLS"
            );
        }

        /// Verifies RLS prevents a tenant from updating another tenant's row.
        ///
        /// # Panics
        ///
        /// Panics when setup, transition execution, transaction lifecycle, or
        /// exact isolation assertions fail.
        #[tokio::test]
        async fn rls_rejects_cross_tenant_update() {
            let TestFixtures {
                fixture, tenant_b, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant_a = fixture.data_tenant_id();
            let prepared = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant_a,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared,
            )
            .await
            .expect("tenant A prepared row");

            let mut conn = TenantConn::acquire(pool, tenant_b).await.unwrap();
            let result = sqlx::query(
                "UPDATE vala.forge_operation_state SET phase = 'committed' WHERE data_tenant_id = $1",
            )
            .bind(tenant_a.as_uuid())
            .execute(&mut **conn.transaction())
            .await
            .expect("RLS filters cross-tenant update");
            conn.commit()
                .await
                .expect("cross-tenant update rollback boundary");
            assert_eq!(
                result.rows_affected(),
                0,
                "RLS must hide tenant A from tenant B"
            );
            assert_eq!(count_state(pool, tenant_a).await, 1);
        }

        // -----------------------------------------------------------------------
        // Concurrent first-Prepared serialization
        // -----------------------------------------------------------------------

        /// Verifies the advisory lock serializes concurrent first Prepared calls.
        ///
        /// # Panics
        ///
        /// Panics when setup, either concurrent transaction, or exact
        /// state/audit cardinality assertions fail.
        #[tokio::test]
        async fn concurrent_first_prepared_serializes_without_duplicate_audit() {
            let TestFixtures { fixture, .. } = setup().await;
            // Share the pool across two independent tenant connections.
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            // Both async blocks use the SAME operation_id via the same prepared detail.
            let event = compact_prepared_event(resource());

            // Run two independent tenant transactions concurrently using tokio::join!.
            // Each async block acquires its own TenantConn (separate Postgres connection),
            // so the advisory lock serializes them at the database level.
            let (result1, result2) = tokio::join!(
                async {
                    let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
                    let ops = ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold)
                        .unwrap();
                    let result = ops.append_prepared(&mut conn, &event).await;
                    if result.is_ok() {
                        conn.commit().await.unwrap();
                    }
                    result
                },
                async {
                    let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
                    let ops = ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold)
                        .unwrap();
                    let result = ops.append_prepared(&mut conn, &event).await;
                    if result.is_ok() {
                        conn.commit().await.unwrap();
                    }
                    result
                },
            );

            let result1 = result1.expect("append_prepared 1");
            let result2 = result2.expect("append_prepared 2");

            // Exactly one Applied, one AlreadyApplied.
            let applied_count = match (&result1, &result2) {
                (
                    ForgeOperationTransition::Applied { .. },
                    ForgeOperationTransition::AlreadyApplied { .. },
                )
                | (
                    ForgeOperationTransition::AlreadyApplied { .. },
                    ForgeOperationTransition::Applied { .. },
                ) => 1,
                _ => 0,
            };
            assert_eq!(
                applied_count, 1,
                "exactly one concurrent first-Prepared must apply"
            );

            // Exactly one state row.
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 1);
        }

        // -----------------------------------------------------------------------
        // Multiple resources and tenants
        // -----------------------------------------------------------------------

        /// Verifies tenant and resource scopes remain independent.
        ///
        /// # Panics
        ///
        /// Panics when setup, transitions, bounded reads, or scope assertions fail.
        #[tokio::test]
        async fn multiple_tenants_and_resources_are_independent() {
            let TestFixtures {
                fixture, tenant_b, ..
            } = setup().await;
            let pool = fixture.app_pool();
            let tenant_a = fixture.data_tenant_id();

            // Two resources for tenant A
            for resource in [resource(), resource_b()] {
                let event = compact_prepared_event(resource);
                append_prepared(
                    pool,
                    tenant_a,
                    resource,
                    ForgeOperationFamily::StagingFold,
                    &event,
                )
                .await
                .expect("prepared");
            }

            // One resource for tenant B
            let event_b = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant_b,
                resource(),
                ForgeOperationFamily::StagingFold,
                &event_b,
            )
            .await
            .expect("prepared for tenant B");

            // Tenant A sees 2 open ops for resource()
            let page_a = list_open(
                pool,
                tenant_a,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await
            .expect("list_open tenant A");
            // resource() and resource_b() are different, so page for resource() should be 1
            assert_eq!(
                page_a.operations.len(),
                1,
                "tenant A sees 1 open on resource()"
            );

            // Tenant B sees 1 open op
            let page_b = list_open(
                pool,
                tenant_b,
                resource(),
                ForgeOperationFamily::StagingFold,
                10,
            )
            .await
            .expect("list_open tenant B");
            assert_eq!(
                page_b.operations.len(),
                1,
                "tenant B sees 1 open on resource()"
            );
        }

        // -----------------------------------------------------------------------
        // Transactional failure injection
        // -----------------------------------------------------------------------

        /// Proves an insert failure rolls back its preceding audit append.
        ///
        /// # Panics
        ///
        /// Panics when setup, temporary trigger management, or rollback
        /// cardinality assertions fail.
        #[tokio::test]
        async fn audit_append_failure_rolls_back_prepared_insert() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let tenant = fixture.data_tenant_id();
            let mut admin_conn = superuser.acquire().await.expect("admin connection");
            let suffix = Uuid::now_v7().simple().to_string();
            let function_name = format!("pg_temp.reject_state_insert_{suffix}");
            let trigger_name = format!("reject_state_insert_{suffix}");

            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"
                CREATE FUNCTION {function_name}()
                RETURNS trigger AS $$
                BEGIN
                    RAISE EXCEPTION 'injected state insert failure';
                END;
                $$ LANGUAGE plpgsql
                "#,
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("create insert failure function");
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TRIGGER {trigger_name} BEFORE INSERT ON vala.forge_operation_state
                 FOR EACH ROW EXECUTE FUNCTION {function_name}()",
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("create insert failure trigger");

            let event = compact_prepared_event(resource());
            let result = {
                let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
                let ops =
                    ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold).unwrap();
                ops.append_prepared(&mut conn, &event).await
            };

            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP TRIGGER {trigger_name} ON vala.forge_operation_state",
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("drop insert failure trigger");

            assert!(
                result.is_err(),
                "state insert must fail under rejection trigger"
            );
            let state_count: (i64,) = sqlx::query_as(
                "SELECT count(*) FROM vala.forge_operation_state WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&mut *admin_conn)
            .await
            .expect("state count");
            let audit_count: (i64,) =
                sqlx::query_as("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1")
                    .bind(tenant.as_uuid())
                    .fetch_one(&mut *admin_conn)
                    .await
                    .expect("audit count");
            assert_eq!(state_count.0, 0, "failed insert must roll back projection");
            assert_eq!(audit_count.0, 0, "failed insert must roll back audit");
        }

        /// Proves a terminal state-update failure rolls back its audit append.
        ///
        /// # Panics
        ///
        /// Panics when setup, temporary trigger management, or exact rollback
        /// state/audit assertions fail.
        #[tokio::test]
        async fn state_update_failure_rolls_back_terminal_append() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let prepared_event = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &prepared_event,
            )
            .await
            .expect("prepared");
            let terminal_event = compact_committed_event(
                prepared_event.detail.as_ref().expect("prepared detail"),
                resource(),
            );

            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let mut admin_conn = superuser.acquire().await.expect("admin connection");
            let suffix = Uuid::now_v7().simple().to_string();
            let function_name = format!("pg_temp.reject_state_update_{suffix}");
            let trigger_name = format!("reject_state_update_{suffix}");
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"
                CREATE FUNCTION {function_name}()
                RETURNS trigger AS $$
                BEGIN
                    RAISE EXCEPTION 'injected state update failure';
                END;
                $$ LANGUAGE plpgsql
                "#,
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("create update failure function");
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "CREATE TRIGGER {trigger_name} BEFORE UPDATE ON vala.forge_operation_state
                 FOR EACH ROW EXECUTE FUNCTION {function_name}()",
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("create update failure trigger");

            let result = {
                let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
                let ops =
                    ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold).unwrap();
                ops.append_terminal(&mut conn, &terminal_event).await
            };

            sqlx::query(sqlx::AssertSqlSafe(format!(
                "DROP TRIGGER {trigger_name} ON vala.forge_operation_state",
            )))
            .execute(&mut *admin_conn)
            .await
            .expect("drop update failure trigger");

            assert!(
                result.is_err(),
                "state update must fail under rejection trigger"
            );
            let state_phase: (String,) = sqlx::query_as(
                "SELECT phase FROM vala.forge_operation_state WHERE data_tenant_id = $1",
            )
            .bind(tenant.as_uuid())
            .fetch_one(&mut *admin_conn)
            .await
            .expect("state phase");
            let audit_count: (i64,) =
                sqlx::query_as("SELECT count(*) FROM vala.audit_outbox WHERE data_tenant_id = $1")
                    .bind(tenant.as_uuid())
                    .fetch_one(&mut *admin_conn)
                    .await
                    .expect("audit count");
            assert_eq!(
                state_phase.0, "prepared",
                "failed update must preserve state"
            );
            assert_eq!(
                audit_count.0, 1,
                "failed update must roll back terminal audit"
            );
        }

        // -----------------------------------------------------------------------
        // Scale: constant open set with large terminal history
        // -----------------------------------------------------------------------

        /// Proves the open-state query remains bounded after a large terminal history.
        ///
        /// This test is excluded from the normal SQL lane and runs as a
        /// dedicated required GitHub Actions check for pull requests targeting
        /// `main`.
        ///
        /// # Panics
        ///
        /// Panics when setup, bulk transitions, PostgreSQL plan collection, or
        /// bounded-work assertions fail.
        #[tokio::test]
        async fn open_set_scan_remains_bounded_after_terminal_growth() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let superuser = fixture.superuser_pool().await.expect("superuser pool");
            let tenant = fixture.data_tenant_id();

            // Create 8 open operations
            let mut open_ops = Vec::new();
            for _ in 0..8 {
                let detail = AuditDetail::ForgeCompaction {
                    operation_id: Uuid::now_v7(),
                    phase: ForgeCompactionPhase::Prepared,
                    group: resource().to_owned(),
                    input_file_ids: vec![Uuid::now_v7(), Uuid::now_v7()],
                    input_paths: vec![
                        StoragePath::new("table/a.parquet").expect("valid path"),
                        StoragePath::new("table/b.parquet").expect("valid path"),
                    ],
                    output_paths: vec![StoragePath::new("table/c.parquet").expect("valid path")],
                    snapshot_id: None,
                };
                let event = event("forge.file_compact.prepared", resource(), Some(detail));
                let result = append_prepared(
                    pool,
                    tenant,
                    resource(),
                    ForgeOperationFamily::StagingFold,
                    &event,
                )
                .await
                .expect("prepared");
                assert!(
                    matches!(result, ForgeOperationTransition::Applied { .. }),
                    "expected Applied"
                );
                open_ops.push(event);
            }

            // Get baseline explain analyze
            let baseline_explain = get_explain_analyze(pool, tenant, 9).await;

            // Add terminal state identities. The original eight remain
            // Prepared and therefore remain visible through the partial index.
            let mut conn = TenantConn::acquire(pool, tenant).await.unwrap();
            let ops = ForgeOperations::new(resource(), ForgeOperationFamily::StagingFold).unwrap();

            for _ in 0..10_000 {
                let prepared_event = compact_prepared_event(resource());
                ops.append_prepared(&mut conn, &prepared_event)
                    .await
                    .expect("terminal-history prepared");
                let terminal_event = compact_committed_event(
                    prepared_event.detail.as_ref().expect("prepared detail"),
                    resource(),
                );
                ops.append_terminal(&mut conn, &terminal_event)
                    .await
                    .expect("terminal-history committed");
            }
            conn.commit().await.unwrap();

            // Remove dead partial-index entries left by the terminal updates so
            // the plan measures the live partial projection, not vacuum debt.
            let mut admin_conn = superuser.acquire().await.expect("admin connection");
            sqlx::query("VACUUM (ANALYZE) vala.forge_operation_state")
                .execute(&mut *admin_conn)
                .await
                .expect("vacuum operation state");

            // Get post-growth explain analyze
            let growth_explain = get_explain_analyze(pool, tenant, 9).await;

            // Verify plan uses forge_operation_state_open index
            assert!(
                baseline_explain.contains("forge_operation_state_open"),
                "baseline plan uses forge_operation_state_open index"
            );
            assert!(
                growth_explain.contains("forge_operation_state_open"),
                "growth plan still uses forge_operation_state_open index"
            );

            assert_plan_bounds(&baseline_explain, "baseline");
            assert_plan_bounds(&growth_explain, "growth");
            let baseline_blocks = total_shared_blocks(&baseline_explain);
            let growth_blocks = total_shared_blocks(&growth_explain);
            assert!(
                growth_blocks <= baseline_blocks + 64,
                "shared blocks grew beyond bound: baseline={baseline_blocks}, growth={growth_blocks}"
            );

            // The same registered scale lane also proves the durable task
            // queue keeps ready and terminal reads on their partial indexes.
            sqlx::query(
                r#"INSERT INTO vala.forge_tasks
                   (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,
                    base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,
                    estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,
                    large_task_ceiling_bytes,state,ready_at,created_at,updated_at)
                   SELECT (substr(md5('task-' || n::text),1,8)||'-'||substr(md5('task-' || n::text),9,4)||'-7'||substr(md5('task-' || n::text),14,3)||'-8'||substr(md5('task-' || n::text),18,3)||'-'||substr(md5('task-' || n::text),21,12))::uuid,
                          $1,'wyrd-redux','vala.bifrost','scale_'||n::text,'small_files','ordinary',n,
                          '{"version":1,"inputs":[],"parameters":{}}'::jsonb,
                          decode(repeat('07',32),'hex'),1,1,1,1,1,1,'succeeded',
                          statement_timestamp(),statement_timestamp()-interval '2 days',statement_timestamp()-interval '2 days'
                     FROM generate_series(1,10000) n"#,
            )
            .bind(tenant.as_uuid())
            .execute(&superuser)
            .await
            .expect("insert terminal task history");
            sqlx::query(
                r#"INSERT INTO vala.forge_tasks
                   (task_id,data_tenant_id,catalog_name,namespace_name,table_name,strategy,lane,
                    base_snapshot_id,plan,plan_hash,estimated_files,estimated_bytes,
                    estimated_parallelism,estimated_memory_bytes,estimated_spill_bytes,
                    large_task_ceiling_bytes,state,ready_at,created_at,updated_at)
                   SELECT (substr(md5('ready-task-' || n::text),1,8)||'-'||substr(md5('ready-task-' || n::text),9,4)||'-7'||substr(md5('ready-task-' || n::text),14,3)||'-8'||substr(md5('ready-task-' || n::text),18,3)||'-'||substr(md5('ready-task-' || n::text),21,12))::uuid,
                          $1,'wyrd-redux','vala.bifrost','ready_scale_'||n::text,'small_files','ordinary',20000+n,
                          '{"version":1,"inputs":[],"parameters":{}}'::jsonb,
                          decode(lpad(to_hex(n),64,'0'),'hex'),1,1,1,1,1,1,'ready',
                          statement_timestamp(),statement_timestamp(),statement_timestamp()
                     FROM generate_series(1,8) n"#,
            )
            .bind(tenant.as_uuid())
            .execute(&superuser)
            .await
            .expect("insert bounded ready set");
            sqlx::query("VACUUM (ANALYZE) vala.forge_tasks")
                .execute(&mut *admin_conn)
                .await
                .expect("vacuum Forge tasks");
            let task_plan = sqlx::query_scalar::<sqlx::Postgres, String>(
                "EXPLAIN (FORMAT TEXT) SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 AND state IN ('succeeded','unschedulable','failed','cancelled') AND updated_at<statement_timestamp() ORDER BY updated_at,task_id LIMIT 100",
            )
            .bind(tenant.as_uuid())
            .fetch_all(&superuser)
            .await
            .expect("explain terminal pruning")
            .join("\n");
            assert!(
                task_plan.contains("forge_tasks_terminal"),
                "terminal pruning uses its bounded partial index: {task_plan}"
            );
            let fair_explain = format!("EXPLAIN (FORMAT TEXT) {FAIR_CLAIM_SQL}");
            let fair_plan =
                sqlx::query_scalar::<sqlx::Postgres, String>(sqlx::AssertSqlSafe(fair_explain))
                    .bind(Uuid::now_v7())
                    .bind(4_i64)
                    .bind(Uuid::now_v7())
                    .bind(30_i64)
                    .bind(10_i64)
                    .bind(1_000_i64)
                    .bind(4_i32)
                    .bind(1_000_i64)
                    .bind(1_000_i64)
                    .bind(2_000_i64)
                    .bind(Option::<Vec<String>>::None)
                    .bind(Option::<String>::None)
                    .fetch_all(&superuser)
                    .await
                    .expect("explain production fair claim")
                    .join("\n");
            assert!(
                fair_plan.contains("forge_tasks_ready")
                    && fair_plan.contains("forge_tasks_publication_active"),
                "production fair claim uses bounded ready/active indexes: {fair_plan}"
            );
            assert!(
                !fair_plan.contains("forge_large_lane_lease"),
                "fair claim no longer touches the removed cluster-wide large-lane lease: {fair_plan}"
            );
            assert!(
                FAIR_CLAIM_SQL.contains("SKIP LOCKED")
                    && FAIR_CLAIM_SQL.contains("eligible_tenants")
                    && FAIR_CLAIM_SQL.contains("last_tenant_id")
                    && FAIR_CLAIM_SQL.contains("large_singleton"),
                "explained statement is the complete production fair-claim CTE"
            );
            let status_plan=sqlx::query_scalar::<sqlx::Postgres,String>("EXPLAIN (FORMAT TEXT) SELECT task_id FROM vala.forge_tasks WHERE data_tenant_id=$1 ORDER BY updated_at DESC,task_id LIMIT 9").bind(tenant.as_uuid()).fetch_all(&superuser).await.expect("explain bounded status").join("\n");
            assert!(
                status_plan.contains("forge_tasks_status"),
                "bounded status uses tenant status index: {status_plan}"
            );

            // Functional test: list_open returns 8 operations
            let page = list_open(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                8,
            )
            .await
            .expect("list_open after growth");
            assert_eq!(page.operations.len(), 8, "8 open ops after terminal growth");
            assert!(!page.overflowed, "no overflow with 8 open and cap=8");

            assert_eq!(count_state(pool, tenant).await, 10_008);
        }

        /// Runs the bounded open-state query through PostgreSQL's JSON planner.
        ///
        /// # Panics
        /// Panics when the fixture connection or explain query cannot be run.
        async fn get_explain_analyze(pool: &PgPool, tenant: DataTenantId, limit: i64) -> String {
            let mut conn = TenantConn::acquire(pool, tenant)
                .await
                .expect("tenant conn for explain");

            let explain = format!(
                r#"
                EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
                WITH selected AS MATERIALIZED (
                    SELECT *
                      FROM vala.forge_operation_state
                     WHERE data_tenant_id = wyrd.current_tenant()
                       AND resource = $1
                       AND family = 'staging_fold'
                       AND phase = 'prepared'
                     ORDER BY prepared_at, operation_id
                     LIMIT {limit}
                )
                SELECT selected.*,
                       audit.operation AS prepared_audit_operation,
                       audit.resource AS prepared_audit_resource,
                       audit.detail AS prepared_audit_detail
                  FROM selected
                  LEFT JOIN vala.audit_outbox AS audit
                    ON audit.data_tenant_id = wyrd.current_tenant()
                   AND audit.seq = selected.prepared_audit_seq
                 ORDER BY selected.prepared_at, selected.operation_id
                "#
            );

            let rows: Vec<(serde_json::Value,)> =
                sqlx::query_as(sqlx::AssertSqlSafe(explain.as_str()))
                    .bind(resource())
                    .fetch_all(&mut **conn.transaction())
                    .await
                    .expect("explain analyze");
            conn.commit().await.unwrap();

            serde_json::to_string_pretty(&rows[0].0).expect("explain json")
        }

        /// Returns all nodes in a JSON-format PostgreSQL plan tree.
        fn collect_plan_nodes<'a>(
            node: &'a serde_json::Value,
            nodes: &mut Vec<&'a serde_json::Value>,
        ) {
            nodes.push(node);
            if let Some(children) = node.get("Plans").and_then(serde_json::Value::as_array) {
                for child in children {
                    collect_plan_nodes(child, nodes);
                }
            }
        }

        /// Validates the bounded state and point-audit access plan.
        ///
        /// # Panics
        ///
        /// Panics when `explain` is not the expected JSON plan shape or when
        /// state/audit access exceeds the `cap + 1` proof bounds.
        fn assert_plan_bounds(explain: &str, label: &str) {
            let document: serde_json::Value = serde_json::from_str(explain).expect("plan json");
            let root = document
                .as_array()
                .and_then(|plans| plans.first())
                .and_then(|entry| entry.get("Plan"))
                .expect("plan root");
            let mut nodes = Vec::new();
            collect_plan_nodes(root, &mut nodes);
            assert!(
                nodes.iter().any(|node| node.get("Index Name")
                    == Some(&serde_json::Value::String(
                        "forge_operation_state_open".to_owned(),
                    ))),
                "{label} plan must use forge_operation_state_open"
            );

            let state_nodes: Vec<_> = nodes
                .iter()
                .filter(|node| {
                    node.get("Relation Name")
                        == Some(&serde_json::Value::String(
                            "forge_operation_state".to_owned(),
                        ))
                })
                .collect();
            assert!(
                !state_nodes.is_empty(),
                "{label} plan must scan operation state"
            );
            for node in state_nodes {
                let actual_rows = node
                    .get("Actual Rows")
                    .and_then(serde_json::Value::as_u64)
                    .expect("state actual rows");
                assert!(
                    actual_rows <= 9,
                    "{label} state rows exceeded cap+1: {actual_rows}"
                );
                assert_ne!(
                    node.get("Node Type").and_then(serde_json::Value::as_str),
                    Some("Seq Scan")
                );
            }

            let audit_nodes: Vec<_> = nodes
                .iter()
                .filter(|node| {
                    node.get("Relation Name")
                        == Some(&serde_json::Value::String("audit_outbox".to_owned()))
                })
                .collect();
            assert!(
                !audit_nodes.is_empty(),
                "{label} plan must point-join audit evidence"
            );
            for node in audit_nodes {
                assert_ne!(
                    node.get("Node Type").and_then(serde_json::Value::as_str),
                    Some("Seq Scan")
                );
                assert!(
                    node.get("Index Name").is_some(),
                    "{label} audit lookup must use an index"
                );
                let loops = node
                    .get("Actual Loops")
                    .and_then(serde_json::Value::as_u64)
                    .expect("audit actual loops");
                assert!(loops <= 9, "{label} audit loops exceeded cap+1: {loops}");
            }
        }

        /// Sums the shared buffer hits and reads reported by plan nodes.
        ///
        /// # Panics
        ///
        /// Panics when `explain` is not the expected JSON plan shape.
        fn total_shared_blocks(explain: &str) -> u64 {
            let document: serde_json::Value = serde_json::from_str(explain).expect("plan json");
            let root = document
                .as_array()
                .and_then(|plans| plans.first())
                .and_then(|entry| entry.get("Plan"))
                .expect("plan root");
            let mut nodes = Vec::new();
            collect_plan_nodes(root, &mut nodes);
            nodes
                .iter()
                .map(|node| {
                    node.get("Shared Hit Blocks")
                        .and_then(serde_json::Value::as_u64)
                        .unwrap_or(0)
                        + node
                            .get("Shared Read Blocks")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0)
                })
                .sum()
        }

        // -----------------------------------------------------------------------
        // Iceberg rewrite family
        // -----------------------------------------------------------------------

        /// Verifies Iceberg rewrite Prepared-to-Committed persistence.
        ///
        /// # Panics
        ///
        /// Panics when setup, typed detail construction, transitions, or exact
        /// persisted phase/sequence/cardinality assertions fail.
        #[tokio::test]
        async fn iceberg_rewrite_prepare_and_commit() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();
            let family = ForgeOperationFamily::IcebergRewrite;

            let detail = AuditDetail::ForgeIcebergRewrite {
                operation_id: Uuid::now_v7(),
                phase: ForgeIcebergRewritePhase::Prepared,
                group: resource().to_owned(),
                base_snapshot_id: 100,
                committed_snapshot_id: None,
                partition_spec_id: 3,
                time_partition: wyrd_spec::vala::api::TimePartitionWire::new(
                    wyrd_spec::vala::api::TimeGranularityWire::Day,
                    chrono::DateTime::from_timestamp(1_756_684_800, 0).expect("fixture instant"),
                )
                .expect("fixture instant is an exact day boundary"),
                target_file_size_bytes: 1024,
                input_paths: vec![StoragePath::new("table/live-a.parquet").expect("valid path")],
                output_paths: vec![
                    StoragePath::new("table/rewrite-a.parquet").expect("valid path"),
                ],
            };
            let prepared_event = event("forge.iceberg_rewrite.prepared", resource(), Some(detail));

            let prepared_seq =
                match append_prepared(pool, tenant, resource(), family, &prepared_event)
                    .await
                    .expect("iceberg prepared")
                {
                    ForgeOperationTransition::Applied { audit_seq } => audit_seq,
                    other => panic!("expected prepared application, got {other:?}"),
                };
            let prepared_detail = prepared_event.detail.as_ref().expect("prepared detail");
            let operation_id = forge_operation_id(prepared_detail);
            let committed_detail = match prepared_detail {
                AuditDetail::ForgeIcebergRewrite {
                    base_snapshot_id,
                    partition_spec_id,
                    time_partition,
                    target_file_size_bytes,
                    input_paths,
                    output_paths,
                    ..
                } => AuditDetail::ForgeIcebergRewrite {
                    operation_id,
                    phase: ForgeIcebergRewritePhase::Committed,
                    group: resource().to_owned(),
                    base_snapshot_id: *base_snapshot_id,
                    committed_snapshot_id: Some(101),
                    partition_spec_id: *partition_spec_id,
                    time_partition: *time_partition,
                    target_file_size_bytes: *target_file_size_bytes,
                    input_paths: input_paths.clone(),
                    output_paths: output_paths.clone(),
                },
                _ => panic!("expected Iceberg rewrite detail"),
            };
            let committed_event = event(
                "forge.iceberg_rewrite.committed",
                resource(),
                Some(committed_detail),
            );
            let terminal_seq =
                match append_terminal(pool, tenant, resource(), family, &committed_event)
                    .await
                    .expect("iceberg committed")
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

        /// Verifies a wrong family operation prefix is rejected.
        ///
        /// # Panics
        ///
        /// Panics when setup, fixed path construction, or rejection assertions fail.
        #[tokio::test]
        async fn mismatched_operation_prefix_is_rejected() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let detail = AuditDetail::ForgeCompaction {
                operation_id: Uuid::now_v7(),
                phase: ForgeCompactionPhase::Prepared,
                group: resource().to_owned(),
                input_file_ids: vec![],
                input_paths: vec![],
                output_paths: vec![],
                snapshot_id: None,
            };
            // Wrong operation prefix for StagingFold
            let bad_event = event("forge.iceberg_rewrite.prepared", resource(), Some(detail));

            let result = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &bad_event,
            )
            .await;
            assert!(matches!(
                result,
                Err(SqlError::Conflict { ref detail })
                    if detail == "operation forge.iceberg_rewrite.prepared does not match family prefix forge.file_compact"
            ));
            assert_eq!(count_state(pool, tenant).await, 0);
            assert_eq!(count_audit(pool, tenant).await, 0);
        }

        /// Verifies events without typed detail are rejected.
        ///
        /// # Panics
        ///
        /// Panics when setup or the exact rejection assertion fails.
        #[tokio::test]
        async fn event_without_detail_is_rejected() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let no_detail_event = event("forge.file_compact.prepared", resource(), None);
            let result = append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &no_detail_event,
            )
            .await;
            assert!(matches!(
                result,
                Err(SqlError::Conflict { ref detail })
                    if detail == "event must carry a typed audit detail"
            ));
            assert_eq!(count_state(pool, tenant).await, 0);
            assert_eq!(count_audit(pool, tenant).await, 0);
        }

        /// Verifies unsupported terminal ordering is rejected.
        ///
        /// # Panics
        ///
        /// Panics when setup, transitions, detail conversion, or conflict
        /// assertions fail.
        #[tokio::test]
        async fn wrong_terminal_phase_is_rejected() {
            let TestFixtures { fixture, .. } = setup().await;
            let pool = fixture.app_pool();
            let tenant = fixture.data_tenant_id();

            let detail = compact_prepared_event(resource());
            append_prepared(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &detail,
            )
            .await
            .expect("prepared");

            let committed_event = compact_committed_event(
                detail.detail.as_ref().expect("prepared detail"),
                resource(),
            );
            append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &committed_event,
            )
            .await
            .expect("committed");

            // Try to recover instead of commit
            let orig_detail = detail.detail.as_ref().unwrap();
            let rec_detail = match orig_detail {
                AuditDetail::ForgeCompaction {
                    operation_id,
                    input_file_ids,
                    input_paths,
                    output_paths,
                    ..
                } => AuditDetail::ForgeCompaction {
                    operation_id: *operation_id,
                    phase: ForgeCompactionPhase::Recovered,
                    group: resource().to_owned(),
                    input_file_ids: input_file_ids.clone(),
                    input_paths: input_paths.clone(),
                    output_paths: output_paths.clone(),
                    snapshot_id: Some(42),
                },
                _ => unreachable!(),
            };
            let rec_event = event("forge.file_compact.recovered", resource(), Some(rec_detail));
            let result = append_terminal(
                pool,
                tenant,
                resource(),
                ForgeOperationFamily::StagingFold,
                &rec_event,
            )
            .await;
            assert!(
                matches!(
                    result,
                    Err(SqlError::Conflict { ref detail })
                        if detail == "cannot append terminal: operation is in Committed state"
                ),
                "a distinct terminal phase after Committed must conflict"
            );
            assert_eq!(count_state(pool, tenant).await, 1);
            assert_eq!(count_audit(pool, tenant).await, 2);
        }

        /// Verifies construction rejects an empty resource identity.
        ///
        /// # Panics
        ///
        /// Panics when the constructor does not return the exact conflict class.
        #[tokio::test]
        async fn empty_resource_is_rejected() {
            let result = ForgeOperations::new("", ForgeOperationFamily::StagingFold);
            assert!(
                matches!(result, Err(SqlError::Conflict { .. })),
                "empty resource must return Conflict"
            );
        }
    }
}
