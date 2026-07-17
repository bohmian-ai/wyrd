//! Tenant-scoped reads + writes for the cross-table derivation registry:
//! `vala.olap_derivations`.
//!
//! A derivation reads a source table and writes a target table. Progress is
//! tracked by a durable WATERMARK: the last source commit position (`batch_id`)
//! fully consumed. This mirrors how `vala.olap_commits` tracks commit progress —
//! by opaque 16-byte `batch_id`, never by a numeric snapshot-id compare. When a
//! derivation has never advanced, `select_derivation_pin` resolves to the
//! `registered_watermark` (the earliest position captured at registration) so
//! the worker replays from the registered/earliest position rather than an
//! undefined start.
//!
//! Tenant-scoped callers pass a [`TenantConn`]; the wyrd-sql RLS bind enforces
//! tenant scope for the wyrd_app role alongside the explicit predicates below.
// raw-query grep allowlist: olap_derivations post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use sqlx::types::Uuid;
use wyrd_sql::TenantConn;

use crate::row_types::olap_derivations::{DerivationFreshnessRow, DerivationRow};
use crate::{OperatorPool, SqlError};

/// One committed source-commit position: the opaque 16-byte `batch_id` in the
/// order it landed on the source table. Ordering is by `(committed_at, batch_id)`
/// so a stable, total order exists even when two commits share a wall-clock
/// timestamp; `batch_id` is UUIDv7 (time-ordered) so the tiebreak still respects
/// commit order.
#[derive(Debug, Clone)]
pub struct CommittedSourceBatch {
    /// Opaque 16-byte source commit position.
    pub batch_id: Vec<u8>,
}

/// Enumerate distinct tenant IDs that have at least one `committed` batch for
/// `source_table_uid`.
///
/// Uses the operator pool (BYPASSRLS) so the derivation worker can discover
/// which tenants need a derivation pass without a per-tenant connection.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
// tenant-isolation: cross-tenant OperatorPool; discovery is restricted to the derivation worker.
pub async fn list_source_commit_tenants(
    op: &OperatorPool,
    source_table_uid: &[u8; 16],
) -> Result<Vec<Uuid>, SqlError> {
    sqlx::query_as::<_, (Uuid,)>(
        r#"
        SELECT DISTINCT data_tenant_id
          FROM vala.olap_commits
         WHERE table_uid = $1
           AND state = 'committed'
        "#,
    )
    .bind(source_table_uid.as_slice())
    .fetch_all(op.pool())
    .await
    .map_err(SqlError::from)
    .map(|rows| rows.into_iter().map(|(id,)| id).collect())
}

/// Per-tenant enumeration of `committed` source batch IDs in commit order.
///
/// Uses the caller's [`TenantConn`] so RLS confines the result to the current
/// tenant. The derivation worker calls this after discovering the tenant via
/// [`list_source_commit_tenants`] to build the ordered delta it must consume.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn list_tenant_committed_batches(
    conn: &mut TenantConn<'_>,
    source_table_uid: &[u8; 16],
) -> Result<Vec<CommittedSourceBatch>, SqlError> {
    sqlx::query_as::<_, (Vec<u8>,)>(
        r#"
        SELECT batch_id
          FROM vala.olap_commits
         WHERE table_uid = $1
           AND state = 'committed'
         ORDER BY committed_at, batch_id
        "#,
    )
    .bind(source_table_uid.as_slice())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
    .map(|rows| {
        rows.into_iter()
            .map(|(batch_id,)| CommittedSourceBatch { batch_id })
            .collect()
    })
}

/// Outcome of [`register_derivation_with_contract`]: distinguishes a brand-new
/// registration, an identical replay (safe to resume), and a drift rejection
/// (fingerprint mismatch; caller must skip to prevent stale watermark reuse).
#[derive(Debug, PartialEq)]
pub enum ContractOutcome {
    /// The derivation did not exist; it was inserted with the supplied fingerprints.
    Registered,
    /// The derivation already exists and all three fingerprints match the stored
    /// values. Safe to resume — no semantic drift detected.
    IdenticalReplay,
    /// The derivation already exists but at least one fingerprint differs from
    /// the stored value. `differing` names the changed columns. The worker must
    /// skip this tenant to avoid resuming a stale watermark under new semantics.
    DriftRejected {
        /// Names of the fingerprint columns that differ: a subset of
        /// `["source_schema_fingerprint", "transform_fingerprint",
        /// "target_set_fingerprint"]`.
        differing: Vec<&'static str>,
    },
}

