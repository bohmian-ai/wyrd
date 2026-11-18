//! The gateway calls and verifications the ui-sql journey cannot export
//! through OTLP: the server writes their tables only from its own work, so
//! [`Workload::prepare`](wyrd_testing::experiment::Workload::prepare) drives
//! real priced calls and real direct and submitted verifications, and every
//! statement reading them must count exactly what was driven.

use std::path::Path;
use std::time::Duration;

use chrono::{DurationRound as _, TimeDelta, Utc};
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use serde_json::json;
use wyrd_client::{Gateway, WyrdClient};
use wyrd_spec::vala::api::QueryParam;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_testing::gateway_capture::{GatewayCapture, PacedUpstream, Pacing};
use wyrd_testing::release_server::SetupTenant;

use super::answer::Answer;
use super::fixture::Journey;
use crate::gateway::workload::{Caller, CapturePolicy, Shape, Tenant as GatewayTenant, call, http};
use crate::verification::workload::{
    ASSERT_VERIFIER, Tenant as VerificationTenant, context, failing,
};

/// Calls or verifications in flight at once while seeding.
const IN_FLIGHT: usize = 8;

/// Gateway calls driven, each answered with [`TOKENS`] tokens.
pub const CALLS: u32 = 100;

/// Tokens each call's usage records: 11 input and 4 output.
pub const TOKENS: u32 = 15;

/// USD each call costs at 2 per million input and 8 per million output
/// tokens.
pub const COST: f64 = 0.000_054;

/// Direct verifications driven; each records a result at once.
pub const DIRECT: u32 = 60;

/// Submitted verifications driven; each completes in the background.
pub const SUBMITTED: u32 = 40;

/// Drives [`CALLS`] priced gateway calls and [`DIRECT`] direct and
/// [`SUBMITTED`] submitted assertion verifications, every odd one
/// failing, in `setup`'s tenant through `admin` and the public clients,
/// both at once, keeping provisioning files under `logs`. Returns the
/// one-hour window, from the minute seeding began, that the statements
/// over these rows bind instead of the journey's hour, since the server
/// stamps them when it does the work.
///
/// # Errors
///
/// Returns a provisioning, policy or client failure, a call not answered,
/// or a direct verdict other than the expected one.
pub async fn seed(
    setup: &SetupTenant,
    admin: &WyrdClient,
    logs: &Path,
) -> crate::gateway::Result<[QueryParam; 2]> {
    let opened = Utc::now().duration_trunc(TimeDelta::minutes(1))?;
    let window = [opened, opened + TimeDelta::hours(1)]
        .map(|at| QueryParam::String(at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()));
    let instant = Pacing {
        buffered_delay: Duration::ZERO,
        buffered_bytes: 0,
        stream_delay: Duration::ZERO,
        chunks: 0,
        chunk_interval: Duration::ZERO,
    };
    let upstream = PacedUpstream::answering(instant, 0, &GatewayCapture::completion()).await?;
    let uri = upstream.uri();
    let gateway = GatewayTenant::provision(setup, [uri.as_str(); 3], 1).await?;
    gateway.capture(CapturePolicy::Metadata).await?;
    Gateway::connect_with(admin)
        .await?
        .put_governance_policy(&serde_json::from_value(json!({
            "limits": [], "budgets": [], "unknown_cost": "allow_unpriced",
            "pricing": [{
                "model": {"deployment": "bench-chat", "model": "gpt-4o"},
                "version": "ui-sql", "currency": "USD",
                "effective_at": "2020-01-01T00:00:00Z", "active": true,
                "rates": [
                    {"dimension": "input_tokens", "unit": "1m_tokens", "price": "2"},
                    {"dimension": "output_tokens", "unit": "1m_tokens", "price": "8"},
                ],
            }],
        }))?)
        .await?;
    let verification = Box::pin(VerificationTenant::provision(
        setup,
        &logs.join("tenants"),
        1,
        None,
        false,
    ))
    .await?;
    let caller = Caller::new(http()?, &gateway.keys[0])?;
    let shape = Shape {
        model: "bench-chat/gpt-4o".to_owned(),
        stream: false,
        request_bytes: 64,
        expected: 2,
    };
    let calls = stream::iter(0..CALLS)
        .map(|n| {
            let (caller, shape) = (&caller, &shape);
            async move {
                let timed = call(caller, shape, shape.body(&format!("ui-sql-{n}"))).await;
                if timed.outcome.answered() {
                    Ok(())
                } else {
                    Err(format!("gateway call {n} ended {:?}", timed.outcome))
                }
            }
        })
        .buffer_unordered(IN_FLIGHT)
        .try_collect::<()>();
    let state = &verification.states[0];
    let verifications = stream::iter(0..DIRECT + SUBMITTED)
        .map(|sequence| async move {
            let sequence = u64::from(sequence);
            let run = state
                .run_for_card("model")
                .map_err(|error| error.to_string())?;
            let observe = run.observe();
            if sequence >= u64::from(DIRECT) {
                return observe
                    .submit(ASSERT_VERIFIER, context(sequence))
                    .await
                    .map(drop)
                    .map_err(|error| error.to_string());
            }
            let verdict = observe
                .verify(ASSERT_VERIFIER, context(sequence))
                .await
                .map_err(|error| error.to_string())?
                .outcome
                .verdict;
            let expected = if failing(sequence) {
                VerificationVerdict::Failed
            } else {
                VerificationVerdict::Passed
            };
            if verdict == expected {
                Ok(())
            } else {
                Err(format!("verification {sequence} answered {verdict:?}"))
            }
        })
        .buffer_unordered(IN_FLIGHT)
        .try_collect::<()>();
    tokio::try_join!(calls, verifications)?;
    Ok(window)
}

