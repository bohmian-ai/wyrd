//! `wyrd mcp` journeys: a host process reaching Wyrd through the CLI proxy,
//! and host-configuration installation.
//!
//! The proxy journeys drive the compiled `wyrd mcp proxy` exactly as an MCP
//! host does — a child process speaking MCP over stdio — against a real
//! Postgres-backed server, so every claim about credentials, permissions, and
//! endpoints is observed through the shipped command rather than an
//! in-process fixture.

use std::path::Path;
use std::process::Stdio;

use rmcp::ServiceExt as _;
use rmcp::model::CallToolRequestParams;
use secrecy::ExposeSecret as _;
use serde_json::Value;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use wyrd_testing::WyrdTestServer;

/// Every ambient variable that could hand the proxy a credential or an
/// endpoint the journey did not choose.
const AMBIENT: [&str; 9] = [
    "WYRD_ACCESS_TOKEN",
    "WYRD_WORKLOAD_TOKEN",
    "WYRD_TENANT",
    "WYRD_API_KEY",
    "WYRD_PLATFORM_CREDENTIAL",
    "WYRD_REFRESH_TOKEN",
    "WYRD_ISSUER_CLIENT_SECRET",
    "WYRD_SERVER_URL",
    "WYRD_GRPC_URL",
];

/// Build the compiled `wyrd` command isolated to `home`.
///
/// `HOME` and `WYRD_CONFIG_HOME` both point into `home`, and every ambient
/// Wyrd credential and endpoint is cleared, so a journey passes only from the
/// sources it was given.
///
/// # Panics
/// Panics when the `wyrd` binary cannot be located.
pub(crate) fn wyrd(home: &Path) -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin("wyrd"));
    command
        .env("HOME", home)
        .env("WYRD_CONFIG_HOME", home.join("wyrd"))
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("CODEX_HOME")
        .env_remove("COPILOT_HOME");
    for variable in AMBIENT {
        command.env_remove(variable);
    }
    command
}

/// A running `wyrd mcp proxy` child, as an MCP host launches it.
pub(crate) struct Proxy {
    /// The proxy process; killed when the handle drops.
    child: Child,
}

