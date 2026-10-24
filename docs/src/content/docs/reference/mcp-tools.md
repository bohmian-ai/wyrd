---
title: MCP tools
description: Wyrd MCP tool names, permissions, shapes, and query limits.
pillar: wyrd
section: Reference
order: 5
status: runnable
---

The server serves MCP protocol revision 2026-07-28 over HTTP at `/mcp`; `wyrd mcp proxy` relays the same tools over stdio.

## Discovery

- `tools/list` returns each tool's current input and output JSON Schemas; named shapes below, such as `VerificationRequest`, are those schemas. Unknown argument fields are refused.
- Read tools and `wyrd.guide` are always listed. Write, revoke, reset, and delete tools are listed only to callers holding their permission. Listing grants nothing.
- Credentials and secrets are never returned.

## Agent frameworks

Tool names contain dots, which many model providers refuse, so each framework forwards an alias.

| Framework | Tested configuration |
| --- | --- |
| OpenAI Agents SDK (Python) | `mcp_config={"convert_schemas_to_strict": True, "include_server_in_tool_names": True}`; aliases such as `mcp_wyrd__wyrd_guide` |
| Pydantic AI | `MCPToolset(url, headers=...).renamed({"wyrd_guide": "wyrd.guide", ...})`, one entry per tool |
| Google ADK | `ADK_ENABLE_MCP_MODERN_PROTOCOL=1` and `GOOGLE_API_USE_CLIENT_CERTIFICATE=false`; override `McpToolset.get_tools` to return copies whose `name` is the alias |

## Investigation guide

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `wyrd.guide` | Any caller | `{topic}` → `{topic, steps, examples}` | Static workflow guidance |

`topic` is `start`, `overview`, `cards`, `verification`, or `bifrost_sql`; `start` is the entry point for a new agent. Each example has `tool`, `purpose`, and `arguments`; angle-bracket placeholders stand for discovered values.

## Cards and verification

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `cards.list` | `cards:read` | List filters and optional cursor → `{items, next_cursor}` | List Card summaries |
| `cards.get` | `cards:read` | `{card_ref}` or `{kind, card_uid}` → `{card, created_at, updated_at}` | Read one Card; pass exactly one form |
| `verification.get_binding` | `cards:read` | `{binding_id}` → binding status | Read one binding |
| `verification.get_execution` | `cards:read` | `{execution_id}` → execution status | Read one execution |
| `verification.evidence` | `verifier:run` | `{files: [{sha256, size_bytes}]}` → `{uploads}` | Plan uploads for files the tenant lacks |
| `verification.execute` | `verifier:run` on exact Verifier | Verifier, subject, optional `run_id`, and named input declarations → `VerificationResult` | Verify now and store the result; no Operator dispatch |
| `verification.enqueue` | `verifier:run` on exact Verifier | The `verification.execute` arguments plus optional `idempotency_key` → `{execution_id}` | Queue a verification; poll `verification.get_execution` |

`cards.list.filter` is a query string, such as `labels.env = "prod" and labels.deprecated not exists`, over `labels.<key>`, `annotations.<key>`, and `attributes.<key>`, with `=`, `in`, `not exists`, `and`, `or`, and `not`; at most 64 predicates. A caller passes `next_cursor` back unchanged with the same filters. Missing and foreign-tenant Cards look the same. `verification.execute` refuses a scheduled drift Verifier.

## Bifrost

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `bifrost.list_tables` | `bifrost_table:read` | `{}` → `{tables}` | List readable tables |
| `bifrost.describe_table` | `bifrost_table:read` | `{namespace, name}` → table description | Read one table's schema and layout |
| `bifrost.query` | scoped `bifrost_query:read` | `{sql, deadline_ms?, max_rows?, max_bytes?}` → `{columns, rows, terminal}` | Run one bounded `SELECT` |

Gateway request and response payload columns also need `gateway_payload:read`.

### Query limits

| Bound | Default | Maximum |
| --- | --- | --- |
| SQL text | Required | 65,536 UTF-8 bytes |
| `max_rows` | 1,000 | 10,000 |
| `max_bytes` | 4 MiB | 16 MiB |
| `deadline_ms` | Server policy | 4,294,967,295 ms; minimum 1 |

