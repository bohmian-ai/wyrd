//! First-class Rust Wyrd SDK.
//!
//! A thin projection of `wyrd-client`, the sole shared client implementation:
//! authentication and transport, [`cards::Cards`], [`state::WyrdState`],
//! [`storage::WyrdStorageClient`], and [`Bifrost`]. This package adds no
//! transport, validation, registry, storage, or lifecycle behavior of its own,
//! and never enables an owner crate's `python` feature.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub use wyrd_client::*;

#[cfg(test)]
mod tests {
    /// The SDK root names every composed client capability from `wyrd-client`.
    #[test]
    fn sdk_root_projects_the_composed_client_capabilities() {
        let _ = super::Bifrost::query_only;
        let _ = super::cards::Cards::with_client;
        let _ = super::state::WyrdState::from_path;
        let _ = super::storage::WyrdStorageClient::new;
        let _ = super::WyrdClient::from_parts;
    }
}