/// The three immutable contract fingerprints for a derivation registration.
///
/// Passed to [`register_derivation_with_contract`] to guard against silent
/// watermark reuse under changed derivation semantics on redeployment.
pub struct DerivationContract<'a> {
    /// SHA-256 of the source table's declared user Arrow fields (name + data_type
    /// per field, in declaration order). Computed via `DomainTable::schema_fingerprint()`.
    pub source_schema_fingerprint: &'a [u8; 32],
    /// SHA-256 of the derivation's `CONTRACT_VERSION` string. Bump the constant
    /// whenever transform semantics change (new mapping, new output schema, etc.).
    pub transform_fingerprint: &'a [u8; 32],
    /// SHA-256 of all target table UIDs concatenated in lexicographic sort order.
    /// Detects changes to the target-table routing set.
    pub target_set_fingerprint: &'a [u8; 32],
}

type StoredFingerprints = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

/// Registration payload for [`register_derivation_with_contract`]:
/// the derivation identity plus its fingerprint contract.
pub struct DerivationRegistration<'a> {
    /// Stable derivation identifier (UUID bytes).
    pub derivation_uid: &'a [u8; 16],
    /// Source table UID.
    pub source_table_uid: &'a [u8; 16],
    /// Target table UID.
    pub target_table_uid: &'a [u8; 16],
    /// Control bind: registration anchor (SYSTEM_OWNER or data tenant).
    pub control_bind: Uuid,
    /// Fully-qualified derivation name.
    pub fqn: &'a str,
    /// Earliest source position the derivation may process.
    pub registered_watermark: Option<&'a [u8; 16]>,
    /// Fingerprint contract bound at registration.
    pub contract: DerivationContract<'a>,
}

