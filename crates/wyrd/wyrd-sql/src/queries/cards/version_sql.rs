//! Shared SQL fragments for version-component pushdown and line locking.
#![deny(missing_docs)]
// raw-query grep allowlist: version-line SQL uses QueryBuilder for dynamic bounds
// and a `pg_advisory_xact_lock` keyed on the tenant; all run on a TenantConn (RLS).
// Run `mise run sqlx:prepare` to promote to macros.

use sqlx::{Postgres, QueryBuilder};
use wyrd_semver::{SemverTriple, VersionBounds};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};

use crate::tenant_conn::TenantConn;

/// Dedicated advisory-lock class for card version resolution.
const ADVISORY_CLASS_CARD_VERSION: i32 = 0x0C_A2_D0_01;

/// Append the half-open bounds predicate to `qb`, binding all integers.
///
/// # Errors
/// Returns `WYRD_REG_400_INVALID_VERSION_BLOCK` when a version component cannot
/// be represented as `i64` for PostgreSQL `BIGINT` comparison.
pub(crate) fn push_bounds(
    qb: &mut QueryBuilder<Postgres>,
    bounds: &VersionBounds,
) -> Result<(), WyrdError> {
    let lower = bounds.lower();
    let (lo_major, lo_minor, lo_patch) = triple_i64(lower)?;
    qb.push(" AND (version_major, version_minor, version_patch) >= (");
    qb.push_bind(lo_major).push(", ");
    qb.push_bind(lo_minor).push(", ");
    qb.push_bind(lo_patch).push(")");

    if let Some(upper) = bounds.upper() {
        let (hi_major, hi_minor, hi_patch) = triple_i64(upper)?;
        if bounds.upper_inclusive() {
            qb.push(" AND (version_major, version_minor, version_patch) <= (");
        } else {
            qb.push(" AND (version_major, version_minor, version_patch) < (");
        }
        qb.push_bind(hi_major).push(", ");
        qb.push_bind(hi_minor).push(", ");
        qb.push_bind(hi_patch).push(")");
    }
    Ok(())
}

fn triple_i64(t: SemverTriple) -> Result<(i64, i64, i64), WyrdError> {
    let conv = |value: u64| {
        i64::try_from(value)
            .map_err(|_| WyrdError::registry_invalid_version_block("version component exceeds i64"))
    };
    Ok((conv(t.major)?, conv(t.minor)?, conv(t.patch)?))
}

/// Take a transaction-scoped advisory lock for one `(tenant, kind, space, name)`.
///
/// Auto-releases on commit or rollback. Distinct card lines proceed in
/// parallel; rare `hashtext` collisions only cause false contention.
///
/// # Errors
/// Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on database errors.
pub async fn lock_version_line(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
) -> Result<(), WyrdError> {
    let objid_key = format!(
        "{}/{}/{}/{}",
        conn.data_tenant_id().as_uuid(),
        kind.wire_name(),
        space.as_str(),
        name.as_str(),
    );
    sqlx::query("SELECT pg_advisory_xact_lock($1, hashtext($2))")
        .bind(ADVISORY_CLASS_CARD_VERSION)
        .bind(objid_key)
        .execute(&mut **conn.transaction())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "card registry version lock failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::{Postgres, QueryBuilder};
    use wyrd_semver::{SemverTriple, VersionBounds};

    use super::push_bounds;

    #[test]
    fn push_bounds_rejects_bigint_overflow() {
        let bounds = VersionBounds::new(
            SemverTriple::ZERO,
            Some(SemverTriple::new(i64::MAX as u64 + 1, 0, 0)),
            false,
        );
        let mut qb = QueryBuilder::<Postgres>::new("SELECT 1 WHERE true");

        let err = push_bounds(&mut qb, &bounds).expect_err("overflow rejected");

        assert_eq!(err.code(), "WYRD_REG_400_INVALID_VERSION_BLOCK");
    }
}
