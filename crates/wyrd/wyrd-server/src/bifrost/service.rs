//! RBAC-gated Bifrost catalog service functions (register / list / describe).
//!
//! Each function authorizes the caller against the runtime RBAC model *before*
//! touching the Redux catalog, mirroring `storage::service`. Engine errors cross
//! the public boundary through `BifrostError::into_public().into()` and are
//! rendered by the single `WyrdErrorResponse`.

use arrow::datatypes::Field;
use vala_bifrost_redux::catalog::{BifrostCatalogError, TableRef};
use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::api::{
    AuditDecision, AuditResult, BifrostTableDescription, BifrostTableEntry, PhysicalLayoutWire,
    RegisterOutcome, RegisterTableRequest, RegisterTableResponse,
};

use wyrd_runtime::PermissionVerdict;

use crate::AppState;
use crate::audit;
use crate::bifrost::convert;
use crate::components::auth::Caller;
use crate::http::error::permission_deny_reason_to_wyrd;

/// Map a Redux catalog error to the public `WyrdError` via the single delegate.
fn map_engine_error(error: BifrostCatalogError) -> WyrdError {
    error.into_public().into()
}

/// Authorize, appending a `decision = deny` audit row (own tx) on refusal.
async fn authorize_audited(
    state: &AppState,
    caller: &Caller,
    required: &Permission,
    operation: &str,
    resource: &str,
) -> Result<(), WyrdError> {
    match state
        .authz
        .permission_check
        .check(&caller.principal, required)
    {
        PermissionVerdict::Allow => Ok(()),
        PermissionVerdict::Deny { reason } => {
            let event = audit::audit_event(
                caller,
                operation,
                resource,
                &required.to_string(),
                AuditDecision::Deny,
                AuditResult::Failure,
                "rbac permission denied",
            );
            audit::record_audit(state.postgres.vala_pool(), caller.data_tenant_id, &event).await?;
            Err(permission_deny_reason_to_wyrd(reason))
        }
    }
}

/// Rejects a re-registration whose declared layout differs from the one the
/// table was created with.
///
/// A retry of the same registration must be idempotent, so the incoming
/// declaration is resolved against the table's stored physical schema and
/// compared with the stored canonical layout. Comparing canonical forms means
/// an equivalent-but-differently-spelled declaration (an omitted declaration
/// versus an explicit empty one) still retries cleanly, while a genuinely
/// different physical layout conflicts.
///
/// `stored_schema` is the provider's own schema rather than anything rebuilt
/// from the describe response: the managed Bloom floor is decided by which
/// server-stamped columns the table actually carries, and describe reports
/// only the managed column a writer may supply itself.
///
/// # Errors
///
/// Returns [`wyrd_spec::vala::BifrostError::PhysicalLayoutMismatch`]
/// (`WYRD_VALA_409_BIFROST_LAYOUT_MISMATCH`) when the layouts differ, and a
/// validation error when the incoming declaration is not valid against the
/// stored schema.
fn assert_registered_layout_matches(
    fqn: &str,
    stored_schema: &arrow::datatypes::Schema,
    stored_layout: &PhysicalLayoutWire,
    declared: Option<&PhysicalLayoutWire>,
) -> Result<(), WyrdError> {
    let canonical =
        vala_bifrost_redux::catalog::layout::PhysicalLayout::resolve(fqn, stored_schema, declared)
            .map_err(WyrdError::from)?;
    if &canonical.to_wire() == stored_layout {
        return Ok(());
    }
    Err(wyrd_spec::vala::BifrostError::PhysicalLayoutMismatch {
        table: fqn.to_owned(),
    }
    .into())
}

