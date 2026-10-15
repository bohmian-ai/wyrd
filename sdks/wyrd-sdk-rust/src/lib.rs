//! First-class Rust Wyrd SDK.
//!
//! A thin projection of `wyrd-client`, the sole shared client implementation:
//! client configuration, [`cards::Cards`], [`auth::Principals`],
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
//! (REQ-210): the `wyrd_client::gateway_caller` module is left off, so the
//! Agent gateway caller is not reachable here. The `wyrd_client::auth` and
//! `storage` modules are left off too (this package's [`auth`] is the principal
//! handle, `wyrd_client::principals`): the authentication middleware and the artifact-transfer
//! client are mechanics behind [`WyrdClient`] and [`cards::Cards`]. [`transport`]
//! carries only the endpoint and credential types [`config::ClientConfig`]
//! names. Raw requests, writer internals, and
//! bundle introspection on the re-exported types exist only under
//! `wyrd-client`'s `internal` feature, which this package never enables
//! (`check:deps` fails if it does).
//!
//! Every server-facing handle opens with `connect()`, which resolves the
//! ambient client, or `connect_with(&client)`, which uses the given one; no
//! other construction form is public, and the principal handle lives in
//! [`auth`], not a `principals` module:
//!
//! ```compile_fail
//! use wyrd_ai::principals::Principals;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Bifrost::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Bifrost::connect_with_table;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Bifrost::connect_with_config;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::bifrost::blocking::Bifrost::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::bifrost::blocking::Bifrost::connect_with_table;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::bifrost::blocking::Bifrost::connect_with_config;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::cards::Cards::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::cards::Cards::with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Gateway::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Gateway::with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::auth::Principals::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::auth::Principals::with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::OperatorConnections::from_env;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::OperatorConnections::with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::Platform::with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::state::WyrdState::from_path_with_client;
//! ```
//!
//! ```compile_fail
//! let _ = wyrd_ai::bifrost::TableConfig::describe_from_env;
//! ```

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use wyrd_client::{
    Bifrost, Gateway, GlobalConfig, OperatorConnections, Platform, Principals, QueueConfig,
    WyrdClient, bifrost, cards, client, config, environment, error, gateway, global_config,
    observe, operator_connections, platform, state,
};

/// Tenant principal administration: the [`auth::Principals`] handle and its
/// wire types.
pub use wyrd_client::principals as auth;

/// Endpoint and credential types the client configuration names.
///
/// Only the types a parity signature uses: [`config::ClientConfig`]'s `grpc`
/// and `http` fields and its `resolve_credential` result. The transports
/// themselves are mechanics behind [`WyrdClient`] and are not projected.
pub mod transport {
    pub use wyrd_client::transport::{GrpcConfig, HttpConfig, ResolvedCredential};
}

/// Stock OpenTelemetry OTLP exporters that authenticate every export with
/// the client's current token.
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

/// Where an Agent's model calls go: declared on every Agent Card and
/// replaceable whole for one invocation with `Run::invoke_on`.
pub use wyrd_spec::ExecutionRoute;

/// The result `observe().verify` and `VerificationExecution::wait` return:
/// the decided verdict, one result per requirement and step, and the
/// evidence each input slot resolved to.
pub use wyrd_spec::verification::VerificationVerdict;
pub use wyrd_spec::verification::execution::{
    CallProvenance, Citation, Locator, RequirementDetail, RequirementOutcome, RequirementResult,
    StepResult, StepStatus, VerificationOutcome, VerificationResult,
};

/// The identity of one queued Verifier execution, as `submit` returns it and
/// `VerificationExecution::connect` takes it.
pub use wyrd_spec::ids::ExecutionId;

/// The document or image format a media input slot accepts, declared on a
/// `File` with `with_format` when its path or bytes do not imply it.
pub use skald_spec::MediaFormat;

/// The session an emitted observation belongs to.
pub use wyrd_spec::vala::ids::SessionId;

/// The trace a [`Bifrost::trace_timeline`] read names.
pub use wyrd_spec::vala::ids::TraceId;

/// One canonical span a [`Bifrost::trace_timeline`] read returns.
pub use wyrd_spec::vala::trace::SpanRecord;

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
        let _ = super::Bifrost::connect_with;
        let _ = super::bifrost::blocking::Bifrost::connect;
        let _ = super::bifrost::blocking::Bifrost::connect_with;
        let _ = super::Bifrost::list_tables;
        let _ = super::bifrost::blocking::Bifrost::list_tables;
        let _ = super::bifrost::BifrostOptions::default;
        let _ = super::cards::Cards::connect;
        let _ = super::cards::Cards::connect_with;
        let _ = super::auth::Principals::connect;
        let _ = super::auth::Principals::connect_with;
        let _ = super::operator_connections::OperatorConnections::connect;
        let _ = super::operator_connections::OperatorConnections::connect_with;
        let _ = super::platform::Platform::connect;
        let _ = super::platform::Platform::connect_with;
        let _ = super::Gateway::connect;
        let _ = super::Gateway::connect_with;
        let _ = |path: &std::path::Path| super::state::WyrdState::from_path(path);
        let _ =
            |path: &std::path::Path, client| super::state::WyrdState::from_path_with(path, client);
        let _ = |fqn: &'static str| TableConfig::describe(fqn);
        let _ = |client: &'static super::WyrdClient, fqn: &'static str| {
            TableConfig::describe_with(client, fqn)
        };
        let _ = super::WyrdClient::with_config;
        let _ = super::observe::VerificationExecution::connect;
        let _ = super::observe::VerificationExecution::connect_with;
        let _ = |bytes: Vec<u8>| {
            super::observe::File::bytes(bytes).with_format(super::MediaFormat::Pdf)
        };
        let _ = super::WyrdError::code;
    }

    /// A Rust SDK user names every compaction type and declares it on a
    /// table config through `wyrd_ai::bifrost` alone, with no direct
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
