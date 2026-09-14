//! Terminal-safe Oracle query command.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

use arrow::json::LineDelimitedWriter;
use clap::{ArgGroup, Args, ValueEnum};
use secrecy::SecretString;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::bifrost::{BifrostClientError, QueryResultStream};
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::{HttpConfig, HttpTransport, ResolvedCredential};
use wyrd_client::{Bifrost, WyrdClient};
use wyrd_spec::vala::api::{BifrostQueryRequest, FreshnessPolicy, VisibilityMode};

use crate::error::{CliBoundaryError, WyrdCliError};

/// Operator arguments for one streaming Oracle query.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("query_input")
        .required(true)
        .multiple(false)
        .args(["sql", "file"])
))]
pub struct QueryCommand {
    /// Wyrd server base URL.
    #[arg(long, env = "WYRD_SERVER_URL")]
    pub server: Option<String>,
    /// Bearer access token.
    #[arg(long, env = "WYRD_ACCESS_TOKEN", hide_env_values = true)]
    pub token: Option<String>,
    /// SELECT-only SQL text.
    #[arg(long)]
    pub sql: Option<String>,
    /// Bounded UTF-8 file containing SELECT-only SQL.
    #[arg(long)]
    pub file: Option<PathBuf>,
    /// Source visibility admitted by Oracle.
    #[arg(long, value_enum, default_value_t = QueryVisibility::PublishedOnly)]
    pub visibility: QueryVisibility,
    /// Freshness behavior when a complete cut is unavailable.
    #[arg(long, value_enum, default_value_t = QueryFreshness::Strict)]
    pub freshness: QueryFreshness,
    /// Row output encoding written to stdout.
    #[arg(long, value_enum, default_value_t = QueryOutputFormat::Jsonl)]
    pub format: QueryOutputFormat,
}

/// CLI spelling for the closed Oracle visibility modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryVisibility {
    /// Query published and sealed sources.
    PublishedOnly,
    /// Fuse live, sealed, and published sources.
    Fused,
}

/// CLI spelling for Oracle freshness policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryFreshness {
    /// Reject a query that cannot meet the requested cut.
    Strict,
    /// Permit a successful degraded terminal.
    AllowDegraded,
}

/// Supported stdout encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum QueryOutputFormat {
    /// Emit one JSON object per result row.
    Jsonl,
    /// Emit one valid Arrow IPC stream.
    Arrow,
}

/// Executes one query and writes data and diagnostics to their dedicated streams.
///
/// # Errors
///
/// Returns a typed CLI error for missing configuration, input IO, client
/// construction, stream validation, Arrow encoding, or stdout failures.
pub async fn dispatch(command: QueryCommand) -> Result<std::process::ExitCode, CliBoundaryError> {
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    execute(command, &mut stdout, &mut stderr).await?;
    Ok(std::process::ExitCode::SUCCESS)
}

/// Runs the command against injectable output sinks for direct tests.
///
/// # Errors
///
/// Returns a typed CLI error under the same conditions as [`dispatch`].
pub async fn execute(
    command: QueryCommand,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), CliBoundaryError> {
    let request = request(&command).map_err(CliBoundaryError::Local)?;
    let client = client(&command).map_err(CliBoundaryError::Local)?;
    let mut stream = Bifrost::query_only(&client)
        .query(&request)
        .await
        .map_err(CliBoundaryError::from)?;
    match command.format {
        QueryOutputFormat::Jsonl => write_jsonl(&mut stream, stdout).await?,
        QueryOutputFormat::Arrow => write_arrow(&mut stream, stdout).await?,
    }
    let terminal = stream
        .terminal()
        .ok_or_else(|| CliBoundaryError::from(BifrostClientError::IncompleteQueryStream))?;
    serde_json::to_writer(&mut *stderr, terminal)
        .map_err(output_error)
        .map_err(CliBoundaryError::Local)?;
    writeln!(stderr)
        .map_err(output_error)
        .map_err(CliBoundaryError::Local)?;
    Ok(())
}

