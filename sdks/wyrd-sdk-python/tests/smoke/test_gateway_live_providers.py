"""Opt-in live provider smoke journeys through the Wyrd gateway.

This lane is the only place a gateway call reaches a real upstream. Each
provider's own client calls its deployment on the session's one live server, so
a failure here is a gateway relay defect or a provider refusal. Nothing
here asserts token counts or deterministic failures: the proof is connectivity
and a parseable response shape.

Run with ``mise run test:gateway:smoke:live``; the credential-free
``test:gateway:journey`` lane never selects it.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any, Literal

import anthropic
import httpx
import jsonschema
import openai
import pytest
from anthropic.types import RawContentBlockDeltaEvent, TextBlock, TextDelta, ToolUseBlock
from google import genai
from openai.types.chat import ChatCompletionMessageFunctionToolCall
from openai.types.responses import ResponseCompletedEvent
from wyrd.prompt import Prompt

from .conftest import (
    JEV_QUESTIONS,
    JEV_STATE,
    LiveGateway,
    assert_jev_answers,
    credential,
    model,
    operation_model,
)

MESSAGES: list[Any] = [{"role": "user", "content": "Reply with the single word: ready"}]

# Prompt every non-chat family sends. It is deliberately trivial: this lane
# proves that a real provider payload decodes through the typed schemas, not
# that a model is good at anything.
PROMPT = "Reply with the single word: ready"

# Output budget every call asks for. It is generous for a one-word answer
# because a thinking model spends this budget on reasoning first: too small a
# cap returns a truncated answer carrying no text, which proves nothing about
# connectivity.
MAX_OUTPUT_TOKENS = 512

SYSTEM = "You answer every question with the single word: ready"

# The one function every forced tool call names. Its schema is plain JSON
# Schema with no keyword a provider's function parameters refuse.
TOOL_NAME = "get_weather"
TOOL_DESCRIPTION = "Return the current weather for a city."
TOOL_PARAMETERS: dict[str, Any] = {
    "type": "object",
    "properties": {"city": {"type": "string"}},
    "required": ["city"],
}
TOOL_PROMPT = "What is the weather in Paris?"

# Structured-output schema. OpenAI strict mode and Anthropic require the
# closed object; Gemini's ``responseSchema`` refuses ``additionalProperties``,
# so its native call sends the open form.
ANSWER_SCHEMA: dict[str, Any] = {
    "type": "object",
    "properties": {"answer": {"type": "string"}},
    "required": ["answer"],
    "additionalProperties": False,
}
OPEN_ANSWER_SCHEMA: dict[str, Any] = {
    key: value for key, value in ANSWER_SCHEMA.items() if key != "additionalProperties"
}
STRUCTURED_PROMPT = "Reply with JSON whose answer is the single word: ready"

VERDICT_PROMPT = (
    "Judge whether the answer `ready` is one word. Cite the input slot `answer` "
    "at JSON pointer `/text`."
)


def verdict_format(tmp_path: Path) -> dict[str, Any]:
    """The ``response_format`` an OpenAI Chat Prompt declaring ``verdict`` lowers to.

    ``Prompt.load`` compiles the body the way a stored Prompt Card is
    compiled, so the schema is the server's runtime-generated plain Verdict,
    never a copied payload.
    """
    path = tmp_path / "verdict-prompt.json"
    authored = {
        "provider": "openai",
        "model": model("openai"),
        "messages": [{"role": "user", "content": VERDICT_PROMPT}],
        "response_format": "verdict",
    }
    path.write_text(json.dumps(authored))
    return Prompt.load(path).render().model_dump()["body"]["response_format"]


def assert_verdict(text: str | None, schema: dict[str, Any]) -> None:
    """Assert ``text`` is a JSON object satisfying ``schema``, the exact schema sent."""
    assert text, "the provider returned no Verdict text"
    jsonschema.Draft202012Validator(schema).validate(json.loads(text))


def test_assert_verdict_validates_against_the_sent_schema(tmp_path: Path) -> None:
    """The Verdict assertion accepts a valid Verdict and rejects replies the schema forbids."""
    schema = verdict_format(tmp_path)["json_schema"]["schema"]
    cited = {"slot": "answer", "locator": {"json_pointer": "/text"}}
    assert_verdict(json.dumps({"outcome": "passed", "reason": "r", "citations": [cited]}), schema)
    for citations in ({}, [{"slot": "answer", "locator": {"page": "not-an-integer"}}]):
        with pytest.raises(jsonschema.ValidationError):
            assert_verdict(
                json.dumps({"outcome": "passed", "reason": "r", "citations": citations}), schema
            )


def openai_compatible(live: LiveGateway) -> openai.OpenAI:
    """The unmodified OpenAI client aimed at the live server's ``/v1`` routes."""
    return openai.OpenAI(base_url=f"{live.base_url}/v1", api_key=live.token, max_retries=0)


