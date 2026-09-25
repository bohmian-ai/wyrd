//! First-class Rust Wyrd SDK.
//!
//! A thin projection of `wyrd-client`, the sole shared client implementation:
//! authentication and transport, [`cards::Cards`], [`principals::Principals`],
//! [`platform::Platform`], [`state::WyrdState`], [`storage::WyrdStorageClient`],
//! and [`Bifrost`]. This package adds no
//! transport, validation, registry, storage, or lifecycle behavior of its own,
//! and never enables an owner crate's `python` feature.
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
//! invokes the gateway. `check:sdk-client-tier` fails if the module is added
//! back. Add a module to this list only after deciding it belongs on a public
//! SDK.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use wyrd_client::{
    Bifrost, EvalProtocol, Gateway, GlobalConfig, Platform, Principals, WyrdClient, auth, bifrost,
    cards, client, config, error, eval, gateway, global_config, platform, principals, state,
    storage, transport,
};

/// SDK root re-export shape.
#[cfg(test)]
mod tests {
    /// The SDK root names every composed client capability from `wyrd-client`.
    #[test]
    fn sdk_root_projects_the_composed_client_capabilities() {
        let _ = super::Bifrost::query_only;
        let _ = super::cards::Cards::with_client;
        let _ = super::principals::Principals::with_client;
        let _ = super::platform::Platform::connect;
        let _ = super::state::WyrdState::from_path;
        let _ = super::storage::WyrdStorageClient::new;
        let _ = super::WyrdClient::from_parts;
        let _ = super::Gateway::new;
    }
}
