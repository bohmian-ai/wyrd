//! First-class Rust Wyrd SDK.
//!
//! A thin projection of `wyrd-client`, the sole shared client implementation:
//! authentication and transport, [`cards::Cards`], [`principals::Principals`],
//! [`platform::Platform`], [`state::WyrdState`], [`storage::WyrdStorageClient`],
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
//! only as CLI commands, through the optional `cli` feature's [`cli`] module
//! when it is enabled.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use wyrd_client::*;

/// In-process `wyrd` CLI commands, behind the optional `cli` feature.
///
/// Each function runs the same command implementation as the `wyrd`
/// executable, takes the command's options as typed arguments, returns the
/// value the command prints with `--format json`, and returns a
/// [`WyrdError`](crate::WyrdError) instead of an exit code. Networked
/// commands read their credential from the ambient chain; `server` re-points
/// only the endpoint.
#[cfg(feature = "cli")]
pub mod cli {
    pub use wyrd_cli::commands::*;
}

/// The stable, catalogued error every SDK capability returns.
pub use wyrd_spec::error::WyrdError;

/// SDK root re-export shape.
#[cfg(test)]
mod tests {
    /// The SDK root names every composed client capability from `wyrd-client`
    /// and the error they return.
    #[test]
    fn sdk_root_exposes_the_supported_surface() {
        let _ = super::Bifrost::query_only;
        let _ = super::cards::Cards::with_client;
        let _ = super::principals::Principals::with_client;
        let _ = super::operator_connections::OperatorConnections::with_client;
        let _ = super::platform::Platform::connect;
        let _ = super::state::WyrdState::from_path;
        let _ = super::storage::WyrdStorageClient::new;
        let _ = super::WyrdClient::from_parts;
        let _ = super::Gateway::new;
        let _ = super::Workflow::as_skald;
        let _ = super::Workflows::new;
        let _ = super::PublicWyrdGatewayCaller::new;
        let _ = super::cards::Cards::workflow;
        let _ = super::WyrdError::code;
    }
}
