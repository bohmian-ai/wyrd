//! CLI-boundary errors.

use thiserror::Error;
use wyrd_error_derive::WyrdError;

/// Errors raised by the `wyrd` binary.
#[derive(Debug, Error, WyrdError)]
pub enum WyrdCliError {
    /// Local startup failed after zero or more committed server-owned stages.
    #[error("local server {stage} failed: {recovery}")]
    #[wyrd_error(
        code = "WYRD_CLI_503_SERVER_DEV",
        status = 503,
        title = "Local server setup failed",
        remediation = "Follow the stage's recovery guidance and rerun with the same tenant and configuration directory; active tenant credentials are never reissued."
    )]
    ServerDev {
        /// Failed lifecycle stage, never a URL or credential.
        stage: &'static str,
        /// Static recovery guidance that identifies retained partial state.
        recovery: &'static str,
    },
    /// Required Oracle client configuration was not supplied.
    #[error("query configuration is missing {field}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_QUERY_CONFIG",
        status = 400,
        title = "Query configuration missing",
        remediation = "Pass --server or set WYRD_SERVER_URL; the credential comes from the ambient chain."
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

    /// A Wyrd route returned a stable Wyrd error.
    ///
    /// The shared client has already mapped the response onto the catalog, so
    /// this variant carries that error through unchanged rather than inventing a
    /// CLI-local status-shaped one. Every command that calls a Wyrd route
    /// reports its server failures here.
    #[error(transparent)]
    #[wyrd_error(delegate)]
    Server {
        /// Stable catalog value the server reported.
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

    /// A platform command ran without the deployment's platform credential.
    ///
    /// The credential is read only from the environment so it never enters
    /// argv or shell history.
    #[error("no platform credential available")]
    #[wyrd_error(
        code = "WYRD_CLI_401_NO_PLATFORM_CREDENTIAL",
        status = 401,
        title = "No platform credential",
        remediation = "Set WYRD_PLATFORM_CREDENTIAL to a platform credential for this deployment."
    )]
    NoPlatformCredential,

    /// `wyrd auth refresh` ran without a refresh token.
    ///
    /// The refresh token is read only from the environment so it never enters
    /// argv or shell history.
    #[error("no refresh token available")]
    #[wyrd_error(
        code = "WYRD_CLI_401_NO_REFRESH_TOKEN",
        status = 401,
        title = "No refresh token",
        remediation = "Set WYRD_REFRESH_TOKEN to the refresh token to rotate."
    )]
    NoRefreshToken,

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

    /// This host's OS and CPU have no official Wyrd server bundle.
    #[error("no official wyrd-server bundle for {os}/{arch}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_SERVER_HOST_UNSUPPORTED",
        status = 400,
        title = "Server host unsupported",
        remediation = "Run the server on macOS on aarch64 or Linux (glibc) on x86_64 or aarch64, or use the official container image."
    )]
    ServerHostUnsupported {
        /// Detected operating system.
        os: &'static str,
        /// Detected CPU architecture.
        arch: &'static str,
    },

    /// No stable release, or no bundle for this host, could be fetched.
    #[error("server release unavailable: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE",
        status = 503,
        title = "Server release unavailable",
        remediation = "Check network access to github.com, then retry; the current installation is unchanged."
    )]
    ServerReleaseUnavailable {
        /// What could not be fetched.
        detail: String,
    },

    /// A downloaded release failed signature, digest, or extraction checks.
    #[error("server release failed verification: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED",
        status = 422,
        title = "Server release unverified",
        remediation = "Retry the install; if it fails again, report the release. The current installation is unchanged."
    )]
    ServerReleaseUnverified {
        /// Which check failed.
        detail: String,
    },

    /// The newest release is already extracted locally but lacks part of its
    /// runnable payload, so installing would only reactivate it.
    #[error(
        "server installation {} is incomplete ({detail}); remove that directory, then rerun `wyrd server install` to download it again",
        dir.display()
    )]
    #[wyrd_error(
        code = "WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE",
        status = 409,
        title = "Server installation incomplete",
        remediation = "Remove the named version directory, then rerun `wyrd server install`; nothing was activated."
    )]
    ServerInstallIncomplete {
        /// Extracted version directory that is not runnable.
        dir: std::path::PathBuf,
        /// Which payload part is missing or empty.
        detail: String,
    },

    /// The newest server release does not share this CLI's compatible version line.
    #[error("wyrd CLI {cli} cannot run wyrd-server {server}")]
    #[wyrd_error(
        code = "WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE",
        status = 409,
        title = "Server version incompatible",
        remediation = "Upgrade the wyrd CLI to the server's version line (pip install -U wyrd, or npm install wyrd-ai@latest), then retry."
    )]
    ServerVersionIncompatible {
        /// This CLI's version.
        cli: semver::Version,
        /// The server release's version.
        server: semver::Version,
    },

    /// `wyrd mcp install` had no host to configure.
    ///
    /// A non-interactive run never selects a host implicitly, and an
    /// interactive run needs at least one detected host to offer.
    #[error("MCP host selection required: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_400_MCP_HOST_SELECTION",
        status = 400,
        title = "MCP host selection required",
        remediation = "Pass --host for each host to connect (codex, claude-code, copilot-cli, vscode, cursor, pi, hermes); install and open the host first if none is detected."
    )]
    McpHostSelection {
        /// Which hosts were detected, or that none was.
        detail: String,
    },

    /// One or more selected MCP hosts could not be connected.
    ///
    /// Each host's reason is printed on its own line; the failed hosts' files
    /// are unchanged and every other selected host was installed.
    #[error("MCP host installation failed for {hosts}")]
    #[wyrd_error(
        code = "WYRD_CLI_409_MCP_HOST_INSTALL",
        status = 409,
        title = "MCP host installation failed",
        remediation = "Fix the problem reported for each failed host, then rerun; hosts reported as added, updated, or unchanged are already connected."
    )]
    McpHostInstall {
        /// Names of the hosts that failed.
        hosts: String,
    },

    /// `wyrd mcp proxy` could not establish the host or Wyrd MCP session.
    #[error("MCP proxy failed: {detail}")]
    #[wyrd_error(
        code = "WYRD_CLI_503_MCP_PROXY",
        status = 503,
        title = "MCP proxy failed",
        remediation = "Check that the Wyrd server URL is reachable and serves /mcp, and that the host launches `wyrd mcp proxy` over stdio, then reconnect the host."
    )]
    McpProxy {
        /// Which session failed and why, without credential values.
        detail: String,
    },
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

