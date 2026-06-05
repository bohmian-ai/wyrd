//! Custom observer example attached with Workflow::with_observers.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use skald_agent::Observer;
use skald_spec::{ProviderRequest, ProviderResponse};
use skald_workflow::{Workflow, WorkflowInput};
use wyrd_observe::OtelObserver;

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
    let wf = Workflow::load(
        "examples/workflows/research.yaml",
        skald_tool::default_registry(),
        skald_agent::default_prompt_resolver(),
    )?
    .with_observers(vec![
        Arc::new(OtelObserver::new()) as Arc<dyn Observer>,
        Arc::clone(&counter) as Arc<dyn Observer>,
    ]);
    let run = wyrd_runtime::runtime().block_on(wf.run(WorkflowInput::from(HashMap::from([(
        "topic".to_owned(),
        "the Rust borrow checker".to_owned(),
    )]))))?;
    println!("model calls: {}", counter.calls.load(Ordering::Relaxed));
    println!("tokens in:   {}", counter.tokens_in.load(Ordering::Relaxed));
    println!("tokens out:  {}", counter.tokens_out.load(Ordering::Relaxed));
    println!("final:       {:?}", run.final_output);
    Ok(())
}
