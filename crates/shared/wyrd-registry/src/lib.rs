//! Client-side Card registry operations.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

mod config;
mod download;
mod engine;
mod error;
mod handle;
mod reads;
mod saga;

pub use handle::{CardSelector, Cards, LoadedCard};
pub use wyrd_spec::registry::{
    CardSummary, ListCardsRequest, ListCardsResponse, RegistrationReceipt,
};
