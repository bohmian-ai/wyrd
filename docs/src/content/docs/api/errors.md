---
title: Errors
description: Error handling guidance for Wyrd API clients and agents.
---

# Errors

Wyrd returns errors as structured RFC 7807 Problem Details objects. Agents and SDK clients must preserve the full structure and must not collapse errors into prose.

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

## Catalog

The full Wyrd error catalog is not emitted as a standalone generated file yet. The Skald runtime codes below are seed tables for that generator.

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
| `SKALD_WORKFLOW_404_TASK` | 404 | `execute_task` or a lookup referenced an unknown task id. |
| `SKALD_WORKFLOW_404_AGENT` | 404 | A task referenced an agent id not present in the workflow's agent set. |
| `SKALD_WORKFLOW_422_DEP_MISSING` | 422 | A task declared a dependency on an unknown task id during `add_task`. |
| `SKALD_WORKFLOW_409_TASK_EXISTS` | 409 | Duplicate task id within the workflow. |
| `SKALD_WORKFLOW_422_SELF_DEP` | 422 | A task declared itself as a dependency. |
| `SKALD_WORKFLOW_422_CYCLE` | 422 | The task graph contains a cycle. |
| `SKALD_WORKFLOW_422_OUTPUT_SCHEMA` | 422 | A task's output failed JSON-schema validation on every retry attempt. |
| `SKALD_WORKFLOW_500_MAX_RETRIES` | 500 | Provider failures exhausted `max_retries` for a task. |
| `SKALD_WORKFLOW_500_STALLED` | 500 | No ready tasks remain but the workflow is incomplete, typically after a dependency failed terminally. |
| `SKALD_WORKFLOW_500_LOCK` | 500 | Internal task lock acquisition failed. |
| `SKALD_WORKFLOW_501_UNSUPPORTED_HANDOFF` | 501 | No message conversion exists for the source and destination provider pair, or the carried message was `RawV1`. |
