//! Custom observer example attached with Workflow::with_observers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use skald_agent::Observer;
use skald_spec::{ProviderRequest, ProviderResponse};
use skald_workflow::{Workflow, WorkflowInput};
use wyrd_observe::OtelObserver;

mod common;
use common::{mock_registry, plan_prompt, write_prompt};

#[derive(Default)]
struct TokenCounter {
    calls: AtomicU64,
    tokens_in: AtomicU64,
    tokens_out: AtomicU64,
}

#[async_trait]
impl Observer for TokenCounter {
    async fn on_model_call(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _provider: &str,
        _model: &str,
        _request: &ProviderRequest,
    ) {
        self.calls.fetch_add(1, Ordering::Relaxed);
    }

    async fn on_model_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _finish_reason: &str,
        synthetic: bool,
        response: &ProviderResponse,
    ) {
        if synthetic {
            return;
        }
        if let ProviderResponse::OpenAiChatCompletion(response) = response {
            if let Some(usage) = &response.usage {
                self.tokens_in
                    .fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
                self.tokens_out
                    .fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
            }
        }
    }
}

fn main() -> anyhow::Result<()> {
    let counter = Arc::new(TokenCounter::default());
    let planner = skald_agent::Agent::new(plan_prompt()).name("planner");
    let writer = skald_agent::Agent::new(write_prompt()).name("writer");
    let wf = Workflow::sequential("research", vec![planner, writer])?.with_observers(vec![
        Arc::new(OtelObserver::new()) as Arc<dyn Observer>,
        Arc::clone(&counter) as Arc<dyn Observer>,
    ]);
    let providers = mock_registry(&[
        r#"{"summary":"mock summary","steps":["read","write"]}"#,
        "Final observed brief",
    ]);
    let run = wyrd_runtime::runtime().block_on(wf.run_with(
        &providers,
        WorkflowInput::from(HashMap::from([(
            "topic".to_owned(),
            "the Rust borrow checker".to_owned(),
        )])),
    ))?;
    println!("model calls: {}", counter.calls.load(Ordering::Relaxed));
    println!("tokens in:   {}", counter.tokens_in.load(Ordering::Relaxed));
    println!(
        "tokens out:  {}",
        counter.tokens_out.load(Ordering::Relaxed)
    );
    println!("final:       {:?}", run.final_output);
    Ok(())
}
