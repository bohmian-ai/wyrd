---
title: Set up the LLM gateway
description: Store a provider key with the Wyrd server, name a deployment for it, and call the model through Wyrd with your Wyrd credential.
pillar: wyrd
section: Connect
order: 1
draft: true
status: runnable
---

The gateway lets your code call OpenAI, Anthropic, Gemini, and Jev models through your Wyrd server. Your code authenticates to Wyrd with its normal Wyrd credential. Wyrd then authorizes the call, adds the tenant's provider key, sends the request upstream, and records it. The provider key stays on the server, so it never ends up in your code, your Cards, or your shell history.

By the end of this page you will have one Anthropic deployment named `claude-prod` and a request that reached it.

## How a request finds its provider

A gateway model is addressed as `<deployment>/<model>`:

```text
claude-prod/claude-sonnet-4-5
└─────────┘ └───────────────┘
deployment   upstream model, sent to the provider unchanged
```

These names mean different things:

| Name | Example | Chosen by | Decides |
| --- | --- | --- | --- |
| Provider adapter | `anthropic` | The deployment's `adapter` field | Wire format, upstream URL, and how the provider key is sent |
| Deployment | `claude-prod` | You, when you create it | Which adapter and provider credential serve the call |
| Upstream model | `claude-sonnet-4-5` | The caller | Which model the provider runs |

Wyrd never infers the provider from the deployment name. A deployment called `vertex` can use the Gemini adapter, and `claude-prod` would work just as well if you called it `support`. Pick names that describe your use, not the vendor.

A Prompt Card's `provider` is a third, separate thing. It says which request format the Prompt was written in. When an Agent runs through the gateway, its Prompt's format has to match the adapter of the deployment it asks for. An OpenAI-format Prompt sent to `claude-prod` is refused with `WYRD_GATEWAY_400_INVALID_REQUEST`.

## Prerequisites

- The `wyrd` CLI and PostgreSQL, as in [Set up Wyrd](/get-started/setup/).
- An Anthropic API key in the `ANTHROPIC_API_KEY` environment variable of the shell you run step 2 in.
- For the first request in step 6: Python with `pip install 'wyrd-ai[gateway]'`.

## Set up one provider

1. Start a local server and leave it running in its own terminal:

   ```bash
   wyrd init --tenant acme
   ```

   This saves the server URL and an admin credential for the `acme` tenant. Every later command reads them; you export nothing.

Every gateway resource is a JSON document piped to `wyrd gateway <resource> put --file -`. Each command prints what it stored, and running it again changes nothing.

2. Store the Anthropic key as a credential named `anthropic-key`. The server encrypts it, and every read of it is redacted:

   ```bash
   wyrd gateway credential put --file - <<JSON
   {"name": "anthropic-key", "provider": "anthropic", "source": {"managed_secret": {"secret": "$ANTHROPIC_API_KEY"}}}
   JSON
   ```

   The key goes to the server over stdin. It never lands in a file or your shell history.

