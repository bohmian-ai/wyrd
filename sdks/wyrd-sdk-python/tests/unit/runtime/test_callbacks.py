from wyrd import Agent, FinishReason, Prompt, WyrdError


def test_before_model_callback_fires_with_ambient_mock() -> None:
    calls: list[dict] = []

    def before_model(ctx, request):
        calls.append({"ctx": ctx, "request": request})
        return None

    prompt = Prompt(["hello"], "mock-model", provider="mock")
    agent = Agent(prompt=prompt, before_model_callback=before_model)

    run = agent.run("hello")

    assert "hello" in run.output
    assert calls
    assert calls[0]["ctx"]["agent_id"] == agent.id


def test_after_model_replace_with_changes_output() -> None:
    def after_model(ctx, response):
        return {
            "id": "replacement",
            "object": "chat.completion",
            "created": 0,
            "model": "mock-model",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "synthetic",
                    },
                    "finish_reason": "stop",
                }
            ],
        }

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        after_model_callback=after_model,
    )

    run = agent.run("hello")

    assert run.output == "synthetic"


def test_before_model_raise_aborts_run() -> None:
    def before_model(ctx, request):
        raise WyrdError("WYRD_AGENT_499_CALLBACK_ABORTED", "no")

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        before_model_callback=before_model,
    )

    run = agent.run("hello")

    assert run.finish_reason == FinishReason.CallbackAborted
    assert run.error is not None
    assert run.error.code == "WYRD_AGENT_499_CALLBACK_ABORTED"
