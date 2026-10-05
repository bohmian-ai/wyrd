"""Structured output pipeline with typed wire callbacks."""

from wyrd import Agent, Prompt, ProviderRequest, ProviderResponse, Workflow


def log_request(ctx: object, request: ProviderRequest) -> None:
    try:
        print(f"[callback] model={request.openai().model}")
    except Exception:
        pass


def log_response(ctx: object, response: ProviderResponse) -> None:
    try:
        usage = response.openai().usage
        if usage:
            print(f"[callback] tokens={usage.total_tokens}")
    except Exception:
        pass


def main() -> None:
    planner = Agent(
        name="planner",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=[r'{"summary":"mock summary","steps":["read","write"]}'],
            output={"summary": str, "steps": list[str]},
        ),
        before_model_callback=log_request,
        after_model_callback=log_response,
    )
    writer = Agent(
        name="writer",
        prompt=Prompt(provider="mock", model="mock-model", messages=["Brief: ${summary}"]),
    )
    wf = (
        Workflow.sequential("demo", planner, writer)
        .with_step_inputs("writer", {"summary": "steps.planner.output.structured.summary"})
        .with_outputs(
            {"plan": "steps.planner.output.structured", "brief": "steps.writer.output.text"}
        )
    )
    run = wf.run()
    print("status:", run.status)
    print("outputs:", run.outputs)


if __name__ == "__main__":
    main()
