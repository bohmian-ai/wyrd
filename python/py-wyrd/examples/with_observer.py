"""Custom observer example attached to the workflow via observers=."""

import threading

from wyrd import Agent, Observer, OtelObserver, Prompt, Workflow


class TokenCounter(Observer):
    def __init__(self) -> None:
        self._lock = threading.Lock()
        self.calls = 0
        self.total_tokens_in = 0
        self.total_tokens_out = 0

    def on_model_call(self, run_id, agent_id, iteration, provider, model, request):
        with self._lock:
            self.calls += 1

    def on_model_result(self, run_id, agent_id, iteration, finish_reason, synthetic, response):
        if synthetic:
            return
        try:
            usage = response.openai().usage
        except Exception:
            return
        if usage:
            with self._lock:
                self.total_tokens_in += usage.prompt_tokens
                self.total_tokens_out += usage.completion_tokens


def main() -> None:
    counter = TokenCounter()
    planner = Agent(
        name="planner",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=[r'{"summary":"mock summary","steps":["read","write"]}'],
            output={"summary": str, "steps": list[str]},
        ),
    )
    writer = Agent(
        name="writer",
        prompt=Prompt(provider="mock", model="mock-model", messages=["Brief: ${summary}"]),
    )
    wf = Workflow.sequential("research", planner, writer, observers=[OtelObserver(), counter])
    run = wf.run({"topic": "the Rust borrow checker"})
    print(f"model calls: {counter.calls}")
    print(f"tokens in:   {counter.total_tokens_in}")
    print(f"tokens out:  {counter.total_tokens_out}")
    print(f"final:       {run.final_output}")


if __name__ == "__main__":
    main()