impl Proxy {
    /// Launch `wyrd mcp proxy <arguments>` with `sources` added to the
    /// isolated environment of [`wyrd`].
    ///
    /// # Panics
    /// Panics when the process cannot be spawned.
    pub(crate) fn spawn(home: &Path, arguments: &[&str], sources: &[(&str, &str)]) -> Self {
        let child = wyrd(home)
            .args(["mcp", "proxy"])
            .args(arguments)
            .envs(sources.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("wyrd mcp proxy spawns");
        Self { child }
    }

    /// Hand the proxy's stdio to an MCP client as the host's transport.
    ///
    /// # Panics
    /// Panics when the pipes were already taken.
    pub(crate) fn stdio(&mut self) -> (ChildStdout, ChildStdin) {
        (
            self.child.stdout.take().expect("proxy stdout is piped"),
            self.child.stdin.take().expect("proxy stdin is piped"),
        )
    }

    /// Wait for the proxy to exit and return its exit code and stderr.
    ///
    /// # Panics
    /// Panics when the process cannot be awaited.
    pub(crate) async fn exit(mut self) -> (Option<i32>, String) {
        drop(self.child.stdin.take());
        let output = self
            .child
            .wait_with_output()
            .await
            .expect("proxy process is awaited");
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

/// Read the structured problem line the CLI prints on failure.
///
/// # Panics
/// Panics when `stderr` carries no `wyrd_cli_error` JSON line.
pub(crate) fn cli_problem(stderr: &str) -> Value {
    stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .find(|value| value["kind"] == "wyrd_cli_error")
        .unwrap_or_else(|| panic!("stderr carries a CLI problem: {stderr}"))
}

/// Connect a host to Wyrd through a fresh proxy and call one read tool.
///
/// The host uses the legacy `initialize` lifecycle most shipped hosts speak;
/// bridging it to Wyrd's session-free revision is the proxy's job. Returns
/// the advertised tool names and the `bifrost.list_tables` result.
///
/// # Panics
/// Panics when the host cannot initialize, list tools, or call the tool.
pub(crate) async fn list_tables_through_proxy(
    home: &Path,
    arguments: &[&str],
    sources: &[(&str, &str)],
) -> (Vec<String>, rmcp::model::CallToolResult) {
    let mut proxy = Proxy::spawn(home, arguments, sources);
    let host = ().serve(proxy.stdio()).await.unwrap_or_else(|error| {
        panic!("the host initializes through the proxy: {error}");
    });
    let tools = host
        .list_all_tools()
        .await
        .expect("the host lists Wyrd's tools")
        .into_iter()
        .map(|tool| tool.name.into_owned())
        .collect();
    let result = host
        .call_tool(CallToolRequestParams::new("bifrost.list_tables"))
        .await
        .expect("the host calls a Wyrd read tool");
    host.cancel().await.expect("the host disconnects");
    (tools, result)
}

/// The stable error the shared client reports for `api_key` against `base_url`.
///
/// The proxy owns no credential decision, so its refusal must be exactly
/// this one.
///
/// # Panics
/// Panics when the client cannot be built or the credential is accepted.
async fn shared_client_refusal(base_url: &str, api_key: &str) -> wyrd_spec::error::WyrdError {
    let config = wyrd_client::config::ClientConfig {
        http: wyrd_client::transport::HttpConfig {
            base_url: base_url.to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        ..wyrd_client::config::ClientConfig::default()
    };
    let auth = wyrd_client::auth::AuthMiddleware::new(
        &config,
        wyrd_client::transport::ResolvedCredential::ApiKey(api_key.to_owned().into()),
    )
    .expect("auth middleware builds");
    let http = wyrd_client::transport::HttpTransport::new(&config.http, std::sync::Arc::clone(&auth))
        .expect("HTTP transport builds");
    wyrd_client::WyrdClient::from_parts(auth, http, config.grpc)
        .access_token()
        .await
        .expect_err("the unknown key is refused")
}

/// A host launching `wyrd mcp proxy` discovers Wyrd's tools and calls a read
/// tool with the ordinary client credential chain, the server refuses an
/// under-privileged principal, and credential failures exit before any
/// protocol traffic without disclosing the secret.
///
/// # Panics
/// Panics when the server cannot start, the host cannot reach the catalog or
/// read tool, the denial is not the server's RBAC problem, or a credential
/// failure succeeds, lacks its stable code, or echoes the secret.
pub(crate) async fn mcp_proxy_discovers_and_reads_with_shared_auth() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let base_url = server.base_url().expect("bound server").to_owned();
    let home = tempfile::tempdir().expect("journey home");

    let reader = server
        .bootstrap_service("mcp-proxy-reader", &["admin"])
        .await
        .expect("reader bootstraps");
    let reader_key = reader
        .api_key()
        .expect("service carries a key")
        .expose_secret()
        .to_owned();
    let (tools, listed) = list_tables_through_proxy(
        home.path(),
        &["--server", &base_url],
        &[("WYRD_API_KEY", &reader_key)],
    )
    .await;
    assert!(
        tools.iter().any(|name| name == "bifrost.list_tables")
            && tools.iter().any(|name| name == "cards.get"),
        "the host sees the server's own catalog: {tools:?}"
    );
    assert_ne!(listed.is_error, Some(true), "the read tool succeeds");
    assert!(
        listed
            .structured_content
            .as_ref()
            .is_some_and(|content| content["tables"].is_array()),
        "the read tool returns the server's structured result: {listed:?}"
    );

    let denied = server
        .bootstrap_service("mcp-proxy-denied", &[])
        .await
        .expect("denied principal bootstraps");
    let denied_key = denied
        .api_key()
        .expect("service carries a key")
        .expose_secret()
        .to_owned();
    let (_, refused) = list_tables_through_proxy(
        home.path(),
        &["--server", &base_url],
        &[("WYRD_API_KEY", &denied_key)],
    )
    .await;
    assert_eq!(refused.is_error, Some(true), "the server refuses the call");
    assert_eq!(
        refused
            .structured_content
            .as_ref()
            .map(|content| content["code"].clone()),
        Some(Value::from("WYRD_PERMISSION_403_DENIED_RBAC")),
        "permission is decided by the server, not the proxy"
    );

    let (code, stderr) = Proxy::spawn(home.path(), &["--server", &base_url], &[])
        .exit()
        .await;
    assert_eq!(code, Some(77), "no credential exits as unauthenticated");
    assert_eq!(cli_problem(&stderr)["code"], "WYRD_CLI_401_NO_CREDENTIALS");

    let unknown_key =
        "wyrd_sk_4d5e1c3a9b7f4e2d8a6c0b1e2f3a4b5c_1a2b3c4d_9f8e7d6c5b4a39281706f5e4d3c2b1a0";
    let (code, stderr) = Proxy::spawn(
        home.path(),
        &["--server", &base_url],
        &[("WYRD_API_KEY", unknown_key)],
    )
    .exit()
    .await;
    assert_ne!(code, Some(0), "an unusable credential fails the proxy");
    let shared = shared_client_refusal(&base_url, unknown_key).await;
    let problem = cli_problem(&stderr);
    assert_eq!(
        (problem["code"].as_str(), problem["status"].as_u64()),
        (Some(shared.code()), Some(u64::from(shared.status()))),
        "the shared client's own credential refusal reaches the host log: {stderr}"
    );
    assert!(
        !stderr.contains(unknown_key),
        "the failure never echoes the credential: {stderr}"
    );

    server.shutdown().await.expect("test server shuts down");
}

/// The VS Code user-profile directory under `home` for this platform, as the
/// isolated [`wyrd`] environment resolves it.
fn vscode_profile(home: &Path) -> std::path::PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/Code/User")
    } else {
        home.join(".config/Code/User")
    }
}

/// Every supported host's configuration file under `home`.
struct HostFiles {
    /// Codex `config.toml`.
    codex: std::path::PathBuf,
    /// Claude Code `~/.claude.json`.
    claude: std::path::PathBuf,
    /// Copilot CLI `mcp-config.json`.
    copilot: std::path::PathBuf,
    /// VS Code user `mcp.json`.
    vscode: std::path::PathBuf,
}

impl HostFiles {
    /// Detect all four hosts under `home`, each with an unrelated entry.
    ///
    /// # Panics
    /// Panics when a fixture directory or file cannot be written.
    fn seed(home: &Path) -> Self {
        for dir in [
            home.join(".codex"),
            home.join(".claude"),
            home.join(".copilot"),
            vscode_profile(home),
        ] {
            std::fs::create_dir_all(dir).expect("host directory");
        }
        let files = Self {
            codex: home.join(".codex/config.toml"),
            claude: home.join(".claude.json"),
            copilot: home.join(".copilot/mcp-config.json"),
            vscode: vscode_profile(home).join("mcp.json"),
        };
        let write = |path: &Path, text: &str| std::fs::write(path, text).expect("host file");
        write(
            &files.codex,
            "# user comment\nmodel = \"o3\"\n\n[mcp_servers.other]\ncommand = \"other\"\n",
        );
        write(
            &files.claude,
            r#"{"numStartups":3,"mcpServers":{"other":{"type":"stdio","command":"other"}}}"#,
        );
        write(
            &files.copilot,
            r#"{"mcpServers":{"other":{"type":"local","command":"other","tools":["*"]}}}"#,
        );
        write(&files.vscode, r#"{"servers":{"other":{"command":"other"}}}"#);
        files
    }

    /// Current contents of every file, in a fixed order.
    ///
    /// # Panics
    /// Panics when a file cannot be read.
    fn snapshot(&self) -> [String; 4] {
        [&self.codex, &self.claude, &self.copilot, &self.vscode].map(|path| {
            std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{path:?}: {error}"))
        })
    }
}

/// Run `wyrd mcp install <arguments>` non-interactively in `home`, with a
/// credential in the environment that must never reach a host file.
///
/// # Panics
/// Panics when the process cannot run.
async fn install(home: &Path, arguments: &[&str]) -> (Option<i32>, String, String) {
    let output = wyrd(home)
        .args(["mcp", "install"])
        .args(arguments)
        .env("WYRD_API_KEY", SENTINEL_KEY)
        .stdin(Stdio::null())
        .output()
        .await
        .expect("wyrd mcp install runs");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A credential that would be caught if installation ever copied it.
const SENTINEL_KEY: &str =
    "wyrd_sk_00000000000000000000000000000000_00000000_sentinelsentinelsentinelsentinelsentinelsent";

/// Parse a JSON host file.
///
/// # Panics
/// Panics when the file is not JSON.
fn json_file(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("host file reads"))
        .expect("host file stays valid JSON")
}

/// `wyrd mcp install` changes only the hosts a caller names, writes each
/// host's native entry shape with no credential, preserves unrelated entries,
/// is repeatable, and reports conflicts, unwritable files, and undetected
/// hosts per host without touching them.
///
/// # Panics
/// Panics when any selection, edit, repeat, or per-host failure differs from
/// that contract.
pub(crate) async fn mcp_install_changes_only_selected_hosts() {
    let home = tempfile::tempdir().expect("journey home");
    let files = HostFiles::seed(home.path());
    let original = files.snapshot();
    let config_home = home.path().join("wyrd").display().to_string();

    let (code, _, stderr) = install(home.path(), &[]).await;
    assert_eq!(code, Some(64), "a script must name its hosts");
    let problem = cli_problem(&stderr);
    assert_eq!(problem["code"], "WYRD_CLI_400_MCP_HOST_SELECTION");
    assert!(
        problem["message"]
            .as_str()
            .is_some_and(|message| message.contains("codex, claude-code, copilot-cli, vscode")),
        "the refusal names the detected hosts: {problem}"
    );
    assert_eq!(files.snapshot(), original, "no host changes implicitly");

    let (code, stdout, stderr) =
        install(home.path(), &["--host", "codex", "--host", "claude-code"]).await;
    assert_eq!(code, Some(0), "selected hosts install: {stdout}{stderr}");
    assert!(stdout.contains("codex: added") && stdout.contains("claude-code: added"));
    let first = files.snapshot();
    assert_eq!(first[2], original[2], "Copilot CLI was not selected");
    assert_eq!(first[3], original[3], "VS Code was not selected");
    for text in &first {
        assert!(!text.contains(SENTINEL_KEY), "a host file holds no credential");
    }

    assert!(first[0].starts_with("# user comment\nmodel = \"o3\"\n"));
    let codex: Value = toml::from_str(&first[0]).expect("Codex config stays TOML");
    assert_eq!(codex["mcp_servers"]["other"]["command"], "other");
    let codex_entry = &codex["mcp_servers"]["wyrd"];
    assert_eq!(codex_entry["args"], serde_json::json!(["mcp", "proxy"]));
    assert_eq!(codex_entry["env"]["WYRD_CONFIG_HOME"], config_home.as_str());
    assert!(codex_entry["command"].as_str().is_some_and(|c| c.ends_with("wyrd")));
    assert!(codex_entry.get("type").is_none(), "Codex entries carry no type");

    let claude = json_file(&files.claude);
    assert_eq!(claude["numStartups"], 3);
    assert_eq!(claude["mcpServers"]["other"]["command"], "other");
    let claude_entry = &claude["mcpServers"]["wyrd"];
    assert_eq!(claude_entry["type"], "stdio");
    assert_eq!(claude_entry["args"], serde_json::json!(["mcp", "proxy"]));
    assert_eq!(claude_entry["env"]["WYRD_CONFIG_HOME"], config_home.as_str());

    let (code, stdout, _) =
        install(home.path(), &["--host", "codex", "--host", "claude-code"]).await;
    assert_eq!(code, Some(0));
    assert!(stdout.contains("codex: unchanged") && stdout.contains("claude-code: unchanged"));
    assert_eq!(files.snapshot(), first, "a repeat rewrites nothing");

    std::fs::write(
        &files.copilot,
        r#"{"mcpServers":{"wyrd":{"type":"local","command":"someone-else"}}}"#,
    )
    .expect("foreign entry");
    let foreign = files.snapshot()[2].clone();
    let (code, stdout, stderr) =
        install(home.path(), &["--host", "copilot-cli", "--host", "vscode"]).await;
    assert_eq!(code, Some(73), "a conflict fails the run");
    assert_eq!(cli_problem(&stderr)["code"], "WYRD_CLI_409_MCP_HOST_INSTALL");
    assert!(stdout.contains("copilot-cli: conflict"), "{stdout}");
    assert!(stdout.contains("vscode: added"), "{stdout}");
    assert_eq!(files.snapshot()[2], foreign, "the conflicting file is untouched");
    let vscode = json_file(&files.vscode);
    assert_eq!(vscode["servers"]["other"]["command"], "other");
    assert_eq!(vscode["servers"]["wyrd"]["type"], "stdio");
    assert_eq!(vscode["servers"]["wyrd"]["args"], serde_json::json!(["mcp", "proxy"]));

    std::fs::write(&files.copilot, &original[2]).expect("restore Copilot CLI file");
    let (code, _, _) = install(home.path(), &["--host", "copilot-cli"]).await;
    assert_eq!(code, Some(0));
    let copilot = json_file(&files.copilot);
    assert_eq!(copilot["mcpServers"]["other"]["command"], "other");
    let copilot_entry = &copilot["mcpServers"]["wyrd"];
    assert_eq!(copilot_entry["type"], "stdio");
    assert_eq!(copilot_entry["tools"], serde_json::json!(["*"]));
    assert_eq!(copilot_entry["args"], serde_json::json!(["mcp", "proxy"]));

    let before = files.snapshot();
    let codex_dir = home.path().join(".codex");
    let set_mode = |mode: u32| {
        std::fs::set_permissions(
            &codex_dir,
            std::os::unix::fs::PermissionsExt::from_mode(mode),
        )
        .expect("codex dir mode changes");
    };
    set_mode(0o555);
    let (code, stdout, _) = install(
        home.path(),
        &["--host", "codex", "--host", "claude-code", "--server", "https://wyrd.example"],
    )
    .await;
    set_mode(0o755);
    assert_eq!(code, Some(73), "an unwritable host fails the run: {stdout}");
    assert!(stdout.contains("codex: cannot write"), "{stdout}");
    assert!(stdout.contains("claude-code: updated"), "{stdout}");
    assert_eq!(files.snapshot()[0], before[0], "the unwritable file is intact");

    let empty = tempfile::tempdir().expect("hostless home");
    let (code, stdout, _) = install(empty.path(), &["--host", "vscode"]).await;
    assert_eq!(code, Some(73));
    assert!(stdout.contains("vscode: not detected"), "{stdout}");
    assert!(!vscode_profile(empty.path()).exists(), "nothing is created");
}

/// `wyrd mcp install --server` retains an external URL in the host's launch
/// command, and launching exactly that command reaches the external server
/// through the saved credential while the global endpoint — pointed at an
/// unreachable local default — is neither used nor rewritten.
///
/// # Panics
/// Panics when installation fails, the entry omits the URL or holds the
/// credential, the launched host cannot call a read tool, or `config.toml`
/// changes.
pub(crate) async fn mcp_external_server_preserves_global_endpoint() {
    let server = WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let base_url = server.base_url().expect("bound server").to_owned();
    let principal = server
        .bootstrap_service("mcp-external", &["admin"])
        .await
        .expect("principal bootstraps");
    let key = principal
        .api_key()
        .expect("service carries a key")
        .expose_secret()
        .to_owned();

    let home = tempfile::tempdir().expect("journey home");
    let config_dir = home.path().join("wyrd");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    let global = "[client]\nhttp_url = \"http://127.0.0.1:9\"\n";
    std::fs::write(config_dir.join("config.toml"), global).expect("global config");
    let credentials = config_dir.join("credentials.toml");
    std::fs::write(&credentials, format!("[default]\napi_key = \"{key}\"\n")).expect("credentials");
    let mut private = std::fs::metadata(&credentials).expect("credentials").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut private, 0o600);
    std::fs::set_permissions(&credentials, private).expect("credentials are private");
    std::fs::create_dir_all(home.path().join(".claude")).expect("Claude Code detected");