@pytest.fixture
def openai_client(live_gateway: LiveGateway) -> openai.OpenAI:
    """The OpenAI client for the OpenAI cases, skipping them without an OpenAI key."""
    credential("openai")
    return openai_compatible(live_gateway)


@pytest.fixture
def anthropic_client(live_gateway: LiveGateway) -> anthropic.Anthropic:
    """The official Anthropic client aimed at the live server's native ingress."""
    credential("anthropic")
    return anthropic.Anthropic(
        base_url=live_gateway.base_url, api_key=live_gateway.token, max_retries=0
    )


@pytest.fixture
def gemini_client(live_gateway: LiveGateway) -> genai.Client:
    """The official Google GenAI client aimed at the live server's native ingress."""
    credential("gemini")
    return genai.Client(
        api_key=live_gateway.token,
        http_options=genai.types.HttpOptions(base_url=live_gateway.base_url, retry_options=None),
    )


@pytest.mark.smoke
@pytest.mark.parametrize("provider", ["openai", "anthropic", "gemini"])
def test_openai_compatible_client_reaches_a_live_provider(
    provider: Literal["openai", "anthropic", "gemini"], live_gateway: LiveGateway
) -> None:
    """The unmodified OpenAI client reaches each real provider through the server.

    For OpenAI this is also the plain-text native Chat Completions case.
    """
    credential(provider)
    completion = openai_compatible(live_gateway).chat.completions.create(
        model=f"{provider}/{model(provider)}",
        messages=MESSAGES,
        max_completion_tokens=MAX_OUTPUT_TOKENS,
    )
    assert completion.choices, f"{provider} returned no choice"
    assert isinstance(completion.choices[0].message.content, str)
    assert completion.usage is not None, f"{provider} reported no usage"


@pytest.mark.smoke
def test_openai_chat_relays_a_system_message(openai_client: openai.OpenAI) -> None:
    """A Chat Completions system message reaches OpenAI and text comes back."""
    messages: list[Any] = [{"role": "system", "content": SYSTEM}, *MESSAGES]
    completion = openai_client.chat.completions.create(
        model=f"openai/{model('openai')}",
        messages=messages,
        max_completion_tokens=MAX_OUTPUT_TOKENS,
    )
    assert completion.choices[0].message.content, "openai returned no text"


@pytest.mark.smoke
def test_openai_chat_relays_a_stream(openai_client: openai.OpenAI) -> None:
    """A streamed Chat Completion arrives as relayed chunks carrying text."""
    chunks = openai_client.chat.completions.create(
        model=f"openai/{model('openai')}",
        messages=MESSAGES,
        max_completion_tokens=MAX_OUTPUT_TOKENS,
        stream=True,
    )
    text = "".join(chunk.choices[0].delta.content or "" for chunk in chunks if chunk.choices)
    assert text.strip(), "the relayed stream carried no text"


