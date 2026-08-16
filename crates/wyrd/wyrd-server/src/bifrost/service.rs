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
    AuditDecision, AuditResult, BifrostTableDescription, BifrostTableEntry, RegisterOutcome,
    RegisterTableRequest, RegisterTableResponse,
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

/// Authorize the caller against a required permission before execution.
fn authorize(state: &AppState, caller: &Caller, required: &Permission) -> Result<(), WyrdError> {
    state
        .authz
        .permission_check
        .check(&caller.principal, required)
        .into_result()
        .map_err(permission_deny_reason_to_wyrd)
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

    let mut partition_columns = Vec::with_capacity(body.partition_columns.len());
    for spec in &body.partition_columns {
        partition_columns.push(convert::partition_column_to_engine(spec)?);
    }
    if !partition_columns.is_empty()
        && partition_columns
            != [(
                "wyrd_event_time".to_owned(),
                vala_bifrost_redux::catalog::PartitionTransform::Day,
            )]
    {
        return Err(WyrdError::Validation {
            message: "Redux Bifrost tables use the fixed wyrd_event_time/day partition".to_owned(),
            details: serde_json::json!({ "table": fqn_for_audit }),
        });
    }

    let fqn = format!("{}.{}", ns.as_str(), body.name);
    let table = TableRef::new(ns, body.name.clone());
    let catalog = state.bifrost.as_ref();

    match catalog.describe_table(&table, caller.data_tenant_id).await {
        Ok(existing) => {
            if existing.entry.fingerprint == fingerprint {
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
                .register_dataset(caller.data_tenant_id, table, user_fields, Some(event))
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
pub async fn list_tables(
    state: &AppState,
    caller: Caller,
) -> Result<Vec<BifrostTableEntry>, WyrdError> {
    authorize(state, &caller, &Permission::bifrost_table_read())?;
    state
        .bifrost
        .list_tables(caller.data_tenant_id)
        .await
        .map_err(map_engine_error)
}

/// Describe a single table (entry plus its stored field list).
pub async fn describe_table(
    state: &AppState,
    caller: Caller,
    namespace: String,
    name: String,
) -> Result<BifrostTableDescription, WyrdError> {
    authorize(state, &caller, &Permission::bifrost_table_read())?;
    let ns = convert::namespace_from_wire(&namespace)?;
    let table = TableRef::new(ns, name);
    state
        .bifrost
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
        AppState::new(
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

    fn register_req(name: &str, fields: Vec<FieldSpec>) -> RegisterTableRequest {
        RegisterTableRequest {
            namespace: "vala.datasets".to_owned(),
            name: name.to_owned(),
            fields,
            partition_columns: vec![],
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

            let described =
                describe_table(&state, caller, "vala.datasets".to_owned(), name.clone())
                    .await
                    .expect("describe");
            assert_eq!(described.entry.name, name);
            assert!(
                described
                    .fields
                    .iter()
                    .any(|f| f.name == "value" && f.data_type == DataTypeSpec::Int64),
                "describe surfaces the user field"
            );
        });
    }

    #[test]
    fn bifrost_tables_list_requires_read_permission() {
        wyrd_runtime::runtime().block_on(async {
            let state = test_state().await;
            let caller = caller_with([]).await;
            let err = list_tables(&state, caller)
                .await
                .expect_err("no read permission is denied");
            assert_eq!(err.status(), 403);
        });
    }
}
