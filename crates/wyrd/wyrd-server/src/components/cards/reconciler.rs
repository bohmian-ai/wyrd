//! Bounded Card lifecycle reconciliation worker.

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
const LEASE_SAFETY: std::time::Duration = std::time::Duration::from_secs(5);

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
        // Sampled before the claim statement so the projected deadline
        // conservatively absorbs query and return latency.
        let claim_started = tokio::time::Instant::now();
        let mut claims = claim_card_reconciliation(operator, LEASE_SECONDS, 1).await?;
        let Some(claim) = claims.pop() else {
            break;
        };
        process_claim(state, claim, claim_started).await;
    }
    Ok(())
}

/// Reconcile one claim, giving up the lease when too little of it remains.
///
/// The lease budget is projected from the database-reported remainder onto the
/// local monotonic clock sampled before the claim; the database deadline is
/// never compared with the host wall clock.
async fn process_claim(
    state: &AppState,
    claim: CardReconcileClaim,
    claim_started: tokio::time::Instant,
) {
    counter!("wyrd_card_reconciliation_attempts_total", "kind" => claim.reconcile_kind.clone())
        .increment(1);
    let Ok(tenant_id) = DataTenantId::try_from(claim.data_tenant_id) else {
        tracing::error!(kind = %claim.reconcile_kind, "card reconciliation claim has invalid tenant id");
        return;
    };
    let caller = service::reconciliation_caller(tenant_id);
    let lease_deadline =
        claim_started + std::time::Duration::from_secs_f64(claim.lease_remaining_seconds.max(0.0));
    if lease_deadline <= tokio::time::Instant::now() + LEASE_SAFETY {
        if let Err(error) =
            service::reschedule_reconciliation_claim(state, &caller, &claim, 1).await
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
            let retry_delay_seconds = retry_delay_seconds(claim.reconcile_attempts);
            match service::record_reconciliation_failure(
                state,
                &caller,
                &claim,
                &error,
                retry_delay_seconds,
            )
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

/// Fixed retry backoff, in seconds, for the given completed attempt number.
///
/// `PostgreSQL` turns this delay into the next-attempt deadline, so the schedule
/// is independent of the reconciler host's wall clock.
fn retry_delay_seconds(attempt: i32) -> i64 {
    match attempt {
        2 => 4,
        _ if attempt >= MAX_RECONCILE_ATTEMPTS => 16,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::retry_delay_seconds;

    /// The retry backoff stays `1s`, `4s`, `16s` after moving to SQL-derived
    /// deadlines.
    #[test]
    fn retry_schedule_is_fixed_and_bounded() {
        assert_eq!(retry_delay_seconds(1), 1);
        assert_eq!(retry_delay_seconds(2), 4);
        assert_eq!(retry_delay_seconds(3), 16);
    }
}
