//! Local startup through the installed CLI and real serving binary, isolated
//! from the user's credentials, database, working directory, and MCP hosts.

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use wyrd_client::bifrost::{Correlation, TableConfig};
use wyrd_client::cards::{CardSelector, Cards};
use wyrd_client::config::ClientConfig;
use wyrd_client::environment::Environment;
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_dev_fixtures::pg::UnmigratedDatabase;
use wyrd_spec::envelope::CardKind;

use crate::mcp_journey::{list_tables_through_proxy, wyrd};

/// A fresh external database, installed artifacts, and one owned CLI process.
struct LocalDev {
    /// Foreground CLI until it has been interrupted and reaped.
    child: Option<Child>,
    /// Captured stdout drained continuously, so logs cannot block the child.
    stdout: Option<JoinHandle<String>>,
    /// Redacted stderr drained continuously, independently of stdout.
    stderr: Option<JoinHandle<String>>,
    /// Temporary deployment and user files; survives safe reruns.
    home: tempfile::TempDir,
    /// Owns the initially empty database until every child has stopped.
    _database: UnmigratedDatabase,
    /// Ordinary database-owner URL passed only through child environments.
    dsn: SecretString,
    /// HTTP socket reserved independently for each fixture.
    bind: String,
    /// Isolated gRPC listener used by the normal server.
    grpc: String,
    /// Isolated Bifrost peer listener used by the normal server.
    peer: String,
}

impl LocalDev {
    /// Create a deployment with no migrations, root, tenant, or saved key.
    /// The installer is already present, as on a later installed-CLI run;
    /// signed-download behavior has its own release-service journeys.
    ///
    /// # Panics
    /// Panics when the managed database, compiled binary, or fixture files
    /// cannot be prepared. Build wyrd-server through mise before this target.
    async fn new() -> Self {
        let database = UnmigratedDatabase::create()
            .await
            .expect("empty managed database");
        let options = database.migrator_pool().connect_options();
        let name = options.get_database().expect("fixture database name");
        let mut dsn = url::Url::parse(
            &std::env::var("WYRD_PLATFORM_DATABASE_URL").expect("managed platform URL"),
        )
        .expect("local database URL");
        dsn.set_path(&format!("/{name}"));
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("isolated home");
        let version = env!("CARGO_PKG_VERSION");
        let root = home.path().join("wyrd/server");
        let installed = root.join("versions").join(version);
        fs::create_dir_all(&installed).expect("installation directory");
        let binary = assert_cmd::cargo::cargo_bin("wyrd").with_file_name("wyrd-server");
        assert!(
            binary.is_file(),
            "build the production wyrd-server binary through mise first"
        );
        symlink(&binary, installed.join("wyrd-server")).expect("installed executable fixture");
        symlink(format!("versions/{version}"), root.join("current")).expect("active version");
        Self {
            child: None,
            stdout: None,
            stderr: None,
            home,
            _database: database,
            dsn: SecretString::from(dsn.to_string()),
            bind: free_bind(),
            grpc: free_bind(),
            peer: free_bind(),
        }
    }

    /// A fresh process resolving only the fixture's saved local files.
    fn command(&self) -> Command {
        let mut command = wyrd(self.home.path());
        command
            .current_dir(self.home.path())
            .env("WYRD_DATABASE_URL", self.dsn.expose_secret())
            .env_remove("WYRD_PLATFORM_DATABASE_URL")
            .env_remove("WYRD_STORAGE_URL")
            .env_remove("WYRD_CONFIG_PATH")
            .env("APP_ENV", "development")
            .env("WYRD_SERVER_BIND", &self.bind)
            .env("WYRD_GRPC_BIND", &self.grpc)
            .env("WYRD_BIFROST_PEER_BIND_ADDR", &self.peer)
            .env("WYRD_METRICS_BIND", "127.0.0.1:0");
        command
    }

