//! Register-path version resolution and hash dedup.
#![deny(missing_docs)]

use sqlx::{Postgres, QueryBuilder};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec, seed_version};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};
use wyrd_spec::registry::RegistrationOperationId;

use crate::queries::cards::version_sql::push_bounds;
use crate::tenant_conn::TenantConn;

/// Outcome of resolving the authored version against existing rows.
pub enum Resolution {
    /// Insert at this concrete version.
    Fresh(VersionBlock),
    /// Content matches the latest-in-line row; return it, do not insert.
    Deduplicated {
        /// Existing version to return.
        version: VersionBlock,
    },
}

/// Canonical identities used for artifact-aware deduplication.
///
/// `operation_id` names the registration transaction submitting the Card. Its
/// own Pending rows join the Active rows eligible for dedup and next-version
/// allocation, so repeated selection on one line inside one composite sees the
/// versions it already chose; other operations' Pending rows stay excluded.
pub struct SubmittedCardIdentity<'a> {
    /// Registration operation whose Pending rows are eligible.
    pub operation_id: RegistrationOperationId,
    /// Canonical spec hash.
    pub spec_hash: &'a str,
    /// Canonical artifact-manifest hash, when the submission is heavy.
    pub artifact_hash: Option<&'a str>,
}

struct LatestInLine {
    version: VersionBlock,
    spec_hash: String,
    artifact_hash: Option<String>,
}

// No statement_timeout on version-resolve reads: they hit idx_cards_version_latest
// (partial, bounded index) and carry no user-supplied regex. Add one if these
// queries are ever exposed to unconstrained inputs.
/// Read the highest stable version on one Card line, with its content hashes.
///
/// The line is `kind`/`space`/`name` in the tenant bound to `conn`. Eligible
/// rows are Active ones plus the Pending rows `operation_id` wrote earlier in
/// this same registration transaction, so a composite selecting the line twice
/// sees its own earlier choice; other operations' Pending rows, prereleases,
/// and deleted rows are excluded. `bounds`, when present, restricts the read to
/// the authored half-open version range. The read runs on the caller-owned
/// transaction under the caller's version-line lock, writes nothing, and never
/// commits; cancelling it leaves that transaction for the caller to finish or
/// roll back.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` when a bound cannot be
/// bound as a PostgreSQL `BIGINT` or the stored version does not parse, and
/// `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` when the query fails.
async fn latest_stable_in_line(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    operation_id: RegistrationOperationId,
    bounds: Option<&wyrd_semver::VersionBounds>,
) -> Result<Option<LatestInLine>, WyrdError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT version, spec_hash, artifact_hash FROM wyrd.cards \
         WHERE (status = 'active' OR (status = 'pending' AND registration_operation_id = ",
    );
    qb.push_bind(operation_id.as_uuid());
    qb.push(")) AND NOT version_is_prerelease AND kind = ");
    qb.push_bind(kind.wire_name());
    qb.push(" AND space = ").push_bind(space.as_str());
    qb.push(" AND name = ").push_bind(name.as_str());
    if let Some(bounds) = bounds {
        push_bounds(&mut qb, bounds)?;
    }
    qb.push(" ORDER BY version_major DESC, version_minor DESC, version_patch DESC LIMIT 1");

    let row = qb
        .build_query_as::<(String, String, Option<String>)>()
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "card registry version query failed");
            WyrdError::registry_unavailable("card registry unavailable")
        })?;

    row.map(|(version, spec_hash, artifact_hash)| {
        VersionBlock::parse(version)
            .map(|version| LatestInLine {
                version,
                spec_hash,
                artifact_hash,
            })
            .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))
    })
    .transpose()
}

/// Resolve the authored version + bump against existing rows.
///
/// Hash dedup compares only against the latest stable row in the authored line,
/// counting the submitting operation's own Pending rows as eligible.
///
/// # Errors
/// Returns `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` for invalid bounds, overflow,
/// stored-version parse failures, or a scoped bump that leaves the authored
/// range. Returns `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` on database errors.
///
/// # Panics
/// Panics when `version` is an exact `VersionSpec::Pin`: this resolver owns
/// only auto and scoped versions, and callers such as registration's
/// `resolve_existing` look exact pins up directly instead.
pub async fn resolve_version(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: Option<&VersionSpec>,
    bump: Option<&VersionBump>,
    submitted: SubmittedCardIdentity<'_>,
) -> Result<Resolution, WyrdError> {
    let bump = bump.cloned().unwrap_or(VersionBump::Patch);
    match version {
        Some(VersionSpec::Pin(_)) => {
            unreachable!("resolve_version is only called on the auto/scope path")
        }
        None => match latest_stable_in_line(conn, kind, space, name, submitted.operation_id, None)
            .await?
        {
            Some(latest)
                if latest.spec_hash == submitted.spec_hash
                    && latest.artifact_hash.as_deref() == submitted.artifact_hash =>
            {
                Ok(Resolution::Deduplicated {
                    version: latest.version,
                })
            }
            Some(latest) => {
                let next = latest
                    .version
                    .bump(bump)
                    .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;
                Ok(Resolution::Fresh(next))
            }
            None => Ok(Resolution::Fresh(seed_version())),
        },
        Some(VersionSpec::Scope(range)) => {
            let bounds = range
                .to_bounds()
                .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;
            match latest_stable_in_line(
                conn,
                kind,
                space,
                name,
                submitted.operation_id,
                Some(&bounds),
            )
            .await?
            {
                Some(latest)
                    if latest.spec_hash == submitted.spec_hash
                        && latest.artifact_hash.as_deref() == submitted.artifact_hash =>
                {
                    Ok(Resolution::Deduplicated {
                        version: latest.version,
                    })
                }
                Some(latest) => {
                    let next = latest
                        .version
                        .bump(bump)
                        .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))?;
                    if !range.matches(&next) {
                        return Err(WyrdError::registry_invalid_version_block(format!(
                            "bump exits authored version scope: {next} not in {range}"
                        )));
                    }
                    Ok(Resolution::Fresh(next))
                }
                None => Ok(Resolution::Fresh(VersionBlock::from_triple(bounds.lower()))),
            }
        }
    }
}
