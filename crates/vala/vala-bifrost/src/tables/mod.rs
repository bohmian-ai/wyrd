use std::sync::Arc;

use arrow::datatypes::{Field, Schema, SchemaRef};

use crate::catalog::WyrdCatalog;
use crate::error::BifrostError;
use crate::schema::fingerprint::fingerprint_fields;
use crate::tables::managed_columns::ensure_managed_columns;
use crate::types::PartitionTransform;
use wyrd_spec::vala::managed_columns::{
    DATA_TENANT_ID, WYRD_BATCH_ID, WYRD_EVENT_TIME, WYRD_INGESTED_AT, is_reserved_managed_column,
};

pub mod fields;
pub mod managed_columns;

pub mod dev;
pub mod drift;
pub mod eval;
pub mod genai;
pub mod logs;
pub mod metrics;
pub mod system;
pub mod traces;

/// Which universal correlation columns a pre-declared domain table carries (C-01).
///
/// A single universal policy is wrong — the pre-declared set spans three shapes.
/// Each table declares its policy; `ensure_managed_columns` appends columns from the
/// policy, not unconditionally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrelationPolicy {
    /// Observation facts (traces, metrics, logs, genai, eval, drift).
    /// Appends `run_id`, `card_uid`, `principal_id` (all nullable).
    Observation,
    /// `agent_traces`: the code axis owns `run_id`, so the universal `run_id` is
    /// NOT appended (it would collide with the code-axis column).
    /// `card_uid` + `principal_id` ARE appended.
    CodeAxis,
    /// `audit_log`: 16 bespoke content columns + Bifrost system columns, NO
    /// universal correlation columns appended. `audit_card_ref` is the
    /// writer-identity content column, distinct from correlation `card_uid`.
    None,
}

impl CorrelationPolicy {
    /// Returns the names of the universal correlation columns this policy appends.
    pub fn appended_correlation_columns(self) -> &'static [&'static str] {
        match self {
            Self::Observation => &[
                wyrd_spec::vala::RUN_ID,
                wyrd_spec::vala::CARD_UID,
                wyrd_spec::vala::PRINCIPAL_ID,
            ],
            Self::CodeAxis => &[wyrd_spec::vala::CARD_UID, wyrd_spec::vala::PRINCIPAL_ID],
            Self::None => &[],
        }
    }
}

/// Write-path payload classification (M-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadClass {
    /// No sensitive-default payload columns — commit untouched.
    Standard,
    /// Carries sensitive-default payload named by `DomainTable::SENSITIVE_PAYLOAD_COLUMNS`.
    /// The built-in redaction pass scrubs those columns on write before commit.
    Sensitive,
}

/// Sort key specification for a domain table.
#[derive(Debug, Clone)]
pub struct SortKey {
    pub column: String,
    pub ascending: bool,
    pub nulls_first: bool,
}

/// Index declared by a domain table for `vala.olap_indexes`.
#[derive(Debug, Clone)]
pub struct DeclaredIndex {
    pub name: String,
    pub columns: Vec<String>,
    pub kind: IndexKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    BloomFilter,
    ZOrder,
}

/// Mapping for `entity_time_bounds` best-effort acceleration (M-06).
#[derive(Debug, Clone)]
pub struct EntityBoundsMapping {
    pub entity_kind: String,
    pub entity_id_column: String,
}

