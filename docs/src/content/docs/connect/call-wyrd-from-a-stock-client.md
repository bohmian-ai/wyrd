---
title: Gateway from any client
description: Run inference through a named Wyrd gateway deployment from the OpenAI, Anthropic, and Google GenAI SDKs, the OpenAI Agents SDK, and Pydantic AI.
pillar: wyrd
section: Connect
order: 2
draft: true
---

You don't need a Wyrd adapter to call the gateway from a provider SDK. Point the SDK's base URL at your Wyrd server, give it Wyrd's HTTP transport, and address the model as `<deployment>/<model>`. Each example on this page runs one real inference and prints the reply.

The transport is `wyrd.http_auth` in Python, an `httpx.Auth` that also works as an `httpx2.Auth`, sync or async, and `wyrd.httpAuth` in TypeScript, a `fetch`. On every request it removes `Authorization`, `x-api-key`, and `x-goog-api-key`, then sends your current Wyrd access token as `X-Wyrd-Access-Token: Bearer <token>`. Provider SDKs insist on some key, so pass the placeholder `"wyrd"`. It never leaves your process. Tokens are fetched per request, so a long-running process keeps working after any one token expires.

## Prerequisites

- A deployment for each model you call. The examples use `openai-prod`, `claude-prod`, and `gemini-team` from [Set up the LLM gateway](/connect/set-up-the-llm-gateway/). Swap in your own deployment names.
- The capture policy from [step 5 of the setup](/connect/set-up-the-llm-gateway/#set-up-one-provider). Calls run without it, but none are recorded, so [Check that the call was recorded](#check-that-the-call-was-recorded) finds nothing.
- A Wyrd credential the client can find: the config `wyrd init` saves, `WYRD_API_KEY`, `WYRD_ACCESS_TOKEN`, or a saved `wyrd auth login`.
- Python: `uv add 'wyrd-ai[gateway]'`, plus the SDK or framework you use. These examples were checked with `openai` 3.14, `anthropic` 1.6, `google-genai` 2.30, `openai-agents` 0.23, and `pydantic-ai-slim` 2.56.
- TypeScript: `pnpm add wyrd-ai`, plus `openai` or `@anthropic-ai/sdk`.

## OpenAI

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import openai
from wyrd import WyrdClient

wyrd = WyrdClient()
client = openai.OpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=openai.DefaultHttpxClient(auth=wyrd.http_auth),
)

completion = client.chat.completions.create(
    model="openai-prod/gpt-4o",
    messages=[{"role": "user", "content": "Say hi."}],
)
print(completion.choices[0].message.content)
```

For `openai.AsyncOpenAI`, use `openai.DefaultAsyncHttpxClient(auth=wyrd.http_auth)`.

</LangTab>
<LangTab lang="typescript">

```typescript
import OpenAI from "openai";
import { WyrdClient } from "wyrd-ai";

const wyrd = WyrdClient.connect();
const client = new OpenAI({ baseURL: `${wyrd.serverUrl}/v1`, apiKey: "wyrd", fetch: wyrd.httpAuth });

const completion = await client.chat.completions.create({
  model: "openai-prod/gpt-4o",
  messages: [{ role: "user", content: "Say hi." }],
});
console.log(completion.choices[0]?.message.content);
```

</LangTab>
</LangTabs>

## Anthropic

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import anthropic
from wyrd import WyrdClient

wyrd = WyrdClient()
client = anthropic.Anthropic(
    base_url=wyrd.server_url,
    api_key="wyrd",
    http_client=anthropic.DefaultHttpxClient(auth=wyrd.http_auth),
)

message = client.messages.create(
    model="claude-prod/claude-sonnet-4-5",
    max_tokens=64,
    messages=[{"role": "user", "content": "Say hi."}],
)
print(message.content[0].text)
```

</LangTab>
<LangTab lang="typescript">

```typescript
import Anthropic from "@anthropic-ai/sdk";
import { WyrdClient } from "wyrd-ai";

const wyrd = WyrdClient.connect();
const client = new Anthropic({ baseURL: wyrd.serverUrl, apiKey: "wyrd", fetch: wyrd.httpAuth });

const message = await client.messages.create({
  model: "claude-prod/claude-sonnet-4-5",
  max_tokens: 64,
  messages: [{ role: "user", content: "Say hi." }],
});
console.log(message.content);
```

</LangTab>
</LangTabs>

## Google GenAI

Python only. The deployment name goes into the request path, so Gemini sees the same `gemini-2.5-flash` it would without Wyrd.

```python
import httpx
from google import genai
from google.genai import types
from wyrd import WyrdClient

wyrd = WyrdClient()
client = genai.Client(
    api_key="wyrd",
    http_options=types.HttpOptions(
        base_url=wyrd.server_url, httpx_client=httpx.Client(auth=wyrd.http_auth)
    ),
)

answer = client.models.generate_content(model="gemini-team/gemini-2.5-flash", contents="Say hi.")
print(answer.text)
```