/// Whether statement `id` reads seeded rows, so binds the seeded window.
#[must_use]
pub fn seeded(id: &str) -> bool {
    id.starts_with("verify-")
        || ["stat-tokens-", "stat-calls-", "stat-cost-"]
            .iter()
            .any(|prefix| id.starts_with(prefix))
}

/// One property a statement's reference answer must have.
#[derive(Debug, Clone, PartialEq)]
pub enum Known {
    /// Exactly this many rows.
    Rows(usize),
    /// A numeric column summing to this value.
    Sum(&'static str, f64),
    /// A column holding exactly these cells, in row order.
    Cells(&'static str, Vec<String>),
}

/// What statement `id` must answer over driven or journey rows; empty for
/// a statement no count is known for. Every execution is `completed` or
/// `pending`: the server records no `status` event, so the `waiting`
/// filter answers no row.
#[must_use]
pub fn known(id: &str, journey: Journey) -> Vec<Known> {
    let executions = DIRECT + SUBMITTED;
    let half = executions / 2;
    let calls = f64::from(CALLS);
    let cells = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    match id {
        "stat-invocations-total" | "stat-invocations-buckets" => {
            vec![Known::Sum("value", journey.invocations() as f64)]
        }
        "stat-calls-total" | "stat-calls-buckets" => vec![Known::Sum("value", calls)],
        "stat-tokens-total" | "stat-tokens-buckets" => {
            vec![Known::Sum("value", calls * f64::from(TOKENS))]
        }
        "stat-cost-total" => vec![
            Known::Cells("currency", cells(&["USD"])),
            Known::Sum("value", calls * COST),
        ],
        "stat-cost-buckets" => vec![Known::Sum("value", calls * COST)],
        "verify-list" => vec![Known::Rows(51)],
        "verify-facet-status" => vec![
            Known::Cells("value", cells(&["completed", "pending"])),
            Known::Cells("n", vec![DIRECT.to_string(), SUBMITTED.to_string()]),
        ],
        "verify-facet-verdict" => vec![
            Known::Cells("value", cells(&["failed", "passed"])),
            Known::Cells("n", vec![half.to_string(), half.to_string()]),
        ],
        "verify-chart-verdict" => vec![Known::Sum("executions", f64::from(executions))],
        "verify-filter-verdict" => vec![Known::Rows(usize::try_from(half).unwrap_or(usize::MAX))],
        "verify-filter-status" => vec![Known::Rows(0)],
        _ => Vec::new(),
    }
}

/// Checks statement `id`'s reference `answer` against [`known`].
///
/// # Errors
///
/// Returns the first property the answer misses.
pub fn check(id: &str, answer: &Answer, journey: Journey) -> Result<(), String> {
    for property in known(id, journey) {
        let held = match &property {
            Known::Rows(rows) => answer.rows.len() == *rows,
            Known::Sum(column, sum) => {
                let total: f64 = answer
                    .column(column)
                    .into_iter()
                    .map(|cell| {
                        cell.and_then(|cell| cell.parse::<f64>().ok())
                            .unwrap_or(f64::NAN)
                    })
                    .sum();
                !answer.rows.is_empty() && (total - sum).abs() <= sum.abs() * 1e-9
            }
            Known::Cells(column, cells) => {
                answer.column(column)
                    == cells
                        .iter()
                        .map(|cell| Some(cell.as_str()))
                        .collect::<Vec<_>>()
            }
        };
        if !held {
            return Err(format!(
                "{id} answers {} rows {:?}, not {property:?}",
                answer.rows.len(),
                answer.rows
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::answer::{Answer, Column};
    use super::super::fixture::Journey;
    use super::check;

    /// A one-column answer of `cells`.
    fn answer(column: &str, cells: &[&str]) -> Answer {
        Answer {
            columns: vec![Column {
                name: column.to_owned(),
                data_type: "Int64".to_owned(),
                metadata: std::collections::BTreeMap::new(),
            }],
            rows: cells
                .iter()
                .map(|cell| vec![Some((*cell).to_owned())])
                .collect(),
        }
    }

    /// An empty answer fails a statement over driven rows, the expected
    /// count passes, and a statement nothing names passes as it is.
    ///
    /// # Panics
    ///
    /// Panics when an empty or wrong answer passes, or a right one fails.
    #[test]
    fn driven_statements_must_count_what_was_driven() {
        assert!(check("stat-calls-total", &answer("value", &[]), Journey::DEFAULT).is_err());
        assert!(
            check(
                "stat-calls-total",
                &answer("value", &["99"]),
                Journey::DEFAULT
            )
            .is_err()
        );
        assert_eq!(
            check(
                "stat-calls-total",
                &answer("value", &["100"]),
                Journey::DEFAULT
            ),
            Ok(())
        );
        assert!(
            check(
                "verify-filter-status",
                &answer("value", &["1"]),
                Journey::DEFAULT
            )
            .is_err()
        );
        assert_eq!(
            check(
                "verify-filter-status",
                &answer("value", &[]),
                Journey::DEFAULT
            ),
            Ok(())
        );
        assert_eq!(
            check("trace-list", &answer("value", &[]), Journey::DEFAULT),
            Ok(())
        );
    }
}
