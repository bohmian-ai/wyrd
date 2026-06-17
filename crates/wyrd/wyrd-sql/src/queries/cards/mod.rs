//! Tenant-scoped CRUD for `wyrd.cards`.
#![deny(missing_docs)]

mod audit;
mod auth_projection;
mod delete;
mod get;
mod list;
mod register;

pub use delete::soft_delete_card;
pub use get::{get_card_by_ref, get_card_by_uid};
pub use list::{
    list_cards_by_kind, list_cards_by_space, list_cards_by_status, ListCursor, ListPage,
    MAX_LIST_LIMIT,
};
pub use register::{
    register_card, RegisterCardOutcome, RegisterCardOutcomeKind, RegisterCardRequest,
};
