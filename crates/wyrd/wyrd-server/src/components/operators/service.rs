//! Operator connection management shared by HTTP and MCP.
//!
//! [`OperatorConnectionControl`] is the one owner of create, list, get,
//! update, disable, and encryption renewal. Every read authorizes `operators:read`, every write
//! `operators:write`, and each decision is staged on the process audit outbox
//! before the operation runs. Secrets are sealed by [`OperatorSecrets`] before
//! any transaction opens, writes are fenced on the exact row they were
//! prepared from, and no operation returns or logs a secret.

use wyrd_runtime::Permission;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::OperatorConnectionId;
use wyrd_spec::operator_connection::{
    ConnectionSecret, CreateOperatorConnectionRequest, OperatorConnectionStatus,
    OperatorConnectionView, UpdateOperatorConnectionRequest,
};
use wyrd_sql::queries::operator_connections::{
    ConnectionChange, NewConnection, get_connection, insert_connection, list_connections,
    update_connection,
};
use zeroize::Zeroizing;

use super::secrets::{OperatorSecrets, SecretIdentity};
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
/// Audit operation of a connection encryption renewal.
const RENEW_ENCRYPTION: &str = "operator_connection.renew_encryption";

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
    serde_json::to_vec(secret).map(Zeroizing::new).map_err(|_| {
        WyrdError::OperatorConnectionInvalid {
            message: "secret could not be encoded".to_owned(),
            details: serde_json::json!({ "field": "secret" }),
        }
    })
}

/// A change to an existing connection: a typed update, a disable, or an
/// encryption renewal.
enum Change {
    /// PATCH semantics.
    Update(UpdateOperatorConnectionRequest),
    /// DELETE semantics: disable, keep the row.
    Disable,
    /// Reseal the unchanged secret under a fresh DEK; authority, status, and
    /// secret version are kept.
    RenewEncryption,
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

    /// The credential owner.
    fn secrets(&self) -> &OperatorSecrets {
        self.state.verification.operator_secrets()
    }

