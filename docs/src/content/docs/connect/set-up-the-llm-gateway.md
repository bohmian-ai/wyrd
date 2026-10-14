---
title: Set up the LLM gateway
description: Store a provider key, deploy a model on it, turn on call capture, and check that the gateway records a call.
pillar: wyrd
section: Connect
order: 4
draft: true
status: runnable
---

Wyrd's gateway sits between your code and a model provider. You store the provider key in Wyrd once; callers then reach the model with their ordinary Wyrd credential and never hold the provider key. Every call is checked against the caller's permissions, and, once you turn capture on, recorded in `vala.gateway.calls`.

This page sets up `gpt-4o` for the `support-desk` service from the [quickstart](/get-started/quickstart/). It takes three resources: a provider credential, a deployment, and a capture policy. To send requests once it is set up, see [Make gateway requests from a framework](/connect/make-gateway-requests-from-a-framework/).

## Prerequisites

- A Wyrd server: the local one from [Set up Wyrd](/get-started/setup/), or a deployed one.
- A credential with the `admin` role. Only `admin` holds `gateway:write` and `gateway:delete`. The tenant key that `wyrd server dev` saves is one.
- A sealing key on the server, so it can encrypt the provider key. `wyrd server dev` creates one on first run. A deployed server needs `WYRD_SEALING_KEY_FILE`, as in [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/#4-start-the-server-and-check-readiness).
- An OpenAI API key with access to `gpt-4o`, in `OPENAI_API_KEY`.

## 1. Store the provider key

Pipe the credential to the server on stdin, so the key stays out of files, process arguments, and shell history:

```bash
printf '{"name":"openai-key","provider":"openai","source":{"managed_secret":{"secret":"%s"}}}' \
  "$OPENAI_API_KEY" | wyrd gateway credential put --file -
```

```json
{
  "name": "openai-key",
  "provider": "openai",
  "source": "managed_secret",
  "state": "active",
  "created_at": "2026-10-14T15:02:11.482Z",
  "updated_at": "2026-10-14T15:02:11.482Z",
  "rotated_at": null,
  "revoked_at": null
}
```

The server encrypts the key with its sealing key, bound to your tenant, the credential name, and the provider. No read, listing, log, or audit record returns it; `source` reads back as the bare string `managed_secret`. Without a sealing key the put fails with `WYRD_SERVER_503_SERVICE_UNAVAILABLE` and the message `gateway credential protection is unavailable`, and nothing is stored.

To rotate the key, run the same command with the new value. `rotated_at` changes, and calls admitted after the put use the new key. `wyrd gateway credential revoke openai-key` is permanent: a revoked name can never be put again.

`managed_secret` is one of three sources. The other two, `environment` and `external_secret`, name a binding or secret backend that the server's operator configured, so the key never passes through the API at all. They are out of scope here.

## 2. Deploy the model

A deployment ties one exact model to a provider protocol and a credential:

```bash
wyrd gateway deployment put --file - <<'JSON'
{
  "name": "gpt-4o",
  "model": { "provider": "openai", "model": "gpt-4o" },
  "adapter": "openai",
  "auth": { "bearer": { "credential": "openai-key" } },
  "capabilities": ["chat_completions", "responses"],
  "routing_weight": 1
}
JSON
```

The command prints the stored deployment. Callers never name a deployment; they ask for the model `openai/gpt-4o`, and the gateway picks among the deployments serving it by `routing_weight`. Put the same `name` again to replace a deployment. The quickstart declared only `chat_completions`; rerunning this command adds `responses`.

`capabilities` is explicit. The gateway never probes a provider to find out what it supports, and a call for an operation no deployment declares fails with `WYRD_GATEWAY_404_MODEL_UNAVAILABLE`.

Each built-in adapter needs its own provider name and upstream authentication:

| `adapter` | `model.provider` | `auth` | Capabilities it accepts |
| --- | --- | --- | --- |
| `"openai"` | `openai` | `{"bearer": {"credential": ...}}` | Every operation except `system_one` |
| `"anthropic"` | `anthropic` | `{"api_key_header": {"header": "x-api-key", "credential": ...}}` | `chat_completions` |
| `"gemini"` | `gemini` | `{"api_key_header": {"header": "x-goog-api-key", "credential": ...}}` | `chat_completions` |
| `{"vertex": {"project": ..., "location": ...}}` | `vertex` | `{"bearer": {"credential": ...}}` | `chat_completions` |
| `"jev"` | `jev` | `{"bearer": {"credential": ...}}` | `system_one`, `chat_completions` |
| `{"openai_compatible": {"base_url": ...}}` | any name except the five above | `bearer`, `api_key_header`, or `"none"` | Every operation except `system_one` |

The full operation list is `chat_completions`, `responses`, `embeddings`, `images`, `audio`, `batches`, and `system_one`. The Vertex adapter presents the stored value as a Google OAuth access token exactly as stored. It does not mint or refresh one, so put the credential again before the token expires.

The server rejects a provider that does not match its adapter and a capability the adapter cannot serve with `WYRD_GATEWAY_400_INVALID_CONFIGURATION`. It does not check the `auth` pairing at put time: a deployment with any other `auth` is stored, and each call to it then fails with `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE`.

## 3. Turn on capture

Capture is off by default, and with it off the gateway still serves calls but writes nothing to Bifrost. Choose what to record:

```bash
wyrd gateway capture-policy put --file - <<'JSON'
{ "mode": "metadata", "payload_fields": [] }
JSON
```

```json
{
  "mode": "metadata",
  "payload_fields": [],
  "version": 2
}
```

| `mode` | Records | `payload_fields` |
| --- | --- | --- |
| `disabled` | Nothing | `[]` |
| `metadata` | Caller, model, outcome, usage, timing, and any run or Card correlation; no message content | `[]` |
| `payload` | Metadata, plus the redacted request, response, or both | `["request"]`, `["response"]`, or both |

Start with `metadata`. Payload capture stores your users' prompts and the model's answers, and anyone who reads them also needs `gateway_payload:read`. The default policy is `disabled` at `version` 1. The version goes up only when the content changes, so putting the same policy twice returns the same version.

## 4. Let callers invoke the model

Calling a model requires `gateway:invoke` on that model. The `workload` role that a registered Service or Agent gets has it, as do `editor` and `admin`. `viewer` does not. Grant a role with `wyrd principal role grant`, as in [Grant and inspect permissions](/operate/grant-and-inspect-permissions/). A custom role can narrow `gateway:invoke` to one provider or one model; see [Identity and authorization](/understand/identity-and-authorization/#roles-and-permissions).

The caller's tenant always comes from its credential. A request cannot reach another tenant's credentials or deployments.

## 5. Confirm it works

Check that the credential is active and the deployment exists:

```bash
wyrd gateway credential get openai-key
wyrd gateway deployment list
```

Then send one request, using any example from [Make gateway requests from a framework](/connect/make-gateway-requests-from-a-framework/#1-call-the-unified-route), and look for its row:

```bash
wyrd query --sql "SELECT started_at, requested_model['model'] AS model, outcome, run_id, card_uid
  FROM vala.gateway.calls ORDER BY started_at DESC LIMIT 5"
```

A successful call has `outcome` `succeeded`. Rows are written in the background after the call returns, so a call made a few seconds ago may not show yet; query again before deciding it is missing. If no call ever appears, check that the capture policy is not `disabled`.

## What can go wrong

| Symptom | Cause | Fix |
| --- | --- | --- |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE`, `gateway credential protection is unavailable` on `credential put` | The server has no sealing key | Set `WYRD_SEALING_KEY_FILE` and restart. `wyrd server dev` creates one for you |
| `WYRD_PERMISSION_403_DENIED_RBAC` on any `put` or `delete` | Your credential is not `admin` | Use the tenant administrator's credential |
| `WYRD_GATEWAY_400_INVALID_CONFIGURATION` on `deployment put` | `details.field` names the problem: a provider that does not match the adapter, an undeclared or unsupported capability, or a credential that is missing, revoked, or for another provider | Correct that field. A revoked credential needs a new name |
| `WYRD_GATEWAY_400_INVALID_CONFIGURATION` on `capture-policy put` | `payload_fields` is set outside `payload` mode, or empty in it | Match the table in step 3 |
| `WYRD_GATEWAY_409_RESOURCE_CONFLICT` on `credential put` or `delete` | The credential is revoked, or a deployment still references it | Use a new name, or delete the deployment first |
| Calls fail with `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE` after the sealing key changed | The provider key was sealed under a key the server no longer holds | List the old key in `WYRD_SEALING_RETAINED_KEYS_FILE` and restart, or put the credential again |
| No rows in `vala.gateway.calls` | Capture is `disabled`, or the rows are not written yet | Run step 3, then query again |

The same resources are available over HTTP under `/v1/admin/gateway/` (`provider-credentials`, `provider-deployments`, `capture-policy`, `fallback-policy`, `governance-policy`) and as `gateway.*` tools in the [MCP tool reference](/reference/mcp-tools/#gateway-administration). The Python, Rust, and TypeScript SDKs' `Gateway` class can put deployments and policies and read credentials. None of them can write a credential; use the CLI or MCP for that.
