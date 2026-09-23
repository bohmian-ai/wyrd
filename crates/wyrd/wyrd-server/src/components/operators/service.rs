//! Operator connection management shared by HTTP and MCP.
//!
//! [`OperatorConnectionControl`] is the one owner of create, list, get,
//! update, and disable. Every read authorizes `operators:read`, every write
//! `operators:write`, and each write appends its allowed decision inside the
//! transaction that performs it. Secrets are sealed by [`OperatorKeys`] before
//! they reach SQL, and no operation returns or logs one.

use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::OperatorConnectionId;
use wyrd_spec::operator_connection::{
    ConnectionSecret, CreateOperatorConnectionRequest, OperatorConnectionStatus,
    OperatorConnectionView, UpdateOperatorConnectionRequest,
};
use wyrd_spec::vala::api::AuditEvent;
use wyrd_sql::queries::operator_connections::{
    ConnectionChange, NewConnection, get_connection, insert_connection, list_connections,
    update_connection,
};
use zeroize::Zeroizing;

use super::keys::{OperatorKeys, SecretIdentity};
use crate::audit;
use crate::components::auth::Caller;
use crate::state::{AppState, registry_db_error};

/// Audit operation of a connection create.
const CREATE: &str = "operator_connection.create";
/// Audit operation of a connection list.
const LIST: &str = "operator_connection.list";
/// Audit operation of a connection read.
const GET: &str = "operator_connection.read";
/// Audit operation of a connection update.
const UPDATE: &str = "operator_connection.update";
/// Audit operation of a connection disable.
const DISABLE: &str = "operator_connection.disable";

/// The not-found refusal for `connection_id`.
fn not_found(connection_id: OperatorConnectionId) -> WyrdError {
    WyrdError::OperatorConnectionNotFound {
        message: "no Operator connection with this ID in the caller's tenant".to_owned(),
        details: serde_json::json!({ "connection_id": connection_id }),
    }
}

/// Serialize a secret into zeroizing plaintext for sealing.
///
/// # Errors
/// Returns [`WyrdError::OperatorConnectionInvalid`] when it cannot serialize.
fn plaintext(secret: &ConnectionSecret) -> Result<Zeroizing<Vec<u8>>, WyrdError> {
    serde_json::to_vec(secret)
        .map(Zeroizing::new)
        .map_err(|_| WyrdError::OperatorConnectionInvalid {
            message: "secret could not be encoded".to_owned(),
            details: serde_json::json!({ "field": "secret" }),
        })
}

/// A change to an existing connection: a typed update or a disable.
enum Change {
    /// PATCH semantics.
    Update(UpdateOperatorConnectionRequest),
    /// DELETE semantics: disable, keep the row.
    Disable,
}

/// Owner of Operator connection management for one request.
pub(crate) struct OperatorConnectionControl<'a> {
    /// Server state providing tenant connections, authorization, audit, and keys.
    state: &'a AppState,
}

impl<'a> OperatorConnectionControl<'a> {
    /// Build the control plane over this process's state.
    pub(crate) const fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    /// The key owner.
    fn keys(&self) -> &OperatorKeys {
        &self.state.operator_keys
    }

    /// Create a connection, sealing its secret under the active key version.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an invalid body,
    /// [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorKeyUnavailable`] when no active key is readable,
    /// [`WyrdError::OperatorConnectionConflict`] when the provider/name exists,
    /// [`WyrdError::AuditUnavailable`], or a registry error.
    pub(crate) async fn create(
        &self,
        caller: &Caller,
        request: CreateOperatorConnectionRequest,
    ) -> Result<OperatorConnectionView, WyrdError> {
        let (name, config, secret) = request.split()?;
        let resource = format!("operator_connection:{}/{name}", config.provider());
        let allowed = self.authorize_write(caller, CREATE, &resource).await?;
        let connection_id = OperatorConnectionId::new_v7();
        let identity = SecretIdentity {
            tenant: caller.data_tenant_id,
            connection_id,
            provider: config.provider(),
            name: &name,
            secret_version: 1,
        };
        let committed = async {
            let sealed = self.keys().seal(identity, &plaintext(&secret)?).await?;
            let mut conn = self
                .state
                .registry_tenant_conn(caller.data_tenant_id)
                .await?;
            audit::append_on(&mut conn, &allowed).await?;
            let stored = insert_connection(
                &mut conn,
                &NewConnection {
                    connection_id,
                    name: &name,
                    config: &config,
                    sealed: &sealed,
                    secret_version: 1,
                    principal: caller.principal.id,
                },
            )
            .await
            .map_err(registry_db_error)?;
            conn.commit().await.map_err(registry_db_error)?;
            Ok::<_, WyrdError>(stored)
        }
        .await;
        match self.record_if_uncommitted(caller, &allowed, committed).await? {
            Some(stored) => Ok(stored.view),
            None => Err(WyrdError::OperatorConnectionConflict {
                message: format!(
                    "a {} connection named {name} already exists",
                    config.provider()
                ),
                details: serde_json::json!({ "provider": config.provider(), "name": name }),
            }),
        }
    }

    /// List the tenant's connections, redacted.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:read`,
    /// [`WyrdError::AuditUnavailable`], or a registry error.
    pub(crate) async fn list(
        &self,
        caller: &Caller,
    ) -> Result<Vec<OperatorConnectionView>, WyrdError> {
        audit::authorize(
            self.state,
            caller,
            &Permission::operators_read(),
            LIST,
            "operator_connections",
        )
        .await?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let rows = list_connections(&mut conn)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        Ok(rows.into_iter().map(|row| row.view).collect())
    }

