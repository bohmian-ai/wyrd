//! Local startup through the installed CLI and real serving binary, isolated
//! from the user's credentials, database, working directory, and MCP hosts.

use std::fs;
use std::net::TcpListener;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
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
use wyrd_client::{Bifrost, GlobalConfig, WyrdClient};
use wyrd_dev_fixtures::pg::UnmigratedDatabase;
use wyrd_spec::envelope::CardKind;
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
    /// Isolated listener and public origin of the production UI's Node BFF.
    ui: String,
}

impl LocalDev {
    /// Create a deployment with no migrations, root, tenant, or saved key.
    /// The installer is already present, as on a later installed-CLI run,
    /// with the checkout's built production UI as its `ui/` payload;
    /// signed-download behavior has its own release-service journeys.
    ///
    /// # Panics
    /// Panics when the managed database, compiled binary, UI build, or fixture
    /// files cannot be prepared. Build wyrd-server and the UI through mise
    /// before this target.
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
        symlink(ui_build(), installed.join("ui")).expect("installed UI fixture");
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
            ui: free_bind(),
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
            .env("WYRD_UI_BIND", &self.ui)
            .env("WYRD_METRICS_BIND", "127.0.0.1:0");
        command
    }

    /// Start local setup and wait for its explicit client-ready message,
    /// which names the production UI's address.
    ///
    /// # Panics
    /// Panics when startup fails, hangs beyond its readiness budget, omits
    /// the UI, or puts credentials in stdout. An unsuccessful child is
    /// settled before panic.
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
        assert!(
            printed.contains(&format!("http://{}", self.ui)),
            "the ready message names the UI: {printed}"
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

    /// Interrupt only the CLI and prove it settles its owned server and UI,
    /// returning its captured stdout and stderr for further leak checks.
    ///
    /// # Panics
    /// Panics when interruption fails, the CLI reports a different status,
    /// credentials reach output, or the server or UI listener remains active.
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
        assert!(
            TcpListener::bind(&self.ui).is_ok(),
            "the UI's listener is closed after interrupting its CLI"
        );
        stdout + &stderr
    }

    /// Use the production UI at its own origin as a browser does: the
    /// authorization server's browser pages answer there under that origin,
    /// and API-key recovery sign-in with the saved key opens the tenant's page.
    ///
    /// # Panics
    /// Panics when `/auth` is not forwarded to the server under the UI's
    /// origin, the sign-in is refused or sets no session, or the signed-in
    /// page does not load.
    async fn sign_in_through_ui(&self) {
        let origin = format!("http://{}", self.ui);
        let browser = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("browser client");
        let device = browser
            .get(format!("{origin}/auth/device?tenant=local&user_code=ABCD-EFGH"))
            .send()
            .await
            .expect("device page at the UI origin");
        assert_eq!(device.status(), 200);
        assert!(
            device
                .text()
                .await
                .expect("device page")
                .contains("Approve a Wyrd CLI login"),
            "the server's device page answered"
        );
        // The server accepts only a form posted from the origin it derives,
        // so a code refusal rather than a cross-origin 403 proves the UI's
        // origin reached it.
        let unknown = browser
            .post(format!("{origin}/auth/device"))
            .header("origin", &origin)
            .form(&[
                ("tenant", "local"),
                ("user_code", "ABCD-EFGH"),
                ("decision", "approve"),
            ])
            .send()
            .await
            .expect("device approval at the UI origin");
        assert_eq!(unknown.status(), 400);
        let key = GlobalConfig::local_api_key(&self.environment())
            .expect("protected key")
            .expect("key saved");
        let signed_in = browser
            .post(format!("{origin}/t/local/login/api-key"))
            .header("origin", &origin)
            .header("accept", "text/html")
            .form(&[("apiKey", key.expose_secret())])
            .send()
            .await
            .expect("API-key sign-in");
        if signed_in.status() != 303 {
            panic!(
                "API-key sign-in answered {}: {}",
                signed_in.status(),
                signed_in.text().await.unwrap_or_default()
            );
        }
        assert_eq!(
            signed_in
                .headers()
                .get("location")
                .map(|value| value.as_bytes()),
            Some(b"/t/local".as_slice())
        );
        let session = signed_in
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|cookie| cookie.to_str().ok()?.split(';').next())
            .collect::<Vec<_>>()
            .join("; ");
        assert!(!session.is_empty(), "sign-in sets the session cookie");
        let page = browser
            .get(format!("{origin}/t/local"))
            .header("cookie", session)
            .send()
            .await
            .expect("signed-in tenant page");
        assert_eq!(page.status(), 200);
        assert!(
            !page
                .text()
                .await
                .expect("tenant page")
                .contains(key.expose_secret()),
            "the page carries no credential"
        );
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