/// Pre-declared domain table contract.
///
/// Implementors declare user fields, correlation policy, payload class, partition
/// columns, sort keys, declared indexes, and (optionally) entity bounds mapping.
/// The `register_all` boot path pins each table to its `schema_fingerprint()`.
///
/// # Implementing `DomainTable`
///
/// 1. **Field ordering is fingerprint-identity-significant.** Do not reorder fields
///    in `arrow_fields()` — any reorder changes `schema_fingerprint()` and causes boot
///    to treat the existing Iceberg table as drifted.
/// 2. **`NAMESPACE` must round-trip through `BifrostNamespace::from_domain_namespace`.**
///    If it does not, `register_all` panics when resolving the table's Iceberg path.
/// 3. **`SENSITIVE_PAYLOAD_COLUMNS` must be non-empty if and only if
///    `PAYLOAD_CLASS == PayloadClass::Sensitive`.** The redaction pass and read gate
///    both depend on this invariant; mismatches cause silent data leakage or spurious
///    column-drop on reads.
/// 4. **`register_all` is append-only.** It never removes or modifies existing Iceberg
///    entries. Adding a new table is safe; removing or reordering existing calls is not.
pub trait DomainTable: Send + Sync + 'static {
    const NAMESPACE: &'static str;
    const NAME: &'static str;
    /// Per-table correlation policy (C-01).
    const CORRELATION_POLICY: CorrelationPolicy;
    /// Write-path payload classification (M-03). Defaults to `Standard`.
    const PAYLOAD_CLASS: PayloadClass = PayloadClass::Standard;
    /// Sensitive payload columns scanned by the redaction pass. Non-empty exactly
    /// when `PAYLOAD_CLASS == Sensitive`. Single source of truth for both the
    /// write-path pass (M-03) and the task-16 read gate.
    const SENSITIVE_PAYLOAD_COLUMNS: &'static [&'static str] = &[];

    /// The table's declared **user** fields (no system or correlation columns).
    /// System + policy-appended correlation columns are added by `schema()`.
    fn arrow_fields() -> Vec<Field>;

    /// Full physical schema: user fields + policy correlation columns + Bifrost
    /// system columns. Built at runtime from `arrow_fields()` and the policy.
    /// Boot-cold (≈2 calls per table per boot) — no memoization needed.
    fn schema() -> SchemaRef {
        SchemaRef::new(Schema::new(ensure_managed_columns(
            Self::arrow_fields(),
            Self::CORRELATION_POLICY,
        )))
    }

    /// SHA-256 over the Arrow IPC bytes of `arrow_fields()` exactly as declared.
    /// Byte-identical to the removed build-time `compute_fingerprint`; pins the
    /// declared user schema for the `register` boot state machine.
    fn schema_fingerprint() -> [u8; 32] {
        fingerprint_fields(&Self::arrow_fields())
    }

    /// Partition columns as `(column_name, transform)` pairs. Defaults to
    /// unpartitioned — the current writer uses `UnpartitionedWriter` and cannot
    /// produce partition values. Day-partitioning on `wyrd_event_time` is the
    /// Stage-5 target once a partitioned writer is wired in.
    fn partition_columns() -> Vec<(String, PartitionTransform)> {
        vec![]
    }

    fn sort_keys() -> Vec<SortKey>;
    fn declared_indexes() -> Vec<DeclaredIndex>;

    /// Best-effort `entity_time_bounds` producer mapping (M-06). `Some` names the
    /// `entity_kind` + which physical column supplies `entity_id`. `None` = no bounds.
    fn entity_bounds_mapping() -> Option<EntityBoundsMapping> {
        None
    }
}

/// Policy-aware reserved-field validation (C-03).
///
/// For a pre-declared table: rejects a declared user field only when its name
/// would actually duplicate a column the table gets from system columns or the
/// policy-appended correlation set.
///
/// - System names are always rejected regardless of policy (C-02 guard).
/// - A universal-correlation name is rejected only if the table's policy appends it.
pub fn reject_reserved_domain_fields(
    user_field_names: &[&str],
    policy: CorrelationPolicy,
) -> Result<(), BifrostError> {
    let appended = policy.appended_correlation_columns();
    let always_reserved = [
        WYRD_EVENT_TIME,
        WYRD_INGESTED_AT,
        WYRD_BATCH_ID,
        DATA_TENANT_ID,
    ];
    for name in user_field_names {
        if always_reserved.contains(name) || is_reserved_managed_column(name) {
            return Err(BifrostError::ReservedColumn((*name).to_string()));
        }
        if appended.contains(name) {
            return Err(BifrostError::ReservedColumn((*name).to_string()));
        }
    }
    Ok(())
}

/// Register all pre-declared domain tables at boot. Called from `WyrdCatalog::new`.
///
/// Append-only — do not reorder or remove entries.
pub async fn register_all(catalog: &Arc<WyrdCatalog>) -> Result<(), BifrostError> {
    // traces
    register::<traces::SpansTable>(catalog).await?;
    register::<traces::EventsTable>(catalog).await?;
    register::<traces::LinksTable>(catalog).await?;
    // genai
    register::<genai::MessagesTable>(catalog).await?;
    register::<genai::EmbeddingsTable>(catalog).await?;
    register::<genai::ToolCallsTable>(catalog).await?;
    register::<genai::MemoryTable>(catalog).await?;
    // metrics
    register::<metrics::PointsTable>(catalog).await?;
    // logs
    register::<logs::RecordsTable>(catalog).await?;
    // eval
    register::<eval::RunsTable>(catalog).await?;
    register::<eval::AssertionsTable>(catalog).await?;
    // drift
    register::<drift::ObservationsTable>(catalog).await?;
    // dev
    register::<dev::AgentTracesTable>(catalog).await?;
    // system
    register::<system::AuditLogTable>(catalog).await?;
    Ok(())
}

