//! Bounded Card lifecycle reconciliation worker.

use chrono::{Duration as ChronoDuration, Utc};
use metrics::counter;
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_sql::OperatorPool;
use wyrd_sql::queries::cards::{
    CardReconcileClaim, MAX_RECONCILE_ATTEMPTS, RECONCILE_KIND_CLEANUP, claim_card_reconciliation,
};

use crate::components::cards::service;
use crate::state::AppState;

const CLAIM_LIMIT: i64 = 32;
const LEASE_SECONDS: i64 = 30;
const LEASE_SAFETY_SECONDS: i64 = 5;

/// Run the supervised Card reconciler until shutdown is requested.
pub(crate) async fn run(state: AppState, operator: OperatorPool, shutdown: CancellationToken) {
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        if let Err(error) = run_once(&state, &operator).await {
            tracing::error!(code = error.code(), "card reconciliation tick failed");
        }
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
        }
    }
}

/// Run one claim-and-reconcile pass.
pub(crate) async fn run_once(state: &AppState, operator: &OperatorPool) -> Result<(), WyrdError> {
    for _ in 0..CLAIM_LIMIT {
        let now = Utc::now();
        let mut claims = claim_card_reconciliation(
            operator,
            now,
            now + ChronoDuration::seconds(LEASE_SECONDS),
            1,
        )
        .await?;
        let Some(claim) = claims.pop() else {
            break;
        };
        process_claim(state, claim).await;
    }
    Ok(())
}

async fn process_claim(state: &AppState, claim: CardReconcileClaim) {
    counter!("wyrd_card_reconciliation_attempts_total", "kind" => claim.reconcile_kind.clone())
        .increment(1);
    let Ok(tenant_id) = DataTenantId::try_from(claim.data_tenant_id) else {
        tracing::error!(kind = %claim.reconcile_kind, "card reconciliation claim has invalid tenant id");
        return;
    };
    let caller = service::reconciliation_caller(tenant_id);
    if claim.reconcile_lease_expires_at
        <= Utc::now() + ChronoDuration::seconds(LEASE_SAFETY_SECONDS)
    {
        if let Err(error) = service::reschedule_reconciliation_claim(
            state,
            &caller,
            &claim,
            Utc::now() + ChronoDuration::seconds(1),
        )
        .await
        {
            tracing::error!(
                code = error.code(),
                "card reconciliation lease reschedule failed"
            );
        }
        return;
    }
    let result = if claim.reconcile_kind == RECONCILE_KIND_CLEANUP {
        service::reconcile_cleanup_claim(state, &caller, &claim).await
    } else {
        service::reconcile_card_claim(state, &caller, &claim).await
    };
    match result {
        Ok(()) => {
            counter!("wyrd_card_reconciliation_recoveries_total", "kind" => claim.reconcile_kind)
                .increment(1);
        }
        Err(error) => {
            let retry_at = Utc::now() + retry_delay(claim.reconcile_attempts);
            match service::record_reconciliation_failure(state, &caller, &claim, &error, retry_at)
                .await
            {
                Ok(true) => {
                    counter!("wyrd_card_reconciliation_dead_letters_total", "kind" => claim.reconcile_kind.clone())
                        .increment(1);
                    tracing::error!(
                        kind = %claim.reconcile_kind,
                        attempts = claim.reconcile_attempts,
                        code = error.code(),
                        "card reconciliation dead-lettered"
                    );
                }
                Ok(false) => {
                    tracing::warn!(
                        kind = %claim.reconcile_kind,
                        attempt = claim.reconcile_attempts,
                        code = error.code(),
                        "card reconciliation attempt failed; retry scheduled"
                    );
                }
                Err(record_error) => {
                    tracing::error!(
                        kind = %claim.reconcile_kind,
                        attempt = claim.reconcile_attempts,
                        code = record_error.code(),
                        "card reconciliation failure state could not be persisted"
                    );
                }
            }
        }
    }
}

fn retry_delay(attempt: i32) -> ChronoDuration {
    match attempt {
        1 => ChronoDuration::seconds(1),
        2 => ChronoDuration::seconds(4),
        _ if attempt >= MAX_RECONCILE_ATTEMPTS => ChronoDuration::seconds(16),
        _ => ChronoDuration::seconds(1),
    }
}

#[cfg(test)]
mod tests {
    use super::retry_delay;

    #[test]
    fn retry_schedule_is_fixed_and_bounded() {
        assert_eq!(retry_delay(1).num_seconds(), 1);
        assert_eq!(retry_delay(2).num_seconds(), 4);
        assert_eq!(retry_delay(3).num_seconds(), 16);
    }
}