/// Projects a CLI failure onto the shared catalog for in-process callers.
///
/// The in-process command functions surface in Rust, Python, and TypeScript,
/// where only the shared [`wyrd_spec::error::WyrdError`] catalog crosses the
/// boundary. Server failures pass through unchanged; client-assembly failures
/// land on their `WYRD_CLIENT_*` equivalents; local loader failures keep their
/// diagnostics as structured details; argument and input failures become
/// request validation; and the remaining local faults become internal errors.
/// The executable keeps rendering the CLI's own codes.
impl From<WyrdCliError> for wyrd_spec::error::WyrdError {
    fn from(error: WyrdCliError) -> Self {
        use wyrd_spec::error::WyrdError;
        let message = error.to_string();
        let details = serde_json::json!({});
        match error {
            WyrdCliError::Server { source } => source,
            WyrdCliError::NoCredentials
            | WyrdCliError::NoPlatformCredential
            | WyrdCliError::NoRefreshToken => WyrdError::ClientNoCredentials { message, details },
            WyrdCliError::ClientConfig { .. } => {
                WyrdError::ClientConfigInvalid { message, details }
            }
            WyrdCliError::ClientTransport { .. }
            | WyrdCliError::ServerDev { .. }
            | WyrdCliError::ServerReleaseUnavailable { .. }
            | WyrdCliError::McpProxy { .. } => WyrdError::ClientTransportDown { message, details },
            WyrdCliError::CardLoad(error) => WyrdError::from(error),
            WyrdCliError::InvalidArgument { field, .. } => WyrdError::Validation {
                message,
                details: serde_json::json!({ "field": field }),
            },
            WyrdCliError::QueryConfig { .. }
            | WyrdCliError::DeleteSelectorRequiresExact
            | WyrdCliError::ServerHostUnsupported { .. }
            | WyrdCliError::ServerVersionIncompatible { .. }
            | WyrdCliError::McpHostSelection { .. } => WyrdError::Validation { message, details },
            WyrdCliError::Query { .. }
            | WyrdCliError::Io { .. }
            | WyrdCliError::Output { .. }
            | WyrdCliError::ServerReleaseUnverified { .. }
            | WyrdCliError::ServerInstallIncomplete { .. }
            | WyrdCliError::McpHostInstall { .. } => WyrdError::Internal { message, details },
        }
    }
}