    let wyrd_dir = assert_cmd::cargo::cargo_bin("wyrd")
        .parent()
        .expect("binary directory")
        .to_owned();
    let output = wyrd(home.path())
        .args(["mcp", "install", "--host", "claude-code", "--server", &base_url])
        .env("PATH", &wyrd_dir)
        .stdin(Stdio::null())
        .output()
        .await
        .expect("install runs");
    assert!(
        output.status.success(),
        "install succeeds: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let host_file = std::fs::read_to_string(home.path().join(".claude.json")).expect("host file");
    assert!(!host_file.contains(&key), "the host file holds no credential");
    let entry = &serde_json::from_str::<Value>(&host_file).expect("JSON")["mcpServers"]["wyrd"];
    assert_eq!(
        entry["args"],
        serde_json::json!(["mcp", "proxy", "--server", base_url]),
        "the external URL is retained in the launch command"
    );
    let command = entry["command"].as_str().expect("command");
    assert_eq!(Path::new(command), wyrd_dir.join("wyrd"), "the resolved wyrd is recorded");

    let mut host_launch = Command::new(command);
    host_launch
        .args(
            entry["args"]
                .as_array()
                .expect("args")
                .iter()
                .filter_map(Value::as_str),
        )
        .env_clear()
        .env("HOME", home.path())
        .envs(
            entry["env"]
                .as_object()
                .expect("env")
                .iter()
                .filter_map(|(name, value)| Some((name, value.as_str()?))),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let mut child = host_launch.spawn().expect("the host launches its entry");
    let stdio = (
        child.stdout.take().expect("stdout"),
        child.stdin.take().expect("stdin"),
    );
    let host = ().serve(stdio).await.expect("the host initializes");
    let listed = host
        .call_tool(CallToolRequestParams::new("bifrost.list_tables"))
        .await
        .expect("the host calls a read tool");
    assert_ne!(listed.is_error, Some(true), "the external server answers");
    host.cancel().await.expect("the host disconnects");

    assert_eq!(
        std::fs::read_to_string(config_dir.join("config.toml")).expect("global config"),
        global,
        "the global endpoint is unchanged"
    );
    server.shutdown().await.expect("test server shuts down");
}
