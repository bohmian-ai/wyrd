//! The tokenless read authority scheduled Drift retained-row reads run under.
//!
//! A scheduled Drift Verifier reads its retained observation rows in-process
//! as its tenant's SYSTEM principal (REQ-086); caller-submitted trace inputs
//! are read under the submitting caller's own authorization, not here. The
//! run's claim returns that principal's stable id from tenant-owned state,
//! and [`SystemReadAuthority::resolve`] turns it into an
//! authorized Oracle query context whose only permissions are
//! `bifrost_query:read` grants scoped by registered table UID to exactly the
//! tables the run reads. No token is minted or verified; Oracle's table
//! authorization enforces and audits every read under it like any caller's.
//!
//! Each read is recorded for the run's subject with SYSTEM as actor, and the
//! run's accepted delegation chain as history, through
//! [`SystemReadAuthority::serving`]; neither authorizes anything.
//!
//! Agent-check calls are not reads: they run under the execution's accepted authority
//! with SYSTEM as actor. [`BackgroundAuthority`] resolves that authority for
//! the runs Wyrd starts itself.

use vala_bifrost_redux::catalog::{BifrostCatalogError, TableRef};
use vala_bifrost_redux::oracle::{AuthorizedQueryContext, ServedSubject};
use vala_sql::queries::olap_catalog::get_by_fqn;
use wyrd_auth::issuance::{IssuanceError, TenantTokenIssuer};
use wyrd_runtime::permission::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_runtime::{
    AcceptedAuthority, Action, BifrostPermissionScope, BifrostTableScope, Permission,
    PermissionScope,
};
use wyrd_spec::DataTenantId;
use wyrd_spec::reference::CardRefScope;
use wyrd_spec::request_id::RequestId;
use wyrd_spec::vala::api::AuthMethod;
use wyrd_spec::vala::error::BifrostError;
use wyrd_sql::queries::auth::service_accounts::system_principal_id;
use wyrd_sql::queries::verifier_executions::ExecutionAuthority;
use wyrd_sql::{SqlError, TenantConn};

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

    /// Record every read under this authority as served for the run's
    /// accepted subject, with SYSTEM as actor and the accepted
    /// initiator-first delegation chain as its history.
    ///
    /// Attribution only: the reads stay authorized by the SYSTEM principal's
    /// narrow table grants, never by the subject's permissions or the chain.
    /// The chain rides on the query context, so a read forwarded to a peer
    /// Oracle records the same history.
    #[must_use]
    pub fn serving(self, accepted: &AcceptedAuthority) -> Self {
        Self {
            context: self
                .context
                .serving(ServedSubject {
                    id: accepted.principal.id,
                    kind: accepted.principal.kind.tag(),
                })
                .with_delegation_chain(accepted.delegation_chain.clone()),
        }
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

/// Resolves the authority a background Verifier run is accepted under.
///
/// Scheduled runs serve their binding owner: at acceptance
/// the owner's current authority is recovered through the tenant issuer's
/// [`recover`](TenantTokenIssuer::recover), on the accepting transaction, and
/// paired with the tenant SYSTEM principal as actor. An owner that is missing,
/// inactive, or not admitted, or a tenant without SYSTEM, yields no authority
/// and therefore no run; nothing falls back to another identity.
#[derive(Clone)]
pub struct BackgroundAuthority {
    /// Tenant issuer that recovers the owner's current authority; `None`
    /// when no signing key is configured, so no background run is accepted.
    issuer: Option<TenantTokenIssuer>,
}

impl BackgroundAuthority {
    /// Resolve background authority through `issuer`.
    #[must_use]
    pub const fn new(issuer: Option<TenantTokenIssuer>) -> Self {
        Self { issuer }
    }

    /// The owner/SYSTEM authority a run serving `owner` is accepted under,
    /// or `None` when either identity cannot be resolved in `conn`'s tenant.
    ///
    /// Reads SYSTEM and recovers the owner on `conn` without writing, so the
    /// resolution commits or rolls back with the run it authorizes.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a tenant read fails; the caller's
    /// transaction is then unusable and the work is retried.
    pub async fn resolve(
        &self,
        conn: &mut TenantConn<'_>,
        owner: PrincipalId,
    ) -> Result<Option<ExecutionAuthority>, SqlError> {
        let Some(issuer) = &self.issuer else {
            tracing::warn!(%owner, "no tenant issuer is configured; background run not accepted");
            return Ok(None);
        };
        let Some(system) = system_principal_id(conn)
            .await
            .map_err(SqlError::Query)?
            .map(PrincipalId::new)
        else {
            tracing::warn!(%owner, "the tenant has no SYSTEM principal; background run not accepted");
            return Ok(None);
        };
        match issuer.recover(conn, owner.as_uuid()).await {
            Ok(verified) => Ok(Some(ExecutionAuthority::background(
                AcceptedAuthority::new(&verified.principal, &verified.delegation_chain),
                system,
            ))),
            Err(IssuanceError::Store(error)) => Err(error),
            Err(IssuanceError::Database(error)) => Err(SqlError::Query(error)),
            Err(error) => {
                tracing::info!(%owner, %error, "binding owner has no authority; background run not accepted");
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use wyrd_runtime::{DelegationStep, PrincipalRef};

    use super::*;

    /// Serving an accepted delegated authority adds its subject and history
    /// to the SYSTEM read context and nothing else (FIND-TASK-001-6).
    ///
    /// The Drift reader builds its context through
    /// [`SystemReadAuthority::serving`]. The served context keeps SYSTEM as
    /// its principal with exactly its one table grant, records User A as the
    /// served subject and `[B]` as the chain, and keeps both across the JSON
    /// form a forwarded read's envelope carries.
    ///
    /// # Panics
    /// Panics when the context cannot be built or serialized, or when the
    /// served context's principal, grants, subject, or chain differ.
    #[test]
    fn serving_keeps_accepted_history_without_widening() {
        let tenant = DataTenantId::new_v7();
        let principal = |kind| {
            Principal::new(
                PrincipalId::new(uuid::Uuid::now_v7()),
                kind,
                tenant,
                Vec::new(),
                PermissionSet::default(),
            )
        };
        let grant = Permission {
            resource: wyrd_runtime::Resource::BifrostQuery,
            action: Action::Read,
            scope: PermissionScope::Bifrost(BifrostPermissionScope::Table(BifrostTableScope {
                catalog: "vala".to_owned(),
                schema: "drift".to_owned(),
                table_uid: uuid::Uuid::now_v7(),
            })),
        };
        let system = Principal::new(
            PrincipalId::new(uuid::Uuid::now_v7()),
            PrincipalKind::System {
                card_ref_scope: CardRefScope::default(),
            },
            tenant,
            Vec::new(),
            PermissionSet::from_iter([grant]),
        );
        let context = AuthorizedQueryContext::try_new(
            system.clone(),
            tenant,
            RequestId::now_v7(),
            None,
            AuthMethod::Internal,
            Permission::bifrost_query_read(),
        )
        .expect("the SYSTEM principal is in the tenant");
        let (a, b) = (
            principal(PrincipalKind::User),
            principal(PrincipalKind::User),
        );
        let chain = vec![DelegationStep {
            principal: PrincipalRef::from_principal(&b),
        }];
        let accepted = AcceptedAuthority::new(&a, &chain);

        let served = SystemReadAuthority { context }
            .serving(&accepted)
            .into_context();
        let forwarded: AuthorizedQueryContext =
            serde_json::from_value(serde_json::to_value(&served).expect("the context serializes"))
                .expect("the context deserializes");

        for context in [&served, &forwarded] {
            assert_eq!(context.principal, system, "SYSTEM keeps only its grant");
            assert_eq!(
                context.served_subject,
                Some(ServedSubject {
                    id: a.id,
                    kind: a.kind.tag(),
                })
            );
            assert_eq!(context.delegation_chain, chain);
        }
    }
}