/// Register a single pre-declared domain table with exhaustive cross-store
/// state checking and concurrent-boot fencing (M-01/M-02).
pub async fn register<T: DomainTable>(catalog: &Arc<WyrdCatalog>) -> Result<(), BifrostError> {
    let _guard = catalog.advisory_lock_for(T::NAMESPACE, T::NAME).await?;

    let control = catalog
        .domain_table_fingerprint(T::NAMESPACE, T::NAME)
        .await?;
    let iceberg = catalog.iceberg_table_exists(T::NAMESPACE, T::NAME).await?;

    match (control, iceberg) {
        // STEADY STATE — only healthy path (M-01).
        (Some(fp), true) if fp == T::schema_fingerprint() => {
            let physical = catalog
                .iceberg_physical_schema(T::NAMESPACE, T::NAME)
                .await?;
            if catalog.physical_matches_declared::<T>(&physical) {
                Ok(())
            } else {
                Err(BifrostError::PhysicalDrift {
                    namespace: T::NAMESPACE,
                    name: T::NAME,
                    expected: T::schema_fingerprint(),
                    actual: catalog.fingerprint_of_user_fields(&physical),
                })
            }
        }
        // Control present, Iceberg MISSING — terminal (M-01).
        (Some(_), false) => Err(BifrostError::IcebergMissing {
            namespace: T::NAMESPACE,
            name: T::NAME,
        }),
        // Fingerprint drifted.
        (Some(actual), true) => Err(BifrostError::SchemaDrift {
            namespace: T::NAMESPACE,
            name: T::NAME,
            expected: T::schema_fingerprint(),
            actual,
        }),
        // Clean first boot.
        (None, false) => match catalog.create_domain_table::<T>().await {
            Ok(()) => {
                catalog
                    .register_domain_control_row::<T>(T::schema_fingerprint())
                    .await?;
                for idx in T::declared_indexes() {
                    catalog.declare_domain_index(T::NAMESPACE, T::NAME, &idx)?;
                }
                Ok(())
            }
            Err(BifrostError::IcebergAlreadyExists { .. }) => {
                repair_control_from_physical::<T>(catalog).await
            }
            Err(e) => Err(e),
        },
        // SPLIT BRAIN: Iceberg present, control row missing (M-02).
        (None, true) => repair_control_from_physical::<T>(catalog).await,
    }
}

