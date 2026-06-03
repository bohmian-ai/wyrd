from wyrd import Agent, CallbackOutcome, Prompt
from wyrd.providers import mock_registry


def test_before_model_callback_fires_with_mock_registry() -> None:
    calls: list[dict] = []

    def before_model(ctx, request):
        calls.append({"ctx": ctx, "request": request})
        return CallbackOutcome.Continue

    prompt = Prompt(["hello"], "mock-model", provider="mock")
    agent = Agent(
        prompt=prompt,
        providers=mock_registry("done"),
        before_model_callback=before_model,
    )

    run = agent.run("hello")

    assert run.output == "done"
    assert calls
    assert calls[0]["ctx"]["agent_id"] == agent.id


def test_after_model_replace_with_changes_output() -> None:
    def after_model(ctx, response):
        return CallbackOutcome.replace_with(
            {
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
        )

    agent = Agent(
        prompt=Prompt(["hello"], "mock-model", provider="mock"),
        providers=mock_registry("real"),
        after_model_callback=after_model,
    )

    run = agent.run("hello")

    assert run.output == "synthetic"
