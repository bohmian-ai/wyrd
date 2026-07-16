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
pub use get::{find_card_by_ref, get_card_by_ref, get_card_by_uid};
pub use list::{
    CardQuery, ListCursor, ListPage, MAX_LIST_LIMIT, check_uid_exists, find_card_by_spec_hash,
    get_unique_spaces, query_cards,
};
pub use register::{
    CardArtifactManifestRow, CardRegistrationOperationRow, NewCardRow, NewRegistrationOperation,
    RegisterCardOutcome, RegisterCardOutcomeKind, RegisterCardRequest, RegisteredCardRow,
    artifact_manifest_hash, insert_artifact_manifest_rows, insert_card_row,
    insert_registration_operation, lookup_existing_operation, manifest_rows_for_init,
    mark_manifest_upload_initialized, operation_outcome, operation_status, register_card,
    registration_request_hash, select_card_uids_by_ref_batch, update_registration_operation_plans,
};
pub use version_query::{get_latest_card_by_range, list_versions};
pub use version_resolve::{Resolution, SubmittedCardIdentity, resolve_version};
pub use version_sql::lock_version_line;
