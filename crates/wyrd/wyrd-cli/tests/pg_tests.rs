//! Local startup through the installed CLI and real serving binary, isolated
//! from the user's credentials, database, working directory, and MCP hosts.

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _, symlink};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use wyrd_client::bifrost::BifrostOptions;

use secrecy::{ExposeSecret as _, SecretString};
use serde_json::json;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, BufReader};
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use wyrd_client::bifrost::{Correlation, TableConfig};
use wyrd_client::cards::{CardSelector, Cards};
use wyrd_client::environment::Environment;
use wyrd_client::saved_login::{SavedLogin, SavedLogins, canonical_origin};
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_dev_fixtures::pg::UnmigratedDatabase;
use wyrd_spec::auth::{SecretBearer, TokenResponse, TokenType};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::ids::TenantSlug;
use wyrd_testing::reserve_loopback_addr;

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
    /// The JSON readiness result of the current start, when JSON was used.
    ready: Option<serde_json::Value>,
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
        let dsn = database.url().expect("fixture database URL");
        let home = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("isolated home");
        let version = env!("CARGO_PKG_VERSION");
        let root = home.path().join("wyrd/server");
        let installed = root.join("versions").join(version);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&installed)
            .expect("installation directory");
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
            dsn,
            bind: free_bind(),
            grpc: free_bind(),
            peer: free_bind(),
            ready: None,
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
            .env_remove("WYRD_SEALING_KEY_FILE")
            .env_remove("WYRD_SEALING_KEY_BASE64")
            .env("APP_ENV", "development")
            .env("WYRD_SERVER_BIND", &self.bind)
            .env("WYRD_GRPC_BIND", &self.grpc)
            .env("WYRD_BIFROST_PEER_BIND_ADDR", &self.peer)
            .env("WYRD_METRICS_BIND", "127.0.0.1:0")
            .env_remove("RUST_LOG");
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
        self.start_with(command).await;
    }

    /// Start unattended JSON-mode setup and wait for its readiness result.
    ///
    /// # Panics
    /// Panics as [`LocalDev::start`] does, or when anything but the one
    /// readiness line reaches stdout before readiness.
    async fn start_json(&mut self) {
        let mut command = self.command();
        command.args([
            "server",
            "dev",
            "--tenant",
            "local",
            "--non-interactive",
            "--format",
            "json",
        ]);
        self.start_with(command).await;
        let ready = self.ready.as_ref().expect("a JSON readiness result");
        assert_eq!(ready["ready"], true);
    }

    /// Spawn the prepared `server dev` command and wait for readiness.
    ///
    /// # Panics
    /// Panics as [`LocalDev::start`] does.
    async fn start_with(&mut self, mut command: Command) {
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
                let json = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .filter(|value| value["ready"] == true);
                let ready = line.starts_with("Local Wyrd ready at ") || json.is_some();
                printed.push_str(&line);
                if ready {
                    if let Some(json) = json {
                        assert_eq!(printed, line, "JSON mode prints only its readiness result");
                        self.ready = Some(json);
                    }
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
            !printed.contains("wyrd_sk_")
                && !printed.contains("wyrd_global_")
                && !printed.contains(self.dsn.expose_secret()),
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

    /// Interrupt only the CLI and prove it settles its owned server,
    /// returning its captured stdout and stderr for further leak checks.
    ///
    /// # Panics
    /// Panics when interruption fails, the CLI reports a different status,
    /// credentials reach output, or the server listener remains active.
    async fn stop(&mut self, signal: &str) -> String {
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
        stdout + &stderr
    }

    /// Run a fresh `wyrd <args>` that reads the saved local files, piping
    /// `stdin` to it as the quickstart does for a managed secret.
    ///
    /// # Panics
    /// Panics when the command cannot be spawned, fed, or reaped.
    async fn stdin_command(&self, args: &[&str], stdin: &str) -> std::process::Output {
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("fresh CLI starts");
        let mut input = child.stdin.take().expect("stdin pipe");
        tokio::io::AsyncWriteExt::write_all(&mut input, stdin.as_bytes())
            .await
            .expect("stdin written");
        drop(input);
        child.wait_with_output().await.expect("fresh CLI exits")
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
        WyrdClient::from_environment(self.environment()).expect("fresh client resolves saved key")
    }
}

/// Reserve one loopback address for a child listener through the shared
/// test-port reserver, whose cross-process claim keeps every sibling test
/// process, `WyrdTestServer` included, off the same port until this process
/// exits.
///
/// # Panics
/// Panics when no loopback port can be reserved.
fn free_bind() -> String {
    reserve_loopback_addr()
        .expect("a free loopback port is reserved")
        .to_string()
}

/// Every tenant slug in the deployment database at `dsn`, read in an
/// operator session because tenant rows are hidden from ordinary sessions.
///
/// # Panics
/// Panics when the database cannot be reached or the read fails.
async fn tenant_slugs(dsn: &SecretString) -> Vec<String> {
    let pool = sqlx::PgPool::connect(dsn.expose_secret())
        .await
        .expect("deployment database connects");
    let mut operator = pool.begin().await.expect("operator transaction");
    sqlx::query("SELECT set_config('app.operator', 'on', true)")
        .execute(&mut *operator)
        .await
        .expect("operator session");
    let slugs = sqlx::query_scalar("SELECT slug FROM platform.tenants ORDER BY slug")
        .fetch_all(&mut *operator)
        .await
        .expect("tenant slugs");
    drop(operator);
    pool.close().await;
    slugs
}

/// First use migrates an empty database, saves a private key and both
/// endpoints, and supports fresh CLI writes and client reads outside the
/// checkout. Unattended JSON mode refuses missing input before any side
/// effect and an occupied listener by stage, each as one stderr document,
/// then prints one readiness line and keeps serving while the client works;
/// its progress and server output reach only the named diagnostic log. A stale saved gRPC destination is replaced by the selected
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
        format!(
            "[grpc]\nbind = \"0.0.0.0:{}\"\n",
            local.grpc.rsplit_once(':').expect("port").1
        ),
    )
    .expect("non-loopback server file");
    let json_args = [
        "server",
        "dev",
        "--tenant",
        "local",
        "--non-interactive",
        "--format",
        "json",
    ];
    let missing = local
        .command()
        .env_remove("WYRD_DATABASE_URL")
        .args(json_args)
        .output()
        .await
        .expect("missing-input run");
    let refused = json_failure(&missing);
    assert_eq!(refused["details"]["stage"], "input");
    assert!(
        refused["details"]["log"].is_null(),
        "nothing has run: {refused}"
    );
    assert!(!local.home.path().join("wyrd/server/logs").exists());
    let occupied = TcpListener::bind(&local.bind).expect("occupy the HTTP listener");
    let busy = local
        .command()
        .args(json_args)
        .output()
        .await
        .expect("occupied-listener run");
    drop(occupied);
    let refused = json_failure(&busy);
    assert_eq!(refused["details"]["stage"], "listener");
    let log = local.home.path().join("wyrd/server/logs/server-dev.log");
    assert_eq!(refused["details"]["log"], log.to_str().expect("UTF-8 log"));
    local.start_json().await;
    let ready = local.ready.clone().expect("readiness result");
    assert_eq!(ready["endpoint"], format!("http://{}", local.bind));
    assert_eq!(ready["grpc"], format!("http://{}", local.grpc));
    assert_eq!(ready["tenant"], "local");
    assert_eq!(ready["log"], log.to_str().expect("UTF-8 log"));
    assert_eq!(ready["mcp"], serde_json::Value::Null);
    let global = GlobalConfig::load_in(&local.environment()).expect("saved config");
    let dev = &global.dev;
    assert_eq!(dev.tenant.as_deref(), Some("local"));
    assert_eq!(dev.bind.as_deref(), Some(local.bind.as_str()));
    assert_eq!(dev.grpc_bind.as_deref(), Some(local.grpc.as_str()));
    let storage = local.home.path().join("wyrd/server/data/storage");
    assert_eq!(
        dev.storage_url.as_deref(),
        Some(
            url::Url::from_file_path(&storage)
                .expect("absolute")
                .as_str()
        )
    );
    assert!(storage.is_dir() && !local.home.path().join(".wyrd").exists());
    assert_eq!(
        dev.bifrost_data_dir.as_deref().map(Path::new),
        Some(local.home.path().join("wyrd/server/data/bifrost").as_path())
    );
    let general = fs::read_to_string(&config).expect("saved config text");
    assert!(!general.contains(local.dsn.expose_secret()) && !general.contains("wyrd_sk_"));
    let (database, _) = GlobalConfig::local_databases(&local.environment()).expect("protected");
    assert_eq!(
        database.expect("saved database").expose_secret(),
        local.dsn.expose_secret()
    );
    let saved = global.client;
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
    let writer = Bifrost::connect_with(
        &local.client(),
        BifrostOptions {
            table: Some(table),
            ..BifrostOptions::default()
        },
    )
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
    let rows: Vec<serde_json::Value> =
        Bifrost::connect_with(&local.client(), BifrostOptions::default())
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
        .args(["declare", prompt.to_str().expect("UTF-8 prompt")])
        .output()
        .await
        .expect("fresh CLI write");
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let cards = Cards::connect_with(&local.client())
        .await
        .expect("handle connects");
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
    let output = local.stop("-INT").await;
    assert_eq!(
        output.trim(),
        serde_json::to_string(&ready).expect("readiness serializes"),
        "JSON mode writes only its readiness line to stdout and nothing to stderr"
    );
    let diagnostics = fs::read_to_string(&log).expect("diagnostic log");
    assert!(diagnostics.contains("Migrating the installed Wyrd server"));
    assert!(diagnostics.contains("Local startup stopped"));
    assert!(!diagnostics.contains("wyrd_sk_") && !diagnostics.contains(local.dsn.expose_secret()));
}