@pytest.mark.smoke
def test_openai_chat_relays_a_forced_tool_call(openai_client: openai.OpenAI) -> None:
    """A forced function tool comes back as one call with parseable arguments.

    ``reasoning_effort: none`` is the model's own requirement: a reasoning
    model refuses function tools while it reasons.
    """
    tools: list[Any] = [
        {
            "type": "function",
            "function": {
                "name": TOOL_NAME,
                "description": TOOL_DESCRIPTION,
                "parameters": TOOL_PARAMETERS,
            },
        }
    ]
    completion = openai_client.chat.completions.create(
        model=f"openai/{model('openai')}",
        messages=[{"role": "user", "content": TOOL_PROMPT}],
        tools=tools,
        tool_choice={"type": "function", "function": {"name": TOOL_NAME}},
        reasoning_effort="none",
        max_completion_tokens=MAX_OUTPUT_TOKENS,
    )
    calls = completion.choices[0].message.tool_calls
    assert calls, "openai returned no tool call"
    call = calls[0]
    assert isinstance(call, ChatCompletionMessageFunctionToolCall)
    assert call.function.name == TOOL_NAME
    assert json.loads(call.function.arguments)["city"]


@pytest.mark.smoke
def test_openai_chat_relays_structured_output(openai_client: openai.OpenAI) -> None:
    """A strict JSON Schema ``response_format`` comes back as parseable JSON."""
    completion = openai_client.chat.completions.create(
        model=f"openai/{model('openai')}",
        messages=[{"role": "user", "content": STRUCTURED_PROMPT}],
        response_format={
            "type": "json_schema",
            "json_schema": {"name": "answer", "schema": ANSWER_SCHEMA, "strict": True},
        },
        max_completion_tokens=MAX_OUTPUT_TOKENS,
    )
    content = completion.choices[0].message.content
    assert content is not None, "openai returned no content"
    assert isinstance(json.loads(content)["answer"], str)


@pytest.mark.smoke
def test_anthropic_client_reaches_live_anthropic(anthropic_client: anthropic.Anthropic) -> None:
    """The official Anthropic client reaches real Anthropic through native ingress.

    Native ingress names the ``<deployment>/<model>`` projection, as every
    gateway route does; the gateway sends the provider its own model id.
    """
    message = anthropic_client.messages.create(
        model=f"anthropic/{model('anthropic')}",
        max_tokens=MAX_OUTPUT_TOKENS,
        messages=MESSAGES,
    )
    assert message.content, "anthropic returned no content block"
    assert message.usage.output_tokens >= 0


@pytest.mark.smoke
def test_anthropic_messages_relays_a_system_instruction(
    anthropic_client: anthropic.Anthropic,
) -> None:
    """A Messages ``system`` field reaches Anthropic and text comes back."""
    message = anthropic_client.messages.create(
        model=f"anthropic/{model('anthropic')}",
        max_tokens=MAX_OUTPUT_TOKENS,
        system=SYSTEM,
        messages=MESSAGES,
    )
    assert any(isinstance(block, TextBlock) and block.text for block in message.content)


@pytest.mark.smoke
def test_anthropic_messages_relays_a_stream(anthropic_client: anthropic.Anthropic) -> None:
    """A streamed Message arrives as relayed events ending in ``message_stop``."""
    events = list(
        anthropic_client.messages.create(
            model=f"anthropic/{model('anthropic')}",
            max_tokens=MAX_OUTPUT_TOKENS,
            messages=MESSAGES,
            stream=True,
        )
    )
    text = "".join(
        event.delta.text
        for event in events
        if isinstance(event, RawContentBlockDeltaEvent) and isinstance(event.delta, TextDelta)
    )
    assert text.strip(), "the relayed stream carried no text"
    assert events[-1].type == "message_stop", "the relayed stream carried no terminal event"


@pytest.mark.smoke
def test_anthropic_messages_relays_a_forced_tool_call(
    anthropic_client: anthropic.Anthropic,
) -> None:
    """A forced tool comes back as one ``tool_use`` block with parsed input."""
    message = anthropic_client.messages.create(
        model=f"anthropic/{model('anthropic')}",
        max_tokens=MAX_OUTPUT_TOKENS,
        messages=[{"role": "user", "content": TOOL_PROMPT}],
        tools=[
            {
                "name": TOOL_NAME,
                "description": TOOL_DESCRIPTION,
                "input_schema": TOOL_PARAMETERS,
            }
        ],
        tool_choice={"type": "tool", "name": TOOL_NAME},
    )
    calls = [block for block in message.content if isinstance(block, ToolUseBlock)]
    assert calls, "anthropic returned no tool_use block"
    assert calls[0].name == TOOL_NAME
    assert isinstance(calls[0].input, dict)
    assert calls[0].input["city"]


