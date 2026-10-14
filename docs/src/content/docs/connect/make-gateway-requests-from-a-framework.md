---
title: Make gateway requests from a framework
description: Point a stock OpenAI, Anthropic, or Google client at the Wyrd gateway, tie each call to a run and Card, and recover from gateway errors.
pillar: wyrd
section: Connect
order: 5
draft: true
status: runnable
---

Point your existing model client at your Wyrd server instead of the provider, and authenticate with your Wyrd credential. The gateway checks the caller's permission, calls the provider with the key you stored, and returns the provider's answer in the same shape your client expects. Your code keeps its usual SDK; only the base URL and the credential change.

One unified route serves every deployed provider in OpenAI's format. Native routes each speak one provider's own protocol:

| Route | Use it for | `model` names |
| --- | --- | --- |
| `POST /v1/chat/completions`, `POST /v1/responses` | Any deployed model, from an OpenAI-compatible client | `<provider>/<model>`, such as `openai/gpt-4o` or `anthropic/claude-sonnet-5` |
| `POST /v1/messages` | Anthropic's own client | An Anthropic model, such as `claude-sonnet-5` |
| `POST /v1beta/models/<model>:generateContent` | Google's `google-genai` client, Gemini Developer API models | A Gemini model, such as `gemini-2.5-flash` |
| `POST /v1/systemone` | Jev's question-and-answer protocol | A Jev model, such as `jev-latest` |

The examples continue the `support-desk` service and call `gpt-4o` from [Set up the LLM gateway](/connect/set-up-the-llm-gateway/).

## Prerequisites

- A deployment for each model you call, with the capability you use declared, as in [Set up the LLM gateway](/connect/set-up-the-llm-gateway/#2-deploy-the-model).
- A Wyrd credential holding `gateway:invoke` for that model. The `workload`, `editor`, and `admin` roles have it; `viewer` does not. The examples resolve the credential the same way every Wyrd client does, so a saved login, `credentials.toml`, or `WYRD_API_KEY` works with no extra code.
- The Wyrd SDK and your model client:

| Language | Install |
| --- | --- |
| Python | `pip install "wyrd-ai[gateway]" openai` |
| TypeScript | `npm install wyrd-ai openai` |
| Rust | The `wyrd-ai`, `reqwest` (with its `json` feature), `serde_json`, and `tokio` crates |

## 1. Call the unified route

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import openai
from wyrd.client import WyrdClient
from wyrd.gateway import GatewayAuth

wyrd = WyrdClient()
gateway = openai.OpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",  # placeholder; GatewayAuth sends the real credential
    http_client=openai.DefaultHttpxClient(auth=GatewayAuth(wyrd)),
)

completion = gateway.chat.completions.create(
    model="openai/gpt-4o",
    max_completion_tokens=256,
    messages=[{"role": "user", "content": "Where is my order 4417?"}],
)
print(completion.choices[0].message.content)
```

</LangTab>
<LangTab lang="rust">

```rust
use serde_json::{Value, json};
use wyrd_ai::WyrdClient;

let wyrd = WyrdClient::from_global()?;
let token = wyrd.access_token().await?;
let completion: Value = reqwest::Client::new()
    .post(format!("{}/v1/chat/completions", wyrd.server_url()))
    .bearer_auth(token.expose())
    .json(&json!({
        "model": "openai/gpt-4o",
        "max_completion_tokens": 256,
        "messages": [{ "role": "user", "content": "Where is my order 4417?" }],
    }))
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
println!("{}", completion["choices"][0]["message"]["content"]);
```

</LangTab>
<LangTab lang="typescript">

```typescript
import OpenAI from "openai";
import { WyrdClient, gatewayFetch } from "wyrd-ai";

const wyrd = WyrdClient.connect();
const gateway = new OpenAI({
  baseURL: `${wyrd.serverUrl}/v1`,
  apiKey: "wyrd", // placeholder; gatewayFetch sends the real credential
  fetch: gatewayFetch(wyrd),
});