/// Decode a JSON-mode `server dev` refusal: nonzero exit, empty stdout, and
/// one `WYRD_CLI_503_SERVER_DEV` document as the whole of stderr.
///
/// # Panics
/// Panics when the run succeeded, wrote stdout, or stderr is not that one
/// document.
fn json_failure(output: &std::process::Output) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(69));
    assert!(output.stdout.is_empty(), "a refusal prints no result");
    let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap_or_else(|_| {
        panic!(
            "stderr is one error document: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(error["code"], "WYRD_CLI_503_SERVER_DEV");
    error
}

/// Reruns retain the active release and key; a failed one-time save does not
/// turn a retry into another issuance. Missing input and a non-loopback
/// public gRPC bind are refused before setup, and an explicit host selected
/// on a later start is still connected. Another database whose server rejects
/// the saved key, and a configuration directory selecting a remote server
/// with or without a key, are refused without changing saved files or
/// creating a tenant, and the saved deployment then resumes. A same-origin
/// key with no saved `[dev]` tenant is refused before startup even with real
/// platform authority, and a valid same-origin, same-slug login from another
/// deployment never stands in for the saved key when its database is
/// selected.
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

    // A same-origin key with no saved [dev] tenant is not this local
    // deployment's selection: requesting another tenant, with or without real
    // platform authority, is refused before startup and changes nothing.
    let recovered = Command::new(local.home.path().join("wyrd/server/current/wyrd-server"))
        .arg("recover-root")
        .env("WYRD_DATABASE_URL", local.dsn.expose_secret())
        .env_remove("WYRD_PLATFORM_DATABASE_URL")
        .env("APP_ENV", "development")
        .output()
        .await
        .expect("platform authority");
    assert!(
        recovered.status.success(),
        "platform recovery: {}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let authority = String::from_utf8(recovered.stdout)
        .expect("UTF-8 recovery")
        .lines()
        .find(|line| line.starts_with("wyrd_global_"))
        .expect("platform credential")
        .to_owned();
    let settings = fs::read_to_string(&config).expect("saved settings");
    let mut unselected: toml_edit::DocumentMut = settings.parse().expect("saved settings parse");
    unselected.remove("dev").expect("saved dev section");
    fs::write(&config, unselected.to_string()).expect("unselected key");
    let unselected = (
        fs::read(&credentials).expect("key"),
        fs::read(&config).expect("config"),
    );
    for platform in [None, Some(authority.as_str())] {
        let mut command = local.command();
        if let Some(platform) = platform {
            command.env("WYRD_PLATFORM_CREDENTIAL", platform);
        }
        let refused = command
            .args(["server", "dev", "--tenant", "another", "--non-interactive"])
            .output()
            .await
            .expect("unselected key");
        let errors = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{errors}");
        assert!(errors.contains("not created by local setup"), "{errors}");
        assert!(!errors.contains("wyrd_global_") && !errors.contains("wyrd_sk_"));
        assert_eq!(
            (
                fs::read(&credentials).expect("key"),
                fs::read(&config).expect("config")
            ),
            unselected,
            "the unselected key and settings are byte-preserved"
        );
        assert!(TcpListener::bind(&local.bind).is_ok(), "nothing started");
    }
    assert!(
        !tenant_slugs(&local.dsn)
            .await
            .contains(&"another".to_owned()),
        "no other tenant or key was issued"
    );
    fs::write(&config, &settings).expect("restore the dev selection");

    // A valid login for this origin and tenant slug from another deployment
    // cannot stand in for the saved key when its database is selected: only
    // the exact saved key is proven, and refusal precedes every write.
    let signing = wyrd_testing::keys::oracle_peer_signing_key_pem();
    let mut elsewhere = LocalDev::new().await;
    elsewhere.bind.clone_from(&local.bind);
    elsewhere.grpc.clone_from(&local.grpc);
    elsewhere.peer.clone_from(&local.peer);
    let mut command = elsewhere.command();
    command
        .env("WYRD_SIGNING_KEY_PEM", signing.expose_secret())
        .args(["server", "dev", "--tenant", "local"]);
    elsewhere.start_with(command).await;
    let token = elsewhere
        .client()
        .access_token()
        .await
        .expect("the other deployment's session");
    elsewhere.stop("-INT").await;
    let logins = SavedLogins::at(local.home.path().join("wyrd"));
    let origin = canonical_origin(&format!("http://{}", local.bind)).expect("origin");
    let slug: TenantSlug = "local".parse().expect("slug");
    logins
        .save(
            SavedLogin::from_token(
                origin.clone(),
                slug.clone(),
                TokenResponse {
                    access_token: token,
                    token_type: TokenType::Bearer,
                    expires_in: 3600,
                    refresh_token: Some(SecretBearer::new("unused-refresh".to_owned())),
                    issued_token_type: None,
                },
            )
            .expect("login record"),
        )
        .expect("competing login");
    let masked_files = (
        fs::read(&credentials).expect("key"),
        fs::read(&config).expect("config"),
    );
    let data = elsewhere.home.path().join("wyrd/server/data");
    let masked = local
        .command()
        .env("WYRD_DATABASE_URL", elsewhere.dsn.expose_secret())
        .env("WYRD_SIGNING_KEY_PEM", signing.expose_secret())
        .env(
            "WYRD_STORAGE_URL",
            url::Url::from_file_path(data.join("storage"))
                .expect("absolute")
                .as_str(),
        )
        .env("WYRD_BIFROST_DATA_DIR", data.join("bifrost"))
        .args(["server", "dev", "--non-interactive"])
        .output()
        .await
        .expect("login-masked key");
    let errors = String::from_utf8_lossy(&masked.stderr);
    assert!(!masked.status.success(), "{errors}");
    assert!(errors.contains("does not accept the saved key"), "{errors}");
    assert_eq!(
        (
            fs::read(&credentials).expect("key"),
            fs::read(&config).expect("config")
        ),
        masked_files,
        "a competing login never lets setup replace saved settings"
    );
    assert!(
        TcpListener::bind(&local.bind).is_ok(),
        "refusal settles the server"
    );
    logins
        .remove(&origin, &slug)
        .expect("competing login removed");

    // Selecting another database with the same tenant slug is refused once
    // its server rejects the saved key: the saved files are untouched, no
    // tenant is created there, and the saved deployment then resumes.
    let other = UnmigratedDatabase::create()
        .await
        .expect("second managed database");
    let other_dsn = other.url().expect("second database URL");
    let saved_files = (
        fs::read(&credentials).expect("key"),
        fs::read(&config).expect("config"),
    );
    let switched = local
        .command()
        .env("WYRD_DATABASE_URL", other_dsn.expose_secret())
        .args(["server", "dev", "--non-interactive"])
        .output()
        .await
        .expect("another database");
    let errors = String::from_utf8_lossy(&switched.stderr);
    assert!(!switched.status.success(), "{errors}");
    assert!(errors.contains("does not accept the saved key"), "{errors}");
    let secret = GlobalConfig::local_api_key(&local.environment())
        .expect("protected key")
        .expect("key saved");
    assert!(
        !format!("{}{errors}", String::from_utf8_lossy(&switched.stdout))
            .contains(secret.expose_secret()),
        "the refused key is never printed"
    );
    assert_eq!(
        (
            fs::read(&credentials).expect("key"),
            fs::read(&config).expect("config")
        ),
        saved_files,
        "the saved deployment's files are unchanged"
    );
    assert!(
        TcpListener::bind(&local.bind).is_ok(),
        "refusal settles the server"
    );
    assert!(
        !tenant_slugs(&other_dsn).await.contains(&"local".to_owned()),
        "no tenant or key was issued in the other database"
    );
    local.start(&[]).await;
    local.stop("-INT").await;
    assert_eq!(fs::read(&credentials).expect("key"), saved_files.0);

    // A configuration directory that selects another server, with or without
    // a saved key and with no local deployment, is refused before anything.
    for key in [None, Some("wyrd_sk_remote_unrelated")] {
        let remote = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .expect("remote config home");
        let remote_config = remote.path().join("config.toml");
        fs::write(
            &remote_config,
            "[client]\nhttp_url = \"https://wyrd.example.com\"\n",
        )
        .expect("remote selection");
        fs::set_permissions(&remote_config, fs::Permissions::from_mode(0o600))
            .expect("protected config");
        if let Some(key) = key {
            let remote_key = remote.path().join("credentials.toml");
            fs::write(&remote_key, format!("[default]\napi_key = \"{key}\"\n"))
                .expect("remote key");
            fs::set_permissions(&remote_key, fs::Permissions::from_mode(0o600))
                .expect("protected key");
        }
        let before: Vec<_> = fs::read_dir(remote.path())
            .expect("remote files")
            .map(|entry| {
                let path = entry.expect("entry").path();
                (path.clone(), fs::read(path).expect("remote file"))
            })
            .collect();
        let refused = local
            .command()
            .env("WYRD_CONFIG_HOME", remote.path())
            .args(["server", "dev", "--tenant", "local", "--non-interactive"])
            .output()
            .await
            .expect("remote selection");
        let errors = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{errors}");
        assert!(errors.contains("selects another Wyrd server"), "{errors}");
        let after: Vec<_> = fs::read_dir(remote.path())
            .expect("remote files")
            .map(|entry| {
                let path = entry.expect("entry").path();
                (path.clone(), fs::read(path).expect("remote file"))
            })
            .collect();
        assert_eq!(after, before, "the remote selection is byte-preserved");
    }

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
    assert!(output.contains("No tenant key has been issued"), "{output}");
    assert!(!output.contains("wyrd_sk_") && !output.contains("wyrd_global_"));
    assert!(!directory.join("credentials.toml").exists());
    assert!(
        TcpListener::bind(&partial.bind).is_ok(),
        "failure settles the server"
    );
    let mut partial = partial;
    partial.start(&[]).await;
    partial.stop("-INT").await;
    let credentials = directory.join("credentials.toml");
    let mut lost = fs::read_to_string(&credentials).expect("saved key");
    let saved = GlobalConfig::local_api_key(&partial.environment())
        .expect("protected")
        .expect("key");
    lost = lost.replace(saved.expose_secret(), "");
    fs::write(&credentials, lost.replace("api_key = \"\"\n", "")).expect("key lost");
    for platform in [None, Some("wyrd_global_unconfirmed")] {
        let mut command = partial.command();
        if let Some(platform) = platform {
            command.env("WYRD_PLATFORM_CREDENTIAL", platform);
        }
        let retried = command
            .args(["server", "dev", "--non-interactive"])
            .output()
            .await
            .expect("lost-key retry");
        assert!(!retried.status.success());
        let errors = String::from_utf8_lossy(&retried.stderr);
        assert!(errors.contains("saved credential"), "{errors}");
        assert!(errors.contains("recover-admin"), "{errors}");
        assert!(
            GlobalConfig::local_api_key(&partial.environment())
                .expect("protected")
                .is_none(),
            "an active tenant's key is never reissued without confirmed recovery"
        );
        assert!(
            TcpListener::bind(&partial.bind).is_ok(),
            "failure settles the server"
        );
    }
}

/// A bare unattended restart from another working directory, with no
/// database, listener, storage, or tenant inputs, reuses the saved settings,
/// key, and data. Unattended runs missing inputs list them all before any
/// effect, and conflicting or credential-bearing arguments are refused. A
/// later listener override of the same deployment keeps its key and resaves
/// the endpoint.
///
/// # Panics
/// Panics when the restart changes identity or data, or a refusal has effects.
#[tokio::test(flavor = "multi_thread")]
async fn server_dev_saved_settings_survive_working_directory_change() {
    let mut local = LocalDev::new().await;
    let fresh = local
        .command()
        .env_remove("WYRD_DATABASE_URL")
        .args(["server", "dev", "--non-interactive"])
        .output()
        .await
        .expect("missing inputs");
    assert!(!fresh.status.success());
    let errors = String::from_utf8_lossy(&fresh.stderr);
    assert!(
        errors.contains("WYRD_DATABASE_URL") && errors.contains("--tenant"),
        "{errors}"
    );
    for refused in [
        &[
            "--tenant",
            "local",
            "--storage-url",
            "s3://user:pw@bucket/x",
        ][..],
        &[
            "--tenant",
            "local",
            "--gateway-file",
            "g.json",
            "--skip-gateway",
        ],
    ] {
        let output = local
            .command()
            .args(["server", "dev", "--non-interactive"])
            .args(refused)
            .output()
            .await
            .expect("refused input");
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("pw@"));
    }
    assert!(!local.home.path().join("wyrd/credentials.toml").exists());
    assert!(!local.home.path().join("wyrd/server/data").exists());

    local.start(&[]).await;
    let prompt = local.home.path().join("ask-prompt.yaml");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../fixtures/cards/gateway_inference/ask-prompt.yaml"),
        &prompt,
    )
    .expect("prompt fixture");
    let applied = local
        .command()
        .args(["declare", prompt.to_str().expect("UTF-8 prompt")])
        .output()
        .await
        .expect("card write");
    assert!(applied.status.success());
    local.stop("-INT").await;
    let key = GlobalConfig::local_api_key(&local.environment())
        .expect("protected")
        .expect("key");

    let elsewhere = tempfile::tempdir().expect("another working directory");
    let mut command = local.command();
    command
        .current_dir(elsewhere.path())
        .env_remove("WYRD_DATABASE_URL")
        .env_remove("WYRD_SERVER_BIND")
        .env_remove("WYRD_GRPC_BIND")
        .env_remove("WYRD_BIFROST_DATA_DIR")
        .args(["server", "dev", "--non-interactive"]);
    local.start_with(command).await;
    let restarted = GlobalConfig::local_api_key(&local.environment())
        .expect("protected")
        .expect("key");
    assert_eq!(restarted.expose_secret(), key.expose_secret());
    let card = Cards::connect_with(&local.client())
        .await
        .expect("handle connects")
        .get(CardSelector::named(
            CardKind::Prompt,
            "default".parse().expect("space"),
            "ask-prompt".parse().expect("name"),
        ))
        .await
        .expect("data retained across directories");
    assert_eq!(card.metadata.name.as_str(), "ask-prompt");
    assert!(!elsewhere.path().join(".wyrd").exists());
    let output = local.stop("-INT").await;
    assert!(output.contains("Gateway: skipped"), "{output}");

    // A listener override of the same deployment is accepted: the saved key
    // authenticates against the newly selected listener, which is resaved.
    local.bind = free_bind();
    let mut moved = local.command();
    moved
        .current_dir(elsewhere.path())
        .env_remove("WYRD_DATABASE_URL")
        .args(["server", "dev", "--non-interactive"]);
    local.start_with(moved).await;
    let global = GlobalConfig::load_in(&local.environment()).expect("saved config");
    assert_eq!(
        global.client.http_url,
        Some(format!("http://{}", local.bind))
    );
    assert_eq!(global.dev.bind.as_deref(), Some(local.bind.as_str()));
    assert_eq!(
        GlobalConfig::local_api_key(&local.environment())
            .expect("protected")
            .expect("key")
            .expose_secret(),
        key.expose_secret()
    );
    local.stop("-INT").await;
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

/// Provider key the quickstart form stores and the mock provider demands.
const LOCAL_PROVIDER_KEY: &str = "sk-local-dev-managed-secret";

/// Supplied sealing key, base64 of 32 bytes, that a deployment retains.
const SUPPLIED_SEALING_KEY: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";

/// A fresh `wyrd server dev` deployment seals a quickstart-form managed
/// secret on first boot with no extra key step, delivers it to a local
/// provider through the gateway, and keeps delivering it after a bare rerun
/// from another working directory. The sealing key is generated, or supplied
/// through `WYRD_SEALING_KEY_FILE` or `WYRD_SEALING_KEY_BASE64` on the first
/// run only and retained; a different supplied key is later refused without
/// replacing it. The tenant key is unchanged and the secret never reaches any
/// captured output.
///
/// # Panics
/// Panics when setup, the credential or deployment write, either gateway
/// call, the retained key, the refusal, or a leak check fails.
#[tokio::test(flavor = "multi_thread")]
async fn server_dev_keeps_the_gateway_managed_secret_key() {
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let provider = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header(
            "authorization",
            format!("Bearer {LOCAL_PROVIDER_KEY}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "chatcmpl-local",
            "object": "chat.completion",
            "created": 1,
            "model": "m",
            "choices": [{"index": 0, "message": {"role": "assistant", "content": "hi"}, "finish_reason": "stop", "logprobs": null}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
        })))
        .expect(6)
        .mount(&provider)
        .await;

    for source in ["generated", "file", "inline"] {
        let mut local = LocalDev::new().await;
        let mut first = local.command();
        let supplied = local.home.path().join("supplied.key");
        fs::write(&supplied, SUPPLIED_SEALING_KEY).expect("supplied key");
        fs::set_permissions(&supplied, fs::Permissions::from_mode(0o600)).expect("owner-only");
        match source {
            "file" => first.env("WYRD_SEALING_KEY_FILE", &supplied),
            "inline" => first.env("WYRD_SEALING_KEY_BASE64", SUPPLIED_SEALING_KEY),
            _ => &mut first,
        };
        first.args(["server", "dev", "--tenant", "local"]);
        local.start_with(first).await;
        let retained = local.home.path().join("wyrd/server/sealing.key");
        if source != "generated" {
            assert_eq!(
                fs::read_to_string(&retained).expect("retained key").trim(),
                SUPPLIED_SEALING_KEY,
                "{source}: the supplied key is retained"
            );
        }
        let mut printed = String::new();
        for (args, document) in [
            (
                ["gateway", "credential", "put", "--file", "-"],
                format!(
                    r#"{{"name":"local-key","provider":"local-mock","source":{{"managed_secret":{{"secret":"{LOCAL_PROVIDER_KEY}"}}}}}}"#
                ),
            ),
            (
                ["gateway", "deployment", "put", "--file", "-"],
                json!({
                    "name": "local-mock",
                    "adapter": {"openai_compatible": {"base_url": format!("{}/v1", provider.uri())}},
                    "auth": {"bearer": {"credential": "local-key"}},
                    "capabilities": ["chat_completions"],
                })
                .to_string(),
            ),
        ] {
            let output = local.stdin_command(&args, &document).await;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.status.success(), "{source} {args:?} failed: {text}");
            printed.push_str(&text);
        }
        let credentials = local.home.path().join("wyrd/credentials.toml");
        let saved = fs::read(&credentials).expect("saved credentials");
        let elsewhere = local.home.path().join("elsewhere");
        fs::create_dir_all(&elsewhere).expect("other working directory");

        for run in ["first run", "rerun"] {
            if run == "rerun" {
                let mut bare = local.command();
                bare.current_dir(&elsewhere)
                    .args(["server", "dev", "--non-interactive"]);
                local.start_with(bare).await;
            }
            let token = local
                .client()
                .access_token()
                .await
                .expect("saved key exchanges");
            let answer = reqwest::Client::new()
                .post(format!("http://{}/v1/chat/completions", local.bind))
                .bearer_auth(token.expose())
                .json(&json!({
                    "model": "local-mock/m",
                    "messages": [{"role": "user", "content": "hi"}],
                }))
                .send()
                .await
                .expect("gateway call sends");
            let status = answer.status();
            let body = answer.text().await.expect("gateway answer");
            assert_eq!(status.as_u16(), 200, "{source} {run}: {body}");
            assert!(body.contains("chatcmpl-local"), "{source} {run}: {body}");
            printed.push_str(&body);
            printed.push_str(&local.stop("-INT").await);
        }
        assert_eq!(fs::read(&credentials).expect("credentials"), saved);

        let kept = fs::read(&retained).expect("retained key");
        let other = local
            .command()
            .env(
                "WYRD_SEALING_KEY_BASE64",
                "CAgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg=",
            )
            .args(["server", "dev", "--non-interactive"])
            .output()
            .await
            .expect("different key");
        let refused = String::from_utf8_lossy(&other.stderr);
        assert!(!other.status.success(), "{source}: {refused}");
        assert!(
            refused.contains("differs from the retained"),
            "{source}: {refused}"
        );
        assert_eq!(fs::read(&retained).expect("retained key"), kept);
        assert!(
            !printed.contains(LOCAL_PROVIDER_KEY) && !printed.contains(SUPPLIED_SEALING_KEY),
            "{source}: no provider secret or sealing key reaches CLI, server, or gateway output"
        );
    }
}