/// Builds the pure wire request after reading the selected SQL source.
///
/// # Errors
///
/// Returns a CLI IO error when the selected file cannot be read as bounded
/// UTF-8, or a configuration error if clap invariants are bypassed in tests.
fn request(command: &QueryCommand) -> Result<BifrostQueryRequest, WyrdCliError> {
    const MAX_SQL_BYTES: u64 = 1024 * 1024;
    let sql = match (&command.sql, &command.file) {
        (Some(sql), None) => sql.clone(),
        (None, Some(path)) => {
            let metadata = fs::metadata(path).map_err(|source| WyrdCliError::Io { source })?;
            if metadata.len() > MAX_SQL_BYTES {
                return Err(WyrdCliError::Query {
                    detail: "SQL file exceeds 1 MiB".to_owned(),
                });
            }
            fs::read_to_string(path).map_err(|source| WyrdCliError::Io { source })?
        }
        _ => {
            return Err(WyrdCliError::QueryConfig {
                field: "--sql or --file",
            });
        }
    };
    Ok(BifrostQueryRequest {
        sql,
        visibility: match command.visibility {
            QueryVisibility::PublishedOnly => VisibilityMode::PublishedOnly,
            QueryVisibility::Fused => VisibilityMode::Fused,
        },
        freshness: match command.freshness {
            QueryFreshness::Strict => FreshnessPolicy::Strict,
            QueryFreshness::AllowDegraded => FreshnessPolicy::AllowDegraded,
        },
        deadline_ms: None,
    })
}

/// Constructs the existing authenticated Wyrd client stack.
///
/// # Errors
///
/// Returns a configuration error for missing credentials or a query setup
/// error when existing auth or transport construction rejects the settings.
fn client(command: &QueryCommand) -> Result<WyrdClient, WyrdCliError> {
    let server = command
        .server
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(WyrdCliError::QueryConfig { field: "--server" })?;
    let token = command
        .token
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(WyrdCliError::QueryConfig { field: "--token" })?;
    let config = ClientConfig {
        http: HttpConfig {
            base_url: server.trim_end_matches('/').to_owned(),
            ..HttpConfig::default()
        },
        ..ClientConfig::default()
    };
    let auth = AuthMiddleware::new(
        &config,
        ResolvedCredential::BearerToken(SecretString::from(token.to_owned())),
    )
    .map_err(output_error)?;
    let http = HttpTransport::new(&config.http, Arc::clone(&auth)).map_err(output_error)?;
    Ok(WyrdClient::from_parts(auth, http, config.grpc))
}

/// Emits record batches as typed line-delimited JSON.
///
/// # Errors
///
/// Returns a query error for stream or Arrow JSON failures, including broken
/// stdout pipes, and never reports truncated output as success.
async fn write_jsonl(
    stream: &mut QueryResultStream,
    stdout: &mut dyn Write,
) -> Result<(), CliBoundaryError> {
    let mut writer = LineDelimitedWriter::new(stdout);
    while let Some(batch) = stream.next_batch().await.map_err(CliBoundaryError::from)? {
        writer
            .write(&batch)
            .map_err(output_error)
            .map_err(CliBoundaryError::Local)?;
    }
    writer
        .finish()
        .map_err(output_error)
        .map_err(CliBoundaryError::Local)
}

/// Emits all batches as one valid Arrow IPC stream without terminal bytes.
///
/// # Errors
///
/// Returns a query error for stream, schema, Arrow IPC, or stdout failures.
async fn write_arrow(
    stream: &mut QueryResultStream,
    stdout: &mut dyn Write,
) -> Result<(), CliBoundaryError> {
    let first = stream.next_batch().await.map_err(CliBoundaryError::from)?;
    let schema = stream.schema().cloned().ok_or_else(|| {
        CliBoundaryError::Local(WyrdCliError::Query {
            detail: "query stream did not provide an Arrow schema".to_owned(),
        })
    })?;
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(stdout, schema.as_ref())
        .map_err(output_error)
        .map_err(CliBoundaryError::Local)?;
    if let Some(batch) = first {
        writer
            .write(&batch)
            .map_err(output_error)
            .map_err(CliBoundaryError::Local)?;
    }
    while let Some(batch) = stream.next_batch().await.map_err(CliBoundaryError::from)? {
        writer
            .write(&batch)
            .map_err(output_error)
            .map_err(CliBoundaryError::Local)?;
    }
    writer
        .finish()
        .map_err(output_error)
        .map_err(CliBoundaryError::Local)
}

