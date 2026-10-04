//! `wyrd workflow` commands: run a Workflow and manage server runs.
//!
//! `run` selects exactly one source — an authored file, a registered UID, or
//! an exact registered `space`/`name`/`version` — and executes it in this
//! process or, with `--execution server`, on the Wyrd server. Local runs
//! delegate loading, route preparation, and execution to the shared
//! [`wyrd_client::Workflow`]; server runs, `status`, and `cancel` delegate to
//! [`wyrd_client::Workflows`]. Every success prints the portable
//! [`WorkflowRun`] snapshot; failures keep their stable Wyrd error code.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgGroup, Args, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::{Map, Value};
use wyrd_client::cards::{CardSelector, Cards};
use wyrd_client::{Workflow, Workflows};
use wyrd_semver::VersionBlock;
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, WorkflowRun, WorkflowRunStatus};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::WorkflowRunId;
use wyrd_spec::reference::CardRef;

use crate::card::{OutputFormat, exact_card_ref, invalid_argument, parse_id, print_json};
use crate::error::WyrdCliError;

/// Process code for a run that ended in any status other than `succeeded`.
const RUN_NOT_SUCCEEDED: u8 = 2;

/// Process code after an interrupt stops waiting for a server run.
pub(crate) const INTERRUPTED: u8 = 130;

/// Workflow execution and server-run management.
#[derive(Debug, Args)]
pub struct WorkflowCommand {
    /// Selected Workflow verb.
    #[command(subcommand)]
    pub verb: WorkflowVerb,
}

/// Workflow verbs.
#[derive(Debug, Subcommand)]
pub enum WorkflowVerb {
    /// Run a Workflow from a file or the registry, locally or on the server.
    Run(RunArgs),
    /// Print the current snapshot of a server run.
    Status(RunIdArgs),
    /// Cancel a server run; cancelling a finished run returns it unchanged.
    Cancel(RunIdArgs),
}

/// Where a Workflow run executes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum Execution {
    /// In this process, with the shared local route configuration.
    #[default]
    Local,
    /// On the Wyrd server, as an accepted server run.
    Server,
}

/// Arguments for `wyrd workflow run`.
#[derive(Debug, Args)]
#[command(
    disable_version_flag = true,
    group(ArgGroup::new("source").required(true).args(["file", "uid", "space"])),
    group(ArgGroup::new("run_input").args(["input", "input_file"]))
)]
pub struct RunArgs {
    /// Authored Workflow file; runs locally only.
    #[arg(long, value_name = "PATH")]
    pub file: Option<PathBuf>,
    /// Registered Workflow Card UID.
    #[arg(long, value_name = "UID")]
    pub uid: Option<String>,
    /// Registered Workflow space; requires --name and --version.
    #[arg(long, value_name = "SPACE", requires_all = ["name", "version"])]
    pub space: Option<String>,
    /// Registered Workflow name.
    #[arg(long, value_name = "NAME", requires = "space")]
    pub name: Option<String>,
    /// Exact registered Workflow version.
    #[arg(long, value_name = "VERSION", requires = "space")]
    pub version: Option<String>,
    /// Run input as a JSON object.
    #[arg(long, value_name = "JSON")]
    pub input: Option<String>,
    /// File holding the run input as a JSON object.
    #[arg(long, value_name = "PATH")]
    pub input_file: Option<PathBuf>,
    /// Where the run executes.
    #[arg(long, value_enum, default_value_t)]
    pub execution: Execution,
    /// Return once the server accepts the run instead of waiting for it.
    #[arg(long)]
    pub detach: bool,
    /// Include every step's result in text output.
    #[arg(long)]
    pub steps: bool,
    /// Output encoding; JSON prints the run snapshot.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
    /// Wyrd server base URL; defaults to the shared client configuration.
    #[arg(long, value_name = "URL", conflicts_with = "file")]
    pub server: Option<String>,
}

