mod pg_tests {
    //! Durable proof that verified-contact replacement keeps one revision
    //! history: a write takes effect only at the revision its proof was
    //! begun against, including two first contacts racing with no row to
    //! lock.

    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_sql::queries::auth::{
        insert_local_user, replace_verified_contact, verified_contact_for_update,
    };

    /// Two verifications begun against no contact race: exactly one inserts
    /// revision one, the other changes nothing, and a later write at a stale
    /// revision is refused while the current one advances.
    ///
    /// # Panics
    /// Panics when both racing writes succeed, neither does, or a stale write
    /// takes effect.
    #[tokio::test]
    async fn contact_replacement_takes_effect_only_at_its_expected_revision() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let user = Uuid::now_v7();
        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        insert_local_user(
            &mut conn,
            user,
            "ada",
            "$argon2id$v=19$m=19456,t=2,p=1$x$y",
            None,
        )
        .await
        .expect("user inserts");
        conn.commit().await.expect("user commits");

        let mut first = fixture.tenant_conn().await.expect("conn opens");
        let mut second = fixture.tenant_conn().await.expect("conn opens");
        assert!(
            verified_contact_for_update(&mut first, user)
                .await
                .expect("read runs")
                .is_none()
        );
        assert!(
            verified_contact_for_update(&mut second, user)
                .await
                .expect("read runs")
                .is_none()
        );
        let won = replace_verified_contact(&mut first, user, "a@example.test", 0)
            .await
            .expect("first write runs");
        let (lost, ()) = tokio::join!(
            async {
                let lost = replace_verified_contact(&mut second, user, "b@example.test", 0)
                    .await
                    .expect("second write runs");
                second.commit().await.expect("second commits");
                lost
            },
            async { first.commit().await.expect("first commits") },
        );
        assert_eq!((won, lost), (Some(1), None));

        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        assert_eq!(
            replace_verified_contact(&mut conn, user, "c@example.test", 0)
                .await
                .expect("stale write runs"),
            None
        );
        assert_eq!(
            replace_verified_contact(&mut conn, user, "c@example.test", 1)
                .await
                .expect("current write runs"),
            Some(2)
        );
        let stored = verified_contact_for_update(&mut conn, user)
            .await
            .expect("read runs")
            .expect("a contact");
        assert_eq!(
            (stored.address.as_str(), stored.revision),
            ("c@example.test", 2)
        );
        conn.commit().await.expect("commits");
    }
}