/// Error crossing the binary boundary from either local CLI work or Oracle query SDK work.
#[derive(Debug)]
pub enum CliBoundaryError {
    /// A derive-catalogued failure owned by the local CLI.
    Local(WyrdCliError),
    /// A server or shared-client failure already projected onto the shared
    /// catalog, such as an Oracle query or gateway administration error.
    ///
    /// The SDK owns that projection, so the binary boundary reads its stable
    /// metadata straight off the catalog error instead of keeping a second
    /// per-variant table here.
    Remote(wyrd_spec::error::WyrdError),
}

impl From<WyrdCliError> for CliBoundaryError {
    /// Preserves an existing local CLI error at the binary boundary.
    fn from(error: WyrdCliError) -> Self {
        Self::Local(error)
    }
}

impl From<wyrd_client::bifrost::BifrostClientError> for CliBoundaryError {
    /// Projects an originating query SDK error onto the shared catalog.
    fn from(error: wyrd_client::bifrost::BifrostClientError) -> Self {
        Self::Remote(wyrd_spec::error::WyrdError::from(error))
    }
}

impl From<wyrd_spec::error::WyrdError> for CliBoundaryError {
    /// Preserves an originating shared-catalog error from a client handle.
    fn from(error: wyrd_spec::error::WyrdError) -> Self {
        Self::Remote(error)
    }
}

impl CliBoundaryError {
    /// Prints one machine-readable problem followed by one human-readable stderr line.
    ///
    /// The method performs exactly two stderr writes and never writes stdout,
    /// mutates durable state, or changes the originating error metadata.
    pub fn print(&self) {
        let json = serde_json::json!({
            "kind": "wyrd_cli_error",
            "code": self.code(),
            "status": self.status(),
            "title": self.title(),
            "message": self.detail(),
            "remediation": self.remediation(),
            "details": self.details(),
        });
        eprintln!("{json}");
        eprintln!("error: {}", self.detail());
    }

    /// Returns the originating stable error code.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Local(error) => error.code(),
            Self::Remote(error) => error.code(),
        }
    }

    /// Returns the originating HTTP-equivalent status.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            Self::Local(error) => error.status(),
            Self::Remote(error) => error.status(),
        }
    }

    /// Returns the originating stable title.
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::Local(error) => error.title(),
            Self::Remote(error) => error.title(),
        }
    }

    /// Returns the scrubbed human-readable detail.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::Local(error) => error.to_string(),
            Self::Remote(error) => error
                .as_problem_json()
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("query failed")
                .to_owned(),
        }
    }

    /// Returns the originating remediation.
    #[must_use]
    pub fn remediation(&self) -> &str {
        match self {
            Self::Local(error) => error.remediation(),
            Self::Remote(error) => error.remediation(),
        }
    }

    /// Returns JSON-safe structured diagnostics when supplied by the source.
    #[must_use]
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            Self::Local(_) => None,
            Self::Remote(error) => error.as_problem_json().get("details").cloned(),
        }
    }

    /// Returns the process exit code associated with the boundary status.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Local(error) => error.exit_code(),
            Self::Remote(error) => match error.status() {
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
        let error = CliBoundaryError::from(wyrd_client::bifrost::BifrostClientError::Transport(
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