/// Arguments naming one server run.
#[derive(Debug, Args)]
pub struct RunIdArgs {
    /// Server Workflow run ID.
    #[arg(value_name = "RUN_ID")]
    pub run_id: String,
    /// Include every step's result in text output.
    #[arg(long)]
    pub steps: bool,
    /// Output encoding; JSON prints the run snapshot.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
    /// Wyrd server base URL; defaults to the shared client configuration.
    #[arg(long, value_name = "URL")]
    pub server: Option<String>,
}

/// The one Workflow a run selects.
enum Source {
    /// An authored Workflow file.
    File(PathBuf),
    /// A registered Workflow Card.
    Registered(Registered),
}

/// How a registered Workflow Card is selected.
enum Registered {
    /// By UID.
    Uid(CardSelector),
    /// By exact space, name, and version.
    Exact(CardRef),
}

impl WorkflowCommand {
    /// Run the selected Workflow verb.
    ///
    /// # Errors
    /// Returns the errors of [`RunArgs::run`] or [`RunIdArgs::status`] and
    /// [`RunIdArgs::cancel`].
    pub async fn dispatch(self) -> Result<ExitCode, WyrdCliError> {
        match self.verb {
            WorkflowVerb::Run(args) => args.run().await,
            WorkflowVerb::Status(args) => args.status().await,
            WorkflowVerb::Cancel(args) => args.cancel().await,
        }
    }
}

impl RunArgs {
    /// Validate the invocation, then run the selected Workflow.
    ///
    /// Selector, input, and execution choices are checked before any client
    /// is built, file is read for loading, or request is sent. A local run
    /// loads the Workflow (a file through the shared loader, a registered
    /// Workflow by its exact locked graph), runs it in this process, and
    /// prints the terminal snapshot. A server run is submitted once; its ID
    /// is printed to stderr on acceptance. With `--detach` the accepted
    /// snapshot is printed and the command returns; otherwise the command
    /// polls until the run is terminal. An interrupt while polling stops
    /// polling only: the run keeps going on the server, nothing is cancelled
    /// or resubmitted, and the command exits with code 130.
    ///
    /// The process exits 0 when the run succeeded or was detached and 2 when
    /// it reached any other terminal status.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_400_INVALID_ARGUMENT` for a malformed selector or
    /// input, a file source with server execution, or `--detach` with local
    /// execution; `WYRD_CLI_500_IO` when the input file cannot be read; the
    /// client construction errors; and the stable Workflow, registry, or
    /// server error of the failing load, run, or request.
    pub async fn run(self) -> Result<ExitCode, WyrdCliError> {
        let source = self.source()?;
        let input = self.input()?;
        match (self.execution, source) {
            (Execution::Local, _) if self.detach => Err(invalid_argument(
                "detach",
                "true",
                "only with --execution server",
            )),
            (Execution::Local, source) => self.run_local(source, input).await,
            (Execution::Server, Source::File(path)) => Err(invalid_argument(
                "file",
                &path.display().to_string(),
                "file sources run locally; register the Workflow to run it on the server",
            )),
            (Execution::Server, Source::Registered(registered)) => {
                self.run_server(registered, input).await
            }
        }
    }

    /// Run `source` in this process and print its terminal snapshot.
    ///
    /// # Errors
    /// Returns the client construction errors and the stable error of the
    /// failing load or pre-dispatch refusal; step failures are reported in
    /// the printed run.
    async fn run_local(
        &self,
        source: Source,
        input: Map<String, Value>,
    ) -> Result<ExitCode, WyrdCliError> {
        let workflow = match source {
            Source::File(path) => Workflow::from_path(path).await?,
            Source::Registered(Registered::Uid(selector)) => {
                self.cards()?.workflow().load(&selector).await?
            }
            Source::Registered(Registered::Exact(card_ref)) => {
                self.cards()?
                    .workflow()
                    .load(&CardSelector::exact(card_ref))
                    .await?
            }
        };
        let run = workflow.run(input).await.map_err(WyrdError::from)?;
        Report::new(self.format, self.steps).finished(&run)
    }

