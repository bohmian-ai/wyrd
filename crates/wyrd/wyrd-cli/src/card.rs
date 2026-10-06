//! Card lifecycle verbs for the `wyrd` command-line client.

use std::fmt;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Args, ValueEnum};
use serde::Serialize;
use wyrd_client::cards::RegistrationReceipt;
use wyrd_client::cards::{CardGraphHydrator, CardSelector, Cards, HydrationMode, HydrationSummary};
use wyrd_loader::{
    Diagnostic, LoadError, RegistrationInput, build_registration_input, load as load_tree,
};
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

/// Deterministic local registration plan: the result of `wyrd plan`, which
/// `--format json` prints and the in-process [`plan`] returns.
#[derive(Debug, Serialize)]
pub struct PlanReport {
    /// Whether the tree loaded and resolved; a returned report is always `true`.
    pub ok: bool,
    /// Cards the tree would register, in registration order.
    pub cards: Vec<PlanCard>,
    /// Non-fatal loader diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// One Card in a [`PlanReport`], identified as authored.
#[derive(Debug, Serialize)]
pub struct PlanCard {
    /// Card kind wire name.
    pub kind: String,
    /// Authored space, or `None` for the default space.
    pub space: Option<String>,
    /// Card name.
    pub name: String,
    /// Authored version, or `None` when the server assigns one.
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
struct LatestOutput {
    card_ref: CardRef,
}

/// Result of `wyrd load`, which `--format json` prints and the in-process
/// [`load`] returns.
#[derive(Debug, Serialize)]
pub struct LoadOutput {
    /// Exact reference of the Card the selector resolved to.
    pub card_ref: CardRef,
    /// Whether the Card's artifacts were materialized locally; always `true`
    /// on success.
    pub materialized: bool,
}

#[derive(Debug, Serialize)]
struct DeleteOutput {
    deleted: bool,
}

/// Validate a local Card tree and return its deterministic registration plan.
///
/// The in-process form of `wyrd plan`: it loads the authored files, resolves
/// local registration inputs, and reports the Cards and loader diagnostics
/// without constructing credentials or contacting a Wyrd server. The value is
/// exactly what `wyrd plan --format json` prints.
///
/// # Errors
/// Returns `WYRD_LOADER_400_INVALID_ENVELOPE`, with the loader diagnostics as
/// details, when the tree cannot be loaded or resolved.
pub fn plan(path: &Path) -> Result<PlanReport, WyrdError> {
    plan_tree(path).map_err(WyrdError::from)
}

/// Load and resolve a local Card tree into its registration plan.
///
/// # Errors
/// Returns `WyrdCliError::CardLoad` when loading or local registration-input
/// construction fails.
fn plan_tree(path: &Path) -> Result<PlanReport, WyrdCliError> {
    let tree = load_tree(path).map_err(WyrdCliError::CardLoad)?;
    let diagnostics = tree.diagnostics.clone();
    let input = build_registration_input(tree).map_err(WyrdCliError::CardLoad)?;
    Ok(PlanReport {
        ok: true,
        cards: plan_cards(&input),
        diagnostics,
    })
}

/// Render `wyrd plan` over [`plan_tree`].
///
/// A load failure is also printed as an `ok: false` report so a JSON caller
/// reads the diagnostics from stdout.
///
/// # Errors
/// Returns `WyrdCliError::CardLoad` when loading or local registration-input
/// construction fails, or an output error when JSON serialization fails.
pub fn dispatch_plan(args: PlanArgs) -> Result<ExitCode, WyrdCliError> {
    let report = match plan_tree(&args.path) {
        Ok(report) => report,
        Err(WyrdCliError::CardLoad(error)) => {
            print_load_failure(&error, args.format);
            return Err(WyrdCliError::CardLoad(error));
        }
        Err(error) => return Err(error),
    };
    match args.format {
        OutputFormat::Text => print_plan_text(&report),
        OutputFormat::Json => print_json(&report)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Load a local Card tree, register it through the shared registry handle,
/// and return the registration receipt.
///
/// The in-process form of `wyrd apply`; the receipt is exactly what
/// `wyrd apply --format json` prints. Local loading and validation complete
/// before the client is constructed from the ambient credential chain, which
/// `server` re-points. Registration then performs the remote artifact and Card
/// lifecycle operations owned by `wyrd_client::cards`.
///
/// # Errors
/// Returns `WYRD_LOADER_400_INVALID_ENVELOPE` for invalid local input, a
/// `WYRD_CLIENT_*` error when configuration or credentials cannot be prepared,
/// or the server's stable error when registration fails.
///
/// # Cancellation
/// Cancellation may stop the command during artifact transfer or server
/// completion. Retry behavior follows the registry operation's idempotency
/// contract.
pub async fn apply(path: &Path, server: Option<&str>) -> Result<RegistrationReceipt, WyrdError> {
    register_tree(path, server).await.map_err(WyrdError::from)
}

/// Load, resolve, and register one local Card tree.
///
/// # Errors
/// Returns a CLI load error for invalid local input, a client error when
/// configuration or credentials cannot be prepared, or a server error when
/// registration fails.
async fn register_tree(
    path: &Path,
    server: Option<&str>,
) -> Result<RegistrationReceipt, WyrdCliError> {
    let tree = load_tree(path).map_err(WyrdCliError::CardLoad)?;
    let input = build_registration_input(tree).map_err(WyrdCliError::CardLoad)?;
    build_cards(server)?
        .register(&input)
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Render `wyrd apply` over [`register_tree`].
///
/// # Errors
/// Returns the errors of [`register_tree`], or an output error when JSON
/// serialization fails.
pub async fn dispatch_apply(args: ApplyArgs) -> Result<ExitCode, WyrdCliError> {
    let receipt = register_tree(&args.path, args.connection.server.as_deref()).await?;
    match args.format {
        OutputFormat::Text => print_apply_text(&receipt),
        OutputFormat::Json => print_json(&receipt)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve a Card and hydrate its reachable graph into `output_dir`.
///
/// The in-process form of `wyrd get`; the summary is exactly what
/// `wyrd get --format json` prints. Complete hydration is the default;
/// `metadata_only` writes an inspectable, non-runnable bundle without artifact
/// payloads. The client comes from the ambient credential chain, which
/// `server` re-points.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid selector, a `WYRD_CLIENT_*`
/// error when configuration or credentials fail, or the server's stable error
/// when reads or hydration fail.
///
/// # Cancellation
/// Cancellation may stop remote reads or artifact transfers after partial
/// staging progress. The registry hydration workflow owns cleanup for returned
/// errors; a dropped task may require later staging cleanup.
pub async fn get(
    selector: &SelectorArgs,
    output_dir: &Path,
    metadata_only: bool,
    server: Option<&str>,
) -> Result<HydrationSummary, WyrdError> {
    hydrate(selector, Some(output_dir), metadata_only, server)
        .await
        .map_err(WyrdError::from)
}

/// Validate a selector and destination, then hydrate the selected graph.
///
/// The selector and destination are checked before the client is built, so
/// an invalid request never resolves credentials.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector or missing output
/// directory, a client error when configuration or credentials fail, or a
/// server error when reads or hydration fail.
async fn hydrate(
    selector: &SelectorArgs,
    output_dir: Option<&Path>,
    metadata_only: bool,
    server: Option<&str>,
) -> Result<HydrationSummary, WyrdCliError> {
    let selector = selector_from_args(selector, false)?;
    let output_dir = output_dir.ok_or_else(|| {
        invalid_argument(
            "output-dir",
            "<missing>",
            "required for hydrated get output",
        )
    })?;
    let cards = build_cards(server)?;
    let mode = if metadata_only {
        HydrationMode::MetadataOnly
    } else {
        HydrationMode::Complete
    };
    CardGraphHydrator::new(cards.registry_context())
        .hydrate(&selector, output_dir, mode)
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Render `wyrd get` over [`hydrate`].
///
/// # Errors
/// Returns the errors of [`hydrate`], or an output error when JSON
/// serialization fails.
pub async fn dispatch_get(args: GetArgs) -> Result<ExitCode, WyrdCliError> {
    let output = hydrate(
        &args.selector,
        args.output_dir.as_deref(),
        args.metadata_only,
        args.connection.server.as_deref(),
    )
    .await?;
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
    let cards = build_cards(args.connection.server.as_deref())?;
    let card_ref = cards
        .resolve_latest(kind, space, name)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
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
    let cards = build_cards(args.connection.server.as_deref())?;
    let response = cards
        .list(request)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
    match args.format {
        OutputFormat::Text => print_list_text(&response),
        OutputFormat::Json => print_json(&response)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Load one Card and materialize its server-owned artifacts locally.
///
/// The in-process form of `wyrd load`; the output is exactly what
/// `wyrd load --format json` prints. Card and artifact reads go through the
/// shared registry handle, built from the ambient credential chain that
/// `server` re-points. A caller-provided `path` receives the artifacts;
/// otherwise the registry handle manages a temporary directory.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid selector, a `WYRD_CLIENT_*`
/// error when configuration or credentials fail, or the server's stable error
/// when the Card, inventory, destination, or artifact transfer fails.
///
/// # Cancellation
/// Cancellation may stop artifact materialization after partial local progress;
/// the registry load operation owns the temporary-directory lifecycle, while a
/// caller-provided destination may retain already downloaded files.
pub async fn load(
    selector: &SelectorArgs,
    path: Option<&Path>,
    server: Option<&str>,
) -> Result<LoadOutput, WyrdError> {
    materialize(selector, path, server)
        .await
        .map_err(WyrdError::from)
}

/// Resolve one Card, materialize its artifacts, and pin the exact reference.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector, a client error when
/// configuration or credentials fail, or a server error when the load fails or
/// the response names no exact version.
async fn materialize(
    selector: &SelectorArgs,
    path: Option<&Path>,
    server: Option<&str>,
) -> Result<LoadOutput, WyrdCliError> {
    let selector = selector_from_args(selector, false)?;
    let loaded = build_cards(server)?
        .load(selector, path)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
    Ok(LoadOutput {
        card_ref: exact_card_ref(&loaded.card)?,
        materialized: true,
    })
}

/// Render `wyrd load` over [`materialize`].
///
/// # Errors
/// Returns the errors of [`materialize`], or an output error when JSON
/// serialization fails.
pub async fn dispatch_load(args: LoadArgs) -> Result<ExitCode, WyrdCliError> {
    let output = materialize(
        &args.selector,
        args.path.as_deref(),
        args.connection.server.as_deref(),
    )
    .await?;
    match args.format {
        OutputFormat::Text => println!("loaded: {}", output.card_ref),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Dispatch `wyrd delete`.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for a selector that does not name
/// exactly one card, a connection error when the client cannot be built, and
/// [`WyrdCliError::Server`] when the server refuses the delete.
pub async fn dispatch_delete(args: DeleteArgs) -> Result<ExitCode, WyrdCliError> {
    let selector = selector_from_args(&args.selector, true)?;
    let cards = build_cards(args.connection.server.as_deref())?;
    cards
        .delete(selector)
        .await
        .map_err(|source| WyrdCliError::Server { source })?;
    let output = DeleteOutput { deleted: true };
    match args.format {
        OutputFormat::Text => println!("deleted"),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Build the card-administration handle, optionally re-pointed at `server`.
///
/// Construction lives in [`crate::client`]: the ambient configuration is the
/// right default here, because `wyrd apply` and `wyrd card get` administer the
/// deployment the user is already pointed at.
///
/// # Errors
/// Returns the client-assembly errors documented on
/// [`crate::client::from_global`].
fn build_cards(server: Option<&str>) -> Result<Cards, WyrdCliError> {
    Ok(Cards::with_client(crate::client::from_global(server)?))
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

/// Parse one typed identifier argument.
///
/// # Errors
/// Returns `WYRD_CLI_400_INVALID_ARGUMENT` naming `field`, `value`, and the
/// `expected` shape with the parser's reason.
pub(crate) fn parse_id<T>(field: &str, value: &str, expected: &str) -> Result<T, WyrdCliError>
where
    T: FromStr,
    T::Err: Display,
{
    value
        .parse()
        .map_err(|error: T::Err| invalid_argument(field, value, &format!("{expected}: {error}")))
}

/// The `WYRD_CLI_400_INVALID_ARGUMENT` error for one argument.
pub(crate) fn invalid_argument(field: &str, value: &str, expected: &str) -> WyrdCliError {
    WyrdCliError::InvalidArgument {
        field: field.to_owned(),
        value: value.to_owned(),
        expected: expected.to_owned(),
    }
}

/// Pin a server-returned card to the exact version it resolved to.
///
/// Output and follow-up commands quote the resolved version rather than the
/// requirement the caller typed, so a later run cannot silently address a
/// different card.
///
/// # Errors
/// Returns [`WyrdCliError::Server`] when the response carries no resolved
/// version, which means the server did not answer with an exact card.
pub(crate) fn exact_card_ref(card: &Card) -> Result<CardRef, WyrdCliError> {
    let version = card
        .metadata
        .resolved_pin()
        .cloned()
        .ok_or_else(|| WyrdCliError::Server {
            source: WyrdError::RegistryInvalidCardSpec {
                message: "server response did not contain an exact card version".to_owned(),
                details: serde_json::json!({}),
            },
        })?;
    let space = card
        .metadata
        .space
        .clone()
        .ok_or_else(|| WyrdCliError::Server {
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

/// Print one response as pretty JSON; shared by the card and Operator
/// connection verbs.
///
/// # Errors
/// Returns [`WyrdCliError::Output`] when the value cannot be serialized.
pub(crate) fn print_json<T: Serialize>(value: &T) -> Result<(), WyrdCliError> {
    let output = serde_json::to_string_pretty(value).map_err(|error| WyrdCliError::Output {
        detail: error.to_string(),
    })?;
    println!("{output}");
    Ok(())
}

/// In-process command contract for the Card lifecycle verbs.
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SelectorArgs, apply, get, load, plan};

    /// A named selector whose kind is not a Card kind, refused before any
    /// client is built.
    fn unknown_kind_selector() -> SelectorArgs {
        SelectorArgs {
            kind: Some("NotAKind".to_owned()),
            space: Some("default".to_owned()),
            name: Some("cli-prompt".to_owned()),
            version: None,
            uid: None,
        }
    }

    /// Plan, apply, get, and load return the value `--format json` prints, or
    /// a shared-catalog error, without writing output or producing an exit
    /// code; local failures surface before any credential is resolved.
    ///
    /// # Panics
    /// Panics when a command returns the wrong value or error code.
    #[tokio::test]
    async fn in_process_commands_return_the_json_result_without_exiting() {
        let temp = tempfile::tempdir().expect("tempdir creates");
        let path = temp.path().join("prompt.yaml");
        std::fs::write(
            &path,
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: cli-prompt\n  version: 1.0.0\n  space: default\nspec:\n  provider: openai\n  model: gpt-4o\n  messages: [hello]\n",
        )
        .expect("prompt card writes");
        let missing = temp.path().join("missing.yaml");

        let report = plan(&path).expect("a valid tree plans");
        assert_eq!(
            serde_json::to_value(&report).expect("plan report serializes"),
            json!({
                "ok": true,
                "cards": [{
                    "kind": "Prompt",
                    "space": "default",
                    "name": "cli-prompt",
                    "version": "1.0.0",
                }],
                "diagnostics": [],
            })
        );
        assert_eq!(
            plan(&missing)
                .expect_err("a missing tree is refused")
                .code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
        assert_eq!(
            apply(&missing, None)
                .await
                .expect_err("a missing tree is refused before registration")
                .code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
        assert_eq!(
            get(&unknown_kind_selector(), temp.path(), false, None)
                .await
                .expect_err("an unknown kind is refused")
                .code(),
            "WYRD_SPEC_400_VALIDATION"
        );
        assert_eq!(
            load(&unknown_kind_selector(), None, None)
                .await
                .expect_err("an unknown kind is refused")
                .code(),
            "WYRD_SPEC_400_VALIDATION"
        );
    }
}