    /// Start local setup and wait for its explicit client-ready message.
    ///
    /// # Panics
    /// Panics when startup fails, hangs beyond its readiness budget, or puts
    /// credentials in stdout. An unsuccessful child is settled before panic.
    async fn start(&mut self, hosts: &[&str]) {
        let mut command = self.command();
        command.args(["server", "dev", "--tenant", "local"]);
        for host in hosts {
            command.args(["--host", host]);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("installed CLI starts");
        let mut stdout = BufReader::new(child.stdout.take().expect("stdout pipe"));
        let mut stderr = child.stderr.take().expect("stderr pipe");
        let errors = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr
                .read_to_end(&mut bytes)
                .await
                .expect("stderr drained");
            String::from_utf8_lossy(&bytes).into_owned()
        });
        let mut printed = String::new();
        let ready = tokio::time::timeout(Duration::from_secs(150), async {
            loop {
                let mut line = String::new();
                if stdout.read_line(&mut line).await.expect("startup output") == 0 {
                    return false;
                }
                let ready = line.starts_with("Local Wyrd ready at ");
                printed.push_str(&line);
                if ready {
                    return true;
                }
            }
        })
        .await
        .unwrap_or(false);
        if !ready {
            if let Some(pid) = child.id() {
                Command::new("kill")
                    .args(["-INT", &pid.to_string()])
                    .status()
                    .await
                    .expect("interrupt failed setup");
            }
            child.wait().await.expect("failed child reaped");
            panic!(
                "local startup failed: {printed}\n{}",
                errors.await.expect("stderr captured")
            );
        }
        assert!(
            !printed.contains("wyrd_sk_") && !printed.contains("wyrd_global_"),
            "startup stdout carries no secrets"
        );
        self.stdout = Some(tokio::spawn(async move {
            let mut remainder = String::new();
            stdout
                .read_to_string(&mut remainder)
                .await
                .expect("stdout drained");
            printed.push_str(&remainder);
            printed
        }));
        self.stderr = Some(errors);
        self.child = Some(child);
    }

    /// Interrupt only the CLI and prove it settles its owned server.
    ///
    /// # Panics
    /// Panics when interruption fails, the CLI reports a different status,
    /// credentials reach output, or the server listener remains active.
    async fn stop(&mut self, signal: &str) {
        let mut child = self.child.take().expect("running CLI");
        let pid = child.id().expect("live CLI pid");
        assert!(
            Command::new("kill")
                .args([signal, &pid.to_string()])
                .status()
                .await
                .expect("interrupt CLI")
                .success()
        );
        assert_eq!(child.wait().await.expect("CLI reaped").code(), Some(130));
        let stdout = self
            .stdout
            .take()
            .expect("stdout capture")
            .await
            .expect("stdout joined");
        let stderr = self
            .stderr
            .take()
            .expect("stderr capture")
            .await
            .expect("stderr joined");
        let key = fs::read_to_string(self.home.path().join("wyrd/credentials.toml"))
            .expect("saved credential");
        let environment = self.environment();
        let secret = GlobalConfig::local_api_key(&environment)
            .expect("protected key")
            .expect("key saved");
        assert!(
            !stdout.contains(secret.expose_secret()) && !stderr.contains(secret.expose_secret())
        );
        assert!(
            !stderr.contains("wyrd_sk_") && !stderr.contains("wyrd_global_"),
            "stderr carries no tenant or platform credentials"
        );
        assert!(key.contains("api_key"));
        assert!(
            TcpListener::bind(&self.bind).is_ok(),
            "the server's listener is closed after interrupting its CLI"
        );
    }

    /// Credentials-test environment containing no exported key or endpoint.
    ///
    /// # Panics
    /// Panics if the temporary home path is not UTF-8, which the fixture's
    /// `tempfile` location never produces on supported hosts.
    fn environment(&self) -> Environment {
        Environment::from([(
            "WYRD_CONFIG_HOME",
            self.home.path().join("wyrd").to_str().expect("UTF-8 home"),
        )])
    }

    /// Build a new shared client from the files local setup saved.
    ///
    /// # Panics
    /// Panics when local setup's public configuration is unusable.
    fn client(&self) -> WyrdClient {
        let environment = self.environment();
        let global = GlobalConfig::load_in(&environment).expect("saved config loads");
        WyrdClient::with_config(ClientConfig::from_environment(
            environment,
            &global,
            None,
            None,
        ))
        .expect("fresh client resolves saved key")
    }
}

