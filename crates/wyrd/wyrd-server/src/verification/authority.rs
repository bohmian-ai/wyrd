//! The tokenless read authority every Verifier input read runs under.
//!
//! A Verifier reads its inputs in-process as its tenant's SYSTEM principal
//! (REQ-086). The run's claim returns that principal's stable id from
//! tenant-owned state, and [`SystemReadAuthority::resolve`] turns it into an
//! authorized Oracle query context whose only permissions are
//! `bifrost_query:read` grants scoped by registered table UID to exactly the
//! tables the run reads. No token is minted or verified; Oracle's table
//! authorization enforces and audits every read under it like any caller's.
//!
//! LLM judge calls are not reads: [`writer_caller`] recovers the observation
//! writer's own current authority for them, never the SYSTEM principal's.

use std::sync::Arc;

use vala_bifrost_redux::catalog::{BifrostCatalogError, TableRef};
use vala_bifrost_redux::oracle::AuthorizedQueryContext;
use vala_sql::queries::olap_catalog::get_by_fqn;
use wyrd_auth::issuance::IssuanceError;
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_runtime::{
    Action, BifrostPermissionScope, BifrostTableScope, Permission, PermissionScope,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuthMethod;
use wyrd_spec::vala::error::BifrostError;

use crate::components::auth::{AuthenticatedPrincipal, Caller};
use crate::state::AppState;

/// Why a SYSTEM read authority cannot be built for a run.
#[derive(Debug, thiserror::Error)]
pub enum SystemReadAuthorityError {
    /// The tenant has no active SYSTEM principal with a `UUIDv7` id.
    #[error("the tenant has no active System principal")]
    SystemPrincipalMissing,
    /// Reading an input table's registered identity failed.
    #[error("the System read authority is unavailable: {0}")]
    Unavailable(String),
    /// The authorized context's tenant invariant does not hold.
    #[error(transparent)]
    Context(#[from] BifrostError),
}

/// A tenant SYSTEM principal authorized to read exactly one run's input tables.
///
/// The principal is credentialless and role-free, carries an empty Card
/// scope, and holds one `bifrost_query:read` grant per input table the tenant
/// has registered, scoped to that table's UID. It never carries a general
/// query grant or any write scope, and it is never issued as a token.
#[derive(Debug, Clone)]
pub struct SystemReadAuthority {
    /// The SYSTEM principal's authorized query context for one tenant.
    context: AuthorizedQueryContext,
}

impl SystemReadAuthority {
    /// Authorize `principal` to read `tables` in `tenant`.
    ///
    /// `principal` is the SYSTEM principal the run's claim returned. Each
    /// table's registered UID comes from this process's Bifrost catalog cache
    /// when the pod runs Scribe or Oracle, and otherwise from one short read
    /// of the tenant's catalog row. A table the tenant has not registered yet
    /// gets no grant, so a read of it stays not-found; nothing is created or
    /// registered.
    ///
    /// # Errors
    /// Returns [`SystemReadAuthorityError::SystemPrincipalMissing`] when
    /// `principal` is absent or not `UUIDv7`,
    /// [`SystemReadAuthorityError::Unavailable`] when a table identity cannot
    /// be read or is malformed, and [`SystemReadAuthorityError::Context`] when
    /// the context's tenant invariant fails.
    pub async fn resolve(
        state: &AppState,
        tenant: DataTenantId,
        principal: Option<PrincipalId>,
        tables: &[TableRef],
    ) -> Result<Self, SystemReadAuthorityError> {
        let principal = principal
            .filter(|id| id.as_uuid().get_version_num() == 7)
            .ok_or(SystemReadAuthorityError::SystemPrincipalMissing)?;
        let mut grants = Vec::new();
        for table in tables {
            let Some(table_uid) = Self::table_uid(state, tenant, table).await? else {
                continue;
            };
            let (catalog, schema) = table
                .namespace
                .as_str()
                .split_once('.')
                .unwrap_or(("vala", table.namespace.as_str()));
            grants.push(Permission {
                resource: wyrd_runtime::Resource::BifrostQuery,
                action: Action::Read,
                scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                    catalog: catalog.to_owned(),
                    schema: schema.to_owned(),
                    table_uid,
                })),
            });
        }
        let principal = Principal::new(
            principal,
            PrincipalKind::System {
                card_ref_scope: CardRefScope::default(),
            },
            tenant,
            Vec::new(),
            PermissionSet::from_iter(grants),
        );
        let context = AuthorizedQueryContext::try_new(
            principal,
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            Permission::bifrost_query_read(),
        )?;
        Ok(Self { context })
    }

    /// The registered UID of `table` in `tenant`, or `None` when unregistered.
    ///
    /// # Errors
    /// Returns [`SystemReadAuthorityError::Unavailable`] when the catalog read
    /// fails or the stored UID is malformed.
    async fn table_uid(
        state: &AppState,
        tenant: DataTenantId,
        table: &TableRef,
    ) -> Result<Option<uuid::Uuid>, SystemReadAuthorityError> {
        let unavailable = |error: &dyn std::fmt::Display| {
            SystemReadAuthorityError::Unavailable(error.to_string())
        };
        if let Some(catalog) = state.bifrost.catalog() {
            return match catalog.table_uid(table, tenant).await {
                Ok(uid) => Ok(Some(uuid::Uuid::from_bytes(*uid.as_bytes()))),
                Err(BifrostCatalogError::TableNotFound(_)) => Ok(None),
                Err(error) => Err(unavailable(&error)),
            };
        }
        // ponytail: a pod without Scribe or Oracle has no catalog cache, so it
        // reads the catalog row per run; cache it here if that read shows up.
        let mut conn = state
            .postgres
            .vala()
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(&error))?;
        let row = get_by_fqn(&mut conn, &table.fqn())
            .await
            .map_err(|error| unavailable(&error))?;
        row.map(|row| uuid::Uuid::from_slice(&row.table_uid))
            .transpose()
            .map_err(|error| unavailable(&error))
    }

    /// The SYSTEM principal's authorized query context.
    #[must_use]
    pub const fn context(&self) -> &AuthorizedQueryContext {
        &self.context
    }

    /// Consume the authority into its authorized query context.
    #[must_use]
    pub fn into_context(self) -> AuthorizedQueryContext {
        self.context
    }
}

