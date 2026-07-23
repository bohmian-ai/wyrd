//! Tenant-scoped CRUD for `wyrd.cards`.
#![deny(missing_docs)]

mod audit;
mod auth_projection;
mod delete;
mod field_resolver;
mod get;
mod lifecycle;
mod list;
mod register;
pub mod relationships;
mod version_query;
mod version_resolve;
mod version_sql;

pub use auth_projection::upsert_service_account_from_card;
pub use delete::{
    CardDeleteState, soft_delete_card, soft_delete_card_by_ref, soft_delete_card_with_kind,
    soft_delete_card_with_state,
};
pub use get::{find_card_by_ref, get_card_by_ref, get_card_by_uid};
pub use lifecycle::{
    CardManifestCompletionRow, CardReconcileClaim, MAX_RECONCILE_ATTEMPTS, RECONCILE_KIND_BLOB,
    RECONCILE_KIND_CLEANUP, RECONCILE_KIND_FINALIZATION, RECONCILE_KIND_REGISTRATION,
    activate_card, claim_card_reconciliation, fail_card, lock_card_reconciliation_lease,
    lock_pending_card_for_activation, manifest_completion_rows, mark_card_reconciliation_succeeded,
    mark_manifest_verified, record_blob_failure, record_card_blob,
    record_card_reconciliation_failure, schedule_card_reconciliation,
};
pub use list::{
    CardQuery, ListCursor, ListPage, MAX_LIST_LIMIT, check_uid_exists, find_card_by_spec_hash,
    get_unique_spaces, query_cards,
};
pub use register::{
    CardArtifactManifestRow, CardRegistrationOperationRow, NewCardRow, NewRegistrationOperation,
    RegisteredCardRow, artifact_manifest_hash, commit_registration_operation,
    insert_artifact_manifest_rows, insert_card_row, insert_registration_operation,
    lookup_existing_operation, lookup_expired_operation, lookup_operation_by_id,
    manifest_rows_for_init, mark_manifest_upload_initialized, registration_request_hash,
    select_card_uids_by_ref_batch,
};
pub use relationships::{
    inbound_relationships, persist_outbound_relationships, recheck_active_card_refs,
};
pub use version_query::{get_latest_card_by_range, list_versions};
pub use version_resolve::{Resolution, SubmittedCardIdentity, resolve_version};
pub use version_sql::lock_version_line;