/// Select an isolated loopback listener for one child input.
///
/// # Panics
/// Panics if the OS cannot allocate a local ephemeral listener.
fn free_bind() -> String {
    TcpListener::bind("127.0.0.1:0")
        .expect("ephemeral port")
        .local_addr()
        .expect("bound address")
        .to_string()
}

/// First use migrates an empty database, saves a private key and both
/// endpoints, and supports fresh CLI writes and client reads outside the
/// checkout. A stale saved gRPC destination is replaced by the selected
/// custom loopback listener, which also governs a non-loopback server-file
/// setting, and a fresh saved client writes Bifrost rows over it.
///
/// # Panics
/// Panics when bootstrap, the public write/read, or process cleanup fails,
/// or when the stale destination receives a connection.
#[tokio::test(flavor = "multi_thread")]
async fn server_dev_first_run_is_client_ready() {
    let mut local = LocalDev::new().await;
    let stale = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("stale gRPC destination");
    let config = local.home.path().join("wyrd/config.toml");
    fs::write(
        &config,
        format!(
            "# retain unrelated settings\n[client]\ngrpc_url = \"http://{}\"\n",
            stale.local_addr().expect("stale address")
        ),
    )
    .expect("stale client config");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).expect("protected config");
    let server_file = local.home.path().join(".config/wyrd");
    fs::create_dir_all(&server_file).expect("server file directory");
    fs::write(
        server_file.join("server.toml"),
        format!("[grpc]\nbind = \"0.0.0.0:{}\"\n", local.grpc.rsplit_once(':').expect("port").1),
    )
    .expect("non-loopback server file");
    local.start(&[]).await;
    let saved = GlobalConfig::load_in(&local.environment())
        .expect("saved config")
        .client;
    assert_eq!(saved.grpc_url, Some(format!("http://{}", local.grpc)));
    assert!(
        fs::read_to_string(&config)
            .expect("saved config text")
            .contains("# retain unrelated settings")
    );
    let table = TableConfig::from_json_schema(
        "vala.datasets.local_rows",
        &json!({
            "type": "object",
            "properties": { "id": { "type": "integer" } },
            "required": ["id"],
        }),
    )
    .expect("table declares");
    let writer = Bifrost::connect_with_table(&local.client(), table)
        .await
        .expect("saved gRPC destination connects");
    writer.register().await.expect("table registers");
    writer
        .insert(
            serde_json::to_vec(&json!({ "id": 1 })).expect("row serializes"),
            Correlation::default(),
        )
        .expect("row inserts");
    writer.shutdown().await.expect("gRPC write is acknowledged");
    let rows: Vec<serde_json::Value> = Bifrost::connect(&local.client())
        .await
        .expect("fresh reader")
        .sql_as("SELECT id FROM vala.datasets.local_rows", &[])
        .await
        .expect("fresh saved client reads");
    assert_eq!(rows, [json!({ "id": 1 })]);
    assert!(
        tokio::time::timeout(Duration::ZERO, stale.accept())
            .await
            .is_err(),
        "no bearer or data reaches the stale gRPC destination"
    );
    let listed = local
        .command()
        .arg("list")
        .output()
        .await
        .expect("documented fresh CLI list");
    assert!(
        listed.status.success(),
        "{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let prompt = local.home.path().join("ask-prompt.yaml");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/cards/gateway_inference/ask-prompt.yaml"),
        &prompt,
    )
    .expect("checked-in prompt copied outside checkout");
    let applied = local
        .command()
        .args(["apply", prompt.to_str().expect("UTF-8 prompt")])
        .output()
        .await
        .expect("fresh CLI write");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let cards = Cards::with_client(local.client());
    let card = cards
        .get(CardSelector::named(
            CardKind::Prompt,
            "default".parse().expect("space"),
            "ask-prompt".parse().expect("name"),
        ))
        .await
        .expect("fresh client read");
    assert_eq!(card.metadata.name.as_str(), "ask-prompt");
    assert_eq!(
        fs::metadata(local.home.path().join("wyrd/credentials.toml"))
            .expect("credential file")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    local.stop("-INT").await;
}

