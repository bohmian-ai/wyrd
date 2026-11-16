//! Author a Service, its Agent, and its Verifier in code, declare them,
//! verify two answers, and record each answer in the Service's table.

use wyrd_ai::serde_json::json;
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Agent, Check, ComparisonOperator, Component, Inputs, OpenAiChatOptions, Requirement, Service,
    Table, Verifier, openai_chat,
};

/// Declares `refund-replies` from code, prints the exact Service and Agent
/// versions declaration selected, verifies a passing and a failing answer on
/// the Agent's view, and writes each answer and verdict to `refund_replies`.
///
/// # Errors
///
/// Returns the client's configuration or credential error, the declaration's
/// refusal, a verification or write refusal, and a failed shutdown drain.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // The Verifier: one requirement over the evidence slot named `answer`.
    let no_refund_guarantee = Verifier::new("no-refund-guarantee", None)
        .inputs([("answer", json!({ "type": "string" }))])
        .requirements([Requirement::new(
            "no-refund-guarantee",
            "The answer never promises a guaranteed refund.",
            Check::assert(
                "inputs.answer",
                ComparisonOperator::NotContains,
                "guaranteed refund",
            ),
        )]);

    // The component: an Agent and the Prompt it runs.
    let refund_agent = Agent::new("refund-agent", None)
        .prompt(openai_chat(
            "gpt-4o",
            OpenAiChatOptions {
                messages: vec!["Answer the refund question: {{question}}".into()],
                ..Default::default()
            },
        )?)
        .execution_route(wyrd_ai::ExecutionRoute::WyrdGateway {
            deployment: "openai".parse()?,
        });

    // The Service: the Agent under the alias `agent`, verified by the
    // Verifier, and a table for the answers it gives.
    let service = Service::new("refund-replies", None)
        // One column per property, in order.
        .tables([Table::new(
            "refund_replies",
            json!({
                "type": "object",
                "properties": {
                    "answer": { "type": "string" },
                    "verdict": { "type": "string" },
                },
                "required": ["answer", "verdict"],
            }),
        )])
        .components([Component::new("agent", refund_agent).verified_by([no_refund_guarantee])]);

    // No Card names a version: declaration selects each one and returns them.
    let state = WyrdState::declare(&service).await?;
    println!("service: {}", state.root_ref());
    println!("agent:   {}", state.card_ref("agent")?);

    state.start_bifrost().await?; // the writer; the one declared table is active
    for answer in [
        "Your refund request is approved.",
        "You have a guaranteed refund.",
    ] {
        // A Run whose view is the Agent, where the Verifier is bound.
        let run = state.run_for_card("agent")?;
        let result = run
            .observe()
            .verify(
                "no-refund-guarantee",
                Inputs::named([("answer", json!(answer))]),
            )
            .await?;
        // Buffered, not yet stored: shutdown() drains it.
        run.bifrost()
            .write(&json!({ "answer": answer, "verdict": result.outcome.verdict }))
            .await?;
        println!("{}: {answer}", result.outcome.verdict);
    }
    state.shutdown().await?;
    Ok(())
}