const completion = await gateway.chat.completions.create({
  model: "openai/gpt-4o",
  max_completion_tokens: 256,
  messages: [{ role: "user", content: "Where is my order 4417?" }],
});
console.log(completion.choices[0]?.message.content);
```

</LangTab>
</LangTabs>

The answer is OpenAI's own Chat Completions response, and `stream: true` returns OpenAI's server-sent events. Change only `model` to reach another deployed provider through the same client: `anthropic/claude-sonnet-5`, `gemini/gemini-2.5-flash`, `vertex/gemini-2.5-pro`, or the name of an `openai_compatible` provider. The gateway translates the request for the provider's protocol and the answer back.

`GatewayAuth` and `gatewayFetch` ask the Wyrd client for its access token on every request, so a long-running process keeps working after any one token expires. Rust has no OpenAI client in its dependency tree; it sends the request with `reqwest` and the token from `access_token()`. Call `access_token()` again for each request rather than keeping the value; the client renews it before expiry.

`POST /v1/responses` works the same way for deployments that declare `responses`. Only `openai` and `openai_compatible` deployments can:

```python
response = gateway.responses.create(
    model="openai/gpt-4o",
    max_output_tokens=256,
    input="Where is my order 4417?",
)
print(response.output_text)
```

To check what you can call, list models. `GET /v1/models` returns only the deployed models your credential may invoke:

```python
print([model.id for model in gateway.models.list()])
```

<Aside type="note" title="OpenAI Agents SDK">The Agents SDK calls models through a stock OpenAI client, so the client above is the one to give it. Wyrd's test suite does not run the Agents SDK against the gateway, so that combination is not verified here.</Aside>

## 2. Tie the call to a run and a Card

Two optional request headers attribute a call. The gateway reads them and never forwards them to the provider:

| Header | Value | Written to |
| --- | --- | --- |
| `wyrd-run-id` | 1 to 128 bytes of visible ASCII, normally the current run's ID | `vala.gateway.calls.run_id` |
| `wyrd-card-uid` | The UID of the Card that made the call, such as your Agent | `vala.gateway.calls.card_uid` |

Each header works alone; send either, both, or neither. With the run ID set, the call joins the eval observations and dataset rows of the same run in one query, as described in [Runs, observations, and retained evidence](/understand/runs-observations-and-evidence/#query-the-evidence).

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
with state.run("agent") as run:
    completion = gateway.chat.completions.create(
        model="openai/gpt-4o",
        max_completion_tokens=256,
        messages=[{"role": "user", "content": question}],
        extra_headers={"wyrd-run-id": run.run_id},
    )
```

</LangTab>
<LangTab lang="rust">

```rust
let run = state.run_for_card("agent")?;
let completion: Value = reqwest::Client::new()
    .post(format!("{}/v1/chat/completions", wyrd.server_url()))
    .bearer_auth(wyrd.access_token().await?.expose())
    .header("wyrd-run-id", run.run_id().to_string())
    .json(&json!({
        "model": "openai/gpt-4o",
        "max_completion_tokens": 256,
        "messages": [{ "role": "user", "content": question }],
    }))
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
```

</LangTab>
<LangTab lang="typescript">

```typescript
const run = state.run("agent");
const completion = await gateway.chat.completions.create(
  {
    model: "openai/gpt-4o",
    max_completion_tokens: 256,
    messages: [{ role: "user", content: question }],
  },
  { headers: { "wyrd-run-id": run.runId } },
);
```

</LangTab>
</LangTabs>

`state` is the loaded service from [Record a run and observations](/build-and-verify/observe/record-a-run-and-observations/). If your Agent's Prompt runs through `run.invoke`, you do not need these headers: the SDK sends the run and the Agent's Card for you.

The server checks `wyrd-card-uid` against the caller. A key issued to one Card can attribute calls only to Cards in its scope. A caller bound to no Card, such as a person's login, can name any registered Card of its tenant that accepts observations. Anything else is refused before the provider is called.

## 3. Use a provider's own client