/// Register (idempotently create) a Bifrost table.
///
/// Dataset registration requires `bifrost_table:write`. A matching-fingerprint re-register returns
/// `AlreadyExists`; a conflicting schema is `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH`.
pub async fn register_table(
    state: &AppState,
    caller: Caller,
    body: RegisterTableRequest,
) -> Result<RegisterTableResponse, WyrdError> {
    let required = Permission::bifrost_table_write();
    let operation = "vala.bifrost.register";
    let fqn_for_audit = format!("{}.{}", body.namespace, body.name);
    authorize_audited(state, &caller, &required, operation, &fqn_for_audit).await?;

    let ns = convert::namespace_from_wire(&body.namespace)?;
    if ns != vala_bifrost_redux::namespaces::BifrostNamespace::Datasets {
        return Err(WyrdError::Validation {
            message: "only vala.datasets registrations are caller-owned".to_owned(),
            details: serde_json::json!({ "table": fqn_for_audit }),
        });
    }
    let user_fields: Vec<Field> = body.fields.iter().map(convert::field_to_arrow).collect();
    let fingerprint = convert::fingerprint_hex(&user_fields);

    let fqn = format!("{}.{}", ns.as_str(), body.name);
    let table = TableRef::new(ns, body.name.clone());
    let catalog = state
        .bifrost
        .catalog()
        .ok_or(wyrd_spec::vala::BifrostError::ScribeRoleUnavailable)?
        .as_ref();

    match catalog.describe_table(&table, caller.data_tenant_id).await {
        Ok(existing) => {
            if existing.entry.fingerprint == fingerprint {
                let stored_schema = catalog
                    .assignment_schema(&table, caller.data_tenant_id)
                    .await
                    .map_err(map_engine_error)?;
                assert_registered_layout_matches(
                    &fqn,
                    &stored_schema,
                    &existing.physical_layout,
                    body.physical_layout.as_ref(),
                )?;
                Ok(RegisterTableResponse {
                    outcome: RegisterOutcome::AlreadyExists,
                    table_uid: existing.entry.table_uid,
                    fingerprint,
                })
            } else {
                Err(wyrd_spec::vala::BifrostError::FingerprintMismatch { table: fqn }.into())
            }
        }
        Err(BifrostCatalogError::TableNotFound(_)) => {
            // Audit the successful registration in the SAME tx as the catalog row
            // (append happens inside `create_table` before its commit).
            let event = audit::audit_event(
                &caller,
                operation,
                &fqn,
                &required.to_string(),
                AuditDecision::Allow,
                AuditResult::Success,
                "bifrost table registered",
            );
            let table_uid = catalog
                .register_dataset(
                    caller.data_tenant_id,
                    table,
                    user_fields,
                    body.physical_layout.clone(),
                    Some(event),
                )
                .await
                .map_err(map_engine_error)?;
            Ok(RegisterTableResponse {
                outcome: RegisterOutcome::Created,
                table_uid: convert::to_hex(table_uid.as_bytes()),
                fingerprint,
            })
        }
        Err(other) => Err(map_engine_error(other)),
    }
}

/// List the tables visible to the caller's tenant (schema-free entries).
///
/// Listing requires `bifrost_table:read`. A denial appends a canonical
/// `decision = deny` row to the tenant audit outbox before the public 403 is
/// returned, and is fail-closed: an audit-append failure refuses the read with
/// `WYRD_VALA_500_AUDIT_UNAVAILABLE`. A successful list is an ordinary
/// tenant-bound read and records no durable transition.
///
/// # Errors
///
/// Returns a permission error when the caller lacks `bifrost_table:read`,
/// [`WyrdError::AuditUnavailable`] when the denial audit append fails,
/// [`wyrd_spec::vala::BifrostError::ScribeRoleUnavailable`] when this server
/// carries no catalog, or the mapped catalog error when the listing query
/// fails.
pub async fn list_tables(
    state: &AppState,
    caller: Caller,
) -> Result<Vec<BifrostTableEntry>, WyrdError> {
    authorize_audited(
        state,
        &caller,
        &Permission::bifrost_table_read(),
        "vala.bifrost.list",
        "bifrost.tables",
    )
    .await?;
    state
        .bifrost
        .catalog()
        .ok_or(wyrd_spec::vala::BifrostError::ScribeRoleUnavailable)?
        .list_tables(caller.data_tenant_id)
        .await
        .map_err(map_engine_error)
}

/// Describe a single table (entry plus its stored field list).
///
/// Describing requires `bifrost_table:read`. A denial appends a canonical
/// `decision = deny` row naming the requested fully qualified table before the
/// public 403 is returned, and is fail-closed: an audit-append failure refuses
/// the read. The audited resource is built from the request as received —
/// authorization is decided before namespace validation so an unauthorized
/// caller cannot distinguish a valid namespace from an invalid one — while the
/// audit tenant always remains `caller.data_tenant_id`, never request payload.
/// A successful describe records no durable transition.
///
/// # Errors
///
/// Returns a permission error when the caller lacks `bifrost_table:read`,
/// [`WyrdError::AuditUnavailable`] when the denial audit append fails,
/// a validation error for an unknown namespace,
/// [`wyrd_spec::vala::BifrostError::ScribeRoleUnavailable`] when this server
/// carries no catalog, and the mapped catalog error (including table-not-found)
/// otherwise.
pub async fn describe_table(
    state: &AppState,
    caller: Caller,
    namespace: String,
    name: String,
) -> Result<BifrostTableDescription, WyrdError> {
    let requested_fqn = format!("{namespace}.{name}");
    authorize_audited(
        state,
        &caller,
        &Permission::bifrost_table_read(),
        "vala.bifrost.describe",
        &requested_fqn,
    )
    .await?;
    let ns = convert::namespace_from_wire(&namespace)?;
    let table = TableRef::new(ns, name);
    state
        .bifrost
        .catalog()
        .ok_or(wyrd_spec::vala::BifrostError::ScribeRoleUnavailable)?
        .describe_table(&table, caller.data_tenant_id)
        .await
        .map_err(map_engine_error)
}

