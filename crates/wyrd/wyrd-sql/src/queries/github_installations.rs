//! Tenant GitHub App installation associations in `wyrd.github_installations`.
//!
//! A tenant has at most one connection slot. [`associate_github_installation`]
//! completes a verified connect callback only against the exact slot and
//! revision the connection transaction began from, so a callback begun before
//! a disconnect, reconnect or replacement can never restore the old
//! association. The webhook writers ([`set_github_installation_suspended`],
//! [`remove_github_repositories`]) and [`release_github_installation`] change
//! only an associated slot and advance its revision. Every function runs on
//! the caller's RLS [`TenantConn`] and never commits, so a caller composes the
//! write with its fence. The primary key and the
//! `github_installations_one_tenant` unique index enforce one installation per
//! tenant and one tenant per installation, including under concurrent claims.
// raw-query grep allowlist: GitHub installation tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use chrono::{DateTime, Utc};
use sqlx::types::Json;
use uuid::Uuid;
use wyrd_spec::github_connection::GithubRepositoryAccess;

use crate::{SqlError, TenantConn};

/// Unique index refusing a second tenant for one (host, installation).
const ONE_TENANT_INDEX: &str = "github_installations_one_tenant";

/// Every column a slot read returns, as a literal for `concat!`.
macro_rules! columns {
    () => {
        "connection_id, revision, host, installation_id, repositories, suspended_at, \
         connected_at, updated_at"
    };
}

/// Read the tenant's slot.
const SLOT_SQL: &str = concat!("SELECT ", columns!(), " FROM wyrd.github_installations");

/// Read and lock the tenant's slot.
const LOCK_SLOT_SQL: &str = concat!(
    "SELECT ",
    columns!(),
    " FROM wyrd.github_installations FOR UPDATE"
);

/// Create the tenant's slot with its first association. A concurrent first
/// association by the same tenant inserts nothing.
const INSERT_SQL: &str = concat!(
    "INSERT INTO wyrd.github_installations (
         data_tenant_id, connection_id, revision, host, installation_id, repositories,
         connected_at
     ) VALUES (wyrd.current_tenant(), $1, 1, $2, $3, $4, statement_timestamp())
     ON CONFLICT (data_tenant_id) DO NOTHING
     RETURNING ",
    columns!()
);

/// Associate a released slot, or refresh the selection of the slot's own
/// installation, at the bound revision.
const ASSOCIATE_SQL: &str = concat!(
    "UPDATE wyrd.github_installations
        SET revision = revision + 1, host = $3, installation_id = $4, repositories = $5,
            connected_at = COALESCE(connected_at, statement_timestamp()),
            updated_at = statement_timestamp()
      WHERE connection_id = $1 AND revision = $2
        AND (installation_id IS NULL OR (host = $3 AND installation_id = $4))
      RETURNING ",
    columns!()
);

/// Record GitHub's suspension or unsuspension of the associated installation;
/// an unchanged state writes nothing.
const SUSPEND_SQL: &str = concat!(
    "UPDATE wyrd.github_installations
        SET revision = revision + 1,
            suspended_at = CASE WHEN $3 THEN statement_timestamp() END,
            updated_at = statement_timestamp()
      WHERE host = $1 AND installation_id = $2 AND (suspended_at IS NULL) = $3
      RETURNING ",
    columns!()
);

/// Drop repositories GitHub removed from the associated installation; a
/// removal touching no selected repository writes nothing.
const REMOVE_REPOSITORIES_SQL: &str = concat!(
    "UPDATE wyrd.github_installations
        SET revision = revision + 1,
            repositories = COALESCE((
                SELECT jsonb_agg(entry ORDER BY ordinality)
                  FROM jsonb_array_elements(repositories) WITH ORDINALITY AS kept(entry, ordinality)
                 WHERE NOT (entry->>'repository_id' = ANY($3))
            ), '[]'::jsonb),
            updated_at = statement_timestamp()
      WHERE host = $1 AND installation_id = $2
        AND EXISTS (SELECT 1 FROM jsonb_array_elements(repositories) AS held(entry)
                     WHERE entry->>'repository_id' = ANY($3))
      RETURNING ",
    columns!()
);