/// Register a derivation with immutable contract fingerprints.
///
/// On first call for a `(source_table_uid, target_table_uid, control_bind)`
/// triple: inserts the row with all three fingerprints and returns
/// [`ContractOutcome::Registered`].
///
/// On subsequent calls:
/// - If all three fingerprints match the stored values, returns
///   [`ContractOutcome::IdenticalReplay`] without modifying the row.
/// - If any fingerprint differs, returns [`ContractOutcome::DriftRejected`]
///   listing which columns changed. No insert or update is performed; the
///   caller must skip derivation processing to prevent stale watermark reuse
///   under changed semantics.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn register_derivation_with_contract(
    conn: &mut TenantConn<'_>,
    reg: DerivationRegistration<'_>,
) -> Result<ContractOutcome, SqlError> {
    let DerivationRegistration {
        derivation_uid,
        source_table_uid,
        target_table_uid,
        control_bind,
        fqn,
        registered_watermark,
        contract,
    } = reg;
    let existing: Option<StoredFingerprints> = sqlx::query_as(
        r#"
            SELECT source_schema_fingerprint,
                   transform_fingerprint,
                   target_set_fingerprint
              FROM vala.olap_derivations
             WHERE derivation_uid = $1
            "#,
    )
    .bind(derivation_uid.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    if let Some((stored_source, stored_transform, stored_target)) = existing {
        let mut differing = Vec::new();
        if stored_source.as_deref() != Some(contract.source_schema_fingerprint.as_slice()) {
            differing.push("source_schema_fingerprint");
        }
        if stored_transform.as_deref() != Some(contract.transform_fingerprint.as_slice()) {
            differing.push("transform_fingerprint");
        }
        if stored_target.as_deref() != Some(contract.target_set_fingerprint.as_slice()) {
            differing.push("target_set_fingerprint");
        }
        if differing.is_empty() {
            return Ok(ContractOutcome::IdenticalReplay);
        }
        return Ok(ContractOutcome::DriftRejected { differing });
    }

    sqlx::query(
        r#"
        INSERT INTO vala.olap_derivations
            (data_tenant_id, derivation_uid, source_table_uid, target_table_uid,
             control_bind, fqn, registered_watermark,
             source_schema_fingerprint, transform_fingerprint, target_set_fingerprint)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (data_tenant_id, source_table_uid, target_table_uid, control_bind)
        DO NOTHING
        "#,
    )
    .bind(derivation_uid.as_slice())
    .bind(source_table_uid.as_slice())
    .bind(target_table_uid.as_slice())
    .bind(control_bind)
    .bind(fqn)
    .bind(registered_watermark.map(|w| w.as_slice()))
    .bind(contract.source_schema_fingerprint.as_slice())
    .bind(contract.transform_fingerprint.as_slice())
    .bind(contract.target_set_fingerprint.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(ContractOutcome::Registered)
}

/// Register a derivation idempotently.
///
/// Inserts one `(source, target, control_bind)` derivation for the current
/// tenant. `registered_watermark` is the earliest source position the
/// derivation is pinned to (`None` = from the beginning). `ON CONFLICT DO
/// NOTHING` keys on the `(data_tenant_id, source_table_uid, target_table_uid,
/// control_bind)` unique constraint so re-registration is a no-op.
///
/// # Deprecated
/// Use [`register_derivation_with_contract`] for new call sites. This function
/// does not populate the contract fingerprint columns and cannot detect
/// derivation semantic drift on redeployment.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or an RLS policy rejects the row.
pub async fn insert_derivation(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
    source_table_uid: &[u8; 16],
    target_table_uid: &[u8; 16],
    control_bind: Uuid,
    fqn: &str,
    registered_watermark: Option<&[u8; 16]>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.olap_derivations
            (data_tenant_id, derivation_uid, source_table_uid, target_table_uid,
             control_bind, fqn, registered_watermark)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6)
        ON CONFLICT (data_tenant_id, source_table_uid, target_table_uid, control_bind)
        DO NOTHING
        "#,
    )
    .bind(derivation_uid.as_slice())
    .bind(source_table_uid.as_slice())
    .bind(target_table_uid.as_slice())
    .bind(control_bind)
    .bind(fqn)
    .bind(registered_watermark.map(|w| w.as_slice()))
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Flip a derivation into the `deriving` (in-progress) health state.
///
/// Called when the worker claims a derivation and starts consuming a delta.
/// Returns `true` when a row moved into `deriving`, `false` when no matching
/// derivation exists for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn mark_deriving(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_derivations
           SET derivation_state = 'deriving',
               updated_at       = now()
         WHERE derivation_uid   = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Advance a derivation's watermark to `watermark` after a delta is fully
/// derived into the target.
///
/// Records `watermark` as the last consumed source commit position, stamps
/// `derived_at`, and returns the derivation to `idle`. `watermark` is an opaque
/// 16-byte source `batch_id`; it is stored, not numerically compared. Returns
/// `true` when a row advanced, `false` when no matching derivation exists.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn advance_watermark(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
    watermark: &[u8; 16],
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_derivations
           SET watermark        = $2,
               derivation_state = 'idle',
               derived_at       = now(),
               updated_at       = now()
         WHERE derivation_uid   = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .bind(watermark.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Read the freshness / lag of one derivation for the current tenant.
///
/// `lag_seconds` is `now() - derived_at` in seconds, `NULL` when the derivation
/// has never advanced. Used by tests and the Task F read-back polling loop.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn derivation_freshness(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<Option<DerivationFreshnessRow>, SqlError> {
    sqlx::query_as::<_, DerivationFreshnessRow>(
        r#"
        SELECT derivation_uid,
               watermark,
               derivation_state,
               derived_at,
               EXTRACT(EPOCH FROM (now() - derived_at))::float8 AS lag_seconds
          FROM vala.olap_derivations
         WHERE derivation_uid = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// List derivations that need work: registered against `source_table_uid` and
/// not in a terminal-failure/degraded state.
///
/// The worker uses this to find derivations to advance for a source table that
/// just committed. Filters out `degraded` rows (structurally broken) so only
/// runnable candidates are returned.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn select_derivation_candidates(
    conn: &mut TenantConn<'_>,
    source_table_uid: &[u8; 16],
) -> Result<Vec<DerivationRow>, SqlError> {
    sqlx::query_as::<_, DerivationRow>(
        r#"
        SELECT data_tenant_id,
               derivation_uid,
               source_table_uid,
               target_table_uid,
               control_bind,
               watermark,
               registered_watermark,
               derivation_state,
               fqn,
               registered_at,
               derived_at,
               updated_at
          FROM vala.olap_derivations
         WHERE source_table_uid = $1
           AND derivation_state <> 'degraded'
        "#,
    )
    .bind(source_table_uid.as_slice())
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Flip a derivation into the `failed` health state.
///
/// Called when a target write fails and the derivation tick cannot complete.
/// Sets `derivation_state = 'failed'` and stamps `updated_at`. The watermark
/// is NOT advanced — the next tick will retry from the same position. Returns
/// `true` when a row was updated, `false` when the derivation row does not
/// exist for the current tenant.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn mark_failed(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<bool, SqlError> {
    let result = sqlx::query(
        r#"
        UPDATE vala.olap_derivations
           SET derivation_state = 'failed',
               updated_at       = now()
         WHERE derivation_uid   = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(result.rows_affected() > 0)
}

/// Return the pinned watermark for a derivation: the position from which the
/// worker should read next.
///
/// Resolves to `COALESCE(watermark, registered_watermark)` — a NULL watermark
/// (nothing consumed yet) pins to the registered/earliest position rather than
/// an undefined start. Returns `Ok(None)` when the derivation does not exist,
/// and `Ok(Some(None))` when both the watermark and the registered pin are NULL
/// (pin from the absolute beginning of the source).
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn select_derivation_pin(
    conn: &mut TenantConn<'_>,
    derivation_uid: &[u8; 16],
) -> Result<Option<Option<Vec<u8>>>, SqlError> {
    let pin: Option<(Option<Vec<u8>>,)> = sqlx::query_as(
        r#"
        SELECT COALESCE(watermark, registered_watermark) AS pin
          FROM vala.olap_derivations
         WHERE derivation_uid = $1
        "#,
    )
    .bind(derivation_uid.as_slice())
    .fetch_optional(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;

    Ok(pin.map(|(p,)| p))
}
