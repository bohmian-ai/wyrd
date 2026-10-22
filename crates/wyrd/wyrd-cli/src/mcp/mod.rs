//! `wyrd mcp`: connect local MCP hosts to a Wyrd server.
//!
//! A host launches `wyrd mcp proxy`, which serves Wyrd's own `/mcp` tools over
//! stdio and authenticates every upstream request through the shared
//! [`wyrd_client`] credential chain. `wyrd mcp install` writes only that launch
//! command into the selected hosts' configuration, so no host file ever holds
//! a Wyrd credential.

mod hosts;
mod proxy;

use std::io::{self, IsTerminal as _};
use std::process::ExitCode;

use clap::{Args, Subcommand, ValueEnum as _};
use serde::Serialize;
use wyrd_client::transport::HttpConfig;
use wyrd_spec::error::WyrdError;

#[cfg(unix)]
pub(crate) use hosts::prompt_agent;
pub(crate) use hosts::selected;
pub use hosts::{McpHost, McpHostReport, McpHostStatus};

use crate::card::{OutputFormat, print_json};
use crate::client::map_client_error;
use crate::error::{CliBoundaryError, WyrdCliError};
use hosts::{HostInstaller, prompt_selection};

/// `wyrd mcp` subcommands.
#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Connect detected MCP hosts (Codex, Claude Code, Copilot CLI, VS Code,
    /// Cursor, Pi, Hermes Agent) to Wyrd. Interactive runs offer a
    /// multi-select; scripts pass --host.
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
    /// Output encoding; `json` prints one per-host installation report and never
    /// prompts, so it requires `--host`.
    #[arg(long, default_value_t)]
    format: OutputFormat,
}

/// Arguments for `wyrd mcp proxy`.
#[derive(Debug, Args)]
pub struct ProxyArgs {
    /// Wyrd server to proxy to. Omitted, the global client endpoint is used.
    #[arg(long)]
    server: Option<String>,
}

/// What `wyrd mcp install` did to each selected host, in selection order.
#[derive(Debug, Serialize)]
pub struct McpInstallReport {
    /// One outcome per distinct selected host.
    pub hosts: Vec<McpHostReport>,
}

