//! End-to-end auth test against the real `WyrdTestServer` router.
//!
//! It assembles a real [`WyrdClient`] from config — credential resolution →
//! `AuthMiddleware` API-key exchange → `HttpTransport` — and drives requests
//! *through the client* against the live server, proving the data-plane header
//! contract: the server authenticates from `x-wyrd-access-token` and does
//! **not** read the application-owned `Authorization` header.
//!
//! Two signals, both decisive:
//!  1. **Token-layer e2e** — `client.auth().bearer()` exchanges the resolved
//!     API key at the real `/auth/token` and mints a JWT. A success proves the
//!     client's API-key → JWT path works against the live server.
//!  2. **Data-plane header contract** — `client.request_json` (which sends
//!     `x-wyrd-access-token`) to `GET /v1/cards` is *authenticated* and
//!     rejected only at the permission check (`403 PERMISSION_403_DENIED_RBAC`,
//!     the service holds no role), never `401`. The negative control re-sends
//!     the *same* JWT in `Authorization` only and gets `401`, proving the
//!     server ignores it.
//!
//! A role-less principal on the card list is used deliberately: the route
//! needs no pre-created card, and its permission check runs right after
//! authentication — so a `403` there is an unambiguous "auth passed" signal,
//! while the `Authorization`-only control yields a clean `401`.
//!
//! Ungated like `discovery_against_fixture.rs`: it runs in the Postgres test
//! lane the fixture requires.

// Wrapped in `mod pg_tests` so the fast family lane skips it via
// `--skip pg_tests` (it needs the real `WyrdTestServer` + Postgres); the
// infra e2e lane selects it by `--test` and runs it.
mod pg_tests {
    use wyrd_client::WyrdClient;
    use wyrd_client::config::ClientConfig;

    #[tokio::test(flavor = "multi_thread")]
    async fn wyrd_client_authenticates_via_wyrd_access_token_header() {
        let srv = wyrd_testing::WyrdTestServer::start_bound()
            .await
            .expect("test server starts");
        let base_url = srv
            .base_url()
            .expect("bound server has a base url")
            .to_owned();

        // A real, hashed-into-the-DB service API key.
        let bootstrap = srv
            .bootstrap_service("wyrd-client-e2e", &[])
            .await
            .expect("service bootstraps");
        let api_key = bootstrap
            .api_key()
            .expect("machine bootstrap yields an api key")
            .clone();

        // Assemble the full client the way a caller would: config in, client out.
        // Setting `api_key` makes resolve_credential pick it as the tier-0 source,
        // so this also exercises credential resolution, not just direct wiring.
        let mut config = ClientConfig::default();
        config.http.base_url = base_url.clone();
        config.credential = Some(api_key);
        let client = WyrdClient::with_config(config).expect("client assembles");

        // (1) Token-layer e2e: the real /auth/token accepts the resolved API key and
        // mints a JWT. Reused below as the negative-control credential.
        let jwt = client
            .auth()
            .bearer()
            .await
            .expect("api key exchanges for an access token against the real server")
            .expose()
            .to_owned();

        // (2) Data-plane: the request carries x-wyrd-access-token, so the server
        // authenticates the principal and rejects only at the permission check.
        let err = client
            .request_json::<serde_json::Value, serde_json::Value>(
                reqwest::Method::GET,
                "/v1/cards",
                None,
            )
            .await
            .expect_err("a role-less token is authenticated, then permission-rejected");
        assert_eq!(
            err.code(),
            "WYRD_PERMISSION_403_DENIED_RBAC",
            "auth must succeed from x-wyrd-access-token (403 at the permission check), not fail at 401; got {err:?}"
        );

        // Negative control: the SAME JWT in Authorization only must be unauthenticated,
        // proving the server never reads Authorization for the data plane.
        let raw = reqwest::Client::new()
            .get(format!("{base_url}/v1/cards"))
            .header("Authorization", format!("Bearer {jwt}"))
            .send()
            .await
            .expect("control request sends");
        assert_eq!(
            raw.status().as_u16(),
            401,
            "a valid JWT presented only in Authorization must be rejected as unauthenticated"
        );

        let _ = srv.shutdown().await;
    }
}
