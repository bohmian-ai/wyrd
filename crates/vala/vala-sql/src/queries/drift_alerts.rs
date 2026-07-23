//! Query functions for `vala.drift_alerts`.
//!
//! Tenant-scoped functions take `&mut TenantConn<'_>`.
// raw-query grep allowlist: drift_alerts post-dates the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use wyrd_spec::reference::CardRef;
use wyrd_sql::TenantConn;

use crate::SqlError;
use crate::row_types::alerts::DriftAlertRow;

/// Insert parameters for a single drift alert.
pub struct DriftAlertInsert<'a> {
    /// Card reference identifying the alerting card.
    pub drift_ref: &'a CardRef,
    /// Discriminator: `spc`, `psi`, `custom`, or `eval`.
    pub drift_type: &'a str,
    /// Feature or metric that alerted; `None` = aggregate alert.
    pub series: Option<&'a str>,
    /// Alert payload from the scoring layer.
    pub alert: &'a serde_json::Value,
}

/// Upsert a drift alert for the current tenant.
///
/// Uses `ON CONFLICT DO UPDATE` so a re-triggering alert refreshes its payload
/// and stays active.
///
/// # Errors
/// Returns [`SqlError`] when the query fails or RLS rejects the write.
pub async fn upsert_drift_alert(
    conn: &mut TenantConn<'_>,
    ins: &DriftAlertInsert<'_>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        INSERT INTO vala.drift_alerts
            (data_tenant_id, drift_ref_kind, drift_ref_name, drift_ref_ver,
             drift_ref_space, drift_type, series, alert, active)
        VALUES (wyrd.current_tenant(), $1, $2, $3, $4, $5, $6, $7, TRUE)
        ON CONFLICT (data_tenant_id, drift_ref_kind, drift_ref_name, drift_ref_ver,
                     drift_ref_space, drift_type, series)
        DO UPDATE SET
            alert      = EXCLUDED.alert,
            active     = TRUE,
            updated_at = NOW()
        "#,
    )
    .bind(ins.drift_ref.kind.wire_name())
    .bind(ins.drift_ref.name.as_str())
    .bind(ins.drift_ref.version.to_string())
    .bind(resolved_space(ins.drift_ref)?)
    .bind(ins.drift_type)
    .bind(ins.series.unwrap_or(""))
    .bind(ins.alert)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

/// Resolve (deactivate) all active alerts for the given card reference.
///
/// Returns the rows that were resolved.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn resolve_drift_alerts(
    conn: &mut TenantConn<'_>,
    drift_ref: &CardRef,
) -> Result<Vec<DriftAlertRow>, SqlError> {
    sqlx::query_as::<_, DriftAlertRow>(
        r#"
        UPDATE vala.drift_alerts
           SET active = FALSE, updated_at = NOW()
         WHERE data_tenant_id  = wyrd.current_tenant()
           AND drift_ref_kind  = $1
           AND drift_ref_name  = $2
           AND drift_ref_ver   = $3
           AND drift_ref_space = $4
           AND active = TRUE
        RETURNING *
        "#,
    )
    .bind(drift_ref.kind.wire_name())
    .bind(drift_ref.name.as_str())
    .bind(drift_ref.version.to_string())
    .bind(resolved_space(drift_ref)?)
    .fetch_all(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)
}

/// Acknowledge (deactivate) a specific alert by drift_ref, type, and series.
///
/// # Errors
/// Returns [`SqlError`] when the query fails.
pub async fn acknowledge_drift_alert(
    conn: &mut TenantConn<'_>,
    drift_ref: &CardRef,
    drift_type: &str,
    series: Option<&str>,
) -> Result<(), SqlError> {
    sqlx::query(
        r#"
        UPDATE vala.drift_alerts
           SET active = FALSE, updated_at = NOW()
         WHERE data_tenant_id  = wyrd.current_tenant()
           AND drift_ref_kind  = $1
           AND drift_ref_name  = $2
           AND drift_ref_ver   = $3
           AND drift_ref_space = $4
           AND drift_type      = $5
           AND COALESCE(series, '') = COALESCE($6, '')
        "#,
    )
    .bind(drift_ref.kind.wire_name())
    .bind(drift_ref.name.as_str())
    .bind(drift_ref.version.to_string())
    .bind(resolved_space(drift_ref)?)
    .bind(drift_type)
    .bind(series)
    .execute(&mut **conn.transaction())
    .await
    .map_err(SqlError::from)?;
    Ok(())
}

fn resolved_space(card_ref: &CardRef) -> Result<&str, SqlError> {
    card_ref
        .space
        .as_ref()
        .map(|space| space.as_str())
        .ok_or_else(|| SqlError::InvariantViolation {
            detail: format!("drift alert reference {} has no resolved space", card_ref),
        })
}

#[cfg(test)]
mod tests {
    use super::resolved_space;
    use wyrd_semver::VersionBlock;
    use wyrd_spec::envelope::CardKind;
    use wyrd_spec::ids::{CardName, SpaceName};
    use wyrd_spec::reference::CardRef;

    fn card_ref(space: Option<SpaceName>) -> CardRef {
        CardRef {
            kind: CardKind::Drift,
            name: CardName::new("drift").expect("test card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("test version is valid"),
            space,
            uid: None,
        }
    }

    #[test]
    fn resolved_space_returns_persisted_space() {
        let reference = card_ref(Some(SpaceName::new("default").expect("space is valid")));
        assert_eq!(
            resolved_space(&reference).expect("space resolves"),
            "default"
        );
    }

    #[test]
    fn resolved_space_rejects_unresolved_reference() {
        let error = resolved_space(&card_ref(None)).expect_err("space must be resolved");
        assert!(matches!(error, crate::SqlError::InvariantViolation { .. }));
    }
}
