//! Tenant-scoped CRUD for `wyrd.cards`.
#![deny(missing_docs)]

mod audit;
mod auth_projection;
mod delete;
mod field_resolver;
mod get;
mod list;
mod register;
mod version_query;
mod version_resolve;
mod version_sql;

pub use delete::soft_delete_card;
pub use get::{get_card_by_ref, get_card_by_uid};
pub use list::{
    CardQuery, ListCursor, ListPage, MAX_LIST_LIMIT, check_uid_exists, find_card_by_spec_hash,
    get_unique_spaces, query_cards,
};
pub use register::{
    RegisterCardOutcome, RegisterCardOutcomeKind, RegisterCardRequest, register_card,
};
pub use version_query::{get_latest_card_by_range, list_versions};
