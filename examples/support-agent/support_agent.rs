//! Declare a support Service and its Verifier in code, ask a support agent
//! one question, and verify its reply and a policy-breaking reply.

use std::error::Error;

use schemars::JsonSchema;
use serde::Serialize;
use wyrd_ai::serde_json::{self, Value, json};
use wyrd_ai::state::WyrdState;
use wyrd_ai::verify::{
    Agent, Check, ComparisonOperator, InputSlot, OpenAiChatOptions, Requirement, ResponseFormat,
    Service, Verifier, openai_chat,
};
use wyrd_ai::{ExecutionRoute, RequirementOutcome};

const POLICY: &str = "Refunds are possible within 30 days of purchase. Never promise a refund.";

/// The evidence the application sends for each reply.
#[derive(Serialize, JsonSchema)]
struct Reply {
    /// The customer's question.
    question: String,
    /// The support agent's answer.
    answer: String,
}

/// The judge: a model that reads the reply and decides whether it follows
/// the policy. Wyrd runs it through the `openai` gateway deployment.
///
/// # Errors
/// Returns the Prompt builder's or the route's refusal.
fn policy_judge() -> Result<Agent, Box<dyn Error>> {
    let prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            messages: vec![
                format!("Policy: {POLICY} Does this support answer follow the policy? ")
                    + "Question: {{question}} Answer: {{answer}}",
            ],
            response_format: Some(ResponseFormat::verdict(None)),
            ..Default::default()
        },
    )?;
    Ok(Agent::new("policy-judge", None)
        .prompt(prompt)
        .execution_route(ExecutionRoute::WyrdGateway {
            deployment: "openai".parse()?,
        }))
}

/// Your support agent: any model call.
///
/// # Errors
/// Returns the request's transport or status error.
async fn support_agent(question: &str) -> Result<String, Box<dyn Error>> {
    let base =
        std::env::var("OPENAI_BASE_URL").unwrap_or_else(|_| "https://api.openai.com/v1".into());
    let completion: Value = reqwest::Client::new()
        .post(format!("{base}/chat/completions"))
        .bearer_auth(std::env::var("OPENAI_API_KEY")?)
        .json(&json!({
            "model": "gpt-4o-mini",
            "messages": [
                { "role": "system", "content": format!("You are a support agent. {POLICY}") },
                { "role": "user", "content": question },
            ],
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(completion["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or_default()
        .to_owned())
}

/// Declare the Cards, then verify the agent's reply and a reply that breaks
/// the policy, printing each verdict and every failing requirement's reason.
///
/// # Errors
/// Returns the client's configuration or credential error, the declaration's
/// refusal, the agent's request error, a refused check, or a failed shutdown.
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // The Verifier: two requirements every reply must meet.
    let reply_check = Verifier::new("reply-check", None)
        .inputs(InputSlot::fields_of::<Reply>())
        .requirements([
            Requirement::new(
                "no-refund-promise",
                "The answer never promises a guaranteed refund.",
                Check::assert(
                    "inputs.answer",
                    ComparisonOperator::NotContains,
                    "guaranteed refund",
                ),
            ),
            Requirement::new(
                "follows-policy",
                "The answer follows the refund policy.",
                Check::agent(policy_judge()?)
                    .inputs([("question", "inputs.question"), ("answer", "inputs.answer")]),
            ),
        ]);

    // Declare the Cards and get a ready state.
    let state =
        WyrdState::declare(&Service::new("support", None).verified_by([reply_check])).await?;

    let question = "I bought this 45 days ago. Can I get my money back?";
    // The agent's reply, and a reply that breaks the policy.
    for answer in [
        support_agent(question).await?,
        "Yes, you have a guaranteed refund.".into(),
    ] {
        let reply = Reply {
            question: question.into(),
            answer,
        };
        let input = serde_json::to_value(&reply)?;
        let result = state.run().observe().verify("reply-check", input).await?;
        println!("{}: {}", result.outcome.verdict, reply.answer);
        for requirement in &result.outcome.requirements {
            if requirement.outcome != RequirementOutcome::Passed {
                println!("  {}: {}", requirement.id.as_str(), requirement.reason);
            }
        }
    }
    state.shutdown().await?;
    Ok(())
}
