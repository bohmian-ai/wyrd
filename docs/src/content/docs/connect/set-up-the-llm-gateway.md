---
title: LLM gateway
description: Store a provider key on your Wyrd server, create a deployment for it, and call the model through Wyrd with your Wyrd credential.
pillar: wyrd
section: Connect
order: 1
status: runnable
---

Call OpenAI through your Wyrd server. Your code signs in to Wyrd, Wyrd adds the provider key and records the call, and the key never leaves the server.

You need a running server from [Set up Wyrd](/get-started/setup/) and an OpenAI key in `OPENAI_API_KEY`.

<Aside type="note" title="Already offered at setup">

If you accepted the gateway offer during `wyrd init`, the `openai` deployment exists. Skip to step 3.

</Aside>

## 1. Store the key

```bash
wyrd gateway credential put --file - <<JSON
{"name": "openai-key", "provider": "openai", "source": {"managed_secret": {"secret": "$OPENAI_API_KEY"}}}
JSON
```

The key goes to the server over stdin, so it never lands in a file or your shell history. The server encrypts it, and every read of it is redacted.

## 2. Create the deployment

A deployment names a provider and the key to call it with. Your code addresses models through it. The name is yours, so one provider can have several deployments, such as one per team or caller, each with its own key:

```bash
wyrd gateway deployment put --file - <<'JSON'
{"name": "openai", "adapter": "openai", "auth": {"bearer": {"credential": "openai-key"}}}
JSON
```

## 3. Turn on recording

A new tenant records no calls. Record each call's caller, model, outcome, and token usage, without the prompt or reply:

```bash
wyrd gateway capture-policy put --file - <<'JSON'
{"mode": "metadata", "payload_fields": []}
JSON
```

Calls made before this step are never recorded.

## 4. Make a request

```bash
uv add "wyrd-ai[gateway]==0.3.0" openai
```

```python title="call.py"
from openai import DefaultHttpxClient, OpenAI
from wyrd import WyrdClient

wyrd = WyrdClient()  # finds the server and credential wyrd init saved
client = OpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",  # placeholder; http_auth replaces it with your Wyrd token
    http_client=DefaultHttpxClient(auth=wyrd.http_auth),
)

reply = client.chat.completions.create(
    model="openai/gpt-4o",  # <deployment>/<model>
    messages=[{"role": "user", "content": "Say hi."}],
)
print(reply.choices[0].message.content)
```

```bash
uv run python call.py
```

It prints the model's reply. OpenAI received `gpt-4o` with your stored key and never saw your Wyrd token.

## 5. Check the call was recorded

```python title="calls.py"
from wyrd.bifrost import Bifrost

rows = Bifrost().sql(
    "SELECT requested_model, outcome FROM vala.gateway.calls ORDER BY started_at DESC LIMIT 1"
).to_arrow().to_pylist()
print(rows)
```

```bash
uv run python calls.py
```

```text
[{'requested_model': {'deployment': 'openai', 'model': 'gpt-4o'}, 'outcome': 'succeeded'}]
```

Rows appear a few seconds after the call. If the result is empty, query again.

## Add another provider

Repeat steps 1 and 2 with that provider's key and authentication:

| Provider | `provider` and `adapter` | `auth` |
| --- | --- | --- |
| OpenAI | `openai` | `{"bearer": {"credential": "openai-key"}}` |
| Anthropic | `anthropic` | `{"api_key_header": {"header": "x-api-key", "credential": "anthropic-key"}}` |
| Gemini | `gemini` | `{"api_key_header": {"header": "x-goog-api-key", "credential": "gemini-key"}}` |

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | Check the deployment name with `wyrd gateway deployment list` |
| `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE` | Check the key with `wyrd gateway credential get openai-key`, and that the provider is reachable |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Run `wyrd init` in this directory, or set `WYRD_API_KEY` |
| The query in step 5 stays empty | Run `wyrd gateway capture-policy get`; if it says `disabled`, repeat step 3 |

## Next

Judge answers with a model in [Write a Verifier](/build-and-verify/verify/author-a-verifier/#5-add-a-model-judge).
