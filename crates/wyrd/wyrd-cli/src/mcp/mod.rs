//! `wyrd mcp`: connect local MCP hosts to a Wyrd server.
//!
//! A host launches `wyrd mcp proxy`, which serves Wyrd's own `/mcp` tools over
//! stdio and authenticates every upstream request through the shared
//! [`wyrd_client`] credential chain. `wyrd mcp install` writes only that launch
//! command into the selected hosts' configuration, so no host file ever holds
//! a Wyrd credential.

mod hosts;
mod proxy;

use std::io::IsTerminal as _;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use wyrd_client::transport::HttpConfig;

pub use hosts::McpHost;

use crate::error::{CliBoundaryError, WyrdCliError};
use hosts::{HostInstaller, prompt_selection};

/// `wyrd mcp` subcommands.
#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Connect detected MCP hosts (Codex, Claude Code, Copilot CLI, VS Code)
    /// to Wyrd. Interactive runs offer a multi-select; scripts pass --host.
    Install(InstallArgs),
    /// Serve Wyrd's MCP tools over stdio for a host, authenticating with the
    /// ordinary Wyrd client credentials. Hosts launch this; `install` writes it.
    Proxy(ProxyArgs),
}

/// Arguments for `wyrd mcp install`.
#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Host to connect; repeat for several. Required when stdin is not a
    /// terminal, so a script never configures a host implicitly.
    #[arg(long = "host", value_enum)]
    hosts: Vec<McpHost>,
    /// Wyrd server the host connects to, retained in the host's launch
    /// command. Omitted, the host follows the global client endpoint.
    #[arg(long)]
    server: Option<String>,
}

/// Arguments for `wyrd mcp proxy`.
#[derive(Debug, Args)]
pub struct ProxyArgs {
    /// Wyrd server to proxy to. Omitted, the global client endpoint is used.
    #[arg(long)]
    server: Option<String>,
}

impl McpCommand {
    /// Run the selected `wyrd mcp` subcommand.
    ///
    /// # Errors
    /// `install` returns [`WyrdCliError::ClientConfig`] for an invalid
    /// `--server`, [`WyrdCliError::McpHostSelection`] when no host was
    /// selected, and [`WyrdCliError::McpHostInstall`] when any selected host
    /// could not be connected. `proxy` returns the client's credential or
    /// transport error, or [`WyrdCliError::McpProxy`] when the MCP session
    /// cannot be established.
    pub async fn dispatch(self) -> Result<ExitCode, CliBoundaryError> {
        match self {
            Self::Install(args) => args.run().map_err(Into::into),
            Self::Proxy(args) => proxy::run(args.server.as_deref()).await,
        }
    }
}

impl InstallArgs {
    /// Select hosts and write the Wyrd proxy entry into each one's config.
    ///
    /// The explicit `--server` is validated with the shared client's endpoint
    /// rules before any host file is touched. Explicit `--host` values win;
    /// otherwise an interactive terminal is offered the detected hosts, and a
    /// non-interactive run fails without editing anything. Each selected host
    /// is installed independently and reported on its own line, so one
    /// host's failure never blocks or corrupts another's.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::ClientConfig`] for an invalid `--server`,
    /// [`WyrdCliError::McpHostSelection`] when no host was selected and
    /// selection was required, [`WyrdCliError::Io`] when the prompt cannot be
    /// read, and [`WyrdCliError::McpHostInstall`] naming every host that
    /// failed.
    fn run(self) -> Result<ExitCode, WyrdCliError> {
        if let Some(server) = &self.server {
            HttpConfig {
                base_url: server.clone(),
                ..HttpConfig::default()
            }
            .validate()
            .map_err(crate::client::map_client_error)?;
        }
        let installer = HostInstaller::from_process(self.server.as_deref());
        let detected: Vec<McpHost> = McpHost::ALL
            .into_iter()
            .filter(|host| installer.config_path(*host).is_some())
            .collect();
        let selected = if !self.hosts.is_empty() {
            self.hosts
        } else if std::io::stdin().is_terminal() {
            if detected.is_empty() {
                return Err(WyrdCliError::McpHostSelection {
                    detail: "no supported MCP host was detected".to_owned(),
                });
            }
            let selected = prompt_selection(
                &detected,
                &mut std::io::stdin().lock(),
                &mut std::io::stdout(),
            )
            .map_err(|source| WyrdCliError::Io { source })?;
            if selected.is_empty() {
                println!("No host selected; nothing changed.");
                return Ok(ExitCode::SUCCESS);
            }
            selected
        } else {
            let names: Vec<&str> = detected.iter().map(|host| host.name()).collect();
            return Err(WyrdCliError::McpHostSelection {
                detail: format!(
                    "no host selected; detected: {}",
                    if names.is_empty() {
                        "none".to_owned()
                    } else {
                        names.join(", ")
                    }
                ),
            });
        };

        let mut failed = Vec::new();
        for host in dedup(selected) {
            match installer.install(host) {
                Ok((change, path)) => println!("{}: {change} {}", host.name(), path.display()),
                Err(failure) => {
                    println!("{}: {failure}", host.name());
                    failed.push(host.name());
                }
            }
        }
        if failed.is_empty() {
            Ok(ExitCode::SUCCESS)
        } else {
            Err(WyrdCliError::McpHostInstall {
                hosts: failed.join(", "),
            })
        }
    }
}

/// Drop repeated selections while keeping the caller's order.
fn dedup(hosts: Vec<McpHost>) -> Vec<McpHost> {
    let mut unique = Vec::with_capacity(hosts.len());
    for host in hosts {
        if !unique.contains(&host) {
            unique.push(host);
        }
    }
    unique
}