@pytest.mark.smoke
def test_anthropic_messages_relays_structured_output(
    anthropic_client: anthropic.Anthropic,
) -> None:
    """A JSON Schema ``output_config`` comes back as parseable JSON text."""
    message = anthropic_client.messages.create(
        model=f"anthropic/{model('anthropic')}",
        max_tokens=MAX_OUTPUT_TOKENS,
        messages=[{"role": "user", "content": STRUCTURED_PROMPT}],
        output_config={"format": {"type": "json_schema", "schema": ANSWER_SCHEMA}},
    )
    text = "".join(block.text for block in message.content if isinstance(block, TextBlock))
    assert isinstance(json.loads(text)["answer"], str)


@pytest.mark.smoke
def test_google_client_reaches_live_gemini(gemini_client: genai.Client) -> None:
    """The official Google GenAI client reaches real Gemini through native ingress."""
    answer = gemini_client.models.generate_content(
        model=f"gemini/{model('gemini')}", contents=PROMPT
    )
    assert answer.candidates, "gemini returned no candidate"


@pytest.mark.smoke
def test_gemini_generate_content_relays_a_system_instruction(gemini_client: genai.Client) -> None:
    """A ``systemInstruction`` reaches Gemini and text comes back."""
    answer = gemini_client.models.generate_content(
        model=f"gemini/{model('gemini')}",
        contents=PROMPT,
        config=genai.types.GenerateContentConfig(system_instruction=SYSTEM),
    )
    assert answer.text, "gemini returned no text"


@pytest.mark.smoke
def test_gemini_generate_content_relays_a_stream(gemini_client: genai.Client) -> None:
    """A ``streamGenerateContent`` answer arrives as relayed chunks carrying text."""
    chunks = list(
        gemini_client.models.generate_content_stream(
            model=f"gemini/{model('gemini')}", contents=PROMPT
        )
    )
    assert chunks, "the relayed stream carried no chunk"
    assert "".join(chunk.text or "" for chunk in chunks).strip(), "the stream carried no text"


@pytest.mark.smoke
def test_gemini_generate_content_relays_a_forced_tool_call(gemini_client: genai.Client) -> None:
    """A forced function declaration comes back as one call with parsed arguments."""
    answer = gemini_client.models.generate_content(
        model=f"gemini/{model('gemini')}",
        contents=TOOL_PROMPT,
        config=genai.types.GenerateContentConfig(
            tools=[
                genai.types.Tool(
                    function_declarations=[
                        genai.types.FunctionDeclaration(
                            name=TOOL_NAME,
                            description=TOOL_DESCRIPTION,
                            parameters_json_schema=TOOL_PARAMETERS,
                        )
                    ]
                )
            ],
            tool_config=genai.types.ToolConfig(
                function_calling_config=genai.types.FunctionCallingConfig(
                    mode=genai.types.FunctionCallingConfigMode.ANY,
                    allowed_function_names=[TOOL_NAME],
                )
            ),
        ),
    )
    calls = answer.function_calls
    assert calls, "gemini returned no function call"
    assert calls[0].name == TOOL_NAME
    assert calls[0].args and calls[0].args["city"]


@pytest.mark.smoke
def test_gemini_generate_content_relays_structured_output(gemini_client: genai.Client) -> None:
    """A JSON ``responseSchema`` comes back as parseable JSON text."""
    answer = gemini_client.models.generate_content(
        model=f"gemini/{model('gemini')}",
        contents=STRUCTURED_PROMPT,
        config=genai.types.GenerateContentConfig(
            response_mime_type="application/json",
            response_schema=OPEN_ANSWER_SCHEMA,
        ),
    )
    assert answer.text is not None, "gemini returned no text"
    assert isinstance(json.loads(answer.text)["answer"], str)


