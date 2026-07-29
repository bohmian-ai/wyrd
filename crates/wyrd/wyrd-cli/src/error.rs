//! CLI-boundary errors.

use thiserror::Error;
use wyrd_error_derive::WyrdError;

/// Errors raised by the `wyrd` binary.
#[derive(Debug, Error, WyrdError)]
pub enum WyrdCliError {
    /// Client-delegated simulated-user mode was requested without a script.
    #[error("--simulated-user client requires --simulated-user-script <PATH>")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SIMULATED_USER_SCRIPT_REQUIRED",
        status = 400,
        title = "Missing simulated-user script",
        remediation = "Pass --simulated-user-script <PATH> when using --simulated-user client."
    )]
    SimulatedUserScriptRequired,

    /// A scripted simulated-user turn was missing.
    #[error("scripted-user JSONL missing turn {turn} for scenario {scenario_id}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SCRIPTED_TURN_MISSING",
        status = 400,
        title = "Scripted simulated-user turn missing",
        remediation = "Add a JSONL line with the missing turn and message."
    )]
    ScriptedTurnMissing {
        /// Missing scenario id.
        scenario_id: String,
        /// Missing turn cursor.
        turn: u32,
    },

    /// Registry reference loading is not implemented for this command yet.
    #[error("registry-resolved eval refs are not supported yet; pass a card file path")]
    #[wyrd_error(
        code = "WYRD_CLI_400_REGISTRY_REF_UNSUPPORTED",
        status = 400,
        title = "Registry ref resolution is not implemented",
        remediation = "Pass a filesystem path to an Eval card YAML or JSON file."
    )]
    RegistryRefUnsupported,

    /// Server mode requires an agent URL.
    #[error("--server requires --agent-url")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SERVER_REQUIRES_AGENT_URL",
        status = 400,
        title = "Server mode requires an agent URL",
        remediation = "Pass --agent-url <URL> when using --server."
    )]
    ServerRequiresAgentUrl,

    /// Server mode requires a Wyrd access token.
    #[error("--server requires --token (or WYRD_ACCESS_TOKEN)")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SERVER_REQUIRES_TOKEN",
        status = 400,
        title = "Server mode requires an access token",
        remediation = "Pass --token <JWT> or set WYRD_ACCESS_TOKEN when using --server."
    )]
    ServerRequiresToken,

    /// Server mode does not accept pre-collected records.
    #[error("--server is incompatible with --records")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SERVER_REJECTS_RECORDS",
        status = 400,
        title = "Server mode rejects records",
        remediation = "Use --records without --server, or use --server with --agent-url."
    )]
    ServerRejectsRecords,

    /// Local record replay requires the publisher's subject identity.
    #[error("--records requires --subject <CARD_REF>")]
    #[wyrd_error(
        code = "WYRD_CLI_400_RECORDS_REQUIRE_SUBJECT",
        status = 400,
        title = "Record replay requires a subject",
        remediation = "Pass the exact card reference of the publisher that emitted the records."
    )]
    RecordsRequireSubject,

    /// LLM judge tasks require the deterministic mock until provider wiring lands.
    #[error("LLM judge tasks require --judge-mock")]
    #[wyrd_error(
        code = "WYRD_CLI_400_JUDGE_MOCK_REQUIRED",
        status = 400,
        title = "Judge mock required",
        remediation = "Pass --judge-mock for Eval specs containing LLM judge tasks."
    )]
    JudgeMockRequired,

    /// Eval card path had an unsupported extension.
    #[error("eval card file extension is not supported: {path}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED",
        status = 400,
        title = "Eval card extension unsupported",
        remediation = "Use .json, .yaml, or .yml for Eval card files."
    )]
    CardExtensionUnsupported {
        /// Display path.
        path: String,
    },

    /// Eval card file did not contain an Eval card.
    #[error("card file must contain kind: Eval")]
    #[wyrd_error(
        code = "WYRD_CLI_400_NOT_EVAL_CARD",
        status = 400,
        title = "Card file is not an Eval card",
        remediation = "Pass a Wyrd card envelope with kind: Eval."
    )]
    NotEvalCard,

    /// Records JSONL parsing failed.
    #[error("records JSONL parse failed for {path}: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_RECORDS_PARSE",
        status = 400,
        title = "Records JSONL parse failed",
        remediation = "Verify each non-empty line is one EvalRecordObservation JSON object."
    )]
    RecordsParse {
        /// Path being parsed.
        path: String,
        /// Parser detail.
        detail: String,
    },

    /// Generic JSON/YAML parse failure.
    #[error("{kind} parse failed for {path}: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_PARSE",
        status = 400,
        title = "CLI input parse failed",
        remediation = "Check the file against the expected Wyrd YAML or JSON shape."
    )]
    Parse {
        /// Input kind.
        kind: &'static str,
        /// Path being parsed.
        path: String,
        /// Parser detail.
        detail: String,
    },

    /// Eval spec validation failed.
    #[error("eval spec validation failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_EVAL_SPEC_INVALID",
        status = 400,
        title = "Eval spec invalid",
        remediation = "Fix the Eval card spec before running the evaluation."
    )]
    EvalSpecInvalid {
        /// Validation detail.
        detail: String,
    },

    /// Database pool, connection, or query failed.
    #[error("database error: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_DATABASE",
        status = 500,
        title = "Database error",
        remediation = "Check WYRD_DATABASE_URL, database connectivity, and credentials."
    )]
    Database {
        /// Error detail.
        detail: String,
    },

    /// Database migration failed.
    #[error("migration failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_MIGRATION",
        status = 500,
        title = "Database migration failed",
        remediation = "Inspect the migration error and ensure the migrator DSN has the required privileges."
    )]
    Migration {
        /// Error detail.
        detail: String,
    },

    /// Embedded Postgres did not return a platform-admin DSN.
    #[error("embedded postgres did not return a platform-admin DSN")]
    #[wyrd_error(
        code = "WYRD_CLI_500_MISSING_ADMIN_DSN",
        status = 500,
        title = "Missing platform-admin DSN",
        remediation = "Ensure embedded Postgres is configured to emit a platform-admin DSN, or supply WYRD_PLATFORM_ADMIN_DSN."
    )]
    MissingAdminDsn,

    /// API key hashing failed.
    #[error("api key hashing failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_HASHING",
        status = 500,
        title = "API key hashing failed",
        remediation = "This is an internal error; retry or check the wyrd-auth-issue configuration."
    )]
    Hashing {
        /// Error detail.
        detail: String,
    },

    /// Filesystem IO failed.
    #[error("io failed: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_IO",
        status = 500,
        title = "CLI filesystem IO failed",
        remediation = "Check file paths and permissions, then retry."
    )]
    Io {
        /// Source error.
        #[source]
        source: std::io::Error,
    },

    /// HTTP client construction failed.
    #[error("could not build HTTP client: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_HTTP_BUILD",
        status = 500,
        title = "HTTP client build failed",
        remediation = "Retry with valid HTTP settings."
    )]
    HttpBuild {
        /// Source error.
        #[source]
        source: reqwest::Error,
    },

    /// HTTP request failed.
    #[error("HTTP request failed: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_502_HTTP",
        status = 502,
        title = "HTTP request failed",
        remediation = "Check the server or agent endpoint and retry."
    )]
    Http {
        /// Source error.
        #[source]
        source: reqwest::Error,
    },

    /// URL join failed.
    #[error("URL join failed: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_URL_JOIN",
        status = 400,
        title = "URL construction failed",
        remediation = "Pass a valid base URL."
    )]
    UrlJoin {
        /// Source error.
        #[source]
        source: url::ParseError,
    },

    /// Auth request failed (login, refresh, callback).
    #[error("auth request failed: status={status}, detail={detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_401_AUTH_FAILED",
        status = 401,
        title = "Auth request failed",
        remediation = "Check the server URL, credentials, and network connectivity."
    )]
    AuthFailed {
        /// HTTP status returned by the server (0 = client-side parse failure).
        status: u16,
        /// Response body or client error detail.
        detail: String,
    },

    /// Principal revoke request failed.
    #[error("principal revoke failed: status={status}, detail={detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_REVOKE_FAILED",
        status = 500,
        title = "Principal revoke failed",
        remediation = "Check the principal ID, kind, access token, and server connectivity."
    )]
    RevokeFailed {
        /// HTTP status returned by the server.
        status: u16,
        /// Response body or error detail.
        detail: String,
    },

    /// Admin operation failed (trusted-issuer or workload-binding management).
    #[error("admin operation failed: status={status}, detail={detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_ADMIN_FAILED",
        status = 500,
        title = "Admin operation failed",
        remediation = "Check the request parameters, access token, and server connectivity."
    )]
    AdminFailed {
        /// HTTP status returned by the server (400 for local validation failures).
        status: u16,
        /// Response body or error detail.
        detail: String,
    },

    /// API key issuance request failed.
    #[error("api key issuance failed: status={status}, detail={detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_ISSUE_KEY_FAILED",
        status = 500,
        title = "API key issuance failed",
        remediation = "Check the card ref, access token, and server connectivity."
    )]
    IssueKeyFailed {
        /// HTTP status returned by the server (400 for local validation failures).
        status: u16,
        /// Response body or error detail.
        detail: String,
    },

    /// Eval engine failed.
    #[error("eval engine failed: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_EVAL_ENGINE",
        status = 500,
        title = "Eval engine failed",
        remediation = "Inspect the Eval spec, records, scenarios, and CLI logs."
    )]
    EvalEngine {
        /// Source error.
        #[source]
        source: vala_eval::EvalExecError,
    },

    /// Orchestrator failed.
    #[error("eval orchestrator failed: {source}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_ORCHESTRATOR",
        status = 500,
        title = "Eval orchestrator failed",
        remediation = "Inspect agent endpoint responses and scenario configuration."
    )]
    Orchestrator {
        /// Source error.
        #[source]
        source: vala_eval::orchestrator::OrchestratorError,
    },

    /// A card CLI argument could not be parsed or does not satisfy the
    /// selector contract.
    #[error("invalid {field} {value:?}: {expected}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_INVALID_ARGUMENT",
        status = 400,
        title = "Invalid CLI argument",
        remediation = "Fix the argument and retry."
    )]
    InvalidArgument {
        /// Argument name.
        field: String,
        /// Supplied value.
        value: String,
        /// Expected shape.
        expected: String,
    },

    /// The local card loader rejected a card tree.
    #[error("card load failed: {0}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_CARD_LOAD",
        status = 400,
        title = "Card load failed",
        remediation = "Fix the card files and local references, then retry."
    )]
    CardLoad(#[source] wyrd_loader::LoadError),

    /// A registry operation returned a stable Wyrd error.
    #[error(transparent)]
    #[wyrd_error(delegate)]
    Registry {
        /// Registry error catalog value.
        #[from]
        source: wyrd_spec::error::WyrdError,
    },

    /// The CLI could not construct its typed client from local configuration.
    #[error("client configuration failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_CLIENT_CONFIG",
        status = 400,
        title = "Client configuration failed",
        remediation = "Pass a valid server URL and configure a Wyrd credential."
    )]
    ClientConfig {
        /// Configuration detail without secret values.
        detail: String,
    },

    /// No credential was available for a networked card operation.
    #[error("no Wyrd credentials available")]
    #[wyrd_error(
        code = "WYRD_CLI_401_NO_CREDENTIALS",
        status = 401,
        title = "No Wyrd credentials",
        remediation = "Set WYRD_ACCESS_TOKEN, configure workload identity or WYRD_API_KEY, or add [default].api_key to ~/.config/wyrd/credentials.toml."
    )]
    NoCredentials,

    /// The card client could not reach the configured server.
    #[error("card client transport failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_503_CLIENT_TRANSPORT",
        status = 503,
        title = "Wyrd server unavailable",
        remediation = "Check the server URL and network connectivity, then retry."
    )]
    ClientTransport {
        /// Transport detail without credential values.
        detail: String,
    },

    /// A successful card response could not be rendered in the requested
    /// machine-readable format.
    #[error("card output serialization failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_OUTPUT",
        status = 500,
        title = "CLI output failed",
        remediation = "Retry the command or use --format text."
    )]
    Output {
        /// Serialization detail.
        detail: String,
    },

    /// A delete selector must identify one exact card version.
    #[error(
        "delete requires --uid or an explicit --version; named selectors never default to latest"
    )]
    #[wyrd_error(
        code = "WYRD_CLI_400_DELETE_SELECTOR_EXACT",
        status = 400,
        title = "Delete selector is not exact",
        remediation = "Pass --uid, or pass --kind, --space, --name, and --version."
    )]
    DeleteSelectorRequiresExact,
}

impl WyrdCliError {
    /// Process exit code for this error.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.status() {
            400 => 64,
            401 | 403 => 77,
            404 => 66,
            422 => 65,
            409 => 73,
            410 | 507 => 74,
            502 | 503 => 69,
            _ => 1,
        }
    }
}
