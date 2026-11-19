//! The scheduled-check guide's Rust programs, run as a reader runs them.
//!
//! The journey declares and pulls the example's `checkout` Service, creates
//! its runtime account, and runs the built `record` and `results` binaries
//! from the reader's directory. The real every-minute scheduler, not the
//! test, judges each window: fast requests pass, slow ones fail, and a
//! window with nothing recorded is inconclusive.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_ai::WyrdClient;
use wyrd_ai::auth::{
    CreateServicePrincipalRequest, Principals, WorkloadFamily, WorkloadKind, WorkloadRestriction,
};
use wyrd_ai::bifrost::{Bifrost, BifrostOptions, QueryParam};
use wyrd_ai::cards::{BundleMode, CardKind, CardSelector, Cards};
use wyrd_ai::config::ClientConfig;
use wyrd_ai::transport::{GrpcConfig, HttpConfig};
use wyrd_testing::server::WyrdTestServer;

/// What `results` prints, as the guide shows it, with its times.
const OUTPUT: &str = include_str!("../scheduled-check/results-output.txt");

/// How long one window may take to be judged and stored.
const WAIT: Duration = Duration::from_secs(90);

/// One stored scheduled result, its window in epoch seconds.
#[derive(Deserialize)]
struct Judged {
    /// Inclusive window start.
    opened: i64,
    /// Exclusive window end.
    closed: i64,
    /// The window's verdict.
    verdict: String,
}

/// The reader's directory and the server it reaches.
struct Reader {
    /// The directory the reader works in.
    dir: tempfile::TempDir,
    /// The reader's `WYRD_CONFIG_HOME`.
    config: tempfile::TempDir,
    /// The server's HTTP URL.
    server_url: String,
    /// The server's gRPC URL.
    grpc_url: String,
}

impl Reader {
    /// Run the built `program` with `args` as `key`, as `cargo run --bin`
    /// does from the reader's directory, and return what it printed.
    ///
    /// # Panics
    /// Panics when the program cannot start or fails.
    async fn run(&self, program: &'static str, key: &str, args: &[&str]) -> String {
        let (dir, config): (PathBuf, PathBuf) =
            (self.dir.path().to_owned(), self.config.path().to_owned());
        let (server_url, grpc_url, key) = (
            self.server_url.clone(),
            self.grpc_url.clone(),
            key.to_owned(),
        );
        let args: Vec<String> = args.iter().map(|&arg| arg.to_owned()).collect();
        // The in-process server keeps serving while the binary runs.
        let output = tokio::task::spawn_blocking(move || {
            Command::new(program)
                .args(args)
                .current_dir(dir)
                .env("WYRD_CONFIG_HOME", config)
                .env("WYRD_SERVER_URL", server_url)
                .env("WYRD_GRPC_URL", grpc_url)
                .env("WYRD_API_KEY", key)
                .env_remove("WYRD_ACCESS_TOKEN")
                .env_remove("WYRD_WORKLOAD_TOKEN")
                .env_remove("WYRD_TENANT")
                .output()
                .expect("the binary starts")
        })
        .await
        .expect("the program task joins");
        assert!(
            output.status.success(),
            "{program} fails: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("the program prints UTF-8")
    }
}

/// Seconds since the Unix epoch.
///
/// # Panics
/// Panics when the system clock reads before the Unix epoch.
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_secs_f64()
}

/// Wait until at least ten seconds remain in the current minute, so the
/// samples about to be recorded fall in one window, and return the time.
///
/// # Panics
/// Panics when the system clock reads before the Unix epoch.
async fn clear_of_boundary() -> f64 {
    let second = now() % 60.0;
    if second >= 50.0 {
        tokio::time::sleep(Duration::from_secs_f64(61.0 - second)).await;
    }
    now()
}

