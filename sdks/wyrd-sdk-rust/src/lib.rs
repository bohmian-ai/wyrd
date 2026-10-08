//! First-class Rust Wyrd SDK.
//!
//! A thin projection of `wyrd-client`, the sole shared client implementation:
//! client configuration, [`cards::Cards`], [`principals::Principals`],
//! [`platform::Platform`], [`state::WyrdState`],
//! [`operator_connections::OperatorConnections`], and [`Bifrost`], plus the
//! [`WyrdError`] every capability returns. This
//! package adds no transport, validation, registry, storage, or lifecycle
//! behavior of its own, and never enables an owner crate's `python` feature.
//!
//! The projection is enumerated rather than a blanket re-export for one
//! reason: `wyrd_client::gateway_credential` mutates provider credentials —
//! submission, rotation, revocation, and deletion — and only the CLI and the
//! scoped MCP tool may do that. Submission can carry a managed secret
//! outright; revocation and deletion name a credential without saying which
//! source backs it, so neither can be offered here without also offering
//! `ManagedSecret` mutation. Leaving that module off the list below is the whole
//! enforcement: [`Gateway`] has no mutation method to call, so a caller here
//! reads redacted credentials, administers deployments and policies, and
//! invokes the gateway. `check:deps` fails if the module is added
//! back. Add a module to this list only after deciding it belongs on a public
//! SDK. Provider credential writes and Card-scoped key issuance are reachable
//! only as CLI commands. Their in-process form, `cli`, is a test surface
//! behind the `testing` feature and is absent from production builds.
//!
//! The list also holds the projection to the surface every SDK shares
//! (REQ-210): the `wyrd_client::workflow` module is left off, so the remote
//! `Workflows` handle and the gateway caller are not reachable here; Workflows
//! are YAML-authored through [`Workflow`]. The `auth` and `storage` modules
//! are left off too: the authentication middleware and the artifact-transfer
//! client are mechanics behind [`WyrdClient`] and [`cards::Cards`]. [`transport`]
//! carries only the endpoint and credential types [`config::ClientConfig`]
//! names. Raw requests, writer internals, and
//! bundle introspection on the re-exported types exist only under
//! `wyrd-client`'s `internal` feature, which this package never enables
//! (`check:deps` fails if it does).

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use wyrd_client::{
    Bifrost, Gateway, GlobalConfig, OperatorConnections, Platform, Principals, QueueConfig,
    Workflow, WorkflowCards, WyrdClient, bifrost, cards, client, config, environment, error,
    gateway, global_config, observe, operator_connections, platform, principals, state,
};

/// Endpoint and credential types the client configuration names.
///
/// Only the types a parity signature uses: [`config::ClientConfig`]'s `grpc`
/// and `http` fields and its `resolve_credential` result. The transports
/// themselves are mechanics behind [`WyrdClient`] and are not projected.
pub mod transport {
    pub use wyrd_client::transport::{GrpcConfig, HttpConfig, ResolvedCredential};
}

#[cfg(feature = "otel")]
pub mod otel;

/// In-process `wyrd` CLI commands: a test surface behind the `testing`
/// feature.
///
/// Each function runs the same command implementation as the `wyrd`
/// executable, takes the command's options as typed arguments, returns the
/// value the command prints with `--format json`, and returns a
/// [`WyrdError`] instead of an exit code. A networked
/// command takes an optional [`WyrdClient`] and runs as
/// its principal; omitted, it resolves the server and credential from the
/// ambient chain exactly as the executable does.
#[cfg(feature = "testing")]
pub mod cli {
    pub use wyrd_cli::commands::*;
}

/// The stable, catalogued error every SDK capability returns.
pub use wyrd_spec::error::WyrdError;

/// A registered Card envelope, as [`cards::Cards::get`] returns it, and its
/// typed spec.
pub use wyrd_spec::envelope::{Card, Spec};

/// How a Verifier Card judges: an Eval or a Drift implementation.
pub use wyrd_spec::card::verifier::VerifierImplementation;

/// The record of one Workflow run, as [`Workflow::run`] returns it, and its
/// final status.
pub use wyrd_spec::card::workflow::{WorkflowRun, WorkflowRunStatus};

/// The count rollup a [`Judgment`] carries.
pub use wyrd_spec::card::operator::VerifierCounts;

/// The judgment `observe().verify` returns, with its verdict and Verifier
/// classification.
pub use wyrd_spec::verification::{Judgment, VerificationVerdict, VerifierKind};

/// The session an emitted observation belongs to.
pub use wyrd_spec::vala::ids::SessionId;

/// The outcome the terminal frame of a [`bifrost::QueryResultStream`]
/// reports.
pub use wyrd_spec::vala::api::QueryTerminalOutcome;

/// SDK root re-export shape.
#[cfg(test)]
mod tests {
    use super::bifrost::{CompactionTypeWire, TableConfig};

    /// The SDK root names every composed client capability from `wyrd-client`
    /// and the error they return.
    #[test]
    fn sdk_root_exposes_the_supported_surface() {
        let _ = super::Bifrost::connect;
        let _ = super::cards::Cards::with_client;
        let _ = super::principals::Principals::with_client;
        let _ = super::operator_connections::OperatorConnections::with_client;
        let _ = super::platform::Platform::with_client;
        let _ = |path: &std::path::Path| super::state::WyrdState::from_path(path);
        let _ = |path: &std::path::Path, client| {
            super::state::WyrdState::from_path_with_client(path, client)
        };
        let _ = super::WyrdClient::with_config;
        let _ = super::Gateway::with_client;
        let _ = super::Gateway::from_env;
        let _ = super::Workflow::from_yaml;
        let _ = super::Workflow::steps;
        let _ = super::cards::Cards::workflow;
        let _ = super::WyrdError::code;
    }

    /// A Rust SDK user names every compaction type and declares it on a
    /// table config through `wyrd_sdk::bifrost` alone, with no direct
    /// `wyrd-spec` dependency.
    ///
    /// # Panics
    ///
    /// Panics when the one-column JSON schema is rejected or a config does not
    /// report the compaction type it was given.
    #[test]
    fn sdk_bifrost_names_the_compaction_type() {
        for kind in [
            CompactionTypeWire::Auto,
            CompactionTypeWire::Full,
            CompactionTypeWire::SmallFiles,
            CompactionTypeWire::FilesWithDelete,
        ] {
            let config = TableConfig::from_json_schema(
                "vala.datasets.events",
                &serde_json::json!({
                    "type": "object",
                    "properties": {"id": {"type": "string"}},
                    "required": ["id"]
                }),
            )
            .expect("a one-column JSON schema is a valid table config")
            .with_compaction_type(kind);
            assert_eq!(config.compaction_type(), Some(kind));
        }
    }
}