/// Write the Wyrd proxy entry into each of the named hosts' configurations.
///
/// The in-process form of `wyrd mcp install --host ...`. `hosts` holds the
/// `--host` values (`codex`, `claude-code`, `copilot-cli`, `vscode`,
/// `cursor`, `pi`, `hermes`). `server`
/// is validated with the shared client's endpoint rules before any host file
/// is touched, then retained verbatim as the proxy's `--server`. Hosts are
/// located from the process environment (`HOME`, `CODEX_HOME`,
/// `COPILOT_HOME`, `XDG_CONFIG_HOME`, `PI_CODING_AGENT_DIR`, `HERMES_HOME`),
/// and repeated hosts are installed
/// once. Each host is installed independently: a host that cannot be
/// connected is reported with its status and detail, leaves its file as it
/// was, and never blocks another host. No credential is written.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an unknown host name (`details`
/// names the `hosts` field) or an empty `hosts`, whose message names the
/// detected hosts, and `WYRD_CLIENT_400_CONFIG_INVALID` for an invalid
/// `server`.
pub fn install(
    hosts: &[impl AsRef<str>],
    server: Option<&str>,
) -> Result<McpInstallReport, WyrdError> {
    let hosts = hosts
        .iter()
        .map(|name| {
            let name = name.as_ref();
            McpHost::from_str(name, false).map_err(|_| WyrdCliError::InvalidArgument {
                field: "hosts".to_owned(),
                value: name.to_owned(),
                expected: "codex, claude-code, copilot-cli, vscode, cursor, pi, or hermes"
                    .to_owned(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(installed(&hosts, server)?)
}

/// [`install`] with the CLI's own error type, shared by the executable.
///
/// # Errors
/// As [`install`].
fn installed(hosts: &[McpHost], server: Option<&str>) -> Result<McpInstallReport, WyrdCliError> {
    if let Some(server) = server {
        HttpConfig {
            base_url: server.to_owned(),
            ..HttpConfig::default()
        }
        .validate()
        .map_err(map_client_error)?;
    }
    let installer = HostInstaller::from_process(server);
    if hosts.is_empty() {
        let detected: Vec<&str> = installer
            .detected()
            .iter()
            .map(|host| host.name())
            .collect();
        return Err(WyrdCliError::McpHostSelection {
            detail: format!(
                "no host selected; detected: {}",
                if detected.is_empty() {
                    "none".to_owned()
                } else {
                    detected.join(", ")
                }
            ),
        });
    }
    let mut unique: Vec<McpHost> = Vec::with_capacity(hosts.len());
    for host in hosts {
        if !unique.contains(host) {
            unique.push(*host);
        }
    }
    Ok(McpInstallReport {
        hosts: unique
            .into_iter()
            .map(|host| installer.install(host))
            .collect(),
    })
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
    /// Select hosts, install them, and print one line per host or, with
    /// `--format json`, the whole report.
    ///
    /// Explicit `--host` values win; otherwise an interactive text-mode
    /// terminal is offered the detected hosts, and a JSON or non-interactive
    /// run fails through [`install`] without editing anything.
    ///
    /// # Errors
    /// Returns [`install`]'s errors, [`WyrdCliError::McpHostSelection`] when
    /// no host is detected for the prompt, [`WyrdCliError::Io`] when the
    /// prompt cannot be read, and [`WyrdCliError::McpHostInstall`] naming
    /// every host that was not connected and carrying the whole report.
    fn run(self) -> Result<ExitCode, WyrdCliError> {
        let mut hosts = self.hosts;
        if hosts.is_empty() && self.format == OutputFormat::Text && io::stdin().is_terminal() {
            let detected = HostInstaller::from_process(None).detected();
            if detected.is_empty() {
                return Err(WyrdCliError::McpHostSelection {
                    detail: "no supported MCP host was detected".to_owned(),
                });
            }
            hosts = prompt_selection(&detected, &mut io::stdin().lock(), &mut io::stdout())
                .map_err(|source| WyrdCliError::Io { source })?;
            if hosts.is_empty() {
                println!("No host selected; nothing changed.");
                return Ok(ExitCode::SUCCESS);
            }
        }
        let report = installed(&hosts, self.server.as_deref())?;
        match self.format {
            OutputFormat::Text => report.print_text(),
            OutputFormat::Json if report.failed().is_empty() => print_json(&report)?,
            OutputFormat::Json => {}
        }
        report.into_result()?;
        Ok(ExitCode::SUCCESS)
    }
}

impl McpInstallReport {
    /// Names of the selected hosts that were not connected.
    fn failed(&self) -> Vec<&'static str> {
        self.hosts
            .iter()
            .filter(|outcome| outcome.detail.is_some())
            .map(|outcome| outcome.host.name())
            .collect()
    }

    /// Print one line per host: its failure reason, or its status and file.
    pub(crate) fn print_text(&self) {
        for outcome in &self.hosts {
            let name = outcome.host.name();
            match (&outcome.detail, &outcome.path) {
                (Some(detail), _) => println!("{name}: {detail}"),
                (None, Some(path)) => println!("{name}: {} {}", outcome.status, path.display()),
                (None, None) => println!("{name}: {}", outcome.status),
            }
        }
    }

    /// Tell the person the exact command that starts each connected host on
    /// `prompt` in the current directory. Nothing is launched.
    pub(crate) fn print_handoff(&self, prompt: &str) {
        for outcome in self
            .hosts
            .iter()
            .filter(|outcome| outcome.status.connected())
        {
            println!(
                "Start {} on the setup prompt in this directory:\n{}",
                outcome.host.label(),
                outcome.host.launch(prompt)
            );
        }
    }

    /// Keep a fully connected report, or refuse naming every failed host.
    ///
    /// # Errors
    /// Returns [`WyrdCliError::McpHostInstall`], carrying this report, when
    /// any selected host was not connected.
    pub(crate) fn into_result(self) -> Result<Self, WyrdCliError> {
        let failed = self.failed();
        if failed.is_empty() {
            Ok(self)
        } else {
            Err(WyrdCliError::McpHostInstall {
                hosts: failed.join(", "),
                report: self,
            })
        }
    }
}

/// Offer the existing host installer after a local deployment is usable.
/// Explicit `hosts` install without a question; otherwise detected hosts are
/// offered only when the caller's resolved policy is `interactive`, so
/// unattended runs select none implicitly. No detected hosts, or no host
/// chosen, is a harmless skip returning `None`. The report may hold failed
/// hosts; the caller renders it, then refuses through
/// [`McpInstallReport::into_result`].
///
/// # Errors
/// Returns the same selection and server errors as independent installation.
pub(crate) async fn offer_local(
    hosts: &[McpHost],
    server: &str,
    interactive: bool,
) -> Result<Option<McpInstallReport>, WyrdCliError> {
    if hosts.is_empty()
        && (!interactive
            || HostInstaller::from_process(Some(server))
                .detected()
                .is_empty())
    {
        return Ok(None);
    }
    let mut chosen = hosts.to_vec();
    #[cfg(unix)]
    if chosen.is_empty() {
        chosen = hosts::prompt_local_selection(
            &HostInstaller::from_process(Some(server)).detected(),
            &mut io::stdout(),
        )
        .await
        .map_err(|source| WyrdCliError::Io { source })?;
        if chosen.is_empty() {
            return Ok(None);
        }
    }
    installed(&chosen, Some(server)).map(Some)
}
