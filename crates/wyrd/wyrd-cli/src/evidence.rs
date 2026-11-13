//! Deferred evidence publication from the command line.
//!
//! `wyrd evidence publish <input>` is one call through
//! `wyrd_client::Evidence`: a `--file` is uploaded for a media input, a
//! `--json` file (or `-` for stdin) is sent inline for a JSON input. The
//! repository and commit come from `--repository`/`--commit`, else
//! `WYRD_REPOSITORY`/`WYRD_COMMIT`, else the working directory's Git
//! checkout; the receipt is printed as JSON.

use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Args, Subcommand};
use serde_json::Value;
use wyrd_client::verify::File;

use crate::card::print_json;
use crate::error::WyrdCliError;
use crate::principal::credential::TenantEndpoint;

/// Deferred evidence verbs.
#[derive(Debug, Subcommand)]
pub enum EvidenceCommand {
    /// Publish evidence for a deferred Verifier input at one repository
    /// commit and print the durable receipt.
    Publish(PublishArgs),
}

/// `wyrd evidence publish` arguments.
#[derive(Debug, Args)]
#[command(group(ArgGroup::new("value").required(true).args(["file", "json"])))]
pub struct PublishArgs {
    /// The deferred input name the evidence supplies.
    #[arg(value_name = "INPUT")]
    pub input: String,
    /// A file to upload for a media input; its extension sets the format.
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,
    /// A JSON document to send inline for a JSON input; `-` reads stdin.
    #[arg(long, value_name = "PATH")]
    pub json: Option<PathBuf>,
    /// The repository as `owner/name`; defaults to `WYRD_REPOSITORY`, then
    /// the checkout's `origin`.
    #[arg(long, value_name = "OWNER/NAME")]
    pub repository: Option<String>,
    /// The full commit SHA the evidence is about; defaults to `WYRD_COMMIT`,
    /// then the checkout's `HEAD`.
    #[arg(long, value_name = "SHA")]
    pub commit: Option<String>,
    /// The server and ambient credential.
    #[command(flatten)]
    pub endpoint: TenantEndpoint,
}

/// Dispatch one evidence verb.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when the `--json` source cannot be read,
/// [`WyrdCliError::InvalidArgument`] when it is not JSON, and the shared
/// client's stable error for an unresolvable source or the server's refusal.
pub async fn dispatch(command: EvidenceCommand) -> Result<ExitCode, WyrdCliError> {
    let EvidenceCommand::Publish(args) = command;
    let evidence = crate::client::evidence(args.endpoint.server.as_str()).await?;
    let repository = args.repository.as_deref();
    let commit = args.commit.as_deref();
    let receipt = match (args.file, args.json) {
        (Some(path), _) => {
            evidence
                .publish(&args.input, File::path(path), repository, commit)
                .await?
        }
        (None, Some(path)) => {
            evidence
                .publish(&args.input, read_json(&path)?, repository, commit)
                .await?
        }
        (None, None) => unreachable!("clap requires --file or --json"),
    };
    print_json(&receipt).map(|()| ExitCode::SUCCESS)
}

/// Read and parse the JSON document at `path`, or stdin for `-`.
///
/// # Errors
/// Returns [`WyrdCliError::Io`] when it cannot be read and
/// [`WyrdCliError::InvalidArgument`] when it is not JSON.
fn read_json(path: &std::path::Path) -> Result<Value, WyrdCliError> {
    let text = if path.as_os_str() == "-" {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|source| WyrdCliError::Io { source })?;
        text
    } else {
        std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?
    };
    serde_json::from_str(&text).map_err(|error| WyrdCliError::InvalidArgument {
        field: "--json".to_owned(),
        value: path.display().to_string(),
        expected: format!("a JSON document ({error})"),
    })
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::{Cli, Command};

    use super::EvidenceCommand;

    /// `publish` takes exactly one of `--file` and `--json`, and the source
    /// flags are optional.
    ///
    /// # Panics
    /// Panics when a valid form is refused or an invalid one parses.
    #[test]
    fn publish_requires_exactly_one_value_source() {
        let parse = |args: &[&str]| {
            Cli::try_parse_from(
                [
                    "wyrd",
                    "evidence",
                    "publish",
                    "ci",
                    "--server",
                    "http://wyrd.test",
                ]
                .iter()
                .chain(args),
            )
        };
        let parsed = parse(&["--json", "results.json"]).expect("json parses");
        let Command::Evidence(EvidenceCommand::Publish(args)) = parsed.command else {
            panic!("evidence publish");
        };
        assert_eq!(args.input, "ci");
        assert!(args.repository.is_none() && args.commit.is_none());
        assert!(
            parse(&[
                "--file",
                "report.pdf",
                "--repository",
                "acme/widget",
                "--commit",
                "abc"
            ])
            .is_ok()
        );
        assert!(parse(&[]).is_err(), "a value is required");
        assert!(
            parse(&["--file", "a.pdf", "--json", "b.json"]).is_err(),
            "only one value"
        );
    }
}