    /// Create a connection, sealing its secret before the insert opens.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionInvalid`] for an invalid body,
    /// [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorKeyUnavailable`] when the secret cannot be sealed,
    /// [`WyrdError::OperatorConnectionConflict`] when the provider/name exists,
    /// or a registry error.
    pub(crate) async fn create(
        &self,
        caller: &Caller,
        request: CreateOperatorConnectionRequest,
    ) -> Result<OperatorConnectionView, WyrdError> {
        let (name, config, secret) = request.split()?;
        let resource = format!("operator_connection:{}/{name}", config.provider());
        self.authorize_write(caller, CREATE, &resource)?;
        let connection_id = OperatorConnectionId::new_v7();
        let identity = SecretIdentity::new(
            caller.data_tenant_id,
            connection_id,
            config.provider(),
            &name,
            1,
        );
        let committed = async {
            let sealed = self.secrets().seal(&identity, &plaintext(&secret)?).await?;
            let mut conn = self
                .state
                .registry_tenant_conn(caller.data_tenant_id)
                .await?;
            let stored = insert_connection(
                &mut conn,
                &NewConnection {
                    connection_id,
                    name: &name,
                    config: &config,
                    sealed: &sealed,
                    secret_version: 1,
                    attribution: caller.attribution(),
                },
            )
            .await
            .map_err(registry_db_error)?;
            conn.commit().await.map_err(registry_db_error)?;
            Ok::<_, WyrdError>(stored)
        }
        .await;
        match committed? {
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
    /// or a registry error.
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
        )?;
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
    /// connection, or a registry error.
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
        )?;
        let mut conn = self
            .state
            .registry_tenant_conn(caller.data_tenant_id)
            .await?;
        let row = get_connection(&mut conn, connection_id)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        row.map(|row| row.view)
            .ok_or_else(|| not_found(connection_id))
    }

    /// Apply a typed update: omitted fields keep, a supplied secret replaces
    /// the ciphertext on the same identity with the next secret version.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorConnectionNotFound`],
    /// [`WyrdError::OperatorConnectionInvalid`] for a wrong provider or
    /// invalid value, [`WyrdError::OperatorKeyUnavailable`] when a supplied
    /// secret cannot be sealed, [`WyrdError::OperatorConnectionConflict`]
    /// when a concurrent write changed the connection first, or a registry
    /// error.
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
    /// [`WyrdError::OperatorConnectionNotFound`], or a registry error.
    pub(crate) async fn disable(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.change(caller, connection_id, DISABLE, Change::Disable)
            .await
    }

    /// Renew a connection's encryption: reseal its unchanged secret under a
    /// fresh DEK and nonce, keeping its identity, authority, status, and
    /// secret version. A disabled connection stays disabled.
    ///
    /// This does not change or revoke the credential at its provider; a
    /// leaked or expired credential needs a replacement through
    /// [`Self::update`].
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] without `operators:write`,
    /// [`WyrdError::OperatorConnectionNotFound`],
    /// [`WyrdError::OperatorKeyUnavailable`] when the stored secret cannot be
    /// opened or resealed, [`WyrdError::OperatorConnectionConflict`] when a
    /// concurrent write changed the connection first, or a registry error.
    pub(crate) async fn renew_encryption(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.change(
            caller,
            connection_id,
            RENEW_ENCRYPTION,
            Change::RenewEncryption,
        )
        .await
    }

    /// Read, prepare, and store one connection after staging its decision.
    ///
    /// The row is read in its own transaction, a replacement secret is sealed
    /// with no transaction open, and the write is fenced on the authority,
    /// status, secret version, and envelope read, so a key-service wait holds
    /// no row lock. A concurrent change that commits first wins the fence, and
    /// this change is refused rather than applied over the winner.
    ///
    /// # Errors
    /// Returns [`WyrdError::OperatorConnectionConflict`] when a concurrent
    /// change committed first, and the refusals named on [`Self::update`].
    async fn change(
        &self,
        caller: &Caller,
        connection_id: OperatorConnectionId,
        operation: &str,
        change: Change,
    ) -> Result<OperatorConnectionView, WyrdError> {
        self.authorize_write(
            caller,
            operation,
            &format!("operator_connection:{connection_id}"),
        )?;
        let tenant = caller.data_tenant_id;
        let mut conn = self.state.registry_tenant_conn(tenant).await?;
        let current = get_connection(&mut conn, connection_id)
            .await
            .map_err(registry_db_error)?;
        conn.commit().await.map_err(registry_db_error)?;
        let current = current.ok_or_else(|| not_found(connection_id))?;
        let update = match &change {
            Change::Disable => wyrd_spec::operator_connection::ConnectionUpdate {
                config: current.view.config.clone(),
                secret: None,
                status: Some(OperatorConnectionStatus::Disabled),
            },
            Change::RenewEncryption => wyrd_spec::operator_connection::ConnectionUpdate {
                config: current.view.config.clone(),
                secret: None,
                status: None,
            },
            Change::Update(request) => request.clone().apply(current.view.config.clone())?,
        };
        let (sealed, secret_version) = match (&change, &update.secret) {
            (Change::RenewEncryption, _) => (
                Some(self.secrets().renew(tenant, &current).await?),
                current.secret_version,
            ),
            (_, Some(secret)) => {
                let next = current.secret_version + 1;
                let sealed = self
                    .secrets()
                    .seal(
                        &SecretIdentity::of(tenant, &current.view, next),
                        &plaintext(secret)?,
                    )
                    .await?;
                (Some(sealed), next)
            }
            (_, None) => (None, current.secret_version),
        };
        let mut conn = self.state.registry_tenant_conn(tenant).await?;
        let stored = update_connection(
            &mut conn,
            &current,
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
        stored
            .map(|stored| stored.view)
            .ok_or_else(|| WyrdError::OperatorConnectionConflict {
                message: "the Operator connection changed concurrently; read it again and retry"
                    .to_owned(),
                details: serde_json::json!({ "connection_id": connection_id }),
            })
    }

    /// Evaluate `operators:write` and stage the decision on the process audit
    /// outbox before the operation runs.
    ///
    /// # Errors
    /// Returns [`WyrdError::PermissionDeniedRbac`] when the caller lacks
    /// `operators:write`.
    fn authorize_write(
        &self,
        caller: &Caller,
        operation: &str,
        resource: &str,
    ) -> Result<(), WyrdError> {
        audit::authorize(
            self.state,
            caller,
            &Permission::operators_write(),
            operation,
            resource,
        )
    }
}
