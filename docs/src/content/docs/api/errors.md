---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
pillar: wyrd
group: Reference
order: 21
---

# Errors

Wyrd returns errors as structured RFC 9457 Problem Details objects (`application/problem+json`). Agents and SDK clients must preserve the full structure and must not collapse errors into prose.

This page is the generated error catalog. For how an agent should act on these errors, see [Error remediation](/for-agents/error-remediation/).

## Client transport codes

| Code | Status | Title | Remediation |
|---|---:|---|---|
| `WYRD_CLIENT_400_CONFIG_INVALID` | 400 | Client transport configuration failed validation | Fix the `details.field` named on the error, then reconstruct the client. Not a retry path. |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | 401 | Credential chain produced no usable credential | Set `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`+`WYRD_TENANT`, or `WYRD_API_KEY`. Not a retry path. |
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
| `WYRD_WORKFLOW_422_MISSING_PARAMETER` | 422 | A prompt placeholder exists in neither workflow input nor upstream parameters. |

## Prompt and CLI authoring codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_AGENT_422_STRUCTURED_DECODE` | 422 | Response text is not valid JSON when `Prompt.output` is set. |
| `WYRD_PROMPT_422_PYDANTIC_REQUIRED` | 422 | Pydantic is required for class-based `Prompt(output=Model)` schemas. |
| `WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA` | 422 | The `Prompt(output=...)` dict cannot be reduced to JSON Schema. |
| `WYRD_PROMPT_400_PROVIDER_MISMATCH` | 400 | A provider accessor was called on a different provider's response. |
| `WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED` | 400 | A card file extension is not `.json`, `.yaml`, or `.yml`. |
| `WYRD_CLI_500_IO` | 500 | Filesystem IO failed during a CLI operation. |

## Registry codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID` | 400 | The `Idempotency-Key` header is malformed. Send a non-empty opaque key and retry the request. |
| `WYRD_SPEC_400_DUPLICATE_SUBMISSION` | 400 | Two submissions use the same `(kind, space, name)` identity. Submit each identity once per request. |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | 409 | Re-apply with the same identity but a different `spec_hash`; same-version cards are immutable. |
| `WYRD_REGISTRY_410_OPERATION_EXPIRED` | 410 | The registration idempotency operation has expired. Start a new registration with a fresh idempotency key. |

## Eval protocol codes

| Code | Status | When |
| --- | --- | --- |
| `WYRD_EVAL_404_RUN_NOT_FOUND` | 404 | The `run_id` path parameter does not match any open run. Re-open with `POST /v1/eval/runs`. |
| `WYRD_EVAL_401_MISSING_LEASE` | 401 | The request did not include an `Authorization: Bearer <token>` header. Send the `lease_token` from the open response. |
| `WYRD_EVAL_403_INVALID_LEASE` | 403 | The bearer token does not match the lease minted for the run. Re-open the run via `POST /v1/eval/runs`; leases are bound to one run. |
| `WYRD_EVAL_409_SUBMISSION_MISMATCH` | 409 | The submission kind, scenario id, or turn counter does not match the outstanding directive. Call `POST /v1/eval/runs/{run_id}/next` to retrieve the current directive and retry. |
| `WYRD_EVAL_429_TOO_MANY_RUNS` | 429 | The server has reached its concurrent-run cap. Wait for an existing run to complete, then retry. |
| `WYRD_EVAL_500_RUN_FAILED` | 500 | The eval engine or scenario loader encountered an internal error. Inspect server logs for the chained detail. |
