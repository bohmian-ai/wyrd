//! The docs Quickstart's Rust program, run as a reader runs it.
//!
//! The journey copies the Quickstart's two Card files into an empty
//! directory and runs the built `quickstart` binary there, twice, against a
//! real server reached through the environment a saved local setup provides.

use std::path::{Path, PathBuf};
use std::process::Command;

use secrecy::ExposeSecret;
use wyrd_testing::server::WyrdTestServer;

/// The Quickstart's directory, holding its Card files and programs.
const QUICKSTART: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/quickstart");

/// Where and as whom a reader's program runs: its working directory and the
/// server address and key a saved local setup would provide.
struct Reader {
    /// The reader's directory, holding only the Card files.
    dir: PathBuf,
    /// An empty Wyrd config directory, so no saved developer setup applies.
    config: PathBuf,
    /// The server's HTTP address.
    server_url: String,
    /// The server's gRPC address.
    grpc_url: String,
    /// The tenant administrator's key.
    key: String,
}

impl Reader {
    /// Run the `quickstart` binary in the reader's directory and return what
    /// it printed.
    ///
    /// # Panics
    /// Panics when the binary cannot start or exits unsuccessfully.
    fn run_quickstart(&self) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_quickstart"))
            .current_dir(&self.dir)
            .env("WYRD_CONFIG_HOME", &self.config)
            .env("WYRD_SERVER_URL", &self.server_url)
            .env("WYRD_GRPC_URL", &self.grpc_url)
            .env("WYRD_API_KEY", &self.key)
            .env_remove("WYRD_ACCESS_TOKEN")
            .env_remove("WYRD_WORKLOAD_TOKEN")
            .env_remove("WYRD_TENANT")
            .output()
            .expect("the quickstart binary starts");
        assert!(
            output.status.success(),
            "the quickstart fails: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("the quickstart prints UTF-8")
    }
}

/// The Rust Quickstart prints a passing and a failing verdict with the exact
/// subject and Verifier identities, and an unchanged rerun prints the same
/// identities and leaves only the Card files in the reader's directory.
///
/// # Panics
/// Panics when the server cannot start or the printed verdicts differ.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the repository-managed Postgres journey lifecycle"]
async fn quickstart_verifies_and_repeats() {
    let server = WyrdTestServer::builder()
        .with_verification_runtime_for_test()
        .start_bound()
        .await
        .expect("test server starts");
    let dir = tempfile::tempdir().expect("reader directory creates");
    let config = tempfile::tempdir().expect("config directory creates");
    for file in ["no-refund-promise.yaml", "support-reply.yaml"] {
        std::fs::copy(Path::new(QUICKSTART).join(file), dir.path().join(file))
            .expect("Card file copies");
    }
    let reader = Reader {
        dir: dir.path().to_owned(),
        config: config.path().to_owned(),
        server_url: server
            .base_url()
            .expect("bound server has a URL")
            .to_owned(),
        grpc_url: server.grpc_url().expect("bound server has a gRPC URL"),
        key: server
            .tenant_admin_key()
            .await
            .expect("administrator key issues")
            .expose_secret()
            .to_owned(),
    };

    // The in-process server keeps serving on the runtime while the binary runs.
    let (first, again) = tokio::task::spawn_blocking(move || {
        let printed = (reader.run_quickstart(), reader.run_quickstart());
        (printed, reader)
    })
    .await
    .map(|(printed, reader)| {
        let mut files: Vec<_> = std::fs::read_dir(&reader.dir)
            .expect("reader directory reads")
            .map(|entry| entry.expect("entry reads").file_name())
            .collect();
        files.sort();
        assert_eq!(files, ["no-refund-promise.yaml", "support-reply.yaml"]);
        printed
    })
    .expect("the quickstart task joins");

    let lines: Vec<&str> = first.lines().collect();
    let [
        passed,
        subject,
        verifier,
        failed,
        failed_subject,
        failed_verifier,
    ] = lines[..]
    else {
        panic!("the quickstart prints two verdicts: {first}");
    };
    assert_eq!(passed, "passed: the comparison held");
    assert_eq!(failed, "failed: operator NotContains returned false");
    assert!(
        subject.starts_with("  subject:  default/Service/support-reply@1.0.0#"),
        "{first}"
    );
    assert!(
        verifier.starts_with("  verifier: default/Verifier/no-refund-promise@1.0.0#"),
        "{first}"
    );
    assert_eq!((failed_subject, failed_verifier), (subject, verifier));
    assert_eq!(again, first, "the rerun declares nothing new");
    server.shutdown().await.expect("test server shuts down");
}