/// The checkout's production UI build, installed as the release's `ui/`.
///
/// # Panics
/// Panics when the UI has not been built through mise.
fn ui_build() -> PathBuf {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wyrd-server/wyrd-ui");
    assert!(
        ui.join("build/index.js").is_file(),
        "build the production UI through mise first"
    );
    ui
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

/// First use migrates an empty database, saves a private key and both
/// endpoints, serves the production UI with its sign-in, and supports fresh
/// CLI writes and client reads outside the checkout. A stale saved gRPC destination is replaced by the selected
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
    local.start(&[]).await;
    local.sign_in_through_ui().await;
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
        .args(["apply", prompt.to_str().expect("UTF-8 prompt")])
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
    local.stop("-INT").await;
}

/// Reruns retain the active release and key; a failed one-time save does not
/// turn a retry into another issuance. Missing input, a non-loopback public
/// gRPC bind, a missing or incompatible system Node, a release without the
/// production UI, and an occupied UI listener are refused before setup, and
/// an explicit host selected on a later start is still connected.
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
    assert!(TcpListener::bind(&local.ui).is_ok());
    let tools = tempfile::tempdir().expect("PATH without Node");
    let refused_ui = |output: std::process::Output, expected: &str| {
        assert!(!output.status.success());
        let errors = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(errors.contains(expected), "{errors}");
        assert!(
            !String::from_utf8_lossy(&output.stdout).contains("Migrating"),
            "refused before migration"
        );
    };
    refused_ui(
        local
            .command()
            .env("PATH", tools.path())
            .args(["server", "dev", "--tenant", "local"])
            .output()
            .await
            .expect("missing Node"),
        "Node.js",
    );
    let old_node = tools.path().join("node");
    fs::write(&old_node, "#!/bin/sh\necho v18.20.0\n").expect("incompatible Node");
    fs::set_permissions(&old_node, fs::Permissions::from_mode(0o755)).expect("executable");
    refused_ui(
        local
            .command()
            .env("PATH", tools.path())
            .args(["server", "dev", "--tenant", "local"])
            .output()
            .await
            .expect("incompatible Node"),
        "Node.js",
    );
    let payload = local.home.path().join("wyrd/server/current/ui");
    fs::remove_file(&payload).expect("release without its UI");
    refused_ui(
        local
            .command()
            .args(["server", "dev", "--tenant", "local"])
            .output()
            .await
            .expect("missing UI payload"),
        "production UI",
    );
    symlink(ui_build(), &payload).expect("restore the UI payload");
    let occupied = TcpListener::bind(&local.ui).expect("occupy the UI listener");
    refused_ui(
        local
            .command()
            .args(["server", "dev", "--tenant", "local"])
            .output()
            .await
            .expect("occupied UI listener"),
        "WYRD_UI_BIND",
    );
    drop(occupied);
    assert!(!local.home.path().join("wyrd/credentials.toml").exists());
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

/// Provider key the quickstart form stores and the mock provider demands.
const LOCAL_PROVIDER_KEY: &str = "sk-local-dev-managed-secret";

/// A fresh `wyrd server dev` deployment seals a quickstart-form managed
/// secret on first boot with no extra key step, delivers it to a local
/// provider through the gateway, and keeps delivering it after a rerun. The
/// secret never reaches any captured output.
///
/// # Panics
/// Panics when setup, the credential or deployment write, either gateway
/// call, or a leak check fails.
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
        .expect(2)
        .mount(&provider)
        .await;

    let mut local = LocalDev::new().await;
    local.start(&[]).await;
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
        assert!(output.status.success(), "{args:?} failed: {text}");
        printed.push_str(&text);
    }

    for run in ["first run", "rerun"] {
        if run == "rerun" {
            local.start(&[]).await;
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
        assert_eq!(status.as_u16(), 200, "{run}: {body}");
        assert!(body.contains("chatcmpl-local"), "{run}: {body}");
        printed.push_str(&body);
        printed.push_str(&local.stop("-INT").await);
    }
    assert!(
        !printed.contains(LOCAL_PROVIDER_KEY),
        "the provider secret never reaches CLI, server, or gateway output"
    );
}