/// The verdict of the scheduled window holding `at`, once the real
/// scheduler stores it.
///
/// # Panics
/// Panics when no such window is judged within [`WAIT`].
async fn window_verdict(server: &WyrdTestServer, bifrost: &Bifrost, uid: &str, at: f64) -> String {
    let query = "SELECT to_unixtime(window_start) AS opened, to_unixtime(window_end) AS closed, \
        verdict FROM bifrost.verification.results WHERE verifier_card_uid = $1";
    let deadline = Instant::now() + WAIT;
    loop {
        server.flush_bifrost().await.expect("Bifrost flushes");
        let rows: Vec<Judged> = bifrost
            .sql_as(query, &[QueryParam::String(uid.to_owned())])
            .await
            .expect("results query");
        if let Some(row) = rows
            .into_iter()
            .find(|row| row.opened as f64 <= at && at < row.closed as f64)
        {
            return row.verdict;
        }
        assert!(
            Instant::now() < deadline,
            "no window holding {at} was judged"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// Replace every `HH:MM-HH:MM` window in `text`, whose times vary per run.
fn without_times(text: &str) -> String {
    text.lines()
        .map(|line| match line.trim_start().split_once(' ') {
            Some((window, verdict)) if window.len() == 11 && window.as_bytes()[5] == b'-' => {
                format!("  HH:MM-HH:MM {verdict}")
            }
            _ => line.to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The every-minute schedule judges the fast window `passed`, the slow one
/// `failed`, and the empty one `inconclusive`, and `results` prints them,
/// newest first, as the guide shows.
///
/// # Panics
/// Panics when the server cannot start, a program fails, a window is not
/// judged in time, or it is judged otherwise.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn scheduled_check_judges_each_window() {
    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .start_bound()
        .await
        .expect("test server starts");
    let reader = Reader {
        dir: tempfile::tempdir().expect("reader directory creates"),
        config: tempfile::tempdir().expect("config directory creates"),
        server_url: server
            .base_url()
            .expect("bound server has a URL")
            .to_owned(),
        grpc_url: server.grpc_url().expect("bound server has a gRPC URL"),
    };
    let key = server
        .tenant_admin_key()
        .await
        .expect("administrator key issues")
        .expose_secret()
        .to_owned();
    let admin = WyrdClient::with_config(ClientConfig {
        http: HttpConfig {
            base_url: reader.server_url.clone(),
            ..HttpConfig::default()
        },
        grpc: GrpcConfig {
            endpoint: reader.grpc_url.clone(),
            ..GrpcConfig::default()
        },
        credential: Some(SecretString::from(key.clone())),
        ..ClientConfig::default()
    })
    .expect("client builds");

    // What `wyrd declare`, `wyrd pull`, and `wyrd principal credential
    // create-principal --workload-name checkout` do in the guide.
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("scheduled-check/checkout");
    let cards = Cards::connect_with(&admin).await.expect("cards connect");
    cards
        .declare_from_path(example.join("checkout.yaml"))
        .await
        .expect("the example declares");
    cards
        .pull(
            CardSelector::named(CardKind::Service, "checkout".parse().expect("name parses")),
            reader.dir.path().join(".wyrd-bundle"),
            BundleMode::Complete,
        )
        .await
        .expect("the Service pulls");
    let runtime_key = Principals::connect_with(&admin)
        .await
        .expect("principals connect")
        .create_service_principal(&CreateServicePrincipalRequest {
            name: "checkout-runtime".to_owned(),
            roles: vec!["workload".to_owned()],
            description: None,
            workload_restriction: Some(WorkloadRestriction::Family(WorkloadFamily {
                kind: WorkloadKind::Service,
                name: "checkout".parse().expect("name parses"),
            })),
        })
        .await
        .expect("the runtime account is created")
        .credential
        .expose()
        .to_owned();
    let uid = cards
        .get(CardSelector::named(
            CardKind::Verifier,
            "latency-budget".parse().expect("name parses"),
        ))
        .await
        .expect("the Verifier reads")
        .metadata
        .uid
        .expect("a declared Card has a uid")
        .to_string();
    let bifrost = Bifrost::connect_with(&admin, BifrostOptions::default())
        .await
        .expect("Bifrost connects");

    let record = env!("CARGO_BIN_EXE_record");
    let fast = clear_of_boundary().await;
    assert_eq!(
        reader.run(record, &runtime_key, &["80"]).await,
        "recorded 5 requests at 80 ms\n"
    );
    assert_eq!(
        window_verdict(&server, &bifrost, &uid, fast).await,
        "passed"
    );
    let slow = clear_of_boundary().await;
    assert_eq!(
        reader.run(record, &runtime_key, &["250"]).await,
        "recorded 5 requests at 250 ms\n"
    );
    assert_eq!(
        window_verdict(&server, &bifrost, &uid, slow).await,
        "failed"
    );
    let quiet = now();
    assert_eq!(
        window_verdict(&server, &bifrost, &uid, quiet).await,
        "inconclusive"
    );

    let printed = reader.run(env!("CARGO_BIN_EXE_results"), &key, &[]).await;
    assert_eq!(without_times(&printed), without_times(OUTPUT));
    server.shutdown().await.expect("test server shuts down");
}