`sql` is one DataFusion SQL `SELECT` with fully qualified table names. Rows are positional arrays in `columns` order. A result is complete only when `terminal.outcome` is `"success"`; a result over `max_rows` or `max_bytes` is refused, never truncated.

## Principal credentials

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `principals.list_credentials` | `service_accounts:write` | `{principal_id}` → CredentialListResponse | List credentials, including expired and revoked |
| `principals.revoke_credential` | `service_accounts:write` | `{principal_id, credential_id}` → CredentialRevoked | Stop token issuance from a credential |
| `principals.set_workload_restriction` | `*` | `{principal_id, workload_restriction}` → PrincipalSummary | Replace a Service account's workload restriction; a family makes it that family's unique scheduled runtime account |

A token already minted from a revoked credential lapses within five minutes.

## Operator connections

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `operator_connections.list` | `operators:read` | `{}` → `{connections}` | List connections |
| `operator_connections.get` | `operators:read` | `{connection_id}` → OperatorConnectionView | Read one connection |
| `operator_connections.create` | `operators:write` | `{request: CreateOperatorConnectionRequest}` → OperatorConnectionView | Create a Slack, PagerDuty, or HTTP connection |
| `operator_connections.update` | `operators:write` | `{connection_id, request}` → OperatorConnectionView | Patch a connection; provider and name are fixed |
| `operator_connections.disable` | `operators:write` | `{connection_id}` → OperatorConnectionView | Disable a connection |

## Gateway administration

Gateway tools address resources by exact `name` and never call a model.

| Tool | Permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `gateway.list_provider_credentials` | `gateway:read` | `{}` → `{items}` of credential views | List credentials |
| `gateway.list_provider_deployments` | `gateway:read` | `{}` → `{items}` of deployments | List deployments |
| `gateway.get_provider_credential` | `gateway:read` | `{name}` → credential view | Read one credential |
| `gateway.get_provider_deployment` | `gateway:read` | `{name}` → deployment | Read one deployment |
| `gateway.get_fallback_policy` | `gateway:read` | `{}` → fallback policy | Read the fallback policy |
| `gateway.get_governance_policy` | `gateway:read` | `{}` → governance policy | Read limits, budgets, and pricing |
| `gateway.get_capture_policy` | `gateway:read` | `{}` → versioned capture policy | Read the capture policy |
| `gateway.put_provider_credential` | `gateway:write` | ProviderCredentialWrite → credential view | Create or rotate a credential |
| `gateway.put_provider_deployment` | `gateway:write` | ProviderDeployment → deployment | Create or replace a deployment |
| `gateway.apply_provider_deployments` | `gateway:write` | ProviderDeploymentsApply → applied credentials and deployments | Write credentials and deployments in one transaction |
| `gateway.put_fallback_policy` | `gateway:write` | FallbackPolicy → fallback policy | Replace the fallback policy |
| `gateway.put_governance_policy` | `gateway:write` | GovernancePolicy → governance policy | Replace the governance policy |
| `gateway.put_capture_policy` | `gateway:write` | CapturePolicyWrite → versioned capture policy | Replace the capture policy |
| `gateway.revoke_provider_credential` | `gateway:write` | `{name}` → credential view | Revoke permanently |
| `gateway.delete_provider_credential` | `gateway:delete` | `{name}` → `{}` | Delete an unreferenced credential; absent succeeds |
| `gateway.delete_provider_deployment` | `gateway:delete` | `{name}` → `{}` | Delete a deployment; absent succeeds |
| `gateway.delete_fallback_policy` | `gateway:write` | `{}` → `{}` | Restore the default |
| `gateway.delete_governance_policy` | `gateway:write` | `{}` → `{}` | Restore the default |

A managed secret is write-only. A credential still referenced by a deployment cannot be deleted.

## Errors

A refusal is a JSON-RPC error or an `isError` result carrying a Wyrd problem with `code` and remediation.

| Code | Cause |
| --- | --- |
| `WYRD_QUERY_400_INVALID_SYNTAX` | Malformed `cards.list` filter |
| `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE` | The query result is too large to return |