/// Release the slot's association, optionally only when it is exactly this
/// (host, installation).
const RELEASE_SQL: &str = concat!(
    "UPDATE wyrd.github_installations
        SET revision = revision + 1, host = NULL, installation_id = NULL,
            repositories = '[]'::jsonb, suspended_at = NULL, connected_at = NULL,
            updated_at = statement_timestamp()
      WHERE installation_id IS NOT NULL
        AND ($1::text IS NULL OR (host = $1 AND installation_id = $2))
      RETURNING ",
    columns!()
);

/// The slot and revision a connection transaction began from.
///
/// Revision 0 names a tenant without a slot; the first association creates
/// the slot under the bound `connection_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct GithubSlotBinding {
    /// Slot identity, or the identity the first association will create.
    pub connection_id: Uuid,
    /// Slot revision, or 0 when the tenant has no slot.
    pub connection_revision: i64,
}

/// One tenant connection slot as stored. It holds no secret.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct GithubInstallationRow {
    /// Slot identity.
    pub connection_id: Uuid,
    /// Advances on every change to the association or its access.
    pub revision: i64,
    /// Configured GitHub web host; `None` when released.
    pub host: Option<String>,
    /// Verified installation id; `None` when released.
    pub installation_id: Option<i64>,
    /// Tenant-selected repositories the installation can currently reach.
    pub repositories: Json<Vec<GithubRepositoryAccess>>,
    /// When GitHub suspended the installation, if it is suspended.
    pub suspended_at: Option<DateTime<Utc>>,
    /// When the current association was verified; `None` when released.
    pub connected_at: Option<DateTime<Utc>>,
    /// Last change to the slot.
    pub updated_at: DateTime<Utc>,
}

impl GithubInstallationRow {
    /// The binding a new connection transaction begun now records.
    #[must_use]
    pub fn binding(&self) -> GithubSlotBinding {
        GithubSlotBinding {
            connection_id: self.connection_id,
            connection_revision: self.revision,
        }
    }
}

/// A verified installation and selection to associate.
#[derive(Debug, Clone, Copy)]
pub struct GithubAssociation<'a> {
    /// Configured GitHub web host.
    pub host: &'a str,
    /// Installation id verified with the App and the authorizing user.
    pub installation_id: i64,
    /// Selected repositories both the installation and the user can reach.
    pub repositories: &'a [GithubRepositoryAccess],
}

/// Result of [`associate_github_installation`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GithubAssociationOutcome {
    /// Associated or refreshed; the committed slot.
    Associated(GithubInstallationRow),
    /// The slot changed since the transaction began.
    Stale,
    /// The tenant is associated with a different installation.
    TenantOccupied,
    /// Another tenant is associated with this installation.
    InstallationClaimed,
}

/// Read the tenant's slot, if it has one.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the read.
pub async fn github_installation(
    conn: &mut TenantConn<'_>,
) -> Result<Option<GithubInstallationRow>, SqlError> {
    Ok(sqlx::query_as::<_, GithubInstallationRow>(SLOT_SQL)
        .fetch_optional(&mut **conn.transaction())
        .await?)
}

/// Read and lock the tenant's slot until the caller's transaction ends.
///
/// Disconnect holds this lock while it fences affected work and releases the
/// association, so no association or webhook change interleaves.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the read.
pub async fn lock_github_installation(
    conn: &mut TenantConn<'_>,
) -> Result<Option<GithubInstallationRow>, SqlError> {
    Ok(sqlx::query_as::<_, GithubInstallationRow>(LOCK_SLOT_SQL)
        .fetch_optional(&mut **conn.transaction())
        .await?)
}

