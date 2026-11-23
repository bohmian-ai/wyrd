mod pg_tests {
    //! Durable proof that verified-contact replacement keeps one revision
    //! history: a write takes effect only at the revision its proof was
    //! begun against, including two first contacts racing with no row to
    //! lock.

    use uuid::Uuid;
    use wyrd_dev_fixtures::pg::PgFixture;
    use wyrd_spec::auth::Sha256Hex;
    use wyrd_sql::queries::auth::{
        consume_password_reset, insert_local_user, insert_password_reset, replace_verified_contact,
        verified_contact_for_update,
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

    /// A mailed reset replaces the User's earlier mailed resets but never an
    /// operator-issued one, keeps the contact revision it was sent to, and
    /// each code is consumed at most once, even by two racing redemptions.
    ///
    /// # Panics
    /// Panics when a replaced code survives, an operator code is replaced,
    /// the revision is lost, or a code is consumed twice.
    #[tokio::test]
    async fn mailed_resets_replace_mailed_codes_and_are_consumed_once() {
        let fixture = PgFixture::start().await.expect("fixture starts");
        let user = Uuid::now_v7();
        let code = |n: u8| Sha256Hex::digest(&[n]);
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
        for (n, contact_revision) in [(1, None), (2, Some(1)), (3, Some(1))] {
            insert_password_reset(
                &mut conn,
                Uuid::now_v7(),
                &code(n),
                user,
                0,
                user,
                contact_revision,
            )
            .await
            .expect("reset inserts");
        }
        conn.commit().await.expect("resets commit");

        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        assert_eq!(
            consume_password_reset(&mut conn, &code(2))
                .await
                .expect("consume runs"),
            None,
            "the newer mailed code replaced it"
        );
        conn.commit().await.expect("commits");

        let mut first = fixture.tenant_conn().await.expect("conn opens");
        let mut second = fixture.tenant_conn().await.expect("conn opens");
        let won = consume_password_reset(&mut first, &code(3))
            .await
            .expect("first consume runs");
        let (lost, ()) = tokio::join!(
            async {
                let lost = consume_password_reset(&mut second, &code(3))
                    .await
                    .expect("second consume runs");
                second.commit().await.expect("second commits");
                lost
            },
            async { first.commit().await.expect("first commits") },
        );
        assert_eq!(won.map(|row| row.contact_revision), Some(Some(1)));
        assert_eq!(lost, None, "a code is consumed once");

        let mut conn = fixture.tenant_conn().await.expect("conn opens");
        let operator = consume_password_reset(&mut conn, &code(1))
            .await
            .expect("consume runs")
            .expect("the operator code survives mailed codes");
        assert_eq!(operator.contact_revision, None);
        conn.commit().await.expect("commits");
    }
}