Use a native route when your code already uses a provider's SDK and you want to keep that SDK's request and response types. The provider is fixed by the route, so `model` is the provider's own model ID with no `<provider>/` prefix, and only that provider's deployments serve it.

These clients put the token in their own API-key slot. Pass the Wyrd access token there, and do not also attach `GatewayAuth`: a request that carries two credentials is refused with `WYRD_AUTH_400_BAD_TOKEN_FORMAT`. The token in the API-key slot is fixed when you build the client, so build a new client when it expires. Wyrd tests these two clients in Python only.

```python
import anthropic
from google import genai
from google.genai import types
from wyrd.client import WyrdClient

wyrd = WyrdClient()

claude = anthropic.Anthropic(base_url=wyrd.server_url, api_key=wyrd.access_token())
message = claude.messages.create(
    model="claude-sonnet-5",
    max_tokens=256,
    messages=[{"role": "user", "content": "Where is my order 4417?"}],
)

gemini = genai.Client(
    api_key=wyrd.access_token(),
    http_options=types.HttpOptions(base_url=wyrd.server_url),
)
answer = gemini.models.generate_content(
    model="gemini-2.5-flash",
    contents="Where is my order 4417?",
)
```

Streaming works on both: `claude.messages.stream(...)` and `gemini.models.generate_content_stream(...)`. The Gemini route serves `gemini` deployments only. A `vertex` deployment is reachable through the unified route, as `vertex/<model>`, and not through `google-genai`.

### Ask Jev typed questions

Jev answers keyed questions about a piece of text, which suits judging the support desk's replies. Each question has a `type` of `noul`, `choice`, or `score`. Send the request to `POST /v1/systemone` with a bearer token:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```python
import httpx
from wyrd.client import WyrdClient
from wyrd.gateway import GatewayAuth

wyrd = WyrdClient()
reply = httpx.post(
    f"{wyrd.server_url}/v1/systemone",
    auth=GatewayAuth(wyrd),
    timeout=60,
    json={
        "model": "jev-latest",
        "state": "Your refund for order 4417 is on its way.",
        "questions": {
            "helpful": {"type": "noul", "instructions": "Does the reply help the customer?"},
            "tone": {
                "type": "choice",
                "instructions": "What is the tone of the reply?",
                "criteria": {"polite": "courteous", "rude": "curt"},
            },
        },
    },
)
reply.raise_for_status()
print(reply.json()["answers"])
```

</LangTab>
<LangTab lang="rust">

```rust
let wyrd = WyrdClient::from_global()?;
let reply: Value = reqwest::Client::new()
    .post(format!("{}/v1/systemone", wyrd.server_url()))
    .bearer_auth(wyrd.access_token().await?.expose())
    .json(&json!({
        "model": "jev-latest",
        "state": "Your refund for order 4417 is on its way.",
        "questions": {
            "helpful": { "type": "noul", "instructions": "Does the reply help the customer?" },
            "tone": {
                "type": "choice",
                "instructions": "What is the tone of the reply?",
                "criteria": { "polite": "courteous", "rude": "curt" },
            },
        },
    }))
    .send()
    .await?
    .error_for_status()?
    .json()
    .await?;
