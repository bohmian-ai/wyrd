//! Wire names of the server-managed Bifrost columns.
//!
//! These are names only. The physical declaration of every managed column —
//! type, nullability, stable id, order, and which tables append it — lives in
//! one place, `vala_bifrost_redux::tables::managed_columns::MANAGED_COLUMNS`.

/// Arrow column name for when the observed thing happened, in microseconds UTC.
///
/// A writer may supply it; otherwise it equals [`WYRD_INGESTED_AT`].
pub const WYRD_EVENT_TIME: &str = "wyrd_event_time";
/// Arrow column name for when Wyrd accepted the row, in microseconds UTC.
///
/// Read once per batch from PostgreSQL at admission; never writer-supplied.
pub const WYRD_INGESTED_AT: &str = "wyrd_ingested_at";
/// Arrow column name for the server-minted request correlation id.
pub const WYRD_REQUEST_ID: &str = "wyrd_request_id";
/// Arrow column name for the client-supplied run correlation id.
pub const RUN_ID: &str = "run_id";
/// Arrow column name for the Card UID a row is correlated to.
///
/// A writer may supply it; the server stores it only after confirming the UID
/// lies in the writer's Card scope, or for a writer bound to no Card, that it
/// names a registered observation-target Card in the writer's tenant.
pub const CARD_UID: &str = "card_uid";
/// Arrow column name for the server-stamped principal id.
pub const PRINCIPAL_ID: &str = "principal_id";