@pytest.mark.smoke
def test_openai_chat_accepts_the_named_verdict_format(
    openai_client: openai.OpenAI, tmp_path: Path
) -> None:
    """The named ``verdict`` format lowers to non-strict output OpenAI accepts and answers."""
    response_format: Any = verdict_format(tmp_path)
    assert response_format["json_schema"]["strict"] is False
    completion = openai_client.chat.completions.create(
        model=f"openai/{model('openai')}",
        messages=[{"role": "user", "content": VERDICT_PROMPT}],
        response_format=response_format,
        max_completion_tokens=MAX_OUTPUT_TOKENS,
    )
    assert_verdict(completion.choices[0].message.content, response_format["json_schema"]["schema"])


@pytest.mark.smoke
@pytest.mark.parametrize("provider", ["openai", "anthropic", "gemini"])
def test_live_providers_accept_the_generated_verdict_schema(
    provider: Literal["openai", "anthropic", "gemini"],
    live_gateway: LiveGateway,
    tmp_path: Path,
) -> None:
    """The generated plain Verdict schema is accepted as each provider's strictest output schema.

    OpenAI receives it in strict mode, Anthropic as ``output_config``, and
    Gemini as ``response_json_schema``; each answers a Verdict.
    """
    credential(provider)
    schema = verdict_format(tmp_path)["json_schema"]["schema"]
    if provider == "openai":
        completion = openai_compatible(live_gateway).chat.completions.create(
            model=f"openai/{model('openai')}",
            messages=[{"role": "user", "content": VERDICT_PROMPT}],
            response_format={
                "type": "json_schema",
                "json_schema": {"name": "verdict", "schema": schema, "strict": True},
            },
            max_completion_tokens=MAX_OUTPUT_TOKENS,
        )
        text = completion.choices[0].message.content
    elif provider == "anthropic":
        message = anthropic.Anthropic(
            base_url=live_gateway.base_url, api_key=live_gateway.token, max_retries=0
        ).messages.create(
            model=f"anthropic/{model('anthropic')}",
            max_tokens=MAX_OUTPUT_TOKENS,
            messages=[{"role": "user", "content": VERDICT_PROMPT}],
            output_config={"format": {"type": "json_schema", "schema": schema}},
        )
        text = "".join(block.text for block in message.content if isinstance(block, TextBlock))
    else:
        client = genai.Client(
            api_key=live_gateway.token,
            http_options=genai.types.HttpOptions(
                base_url=live_gateway.base_url, retry_options=None
            ),
        )
        text = client.models.generate_content(
            model=f"gemini/{model('gemini')}",
            contents=VERDICT_PROMPT,
            config=genai.types.GenerateContentConfig(
                response_mime_type="application/json",
                response_json_schema=schema,
                max_output_tokens=MAX_OUTPUT_TOKENS,
            ),
        ).text
    assert_verdict(text, schema)


@pytest.mark.smoke
def test_jev_answers_live_typed_questions(live_gateway: LiveGateway) -> None:
    """Real Jev answers one typed question of each form on both routes.

    The native body is what a Jev client sends to ``POST /v1/systemone``; its
    state carries quotes and a newline as text. A stock OpenAI client then
    sends the same request as the last user message on
    ``/v1/chat/completions``. Every answer must come back in Jev's typed form,
    which proves the request bounds, the pass-through, and the typed answer
    decoding against the real provider.
    """
    credential("jev")
    jev = model("jev")
    native = httpx.post(
        f"{live_gateway.base_url}/v1/systemone",
        json={"model": f"jev/{jev}", "state": JEV_STATE, "questions": JEV_QUESTIONS},
        headers={"authorization": f"Bearer {live_gateway.token}"},
        timeout=60,
    )
    assert native.status_code == 200, f"jev native call answered {native.status_code}"
    completion = openai_compatible(live_gateway).chat.completions.create(
        model=f"jev/{jev}",
        messages=[
            {"role": "system", "content": "Judge the reply."},
            {
                "role": "user",
                "content": json.dumps({"state": JEV_STATE, "questions": JEV_QUESTIONS}),
            },
        ],
    )
    content = completion.choices[0].message.content
    assert content is not None
    assert completion.usage is not None
    for answer in (native.json(), json.loads(content)):
        assert_jev_answers(answer["answers"])


