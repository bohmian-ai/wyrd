//! Card lifecycle verbs for the `wyrd` command-line client.

use std::fmt::Display;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;
use std::{fmt, path::PathBuf};

use clap::{Args, ValueEnum};
use serde::Serialize;
use wyrd_client::WyrdClient;
use wyrd_client::auth::AuthMiddleware;
use wyrd_client::cards::RegistrationReceipt;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, HydrationSummary};
use wyrd_client::config::ClientConfig;
use wyrd_client::error::WyrdClientError;
use wyrd_client::transport::HttpTransport;
use wyrd_loader::{Diagnostic, LoadError, RegistrationInput, build_registration_input, load};
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::query::MetadataQuery;
use wyrd_spec::reference::CardRef;
use wyrd_spec::registry::{CardLifecycleStatus, ListCardsRequest, ListCardsResponse};

use crate::error::WyrdCliError;

/// Output encoding for card lifecycle commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable terminal output.
    #[default]
    Text,
    /// Stable JSON output for scripts and agents.
    Json,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Text => "text",
            Self::Json => "json",
        })
    }
}

/// Shared server and credential options for networked card commands.
#[derive(Debug, Args)]
pub struct ConnectionArgs {
    /// Wyrd HTTP server base URL.
    #[arg(long, value_name = "URL", env = "WYRD_SERVER_URL")]
    pub server: Option<String>,
}

/// Selector fields shared by get, load, and delete.
#[derive(Debug, Args)]
pub struct SelectorArgs {
    /// Card kind, such as `Model` or `Prompt`.
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,
    /// Card space for a named selector or an optional UID assertion.
    #[arg(long, value_name = "SPACE")]
    pub space: Option<String>,
    /// Card name for a named selector or an optional UID assertion.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Exact Card version.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
    /// Exact Card UID.
    #[arg(long, value_name = "UID")]
    pub uid: Option<String>,
}

