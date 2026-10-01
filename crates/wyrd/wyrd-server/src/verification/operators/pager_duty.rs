//! PagerDuty Events API v2 wire format.
//!
//! Owns only the enqueue event body. Screening, response bounds, and HTTP
//! status classification stay with [`OperatorDelivery`](super::OperatorDelivery);
//! PagerDuty success is the HTTP status alone.

use serde_json::Value;
use wyrd_spec::auth::SecretBearer;
use wyrd_spec::card::operator::{OperatorFailureContext, PagerDutySeverity};

use super::{Attempt, INVALID_REQUEST};

/// The `trigger` event for one dispatch, carrying the routing key in the body.
///
/// The dedup key defaults to the dispatch id, so PagerDuty may group retry
/// events of one dispatch. This is not an exactly-once delivery or
/// incident-grouping guarantee.
///
/// # Errors
/// Returns a terminal [`INVALID_REQUEST`] attempt when the summary or dedup
/// key template does not render against `context`.
pub(super) fn event(
    routing_key: &SecretBearer,
    route: &str,
    severity: PagerDutySeverity,
    summary: &str,
    dedup_key: Option<&str>,
    context: &OperatorFailureContext,
) -> Result<Value, Attempt> {
    let dedup = dedup_key.map_or_else(
        || Ok(context.dispatch_id.to_string()),
        |key| context.render(key),
    );
    let (Ok(summary), Ok(dedup)) = (context.render(summary), dedup) else {
        return Err(Attempt::terminal(
            INVALID_REQUEST,
            "a PagerDuty template is invalid",
        ));
    };
    Ok(serde_json::json!({
        "routing_key": routing_key.expose(),
        "event_action": "trigger",
        "dedup_key": dedup,
        "payload": {
            "summary": summary,
            "source": context.subject_ref,
            "severity": severity,
            "custom_details": {
                "wyrd_route": route,
                "run_id": context.run_id,
                "result_id": context.result_id,
                "binding_id": context.binding_id,
                "verifier_ref": context.verifier_ref,
            },
        },
    }))
}