@pytest.mark.smoke
def test_openai_client_reaches_live_responses(openai_client: openai.OpenAI) -> None:
    """Real Responses answers decode buffered and as a relayed event stream.

    Both shapes are covered because the streamed answer is a different wire
    contract: its usage arrives on the terminal ``response.completed`` event
    rather than a top-level object.
    """
    projection = f"openai/{model('openai')}"
    answer = openai_client.responses.create(
        model=projection,
        input=PROMPT,
        max_output_tokens=MAX_OUTPUT_TOKENS,
    )
    assert answer.output_text.strip(), "responses returned no output text"
    assert answer.usage is not None, "responses reported no usage"

    completed = [
        event
        for event in openai_client.responses.create(
            model=projection,
            input=PROMPT,
            max_output_tokens=MAX_OUTPUT_TOKENS,
            stream=True,
        )
        if isinstance(event, ResponseCompletedEvent)
    ]
    assert completed, "the relayed stream carried no terminal event"
    assert completed[-1].response.usage is not None, "the terminal event reported no usage"


@pytest.mark.smoke
def test_openai_client_reaches_live_embeddings(openai_client: openai.OpenAI) -> None:
    """A real embedding answer decodes with its vector and usage intact."""
    embedding = operation_model("embeddings")
    answer = openai_client.embeddings.create(model=f"openai/{embedding}", input="ready")
    assert answer.data, "embeddings returned no vector"
    assert len(answer.data[0].embedding) > 1, "the vector did not survive the gateway"
    assert answer.usage.prompt_tokens > 0, "embeddings reported no usage"


@pytest.mark.smoke
def test_openai_client_round_trips_live_audio(openai_client: openai.OpenAI) -> None:
    """Speech bytes come back verbatim and transcribe through the multipart route.

    The generated audio is fed straight back as the transcription upload, so
    the two Audio directions prove each other without a fixture file.
    """
    spoken = openai_client.audio.speech.create(
        model=f"openai/{operation_model('speech')}",
        voice="alloy",
        input="ready",
        response_format="mp3",
    )
    audio = spoken.read()
    assert len(audio) > 1024, "speech returned no audio payload"

    transcript = openai_client.audio.transcriptions.create(
        model=f"openai/{operation_model('transcription')}",
        file=("speech.mp3", audio, "audio/mpeg"),
    )
    assert transcript.text.strip(), "transcription returned no text"


@pytest.mark.smoke
def test_openai_client_reaches_live_images(openai_client: openai.OpenAI) -> None:
    """A real image answer decodes and its bytes survive the gateway."""
    answer = openai_client.images.generate(
        model=f"openai/{operation_model('images')}",
        prompt="a plain blue square on a white background",
        n=1,
        size="1024x1024",
    )
    assert answer.data, "images returned no item"
    assert answer.data[0].b64_json, "the image bytes did not survive the gateway"


@pytest.mark.smoke
def test_openai_client_runs_a_live_batch_lifecycle(openai_client: openai.OpenAI) -> None:
    """Upload, create, retrieve, and cancel a real batch under Wyrd ids.

    The batch is cancelled rather than awaited: completion takes up to a day,
    and every typed schema on the submission path is already exercised by the
    four exchanges. The input line names the caller's ``provider/model``
    projection, which the gateway rewrites to the provider's own model before
    the file is uploaded.
    """
    line = json.dumps(
        {
            "custom_id": "smoke-1",
            "method": "POST",
            "url": "/v1/chat/completions",
            "body": {
                "model": f"openai/{model('openai')}",
                "messages": MESSAGES,
                "max_completion_tokens": 16,
            },
        }
    )
    upload = openai_client.files.create(
        file=("batch.jsonl", f"{line}\n".encode(), "application/jsonl"),
        purpose="batch",
    )
    assert upload.id.startswith("file-"), f"unexpected file id: {upload.id}"

    batch = openai_client.batches.create(
        input_file_id=upload.id,
        endpoint="/v1/chat/completions",
        completion_window="24h",
    )
    assert batch.id.startswith("batch_"), f"unexpected batch id: {batch.id}"
    assert openai_client.batches.retrieve(batch.id).id == batch.id
    assert openai_client.batches.cancel(batch.id).status in {"cancelling", "cancelled"}
