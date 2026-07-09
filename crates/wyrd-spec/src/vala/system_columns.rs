/// Arrow column name for the event timestamp in microseconds UTC.
pub const WYRD_EVENT_TIME: &str = "wyrd_event_time";
/// Arrow column name for the ingestion timestamp in microseconds UTC.
pub const WYRD_INGESTED_AT: &str = "wyrd_ingested_at";
/// Arrow column name for the 16-byte batch idempotency key.
pub const WYRD_BATCH_ID: &str = "wyrd_batch_id";
/// Arrow column name for the tenant isolation key on `SystemShared` tables.
pub const DATA_TENANT_ID: &str = "data_tenant_id";

/// Ordered list of column names that are reserved for Bifrost system use.
/// These are always server-stamped and may never appear as user fields.
pub const RESERVED_SYSTEM_COLUMNS: &[&str] = &[
    WYRD_EVENT_TIME,
    WYRD_INGESTED_AT,
    WYRD_BATCH_ID,
    DATA_TENANT_ID,
];

/// Arrow column name for the client-supplied run correlation id.
pub const RUN_ID: &str = "run_id";
/// Wire column name for the client-supplied card reference string (resolved to `card_uid` by the server).
/// Kept for the existing ingest wire path; physical storage uses `CARD_UID`.
pub const CARD_REF: &str = "card_ref";
/// Arrow column name for the server-resolved card uid (resolved from wire `card_ref`).
pub const CARD_UID: &str = "card_uid";
/// Arrow column name for the server-stamped principal id (from verified JWT).
pub const PRINCIPAL_ID: &str = "principal_id";

/// Universal correlation columns appended (nullable) per `CorrelationPolicy` to
/// pre-declared domain tables. Unlike [`RESERVED_SYSTEM_COLUMNS`], these are not
/// blindly appended to every table — each domain table declares its policy (see
/// `DomainTable::CORRELATION_POLICY`).
///
/// A *user* field on an `Observation` table must not collide with any of these names;
/// the reserved-name guard enforces this per policy (policy-aware C-03).
pub const RESERVED_CORRELATION_COLUMNS: &[&str] = &[RUN_ID, CARD_UID, PRINCIPAL_ID];

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

/// Returns `true` if `name` is a reserved universal correlation column name
/// (`run_id`, `card_uid`, or `principal_id`). A *user* field on an `Observation`
/// table may not use these names; the policy-aware guard in `DomainTable`
/// registration enforces per-policy collision checks.
pub fn is_reserved_correlation_column(name: &str) -> bool {
    RESERVED_CORRELATION_COLUMNS.contains(&name)
}
