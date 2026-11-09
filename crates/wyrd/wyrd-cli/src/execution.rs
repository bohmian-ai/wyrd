//! `wyrd execution` inspection and cancellation of one queued Verifier
//! execution.
//!
//! Each verb is one call on the shared [`wyrd_client::verify::Execution`]
//! handle and prints the server's typed response as JSON. Pages, producer
//! selection, authorization, and cancellation stay on the server; a refusal
//! keeps its stable Wyrd error.

use std::process::ExitCode;

use clap::{Args, Subcommand};
use serde_json::Value;
use wyrd_client::WyrdClient;
use wyrd_client::verify::{
    Execution, ExecutionPageQuery, PublicationName, PublicationSelector, TaskId,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::ExecutionId;

use crate::card::print_json;
use crate::error::WyrdCliError;

/// One queued execution's inspection and cancellation against one server.
#[derive(Debug, Args)]
pub struct ExecutionCommand {
    /// Wyrd server base URL. Credentials resolve from the environment or
    /// client configuration, never from arguments.
    #[arg(long, env = "WYRD_SERVER_URL", global = true)]
    pub server: Option<String>,
    /// Verb to run.
    #[command(subcommand)]
    pub verb: ExecutionVerb,
}

/// Execution verbs; reads need `verifier:read`, cancel needs `verifier:run`.
#[derive(Debug, Clone, Subcommand)]
pub enum ExecutionVerb {
    /// Read the execution's lifecycle status, terminal error, and result.
    Status(ExecutionArg),
    /// List one page of the execution's invocation attempts.
    Invocations(PageArgs),
    /// List one page of the execution's publications, without content.
    Publications(PageArgs),
    /// Read one publication of an exact producer attempt.
    Publication(PublicationArgs),
    /// Plan the download of a file publication, or of one listed `--path` of
    /// a directory publication.
    Download(PublicationArgs),
    /// Cancel the execution durably; repeating answers its current status.
    Cancel(ExecutionArg),
}

/// Address one execution.
#[derive(Debug, Clone, Args)]
pub struct ExecutionArg {
    /// Execution `submit` returned.
    #[arg(long, value_name = "UUID")]
    pub execution_id: ExecutionId,
}

/// Address one page of one listing.
#[derive(Debug, Clone, Args)]
pub struct PageArgs {
    /// Execution to list.
    #[command(flatten)]
    pub execution: ExecutionArg,
    /// Entries per page; the server defaults to 50 and allows at most 200.
    #[arg(long)]
    pub limit: Option<u32>,
    /// `next_cursor` of the previous page of this same listing.
    #[arg(long)]
    pub cursor: Option<String>,
}

/// Address one publication by its exact producer attempt.
#[derive(Debug, Clone, Args)]
pub struct PublicationArgs {
    /// Execution that retains it.
    #[command(flatten)]
    pub execution: ExecutionArg,
    /// Producing step or requirement.
    #[arg(long, value_name = "ID")]
    pub step_id: TaskId,
    /// Publication name within that attempt.
    #[arg(long)]
    pub name: PublicationName,
    /// One-based invocation attempt.
    #[arg(long)]
    pub attempt: u32,
    /// Zero-based fan-out item; omitted for none.
    #[arg(long)]
    pub item_index: Option<u32>,
    /// Zero-based loop iteration; omitted for none.
    #[arg(long)]
    pub iteration_index: Option<u32>,
    /// File within a directory publication; only a download takes it.
    #[arg(long)]
    pub path: Option<String>,
}

impl PageArgs {
    /// The page query these arguments select.
    fn query(&self) -> ExecutionPageQuery {
        ExecutionPageQuery {
            limit: self.limit,
            cursor: self.cursor.clone(),
        }
    }
}

impl PublicationArgs {
    /// The producer selector these arguments select.
    fn selector(&self) -> PublicationSelector {
        PublicationSelector {
            attempt: self.attempt,
            item_index: self.item_index,
            iteration_index: self.iteration_index,
            path: self.path.clone(),
        }
    }
}

impl ExecutionVerb {
    /// The execution this verb addresses.
    fn id(&self) -> ExecutionId {
        match self {
            Self::Status(arg) | Self::Cancel(arg) => arg.execution_id,
            Self::Invocations(page) | Self::Publications(page) => page.execution.execution_id,
            Self::Publication(publication) | Self::Download(publication) => {
                publication.execution.execution_id
            }
        }
    }
}

/// Run one execution verb and return the JSON `wyrd execution` prints.
///
/// The in-process form of `wyrd execution <verb>`. The request runs as
/// `client`, or, when it is omitted, as the ambient credential chain.
///
/// # Errors
/// Returns a `WYRD_CLIENT_*` error when the client cannot be built, or the
/// server's stable validation, permission, not-found, or availability error.
pub async fn execution(
    verb: ExecutionVerb,
    client: Option<WyrdClient>,
) -> Result<Value, WyrdError> {
    run(verb, &crate::client::explicit_or_ambient(client)?).await
}

/// Run `verb` over an assembled `client`.
///
/// # Errors
/// Returns the server's stable error for the call.
async fn run(verb: ExecutionVerb, client: &WyrdClient) -> Result<Value, WyrdError> {
    let execution = Execution::connect_with(verb.id(), client).await?;
    let value = match &verb {
        ExecutionVerb::Status(_) => serde_json::to_value(execution.status().await?),
        ExecutionVerb::Invocations(page) => {
            serde_json::to_value(execution.invocations(&page.query()).await?)
        }
        ExecutionVerb::Publications(page) => {
            serde_json::to_value(execution.publications(&page.query()).await?)
        }
        ExecutionVerb::Publication(args) => serde_json::to_value(
            execution
                .publication(&args.step_id, &args.name, &args.selector())
                .await?,
        ),
        ExecutionVerb::Download(args) => serde_json::to_value(
            execution
                .download(&args.step_id, &args.name, &args.selector())
                .await?,
        ),
        ExecutionVerb::Cancel(_) => serde_json::to_value(execution.cancel().await?),
    };
    value.map_err(|error| WyrdError::Internal {
        message: format!("an execution response did not serialize: {error}"),
        details: Value::Null,
    })
}

impl ExecutionCommand {
    /// Dispatch the verb and print its response.
    ///
    /// # Errors
    /// Returns client-construction errors and the server's stable error.
    pub async fn dispatch(self) -> Result<ExitCode, WyrdCliError> {
        let client = crate::client::from_global(self.server.as_deref())?;
        let value = run(self.verb, &client)
            .await
            .map_err(|source| WyrdCliError::Server { source })?;
        print_json(&value).map(|()| ExitCode::SUCCESS)
    }
}