async fn repair_control_from_physical<T: DomainTable>(
    catalog: &Arc<WyrdCatalog>,
) -> Result<(), BifrostError> {
    let physical = catalog
        .iceberg_physical_schema(T::NAMESPACE, T::NAME)
        .await?;
    if catalog.physical_matches_declared::<T>(&physical) {
        catalog
            .register_domain_control_row::<T>(T::schema_fingerprint())
            .await?;
        catalog.ensure_domain_indexes::<T>()?;
        Ok(())
    } else {
        Err(BifrostError::PhysicalDrift {
            namespace: T::NAMESPACE,
            name: T::NAME,
            expected: T::schema_fingerprint(),
            actual: catalog.fingerprint_of_user_fields(&physical),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wyrd_spec::vala::{CARD_UID, PRINCIPAL_ID, RUN_ID};

    #[test]
    fn observation_policy_appends_all_three() {
        let cols = CorrelationPolicy::Observation.appended_correlation_columns();
        assert!(cols.contains(&RUN_ID));
        assert!(cols.contains(&CARD_UID));
        assert!(cols.contains(&PRINCIPAL_ID));
    }

    #[test]
    fn code_axis_policy_omits_run_id() {
        let cols = CorrelationPolicy::CodeAxis.appended_correlation_columns();
        assert!(
            !cols.contains(&RUN_ID),
            "CodeAxis must not append universal run_id"
        );
        assert!(cols.contains(&CARD_UID));
        assert!(cols.contains(&PRINCIPAL_ID));
    }

    #[test]
    fn none_policy_appends_nothing() {
        let cols = CorrelationPolicy::None.appended_correlation_columns();
        assert!(cols.is_empty());
    }

    #[test]
    fn observation_table_rejects_reserved_correlation_names() {
        let policy = CorrelationPolicy::Observation;
        for reserved in [RUN_ID, CARD_UID, PRINCIPAL_ID] {
            assert!(
                reject_reserved_domain_fields(&[reserved], policy).is_err(),
                "'{reserved}' must be rejected on Observation table"
            );
        }
    }

    #[test]
    fn none_table_permits_principal_id_and_run_id_as_content() {
        // audit_log has CorrelationPolicy::None and declares principal_id as a content column.
        let result =
            reject_reserved_domain_fields(&["principal_id", "run_id"], CorrelationPolicy::None);
        assert!(
            result.is_ok(),
            "None policy must permit content columns named principal_id/run_id"
        );
    }

    #[test]
    fn code_axis_table_permits_run_id_content_column() {
        // agent_traces has CorrelationPolicy::CodeAxis and declares run_id as its own code-axis column.
        let result = reject_reserved_domain_fields(&["run_id"], CorrelationPolicy::CodeAxis);
        assert!(
            result.is_ok(),
            "CodeAxis policy must permit run_id as a content column"
        );
    }

    #[test]
    fn system_column_always_rejected_regardless_of_policy() {
        for policy in [
            CorrelationPolicy::Observation,
            CorrelationPolicy::CodeAxis,
            CorrelationPolicy::None,
        ] {
            for sys in [
                "data_tenant_id",
                "wyrd_event_time",
                "wyrd_ingested_at",
                "wyrd_batch_id",
            ] {
                assert!(
                    reject_reserved_domain_fields(&[sys], policy).is_err(),
                    "system column '{sys}' must be rejected by all policies"
                );
            }
        }
    }
}

#[cfg(test)]
mod fingerprint_drift {
    use super::*;
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};

    // ── Test A ────────────────────────────────────────────────────────────────
    // Expected values are the build-time `*_FINGERPRINT` constants copied verbatim
    // from the (now-deleted) src/tables/generated_schemas.snapshot before removal.
    // A deliberate schema change updates the matching array here.
    #[test]
    fn declared_fingerprints_match_pre_refactor_build_output() {
        assert_eq!(traces::SpansTable::schema_fingerprint(), TRACES_SPANS);
        assert_eq!(traces::EventsTable::schema_fingerprint(), TRACES_EVENTS);
        assert_eq!(traces::LinksTable::schema_fingerprint(), TRACES_LINKS);
        assert_eq!(genai::MessagesTable::schema_fingerprint(), GENAI_MESSAGES);
        assert_eq!(
            genai::EmbeddingsTable::schema_fingerprint(),
            GENAI_EMBEDDINGS
        );
        assert_eq!(
            genai::ToolCallsTable::schema_fingerprint(),
            GENAI_TOOL_CALLS
        );
        assert_eq!(genai::MemoryTable::schema_fingerprint(), GENAI_MEMORY);
        assert_eq!(metrics::PointsTable::schema_fingerprint(), METRICS_POINTS);
        assert_eq!(logs::RecordsTable::schema_fingerprint(), LOGS_RECORDS);
        assert_eq!(eval::RunsTable::schema_fingerprint(), EVAL_RUNS);
        assert_eq!(eval::AssertionsTable::schema_fingerprint(), EVAL_ASSERTIONS);
        assert_eq!(
            drift::ObservationsTable::schema_fingerprint(),
            DRIFT_OBSERVATIONS
        );
        assert_eq!(
            dev::AgentTracesTable::schema_fingerprint(),
            DEV_AGENT_TRACES
        );
        assert_eq!(
            system::AuditLogTable::schema_fingerprint(),
            SYSTEM_AUDIT_LOG
        );
    }

    const TRACES_SPANS: [u8; 32] = [
        117, 123, 6, 179, 56, 96, 0, 186, 121, 79, 11, 80, 39, 98, 14, 107, 69, 100, 183, 245, 234,
        210, 43, 64, 31, 239, 198, 172, 171, 145, 83, 133,
    ];
    const TRACES_EVENTS: [u8; 32] = [
        226, 118, 169, 168, 66, 138, 130, 223, 181, 82, 231, 97, 58, 210, 210, 130, 117, 35, 71,
        39, 170, 64, 168, 172, 75, 134, 195, 33, 214, 191, 234, 86,
    ];
    const TRACES_LINKS: [u8; 32] = [
        143, 122, 183, 46, 45, 70, 96, 22, 188, 125, 113, 178, 147, 175, 212, 4, 248, 88, 103, 191,
        254, 215, 103, 109, 128, 107, 244, 28, 77, 199, 241, 93,
    ];
    const GENAI_MESSAGES: [u8; 32] = [
        16, 227, 176, 112, 122, 82, 185, 123, 203, 18, 209, 228, 30, 226, 148, 67, 42, 167, 185,
        146, 25, 26, 161, 62, 158, 151, 145, 191, 219, 188, 211, 248,
    ];
    const GENAI_EMBEDDINGS: [u8; 32] = [
        70, 206, 187, 2, 42, 150, 167, 242, 56, 8, 240, 240, 41, 41, 82, 229, 169, 148, 83, 240,
        102, 131, 103, 157, 156, 128, 136, 223, 4, 12, 235, 56,
    ];
    const GENAI_TOOL_CALLS: [u8; 32] = [
        118, 27, 201, 72, 178, 53, 147, 52, 82, 237, 62, 49, 95, 139, 246, 192, 147, 56, 86, 230,
        31, 222, 32, 41, 216, 100, 121, 43, 204, 154, 117, 214,
    ];
    const GENAI_MEMORY: [u8; 32] = [
        103, 0, 176, 197, 26, 38, 144, 38, 225, 69, 207, 13, 70, 80, 114, 130, 70, 125, 129, 236,
        148, 137, 122, 63, 89, 70, 86, 255, 204, 180, 100, 113,
    ];
    const METRICS_POINTS: [u8; 32] = [
        148, 86, 36, 185, 229, 163, 186, 129, 219, 113, 27, 66, 152, 156, 153, 174, 169, 6, 11,
        192, 176, 50, 56, 214, 66, 127, 186, 36, 143, 88, 81, 167,
    ];
    const LOGS_RECORDS: [u8; 32] = [
        51, 247, 38, 148, 208, 151, 64, 71, 131, 104, 39, 145, 187, 244, 21, 20, 73, 92, 27, 1,
        227, 55, 121, 127, 22, 143, 128, 58, 200, 65, 19, 246,
    ];
    const EVAL_RUNS: [u8; 32] = [
        137, 217, 173, 185, 44, 241, 230, 190, 239, 170, 25, 235, 211, 193, 88, 94, 108, 213, 233,
        203, 37, 111, 127, 130, 212, 50, 112, 50, 215, 227, 178, 121,
    ];
    const EVAL_ASSERTIONS: [u8; 32] = [
        146, 162, 244, 135, 24, 60, 97, 196, 71, 3, 9, 222, 84, 10, 135, 215, 224, 167, 70, 5, 210,
        117, 138, 143, 148, 183, 141, 110, 216, 44, 42, 149,
    ];
    const DRIFT_OBSERVATIONS: [u8; 32] = [
        112, 82, 132, 104, 197, 244, 164, 0, 57, 245, 218, 141, 35, 180, 33, 70, 121, 45, 12, 104,
        180, 254, 240, 146, 230, 91, 106, 203, 91, 71, 223, 119,
    ];
    const DEV_AGENT_TRACES: [u8; 32] = [
        8, 32, 129, 244, 232, 216, 54, 94, 62, 124, 239, 190, 51, 156, 221, 193, 99, 85, 103, 221,
        243, 159, 254, 166, 176, 213, 219, 232, 255, 124, 29, 120,
    ];
    const SYSTEM_AUDIT_LOG: [u8; 32] = [
        89, 188, 52, 180, 246, 60, 100, 140, 252, 148, 135, 184, 247, 96, 117, 175, 246, 148, 183,
        217, 122, 16, 225, 210, 200, 250, 115, 124, 222, 255, 252, 10,
    ];

    // ── Test B ────────────────────────────────────────────────────────────────
    // Each `*_full()` returns the FULL physical schema copied verbatim from the
    // matching `<name>_schema()` body in the deleted generated_schemas.snapshot —
    // i.e. user fields + appended correlation/system columns, exactly as the old
    // build-time `full_schema` emitted them. Assert the runtime `T::schema()` equals
    // it. A deliberate schema change updates the matching `*_full()` body here.
    #[test]
    fn full_physical_schema_matches_pre_refactor_build_output() {
        assert_eq!(traces::SpansTable::schema().as_ref(), &traces_spans_full());
        assert_eq!(
            traces::EventsTable::schema().as_ref(),
            &traces_events_full()
        );
        assert_eq!(traces::LinksTable::schema().as_ref(), &traces_links_full());
        assert_eq!(
            genai::MessagesTable::schema().as_ref(),
            &genai_messages_full()
        );
        assert_eq!(
            genai::EmbeddingsTable::schema().as_ref(),
            &genai_embeddings_full()
        );
        assert_eq!(
            genai::ToolCallsTable::schema().as_ref(),
            &genai_tool_calls_full()
        );
        assert_eq!(genai::MemoryTable::schema().as_ref(), &genai_memory_full());
        assert_eq!(
            metrics::PointsTable::schema().as_ref(),
            &metrics_points_full()
        );
        assert_eq!(logs::RecordsTable::schema().as_ref(), &logs_records_full());
        assert_eq!(eval::RunsTable::schema().as_ref(), &eval_runs_full());
        assert_eq!(
            eval::AssertionsTable::schema().as_ref(),
            &eval_assertions_full()
        );
        assert_eq!(
            drift::ObservationsTable::schema().as_ref(),
            &drift_observations_full()
        );
        assert_eq!(
            dev::AgentTracesTable::schema().as_ref(),
            &dev_agent_traces_full()
        );
        assert_eq!(
            system::AuditLogTable::schema().as_ref(),
            &system_audit_log_full()
        );
    }

    fn traces_spans_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
            Field::new("flags", DataType::UInt32, false),
            Field::new("trace_state", DataType::Utf8, true),
            Field::new("name", DataType::Utf8, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("duration_ms", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("attributes", DataType::Utf8View, true),
            Field::new("dropped_attributes_count", DataType::UInt32, false),
            Field::new("dropped_events_count", DataType::UInt32, false),
            Field::new("dropped_links_count", DataType::UInt32, false),
            Field::new("scope_name", DataType::Utf8, true),
            Field::new("scope_version", DataType::Utf8, true),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn traces_events_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("name", DataType::Utf8, false),
            Field::new("attributes", DataType::Utf8View, true),
            Field::new("dropped_attributes_count", DataType::UInt32, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn traces_links_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new("linked_trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("linked_span_id", DataType::FixedSizeBinary(8), false),
            Field::new("trace_state", DataType::Utf8, true),
            Field::new("flags", DataType::UInt32, false),
            Field::new("attributes", DataType::Utf8View, true),
            Field::new("dropped_attributes_count", DataType::UInt32, false),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn genai_messages_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("duration_ms", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("provider_name", DataType::Utf8, false),
            Field::new("operation_name", DataType::Utf8, false),
            Field::new("request_model", DataType::Utf8, false),
            Field::new("response_model", DataType::Utf8, true),
            Field::new("conversation_id", DataType::Utf8, true),
            Field::new("response_id", DataType::Utf8, true),
            Field::new("response_finish_reasons", DataType::Utf8View, true),
            Field::new(
                "response_time_to_first_chunk_seconds",
                DataType::Float64,
                true,
            ),
            Field::new("request_temperature", DataType::Float64, true),
            Field::new("request_top_p", DataType::Float64, true),
            Field::new("request_top_k", DataType::Int64, true),
            Field::new("request_max_tokens", DataType::Int64, true),
            Field::new("request_frequency_penalty", DataType::Float64, true),
            Field::new("request_presence_penalty", DataType::Float64, true),
            Field::new("request_seed", DataType::Int64, true),
            Field::new("request_choice_count", DataType::Int64, true),
            Field::new("request_stop_sequences", DataType::Utf8View, true),
            Field::new("request_stream", DataType::Boolean, true),
            Field::new("request_encoding_formats", DataType::Utf8View, true),
            Field::new("usage_input_tokens", DataType::Int64, true),
            Field::new("usage_output_tokens", DataType::Int64, true),
            Field::new("usage_cache_creation_input_tokens", DataType::Int64, true),
            Field::new("usage_cache_read_input_tokens", DataType::Int64, true),
            Field::new("usage_reasoning_output_tokens", DataType::Int64, true),
            Field::new("output_type", DataType::Utf8, true),
            Field::new("request_reasoning_level", DataType::Utf8, true),
            Field::new("conversation_compacted", DataType::Boolean, true),
            Field::new("input_messages", DataType::Utf8View, true),
            Field::new("output_messages", DataType::Utf8View, true),
            Field::new("system_instructions", DataType::Utf8View, true),
            Field::new("openai_api_type", DataType::Utf8, true),
            Field::new("openai_request_service_tier", DataType::Utf8, true),
            Field::new("openai_response_service_tier", DataType::Utf8, true),
            Field::new("openai_response_system_fingerprint", DataType::Utf8, true),
            Field::new("error_type", DataType::Utf8, true),
            Field::new("eval_results", DataType::Utf8View, true),
            Field::new("extra", DataType::Utf8View, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn genai_embeddings_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("duration_ms", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("provider_name", DataType::Utf8, false),
            Field::new("operation_name", DataType::Utf8, false),
            Field::new("request_model", DataType::Utf8, false),
            Field::new("response_model", DataType::Utf8, true),
            Field::new("embeddings_dimension_count", DataType::Int64, true),
            Field::new("data_source_id", DataType::Utf8, true),
            Field::new("usage_input_tokens", DataType::Int64, true),
            Field::new("usage_output_tokens", DataType::Int64, true),
            Field::new("retrieval_top_k", DataType::Int64, true),
            Field::new("error_type", DataType::Utf8, true),
            Field::new("retrieval_query_text", DataType::Utf8, true),
            Field::new("extra", DataType::Utf8View, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn genai_tool_calls_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
            Field::new("conversation_id", DataType::Utf8, true),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("duration_ms", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("provider_name", DataType::Utf8, false),
            Field::new("operation_name", DataType::Utf8, false),
            Field::new("tool_name", DataType::Utf8, true),
            Field::new("tool_type", DataType::Utf8, true),
            Field::new("tool_call_id", DataType::Utf8, true),
            Field::new("tool_description", DataType::Utf8, true),
            Field::new("tool_call_arguments", DataType::Utf8View, true),
            Field::new("tool_call_result", DataType::Utf8View, true),
            Field::new("mcp_session_id", DataType::Utf8, true),
            Field::new("mcp_method_name", DataType::Utf8, true),
            Field::new("mcp_protocol_version", DataType::Utf8, true),
            Field::new("mcp_resource_uri", DataType::Utf8, true),
            Field::new("error_type", DataType::Utf8, true),
            Field::new("extra", DataType::Utf8View, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn genai_memory_full() -> Schema {
        Schema::new(vec![
            Field::new("trace_id", DataType::FixedSizeBinary(16), false),
            Field::new("span_id", DataType::FixedSizeBinary(8), false),
            Field::new("parent_span_id", DataType::FixedSizeBinary(8), true),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "end_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("duration_ms", DataType::Int64, false),
            Field::new("status", DataType::Utf8, false),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("provider_name", DataType::Utf8, false),
            Field::new("operation_name", DataType::Utf8, false),
            Field::new("memory_store_id", DataType::Utf8, true),
            Field::new("memory_record_id", DataType::Utf8, true),
            Field::new("memory_record_count", DataType::Int64, true),
            Field::new("memory_query_text", DataType::Utf8, true),
            Field::new("memory_records", DataType::Utf8View, true),
            Field::new("error_type", DataType::Utf8, true),
            Field::new("extra", DataType::Utf8View, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn metrics_points_full() -> Schema {
        Schema::new(vec![
            Field::new("metric_name", DataType::Utf8, false),
            Field::new(
                "time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "start_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Field::new("description", DataType::Utf8, true),
            Field::new("unit", DataType::Utf8, true),
            Field::new("metric_type", DataType::Utf8, false),
            Field::new("temporality", DataType::Utf8, true),
            Field::new("is_monotonic", DataType::Boolean, true),
            Field::new("flags", DataType::UInt32, true),
            Field::new("value", DataType::Float64, true),
            Field::new("count", DataType::Int64, true),
            Field::new("sum", DataType::Float64, true),
            Field::new("min", DataType::Float64, true),
            Field::new("max", DataType::Float64, true),
            Field::new("bucket_counts", DataType::Utf8View, true),
            Field::new("explicit_bounds", DataType::Utf8View, true),
            Field::new("scale", DataType::Int32, true),
            Field::new("zero_count", DataType::Int64, true),
            Field::new("zero_threshold", DataType::Float64, true),
            Field::new("positive_buckets", DataType::Utf8View, true),
            Field::new("negative_buckets", DataType::Utf8View, true),
            Field::new("quantile_values", DataType::Utf8View, true),
            Field::new("exemplars", DataType::Utf8View, true),
            Field::new("attributes", DataType::Utf8View, true),
            Field::new("service_name", DataType::Utf8, false),
            Field::new("scope_name", DataType::Utf8, true),
            Field::new("scope_version", DataType::Utf8, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn logs_records_full() -> Schema {
        Schema::new(vec![
            Field::new(
                "time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                true,
            ),
            Field::new(
                "observed_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("severity_number", DataType::UInt32, true),
            Field::new("severity_text", DataType::Utf8, true),
            Field::new("event_name", DataType::Utf8, true),
            Field::new("body", DataType::Utf8View, true),
            Field::new("trace_id", DataType::FixedSizeBinary(16), true),
            Field::new("span_id", DataType::FixedSizeBinary(8), true),
            Field::new("trace_flags", DataType::UInt32, true),
            Field::new("attributes", DataType::Utf8View, true),
            Field::new("dropped_attributes_count", DataType::UInt32, false),
            Field::new("service_name", DataType::Utf8, true),
            Field::new("scope_name", DataType::Utf8, true),
            Field::new("scope_version", DataType::Utf8, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn eval_runs_full() -> Schema {
        Schema::new(vec![
            Field::new("record_id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, true),
            Field::new("eval_ref", DataType::Utf8, true),
            Field::new("context", DataType::Utf8View, false),
            Field::new("trace_id", DataType::FixedSizeBinary(16), true),
            Field::new("span_id", DataType::FixedSizeBinary(8), true),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("media", DataType::Utf8View, true),
            Field::new("total_tasks", DataType::Int32, true),
            Field::new("passed_tasks", DataType::Int32, true),
            Field::new("failed_tasks", DataType::Int32, true),
            Field::new("pass_rate", DataType::Float64, true),
            Field::new("duration_ms", DataType::Int64, true),
            Field::new("execution_plan", DataType::Utf8View, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn eval_assertions_full() -> Schema {
        Schema::new(vec![
            Field::new("record_id", DataType::Utf8, false),
            Field::new("session_id", DataType::Utf8, true),
            Field::new("eval_ref", DataType::Utf8, true),
            Field::new("trace_id", DataType::FixedSizeBinary(16), true),
            Field::new("span_id", DataType::FixedSizeBinary(8), true),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("assertion_name", DataType::Utf8, false),
            Field::new("score_label", DataType::Utf8, true),
            Field::new("score_value", DataType::Float64, true),
            Field::new("explanation", DataType::Utf8, true),
            Field::new("response_id", DataType::Utf8, true),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn drift_observations_full() -> Schema {
        Schema::new(vec![
            Field::new("record_id", DataType::Utf8, false),
            Field::new("drift_ref", DataType::Utf8, true),
            Field::new("series", DataType::Utf8, false),
            Field::new("num_value", DataType::Float64, true),
            Field::new("str_value", DataType::Utf8, true),
            Field::new("session_id", DataType::Utf8, true),
            Field::new(
                "created_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("run_id", DataType::Utf8, true),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn dev_agent_traces_full() -> Schema {
        Schema::new(vec![
            Field::new("dev_session_id", DataType::Utf8, false),
            Field::new("repo", DataType::Utf8, false),
            Field::new("commit_sha", DataType::Utf8, true),
            Field::new("branch", DataType::Utf8, true),
            Field::new("run_id", DataType::Utf8, false),
            Field::new("trace_id", DataType::FixedSizeBinary(16), true),
            Field::new("span_id", DataType::FixedSizeBinary(8), true),
            Field::new("role", DataType::Utf8, false),
            Field::new("model", DataType::Utf8, false),
            Field::new("provider", DataType::Utf8, false),
            Field::new("messages", DataType::Utf8View, false),
            Field::new("tool_io", DataType::Utf8View, true),
            Field::new(
                "started_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "ended_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("card_uid", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, true),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }

    fn system_audit_log_full() -> Schema {
        Schema::new(vec![
            Field::new("seq", DataType::Int64, false),
            Field::new("entry_hash", DataType::Utf8, false),
            Field::new("prev_hash", DataType::Utf8, false),
            Field::new("request_id", DataType::Utf8, false),
            Field::new("trace_id", DataType::Utf8, true),
            Field::new("operation", DataType::Utf8, false),
            Field::new("resource", DataType::Utf8, false),
            Field::new("audit_card_ref", DataType::Utf8, true),
            Field::new("principal_id", DataType::Utf8, false),
            Field::new("principal_kind", DataType::Utf8, false),
            Field::new("auth_method", DataType::Utf8, false),
            Field::new("permission", DataType::Utf8, false),
            Field::new("decision", DataType::Utf8, false),
            Field::new("result", DataType::Utf8, false),
            Field::new("payload_summary", DataType::Utf8, false),
            Field::new("detail", DataType::Utf8, true),
            Field::new("created_at_us", DataType::Int64, false),
            Field::new(
                "wyrd_event_time",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new(
                "wyrd_ingested_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                false,
            ),
            Field::new("wyrd_batch_id", DataType::FixedSizeBinary(16), false),
            Field::new("data_tenant_id", DataType::Utf8, false),
        ])
    }
}