3. Create the `claude-prod` deployment, which sends requests to Anthropic with that credential:

   ```bash
   wyrd gateway deployment put --file - <<'JSON'
   {"name": "claude-prod", "adapter": "anthropic", "auth": {"api_key_header": {"header": "x-api-key", "credential": "anthropic-key"}}}
   JSON
   ```

   The `auth` block says how the adapter sends the key: Anthropic and Gemini take it in a header, OpenAI and Jev as a bearer token. [Add more providers](#add-more-providers) has each one.

4. Turn on call recording. A new tenant's capture policy is `disabled`, so it records nothing, and Wyrd doesn't backfill: a call made before this step is never recorded.

   ```bash
   wyrd gateway capture-policy put --file - <<'JSON'
   {"mode": "metadata", "payload_fields": []}
   JSON
   ```

   `metadata` records each call's caller, deployment, model, outcome, and token usage, but never the prompt or the reply, and its `payload_fields` must stay empty. We recommend starting here. `payload` mode also stores the redacted request or response, which usually carries user data; turn it on only when you need the content.

5. Check what you stored:

   ```bash
   wyrd gateway deployment get claude-prod
   wyrd gateway credential get anthropic-key
   ```

   The credential prints with its secret redacted.

6. Make the first request. The Anthropic SDK sends it to Wyrd instead of Anthropic, and `wyrd.http_auth` swaps the placeholder `api_key` for your Wyrd access token:

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

   The script prints the model's reply. Anthropic received `claude-sonnet-4-5` with the stored key in `x-api-key`. It never saw your Wyrd token.

7. Confirm Wyrd recorded the call:

   ```python
   from wyrd.bifrost import Bifrost

   rows = Bifrost().sql(
       "SELECT requested_model, outcome, started_at "
       "FROM vala.gateway.calls ORDER BY started_at DESC LIMIT 5"
   ).to_arrow().to_pylist()
   print(rows)
   ```

   The newest row has `requested_model` `{'deployment': 'claude-prod', 'model': 'claude-sonnet-4-5'}` and `outcome` `succeeded`. Rows show up a few seconds after the call. If the result is empty right away, wait and query again. If it stays empty, run `wyrd gateway capture-policy get`: a `disabled` mode means step 4 didn't apply, and calls made before it won't appear.

Calling from another framework or language? [Make gateway requests from a framework](/connect/call-wyrd-from-a-stock-client/) has the OpenAI, Anthropic, Google GenAI, OpenAI Agents SDK, and Pydantic AI versions of step 6.

## Add more providers

`wyrd gateway deployment put` also takes one document holding credentials and deployments together. This one gives each built-in provider its own named deployment, and each credential names the environment variable holding its key instead of the key:

<CodeFromFile file="examples/gateway-setup/gateway.json" lang="json" title="gateway.json" />

Export the four variables and apply it:

```bash
wyrd gateway deployment put --file gateway.json
```

The CLI reads each `local_env` variable and sends its value once, as an encrypted-at-rest managed secret; the variable name is never stored. The server applies every credential and deployment in one transaction: if any is refused, for example a deployment naming a credential that doesn't exist, nothing is applied and the command exits non-zero. Applying it again converges, and deployments the document doesn't name are left alone. `wyrd init --gateway-file gateway.json` applies the same document at startup, and a first interactive `wyrd init` builds one like it from the keys it finds.

Credentials stored separately, as in step 2, work too: a document without `credentials`, a single deployment, or a JSON list of deployments refers to them by name. A list applies in order; if one entry fails, the entries after it are not attempted.

Each adapter serves its provider's own request format on its own route:

| Adapter | Route your client calls | Where the model goes | Supports |
| --- | --- | --- | --- |
| `openai` | `POST /v1/chat/completions`, `POST /v1/responses` | `model: "openai-prod/gpt-4o"` | Chat and responses, streaming |
| `anthropic` | `POST /v1/messages` | `model: "claude-prod/claude-sonnet-4-5"` | Messages, streaming |
| `gemini` | `POST /v1beta/models/gemini-team/gemini-2.5-flash:generateContent` | In the path | `generateContent`, `streamGenerateContent` |
| `jev` | `POST /v1/systemone` | `model: "judge-pool/jev-latest"` | System One evaluation requests only. Jev deployments don't serve chat, tools, or streaming. |

The `examples/gateway-setup` directory in the Wyrd repository holds this `gateway.json` plus the commands that apply it. The full request and response schemas are in the server's OpenAPI document at `GET /openapi.json`.

## When a call is refused

Every refusal is a JSON problem with a stable `code`. A refused call is never sent upstream.

| Code | What happened | Fix |
| --- | --- | --- |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | No deployment with that name serves this operation | Check the name with `wyrd gateway deployment list`. Check that the deployment's `capabilities` include the operation, for example `chat_completions`. |
| `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE` | The deployment's credential is revoked or missing, or the provider couldn't be reached | Run `wyrd gateway credential get anthropic-key`. A revoked name can't be reused: put the key under a new name, point the deployment's `auth.credential` at it, and apply the deployment again. |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The caller's Wyrd credential isn't allowed this model or action. A key scoped to `openai-prod/gpt-4o` can't call `claude-prod`, for example. | Use a principal with the needed role, or have a tenant admin grant it. |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | Your client found no Wyrd credential | Start `wyrd init`, or set `WYRD_API_KEY` or `WYRD_ACCESS_TOKEN`. |

`wyrd gateway credential revoke <name>` is permanent. Every deployment using that credential stops serving right away, and the name can't be put again.