/// Arguments for `wyrd plan`.
#[derive(Debug, Args)]
pub struct PlanArgs {
    /// Card file or directory to load.
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd apply`.
#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Card file or directory to load and register.
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd get`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct GetArgs {
    /// Root Card selector whose reachable graph will be hydrated.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Required destination directory for the hydrated bundle.
    #[arg(long, value_name = "DIRECTORY")]
    pub output_dir: Option<PathBuf>,
    /// Write an inspectable metadata-only bundle without artifact payloads;
    /// the result is not runnable. Complete artifact downloads are the default.
    #[arg(long)]
    pub metadata_only: bool,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd latest`.
#[derive(Debug, Args)]
pub struct LatestArgs {
    /// Card kind, such as `Model` or `Prompt`.
    #[arg(long, value_name = "KIND")]
    pub kind: String,
    /// Card space.
    #[arg(long, value_name = "SPACE")]
    pub space: String,
    /// Card name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd list`.
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Optional Card kind filter.
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,
    /// Optional Card space filter.
    #[arg(long, value_name = "SPACE")]
    pub space: Option<String>,
    /// Optional Card name filter.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    /// Optional exact semver range filter.
    #[arg(long, value_name = "RANGE")]
    pub version_range: Option<String>,
    /// Optional lifecycle status filter.
    #[arg(long, value_name = "STATUS")]
    pub status: Option<String>,
    /// Optional metadata query filter.
    #[arg(long, value_name = "FILTER")]
    pub filter: Option<String>,
    /// Include prerelease versions.
    #[arg(long)]
    pub include_prerelease: bool,
    /// Maximum number of results.
    #[arg(long, value_name = "COUNT")]
    pub limit: Option<i32>,
    /// Opaque continuation cursor from a previous page.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd load`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct LoadArgs {
    /// Card selector.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Directory for downloaded artifacts. Defaults to a managed temporary directory.
    #[arg(long, value_name = "DIRECTORY")]
    pub path: Option<PathBuf>,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd delete`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct DeleteArgs {
    /// Exact Card selector. Named selectors require `--version`.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

#[derive(Debug, Serialize)]
struct PlanReport {
    ok: bool,
    cards: Vec<PlanCard>,
    diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Serialize)]
struct PlanCard {
    kind: String,
    space: Option<String>,
    name: String,
    version: Option<String>,
}

#[derive(Debug, Serialize)]
struct LatestOutput {
    card_ref: CardRef,
}

#[derive(Debug, Serialize)]
struct LoadOutput {
    card_ref: CardRef,
    materialized: bool,
}

#[derive(Debug, Serialize)]
struct DeleteOutput {
    deleted: bool,
}

/// Validate a local Card tree and print the deterministic registration plan.
///
/// The command loads authored files, resolves local registration inputs, and
/// reports the Cards and loader diagnostics without constructing credentials or
/// contacting a Wyrd server.
///
/// # Errors
/// Returns `WyrdCliError::CardLoad` when loading or local registration-input
/// construction fails, or an output error when JSON serialization fails.
pub async fn dispatch_plan(args: PlanArgs) -> Result<ExitCode, WyrdCliError> {
    let tree = match load(&args.path) {
        Ok(tree) => tree,
        Err(error) => {
            print_load_failure(&error, args.format);
            return Err(WyrdCliError::CardLoad(error));
        }
    };
    let diagnostics = tree.diagnostics.clone();
    let input = build_registration_input(tree).map_err(WyrdCliError::CardLoad)?;
    let report = PlanReport {
        ok: true,
        cards: plan_cards(&input),
        diagnostics,
    };
    match args.format {
        OutputFormat::Text => print_plan_text(&report),
        OutputFormat::Json => print_json(&report)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Load a local Card tree, register it through the shared registry handle, and
/// print the registration receipt.
///
/// Local loading and validation complete before the command constructs the
/// configured client. Registration then performs the remote artifact and Card
/// lifecycle operations owned by `wyrd_client::cards`.
///
/// # Errors
/// Returns a CLI load error for invalid local input, a client error when
/// configuration or credentials cannot be prepared, a registry error when
/// registration fails, or an output error when JSON serialization fails.
///
/// # Cancellation
/// Cancellation may stop the command during artifact transfer or server
/// completion. Retry behavior follows the registry operation's idempotency
/// contract.
pub async fn dispatch_apply(args: ApplyArgs) -> Result<ExitCode, WyrdCliError> {
    let tree = load(&args.path).map_err(WyrdCliError::CardLoad)?;
    let input = build_registration_input(tree).map_err(WyrdCliError::CardLoad)?;
    let cards = build_cards(&args.connection)?;
    let receipt = cards
        .register(&input)
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    match args.format {
        OutputFormat::Text => print_apply_text(&receipt),
        OutputFormat::Json => print_json(&receipt)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve a Card and hydrate its reachable graph into a local directory.
///
/// The command validates the selector and required output directory locally,
/// then delegates graph reads, artifact verification, staging, and publication
/// to a graph-focused hydrator sharing the registry context. Complete hydration is the default; the
/// metadata-only flag omits artifact payload downloads.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector or missing output
/// directory, a client error when configuration or credentials fail, a
/// registry error when reads or hydration fail, or an output error when JSON
/// serialization fails.
///
/// # Cancellation
/// Cancellation may stop remote reads or artifact transfers after partial
/// staging progress. The registry hydration workflow owns cleanup for returned
/// errors; a dropped task may require later staging cleanup.
pub async fn dispatch_get(args: GetArgs) -> Result<ExitCode, WyrdCliError> {
    let selector = selector_from_args(&args.selector, false)?;
    let output_dir = args.output_dir.as_deref().ok_or_else(|| {
        invalid_argument(
            "output-dir",
            "<missing>",
            "required for hydrated get output",
        )
    })?;
    let cards = build_cards(&args.connection)?;
    let hydrator = CardGraphHydrator::new(cards.registry_context());
    let mode = if args.metadata_only {
        HydrationMode::MetadataOnly
    } else {
        HydrationMode::Complete
    };
    let output = hydrator
        .hydrate(&selector, output_dir, mode)
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    match args.format {
        OutputFormat::Text => print_get_text(&output),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve and print the latest Active Card version for a named identity.
///
/// The command parses the kind, space, and name before asking the shared
/// registry handle for the exact server-resolved `CardRef`.
///
/// # Errors
/// Returns a CLI argument error for an invalid kind or identity, a client error
/// when configuration or credentials fail, a registry error when no matching
/// Active Card exists or the server request fails, or an output error when JSON
/// serialization fails.
///
/// # Cancellation
/// Cancellation may stop the remote latest-version lookup before a result is
/// printed; the read is non-durable and safe to retry.
pub async fn dispatch_latest(args: LatestArgs) -> Result<ExitCode, WyrdCliError> {
    let kind = parse_kind(&args.kind)?;
    let space = parse_id("space", &args.space, "a valid Card space")?;
    let name = parse_id("name", &args.name, "a valid Card name")?;
    let cards = build_cards(&args.connection)?;
    let card_ref = cards
        .resolve_latest(kind, space, name)
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    let output = LatestOutput { card_ref };
    match args.format {
        OutputFormat::Text => println!("latest: {}", output.card_ref),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// List Card summaries through the typed registry query.
///
/// The command validates optional kind, identity, status, metadata-filter, and
/// limit values locally, then delegates pagination and server-side filtering to
/// the shared `Cards` handle.
///
/// # Errors
/// Returns a CLI argument error for invalid filters or values, a client error
/// when configuration or credentials fail, a registry error when the server
/// query fails, or an output error when JSON serialization fails.
///
/// # Cancellation
/// Cancellation may stop the remote list query before a response is printed;
/// the read is non-durable and safe to retry.
pub async fn dispatch_list(args: ListArgs) -> Result<ExitCode, WyrdCliError> {
    if args.limit.is_some_and(|limit| limit < 1) {
        return Err(invalid_argument(
            "limit",
            &args
                .limit
                .map_or_else(String::new, |limit| limit.to_string()),
            "a positive integer",
        ));
    }
    let request = ListCardsRequest {
        kind: args.kind.as_deref().map(parse_kind).transpose()?,
        space: args
            .space
            .as_deref()
            .map(|value| parse_id("space", value, "a valid Card space"))
            .transpose()?,
        name: args
            .name
            .as_deref()
            .map(|value| parse_id("name", value, "a valid Card name"))
            .transpose()?,
        version_range: args.version_range,
        status: args.status.as_deref().map(parse_status).transpose()?,
        filter: args
            .filter
            .as_deref()
            .map(|value| {
                MetadataQuery::parse(value)
                    .map_err(|error| invalid_argument("filter", value, &error.to_string()))
            })
            .transpose()?,
        include_prerelease: args.include_prerelease,
        limit: args.limit,
        cursor: args.cursor,
    };
    let cards = build_cards(&args.connection)?;
    let response = cards
        .list(request)
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    match args.format {
        OutputFormat::Text => print_list_text(&response),
        OutputFormat::Json => print_json(&response)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Load one Card and materialize its server-owned artifacts locally.
///
/// The command parses the selector, delegates Card and artifact reads to the
/// shared registry handle, and reports the exact resolved Card reference after
/// materialization. A caller-provided path is used when present; otherwise the
/// registry handle manages a temporary artifact directory.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector, a client error when
/// configuration or credentials fail, a registry error when the Card,
/// inventory, destination, or artifact transfer fails, or an output error when
/// JSON serialization fails.
///
/// # Cancellation
/// Cancellation may stop artifact materialization after partial local progress;
/// the registry load operation owns the temporary-directory lifecycle, while a
/// caller-provided destination may retain already downloaded files.
pub async fn dispatch_load(args: LoadArgs) -> Result<ExitCode, WyrdCliError> {
    let selector = selector_from_args(&args.selector, false)?;
    let cards = build_cards(&args.connection)?;
    let loaded = cards
        .load(selector, args.path.as_deref())
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    let output = LoadOutput {
        card_ref: exact_card_ref(&loaded.card)?,
        materialized: true,
    };
    match args.format {
        OutputFormat::Text => println!("loaded: {}", output.card_ref),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Dispatch `wyrd delete`.
pub async fn dispatch_delete(args: DeleteArgs) -> Result<ExitCode, WyrdCliError> {
    let selector = selector_from_args(&args.selector, true)?;
    let cards = build_cards(&args.connection)?;
    cards
        .delete(selector)
        .await
        .map_err(|source| WyrdCliError::Registry { source })?;
    let output = DeleteOutput { deleted: true };
    match args.format {
        OutputFormat::Text => println!("deleted"),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

fn build_cards(connection: &ConnectionArgs) -> Result<Cards, WyrdCliError> {
    let mut config = ClientConfig::from_global().map_err(map_client_error)?;
    if let Some(server) = &connection.server {
        config.http.base_url.clone_from(server);
    }
    config.http.validate().map_err(map_client_error)?;
    let credential = config.resolve_credential().map_err(map_client_error)?;
    let auth = AuthMiddleware::new(&config, credential).map_err(map_client_error)?;
    let transport =
        HttpTransport::new(&config.http, Arc::clone(&auth)).map_err(map_client_error)?;
    Ok(Cards::with_client(WyrdClient::from_parts(
        auth,
        transport,
        config.grpc,
    )))
}

fn map_client_error(error: WyrdClientError) -> WyrdCliError {
    match error {
        WyrdClientError::NoCredentials => WyrdCliError::NoCredentials,
        WyrdClientError::Config { field, reason } => WyrdCliError::ClientConfig {
            detail: format!("{field}: {reason}"),
        },
        WyrdClientError::TransportDown { transport, message } => WyrdCliError::ClientTransport {
            detail: format!("{transport}: {message}"),
        },
    }
}

fn selector_from_args(
    args: &SelectorArgs,
    require_exact_for_delete: bool,
) -> Result<CardSelector, WyrdCliError> {
    let kind = args.kind.as_deref().map(parse_kind).transpose()?;
    let version = args
        .version
        .as_deref()
        .map(|value| {
            VersionBlock::parse(value)
                .map_err(|error| invalid_argument("version", value, &error.to_string()))
        })
        .transpose()?;
    if let Some(uid) = &args.uid {
        let kind =
            kind.ok_or_else(|| invalid_argument("kind", "<missing>", "required with --uid"))?;
        let uid = parse_id("uid", uid, "a valid UUIDv7 Card UID")?;
        let space = args
            .space
            .as_deref()
            .map(|value| parse_id("space", value, "a valid Card space"))
            .transpose()?;
        let name = args
            .name
            .as_deref()
            .map(|value| parse_id("name", value, "a valid Card name"))
            .transpose()?;
        return Ok(CardSelector::uid(kind, uid)
            .with_identity_assertions(space, name)
            .with_optional_version(version));
    }
    if require_exact_for_delete && version.is_none() {
        return Err(WyrdCliError::DeleteSelectorRequiresExact);
    }
    let kind = kind.ok_or_else(|| invalid_argument("kind", "<missing>", "required"))?;
    let space = args
        .space
        .as_deref()
        .ok_or_else(|| invalid_argument("space", "<missing>", "required for a named selector"))
        .and_then(|value| parse_id("space", value, "a valid Card space"))?;
    let name = args
        .name
        .as_deref()
        .ok_or_else(|| invalid_argument("name", "<missing>", "required for a named selector"))
        .and_then(|value| parse_id("name", value, "a valid Card name"))?;
    let selector = CardSelector::named(kind, space, name);
    Ok(selector.with_optional_version(version))
}

trait OptionalVersionSelector {
    fn with_optional_version(self, version: Option<VersionBlock>) -> Self;
}

impl OptionalVersionSelector for CardSelector {
    fn with_optional_version(self, version: Option<VersionBlock>) -> Self {
        match version {
            Some(version) => self.with_version(version),
            None => self,
        }
    }
}

fn parse_kind(value: &str) -> Result<CardKind, WyrdCliError> {
    CardKind::from_wire_name(value).ok_or_else(|| {
        invalid_argument(
            "kind",
            value,
            "a known Wyrd Card kind such as Model, Prompt, or Workflow",
        )
    })
}

fn parse_status(value: &str) -> Result<CardLifecycleStatus, WyrdCliError> {
    match value.to_ascii_lowercase().as_str() {
        "pending" => Ok(CardLifecycleStatus::Pending),
        "active" => Ok(CardLifecycleStatus::Active),
        "deprecated" => Ok(CardLifecycleStatus::Deprecated),
        "failed" => Ok(CardLifecycleStatus::Failed),
        "expired" => Ok(CardLifecycleStatus::Expired),
        "deleted" => Ok(CardLifecycleStatus::Deleted),
        _ => Err(invalid_argument(
            "status",
            value,
            "pending, active, deprecated, failed, expired, or deleted",
        )),
    }
}

fn parse_id<T>(field: &str, value: &str, expected: &str) -> Result<T, WyrdCliError>
where
    T: FromStr,
    T::Err: Display,
{
    value
        .parse()
        .map_err(|error: T::Err| invalid_argument(field, value, &format!("{expected}: {error}")))
}

fn invalid_argument(field: &str, value: &str, expected: &str) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: field.to_owned(),
        value: value.to_owned(),
        expected: expected.to_owned(),
    }
}

fn exact_card_ref(card: &Card) -> Result<CardRef, WyrdCliError> {
    let version = card
        .metadata
        .resolved_pin()
        .cloned()
        .ok_or_else(|| WyrdCliError::Registry {
            source: WyrdError::RegistryInvalidCardSpec {
                message: "server response did not contain an exact card version".to_owned(),
                details: serde_json::json!({}),
            },
        })?;
    let space = card
        .metadata
        .space
        .clone()
        .ok_or_else(|| WyrdCliError::Registry {
            source: WyrdError::RegistryInvalidCardSpec {
                message: "server response did not contain a card space".to_owned(),
                details: serde_json::json!({}),
            },
        })?;
    Ok(CardRef {
        kind: card.kind.clone(),
        name: card.metadata.name.clone(),
        version,
        space: Some(space),
        uid: card.metadata.uid.clone(),
    })
}

fn plan_cards(input: &RegistrationInput) -> Vec<PlanCard> {
    input
        .submissions
        .iter()
        .map(|submission| PlanCard {
            kind: submission.kind.wire_name().to_owned(),
            space: submission.metadata.space.as_ref().map(ToString::to_string),
            name: submission.metadata.name.to_string(),
            version: submission
                .metadata
                .version
                .as_ref()
                .map(ToString::to_string),
        })
        .collect()
}

fn print_plan_text(report: &PlanReport) {
    println!("plan valid: {} card(s)", report.cards.len());
    for card in &report.cards {
        let space = card.space.as_deref().unwrap_or("default");
        let version = card.version.as_deref().unwrap_or("auto");
        println!("  {} {space}/{}@{version}", card.kind, card.name);
    }
    for diagnostic in &report.diagnostics {
        println!("warning {}: {}", diagnostic.code, diagnostic.message);
    }
}

fn print_load_failure(error: &LoadError, format: OutputFormat) {
    match format {
        OutputFormat::Text => {
            for diagnostic in &error.diagnostics {
                println!(
                    "{}: {} ({})",
                    diagnostic.code,
                    diagnostic.message,
                    diagnostic.path.display()
                );
            }
        }
        OutputFormat::Json => {
            let report = PlanReport {
                ok: false,
                cards: Vec::new(),
                diagnostics: error.diagnostics.clone(),
            };
            match serde_json::to_string_pretty(&report) {
                Ok(output) => println!("{output}"),
                Err(_) => println!("{{\"ok\":false,\"cards\":[],\"diagnostics\":[]}}"),
            }
        }
    }
}

/// Print a registration receipt as one root line followed by one line per outcome.
fn print_apply_text(receipt: &RegistrationReceipt) {
    println!("registered: {}", receipt.root);
    for outcome in &receipt.outcomes {
        println!(
            "  {}: {:?} ({:?})",
            outcome.card_ref, outcome.outcome, outcome.status
        );
    }
}

fn print_get_text(output: &HydrationSummary) {
    println!(
        "hydrated: {} destination={} mode={} cards={} artifacts={} downloaded_artifacts={}",
        output.root,
        output.destination.display(),
        output.mode,
        output.card_count,
        output.artifact_count,
        output.downloaded_artifact_count
    );
}

fn print_list_text(response: &ListCardsResponse) {
    for item in &response.items {
        println!(
            "{} {}/{}@{} status={:?}",
            item.kind.wire_name(),
            item.space,
            item.name,
            item.version,
            item.status
        );
    }
    if let Some(cursor) = &response.next_cursor {
        println!("next_cursor: {cursor}");
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), WyrdCliError> {
    let output = serde_json::to_string_pretty(value).map_err(|error| WyrdCliError::Output {
        detail: error.to_string(),
    })?;
    println!("{output}");
    Ok(())
}
