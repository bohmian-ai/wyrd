//! Integration tests for one tenant group's Scribe batch-control fences.
//!
//! Tests verify:
//! - one statement inserts a whole tenant group and one read classifies its
//!   conflicts, in offered order, even when a batch id repeats across tables
//! - different rows under a spent batch id are a per-batch conflict, and a
//!   caller dropping the transaction keeps none of the group's fresh fences
//! - a changed batch under another version reports that version, so its
//!   caller can roll back the fresh fences before comparing it
//! - uncertain-commit resolution classifies a whole offered set in one read
//! - a fence offered for another tenant is refused
//! - a durable-fence lookup names exactly the offered batch ids that hold a
//!   fence on their table
//! - receipt deletion removes only receipts of the named stream shard at or
//!   below its safe segment and outside the age/count retry history
//! - a sweep's cutoff bounds deletion to receipts accepted before it
//! - a node's receipt streams are discovered across tenants and epochs

mod pg_tests {
    use std::sync::{Mutex, OnceLock};
    use std::thread::ThreadId;

    use chrono::{DateTime, Utc};
    use tracing::field::{Field, Visit};
    use tracing_subscriber::layer::{Context, SubscriberExt as _};
    use uuid::Uuid;
    use vala_sql::TenantConn;
    use vala_sql::queries::scribe_batch_commits::{
        HandedOffShard, ReceiptStream, RetryHistory, ScribeBatchCommit,
        ScribeBatchCommitResolution, accepted_version, committed_batches, delete_handed_off,
        receipt_streams, record, resolve,
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
            slice_set_digest: [u8::try_from(lsn % 251).expect("digest byte"); 16],
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

    /// Proves different rows under a spent batch id are a conflict of that
    /// batch alone, and that dropping the group's transaction leaves none of
    /// its fresh fences durable.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, the conflict is misclassified,
    /// or a fresh fence from the dropped group survives.
    #[tokio::test]
    async fn a_conflict_is_refused_per_batch_and_leaves_no_fresh_fence() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let accepted = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        commit_group(&fixture, &[(accepted.clone(), Utc::now())]).await;

        let fresh = fence(tenant, "bifrost.a", Uuid::now_v7(), 20);
        let conflicting = ScribeBatchCommit {
            slice_set_digest: [0xEE; 16],
            ..accepted
        };
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            resolve(&mut conn, std::iter::once(&conflicting))
                .await
                .expect("a conflict resolves"),
            [ScribeBatchCommitResolution::Conflicted],
            "uncertain-commit resolution names the conflict"
        );
        drop(conn);
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            record(
                &mut conn,
                &[(fresh.clone(), Utc::now()), (conflicting, Utc::now())],
            )
            .await
            .expect("the group records"),
            [
                ScribeBatchCommitResolution::Committed,
                ScribeBatchCommitResolution::Conflicted
            ],
            "only the batch reusing a spent id conflicts"
        );
        drop(conn);

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        assert_eq!(
            resolve(&mut conn, std::iter::once(&fresh))
                .await
                .expect("fence resolves"),
            [ScribeBatchCommitResolution::Absent],
            "the dropped group committed none of its fresh fences"
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
            slice_set_digest: [0xEE; 16],
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

    /// Proves the durable-fence lookup answers exactly the offered ids that
    /// hold a fence on the named table: not a fresh id, and not an id fenced
    /// only on another table.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or the lookup answers another set.
    #[tokio::test]
    async fn the_fence_lookup_names_exactly_the_committed_batches() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let shared = Uuid::now_v7();
        let fenced = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        commit_group(
            &fixture,
            &[
                (fenced.clone(), Utc::now()),
                (fence(tenant, "bifrost.b", shared, 20), Utc::now()),
            ],
        )
        .await;
        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let found = committed_batches(
            &mut conn,
            "bifrost.a",
            &[Uuid::now_v7(), fenced.batch_id, shared],
        )
        .await
        .expect("the lookup runs");
        assert_eq!(found, [fenced.batch_id]);
    }

    /// Deletes only receipts the named shard handed off that left recent
    /// retry history, by age or by count, and leaves every other receipt.
    ///
    /// One shard stream holds an old receipt at the safe segment, an old one
    /// above it, and a recent one below it; another stream holds an old and
    /// the newest. Only the first goes. Shrinking the count history to one
    /// identity then removes the recent one too, since another stream's
    /// receipt is newer, while every uncovered or foreign receipt stays. A
    /// deleted identity accepted again above the frontier is not removed.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a statement fails, or the
    /// remaining receipts diverge from the eligibility rule.
    #[tokio::test]
    async fn only_handed_off_receipts_outside_retry_history_are_deleted() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let old = Utc::now() - chrono::Duration::hours(2);
        let at = |lsn, segment| ScribeBatchCommit {
            wal_segment_sequence: segment,
            ..fence(tenant, "bifrost.a", Uuid::now_v7(), lsn)
        };
        let handed_off = at(10, 3);
        let shard = HandedOffShard {
            node_id: handed_off.wal_node_id,
            writer_epoch: 1,
            shard_id: 0,
            safe_segment: 3,
        };
        let same_stream = |commit: ScribeBatchCommit| ScribeBatchCommit {
            wal_node_id: shard.node_id,
            ..commit
        };
        let uncovered = same_stream(at(20, 4));
        let recent = same_stream(at(30, 2));
        let other_stream = at(40, 1);
        let newest = at(50, 1);
        commit_group(
            &fixture,
            &[
                (handed_off.clone(), old),
                (uncovered.clone(), old),
                (other_stream.clone(), old),
                (recent.clone(), Utc::now()),
                (newest.clone(), Utc::now() + chrono::Duration::minutes(1)),
            ],
        )
        .await;
        let remaining = async |history: RetryHistory| {
            let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                .await
                .expect("tenant connection");
            let deleted = delete_handed_off(&mut conn, &shard, &history, Utc::now(), 100)
                .await
                .expect("the delete runs")
                .len();
            let mut left = committed_batches(
                &mut conn,
                "bifrost.a",
                &[
                    handed_off.batch_id,
                    uncovered.batch_id,
                    recent.batch_id,
                    other_stream.batch_id,
                    newest.batch_id,
                ],
            )
            .await
            .expect("the lookup runs");
            conn.commit().await.expect("commit");
            left.sort();
            (deleted, left)
        };
        let sorted = |mut ids: Vec<Uuid>| {
            ids.sort();
            ids
        };

        let (deleted, left) = remaining(RetryHistory::default()).await;
        assert_eq!(deleted, 1);
        assert_eq!(
            left,
            sorted(vec![
                uncovered.batch_id,
                recent.batch_id,
                other_stream.batch_id,
                newest.batch_id
            ])
        );

        let (deleted, left) = remaining(RetryHistory {
            newest: 1,
            ..RetryHistory::default()
        })
        .await;
        assert_eq!(deleted, 1, "the recent receipt is outside the newest one");
        assert_eq!(
            left,
            sorted(vec![
                uncovered.batch_id,
                other_stream.batch_id,
                newest.batch_id
            ])
        );

        // A deleted identity accepted again above the frontier is a fresh
        // receipt; the same handed-off shard cannot remove it.
        let replacement = ScribeBatchCommit {
            batch_id: handed_off.batch_id,
            ..same_stream(at(60, 4))
        };
        commit_group(&fixture, &[(replacement, old)]).await;
        let (deleted, left) = remaining(RetryHistory::default()).await;
        assert_eq!(deleted, 0, "the replacement acceptance stays");
        assert!(left.contains(&handed_off.batch_id));
    }

    /// A sweep deletes only receipts accepted at or before its cutoff, so
    /// receipts arriving during the sweep cannot keep it running.
    ///
    /// Two covered receipts leave a one-identity count history: one accepted
    /// before the cutoff, one after. Only the first goes, and its batch id is
    /// returned so the caller can forget it.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a statement fails, or the
    /// deletion crosses the cutoff.
    #[tokio::test]
    async fn a_sweep_cutoff_bounds_deletion_to_receipts_accepted_before_it() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let cutoff = Utc::now();
        let before = fence(tenant, "bifrost.a", Uuid::now_v7(), 10);
        let shard = HandedOffShard {
            node_id: before.wal_node_id,
            writer_epoch: 1,
            shard_id: 0,
            safe_segment: 1,
        };
        let after = ScribeBatchCommit {
            wal_node_id: shard.node_id,
            ..fence(tenant, "bifrost.a", Uuid::now_v7(), 20)
        };
        let newest = fence(tenant, "bifrost.a", Uuid::now_v7(), 30);
        commit_group(
            &fixture,
            &[
                (before.clone(), cutoff - chrono::Duration::minutes(1)),
                (after.clone(), cutoff + chrono::Duration::minutes(1)),
                (newest, cutoff + chrono::Duration::minutes(2)),
            ],
        )
        .await;

        let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
            .await
            .expect("tenant connection");
        let deleted = delete_handed_off(
            &mut conn,
            &shard,
            &RetryHistory {
                newest: 1,
                ..RetryHistory::default()
            },
            cutoff,
            100,
        )
        .await
        .expect("the delete runs");
        conn.commit().await.expect("commit");

        assert_eq!(
            deleted,
            [before.batch_id],
            "only the receipt before the cutoff goes"
        );
    }

    /// A sweep DELETE grows linearly with the receipts it reads, never
    /// quadratically, when the table grew faster than its planner statistics.
    ///
    /// A write load inserts receipts faster than autovacuum analyzes them, so
    /// the planner believes the shard holds about one row. One shard's fresh
    /// receipts over four tables, none yet due, are recorded with autovacuum
    /// off so they are never analyzed, and the DELETE is timed at 10,000, 20,000 and 40,000 of them
    /// (the median of three runs each, since none deletes anything). Linear
    /// cost grows about 4x from the smallest to the largest size and quadratic
    /// cost about 16x, so growth must stay under 8x; the largest size must
    /// also finish within 3 s. A plan that re-ranks the table once per shard
    /// row took 103 s at 30,000.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a statement fails, a receipt is
    /// deleted, the cost grows 8x or more from 10,000 to 40,000 receipts, or
    /// the largest DELETE takes 3 s or longer.
    #[tokio::test]
    async fn a_sweep_delete_grows_linearly_over_unanalyzed_receipts() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let node_id = Uuid::now_v7();
        let accepted = Utc::now();
        // Autovacuum would analyze the table during a slow DELETE and hand the
        // larger sizes a plan the write load never gets.
        sqlx::query("ALTER TABLE vala.scribe_batch_commits SET (autovacuum_enabled = false)")
            .execute(&fixture.superuser_pool().expect("superuser pool"))
            .await
            .expect("statistics stay as the write load leaves them");
        let shard = HandedOffShard {
            node_id,
            writer_epoch: 1,
            shard_id: 0,
            safe_segment: 1,
        };
        let median_delete = async || {
            let mut runs = Vec::new();
            for _ in 0..3 {
                let mut conn = TenantConn::acquire(fixture.app_pool(), tenant)
                    .await
                    .expect("tenant connection");
                let started = std::time::Instant::now();
                let deleted = delete_handed_off(
                    &mut conn,
                    &shard,
                    &RetryHistory::default(),
                    accepted + chrono::Duration::seconds(1),
                    1_000,
                )
                .await
                .expect("the delete runs");
                runs.push(started.elapsed());
                conn.commit().await.expect("commit");
                assert!(deleted.is_empty(), "no receipt is due yet");
            }
            runs.sort();
            runs[1]
        };

        let mut held = 0_i64;
        let mut timings = Vec::new();
        for size in [10_000_i64, 20_000, 40_000] {
            let receipts: Vec<_> = (held..size)
                .map(|lsn| {
                    let table = format!("bifrost.t{}", lsn % 4);
                    let receipt = ScribeBatchCommit {
                        wal_node_id: node_id,
                        ..fence(tenant, &table, Uuid::now_v7(), lsn * 2)
                    };
                    (receipt, accepted)
                })
                .collect();
            for group in receipts.chunks(5_000) {
                commit_group(&fixture, group).await;
            }
            held = size;
            timings.push((size, median_delete().await));
        }

        let (_, smallest) = timings[0];
        let (_, largest) = timings[2];
        assert!(
            largest < smallest * 8,
            "4x the receipts cost {:.1}x the time: {timings:?}",
            largest.as_secs_f64() / smallest.as_secs_f64()
        );
        assert!(
            largest < std::time::Duration::from_secs(3),
            "one sweep DELETE took {largest:?} over 40,000 unanalyzed receipts"
        );
    }

    /// A node's receipt streams are listed across every tenant and writer
    /// epoch, and another node's are not.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start, a statement fails, or the
    /// discovered streams differ from the recorded ones.
    #[tokio::test]
    async fn receipt_streams_of_a_node_span_tenants_and_epochs() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let other_tenant = fixture
            .seed_additional_tenant("receipt-streams-other")
            .await
            .expect("second tenant");
        let node = Uuid::now_v7();
        let on = |tenant, epoch, shard, lsn| ScribeBatchCommit {
            wal_node_id: node,
            wal_writer_epoch: epoch,
            wal_shard_id: shard,
            ..fence(tenant, "bifrost.a", Uuid::now_v7(), lsn)
        };
        commit_group(
            &fixture,
            &[
                (on(tenant, 1, 0, 10), Utc::now()),
                (on(tenant, 1, 0, 12), Utc::now()),
                (on(tenant, 2, 3, 20), Utc::now()),
                (fence(tenant, "bifrost.a", Uuid::now_v7(), 30), Utc::now()),
            ],
        )
        .await;
        commit_group(&fixture, &[(on(other_tenant, 2, 3, 40), Utc::now())]).await;

        let mut streams = receipt_streams(fixture.operator_pool(), node)
            .await
            .expect("the discovery runs");
        streams.sort_by_key(|stream| (stream.writer_epoch, stream.shard_id, stream.tenant));
        let mut expected = vec![
            ReceiptStream {
                tenant,
                writer_epoch: 1,
                shard_id: 0,
            },
            ReceiptStream {
                tenant,
                writer_epoch: 2,
                shard_id: 3,
            },
            ReceiptStream {
                tenant: other_tenant,
                writer_epoch: 2,
                shard_id: 3,
            },
        ];
        expected.sort_by_key(|stream| (stream.writer_epoch, stream.shard_id, stream.tenant));
        assert_eq!(streams, expected);
    }
}