println!("{}", reply["answers"]);
```

</LangTab>
<LangTab lang="typescript">

```typescript
const wyrd = WyrdClient.connect();
const response = await fetch(`${wyrd.serverUrl}/v1/systemone`, {
  method: "POST",
  headers: {
    authorization: `Bearer ${await wyrd.accessToken()}`,
    "content-type": "application/json",
  },
  body: JSON.stringify({
    model: "jev-latest",
    state: "Your refund for order 4417 is on its way.",
    questions: {
      helpful: { type: "noul", instructions: "Does the reply help the customer?" },
      tone: {
        type: "choice",
        instructions: "What is the tone of the reply?",
        criteria: { polite: "courteous", rude: "curt" },
      },
    },
  }),
});
console.log((await response.json()).answers);
```

</LangTab>
</LangTabs>

The answers look like this; the numbers vary:

```json
{
  "helpful": { "type": "noul", "noul": 0.93 },
  "tone": {
    "type": "choice",
    "choice": "polite",
    "confidence": 0.8,
    "probabilities": { "polite": 0.8, "rude": 0.2 }
  }
}
```

The full answer also carries `model`, the Jev version that answered, and `usage`. The Jev deployment must declare `system_one`. The request is checked before Jev sees it: an unknown member, an unknown question type, or a `state` that is not a string is refused with `WYRD_GATEWAY_400_INVALID_REQUEST`. Jev never streams.

A Jev deployment that also declares `chat_completions` answers on the unified route. Send `model: "jev/jev-latest"` and make the last user message the JSON text of `{"state": ..., "questions": ...}`; the message content of the answer is Jev's answer as JSON. An ordinary chat message to a Jev model is refused with `WYRD_GATEWAY_400_INVALID_REQUEST` and never reaches Jev.

## What can go wrong

Every refusal arrives in the error format of the route's protocol, carrying a stable Wyrd code:

| Route | Where the code is |
| --- | --- |
| Unified routes and `/v1/systemone` | `error.code`; `error.param` names the bad field |
| `/v1/messages` | `error.code` |
| `/v1beta/models/...` | `error.details[0].reason` |

With the OpenAI clients, that is `.code` on the raised exception in Python and `error.code` on the rejected `APIError` in TypeScript. When the provider itself refuses a request, every route except `/v1/systemone` passes on the provider's status and error body instead.

| Code | Cause | Fix |
| --- | --- | --- |
| `WYRD_AUTH_401_UNAUTHENTICATED` | No credential on the request | Pass the access token, or attach `GatewayAuth` or `gatewayFetch` |
| `WYRD_AUTH_400_BAD_TOKEN_FORMAT` | Two credentials on one request, such as `GatewayAuth` plus a native client's API key, or a token in the URL | Send exactly one credential, in a header |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The caller lacks `gateway:invoke` for this model | Grant a role that has it; see [Grant and inspect permissions](/operate/grant-and-inspect-permissions/) |
| `WYRD_GATEWAY_400_INVALID_REQUEST` | The field in `param` is wrong: `model` without `<provider>/`, a repeated or malformed `wyrd-run-id` or `wyrd-card-uid`, a feature no deployment of the model supports, or a malformed Jev request | Fix that field. Nothing reached the provider |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` | `wyrd-card-uid` names a Card outside the caller's scope | Send the caller's own Card UID, or leave the header out |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | No deployment serves this model for this operation | Check `GET /v1/models`, the model spelling, and the deployment's `capabilities` |
| `WYRD_GATEWAY_422_COST_UNBOUNDED` | A budget applies, or unpriced calls are rejected, and the request has no output limit. Jev requests never have one | Set `max_completion_tokens` (Chat Completions), `max_output_tokens` (Responses), `max_tokens` (Anthropic), or `generationConfig.maxOutputTokens` (Gemini), and ask an administrator to add pricing for the model |
| `WYRD_GATEWAY_429_LIMIT_EXCEEDED` | A rate or concurrency limit is used up | Wait `details.retry_after_seconds` and retry |
| `WYRD_GATEWAY_429_BUDGET_EXCEEDED` | The budget for this period is spent | Wait for the next period, or ask for a larger budget |
| `WYRD_GATEWAY_429_UPSTREAM_THROTTLED` | The provider is rate limiting | Retry with backoff |
| `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE` | No provider attempt completed: the provider is down, rejected the stored key, or the deployment is misconfigured | Retry with backoff. If it persists, an administrator checks the deployment and its credential |
| `WYRD_GATEWAY_504_DEADLINE_EXCEEDED` | The call took longer than 5 minutes | Send a smaller request |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE` | The server is shutting down | Retry against another replica or after restart |

A refused call is never sent to the provider, except where the table says the provider refused it. Gateway behavior for every route, header, and status is in the server's live OpenAPI document at `GET /openapi.json`.
