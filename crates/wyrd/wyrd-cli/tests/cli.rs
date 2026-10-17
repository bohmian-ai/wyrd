use clap::Parser as _;

#[path = "auth_issue_key_journey.rs"]
mod auth_issue_key_journey;
#[path = "card_lifecycle.rs"]
mod card_lifecycle;
/// Provider-backed `wyrd auth login`, `status`, and `logout` journey.
#[path = "cli_login_journey.rs"]
mod cli_login_journey;
#[path = "gateway_server_journey.rs"]
mod gateway_server_journey;
#[path = "loader.rs"]
mod loader;
/// `wyrd mcp proxy` and `wyrd mcp install` journeys.
#[path = "mcp_journey.rs"]
mod mcp_journey;
#[path = "operator_journey.rs"]
mod operator_journey;
#[path = "principal_journey.rs"]
mod principal_journey;
#[path = "query_server_journey.rs"]
mod query_server_journey;
#[path = "secret_sources.rs"]
mod secret_sources;
/// `wyrd server install` against a mock release service.
#[path = "server_install.rs"]
mod server_install;

/// Local bootstrap is an installed CLI capability rather than a checkout script.
///
/// # Panics
/// Panics when the installed CLI cannot select local startup.
#[test]
fn server_dev_selects_installed_local_startup() {
    wyrd_cli::Cli::try_parse_from(["wyrd", "server", "dev", "--tenant", "local"])
        .expect("local setup is exposed by the installed CLI");
}

/// PostgreSQL-backed installed local-server lifecycle journeys.
mod pg_tests;

/// Root of the provider-backed CLI login journey, so its exact name
/// `cli_device_login_journey` selects it; only the Keycloak identity lane
/// runs it.
///
/// # Panics
/// Panics when any step of [`cli_login_journey::cli_device_login_journey`]
/// differs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires the Keycloak identity lane; run via `mise run test:identity:journey`"]
async fn cli_device_login_journey() {
    cli_login_journey::cli_device_login_journey().await;
}

/// Root of the authenticated MCP proxy journey, so its exact name selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_proxy_discovers_and_reads_with_shared_auth`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_proxy_discovers_and_reads_with_shared_auth() {
    mcp_journey::mcp_proxy_discovers_and_reads_with_shared_auth().await;
}

/// Root of the selective host-installation journey, so its exact name
/// selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_install_changes_only_selected_hosts`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_install_changes_only_selected_hosts() {
    mcp_journey::mcp_install_changes_only_selected_hosts().await;
}

/// Root of the external-server MCP journey, so its exact name selects it.
///
/// # Panics
/// Panics when any step of
/// [`mcp_journey::mcp_external_server_preserves_global_endpoint`] differs.
#[tokio::test(flavor = "multi_thread")]
async fn mcp_external_server_preserves_global_endpoint() {
    mcp_journey::mcp_external_server_preserves_global_endpoint().await;
}

/// The retired `wyrd eval` command tree is gone: the shipped binary refuses
/// it with clap's ordinary unknown-subcommand usage error, omits it from help,
/// and never contacts the server it was pointed at.
///
/// A loopback listener stands in for the server; it must see no connection.
///
/// # Panics
/// Panics when the binary accepts either command, exits with anything other
/// than the usage code, lists `eval` in help, or connects to the listener.
#[test]
fn retired_command_is_unknown() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
    listener
        .set_nonblocking(true)
        .expect("listener becomes non-blocking");
    let server = format!(
        "http://{}",
        listener.local_addr().expect("listener address")
    );
    for command in [&["eval"][..], &["eval", "run"][..]] {
        let argv = [command, &["--server", &server]].concat();
        let output = principal_journey::run_cli_with_env(&argv, &[]);
        assert_eq!(output.status.code(), Some(64), "{argv:?} is a usage error");
        let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
        assert!(
            stderr.contains("unrecognized subcommand 'eval'"),
            "{argv:?} gets clap's unknown-subcommand refusal: {stderr}"
        );
    }
    let help = principal_journey::run_cli_with_env(&["--help"], &[]);
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("help is UTF-8");
    assert!(
        !help
            .lines()
            .any(|line| line.trim_start().starts_with("eval")),
        "help lists no eval command: {help}"
    );
    assert_eq!(
        listener.accept().map(|_| ()).map_err(|error| error.kind()),
        Err(std::io::ErrorKind::WouldBlock),
        "the refused command never connects"
    );
}

/// The compiled `wyrd query` refuses the retired Eval tables exactly as it
/// refuses a namespace that never existed, printing no rows, while the
/// caller's current table still answers.
///
/// # Panics
/// Panics when setup fails, a retired query succeeds or prints rows, its
/// problem code differs from the never-existing namespace's, or the current
/// table query fails.
#[tokio::test(flavor = "multi_thread")]
async fn retired_tables_are_unknown() {
    let server = wyrd_testing::WyrdTestServer::start_bound()
        .await
        .expect("test server starts");
    let fixture = wyrd_testing::bifrost::seed_query_fixture(&server, "cli-retired")
        .await
        .expect("query fixture seeds");
    let query = |sql: &str| {
        principal_journey::run_cli_with_credential(
            &["query", "--server", &fixture.endpoint, "--sql", sql],
            "WYRD_ACCESS_TOKEN",
            &fixture.token,
        )
    };
    for name in ["result_items", "observations"] {
        let codes = ["vala.eval", "vala.absent"].map(|namespace| {
            let output = query(&format!("SELECT * FROM {namespace}.{name}"));
            assert!(!output.status.success(), "{namespace}.{name} is refused");
            assert!(
                output.stdout.is_empty(),
                "{namespace}.{name} prints no rows"
            );
            String::from_utf8(output.stderr)
                .expect("stderr is UTF-8")
                .lines()
                .find_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .expect("the refusal is a structured problem")["code"]
                .clone()
        });
        assert_eq!(codes[0], codes[1], "{name}");
    }
    assert!(
        query(&format!("SELECT * FROM {}", fixture.table))
            .status
            .success(),
        "the current table answers"
    );
    server.shutdown().await.expect("test server shuts down");
}
