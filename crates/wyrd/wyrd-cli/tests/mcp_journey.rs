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
    assert_eq!(
        cli_problem(&stderr)["status"],
        401,
        "the server's own credential refusal reaches the host log: {stderr}"
    );
    assert!(
        !stderr.contains(unknown_key),
        "the failure never echoes the credential: {stderr}"
    );

    server.shutdown().await.expect("test server shuts down");
}
