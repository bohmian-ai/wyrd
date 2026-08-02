//! CLI-boundary errors.

use thiserror::Error;
use wyrd_error_derive::WyrdError;

/// Errors raised by the `wyrd` binary.
#[derive(Debug, Error, WyrdError)]
pub enum WyrdCliError {
    /// Required Oracle client configuration was not supplied.
    #[error("query configuration is missing {field}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_QUERY_CONFIG",
        status = 400,
        title = "Query configuration missing",
        remediation = "Pass --server and --token or set WYRD_SERVER_URL and WYRD_ACCESS_TOKEN."
    )]
    QueryConfig {
        /// Missing command field.
        field: &'static str,
    },

    /// Oracle query setup, streaming, terminal validation, or output failed.
    #[error("query failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_500_QUERY",
        status = 500,
        title = "Oracle query failed",
        remediation = "Inspect the stable query error and terminal diagnostics, then retry."
    )]
    Query {
        /// Scrubbed query failure detail.
        detail: String,
    },

    /// `wyrd dev bootstrap` rejected because `WYRD_DATABASE_URL` points to a
    /// non-loopback host and `--i-understand-this-is-not-production` was not set.
    #[error(
        "WYRD_DATABASE_URL points to a non-loopback host ({host}); \
         wyrd dev bootstrap refuses to seed against non-local databases. \
         Pass --i-understand-this-is-not-production to override."
    )]
    #[wyrd_error(
        code = "WYRD_CLI_400_NON_LOOPBACK_DSN",
        status = 400,
        title = "Non-loopback database DSN rejected",
        remediation = "Unset WYRD_DATABASE_URL to use embedded Postgres, or pass --i-understand-this-is-not-production to override."
    )]
    NonLoopbackDsn {
        /// The host that was detected as non-loopback.
        host: String,
    },

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
}

impl WyrdCliError {
    /// Process exit code for this error.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self.status() {
            400 => 64,
            422 => 65,
            _ => 1,
        }
    }
}

/// Error crossing the binary boundary from either local CLI work or Oracle query SDK work.
#[derive(Debug)]
pub enum CliBoundaryError {
    /// A derive-catalogued failure owned by the local CLI.
    Local(WyrdCliError),
    /// A typed Oracle query failure projected without recataloguing it.
    Query(vala_sdk::ValaSdkError),
}

impl From<WyrdCliError> for CliBoundaryError {
    /// Preserves an existing local CLI error at the binary boundary.
    fn from(error: WyrdCliError) -> Self {
        Self::Local(error)
    }
}

impl From<vala_sdk::ValaSdkError> for CliBoundaryError {
    /// Preserves an originating query SDK error at the binary boundary.
    fn from(error: vala_sdk::ValaSdkError) -> Self {
        Self::Query(error)
    }
}

impl CliBoundaryError {
    /// Returns the originating stable error code.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Local(error) => error.code(),
            Self::Query(error) => error.code(),
        }
    }

    /// Returns the originating HTTP-equivalent status.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Local(error) => error.status(),
            Self::Query(error) => error.status(),
        }
    }

    /// Returns the originating stable title.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::Local(error) => error.title(),
            Self::Query(error) => error.title(),
        }
    }

    /// Returns the scrubbed human-readable detail.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Local(error) => error.to_string(),
            Self::Query(error) => error.detail(),
        }
    }

    /// Returns the originating remediation.
    #[must_use]
    pub fn remediation(&self) -> &str {
        match self {
            Self::Local(error) => error.remediation(),
            Self::Query(error) => error.remediation(),
        }
    }

    /// Returns JSON-safe structured diagnostics when supplied by the source.
    #[must_use]
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Local(_) => None,
            Self::Query(error) => error.safe_details(),
        }
    }

    /// Returns the process exit code associated with the boundary status.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Local(error) => error.exit_code(),
            Self::Query(error) => match error.status() {
                400 => 64,
                422 => 65,
                _ => 1,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use wyrd_spec::error::WyrdError;

    use super::*;

    /// Query boundary errors retain source metadata and bypass the CLI query catalog.
    #[test]
    fn query_boundary_preserves_originating_problem_metadata() {
        let error = CliBoundaryError::Query(vala_sdk::ValaSdkError::Transport(
            WyrdError::PermissionDeniedRbac {
                message: "query denied".to_owned(),
                details: serde_json::json!({"required_scope": "bifrost_query:read"}),
            },
        ));
        assert_eq!(error.code(), "WYRD_PERMISSION_403_DENIED_RBAC");
        assert_eq!(error.status(), 403);
        assert_eq!(error.title(), "Permission denied (RBAC)");
        assert_eq!(error.detail(), "query denied");
        assert_eq!(
            error.details(),
            Some(serde_json::json!({"required_scope": "bifrost_query:read"}))
        );
    }
}
