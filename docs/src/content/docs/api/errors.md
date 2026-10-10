---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
pillar: wyrd
group: Reference
order: 21
---

# Errors

Wyrd's JSON API returns errors as structured RFC 9457 Problem Details objects (`application/problem+json`). Agents and SDK clients must preserve the full structure and must not collapse errors into prose. The browser sign-in endpoints (`/auth/authorize`, `/auth/callback`, and `/auth/device`) may instead redirect or show an HTML page; see the [OpenAPI reference](/api/openapi/).

The other exception is the four OAuth 2.0 form endpoints (`/auth/token`, `/auth/platform/token`, `/auth/device_authorization`, and `/auth/revoke`). They refuse with the standard RFC 6749 section 5.2 JSON body (`error`, optional `error_description`), as OAuth clients expect. That body carries no Wyrd `code`, so branch on `error`; the codes below never appear in it. The Rust, Python, and TypeScript SDKs map such a refusal onto the nearest catalog code. See [SSO and OIDC](/self-hosting/sso-and-oidc/#oauth-endpoints).

This page is the generated error catalog. For how an agent should act on these errors, see [Error remediation](/for-agents/error-remediation/).

## Client transport codes

| Code | Status | Title | Remediation |
|---|---:|---|---|
| `WYRD_CLIENT_400_CONFIG_INVALID` | 400 | Client transport configuration failed validation | Fix the `details.field` named on the error, then reconstruct the client. Not a retry path. |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | 401 | Credential chain produced no usable credential | Set `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`+`WYRD_TENANT`, or `WYRD_API_KEY`, or run `wyrd auth login`. Not a retry path. |
| `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` | 401 | Saved user login cannot be used | Run `wyrd auth login` again or select the tenant. `details.reason` names the failure. Not a retry path. |
| `WYRD_CLIENT_503_TRANSPORT_DOWN` | 503 | Client transport is unavailable | Retry with backoff once the network or server health recovers. |

## Error fields

| Field | Description |
| --- | --- |
| `type` | URI identifying the error class |
| `title` | Human-readable summary of the error class (stable across instances) |
| `status` | HTTP status code (integer) |
| `code` | Wyrd-specific machine-readable error code (string, stable) |
| `detail` | Instance-specific description of what went wrong |
| `remediation` | Actionable guidance for the caller or agent |
| `context` / `details` | Structured key/value map with operation-specific fields |
| `request_id` | Present when the server can correlate the request |
| `trace_id` | Present when distributed tracing is active |
| `instance` | URI identifying the specific resource or operation that failed (when applicable) |

## Agent handling

- Preserve all structured fields; do not collapse errors into prose.
- Use `code` for programmatic branching, not `title` or `detail` (those are for humans).
- Surface policy and audit failures as decisions (not generic transport failures).
- Keep retries bounded and tied to idempotent operations.
- Link remediation steps to the card or run that caused the error.

## Skald agent codes

| Code | Status | When |
| --- | --- | --- |
| `SKALD_AGENT_404_PROVIDER` | 404 | The provider registry returned no client for the agent's `ProviderName`. Bind the missing provider before constructing the agent, or correct the agent def. |
| `SKALD_AGENT_404_TOOL` | 404 | The provider response requested a tool name not registered for the agent. Add the tool to the agent's `ToolRegistry`, or remove the tool reference from the agent def. |
| `SKALD_AGENT_409_PROVIDER_MISMATCH` | 409 | `Agent::run_prompt` received prompt messages for a different provider than the agent's bound provider. Rebuild the prompt for the agent provider before running it. |
| `SKALD_AGENT_422_TOOL_ARGS` | 422 | A dispatched tool rejected the provider-emitted arguments. Inspect the tool's input schema and the response payload. |
| `SKALD_AGENT_422_SYSTEM_PROMPT` | 422 | The agent's system prompt failed to shape into native messages for the chosen provider. Custom providers must author the system content into the prompt body directly. |
| `SKALD_AGENT_422_PROMPT` | 422 | `Agent::run_prompt` received a prompt that cannot be sent by the agent's provider. Validate the prompt body and provider before dispatch. |
| `SKALD_AGENT_500_MAX_ITERATIONS` | 500 | The bounded tool loop exceeded `RunConfig::max_iterations`. Raise the cap or fix the agent and tool set so the model terminates. |
| `SKALD_AGENT_502_PROVIDER` | 502 | The underlying `skald-runtime` provider call failed. The chained `SKALD_RUNTIME_*` code identifies the transport or decode failure. |

## Skald workflow codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_WORKFLOW_422_CYCLE` | 422 | The workflow DAG contains a cycle. |
| `WYRD_WORKFLOW_422_MISSING_DEPENDENCY` | 422 | A step depends on an id that does not exist. |
| `WYRD_WORKFLOW_422_DUPLICATE_STEP_ID` | 422 | Two steps share the same id. |
| `WYRD_WORKFLOW_422_MISSING_NAME` | 422 | `save` or `to_card` was called on an anonymous workflow. |
| `WYRD_WORKFLOW_422_MISSING_PARAMETER` | 422 | A declared step binding or Workflow output selected a value the run did not produce. |

## Prompt and CLI authoring codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_AGENT_422_STRUCTURED_DECODE` | 422 | Response text is not valid JSON when `Prompt.output` is set. |
| `WYRD_PROMPT_422_PYDANTIC_REQUIRED` | 422 | Pydantic is required for class-based `Prompt(output=Model)` schemas. |
| `WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA` | 422 | The `Prompt(output=...)` dict cannot be reduced to JSON Schema. |
| `WYRD_PROMPT_400_PROVIDER_MISMATCH` | 400 | A provider accessor was called on a different provider's response. |
| `WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED` | 400 | A card file extension is not `.json`, `.yaml`, or `.yml`. |
| `WYRD_CLI_500_IO` | 500 | Filesystem IO failed during a CLI operation. |

## Server installation codes

`wyrd server install` reports these. Every one leaves the previously installed server active.

| Code | Status | When |
| --- | --- | --- |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | 400 | The host is not macOS or glibc Linux on x86_64 or aarch64, so no official bundle exists for it. |
| `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE` | 503 | The release listing or a download failed, no stable release is published, or the newest release has no bundle for this host. |
| `WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED` | 422 | `checksums.txt` or its signature is missing or not signed by the Wyrd release key, the bundle's SHA-256 differs, or the bundle does not extract to a runnable server. |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | 409 | The newest stable server is outside the CLI's compatible version line. Upgrade the CLI. |

## Registry codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID` | 400 | The `Idempotency-Key` header is malformed. Send a non-empty opaque key and retry the request. |
| `WYRD_SPEC_400_DUPLICATE_SUBMISSION` | 400 | Two submissions use the same `(kind, space, name)` identity. Submit each identity once per request. |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | 409 | Re-apply with the same identity but a different `spec_hash`; same-version cards are immutable. |
| `WYRD_REGISTRY_410_OPERATION_EXPIRED` | 410 | The registration idempotency operation has expired. Start a new registration with a fresh idempotency key. |
