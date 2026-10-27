//! Card lifecycle verbs for the `wyrd` command-line client.

use std::fmt;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;
use wyrd_client::WyrdClient;
use wyrd_client::cards::DeclarationReceipt;
use wyrd_client::cards::{BundleMode, CardSelector, Cards, PullSummary};
use wyrd_loader::{
    DeclarationInput, Diagnostic, LoadError, build_declaration_input, load as load_tree,
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

/// Selector fields shared by get, pull, and delete.
#[derive(Debug, Args)]
pub struct SelectorArgs {
    /// Card kind, such as `Model` or `Prompt`.
    #[arg(long, value_name = "KIND")]
    pub kind: Option<String>,
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

/// Arguments for `wyrd declare`.
#[derive(Debug, Args)]
pub struct DeclareArgs {
    /// Card file or directory to load and declare.
    #[arg(value_name = "PATH")]
    pub path: PathBuf,
    /// Check the local files and report the declaration candidates without
    /// contacting a server, uploading artifacts, or assigning versions.
    #[arg(long)]
    pub check: bool,
    /// Wyrd server and credential options; unused by `--check`.
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
    /// Card selector whose envelope is read.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Arguments for `wyrd pull`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct PullArgs {
    /// Root Card selector whose reachable graph is pulled.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Destination directory for the Card bundle.
    #[arg(long, value_name = "DIRECTORY")]
    pub output_dir: PathBuf,
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

/// `wyrd artifact` verbs: publish files as an Artifact Card and list a
/// Card's files.
#[derive(Debug, Subcommand)]
pub enum ArtifactCommand {
    /// Publish one file or directory as an Artifact Card and print its exact
    /// reference; --extract also publishes `extracted/P.json` for each
    /// supported document `P`.
    Upload(UploadArgs),
    /// List the files a Card holds, with their sizes and digests.
    Files(FilesArgs),
}

/// Arguments for `wyrd artifact upload`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct UploadArgs {
    /// Regular file or directory to publish.
    #[arg(value_name = "SOURCE")]
    pub source: PathBuf,
    /// Artifact Card name.
    #[arg(long, value_name = "NAME")]
    pub name: String,
    /// Exact version or version scope; the server assigns one when omitted.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
    /// Version bump the server applies to the latest version.
    #[arg(long, value_name = "LEVEL")]
    pub bump: Option<BumpLevel>,
    /// Artifact kind, an open vocabulary.
    #[arg(long, value_name = "KIND", default_value = "file")]
    pub artifact_kind: String,
    /// Extract supported documents locally and publish the outputs too.
    #[arg(long)]
    pub extract: bool,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// A `--bump` level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BumpLevel {
    /// Increment major.
    Major,
    /// Increment minor.
    Minor,
    /// Increment patch.
    Patch,
}

/// Arguments for `wyrd artifact files`.
#[derive(Debug, Args)]
#[command(disable_version_flag = true)]
pub struct FilesArgs {
    /// Card selector whose files are listed; `--kind` defaults to `Artifact`.
    #[command(flatten)]
    pub selector: SelectorArgs,
    /// Wyrd server and credential options.
    #[command(flatten)]
    pub connection: ConnectionArgs,
    /// Output encoding.
    #[arg(long, default_value_t)]
    pub format: OutputFormat,
}

/// Run one `wyrd artifact` verb.
///
/// # Errors
/// Returns invalid-argument errors for a bad version or selector, the
/// upload preparation's refusals, credential errors, and the server's
/// errors.
pub async fn dispatch_artifact(command: ArtifactCommand) -> Result<ExitCode, WyrdCliError> {
    match command {
        ArtifactCommand::Upload(args) => {
            let server = args.connection.server.as_deref();
            let (root, upload) = upload_artifact(
                &args.source,
                &args.name,
                args.version.as_deref(),
                args.bump,
                &args.artifact_kind,
                args.extract,
                || crate::client::from_global(server),
            )
            .await?;
            match args.format {
                OutputFormat::Text => {
                    println!("uploaded: {root}");
                    for (source, output) in &upload.extracted {
                        println!("  extracted {source} -> {output}");
                    }
                    for source in &upload.skipped {
                        println!("  skipped {source}");
                    }
                }
                OutputFormat::Json => print_json(&root)?,
            }
        }
        ArtifactCommand::Files(args) => {
            let server = args.connection.server.as_deref();
            let inventory =
                list_artifact_files(&args.selector, || crate::client::from_global(server)).await?;
            match args.format {
                OutputFormat::Text => {
                    for file in &inventory.artifacts {
                        println!("{}\t{}", file.relative_path, file.size_bytes);
                    }
                }
                OutputFormat::Json => print_json(&inventory)?,
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Publish one file or directory as an Artifact Card.
///
/// The in-process form of `wyrd artifact upload`; the returned reference is
/// exactly what `wyrd artifact upload --format json` prints. `extract` also
/// publishes `extracted/P.json` for each supported document `P`. Runs as
/// `client`, or, when it is omitted, as a client built from the ambient
/// credential chain.
///
/// # Arguments
/// - `source`: the regular file or directory to publish.
/// - `name`: the Artifact Card name.
/// - `version`: an exact version or version scope; `None` lets the server
///   assign one.
/// - `bump`: the version bump the server applies to the latest version.
/// - `artifact_kind`: the Artifact kind, an open vocabulary.
/// - `extract`: extract supported documents locally and publish the outputs.
/// - `client`: the principal to declare as; `None` resolves the ambient chain.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid name or version, the
/// upload preparation's refusals, a `WYRD_CLIENT_*` error when configuration
/// or credentials fail, or the server's stable error when declaration fails.
pub async fn artifact_upload(
    source: &Path,
    name: &str,
    version: Option<&str>,
    bump: Option<BumpLevel>,
    artifact_kind: &str,
    extract: bool,
    client: Option<WyrdClient>,
) -> Result<CardRef, WyrdError> {
    upload_artifact(source, name, version, bump, artifact_kind, extract, || {
        crate::client::explicit_or_ambient(client)
    })
    .await
    .map(|(root, _)| root)
    .map_err(WyrdError::from)
}

/// Validate and prepare an upload, then declare it.
///
/// Arguments and the source are checked before the client is built, so an
/// invalid request never resolves credentials. The prepared upload is
/// returned beside the root so its extraction report outlives declaration.
///
/// # Errors
/// Returns a CLI argument error for an invalid name or version, the
/// preparation's refusal, the error of `connect`, or a server error.
async fn upload_artifact(
    source: &Path,
    name: &str,
    version: Option<&str>,
    bump: Option<BumpLevel>,
    artifact_kind: &str,
    extract: bool,
    connect: impl FnOnce() -> Result<WyrdClient, WyrdCliError>,
) -> Result<(CardRef, wyrd_loader::ArtifactUpload), WyrdCliError> {
    let version = version
        .map(|value| {
            value
                .parse::<wyrd_semver::VersionSpec>()
                .map_err(|error| invalid_argument("version", value, &error.to_string()))
        })
        .transpose()?;
    let bump = bump.map(|level| match level {
        BumpLevel::Major => wyrd_semver::VersionBump::Major,
        BumpLevel::Minor => wyrd_semver::VersionBump::Minor,
        BumpLevel::Patch => wyrd_semver::VersionBump::Patch,
    });
    let metadata = wyrd_spec::envelope::Metadata {
        version,
        bump,
        ..serde_json::from_value(serde_json::json!({ "name": name }))
            .map_err(|error| invalid_argument("name", name, &error.to_string()))?
    };
    let spec = wyrd_spec::card::artifact::ArtifactSpec {
        artifact_kind: artifact_kind.to_owned(),
        ..wyrd_spec::card::artifact::ArtifactSpec::default()
    };
    let upload = wyrd_loader::ArtifactUpload::prepare(source, metadata, spec, extract)?;
    let receipt = Cards::connect_with(&connect()?)
        .await?
        .declare(&upload.input)
        .await?;
    Ok((receipt.root, upload))
}

/// List the files one Card holds, from the server inventory.
///
/// The in-process form of `wyrd artifact files`; the inventory is exactly
/// what `wyrd artifact files --format json` prints. The selector's kind
/// defaults to `Artifact`. Reads run as `client`, or, when it is omitted, as
/// a client built from the ambient credential chain.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid selector, a
/// `WYRD_CLIENT_*` error when configuration or credentials fail, or the
/// server's stable error when the read fails.
pub async fn artifact_files(
    selector: &SelectorArgs,
    client: Option<WyrdClient>,
) -> Result<wyrd_client::cards::ArtifactInventoryResponse, WyrdError> {
    list_artifact_files(selector, || crate::client::explicit_or_ambient(client))
        .await
        .map_err(WyrdError::from)
}

/// Validate a selector, defaulting its kind to `Artifact`, then read its
/// file inventory.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector, the error of
/// `connect`, or a server error when the read fails.
async fn list_artifact_files(
    selector: &SelectorArgs,
    connect: impl FnOnce() -> Result<WyrdClient, WyrdCliError>,
) -> Result<wyrd_client::cards::ArtifactInventoryResponse, WyrdCliError> {
    let selector = SelectorArgs {
        kind: Some(
            selector
                .kind
                .clone()
                .unwrap_or_else(|| "Artifact".to_owned()),
        ),
        name: selector.name.clone(),
        version: selector.version.clone(),
        uid: selector.uid.clone(),
    };
    let selector = selector_from_args(&selector, false)?;
    Ok(Cards::connect_with(&connect()?)
        .await?
        .list_artifacts(selector)
        .await?)
}

/// Local declaration check: the result of `wyrd declare --check`, which
/// `--format json` prints and the in-process [`declare_check`] returns.
///
/// A successful check proves only that the local files load and resolve; it
/// does not claim that remote references exist, that declaration will be
/// authorized, or which versions the server will assign.
#[derive(Debug, Serialize)]
pub struct CheckReport {
    /// Whether the tree loaded and resolved; a returned report is always `true`.
    pub ok: bool,
    /// Declaration candidates, in declaration order.
    pub cards: Vec<CheckCard>,
    /// Non-fatal loader diagnostics.
    pub diagnostics: Vec<Diagnostic>,
}

/// One declaration candidate in a [`CheckReport`], identified as authored.
#[derive(Debug, Serialize)]
pub struct CheckCard {
    /// Card kind wire name.
    pub kind: String,
    /// Card name.
    pub name: String,
    /// Authored version, or `None` when the server assigns one.
    pub version: Option<String>,
}

#[derive(Debug, Serialize)]
struct LatestOutput {
    card_ref: CardRef,
}

#[derive(Debug, Serialize)]
struct DeleteOutput {
    deleted: bool,
}

/// Check a local Card tree and return its declaration candidates.
///
/// The in-process form of `wyrd declare --check`: it loads the authored files,
/// resolves transitive Card paths and local artifact sources, and reports the
/// candidates and loader diagnostics without constructing credentials or
/// contacting a Wyrd server. The value is exactly what
/// `wyrd declare --check --format json` prints.
///
/// # Arguments
/// - `path`: a Card file or a bundle directory to check.
///
/// # Errors
/// Returns `WYRD_LOADER_400_INVALID_ENVELOPE`, with the loader diagnostics as
/// details, when the tree cannot be loaded or resolved.
pub fn declare_check(path: impl AsRef<Path>) -> Result<CheckReport, WyrdError> {
    check_tree(path.as_ref()).map_err(WyrdError::from)
}

/// Load and resolve a local Card tree into its declaration candidates.
///
/// # Errors
/// Returns `WyrdCliError::CardLoad` when loading or local declaration-input
/// construction fails.
fn check_tree(path: &Path) -> Result<CheckReport, WyrdCliError> {
    let tree = load_tree(path).map_err(WyrdCliError::CardLoad)?;
    let diagnostics = tree.diagnostics.clone();
    let input = build_declaration_input(tree).map_err(WyrdCliError::CardLoad)?;
    Ok(CheckReport {
        ok: true,
        cards: check_cards(&input),
        diagnostics,
    })
}

/// Render `wyrd declare --check` over [`check_tree`].
///
/// A text-mode load failure also prints its diagnostics; in JSON mode they
/// travel only in the error document, so stdout stays empty.
///
/// # Errors
/// Returns `WyrdCliError::CardLoad` when loading or local declaration-input
/// construction fails, or an output error when JSON serialization fails.
fn dispatch_check(path: &Path, format: OutputFormat) -> Result<ExitCode, WyrdCliError> {
    let report = match check_tree(path) {
        Ok(report) => report,
        Err(WyrdCliError::CardLoad(error)) if format == OutputFormat::Text => {
            print_load_failure(&error);
            return Err(WyrdCliError::CardLoad(error));
        }
        Err(error) => return Err(error),
    };
    match format {
        OutputFormat::Text => print_check_text(&report),
        OutputFormat::Json => print_json(&report)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Load a local Card tree, declare it through the shared registry handle,
/// and return the declaration receipt.
///
/// The in-process form of `wyrd declare`; the receipt is exactly what
/// `wyrd declare --format json` prints. Local loading and validation complete
/// before declaration runs as `client`, or, when it is omitted, as a client
/// built from the ambient credential chain. Declaration then performs the
/// remote artifact and Card lifecycle operations owned by `wyrd_client::cards`,
/// and the server validates references authoritatively.
///
/// # Arguments
/// - `path`: a Card file or a bundle directory to declare.
/// - `client`: the principal to declare as; `None` resolves the ambient chain.
///
/// # Errors
/// Returns `WYRD_LOADER_400_INVALID_ENVELOPE` for invalid local input, a
/// `WYRD_CLIENT_*` error when configuration or credentials cannot be prepared,
/// or the server's stable error when declaration fails.
///
/// # Cancellation
/// Cancellation may stop the command during artifact transfer or server
/// completion. Retry behavior follows the registry operation's idempotency
/// contract.
pub async fn declare(
    path: impl AsRef<Path>,
    client: Option<WyrdClient>,
) -> Result<DeclarationReceipt, WyrdError> {
    declare_tree(path.as_ref(), || crate::client::explicit_or_ambient(client))
        .await
        .map_err(WyrdError::from)
}

/// Load, resolve, and declare one local Card tree.
///
/// `connect` supplies the client only after the tree loads and resolves, so
/// invalid local input never resolves a credential.
///
/// # Errors
/// Returns a CLI load error for invalid local input, the error of `connect`
/// when configuration or credentials cannot be prepared, or a server error
/// when declaration fails.
async fn declare_tree(
    path: &Path,
    connect: impl FnOnce() -> Result<WyrdClient, WyrdCliError>,
) -> Result<DeclarationReceipt, WyrdCliError> {
    let tree = load_tree(path).map_err(WyrdCliError::CardLoad)?;
    let input = build_declaration_input(tree).map_err(WyrdCliError::CardLoad)?;
    Cards::connect_with(&connect()?)
        .await?
        .declare(&input)
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Render `wyrd declare` over [`declare_tree`], or `--check` over
/// [`check_tree`] without contacting a server.
///
/// # Errors
/// Returns the errors of [`declare_tree`] or [`check_tree`], or an output
/// error when JSON serialization fails.
pub async fn dispatch_declare(args: DeclareArgs) -> Result<ExitCode, WyrdCliError> {
    if args.check {
        return dispatch_check(&args.path, args.format);
    }
    let server = args.connection.server.as_deref();
    let receipt = declare_tree(&args.path, || crate::client::from_global(server)).await?;
    match args.format {
        OutputFormat::Text => print_declare_text(&receipt),
        OutputFormat::Json => print_json(&receipt)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Read one Card envelope without downloading artifacts.
///
/// The in-process form of `wyrd get`; the envelope is exactly what
/// `wyrd get --format json` prints. The read runs as `client`, or, when it is
/// omitted, as a client built from the ambient credential chain.
///
/// # Arguments
/// - `selector`: the Card to read, by kind, name, and version, or by kind
///   and UID.
/// - `client`: the principal to read as; `None` resolves the ambient chain.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid selector, a `WYRD_CLIENT_*`
/// error when configuration or credentials fail, or the server's stable error
/// when the read fails.
pub async fn get(selector: &SelectorArgs, client: Option<WyrdClient>) -> Result<Card, WyrdError> {
    read_envelope(selector, || crate::client::explicit_or_ambient(client))
        .await
        .map_err(WyrdError::from)
}

/// Validate a selector, then read its one envelope.
///
/// The selector is checked before the client is built, so an invalid request
/// never resolves credentials.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector, the error of
/// `connect` when configuration or credentials fail, or a server error when
/// the read fails.
async fn read_envelope(
    selector: &SelectorArgs,
    connect: impl FnOnce() -> Result<WyrdClient, WyrdCliError>,
) -> Result<Card, WyrdCliError> {
    let selector = selector_from_args(selector, false)?;
    Cards::connect_with(&connect()?)
        .await?
        .get(selector)
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Render `wyrd get` over [`read_envelope`].
///
/// # Errors
/// Returns the errors of [`read_envelope`], or an output error when
/// serialization fails.
pub async fn dispatch_get(args: GetArgs) -> Result<ExitCode, WyrdCliError> {
    let server = args.connection.server.as_deref();
    let card = read_envelope(&args.selector, || crate::client::from_global(server)).await?;
    match args.format {
        OutputFormat::Text => print_get_text(&card)?,
        OutputFormat::Json => print_json(&card)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve a Card and pull its reachable graph into `output_dir`.
///
/// The in-process form of `wyrd pull`; the summary is exactly what
/// `wyrd pull --format json` prints. A complete pull is the default;
/// `metadata_only` writes an inspectable, non-runnable bundle without artifact
/// payloads. Reads run as `client`, or, when it is omitted, as a client built
/// from the ambient credential chain.
///
/// # Arguments
/// - `selector`: the Card to resolve, by kind, name, and version, or by
///   UID.
/// - `output_dir`: the directory the Card bundle is published to.
/// - `metadata_only`: write Card metadata without artifact payloads.
/// - `client`: the principal to read as; `None` resolves the ambient chain.
///
/// # Errors
/// Returns `WYRD_SPEC_400_VALIDATION` for an invalid selector, a `WYRD_CLIENT_*`
/// error when configuration or credentials fail, or the server's stable error
/// when reads or the pull fail.
///
/// # Cancellation
/// Cancellation may stop remote reads or artifact transfers after partial
/// staging progress. The pull workflow owns cleanup for returned errors; a
/// dropped task may require later staging cleanup.
pub async fn pull(
    selector: &SelectorArgs,
    output_dir: impl AsRef<Path>,
    metadata_only: bool,
    client: Option<WyrdClient>,
) -> Result<PullSummary, WyrdError> {
    pull_bundle(selector, output_dir.as_ref(), metadata_only, || {
        crate::client::explicit_or_ambient(client)
    })
    .await
    .map_err(WyrdError::from)
}

/// Validate a selector, then pull the selected graph.
///
/// The selector is checked before the client is built, so an invalid request
/// never resolves credentials.
///
/// # Errors
/// Returns a CLI argument error for an invalid selector, the error of
/// `connect` when configuration or credentials fail, or a server error when
/// reads or the pull fail.
async fn pull_bundle(
    selector: &SelectorArgs,
    output_dir: &Path,
    metadata_only: bool,
    connect: impl FnOnce() -> Result<WyrdClient, WyrdCliError>,
) -> Result<PullSummary, WyrdCliError> {
    let selector = selector_from_args(selector, false)?;
    let mode = if metadata_only {
        BundleMode::MetadataOnly
    } else {
        BundleMode::Complete
    };
    Cards::connect_with(&connect()?)
        .await?
        .pull(selector, output_dir, mode)
        .await
        .map_err(|source| WyrdCliError::Server { source })
}

/// Render `wyrd pull` over [`pull_bundle`].
///
/// # Errors
/// Returns the errors of [`pull_bundle`], or an output error when JSON
/// serialization fails.
pub async fn dispatch_pull(args: PullArgs) -> Result<ExitCode, WyrdCliError> {
    let server = args.connection.server.as_deref();
    let output = pull_bundle(&args.selector, &args.output_dir, args.metadata_only, || {
        crate::client::from_global(server)
    })
    .await?;
    match args.format {
        OutputFormat::Text => print_pull_text(&output),
        OutputFormat::Json => print_json(&output)?,
    }
    Ok(ExitCode::SUCCESS)
}

/// Resolve and print the latest Active Card version for a named identity.
///
/// The command parses the kind and name before asking the shared
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
    let name = parse_id("name", &args.name, "a valid Card name")?;
    let cards = build_cards(args.connection.server.as_deref()).await?;
    let card_ref = cards
        .resolve_latest(kind, name)
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
    let cards = build_cards(args.connection.server.as_deref()).await?;
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

/// Dispatch `wyrd delete`.
///
/// # Errors
/// Returns [`WyrdCliError::InvalidArgument`] for a selector that does not name
/// exactly one card, a connection error when the client cannot be built, and
/// [`WyrdCliError::Server`] when the server refuses the delete.
pub async fn dispatch_delete(args: DeleteArgs) -> Result<ExitCode, WyrdCliError> {
    let selector = selector_from_args(&args.selector, true)?;
    let cards = build_cards(args.connection.server.as_deref()).await?;
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

/// Connect the card-administration handle, optionally re-pointed at `server`.
///
/// Construction lives in [`crate::client`]: the ambient configuration is the
/// right default here, because `wyrd declare` and `wyrd get` administer the
/// deployment the user is already pointed at. Connecting completes the
/// credential exchange before any command runs.
///
/// # Errors
/// Returns the client-assembly errors documented on
/// [`crate::client::from_global`], or the server's refusal of the credential
/// exchange or a transport error when it cannot be reached.
async fn build_cards(server: Option<&str>) -> Result<Cards, WyrdCliError> {
    Ok(Cards::connect_with(&crate::client::from_global(server)?).await?)
}

/// Build the Card selector a `get`, `pull`, or `delete` invocation names.
///
/// `--uid` wins: it needs `--kind` and treats `--name` and `--version` as
/// assertions about the UID's Card. Otherwise the selector is the flat
/// `--kind`/`--name` line, narrowed to one Card by `--version`. A delete
/// (`require_exact_for_delete`) must name one exact Card, never a line.
///
/// # Errors
/// Returns an invalid-argument error for an unknown kind, an ill-formed
/// version, name, or UID, or a missing `--kind` or `--name`, and
/// [`WyrdCliError::DeleteSelectorRequiresExact`] for a delete naming neither a
/// UID nor a version.
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
        let name = args
            .name
            .as_deref()
            .map(|value| parse_id("name", value, "a valid Card name"))
            .transpose()?;
        return Ok(CardSelector::uid(kind, uid)
            .with_identity_assertions(name)
            .with_optional_version(version));
    }
    if require_exact_for_delete && version.is_none() {
        return Err(WyrdCliError::DeleteSelectorRequiresExact);
    }
    let kind = kind.ok_or_else(|| invalid_argument("kind", "<missing>", "required"))?;
    let name = args
        .name
        .as_deref()
        .ok_or_else(|| invalid_argument("name", "<missing>", "required for a named selector"))
        .and_then(|value| parse_id("name", value, "a valid Card name"))?;
    let selector = CardSelector::named(kind, name);
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

/// Parses a `--kind` argument by its wire name.
///
/// # Errors
/// Returns an invalid-argument error for a name that is not a known Card
/// kind.
fn parse_kind(value: &str) -> Result<CardKind, WyrdCliError> {
    CardKind::from_wire_name(value).ok_or_else(|| {
        invalid_argument(
            "kind",
            value,
            "a known Wyrd Card kind such as Model, Prompt, or Agent",
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

/// Project loaded submissions onto the `check` report's flat Card identities.
///
/// The version is the authored selection, or `None` when the server will
/// allocate one.
fn check_cards(input: &DeclarationInput) -> Vec<CheckCard> {
    input
        .submissions
        .iter()
        .map(|submission| CheckCard {
            kind: submission.kind.wire_name().to_owned(),
            name: submission.metadata.name.to_string(),
            version: submission
                .metadata
                .version
                .as_ref()
                .map(ToString::to_string),
        })
        .collect()
}

/// Print a passing `check` report for a person: one `Kind/name@version` line
/// per Card, with `auto` for a server-allocated version, then any warnings.
fn print_check_text(report: &CheckReport) {
    println!("check valid: {} card(s)", report.cards.len());
    for card in &report.cards {
        let version = card.version.as_deref().unwrap_or("auto");
        println!("  {}/{}@{version}", card.kind, card.name);
    }
    for diagnostic in &report.diagnostics {
        println!("warning {}: {}", diagnostic.code, diagnostic.message);
    }
}

/// Print a load failure's diagnostics for a person; JSON callers read the
/// same diagnostics from the error document's `details.diagnostics`.
fn print_load_failure(error: &LoadError) {
    for diagnostic in &error.diagnostics {
        println!(
            "{}: {} ({})",
            diagnostic.code,
            diagnostic.message,
            diagnostic.path.display()
        );
    }
}

/// Print a declaration receipt as one root line followed by one line per outcome.
fn print_declare_text(receipt: &DeclarationReceipt) {
    println!("declared: {}", receipt.root);
    for outcome in &receipt.outcomes {
        println!(
            "  {}: {:?} ({:?})",
            outcome.card_ref, outcome.outcome, outcome.status
        );
    }
}

/// Print one envelope as its YAML document.
///
/// # Errors
/// Returns [`WyrdCliError::Output`] when the envelope cannot be serialized.
fn print_get_text(card: &Card) -> Result<(), WyrdCliError> {
    let output = serde_yaml::to_string(card).map_err(|error| WyrdCliError::Output {
        detail: error.to_string(),
    })?;
    print!("{output}");
    Ok(())
}

fn print_pull_text(output: &PullSummary) {
    println!(
        "pulled: {} destination={} mode={} cards={} artifacts={} downloaded_artifacts={}",
        output.root,
        output.destination.display(),
        output.mode,
        output.card_count,
        output.artifact_count,
        output.downloaded_artifact_count
    );
}

/// Print one list page for a person: one `Kind/name@version status=…` line
/// per summary, then the continuation cursor when another page exists.
fn print_list_text(response: &ListCardsResponse) {
    for item in &response.items {
        println!(
            "{}/{}@{} status={:?}",
            item.kind.wire_name(),
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

    use super::{SelectorArgs, declare, declare_check, get, pull};

    /// A named selector whose kind is not a Card kind, refused before any
    /// client is built.
    fn unknown_kind_selector() -> SelectorArgs {
        SelectorArgs {
            kind: Some("NotAKind".to_owned()),
            name: Some("cli-prompt".to_owned()),
            version: None,
            uid: None,
        }
    }

    /// Check, declare, get, and pull return the value `--format json` prints,
    /// or a shared-catalog error, without writing output or producing an exit
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
            "apiVersion: wyrd/v1\nkind: Prompt\nmetadata:\n  name: cli-prompt\n  version: 1.0.0\nspec:\n  provider: openai\n  model: gpt-4o\n  messages:\n    - role: user\n      content: hello\n",
        )
        .expect("prompt card writes");
        let missing = temp.path().join("missing.yaml");

        let report = declare_check(&path).expect("a valid tree checks");
        assert_eq!(
            serde_json::to_value(&report).expect("check report serializes"),
            json!({
                "ok": true,
                "cards": [{
                    "kind": "Prompt",
                    "name": "cli-prompt",
                    "version": "1.0.0",
                }],
                "diagnostics": [],
            })
        );
        assert_eq!(
            declare_check(&missing)
                .expect_err("a missing tree is refused")
                .code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
        assert_eq!(
            declare(&missing, None)
                .await
                .expect_err("a missing tree is refused before declaration")
                .code(),
            "WYRD_LOADER_400_INVALID_ENVELOPE"
        );
        assert_eq!(
            get(&unknown_kind_selector(), None)
                .await
                .expect_err("an unknown kind is refused")
                .code(),
            "WYRD_SPEC_400_VALIDATION"
        );
        assert_eq!(
            pull(&unknown_kind_selector(), temp.path(), false, None)
                .await
                .expect_err("an unknown kind is refused")
                .code(),
            "WYRD_SPEC_400_VALIDATION"
        );
    }
}