/// Reruns retain the active release and key; a failed one-time save does not
/// turn a retry into another issuance. Missing input and a non-loopback
/// public gRPC bind are refused before setup, and an explicit host selected
/// on a later start is still connected.
///
/// # Panics
/// Panics when a retry changes persisted identity or exposes credentials.
#[tokio::test(flavor = "multi_thread")]
async fn server_dev_repeat_and_partial_failure_preserve_state() {
    let mut local = LocalDev::new().await;
    let current = local.home.path().join("wyrd/server/current");
    let retained = fs::read_link(&current).expect("retained installation");
    fs::remove_file(&current).expect("first-use installation fixture");
    let proxy = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("local download proxy");
    let downloading = local
        .command()
        .env(
            "HTTPS_PROXY",
            format!("http://{}", proxy.local_addr().expect("proxy address")),
        )
        .env("NO_PROXY", "")
        .args(["server", "dev", "--tenant", "local"])
        .kill_on_drop(true)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("first-use download");
    let (_stalled, _) = tokio::time::timeout(Duration::from_secs(15), proxy.accept())
        .await
        .expect("download reaches only the local proxy")
        .expect("proxy accepts download");
    assert!(
        Command::new("kill")
            .args([
                "-INT",
                &downloading.id().expect("downloading CLI").to_string()
            ])
            .status()
            .await
            .expect("interrupt first-use download")
            .success()
    );
    let interrupted = tokio::time::timeout(Duration::from_secs(15), downloading.wait_with_output())
        .await
        .expect("download cancellation settles")
        .expect("download CLI reaped");
    assert_eq!(interrupted.status.code(), Some(130));
    assert!(String::from_utf8_lossy(&interrupted.stderr).contains("Local installation stopped"));
    assert!(!local.home.path().join("wyrd/credentials.toml").exists());
    symlink(retained, &current).expect("restore retained installation");
    let missing = local
        .command()
        .env_remove("WYRD_DATABASE_URL")
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("missing input");
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("WYRD_DATABASE_URL"));
    let exposed = local
        .command()
        .env("WYRD_GRPC_BIND", "0.0.0.0:50051")
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("non-loopback gRPC input");
    assert!(!exposed.status.success());
    let errors = String::from_utf8_lossy(&exposed.stderr);
    assert!(errors.contains("WYRD_GRPC_BIND"), "{errors}");
    assert!(!errors.contains("wyrd_sk_") && !errors.contains("wyrd_global_"));
    assert!(!local.home.path().join("wyrd/credentials.toml").exists());
    let mut restricted =
        url::Url::parse(&std::env::var("WYRD_DATABASE_URL").expect("managed tenant URL"))
            .expect("tenant URL");
    restricted.set_path(
        url::Url::parse(local.dsn.expose_secret())
            .expect("fixture URL")
            .path(),
    );
    let refused = local
        .command()
        .env("WYRD_DATABASE_URL", restricted.as_str())
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("restricted migration");
    assert!(!refused.status.success());
    let errors = String::from_utf8_lossy(&refused.stderr);
    assert!(errors.contains("migration"), "{errors}");
    assert!(
        !errors.contains(restricted.as_str()),
        "database credential remains redacted"
    );
    let hardened = local
        .command()
        .env("APP_ENV", "production")
        .env_remove("WYRD_SIGNING_KEY_FILE")
        .env_remove("WYRD_SIGNING_KEY_PEM")
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("hardened startup");
    assert!(!hardened.status.success());
    let errors = String::from_utf8_lossy(&hardened.stderr);
    assert!(errors.contains("readiness"), "{errors}");
    assert!(TcpListener::bind(&local.bind).is_ok());
    local.start(&[]).await;
    local.stop("-INT").await;
    let credentials = local.home.path().join("wyrd/credentials.toml");
    let key = fs::read_to_string(&credentials).expect("first key");
    let config = local.home.path().join("wyrd/config.toml");
    let original = fs::read_to_string(&config).expect("first config");
    fs::write(&config, format!("# retained local settings\n{original}"))
        .expect("unrelated comment");
    let installed = fs::read_link(local.home.path().join("wyrd/server/current")).expect("release");
    let codex = local.home.path().join(".codex");
    fs::create_dir_all(&codex).expect("later detected host");
    local.start(&["codex"]).await;
    local.stop("-TERM").await;
    assert!(
        fs::read_to_string(codex.join("config.toml"))
            .expect("explicit host on a later start")
            .contains(&format!("http://{}", local.bind))
    );
    assert_eq!(fs::read_to_string(&credentials).expect("retained key"), key);
    assert!(
        fs::read_to_string(&config)
            .expect("retained config")
            .contains("# retained local settings")
    );
    assert_eq!(
        fs::read_link(local.home.path().join("wyrd/server/current")).expect("retained release"),
        installed
    );

    let partial = LocalDev::new().await;
    let directory = partial.home.path().join("wyrd");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o555))
        .expect("read-only directory");
    let failed = partial
        .command()
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("failed one-time persistence");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore directory");
    assert!(!failed.status.success());
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(output.contains("local persistence"), "{output}");
    assert!(output.contains("recover-admin"));
    assert!(!output.contains("wyrd_sk_") && !output.contains("wyrd_global_"));
    assert!(!directory.join("credentials.toml").exists());
    let retried = partial
        .command()
        .args(["server", "dev", "--tenant", "local"])
        .output()
        .await
        .expect("safe partial retry");
    assert!(!retried.status.success());
    let errors = String::from_utf8_lossy(&retried.stderr);
    assert!(errors.contains("saved credential"), "{errors}");
    assert!(
        !directory.join("credentials.toml").exists(),
        "an active tenant's key is never reissued"
    );
    assert!(
        TcpListener::bind(&partial.bind).is_ok(),
        "failure settles the server"
    );
}

/// The selected host configuration discovers and calls a read tool using
/// the same saved key as fresh clients, with no host-file credential.
///
/// # Panics
/// Panics when host installation, discovery, or the authenticated read fails.
#[tokio::test(flavor = "multi_thread")]
async fn server_dev_selected_mcp_host_reads() {
    let mut local = LocalDev::new().await;
    let codex = local.home.path().join(".codex");
    fs::create_dir_all(&codex).expect("detected host");
    fs::write(codex.join("config.toml"), "# retain host settings\n").expect("host config");
    local.start(&["codex"]).await;
    let config = fs::read_to_string(codex.join("config.toml")).expect("selected host config");
    assert!(config.contains("# retain host settings"));
    assert!(config.contains(&format!("http://{}", local.bind)));
    assert!(!config.contains("wyrd_sk_") && !config.contains("wyrd_global_"));
    let endpoint = format!("http://{}", local.bind);
    let (tools, result) =
        list_tables_through_proxy(local.home.path(), &["--server", &endpoint], &[]).await;
    assert!(tools.iter().any(|tool| tool == "bifrost.list_tables"));
    assert_ne!(result.is_error, Some(true));
    local.stop("-INT").await;
}