/// Associate a verified installation at the slot and revision `begun`.
///
/// A revision-0 binding creates the slot; any other binding updates it only
/// when its identity and revision are unchanged. A tenant already associated
/// with a different installation is refused, and another tenant's claim on
/// the same installation is refused by the unique index. After
/// [`GithubAssociationOutcome::InstallationClaimed`] the transaction is
/// aborted and must be rolled back.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects a statement for any other
/// reason.
pub async fn associate_github_installation(
    conn: &mut TenantConn<'_>,
    begun: GithubSlotBinding,
    association: GithubAssociation<'_>,
) -> Result<GithubAssociationOutcome, SqlError> {
    let repositories = Json(association.repositories);
    let written = if begun.connection_revision == 0 {
        sqlx::query_as::<_, GithubInstallationRow>(INSERT_SQL)
            .bind(begun.connection_id)
            .bind(association.host)
            .bind(association.installation_id)
            .bind(repositories)
            .fetch_optional(&mut **conn.transaction())
            .await
    } else {
        let Some(slot) = lock_github_installation(conn).await? else {
            return Ok(GithubAssociationOutcome::Stale);
        };
        if slot.binding() != begun {
            return Ok(GithubAssociationOutcome::Stale);
        }
        if slot
            .installation_id
            .is_some_and(|id| id != association.installation_id)
            || slot
                .host
                .as_deref()
                .is_some_and(|host| host != association.host)
        {
            return Ok(GithubAssociationOutcome::TenantOccupied);
        }
        sqlx::query_as::<_, GithubInstallationRow>(ASSOCIATE_SQL)
            .bind(begun.connection_id)
            .bind(begun.connection_revision)
            .bind(association.host)
            .bind(association.installation_id)
            .bind(repositories)
            .fetch_optional(&mut **conn.transaction())
            .await
    };
    match written.map_err(SqlError::from) {
        Ok(Some(row)) => Ok(GithubAssociationOutcome::Associated(row)),
        Ok(None) => Ok(GithubAssociationOutcome::Stale),
        Err(SqlError::UniqueViolation { constraint }) if constraint == ONE_TENANT_INDEX => {
            Ok(GithubAssociationOutcome::InstallationClaimed)
        }
        Err(error) => Err(error),
    }
}

/// Record that GitHub suspended (`true`) or unsuspended (`false`) the
/// tenant's associated installation `(host, installation_id)`.
///
/// Suspension keeps the association, so the installation stays claimed.
/// Returns the changed slot, or `None` when the slot is not associated with
/// that installation or is already in that state.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn set_github_installation_suspended(
    conn: &mut TenantConn<'_>,
    host: &str,
    installation_id: i64,
    suspended: bool,
) -> Result<Option<GithubInstallationRow>, SqlError> {
    Ok(sqlx::query_as::<_, GithubInstallationRow>(SUSPEND_SQL)
        .bind(host)
        .bind(installation_id)
        .bind(suspended)
        .fetch_optional(&mut **conn.transaction())
        .await?)
}

/// Drop `repository_ids` from the tenant's selection for the associated
/// installation `(host, installation_id)`.
///
/// Returns the changed slot, or `None` when the slot is not associated with
/// that installation or selects none of those repositories.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn remove_github_repositories(
    conn: &mut TenantConn<'_>,
    host: &str,
    installation_id: i64,
    repository_ids: &[String],
) -> Result<Option<GithubInstallationRow>, SqlError> {
    Ok(
        sqlx::query_as::<_, GithubInstallationRow>(REMOVE_REPOSITORIES_SQL)
            .bind(host)
            .bind(installation_id)
            .bind(repository_ids)
            .fetch_optional(&mut **conn.transaction())
            .await?,
    )
}

/// Release the tenant's association, making the installation claimable by
/// another tenant once the caller commits.
///
/// With `only` set, releases only when the slot holds exactly that
/// `(host, installation_id)`, as an installation-deleted webhook requires.
/// The caller fences affected work in the same transaction first. Returns
/// the released slot, or `None` when nothing matching was associated.
///
/// # Errors
/// Returns [`SqlError`] when Postgres rejects the update.
pub async fn release_github_installation(
    conn: &mut TenantConn<'_>,
    only: Option<(&str, i64)>,
) -> Result<Option<GithubInstallationRow>, SqlError> {
    Ok(sqlx::query_as::<_, GithubInstallationRow>(RELEASE_SQL)
        .bind(only.map(|(host, _)| host))
        .bind(only.map(|(_, id)| id))
        .fetch_optional(&mut **conn.transaction())
        .await?)
}