#[cfg(test)]
mod pg_tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    use wyrd_runtime::{PermissionSet, Principal, PrincipalId, PrincipalKind};
    use wyrd_spec::request_id::RequestId;
    use wyrd_spec::vala::api::{DataTypeSpec, FieldSpec};
    use wyrd_storage::{BackendConfig, StorageHandle, StorageSettings};

    async fn test_state() -> AppState {
        let root = tempfile::tempdir().expect("temp dir");
        let storage = StorageHandle::from_settings(StorageSettings {
            backend: BackendConfig::Local {
                root: root.path().to_path_buf(),
            },
            require_encryption: false,
            presign_ttl: Duration::from_secs(600),
            part_size_bytes: 16 * 1024 * 1024,
            multipart_threshold_bytes: 100 * 1024 * 1024,
            public_base_url: Some("https://wyrd.test".to_owned()),
        })
        .await
        .expect("local storage handle");
        let pool = crate::test_support::test_pool().await;
        let wyrd = wyrd_sql::WyrdPostgres::from_pools(pool.clone(), None);
        let vala = vala_sql::ValaPostgres::from_pool(pool);
        let postgres = Arc::new(crate::postgres::ServerPostgres::from_parts(wyrd, vala));
        crate::test_support::test_app_state(
            postgres,
            Arc::clone(&storage),
            crate::test_support::test_catalog().await,
        )
    }

    async fn caller_with(permissions: impl IntoIterator<Item = Permission>) -> Caller {
        let tenant = crate::test_support::test_tenant().await;
        Caller {
            data_tenant_id: tenant,
            principal: Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                vec![],
                PermissionSet::from_iter(permissions),
            ),
            request_id: RequestId::parse(&uuid::Uuid::now_v7().to_string())
                .expect("request id parses"),
            // Nondelegated fixture caller: no verified `act` chain exists.
            delegation_chain: Vec::new(),
        }
    }

    fn unique_name() -> String {
        format!("t_{}", uuid::Uuid::now_v7().simple())
    }

    fn field(name: &str, data_type: DataTypeSpec) -> FieldSpec {
        FieldSpec {
            name: name.to_owned(),
            data_type,
            nullable: true,
            metadata: std::collections::BTreeMap::new(),
        }
    }

    /// Read the caller's own audit-outbox rows for one resource.
    ///
    /// The rows are fetched through the canonical tenant-scoped reader, then
    /// narrowed to the caller's request ID so an assertion sees exactly the
    /// events the operation under test appended, independent of anything else
    /// the shared test tenant has recorded for the same resource.
    async fn audit_rows_for(
        state: &AppState,
        caller: &Caller,
        resource: &str,
    ) -> Vec<vala_sql::row_types::audit_outbox::AuditOutboxRow> {
        let mut conn =
            vala_sql::TenantConn::acquire(state.postgres.vala_pool(), caller.data_tenant_id)
                .await
                .expect("tenant connection");
        let rows = vala_sql::queries::audit_outbox::list_audit_events_for_resource(
            &mut conn, resource, 0, 100,
        )
        .await
        .expect("audit page");
        conn.commit().await.expect("audit read commits");
        rows.into_iter()
            .filter(|row| row.request_id == caller.request_id.as_str())
            .collect()
    }

    /// Assert one row carries the canonical RBAC-denial attribution.
    fn assert_read_denial_row(
        row: &vala_sql::row_types::audit_outbox::AuditOutboxRow,
        operation: &str,
        resource: &str,
        caller: &Caller,
    ) {
        assert_eq!(row.operation, operation);
        assert_eq!(row.resource, resource);
        assert_eq!(
            row.permission,
            Permission::bifrost_table_read().to_string(),
            "denial attributes the required read permission"
        );
        assert_eq!(row.decision, "deny");
        assert_eq!(row.result, "failure");
        assert_eq!(row.principal_id, caller.principal.id.as_uuid());
        assert_eq!(row.request_id, caller.request_id.as_str());
    }

    fn register_req(name: &str, fields: Vec<FieldSpec>) -> RegisterTableRequest {
        RegisterTableRequest {
            namespace: "vala.datasets".to_owned(),
            name: name.to_owned(),
            fields,
            physical_layout: None,
        }
    }

    // These tests drive the shared embedded-Postgres pool, whose connections
    // take reactor affinity from the runtime that establishes them. They run on
    // the process-wide persistent runtime (not a per-test `#[tokio::test]`
    // runtime) so the shared pool is never poisoned by a runtime that dies at
    // test end. See `crate::test_support::shared`.
    #[test]
    fn bifrost_tables_register_is_idempotent() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([Permission::bifrost_table_write()]).await;
            let name = unique_name();
            let req = register_req(&name, vec![field("id", DataTypeSpec::Int64)]);

            let first = register_table(&state, caller.clone(), req.clone())
                .await
                .expect("first register creates");
            assert_eq!(first.outcome, RegisterOutcome::Created);

            let second = register_table(&state, caller.clone(), req)
                .await
                .expect("second register is idempotent");
            assert_eq!(second.outcome, RegisterOutcome::AlreadyExists);
            assert_eq!(first.table_uid, second.table_uid);
            assert_eq!(first.fingerprint, second.fingerprint);
        });
    }

    #[test]
    fn bifrost_tables_register_conflicting_schema_returns_mismatch() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([Permission::bifrost_table_write()]).await;
            let name = unique_name();

            register_table(
                &state,
                caller.clone(),
                register_req(&name, vec![field("id", DataTypeSpec::Int64)]),
            )
            .await
            .expect("first register creates");

            let err = register_table(
                &state,
                caller.clone(),
                register_req(&name, vec![field("id", DataTypeSpec::Utf8)]),
            )
            .await
            .expect_err("conflicting schema is rejected");
            assert_eq!(err.status(), 409);
            assert_eq!(err.code(), "WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH");
        });
    }

    #[test]
    fn bifrost_tables_register_requires_write_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([]).await;
            let err = register_table(
                &state,
                caller,
                register_req(&unique_name(), vec![field("id", DataTypeSpec::Int64)]),
            )
            .await
            .expect_err("no permission is denied");
            assert_eq!(err.status(), 403);
        });
    }

    #[test]
    fn bifrost_tables_register_reserved_namespace_is_rejected() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let writer = caller_with([Permission::bifrost_table_write()]).await;
            let mut denied = register_req(&unique_name(), vec![field("id", DataTypeSpec::Int64)]);
            denied.namespace = "vala.traces".to_owned();
            let err = register_table(&state, writer, denied)
                .await
                .expect_err("built-in namespace is not caller-owned");
            assert_eq!(err.status(), 400);
        });
    }

    #[test]
    fn bifrost_tables_list_and_describe_reflect_registration() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([
                Permission::bifrost_table_write(),
                Permission::bifrost_table_read(),
            ])
            .await;
            let name = unique_name();

            register_table(
                &state,
                caller.clone(),
                register_req(&name, vec![field("value", DataTypeSpec::Int64)]),
            )
            .await
            .expect("register creates");

            let entries = list_tables(&state, caller.clone()).await.expect("list");
            assert!(
                entries
                    .iter()
                    .any(|e| e.name == name && e.namespace == "vala.datasets"),
                "registered table is listed"
            );

            let described = describe_table(
                &state,
                caller.clone(),
                "vala.datasets".to_owned(),
                name.clone(),
            )
            .await
            .expect("describe");
            assert_eq!(described.entry.name, name);
            assert!(
                described
                    .user_fields
                    .iter()
                    .any(|f| f.name == "value" && f.data_type == DataTypeSpec::Int64),
                "describe surfaces the user field"
            );

            assert!(
                audit_rows_for(&state, &caller, "bifrost.tables")
                    .await
                    .is_empty(),
                "a permitted list records no durable audit transition"
            );
            let table_rows =
                audit_rows_for(&state, &caller, &format!("vala.datasets.{name}")).await;
            assert_eq!(
                table_rows.len(),
                1,
                "only the registration is audited for this table"
            );
            assert_eq!(table_rows[0].operation, "vala.bifrost.register");
            assert_eq!(table_rows[0].decision, "allow");
        });
    }

    /// A denied list returns 403 and leaves one canonical deny row behind.
    #[test]
    fn bifrost_tables_list_requires_read_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([]).await;
            let err = list_tables(&state, caller.clone())
                .await
                .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);

            let rows = audit_rows_for(&state, &caller, "bifrost.tables").await;
            assert_eq!(rows.len(), 1, "denied list appends exactly one audit row");
            assert_read_denial_row(&rows[0], "vala.bifrost.list", "bifrost.tables", &caller);
        });
    }

    /// A denied describe returns 403 and audits the requested table name.
    #[test]
    fn bifrost_tables_describe_requires_read_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([]).await;
            let name = unique_name();
            let err = describe_table(
                &state,
                caller.clone(),
                "vala.datasets".to_owned(),
                name.clone(),
            )
            .await
            .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);

            let resource = format!("vala.datasets.{name}");
            let rows = audit_rows_for(&state, &caller, &resource).await;
            assert_eq!(
                rows.len(),
                1,
                "denied describe appends exactly one audit row"
            );
            assert_read_denial_row(&rows[0], "vala.bifrost.describe", &resource, &caller);
        });
    }
}
