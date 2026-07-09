//! Register-path version resolution and hash dedup.
#![deny(missing_docs)]

use sqlx::{Postgres, QueryBuilder};
use wyrd_semver::{VersionBlock, VersionBump, VersionSpec, seed_version};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, SpaceName};

use crate::queries::cards::version_sql::push_bounds;
use crate::tenant_conn::TenantConn;

/// Outcome of resolving the authored version against existing rows.
pub(crate) enum Resolution {
    /// Insert at this concrete version.
    Fresh(VersionBlock),
    /// Content matches the latest-in-line row; return it, do not insert.
    Deduplicated {
        /// Existing version to return.
        version: VersionBlock,
    },
}

struct LatestInLine {
    version: VersionBlock,
    spec_hash: String,
}

async fn latest_stable_in_line(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    bounds: Option<&wyrd_semver::VersionBounds>,
) -> Result<Option<LatestInLine>, WyrdError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT version, spec_hash FROM wyrd.cards \
         WHERE data_tenant_id = wyrd.current_tenant() \
           AND status <> 'deleted' AND NOT version_is_prerelease AND kind = ",
    );
    qb.push_bind(kind.wire_name());
    qb.push(" AND space = ").push_bind(space.as_str());
    qb.push(" AND name = ").push_bind(name.as_str());
    if let Some(bounds) = bounds {
        push_bounds(&mut qb, bounds)?;
    }
    qb.push(" ORDER BY version_major DESC, version_minor DESC, version_patch DESC LIMIT 1");

    let row = qb
        .build_query_as::<(String, String)>()
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(|e| WyrdError::registry_unavailable(e.to_string()))?;

    row.map(|(version, spec_hash)| {
        VersionBlock::parse(version)
            .map(|version| LatestInLine { version, spec_hash })
            .map_err(|e| WyrdError::registry_invalid_version_block(e.to_string()))
    })
    .transpose()
}

/// Resolve the authored version + bump against existing rows.
///
/// Hash dedup compares only against the latest stable row in the authored line.
///
/// # Errors
/// Returns `WYRD_REG_400_INVALID_VERSION_BLOCK` for invalid bounds, overflow,
/// stored-version parse failures, or a scoped bump that leaves the authored
/// range. Returns `WYRD_REG_503_REGISTRY_UNAVAILABLE` on database errors.
pub(crate) async fn resolve_version(
    conn: &mut TenantConn<'_>,
    kind: CardKind,
    space: &SpaceName,
    name: &CardName,
    version: Option<&VersionSpec>,
    bump: Option<&VersionBump>,
    submitted_hash: &str,
) -> Result<Resolution, WyrdError> {
    let bump = bump.cloned().unwrap_or(VersionBump::Patch);
    match version {
        Some(VersionSpec::Pin(_)) => {
            unreachable!("resolve_version is only called on the auto/scope path")
        }
        None => match latest_stable_in_line(conn, kind, space, name, None).await? {
            Some(latest) if latest.spec_hash == submitted_hash => Ok(Resolution::Deduplicated {
                version: latest.version,
            }),
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
            match latest_stable_in_line(conn, kind, space, name, Some(&bounds)).await? {
                Some(latest) if latest.spec_hash == submitted_hash => {
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