/// Recover the current authority of `principal`, the stored writer of one
/// observation, as the caller its queued LLM judge calls run under.
///
/// The writer is resolved through the tenant issuer's
/// [`recover`](wyrd_auth::issuance::TenantTokenIssuer::recover) with its
/// current roles, permissions, and Card scope, so the gateway authorizes,
/// accounts, captures, and audits each judge call as that writer's own.
/// Nothing is cached, presented, or minted.
///
/// # Errors
/// Returns [`WyrdError::CredentialRevoked`] when the writer is missing or
/// not active, [`WyrdError::Internal`] when `principal` is not a principal
/// id or no issuer is configured, and the issuer's store-unavailable error
/// when the tenant store cannot be read.
pub async fn writer_caller(
    state: &AppState,
    tenant: DataTenantId,
    principal: &str,
) -> Result<Caller, WyrdError> {
    let internal = |message: &str| WyrdError::Internal {
        message: message.to_owned(),
        details: serde_json::json!({}),
    };
    let principal = uuid::Uuid::parse_str(principal)
        .map_err(|_| internal("the observation writer is not a principal id"))?;
    let issuer = state
        .auth
        .tenant_issuer(&state.scribe_outbox)
        .ok_or_else(|| internal("no tenant token issuer is configured"))?;
    let mut conn = state
        .postgres
        .tenant_conn(tenant)
        .await
        .map_err(|error| WyrdError::from(IssuanceError::Store(error)))?;
    let verified = issuer.recover(&mut conn, principal).await?;
    conn.commit()
        .await
        .map_err(|error| WyrdError::from(IssuanceError::Store(error)))?;
    Ok(Caller::from_authenticated(
        &AuthenticatedPrincipal::from_verified(Arc::new(verified)),
        RequestId::now_v7(),
    ))
}
