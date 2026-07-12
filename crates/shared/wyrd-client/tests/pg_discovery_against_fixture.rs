//! Harness-seam integration test: proves `ClientConfig::from_env()` reads the
//! exact env var names that `WyrdTestServer` exports via `mutate_env=true`.
//!
//! The literal env names here are the shared string contract.  A rename on
//! either side (fixture or `from_env`) will break this test.

// Wrapped in `mod pg_tests` so the fast family lane skips it via
// `--skip pg_tests` (it needs the real `WyrdTestServer` + Postgres); the
// infra e2e lane selects it by `--test` and runs it.
mod pg_tests {
    use std::sync::{Mutex, OnceLock};

    use secrecy::ExposeSecret;
    use wyrd_client::config::ClientConfig;

    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_mutex() -> &'static Mutex<()> {
        ENV_MUTEX.get_or_init(|| Mutex::new(()))
    }

    unsafe fn restore_env(name: &str, prior: Option<String>) {
        // SAFETY: callers hold ENV_MUTEX. Rust 2024 requires an explicit inner
        // `unsafe` block even inside an `unsafe fn` (unsafe_op_in_unsafe_fn).
        unsafe {
            match prior {
                Some(v) => std::env::set_var(name, v),
                None => std::env::remove_var(name),
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn discovery_against_fixture() {
        let srv = wyrd_testing::WyrdTestServer::start_bound()
            .await
            .expect("test server starts");

        let base_url = srv.base_url().unwrap_or("").to_owned();
        let grpc_url = srv.grpc_url().unwrap_or_default();
        let api_key = srv.api_key().expose_secret().to_owned();

        let _guard = env_mutex().lock().unwrap_or_else(|p| p.into_inner());

        let prior_server_url = std::env::var("WYRD_SERVER_URL").ok();
        let prior_grpc_url = std::env::var("WYRD_GRPC_URL").ok();
        let prior_api_key = std::env::var("WYRD_API_KEY").ok();

        // SAFETY: ENV_MUTEX serializes all env mutations in this test binary.
        unsafe {
            std::env::set_var("WYRD_SERVER_URL", &base_url);
            std::env::set_var("WYRD_GRPC_URL", &grpc_url);
            std::env::set_var("WYRD_API_KEY", &api_key);
        }

        let cfg = ClientConfig::from_env();

        // Verify the shared string contract: from_env() reads the same names the
        // fixture exports.
        assert_eq!(
            cfg.http.base_url, base_url,
            "WYRD_SERVER_URL must feed ClientConfig.http.base_url"
        );
        assert_eq!(
            cfg.grpc.endpoint, grpc_url,
            "WYRD_GRPC_URL must feed ClientConfig.grpc.endpoint"
        );
        assert_eq!(
            std::env::var("WYRD_API_KEY").ok().as_deref(),
            Some(api_key.as_str()),
            "WYRD_API_KEY must carry the fixture api_key value"
        );

        // SAFETY: ENV_MUTEX serializes all env mutations in this test binary.
        unsafe {
            restore_env("WYRD_SERVER_URL", prior_server_url);
            restore_env("WYRD_GRPC_URL", prior_grpc_url);
            restore_env("WYRD_API_KEY", prior_api_key);
        }

        // Release the env lock before the await: a std MutexGuard must not be held
        // across an await point, and shutdown does not touch the environment.
        drop(_guard);

        let _ = srv.shutdown().await;
    }
}
