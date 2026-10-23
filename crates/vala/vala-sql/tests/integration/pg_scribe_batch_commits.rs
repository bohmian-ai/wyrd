//! Integration tests for one tenant group's Scribe batch-control fences.
//!
//! Tests verify:
//! - one statement inserts a whole tenant group and one read classifies its
//!   conflicts, in offered order, even when a batch id repeats across tables
//! - a contradiction rolls back the group's fresh fences and never resolves
//! - a changed batch under another version reports that version, so its
//!   caller can roll back the fresh fences before comparing it
//! - uncertain-commit resolution classifies a whole offered set in one read
//! - a fence offered for another tenant is refused

mod pg_tests {
    use std::sync::{Mutex, OnceLock};
    use std::thread::ThreadId;

    use chrono::{DateTime, Utc};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, SubscriberExt as _};
    use uuid::Uuid;
    use vala_sql::TenantConn;
    use vala_sql::queries::scribe_batch_commits::{
        ScribeBatchCommit, ScribeBatchCommitResolution, accepted_version, record, resolve,
    };
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;

    /// Every statement sqlx executes in this binary, with its executing thread.
    ///
    /// Installed once as the global subscriber: a callsite caches one interest
    /// for the whole process, so a thread-scoped subscriber misses statements
    /// whose callsite another test's thread reached first.
    static STATEMENTS: OnceLock<&'static StatementCapture> = OnceLock::new();

    /// Global layer that records the summary of every sqlx statement.
    #[derive(Default)]
    struct StatementCapture(Mutex<Vec<(ThreadId, String)>>);

    /// Summaries of the statements one captured future executed.
    struct StatementLog(Vec<String>);

    impl StatementLog {
        /// Runs `work` and returns its output with the statements it executed.
        ///
        /// `#[tokio::test]` polls on its own thread, so filtering by thread
        /// keeps parallel tests' statements out.
        ///
        /// # Panics
        ///
        /// Panics when another global subscriber is already installed.
        async fn capture<T>(work: impl Future<Output = T>) -> (T, Self) {
            let capture = STATEMENTS.get_or_init(|| {
                let capture: &'static StatementCapture = Box::leak(Box::default());
                tracing::subscriber::set_global_default(
                    tracing_subscriber::registry().with(capture),
                )
                .expect("install the statement capture");
                capture
            });
            let start = capture.0.lock().expect("statement log lock").len();
            let output = work.await;
            let thread = std::thread::current().id();
            let summaries = capture.0.lock().expect("statement log lock")[start..]
                .iter()
                .filter(|(executed_on, _)| *executed_on == thread)
                .map(|(_, summary)| summary.clone())
                .collect();
            (output, Self(summaries))
        }

        /// Counts executed statements whose summary starts with `prefix`.
        fn count(&self, prefix: &str) -> usize {
            self.0
                .iter()
                .filter(|summary| summary.starts_with(prefix))
                .count()
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for &'static StatementCapture {
        fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
            /// Extracts the `summary` field of one sqlx statement event.
            struct Summary(Option<String>);
            impl Visit for Summary {
                fn record_str(&mut self, field: &Field, value: &str) {
                    if field.name() == "summary" {
                        self.0 = Some(value.to_owned());
                    }
                }
                fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                    if field.name() == "summary" {
                        self.0 = Some(format!("{value:?}").trim_matches('"').to_owned());
                    }
                }
            }
            if event.metadata().target() != "sqlx::query" {
                return;
            }
            let mut summary = Summary(None);
            event.record(&mut summary);
            if let Some(summary) = summary.0 {
                self.0
                    .lock()
                    .expect("statement log lock")
                    .push((std::thread::current().id(), summary));
            }
        }
    }

    /// Builds one fence for `tenant` with fresh WAL coordinates.
    fn fence(tenant: DataTenantId, table: &str, batch_id: Uuid, lsn: i64) -> ScribeBatchCommit {
        ScribeBatchCommit {
            tenant,
            logical_table_fqn: table.to_owned(),
            batch_id,
            table_version: wyrd_spec::vala::ids::TableVersion::INITIAL,
            slice_set_digest: [u8::try_from(lsn % 251).expect("digest byte"); 32],
            slice_count: 1,
            wal_node_id: Uuid::now_v7(),
            wal_writer_epoch: 1,
            wal_shard_id: 0,
            wal_segment_sequence: 1,
            wal_lsn_min: lsn,
            wal_lsn_max: lsn + 1,
            request_id: Uuid::now_v7(),
        }
    }

    /// Records `offered` in one committed tenant transaction.
    async fn commit_group(
        fixture: &PgFixture,
        offered: &[(ScribeBatchCommit, DateTime<Utc>)],
    ) -> Vec<ScribeBatchCommitResolution> {
        let tenant = offered.first().expect("one fence").0.tenant;
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let resolved = record(&mut conn, offered).await.expect("group records");
        conn.commit().await.expect("group commits");
        resolved
    }

    /// Proves one INSERT and one conflict read classify a mixed tenant group
    /// in offered order, keyed by table as well as batch id.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the group does not record, or a
    /// fence is misclassified or costs more than one statement of each kind.
    #[tokio::test]
    async fn one_statement_records_and_classifies_a_tenant_group() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let canonical = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        let earlier = fence(tenant, "bifrost.a", Uuid::now_v7(), 20);
        commit_group(
            &fixture,
            &[
                (canonical.clone(), Utc::now()),
                (earlier.clone(), Utc::now()),
            ],
        )
        .await;

        let shared_id = Uuid::now_v7();
        let fresh_a = fence(tenant, "bifrost.a", shared_id, 30);
        let fresh_b = fence(tenant, "bifrost.b", shared_id, 40);
        let retry = ScribeBatchCommit {
            wal_lsn_min: 90,
            wal_lsn_max: 91,
            request_id: Uuid::now_v7(),
            ..earlier.clone()
        };
        let offered = [
            (fresh_a.clone(), Utc::now()),
            (canonical.clone(), Utc::now()),
            (retry.clone(), Utc::now()),
            (fresh_b.clone(), Utc::now()),
        ];

        let (resolved, log) = StatementLog::capture(commit_group(&fixture, &offered)).await;

        assert_eq!(
            resolved,
            [
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::AlreadyCommitted,
                ScribeBatchCommitResolution::Committed,
            ],
            "every offered fence keeps its own classification and position"
        );
        assert_eq!(
            log.count("INSERT INTO vala.scribe_batch_commits"),
            1,
            "the whole tenant group inserts in one statement"
        );
        assert_eq!(
            log.count("SELECT"),
            1,
            "conflicting fences classify in one read"
        );

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            resolve(&mut conn, [fresh_a, fresh_b, retry].iter())
                .await
                .expect("fences resolve"),
            [
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::AlreadyCommitted,
            ],
            "one batch id on two tables is two fences"
        );
    }

    /// Proves an all-fresh group needs no conflict read.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or the group reads after insert.
    #[tokio::test]
    async fn a_fresh_group_inserts_without_a_conflict_read() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let offered: Vec<_> = (0..8)
            .map(|lsn| {
                (
                    fence(tenant, "bifrost.a", Uuid::now_v7(), lsn * 2),
                    Utc::now(),
                )
            })
            .collect();

        let (resolved, log) = StatementLog::capture(commit_group(&fixture, &offered)).await;

        assert_eq!(resolved, [ScribeBatchCommitResolution::Committed; 8]);
        assert_eq!(log.count("INSERT INTO vala.scribe_batch_commits"), 1);
        assert_eq!(log.count("SELECT"), 0, "no fence conflicted");
    }

    /// Proves a contradiction fails the group and leaves none of its fresh
    /// fences durable.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the contradiction is accepted,
    /// or a fresh fence from the failed group survives.
    #[tokio::test]
    async fn a_contradiction_rolls_back_the_groups_fresh_fences() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let accepted = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        commit_group(&fixture, &[(accepted.clone(), Utc::now())]).await;

        let fresh = fence(tenant, "bifrost.a", Uuid::now_v7(), 20);
        let contradiction = ScribeBatchCommit {
            slice_set_digest: [0xEE; 32],
            ..accepted
        };
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        resolve(&mut conn, std::iter::once(&contradiction))
            .await
            .expect_err("uncertain-commit resolution refuses a contradiction");
        drop(conn);
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        record(
            &mut conn,
            &[(fresh.clone(), Utc::now()), (contradiction, Utc::now())],
        )
        .await
        .expect_err("different rows under a spent batch id are refused");
        drop(conn);

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            resolve(&mut conn, std::iter::once(&fresh))
                .await
                .expect("fence resolves"),
            [ScribeBatchCommitResolution::Absent],
            "the failed group committed none of its fresh fences"
        );
    }

    /// Proves a changed batch under another table version is classified as
    /// that version in the group's one INSERT and one read, and that a caller
    /// dropping the transaction leaves none of the group's fresh fences.
    ///
    /// The exceptional `accepted_version` read then names the accepting
    /// version for the spent id and nothing for a fresh one.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the mismatch is misclassified or
    /// costs more statements, a fresh fence survives the dropped transaction,
    /// or the version read disagrees with the durable fence.
    #[tokio::test]
    async fn a_changed_batch_under_another_version_reports_it_before_any_commit() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let accepted = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        commit_group(&fixture, &[(accepted.clone(), Utc::now())]).await;

        let fresh = [
            fence(tenant, "bifrost.a", Uuid::now_v7(), 20),
            fence(tenant, "bifrost.a", Uuid::now_v7(), 30),
        ];
        let next = wyrd_spec::vala::ids::TableVersion::new(2).expect("positive version");
        let changed = ScribeBatchCommit {
            table_version: next,
            slice_set_digest: [0xEE; 32],
            wal_lsn_min: 90,
            wal_lsn_max: 91,
            request_id: Uuid::now_v7(),
            ..accepted.clone()
        };
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let (resolved, log) = StatementLog::capture(record(
            &mut conn,
            &[
                (fresh[0].clone(), Utc::now()),
                (changed, Utc::now()),
                (fresh[1].clone(), Utc::now()),
            ],
        ))
        .await;
        assert_eq!(
            resolved.expect("the group records"),
            [
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::AcceptedUnder(accepted.table_version),
                ScribeBatchCommitResolution::Committed,
            ],
        );
        assert_eq!(log.count("INSERT INTO vala.scribe_batch_commits"), 1);
        assert_eq!(log.count("SELECT"), 1);
        drop(conn);

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            resolve(&mut conn, fresh.iter())
                .await
                .expect("fences resolve"),
            [ScribeBatchCommitResolution::Absent; 2],
            "the dropped transaction committed no fresh fence"
        );
        assert_eq!(
            accepted_version(&mut conn, "bifrost.a", accepted.batch_id)
                .await
                .expect("version reads"),
            Some(accepted.table_version)
        );
        assert_eq!(
            accepted_version(&mut conn, "bifrost.a", fresh[0].batch_id)
                .await
                .expect("version reads"),
            None
        );
    }

    /// Proves a fence for another tenant is refused before any statement.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or the foreign fence records.
    #[tokio::test]
    async fn a_foreign_tenant_fence_is_refused() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let foreign = fence(DataTenantId::new_v7(), "bifrost.a", Uuid::now_v7(), 10);

        record(&mut conn, &[(foreign.clone(), Utc::now())])
            .await
            .expect_err("a foreign fence is refused");
        resolve(&mut conn, std::iter::once(&foreign))
            .await
            .expect_err("a foreign fence does not resolve");
    }
}