## OpenAI Agents SDK

Python. The Agents SDK takes an `AsyncOpenAI` client, so give it the one from [OpenAI](#openai) built with `DefaultAsyncHttpxClient`. Turn off the SDK's own tracing export unless you also have an OpenAI platform key; otherwise it tries to upload traces to OpenAI.

```python
import openai
from agents import Agent, OpenAIChatCompletionsModel, Runner, set_tracing_disabled
from wyrd import WyrdClient

set_tracing_disabled(True)
wyrd = WyrdClient()
client = openai.AsyncOpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=openai.DefaultAsyncHttpxClient(auth=wyrd.http_auth),
)
agent = Agent(
    name="assistant",
    model=OpenAIChatCompletionsModel(model="openai-prod/gpt-4o", openai_client=client),
)

result = Runner.run_sync(agent, "Say hi.")
print(result.final_output)
```

## Pydantic AI

Python. Pydantic AI's OpenAI provider takes an `httpx2.AsyncClient`, and `wyrd.http_auth` plugs straight into it.

```python
import httpx2
from pydantic_ai import Agent
from pydantic_ai.models.openai import OpenAIChatModel
from pydantic_ai.providers.openai import OpenAIProvider
from wyrd import WyrdClient

wyrd = WyrdClient()
provider = OpenAIProvider(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=httpx2.AsyncClient(auth=wyrd.http_auth),
)
agent = Agent(OpenAIChatModel("openai-prod/gpt-4o", provider=provider))

result = agent.run_sync("Say hi.")
print(result.output)
```

## Check that the call was recorded

With the capture policy applied, each call lands in `bifrost.gateway.calls`, attributed to the Wyrd principal that made it:

```python
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT requested_model, outcome, started_at "
    "FROM bifrost.gateway.calls ORDER BY started_at DESC LIMIT 5"
).to_arrow().to_pylist()
print(rows)
```

Rows appear a few seconds after the call. An empty result right after a call usually means it isn't published yet, so query again. If it stays empty, check that the capture policy is applied with `wyrd gateway capture-policy get`.

## When the gateway refuses

Refusals come back through the SDK's normal error types, with Wyrd's stable code in `code`. Nothing is sent upstream.

```python
try:
    client.chat.completions.create(
        model="undeployed/gpt-4o",
        messages=[{"role": "user", "content": "Say hi."}],
    )
except openai.APIStatusError as refused:
    print(refused.status_code, refused.code)
    print(refused.response.headers["wyrd-request-id"])
```

This prints `404 WYRD_GATEWAY_404_MODEL_UNAVAILABLE` and the request ID. The common codes:

| Code | Cause | Fix |
| --- | --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | No deployment by that name serves the operation | Check the part before `/` against `wyrd gateway deployment list`. |
| `WYRD_PERMISSION_403_DENIED_RBAC` | Your principal isn't allowed that model | Ask a tenant admin for access, or call a deployment you're granted. |
| `WYRD_AUTH_400_BAD_TOKEN_FORMAT` | The request carried a provider key instead of a Wyrd credential | Pass `http_client` with `wyrd.http_auth` (or `fetch: wyrd.httpAuth`). Without it the SDK sends `"wyrd"` as a key. |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | The transport found no Wyrd credential | Start `wyrd init`, or set `WYRD_API_KEY` or `WYRD_ACCESS_TOKEN`. |

[Set up the LLM gateway](/connect/set-up-the-llm-gateway/#when-a-call-is-refused) covers credential and deployment failures.

## Without a transport hook

If a client has no transport hook but accepts a callable API key, pass the access token as the key, for example `apiKey: () => wyrd.accessToken()` in the TypeScript OpenAI SDK. For shells and `apiKeyHelper`-style tools, `wyrd auth access-token` prints the current token followed by a newline.

## Fallback: a static Wyrd API key

Some tools take only a base URL and a static key: environment-only setups, curl, or a language without a Wyrd SDK. The gateway's inference routes (OpenAI `/v1`, Anthropic Messages, and Gemini `generateContent`) also accept a Wyrd API key wherever they accept a token:

```bash
export OPENAI_BASE_URL=$WYRD_SERVER_URL/v1
export OPENAI_API_KEY=$WYRD_API_KEY
```

Anthropic clients send the key as `x-api-key` with `ANTHROPIC_BASE_URL=$WYRD_SERVER_URL`. Gemini clients send it as `x-goog-api-key` against `$WYRD_SERVER_URL`. The server exchanges the key for the key's own principal, so every call is still authorized as that principal, and only the tenant's provider credential travels upstream.

Prefer the access-token paths when you can:

- The API key is long-lived, and this fallback sends it on every request. Any proxy, log, or process that sees the traffic sees the key. An access token expires in minutes.
- The server reuses each key's exchange for up to 60 seconds, so a revoked key keeps working for up to 60 seconds after its last exchange.
- Only the gateway's inference routes take a key this way. `/mcp`, the `/v1` management API, and Bifrost still require an access token.
