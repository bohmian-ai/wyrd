//! Deployment-wide Oracle admission recovery owned by the operator pool.
//!
//! Dynamic query is intentional: startup recovery aggregates runtime lease
//! classes across tenants and is compiled without a live SQLx schema cache.

use chrono::{DateTime, Utc};
use wyrd_spec::DataTenantId;

use super::oracle_admission::OracleAdmissionLeases;
use crate::{OperatorPool, SqlError};

impl OracleAdmissionLeases {
    /// Rebuilds deployment-wide counters from every unexpired lease under the
    /// operator pool, atomically removing stale leases first.
    ///
    /// # Errors
    /// Returns [`SqlError`] when any recovery statement fails; the transaction
    /// is rolled back and callers must keep readiness false.
    pub async fn recover_shared_scopes(
        &self,
        operator: &OperatorPool,
        now: DateTime<Utc>,
    ) -> Result<(), SqlError> {
        let shared_owner = uuid::Uuid::from(DataTenantId::SYSTEM_OWNER);
        let mut transaction = operator.begin().await.map_err(SqlError::from)?;
        let result = async {
            sqlx::query(
            "INSERT INTO vala.oracle_admission_accounting \
             (data_tenant_id,scope_kind,scope_key,accounting_class,used_slots) \
             VALUES ($1,'cluster','global','all',0), \
                    ($1,'class','global','interactive',0), \
                    ($1,'class','global','analytical',0) \
             ON CONFLICT DO NOTHING",
            )
            .bind(shared_owner)
            .execute(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let locked = sqlx::query(
                "SELECT scope_kind, accounting_class \
                 FROM vala.oracle_admission_accounting \
                 WHERE data_tenant_id=$1 AND scope_key='global' \
                 ORDER BY CASE WHEN scope_kind='cluster' THEN 0 ELSE 1 END, accounting_class \
                 FOR UPDATE",
            )
            .bind(shared_owner)
            .fetch_all(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            if locked.len() != 3 {
                return Err(SqlError::InvariantViolation {
                    detail: "Oracle shared accounting rows are incomplete".to_owned(),
                });
            }
            sqlx::query(
                "DELETE FROM vala.oracle_admission_leases WHERE expires_at <= $1",
            )
            .bind(now)
            .execute(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let active = sqlx::query_as::<_, (String, i64)>(
                "SELECT query_class::text, COALESCE(sum(slot_units), 0)::bigint \
                 FROM vala.oracle_admission_leases \
                 WHERE expires_at > $1 GROUP BY query_class",
            )
            .bind(now)
            .fetch_all(&mut *transaction)
            .await
            .map_err(SqlError::from)?;
            let interactive = active
                .iter()
                .find(|(class, _)| class == "interactive")
                .map_or(0, |(_, slots)| *slots);
            let analytical = active
                .iter()
                .find(|(class, _)| class == "analytical")
                .map_or(0, |(_, slots)| *slots);
            let total = interactive + analytical;
            for (scope_kind, accounting_class, used_slots) in [
                ("cluster", "all", total),
                ("class", "interactive", interactive),
                ("class", "analytical", analytical),
            ] {
                sqlx::query(
                    "UPDATE vala.oracle_admission_accounting SET used_slots=$1,updated_at=now() \
                     WHERE data_tenant_id=$2 AND scope_kind=$3 AND scope_key='global' AND accounting_class=$4",
                )
                .bind(used_slots)
                .bind(shared_owner)
                .bind(scope_kind)
                .bind(accounting_class)
                .execute(&mut *transaction)
                .await
                .map_err(SqlError::from)?;
            }
            Ok::<(), SqlError>(())
        }
        .await;
        match result {
            Ok(()) => transaction.commit().await.map_err(SqlError::from),
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(error)
            }
        }
    }
}
