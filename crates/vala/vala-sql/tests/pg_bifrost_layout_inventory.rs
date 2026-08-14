mod pg_tests {
    //! Migration-20 shape and ownership proofs for the operator layout inventory.

    use sqlx::types::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;

    /// Operator-only inventory rows accept hot-only identities and closed states.
    #[tokio::test]
    async fn bifrost_layout_inventory_is_operator_owned_and_constrained() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator = fixture.operator_pool().pool();
        let tenant = Uuid::from(fixture.data_tenant_id());
        let table_uid = Uuid::now_v7();
        let digest = vec![7_u8; 32];

        sqlx::query(
            "INSERT INTO vala.bifrost_layout_inventory \
             (data_tenant_id,table_uid,current_snapshot_id,current_schema_id,metadata_location,unresolved_hot_manifest_digest,table_identity_digest,unmarked_file_count,writer_v1_file_count,writer_v2_file_count,complete,observed_at,attestation_state) \
             VALUES ($1,$2,NULL,4,'s3://bucket/table/metadata.json',$3,$4,0,0,2,true,statement_timestamp(),'verified')",
        )
        .bind(tenant)
        .bind(table_uid)
        .bind(&digest)
        .bind(&digest)
        .execute(operator)
        .await
        .expect("operator inserts a hot-only attestation");

        let row: (Option<i64>, i64, i64, i64, bool, String) = sqlx::query_as(
            "SELECT current_snapshot_id,unmarked_file_count,writer_v1_file_count,writer_v2_file_count,complete,attestation_state \
             FROM vala.bifrost_layout_inventory WHERE data_tenant_id=$1 AND table_uid=$2",
        )
        .bind(tenant)
        .bind(table_uid)
        .fetch_one(operator)
        .await
        .expect("operator reads attestation");
        assert_eq!(row, (None, 0, 0, 2, true, "verified".to_owned()));

        let invalid_state = sqlx::query(
            "UPDATE vala.bifrost_layout_inventory SET attestation_state='unknown' WHERE data_tenant_id=$1 AND table_uid=$2",
        )
        .bind(tenant)
        .bind(table_uid)
        .execute(operator)
        .await;
        assert!(
            invalid_state.is_err(),
            "attestation state is a closed domain"
        );

        let invalid_count = sqlx::query(
            "UPDATE vala.bifrost_layout_inventory SET writer_v1_file_count=-1 WHERE data_tenant_id=$1 AND table_uid=$2",
        )
        .bind(tenant)
        .bind(table_uid)
        .execute(operator)
        .await;
        assert!(
            invalid_count.is_err(),
            "recipe counts cannot become negative"
        );

        let tenant_mutation = sqlx::query(
            "UPDATE vala.bifrost_layout_inventory SET complete=false WHERE data_tenant_id=$1 AND table_uid=$2",
        )
        .bind(tenant)
        .bind(table_uid)
        .execute(fixture.app_pool())
        .await;
        assert!(
            tenant_mutation.is_err(),
            "ordinary app connections cannot mutate inventory"
        );
    }

    /// The singleton global cut retains exact digest, roster, and completeness constraints.
    #[tokio::test]
    async fn bifrost_layout_inventory_global_cut_is_singleton_and_complete() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let operator = fixture.operator_pool().pool();
        let digest = vec![9_u8; 32];
        sqlx::query(
            "INSERT INTO vala.bifrost_layout_inventory_state \
             (inventory_name,global_cut_digest,roster_count,complete,observed_at) \
             VALUES ('oracle_activation',$1,0,true,statement_timestamp())",
        )
        .bind(&digest)
        .execute(operator)
        .await
        .expect("operator inserts completed empty-roster cut");

        let row: (String, Vec<u8>, i64, bool) = sqlx::query_as(
            "SELECT inventory_name,global_cut_digest,roster_count,complete FROM vala.bifrost_layout_inventory_state",
        )
        .fetch_one(operator)
        .await
        .expect("global cut reads");
        assert_eq!(row, ("oracle_activation".to_owned(), digest, 0, true));

        let second = sqlx::query(
            "INSERT INTO vala.bifrost_layout_inventory_state \
             (inventory_name,global_cut_digest,roster_count,complete,observed_at) \
             VALUES ('other',$1,0,false,statement_timestamp())",
        )
        .bind(vec![1_u8; 32])
        .execute(operator)
        .await;
        assert!(
            second.is_err(),
            "the global cut name is a checked singleton"
        );
    }
}
