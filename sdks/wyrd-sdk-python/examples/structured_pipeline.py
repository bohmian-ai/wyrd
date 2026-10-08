"""Structured output with typed wire callbacks."""

from wyrd import Agent, Prompt, ProviderRequest, ProviderResponse


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
    # The mock provider answers with the user message, so this is the plan.
    run = planner.run(r'{"summary":"mock summary","steps":["read","write"]}')
    print("finish:", run.finish_reason)
    print("plan:", run.structured_output)


if __name__ == "__main__":
    main()