    /// Submit `registered` to the server and wait for it unless detached.
    ///
    /// A UID source is first read once to learn its exact identity, which the
    /// server run request requires.
    ///
    /// # Errors
    /// Returns the client construction errors, the registry error of the UID
    /// read, and the server's stable error for the submission or a poll.
    async fn run_server(
        &self,
        registered: Registered,
        input: Map<String, Value>,
    ) -> Result<ExitCode, WyrdCliError> {
        let client = crate::client::from_global(self.server.as_deref())?;
        let workflow = match registered {
            Registered::Exact(card_ref) => card_ref,
            Registered::Uid(selector) => {
                exact_card_ref(&Cards::with_client(client.clone()).get(selector).await?)?
            }
        };
        let workflows = Workflows::new(client);
        let request = CreateWorkflowRunRequest {
            workflow,
            input: input.into_iter().collect(),
            timeout_seconds: None,
        };
        let accepted = workflows.create(&request).await?;
        eprintln!("accepted workflow run {}", accepted.run_id);
        let report = Report::new(self.format, self.steps);
        if self.detach {
            report.print(&accepted)?;
            return Ok(ExitCode::SUCCESS);
        }
        tokio::select! {
            run = workflows.wait(&accepted.run_id) => report.finished(&run?),
            _ = tokio::signal::ctrl_c() => {
                eprintln!(
                    "interrupted; workflow run {id} continues on the server; \
                     check it with `wyrd workflow status {id}`",
                    id = accepted.run_id
                );
                Ok(ExitCode::from(INTERRUPTED))
            }
        }
    }

    /// Build the registry handle for a registered local source.
    ///
    /// # Errors
    /// Returns the client construction errors of [`crate::client::from_global`].
    fn cards(&self) -> Result<Cards, WyrdCliError> {
        Ok(Cards::with_client(crate::client::from_global(
            self.server.as_deref(),
        )?))
    }

    /// Resolve the one selected source.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_400_INVALID_ARGUMENT` for a malformed UID, space,
    /// name, or version, or when no complete source is given.
    fn source(&self) -> Result<Source, WyrdCliError> {
        if let Some(path) = &self.file {
            return Ok(Source::File(path.clone()));
        }
        if let Some(uid) = &self.uid {
            let uid = parse_id("uid", uid, "a valid UUIDv7 Card UID")?;
            return Ok(Source::Registered(Registered::Uid(CardSelector::uid(
                CardKind::Workflow,
                uid,
            ))));
        }
        let (Some(space), Some(name), Some(version)) = (&self.space, &self.name, &self.version)
        else {
            return Err(invalid_argument(
                "source",
                "<missing>",
                "exactly one of --file, --uid, or --space with --name and --version",
            ));
        };
        Ok(Source::Registered(Registered::Exact(CardRef {
            kind: CardKind::Workflow,
            name: parse_id("name", name, "a valid Card name")?,
            version: VersionBlock::parse(version)
                .map_err(|error| invalid_argument("version", version, &error.to_string()))?,
            space: Some(parse_id("space", space, "a valid Card space")?),
            uid: None,
        })))
    }

    /// Read the run input object; no input is the empty object.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_500_IO` when the input file cannot be read and
    /// `WYRD_CLI_400_INVALID_ARGUMENT` when the input is not a JSON object.
    /// The error never echoes the input itself.
    fn input(&self) -> Result<Map<String, Value>, WyrdCliError> {
        let (field, label, text) = match (&self.input, &self.input_file) {
            (Some(text), _) => ("input", "<inline JSON>".to_owned(), text.clone()),
            (None, Some(path)) => (
                "input-file",
                path.display().to_string(),
                std::fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?,
            ),
            (None, None) => return Ok(Map::new()),
        };
        match serde_json::from_str(&text) {
            Ok(Value::Object(input)) => Ok(input),
            _ => Err(invalid_argument(field, &label, "a JSON object")),
        }
    }
}

