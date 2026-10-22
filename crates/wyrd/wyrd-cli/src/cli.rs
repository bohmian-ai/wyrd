//! Top-level `clap` command tree.

use clap::{Parser, Subcommand};

use crate::auth::AuthCommand;
use crate::card::{DeclareArgs, DeleteArgs, GetArgs, LatestArgs, ListArgs, PullArgs};
use crate::gateway::GatewayCommand;
use crate::init::InitArgs;
use crate::mcp::McpCommand;
use crate::operator_connection::OperatorConnectionCommand;
use crate::platform::PlatformCommand;
use crate::principal::PrincipalCommand;
use crate::query::QueryCommand;
use crate::server::ServerCommand;

/// Wyrd command-line interface.
#[derive(Debug, Parser)]
#[command(name = "wyrd", version, about, propagate_version = true)]
pub struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Command,
}

/// Dispatches the parsed client command through its single owning CLI.
impl Cli {
    /// Dispatch the selected client command through its owning CLI capability.
    ///
    /// The method consumes the parser result so each subcommand receives its
    /// owned arguments. It is async because command handlers perform client
    /// HTTP and local runtime IO.
    ///
    /// # Errors
    ///
    /// Returns [`crate::error::CliBoundaryError`] when parsing-compatible
    /// command inputs fail local validation or a client/server operation fails.
    /// Query commands keep their typed Bifrost error; every other command keeps
    /// the derive-catalogued local CLI error.
    pub async fn dispatch(self) -> Result<std::process::ExitCode, crate::error::CliBoundaryError> {
        match self.command {
            Command::Declare(args) => crate::card::dispatch_declare(args)
                .await
                .map_err(Into::into),
            Command::Get(args) => crate::card::dispatch_get(args).await.map_err(Into::into),
            Command::Pull(args) => crate::card::dispatch_pull(args).await.map_err(Into::into),
            Command::Latest(args) => crate::card::dispatch_latest(args).await.map_err(Into::into),
            Command::List(args) => crate::card::dispatch_list(args).await.map_err(Into::into),
            Command::Delete(args) => crate::card::dispatch_delete(args).await.map_err(Into::into),
            Command::Auth(command) => crate::auth::dispatch(command).await.map_err(Into::into),
            Command::Platform(command) => {
                crate::platform::dispatch(command).await.map_err(Into::into)
            }
            Command::OperatorConnection(command) => crate::operator_connection::dispatch(command)
                .await
                .map_err(Into::into),
            Command::Principal(command) => crate::principal::dispatch(command)
                .await
                .map_err(Into::into),
            Command::Query(command) => crate::query::dispatch(command).await,
            Command::Gateway(command) => command.dispatch().await,
            Command::Init(args) => args.run().await.map_err(Into::into),
            Command::Server(command) => command.dispatch().await.map_err(Into::into),
            Command::Mcp(command) => command.dispatch().await,
        }
    }
}

/// Top-level CLI verbs.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Declare a local Card tree and run its artifact lifecycle; --check
    /// validates the local files without contacting a server.
    Declare(DeclareArgs),
    /// Read one Card envelope without downloading artifacts.
    Get(GetArgs),
    /// Pull a selected Card and its reachable graph into --output-dir;
    /// complete artifact downloads are the default and --metadata-only writes
    /// an inspectable, non-runnable bundle.
    Pull(PullArgs),
    /// Resolve the latest active card version by name.
    Latest(LatestArgs),
    /// List card summaries with typed server-side filters.
    List(ListArgs),
    /// Soft-delete one exact card.
    Delete(DeleteArgs),
    /// Authenticate with a Wyrd server (login, refresh, issue-key).
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Administer the platform control plane (credentials).
    #[command(subcommand)]
    Platform(PlatformCommand),
    /// Manage the tenant's encrypted Operator provider connections.
    #[command(subcommand)]
    OperatorConnection(OperatorConnectionCommand),
    /// Manage Wyrd principals (revoke).
    #[command(subcommand)]
    Principal(PrincipalCommand),
    /// Run a terminal-safe streaming Oracle query.
    Query(QueryCommand),
    /// Administer tenant gateway credentials, deployments, and policies.
    Gateway(GatewayCommand),
    /// Migrate, start, and configure a local Wyrd server, then hand off the
    /// guided setup prompt; stays in the foreground until interrupted.
    /// PostgreSQL is supplied externally. `wyrd init guided` also starts the
    /// selected coding agent on the prompt. --print-prompt only prints the
    /// setup prompt.
    Init(InitArgs),
    /// Install and manage the official Wyrd server for this host.
    #[command(subcommand)]
    Server(ServerCommand),
    /// Connect MCP hosts to Wyrd and serve Wyrd's tools to them.
    #[command(subcommand)]
    Mcp(McpCommand),
}