    /// Read one connection, redacted.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:read`,
    /// [`WyrdError::OperatorConnectionNotFound`] when the tenant has no such
    /// connection, [`WyrdError::AuditUnavailable`], or a registry error.
    pub(crate) async fn get(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        audit::authorize(
            self.state,
            caller,
            &Permission::operators_read(),
            GET,
            &format!("operator_connection:{connection_id}"),
        )
        .await?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let row = get_connection(&mut conn, connection_id, false)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        row.map(|row| row.view).ok_or_else(|| not_found(connection_id))
    }

    /// Apply a typed update: omitted fields keep, a supplied secret replaces
    /// the ciphertext on the same identity with the next secret version.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorConnectionNotFound`],
    /// [`WyrdError::OperatorConnectionInvalid`] for a wrong provider or
    /// invalid value, [`WyrdError::OperatorKeyUnavailable`] when a secret is
    /// supplied and no active key is readable, [`WyrdError::AuditUnavailable`],
    /// or a registry error.
    pub(crate) async fn update(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
        request: UpdateOperatorConnectionRequest,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.change(caller, connection_id, UPDATE, Change::Update(request))
            .await
    }

    /// Disable a connection; the row is kept for lineage and may be
    /// re-enabled by an update.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorConnectionNotFound`], [`WyrdError::AuditUnavailable`],
    /// or a registry error.
    pub(crate) async fn disable(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.change(caller, connection_id, DISABLE, Change::Disable)
            .await
    }

    /// Lock, change, and store one connection with its audit decision.
    ///
    /// A committed refusal (not found, invalid) is returned in the inner
    /// result; the allowed decision is recorded standalone only when nothing
    /// committed.
    ///
    /// # Errors
    /// Returns the refusals named on [`Self::update`].
    async fn change(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
        operation: &str,
        change: Change,
    ) -> Result<OperatorConnectionView, WyrdError> {
        let allowed = self
            .authorize_write(
                caller,
                operation,
                &format!("operator_connection:{connection_id}"),
            )
            .await?;
        let committed = async {
            let mut conn = self
                .state
                .registry_tenant_conn(caller.data_tenant_id)
                .await?;
            audit::append_on(&mut conn, &allowed).await?;
            let Some(current) = get_connection(&mut conn, connection_id, true)
                .await
                .map_err(registry_db_error)?
            else {
                conn.commit().await.map_err(registry_db_error)?;
                return Ok(Err(not_found(connection_id)));
            };
            let update = match change {
                Change::Disable => wyrd_spec::operator_connection::ConnectionUpdate {
                    config: current.view.config.clone(),
                    secret: None,
                    status: Some(OperatorConnectionStatus::Disabled),
                },
                Change::Update(request) => match request.apply(current.view.config.clone()) {
                    Ok(update) => update,
                    Err(refusal) => {
                        conn.commit().await.map_err(registry_db_error)?;
                        return Ok(Err(refusal));
                    }
                },
            };
            let secret_version = current.secret_version + 1;
            let sealed = match &update.secret {
                Some(secret) => Some(
                    self.keys()
                        .seal(
                            SecretIdentity {
                                tenant: caller.data_tenant_id,
                                connection_id,
                                provider: current.view.config.provider(),
                                name: &current.view.name,
                                secret_version,
                            },
                            &plaintext(secret)?,
                        )
                        .await?,
                ),
                None => None,
            };
            let stored = update_connection(
                &mut conn,
                connection_id,
                &ConnectionChange {
                    config: &update.config,
                    status: update.status.unwrap_or(current.view.status),
                    secret: sealed.as_ref().map(|sealed| (sealed, secret_version)),
                    principal: caller.principal.id,
                },
            )
            .await
            .map_err(registry_db_error)?;
            conn.commit().await.map_err(registry_db_error)?;
            Ok::<_, WyrdError>(Ok(stored.view))
        }
        .await;
        self.record_if_uncommitted(caller, &allowed, committed)
            .await?
    }

    /// Evaluate `operators:write`, recording a denial standalone and
    /// returning the allowed decision for the operation transaction.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] or
    /// [`WyrdError::AuditUnavailable`].
    async fn authorize_write(
        &self,
        caller: &Caller,
        operation: &str,
        resource: &str,
    ) -> Result<AuditEvent, WyrdError> {
        audit::authorize_recording_denial(
            self.state,
            caller,
            &Permission::operators_write(),
            operation,
            resource,
        )
        .await
    }

    /// Pass a committed result through; for a transaction that did not
    /// commit, record the allowed decision standalone so an evaluated
    /// permission is never lost, then return its error.
    ///
    /// # Errors
    /// Returns the uncommitted error, or [`WyrdError::AuditUnavailable`] when
    /// the standalone decision cannot be recorded.
    async fn record_if_uncommitted<T>(
        &self,
        caller: &Caller,
        allowed: &AuditEvent,
        committed: Result<T, WyrdError>,
    ) -> Result<T, WyrdError> {
        match committed {
            Ok(value) => Ok(value),
            Err(error) => {
                audit::record_audit(
                    self.state.postgres.vala_pool(),
                    caller.data_tenant_id,
                    allowed,
                )
                .await?;
                Err(error)
            }
        }
    }
}
