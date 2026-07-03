/// Arrow column name for the event timestamp in microseconds UTC.
pub const WYRD_EVENT_TIME: &str = "wyrd_event_time";
/// Arrow column name for the ingestion timestamp in microseconds UTC.
pub const WYRD_INGESTED_AT: &str = "wyrd_ingested_at";
/// Arrow column name for the 16-byte batch idempotency key.
pub const WYRD_BATCH_ID: &str = "wyrd_batch_id";
/// Arrow column name for the tenant isolation key on `SystemShared` tables.
pub const DATA_TENANT_ID: &str = "data_tenant_id";

/// Ordered list of column names that are reserved for Bifrost system use.
pub const RESERVED_SYSTEM_COLUMNS: &[&str] = &[
    WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
    WYRD_BATCH_ID,
    DATA_TENANT_ID,
];

/// Arrow column name for the client-supplied run correlation id.
pub const RUN_ID: &str = "run_id";
/// Arrow column name for the client-supplied per-row card correlation tag.
pub const CARD_REF: &str = "card_ref";

/// Universal correlation columns present (nullable) in every Bifrost table's
/// physical schema. Unlike [`RESERVED_SYSTEM_COLUMNS`], these are **client
/// supplied** — the server never stamps them — and they are deliberately absent
/// from [`is_reserved_system_column`] so a caller batch may carry them. A *user*
/// field, however, must not collide with a correlation name; C2 (`create_table`)
/// and C3 (the client `BatchBuilder`) enforce that via
/// [`is_reserved_correlation_column`].
pub const RESERVED_CORRELATION_COLUMNS: &[&str] = &[RUN_ID, CARD_REF];

/// Describes which system columns are appended to a Bifrost table schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemColumnSet {
    /// Column name for the event timestamp.
    pub event_time: &'static str,
    /// Column name for the ingestion timestamp.
    pub ingested_at: &'static str,
    /// Column name for the batch idempotency key.
    pub batch_id: &'static str,
    /// Column name for the tenant isolation key, or `None` for TenantOwned tables.
    pub tenant_id: Option<&'static str>,
}

impl SystemColumnSet {
    /// Returns the system columns for a TenantOwned table (no tenant_id column).
    pub const fn for_tenant_owned() -> Self {
        Self {
            event_time: WYRD_EVENT_TIME,
            ingested_at: WYRD_INGESTED_AT,
            batch_id: WYRD_BATCH_ID,
            tenant_id: None,
        }
    }

    /// Returns the system columns for a `SystemShared` table (includes `tenant_id`).
    pub const fn for_system_shared() -> Self {
        Self {
            event_time: WYRD_EVENT_TIME,
            ingested_at: WYRD_INGESTED_AT,
            batch_id: WYRD_BATCH_ID,
            tenant_id: Some(DATA_TENANT_ID),
        }
    }
}

/// Returns `true` if `name` is a reserved Bifrost system column name.
pub fn is_reserved_system_column(name: &str) -> bool {
    RESERVED_SYSTEM_COLUMNS.contains(&name)
}

/// Returns `true` if `name` is a reserved correlation column name (`run_id` or
/// `card_ref`). A *user* field may not use these names; the ingest/write path
/// carries them as client-supplied cell values, never as user fields.
pub fn is_reserved_correlation_column(name: &str) -> bool {
    RESERVED_CORRELATION_COLUMNS.contains(&name)
}
