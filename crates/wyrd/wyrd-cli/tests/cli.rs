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

/// `wyrd init --print-prompt` prints only this release's setup prompt and
/// touches nothing: an unreadable saved configuration is never read, the
/// selected server and database listener see no connection, and neither the
/// configuration nor home directory changes. Setup options conflict with it,
/// help names it, and the removed `server dev` spelling is unknown.
///
/// # Panics
/// Panics when printing differs from the shared prompt, has side effects, or
/// accepts a setup option, or when the removed command still parses.
#[test]
fn guided_init_print_prompt_is_offline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener binds");
    listener
        .set_nonblocking(true)
        .expect("listener becomes non-blocking");
    let address = listener.local_addr().expect("listener address");
    let home = tempfile::tempdir().expect("isolated home");
    let config = home.path().join("wyrd");
    std::fs::create_dir(&config).expect("config directory");
    std::fs::write(config.join("config.toml"), "not = [valid").expect("unreadable config");
    let server = format!("http://{address}");
    let database = format!("postgres://wyrd@{address}/wyrd");
    let environment = [
        ("HOME", home.path().to_str().expect("UTF-8 home")),
        ("WYRD_CONFIG_HOME", config.to_str().expect("UTF-8 config")),
        ("WYRD_SERVER_URL", server.as_str()),
        ("WYRD_DATABASE_URL", database.as_str()),
    ];
    let listing = || {
        let mut entries = walk(home.path());
        entries.sort();
        entries
    };
    let before = listing();

    let printed = principal_journey::run_cli_with_env(&["init", "--print-prompt"], &environment);
    assert!(printed.status.success(), "{printed:?}");
    assert!(printed.stderr.is_empty(), "{printed:?}");
    assert_eq!(
        String::from_utf8(printed.stdout).expect("UTF-8 prompt"),
        wyrd_spec::mcp::setup_prompt(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(listing(), before, "printing writes nothing");

    for conflict in [
        &["init", "--print-prompt", "--format", "json"][..],
        &["init", "--print-prompt", "--format", "text"][..],
        &["init", "--print-prompt", "--tenant", "local"][..],
        &["init", "--print-prompt", "--non-interactive"][..],
        &["init", "--print-prompt", "--host", "codex"][..],
        &["server", "dev", "--tenant", "local"][..],
        &["server", "dev", "--print-prompt"][..],
    ] {
        let refused = principal_journey::run_cli_with_env(conflict, &environment);
        assert_eq!(refused.status.code(), Some(64), "{conflict:?}: {refused:?}");
        assert!(refused.stdout.is_empty(), "{conflict:?}");
    }
    assert_eq!(listing(), before, "refusals write nothing");
    assert_eq!(
        listener.accept().map(|_| ()).map_err(|error| error.kind()),
        Err(std::io::ErrorKind::WouldBlock),
        "no server or database connection"
    );

    let help = principal_journey::run_cli_with_env(&["init", "--help"], &environment);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--print-prompt"));
    let root = principal_journey::run_cli_with_env(&["--help"], &environment);
    let root = String::from_utf8(root.stdout).expect("UTF-8 help");
    assert!(
        root.lines()
            .any(|line| line.trim_start().starts_with("init "))
    );
    let server = principal_journey::run_cli_with_env(&["server", "--help"], &environment);
    let server = String::from_utf8(server.stdout).expect("UTF-8 help");
    assert!(
        !server
            .lines()
            .any(|line| line.trim_start().starts_with("dev")),
        "{server}"
    );
}

/// Every path beneath `root`, relative to it, with its file contents.
fn walk(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).expect("readable directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            found.push((path.clone(), Vec::new()));
            found.extend(walk(&path));
        } else {
            found.push((path.clone(), std::fs::read(&path).expect("readable file")));
        }
    }
    found
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