/// Maps boundary encoding and setup failures into a scrubbed CLI error.
fn output_error(error: impl std::fmt::Display) -> WyrdCliError {
    WyrdCliError::Query {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use clap::{Parser, Subcommand};

    use super::{QueryCommand, QueryFreshness, QueryOutputFormat, QueryVisibility, request};

    /// Minimal parser that exercises the public query command arguments.
    #[derive(Debug, Parser)]
    struct TestCli {
        /// Selected test command.
        #[command(subcommand)]
        command: TestCommand,
    }

    /// Test-only top-level command projection.
    #[derive(Debug, Subcommand)]
    enum TestCommand {
        /// Runs an Oracle query.
        Query(QueryCommand),
    }

    /// Proves exactly one SQL source is required.
    #[test]
    fn query_parser_rejects_missing_or_duplicate_sql_source() {
        assert!(
            TestCli::try_parse_from(["wyrd", "query", "--server", "x", "--token", "y"]).is_err()
        );
        assert!(
            TestCli::try_parse_from([
                "wyrd",
                "query",
                "--server",
                "x",
                "--token",
                "y",
                "--sql",
                "SELECT 1",
                "--file",
                "query.sql",
            ])
            .is_err()
        );
    }

    /// Pins the operator-facing defaults.
    #[test]
    fn query_parser_uses_safe_defaults() {
        let cli = TestCli::try_parse_from([
            "wyrd", "query", "--server", "x", "--token", "y", "--sql", "SELECT 1",
        ])
        .expect("query arguments are valid");
        let TestCommand::Query(command) = cli.command;
        assert_eq!(command.visibility, QueryVisibility::PublishedOnly);
        assert_eq!(command.freshness, QueryFreshness::Strict);
        assert_eq!(command.format, QueryOutputFormat::Jsonl);
    }

    /// The CLI request projection serializes the shared published/strict defaults.
    #[test]
    fn query_request_defaults_match_shared_client_contract() {
        let command = QueryCommand {
            server: Some("http://localhost".to_owned()),
            token: Some("token".to_owned()),
            sql: Some("SELECT 1".to_owned()),
            file: None,
            visibility: QueryVisibility::PublishedOnly,
            freshness: QueryFreshness::Strict,
            format: QueryOutputFormat::Jsonl,
        };
        let request = request(&command).expect("request projection succeeds");
        assert_eq!(
            serde_json::to_value(&request).expect("request serializes"),
            serde_json::json!({
                "sql": "SELECT 1",
                "visibility": "published_only",
                "freshness": "strict",
                "deadline_ms": null
            })
        );
    }

    /// Explicit CLI policy flags remain opt-ins in the wire request.
    #[test]
    fn query_request_explicit_opt_ins_are_preserved() {
        let command = QueryCommand {
            server: Some("http://localhost".to_owned()),
            token: Some("token".to_owned()),
            sql: Some("SELECT 1".to_owned()),
            file: None,
            visibility: QueryVisibility::Fused,
            freshness: QueryFreshness::AllowDegraded,
            format: QueryOutputFormat::Jsonl,
        };
        let request = request(&command).expect("request projection succeeds");
        assert_eq!(
            serde_json::to_value(&request).expect("request serializes"),
            serde_json::json!({
                "sql": "SELECT 1",
                "visibility": "fused",
                "freshness": "allow_degraded",
                "deadline_ms": null
            })
        );
    }
}
