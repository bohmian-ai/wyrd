//! Declare a refund agent's workflow Verifier in code, answer one refund
//! request twice, once with the order lookup and once without it, and verify
//! each reply with the trace of the work behind it.

use std::error::Error;

use opentelemetry::trace::TraceContextExt as _;
use tracing_opentelemetry::OpenTelemetrySpanExt as _;
use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{Check, ComparisonOperator, InputSlot, Requirement, Service, Verifier};

/// A synthetic order record the agent's lookup tool reads.
struct Order {
    /// The order number.
    id: &'static str,
    /// The day the order was delivered.
    delivered: &'static str,
}

const ORDER_1042: Order = Order {
    id: "1042",
    delivered: "2 May",
};

// ANCHOR: verifier
/// The workflow check: one requirement on the answer, one on what ran.
fn refund_workflow() -> Verifier {
    Verifier::new("refund-workflow", None)
        .inputs([
            ("reply", InputSlot::json(json!({ "type": "string" }))),
            ("trace", InputSlot::trace()),
        ])
        .requirements([
            // Judges the answer only: a reply that skipped the lookup passes it too.
            Requirement::new(
                "reply-names-order",
                "The reply names the customer's order.",
                Check::assert("inputs.reply", ComparisonOperator::Contains, "Order 1042"),
            ),
            // Judges what ran: the agent's trace must hold a look-up-order span.
            Requirement::new(
                "looked-up-order",
                "The agent looked up the order before replying.",
                Check::trace(
                    "inputs.trace",
                    "$.spans[*].name",
                    ComparisonOperator::IsSuperset,
                    json!(["look-up-order"]),
                ),
            ),
        ])
}
// ANCHOR_END: verifier

// ANCHOR: agent
/// The agent's lookup tool.
fn look_up_order(order_id: &str) -> &'static Order {
    let _span = tracing::info_span!("look-up-order", order_id).entered();
    &ORDER_1042
}

/// Answer a refund request; return the reply and the trace ID of the work
/// behind it.
fn refund_agent(order_id: &str, remembered: Option<&Order>) -> (String, String) {
    let span = tracing::info_span!("answer-refund");
    let reply = span.in_scope(|| {
        // The bug: an order remembered from earlier in the conversation skips the lookup.
        let order = remembered.unwrap_or_else(|| look_up_order(order_id));
        let _draft = tracing::info_span!("draft-reply").entered();
        format!(
            "Order {} was delivered on {}, so it qualifies for a refund.",
            order.id, order.delivered
        )
    });
    let trace = span.context().span().span_context().trace_id().to_string();
    (reply, trace)
}
// ANCHOR_END: agent

/// Declare the Cards, answer the request with and without the lookup, and
/// verify each reply, printing every requirement's outcome.
///
/// # Errors
/// Returns the client's configuration or credential error, the declaration's
/// refusal, a telemetry export failure, a refused check, or a failed shutdown.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let state =
        WyrdState::declare(&Service::new("refund-agent", None).verified_by([refund_workflow()]))
            .await?;
    // Export the agent's spans to Wyrd.
    let telemetry = wyrd_ai::otel::start_telemetry(&state)?;

    // ANCHOR: verify
    let complete = refund_agent("1042", None);
    let skipped = refund_agent("1042", Some(&ORDER_1042));
    // Store the spans before they are checked.
    telemetry.shutdown()?;

    let cases = [
        (
            "complete",
            json!({ "reply": complete.0, "trace": complete.1 }),
        ),
        ("skipped", json!({ "reply": skipped.0, "trace": skipped.1 })),
        // The same reply sent without its trace: nothing shows what ran.
        ("no trace", json!({ "reply": complete.0 })),
    ];
    for (label, evidence) in cases {
        let reply = evidence["reply"].as_str().map(str::to_owned);
        let result = state
            .run()
            .observe()
            .verify("refund-workflow", evidence)
            .await?;
        println!(
            "{label}: {} · {}",
            result.outcome.verdict,
            reply.unwrap_or_default()
        );
        for requirement in &result.outcome.requirements {
            println!(
                "  {}: {}",
                requirement.id.as_str(),
                json!(requirement.outcome).as_str().unwrap_or_default()
            );
        }
    }
    state.shutdown().await?;
    // ANCHOR_END: verify
    Ok(())
}
