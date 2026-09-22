mod pg_tests {
    //! Gated execution tests for the metadata query SQL compiler.
    //!
    //! Validates that compiled SQL + binds execute correctly against Postgres and
    //! that the chosen NULL semantics hold for typed-column negation operators.
    //! Requires a live database; the Postgres-backed `test:sql` lane provides it.

    use sqlx::{PgConnection, Postgres, QueryBuilder, Row};
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::error::WyrdError;
    use wyrd_spec::query::{FieldRef, MetadataQuery, QueryFieldErrorDetail, ValueType};
    use wyrd_sql::query::{FieldColumn, FieldResolver, compile_query};

    macro_rules! e2e_test {
        ($name:ident, $body:block) => {
            #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
            async fn $name() {
                $body
            }
        };
    }

    /// Minimal resolver for the `test_items` fixture table.
    struct ItemResolver;

    impl FieldResolver for ItemResolver {
        fn surface(&self) -> &'static str {
            "test_items"
        }

        fn valid_fields(&self) -> &'static [&'static str] {
            &["id", "status", "score"]
        }

        fn resolve(&self, field: &FieldRef) -> Result<FieldColumn, WyrdError> {
            match field {
                FieldRef::Reserved(col) if col == "id" => Ok(FieldColumn::Typed {
                    column: "id",
                    ty: ValueType::Int,
                }),
                FieldRef::Reserved(col) if col == "status" => Ok(FieldColumn::Typed {
                    column: "status",
                    ty: ValueType::Str,
                }),
                FieldRef::Reserved(col) if col == "score" => Ok(FieldColumn::Typed {
                    column: "score",
                    ty: ValueType::Int,
                }),
                FieldRef::Reserved(col) => Err(WyrdError::query_invalid_field_detail(
                    format!("unknown field {col}"),
                    QueryFieldErrorDetail {
                        reason: "unknown_field",
                        field: Some(col),
                        surface: Some("test_items"),
                        valid_fields: self.valid_fields(),
                        ..Default::default()
                    },
                )),
                FieldRef::Attribute(_) => Err(WyrdError::query_invalid_field_detail(
                    "attributes are not queryable on this surface",
                    QueryFieldErrorDetail {
                        reason: "unknown_field",
                        surface: Some("test_items"),
                        valid_fields: self.valid_fields(),
                        ..Default::default()
                    },
                )),
                FieldRef::Label(_) | FieldRef::Annotation(_) => {
                    unreachable!("labels/annotations handled by compiler")
                }
            }
        }
    }

    async fn setup(conn: &mut PgConnection) {
        // TEMP table on a single pinned connection: the restricted RLS app role
        // cannot create tables in `public`, but it can create session-local temp
        // tables in `pg_temp`. Because temp tables are session-scoped, setup and
        // every query in a test must run on the *same* connection — hence each test
        // acquires one connection from the pool and threads it through.
        sqlx::query(
            "CREATE TEMP TABLE IF NOT EXISTS test_items (
            id      BIGINT PRIMARY KEY,
            labels  JSONB  NOT NULL DEFAULT '{}',
            status  TEXT,
            score   BIGINT
        )",
        )
        .execute(&mut *conn)
        .await
        .expect("create temp table");

        sqlx::query("TRUNCATE test_items")
            .execute(&mut *conn)
            .await
            .expect("truncate");

        // Row 1: has env=prod, status=active, score=10
        // Row 2: has env=dev,  status=inactive, score=5
        // Row 3: no labels.env key, status=NULL, score=NULL
        // Row 4: env=prod, status=active, score=NULL
        sqlx::query(
            "INSERT INTO test_items (id, labels, status, score) VALUES
         (1, '{\"env\":\"prod\"}', 'active',   10),
         (2, '{\"env\":\"dev\"}',  'inactive',  5),
         (3, '{}',              NULL,        NULL),
         (4, '{\"env\":\"prod\"}', 'active',   NULL)",
        )
        .execute(&mut *conn)
        .await
        .expect("insert fixture rows");
    }

    async fn query_ids(conn: &mut PgConnection, input: &str) -> Vec<i64> {
        let query = MetadataQuery::parse(input).expect("query parses");
        let mut qb = QueryBuilder::<Postgres>::new("SELECT id FROM test_items WHERE ");
        compile_query(&query, &ItemResolver, &mut qb).expect("query compiles");
        let mut ids: Vec<i64> = qb
            .build()
            .fetch_all(&mut *conn)
            .await
            .expect("query executes")
            .into_iter()
            .map(|row| row.get::<i64, _>("id"))
            .collect();
        ids.sort_unstable();
        ids
    }

    // JSONB Ne: absent-key rows are included (existing behavior, verify it holds).
    e2e_test!(jsonb_key_absent_ne_includes_absent_rows, {
        let fixture = PgFixture::start().await.expect("fixture");
        let mut conn = fixture
            .app_pool()
            .acquire()
            .await
            .expect("acquire connection");
        setup(&mut conn).await;

        let ids = query_ids(&mut conn, "labels.env != \"prod\"").await;
        assert_eq!(
            ids,
            vec![2, 3],
            "absent-key row must be included by JSONB Ne"
        );
    });

    // Typed Ne: NULL rows must be included (matches JSONB behavior after fix).
    e2e_test!(typed_ne_is_null_inclusive, {
        let fixture = PgFixture::start().await.expect("fixture");
        let mut conn = fixture
            .app_pool()
            .acquire()
            .await
            .expect("acquire connection");
        setup(&mut conn).await;

        let ids = query_ids(&mut conn, "status != \"active\"").await;
        assert_eq!(ids, vec![2, 3], "NULL typed column must be included by Ne");
    });

    // Typed NotIn: NULL rows must be included (matches JSONB behavior after fix).
    e2e_test!(typed_not_in_is_null_inclusive, {
        let fixture = PgFixture::start().await.expect("fixture");
        let mut conn = fixture
            .app_pool()
            .acquire()
            .await
            .expect("acquire connection");
        setup(&mut conn).await;

        let ids = query_ids(&mut conn, "score not in [10]").await;
        assert_eq!(
            ids,
            vec![2, 3, 4],
            "NULL typed column must be included by NotIn"
        );
    });

    // Typed In array-bind: only matching non-NULL rows.
    e2e_test!(typed_in_array_bind, {
        let fixture = PgFixture::start().await.expect("fixture");
        let mut conn = fixture
            .app_pool()
            .acquire()
            .await
            .expect("acquire connection");
        setup(&mut conn).await;

        let ids = query_ids(&mut conn, "score in [5, 10]").await;
        assert_eq!(ids, vec![1, 2]);
    });

    // JSONB regex match.
    e2e_test!(jsonb_regex_match, {
        let fixture = PgFixture::start().await.expect("fixture");
        let mut conn = fixture
            .app_pool()
            .acquire()
            .await
            .expect("acquire connection");
        setup(&mut conn).await;

        let ids = query_ids(&mut conn, "labels.env =~ \"pro.*\"").await;
        assert_eq!(ids, vec![1, 4]);
    });
}
