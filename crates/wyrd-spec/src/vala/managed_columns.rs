//! Wire names of the server-managed Bifrost columns.
//!
//! These are names only. The physical declaration of every managed column —
//! type, nullability, stable id, order, and which tables append it — lives in
//! one place, `vala_bifrost_redux::tables::managed_columns::MANAGED_COLUMNS`.

/// Arrow column name for when Wyrd accepted the row, in microseconds UTC.
///
/// Read once per batch from PostgreSQL at admission; never writer-supplied.
pub const WYRD_INGESTED_AT: &str = "wyrd_ingested_at";
/// Arrow column name for the server-minted request correlation id.
pub const WYRD_REQUEST_ID: &str = "wyrd_request_id";
/// Arrow column name for the client-supplied run correlation id.
pub const RUN_ID: &str = "run_id";
/// Arrow column name for the exact Card a row observes or judges.
///
/// A writer may supply it; the server stores it only after confirming the UID
/// lies in the writer's Card scope, or for a writer bound to no Card, that it
/// names a registered observation-target Card in the writer's tenant. OTLP
/// records supply it as the `wyrd.card_uid` attribute.
pub const SUBJECT_CARD_UID: &str = "subject_card_uid";
/// Arrow column name for the validated exact Service release a row's work
/// executed under.
///
/// Never writer-supplied: the server stamps the verified workload root, or a
/// trusted producer's accepted binding owner, and null without proof.
pub const WORKLOAD_CARD_UID: &str = "workload_card_uid";
/// Arrow column name for the server-stamped principal the row's work was for.
pub const SUB_PRINCIPAL_ID: &str = "sub_principal_id";
/// Arrow column name for the server-stamped principal that performed the
/// row's work.
pub const ACT_PRINCIPAL_ID: &str = "act_principal_id";

/// Arrow column name for the workload Service's registered name, or the
/// observed `service.name` of unowned telemetry.
pub const SERVICE_NAME: &str = "service_name";
/// Arrow column name for the workload Service's registered version, or the
/// observed `service.version` of unowned telemetry.
pub const SERVICE_VERSION: &str = "service_version";
/// Arrow column name for the accepted deployment environment tag.
pub const DEPLOYMENT_ENVIRONMENT: &str = "deployment_environment";
/// Arrow column name for the subject Card's canonical kind wire value.
pub const SUBJECT_KIND: &str = "subject_kind";
/// Arrow column name for the subject Card's registered name.
pub const SUBJECT_NAME: &str = "subject_name";
/// Arrow column name for the subject Card's registered version.
pub const SUBJECT_VERSION: &str = "subject_version";