/// Structural checks over the assembled command tree.
#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use super::Cli;

    /// The whole command tree is internally consistent.
    ///
    /// `propagate_version` pushes `--version` into every subcommand, so a
    /// subcommand that declares its own `--version` makes clap panic when that
    /// subcommand is parsed — not when the binary starts. Per-command unit tests
    /// build a bare wrapper without the propagated flag and cannot see it, so
    /// this asserts the real tree the binary ships.
    #[test]
    fn the_shipped_command_tree_is_consistent() {
        Cli::command().debug_assert();
    }

    /// Environment variables that carry a credential. Clap copies an `env`
    /// fallback into the parsed field, so none may back an argument.
    const SECRET_ENVS: [&str; 5] = [
        "WYRD_ACCESS_TOKEN",
        "WYRD_API_KEY",
        "WYRD_REFRESH_TOKEN",
        "WYRD_ISSUER_CLIENT_SECRET",
        "WYRD_PLATFORM_CREDENTIAL",
    ];

    /// Collect every argument that could carry a secret, with its command path.
    ///
    /// A secret-shaped long name is one naming a token, secret, password, API
    /// key, or credential; a path to a secret file (`-file`) or an identifier
    /// (`-id`) names no secret itself.
    fn secret_arguments(command: &clap::Command, path: &str, found: &mut Vec<String>) {
        for argument in command.get_arguments() {
            let long = argument.get_long().unwrap_or_default();
            let secret_named = ["token", "secret", "password", "api-key", "credential"]
                .iter()
                .any(|word| long.contains(word))
                && !long.ends_with("-file")
                && !long.ends_with("-id");
            let secret_env = argument
                .get_env()
                .and_then(|env| env.to_str())
                .is_some_and(|env| SECRET_ENVS.contains(&env));
            if secret_named || secret_env {
                found.push(format!("{path} --{long}"));
            }
        }
        for subcommand in command.get_subcommands() {
            secret_arguments(
                subcommand,
                &format!("{path} {}", subcommand.get_name()),
                found,
            );
        }
    }

    /// No shipped command accepts a secret through argv or a clap `env`
    /// fallback, so none can reach shell history, the process table, or the
    /// derived `Debug` of parsed arguments.
    ///
    /// # Panics
    /// Panics when any command in the tree declares a secret-valued argument.
    #[test]
    fn no_shipped_command_takes_a_secret_argument() {
        let mut found = Vec::new();
        secret_arguments(&Cli::command(), "wyrd", &mut found);
        assert!(found.is_empty(), "secret-valued arguments: {found:?}");
    }

    /// Every former secret spelling is refused by the real root parser, and
    /// the refusal never echoes the value supplied.
    ///
    /// # Panics
    /// Panics when a former secret option parses, is refused as anything other
    /// than an unknown argument, or its refusal message contains the secret.
    #[test]
    fn the_root_parser_refuses_every_former_secret_option_without_echo() {
        let secret = "wyrd-cli-sentinel-secret";
        let server = ["--server", "https://wyrd.example"];
        let cases: [&[&str]; 15] = [
            &["operator-connection", "create", "--bot-token"],
            &["operator-connection", "create", "--integration-key"],
            &[
                "operator-connection",
                "update",
                "--connection-id",
                "0190d6a0-0000-7000-8000-000000000000",
                "--token",
            ],
            &["operator-connection", "create", "--password"],
            &["auth", "issue-key", "--token"],
            &["auth", "trusted-issuer", "add", "--token"],
            &["auth", "trusted-issuer", "add", "--client-secret"],
            &["auth", "trusted-issuer", "list", "--token"],
            &["auth", "trusted-issuer", "rm", "--token"],
            &["auth", "workload-binding", "add", "--token"],
            &["auth", "workload-binding", "list", "--token"],
            &["auth", "workload-binding", "rm", "--token"],
            &["auth", "refresh", "--refresh-token"],
            &["principal", "revoke", "--token"],
            &["query", "--token"],
        ];
        for case in cases {
            let (option, command) = case.split_last().expect("every case names an option");
            for argv in [
                [&["wyrd"], command, &server, &[*option, secret]].concat(),
                [
                    &["wyrd"],
                    command,
                    &server,
                    &[format!("{option}={secret}").as_str()],
                ]
                .concat(),
            ] {
                let refused =
                    Cli::try_parse_from(&argv).expect_err("a former secret option is not accepted");
                assert_eq!(
                    refused.kind(),
                    clap::error::ErrorKind::UnknownArgument,
                    "{argv:?}"
                );
                assert!(
                    !refused.to_string().contains(secret),
                    "{argv:?} echoed the secret: {refused}"
                );
            }
        }
    }
}