impl RunIdArgs {
    /// Print the current snapshot of the named server run.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_400_INVALID_ARGUMENT` for a malformed run ID, the
    /// client construction errors, and the server's stable error.
    pub async fn status(self) -> Result<ExitCode, WyrdCliError> {
        let run_id = self.run_id()?;
        let run = self.workflows()?.get(&run_id).await?;
        Report::new(self.format, self.steps).print(&run)?;
        Ok(ExitCode::SUCCESS)
    }

    /// Request cancellation and print the resulting snapshot.
    ///
    /// Cancellation is idempotent: a finished run is printed unchanged.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_400_INVALID_ARGUMENT` for a malformed run ID, the
    /// client construction errors, and the server's stable error.
    pub async fn cancel(self) -> Result<ExitCode, WyrdCliError> {
        let run_id = self.run_id()?;
        let run = self.workflows()?.cancel(&run_id).await?;
        Report::new(self.format, self.steps).print(&run)?;
        Ok(ExitCode::SUCCESS)
    }

    /// Parse the run ID argument.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_400_INVALID_ARGUMENT` for a value that is not a
    /// `UUIDv7`.
    fn run_id(&self) -> Result<WorkflowRunId, WyrdCliError> {
        parse_id("run-id", &self.run_id, "a valid UUIDv7 Workflow run ID")
    }

    /// Build the server-run handle.
    ///
    /// # Errors
    /// Returns the client construction errors of [`crate::client::from_global`].
    fn workflows(&self) -> Result<Workflows, WyrdCliError> {
        Ok(Workflows::new(crate::client::from_global(
            self.server.as_deref(),
        )?))
    }
}

/// Renders run snapshots in the selected output encoding.
#[derive(Debug, Clone, Copy)]
struct Report {
    /// Output encoding.
    format: OutputFormat,
    /// Whether text output lists every step.
    steps: bool,
}

impl Report {
    /// Bind the output choices.
    const fn new(format: OutputFormat, steps: bool) -> Self {
        Self { format, steps }
    }

    /// Print a terminal run and map its status to the process code.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_500_OUTPUT` when JSON serialization fails.
    fn finished(self, run: &WorkflowRun) -> Result<ExitCode, WyrdCliError> {
        self.print(run)?;
        Ok(if run.status == WorkflowRunStatus::Succeeded {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(RUN_NOT_SUCCEEDED)
        })
    }

    /// Print one snapshot to stdout.
    ///
    /// JSON prints the snapshot itself. Text prints the run ID and status,
    /// each named output, the run error, and, with `--steps`, every step's
    /// status, attempts, output, and error.
    ///
    /// # Errors
    /// Returns `WYRD_CLI_500_OUTPUT` when JSON serialization fails.
    fn print(self, run: &WorkflowRun) -> Result<(), WyrdCliError> {
        if self.format == OutputFormat::Json {
            return print_json(run);
        }
        println!("workflow run {} {}", run.run_id, wire(&run.status));
        for (name, value) in &run.outputs {
            println!("output {name}: {}", plain(value));
        }
        if let Some(error) = &run.error {
            println!("error {}: {}", error.code, error.message);
        }
        if self.steps {
            for (id, step) in &run.steps {
                println!(
                    "step {id}: {} after {} attempts",
                    wire(&step.status),
                    step.attempts
                );
                if let Some(text) = &step.text {
                    println!("  text: {text}");
                }
                if let Some(value) = &step.structured_output {
                    println!("  structured_output: {value}");
                }
                if let Some(error) = &step.error {
                    println!("  error {}: {}", error.code, error.message);
                }
            }
        }
        Ok(())
    }
}

/// Wire name of a serialized unit enum, such as a run or step status.
fn wire<T: Serialize>(value: &T) -> String {
    match serde_json::to_value(value) {
        Ok(Value::String(name)) => name,
        _ => String::new(),
    }
}

/// A JSON value for terminal display: strings unquoted, others compact JSON.
fn plain(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
