mod pg_tests {
    //! Durable proof of the code-review cache metadata statements.
    //!
    //! Every case runs on real Postgres through tenant RLS connections, plus
    //! the operator pool for the cross-tenant expiry sweep: pointer
    //! publication under competition, collection claims against touches and
    //! pointers with retry, context stamps that never extend, database-clock
    //! expiry, a touch racing a claim, and tenant, installation and repository
    //! isolation. Without a database they fail at fixture
    //! start rather than pass.

    use std::time::Duration;

    use serde_json::{Value, json};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::DataTenantId;
    use wyrd_sql::queries::code_review_cache::{
        CodeReviewRepository, CodeReviewSnapshotScope, claim_unreferenced_code_review_snapshots,
        code_review_context, code_review_snapshot_pointer, delete_code_review_context,
        delete_code_review_contexts, delete_code_review_snapshot_pointers,
        delete_code_review_snapshot_record, expired_code_review_contexts,
        publish_code_review_snapshot_pointer, touch_code_review_snapshot_record,
        upsert_code_review_context,
    };

    /// The repository every case caches.
    const REPO: CodeReviewRepository = CodeReviewRepository {
        installation_id: 11,
        repository_id: 7,
    };

    /// The compatibility scope every snapshot case publishes under.
    fn scope() -> CodeReviewSnapshotScope<'static> {
        CodeReviewSnapshotScope {
            repository: REPO,
            extraction_version: "v1",
            format_version: 1,
            max_file_bytes: 1024,
        }
    }

    /// A serialized pointer naming `key` for `tree`.
    fn pointer(tree: &str, key: &str) -> Value {
        json!({"tree": tree, "commit": tree, "key": key, "digest": "d"})
    }

    /// Publish `tree` naming `key` in `tenant` when the current tree is
    /// `expected`, and commit.
    ///
    /// # Panics
    /// Panics when a statement or the commit fails.
    async fn publish(
        fixture: &PgFixture,
        tenant: DataTenantId,
        expected: Option<&str>,
        tree: &str,
        key: &str,
    ) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let moved = publish_code_review_snapshot_pointer(
            &mut conn,
            scope(),
            expected,
            &pointer(tree, key),
            tree,
        )
        .await
        .expect("publish");
        conn.commit().await.expect("commit");
        moved
    }

    /// Touch `key` in `tenant` and commit.
    ///
    /// # Panics
    /// Panics when a statement or the commit fails.
    async fn touch(fixture: &PgFixture, tenant: DataTenantId, key: &str) -> bool {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let touched = touch_code_review_snapshot_record(&mut conn, REPO, key)
            .await
            .expect("touch");
        conn.commit().await.expect("commit");
        touched
    }

    /// Claim `tenant`'s unreferenced records older than `grace` and commit.
    ///
    /// # Panics
    /// Panics when a statement or the commit fails.
    async fn claim(fixture: &PgFixture, tenant: DataTenantId, grace: Duration) -> Vec<String> {
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let mut keys = claim_unreferenced_code_review_snapshots(&mut conn, REPO, grace)
            .await
            .expect("claim");
        conn.commit().await.expect("commit");
        keys.sort();
        keys
    }

    /// Concurrent first publications of one scope converge on one winner, and
    /// a later swap moves the pointer only from the tree it names.
    ///
    /// # Panics
    /// Panics when more or fewer than one first publication wins or a swap
    /// from the wrong tree moves the pointer.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn snapshot_pointers_publish_once_and_swap_on_their_tree() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();

        let (first, second) = tokio::join!(
            publish(&fixture, tenant, None, "tree-a", "key-a"),
            publish(&fixture, tenant, None, "tree-b", "key-b"),
        );
        assert!(first ^ second, "exactly one first publication wins");
        let won = if first { "tree-a" } else { "tree-b" };
        let lost = if first { "tree-b" } else { "tree-a" };

        assert!(
            !publish(&fixture, tenant, Some(lost), "tree-c", "key-c").await,
            "a swap from a tree the pointer no longer names is refused"
        );
        assert!(publish(&fixture, tenant, Some(won), "tree-c", "key-c").await);

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        assert_eq!(
            code_review_snapshot_pointer(&mut conn, scope())
                .await
                .expect("read"),
            Some(pointer("tree-c", "key-c"))
        );
        let other_ceiling = CodeReviewSnapshotScope {
            max_file_bytes: 2048,
            ..scope()
        };
        assert_eq!(
            code_review_snapshot_pointer(&mut conn, other_ceiling)
                .await
                .expect("read"),
            None,
            "another compatibility scope has its own pointer"
        );
    }

    /// Collection claims only unreferenced records older than the grace, a
    /// claimed record refuses touches, an interrupted collection's claims are
    /// returned again, and a deleted record can be recreated.
    ///
    /// # Panics
    /// Panics when a claim includes a referenced or recently touched record,
    /// a claimed record accepts a touch, or a retry loses a claim.
    #[tokio::test]
    async fn collection_claims_race_touches_and_pointers_and_retries() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();

        assert!(touch(&fixture, tenant, "key-live").await);
        assert!(touch(&fixture, tenant, "key-old").await);
        assert!(publish(&fixture, tenant, None, "tree-live", "key-live").await);

        assert_eq!(
            claim(&fixture, tenant, Duration::from_secs(3600)).await,
            Vec::<String>::new(),
            "records touched within the grace are not claimed"
        );
        assert_eq!(
            claim(&fixture, tenant, Duration::ZERO).await,
            vec!["key-old".to_owned()],
            "only the record no pointer names is claimed"
        );
        assert!(
            !touch(&fixture, tenant, "key-old").await,
            "a claimed record refuses a touch"
        );
        assert_eq!(
            claim(&fixture, tenant, Duration::ZERO).await,
            vec!["key-old".to_owned()],
            "an interrupted collection retried with its grace finds its claim again"
        );

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        delete_code_review_snapshot_record(&mut conn, REPO, "key-old")
            .await
            .expect("delete");
        delete_code_review_snapshot_record(&mut conn, REPO, "key-old")
            .await
            .expect("deleting an absent record succeeds");
        delete_code_review_snapshot_pointers(&mut conn, REPO)
            .await
            .expect("remove pointers");
        conn.commit().await.expect("commit");

        assert!(
            touch(&fixture, tenant, "key-old").await,
            "a deleted record is recreated"
        );
        assert_eq!(
            claim(&fixture, tenant, Duration::ZERO).await,
            vec!["key-live".to_owned(), "key-old".to_owned()],
            "removing the pointers leaves every record unreferenced"
        );
    }

    /// A repeated context publication keeps its first stamps and record,
    /// lookup distinguishes absent, live and expired on the database clock,
    /// and the operator sweep names each expired context's tenant.
    ///
    /// # Panics
    /// Panics when a repeat extends the stamps, a lookup misclassifies, or
    /// the sweep misses or misattributes an expired context.
    #[tokio::test]
    async fn contexts_keep_their_stamps_and_expire_on_the_database_clock() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let identity = json!({"tenant": tenant.to_string()});
        let record = json!({"id": "ctx-live", "key": "k-live"});

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let first = upsert_code_review_context(
            &mut conn,
            REPO,
            "ctx-live",
            &identity,
            &record,
            Duration::from_secs(3600),
        )
        .await
        .expect("upsert");
        conn.commit().await.expect("commit");
        assert_eq!(
            first.expires_at - first.published_at,
            chrono::Duration::seconds(3600)
        );

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let repeat = upsert_code_review_context(
            &mut conn,
            REPO,
            "ctx-live",
            &identity,
            &json!({"id": "ctx-live", "key": "k-other"}),
            Duration::from_secs(7200),
        )
        .await
        .expect("repeat upsert");
        upsert_code_review_context(
            &mut conn,
            REPO,
            "ctx-old",
            &identity,
            &json!({"id": "ctx-old", "key": "k-old"}),
            Duration::from_micros(1),
        )
        .await
        .expect("short upsert");
        conn.commit().await.expect("commit");
        assert_eq!(repeat, first, "a repeat keeps the original stamps");

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        let live = code_review_context(&mut conn, REPO, "ctx-live")
            .await
            .expect("lookup")
            .expect("present");
        assert_eq!(live.record, record, "a repeat keeps the original record");
        assert!(!live.expired);
        assert!(
            code_review_context(&mut conn, REPO, "ctx-old")
                .await
                .expect("lookup")
                .expect("present")
                .expired
        );
        assert_eq!(
            code_review_context(&mut conn, REPO, "ctx-none")
                .await
                .expect("lookup"),
            None
        );
        drop(conn);

        let swept = expired_code_review_contexts(fixture.operator_pool(), 100)
            .await
            .expect("sweep");
        let ours: Vec<_> = swept
            .iter()
            .filter(|row| row.data_tenant_id == tenant.as_uuid())
            .collect();
        assert_eq!(ours.len(), 1, "only the expired context is swept: {ours:?}");
        assert_eq!(ours[0].record["key"], "k-old");
        assert_eq!(ours[0].identity, identity);

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        delete_code_review_context(&mut conn, REPO, "ctx-old")
            .await
            .expect("delete");
        delete_code_review_context(&mut conn, REPO, "ctx-old")
            .await
            .expect("deleting an absent context succeeds");
        assert_eq!(
            delete_code_review_contexts(&mut conn, REPO)
                .await
                .expect("remove repository"),
            vec!["k-live".to_owned()]
        );
        conn.commit().await.expect("commit");
    }

    /// Another tenant sees none of a tenant's cache rows, cannot claim its
    /// records, and keeps independent rows under the same keys.
    ///
    /// # Panics
    /// Panics when any row crosses the tenant boundary.
    #[tokio::test]
    async fn cache_rows_stay_inside_their_tenant() {
        let fixture = PgFixture::start().await.expect("fixture");
        let owner = fixture.data_tenant_id();
        let other = fixture
            .seed_additional_tenant("code-review-other")
            .await
            .expect("second tenant");

        assert!(touch(&fixture, owner, "key-a").await);
        assert!(publish(&fixture, owner, None, "tree-a", "key-b").await);
        let mut conn = fixture.tenant_conn_for(owner).await.expect("conn opens");
        upsert_code_review_context(
            &mut conn,
            REPO,
            "ctx",
            &json!({}),
            &json!({"key": "k"}),
            Duration::from_secs(3600),
        )
        .await
        .expect("upsert");
        conn.commit().await.expect("commit");

        let mut conn = fixture.tenant_conn_for(other).await.expect("conn opens");
        assert_eq!(
            code_review_snapshot_pointer(&mut conn, scope())
                .await
                .expect("read"),
            None
        );
        assert_eq!(
            code_review_context(&mut conn, REPO, "ctx")
                .await
                .expect("read"),
            None
        );
        assert_eq!(
            delete_code_review_contexts(&mut conn, REPO)
                .await
                .expect("remove"),
            Vec::<String>::new()
        );
        conn.commit().await.expect("commit");
        assert_eq!(
            claim(&fixture, other, Duration::ZERO).await,
            Vec::<String>::new()
        );
        assert!(
            publish(&fixture, other, None, "tree-x", "key-x").await,
            "the other tenant publishes its own first pointer"
        );
        assert!(
            touch(&fixture, owner, "key-a").await,
            "the other tenant's claim left the owner's record unclaimed"
        );

        let mut conn = fixture.tenant_conn_for(owner).await.expect("conn opens");
        assert_eq!(
            code_review_snapshot_pointer(&mut conn, scope())
                .await
                .expect("read"),
            Some(pointer("tree-a", "key-b"))
        );
        assert!(
            code_review_context(&mut conn, REPO, "ctx")
                .await
                .expect("read")
                .is_some()
        );
    }

    /// A touch racing a collection over a record older than the grace ends
    /// with exactly one winner: either the touch refreshes it and the claim
    /// skips it, or the claim takes it and the touch is refused.
    ///
    /// # Panics
    /// Panics when a round both refreshes and claims the record, or neither.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn an_aged_record_is_either_touched_or_claimed_never_both() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();
        let superuser = fixture.superuser_pool().expect("superuser pool");
        let grace = Duration::from_hours(1);

        for round in 0..24 {
            let key = format!("race-{round}");
            assert!(touch(&fixture, tenant, &key).await);
            sqlx::query(
                "UPDATE wyrd.code_review_snapshot_records
                    SET touched_at = touched_at - interval '2 hours'
                  WHERE object_key = $1",
            )
            .bind(&key)
            .execute(&superuser)
            .await
            .expect("age the record");

            let (claimed, touched) = tokio::join!(
                claim(&fixture, tenant, grace),
                touch(&fixture, tenant, &key)
            );
            assert_ne!(
                claimed.contains(&key),
                touched,
                "round {round}: exactly one of the touch and the claim wins"
            );
            let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
            delete_code_review_snapshot_record(&mut conn, REPO, &key)
                .await
                .expect("delete");
            conn.commit().await.expect("commit");
        }
    }

    /// Rows of one repository are invisible to another repository and to
    /// the same repository ID under another installation of one tenant, and
    /// another repository's pointer never protects this repository's record.
    ///
    /// # Panics
    /// Panics when any row crosses a repository or installation boundary.
    #[tokio::test]
    async fn cache_rows_stay_inside_their_repository_and_installation() {
        let fixture = PgFixture::start().await.expect("fixture");
        let tenant = fixture.data_tenant_id();

        assert!(touch(&fixture, tenant, "key-a").await);
        assert!(publish(&fixture, tenant, None, "tree-a", "key-b").await);
        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        upsert_code_review_context(
            &mut conn,
            REPO,
            "ctx",
            &json!({}),
            &json!({"key": "k"}),
            Duration::from_secs(3600),
        )
        .await
        .expect("upsert");
        conn.commit().await.expect("commit");

        let siblings = [
            CodeReviewRepository {
                repository_id: 8,
                ..REPO
            },
            CodeReviewRepository {
                installation_id: 12,
                ..REPO
            },
        ];
        for sibling in siblings {
            let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
            let sibling_scope = CodeReviewSnapshotScope {
                repository: sibling,
                ..scope()
            };
            assert_eq!(
                code_review_snapshot_pointer(&mut conn, sibling_scope)
                    .await
                    .expect("read"),
                None,
                "{sibling:?} sees no pointer"
            );
            assert_eq!(
                code_review_context(&mut conn, sibling, "ctx")
                    .await
                    .expect("read"),
                None,
                "{sibling:?} sees no context"
            );
            assert_eq!(
                claim_unreferenced_code_review_snapshots(&mut conn, sibling, Duration::ZERO)
                    .await
                    .expect("claim"),
                Vec::<String>::new(),
                "{sibling:?} claims nothing"
            );
            assert!(
                publish_code_review_snapshot_pointer(
                    &mut conn,
                    sibling_scope,
                    None,
                    &pointer("tree-s", "key-a"),
                    "tree-s",
                )
                .await
                .expect("publish"),
                "{sibling:?} publishes its own first pointer"
            );
            conn.commit().await.expect("commit");
        }

        let mut conn = fixture.tenant_conn_for(tenant).await.expect("conn opens");
        assert_eq!(
            code_review_snapshot_pointer(&mut conn, scope())
                .await
                .expect("read"),
            Some(pointer("tree-a", "key-b"))
        );
        conn.commit().await.expect("commit");
        assert_eq!(
            claim(&fixture, tenant, Duration::ZERO).await,
            vec!["key-a".to_owned()],
            "a sibling's pointer naming the same key does not protect this record"
        );
    }
}
