/// Arrow column name for the event timestamp in microseconds UTC.
pub const WYRD_EVENT_TIME: &str = "wyrd_event_time";
/// Arrow column name for the ingestion timestamp in microseconds UTC.
pub const WYRD_INGESTED_AT: &str = "wyrd_ingested_at";
/// Arrow column name for the 16-byte batch idempotency key.
pub const WYRD_BATCH_ID: &str = "wyrd_batch_id";
/// Arrow column name for the zero-based immutable row identity within a batch.
pub const WYRD_ROW_ORDINAL: &str = "wyrd_row_ordinal";
/// Arrow column name for the server-minted request correlation id.
pub const WYRD_REQUEST_ID: &str = "wyrd_request_id";
/// Arrow column name for the tenant isolation key on every physical table.
pub const DATA_TENANT_ID: &str = "data_tenant_id";

/// Ordered list of column names reserved for server-managed storage.
pub const RESERVED_MANAGED_COLUMNS: &[&str] = &[
    WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
    WYRD_BATCH_ID,
    WYRD_ROW_ORDINAL,
    WYRD_REQUEST_ID,
    DATA_TENANT_ID,
];

/// Arrow column name for the client-supplied run correlation id.
pub const RUN_ID: &str = "run_id";
/// Wire column name for the client-supplied card reference string.
pub const CARD_REF: &str = "card_ref";
/// Arrow column name for the server-resolved card uid.
pub const CARD_UID: &str = "card_uid";
/// Arrow column name for the server-stamped principal id.
pub const PRINCIPAL_ID: &str = "principal_id";

/// Universal correlation columns appended according to a table's correlation
/// policy. They are not part of tenant isolation selection.
pub const RESERVED_CORRELATION_COLUMNS: &[&str] = &[RUN_ID, CARD_UID, PRINCIPAL_ID];

/// The managed physical columns present on every Vala table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManagedColumnSet {
    /// Column name for the event timestamp.
    pub event_time: &'static str,
    /// Column name for the ingestion timestamp.
    pub ingested_at: &'static str,
    /// Column name for the batch idempotency key.
    pub batch_id: &'static str,
    /// Column name for the immutable row ordinal within the batch.
    pub row_ordinal: &'static str,
    /// Column name for the tenant isolation key.
    pub tenant_id: &'static str,
}

impl ManagedColumnSet {
    /// Return the complete managed physical column set.
    pub const fn all() -> Self {
        Self {
            event_time: WYRD_EVENT_TIME,
            ingested_at: WYRD_INGESTED_AT,
            batch_id: WYRD_BATCH_ID,
            row_ordinal: WYRD_ROW_ORDINAL,
            tenant_id: DATA_TENANT_ID,
        }
    }
}

/// Returns `true` if `name` is reserved for server-managed storage.
pub fn is_reserved_managed_column(name: &str) -> bool {
    RESERVED_MANAGED_COLUMNS.contains(&name)
}

/// Returns `true` if `name` is a reserved universal correlation column.
pub fn is_reserved_correlation_column(name: &str) -> bool {
    RESERVED_CORRELATION_COLUMNS.contains(&name)
}
