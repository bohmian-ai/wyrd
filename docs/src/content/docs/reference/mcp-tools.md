---
title: MCP tools
description: Exact Wyrd tool names, permissions, argument and result shapes, refusals, and query limits.
pillar: wyrd
section: Reference
order: 3
draft: true
status: runnable
---

The Wyrd server exposes its MCP tools over HTTP at `/mcp`. `wyrd mcp proxy` relays the same tools to an agent over stdio and adds none of its own. [Connect an agent through MCP](/connect/connect-an-agent-through-mcp/) covers installation and credentials.

## Discovery and authentication

Every request is authenticated in the caller's tenant. Use `tools/list` for the current input and output JSON Schemas and effect annotations. Unknown fields in closed argument objects are refused. IDs and versions below are exact discovered values, not names inferred from a prompt.

Read tools and `wyrd.guide` are always advertised. Principal revocation, verification execution/run creation, Operator connection writes, and gateway writes, revocation, resets, and deletions are advertised only to callers with their required write or delete permission. Advertising a tool grants no permission; naming a hidden write tool does not bypass authorization.

The tables below summarize wire shapes. Named shapes such as `ExecuteVerificationRequest` or `OperatorConnectionView` are the JSON Schemas `tools/list` returns for that tool; use them for every nested field, enum, and bound. A `{}` input means an empty argument object. Credentials and connection secrets are never returned.

## Agent frameworks

Wyrd serves MCP protocol revision 2026-07-28 only. Its MCP tool names contain dots and do not change. Model providers commonly accept only `^[a-zA-Z0-9_-]{1,64}$` as a function-tool name, so each framework below forwards a model-facing alias instead; its own MCP client still calls the dotted Wyrd tool and surfaces Wyrd's refusals as tool errors. These are the configurations Wyrd's framework journeys test.

| Framework | Tested configuration |
| --- | --- |
| OpenAI Agents SDK (Python) | `mcp_config={"convert_schemas_to_strict": True, "include_server_in_tool_names": True}` forwards aliases such as `mcp_wyrd__wyrd_guide`. |
| Pydantic AI | `MCPToolset(url, headers=...).renamed({"wyrd_guide": "wyrd.guide", ...})`, one entry per discovered tool. |
| Google ADK | Set `ADK_ENABLE_MCP_MODERN_PROTOCOL=1` (experimental in ADK) to negotiate 2026-07-28, and `GOOGLE_API_USE_CLIENT_CERTIFICATE=false` to skip ADK's Google Cloud mTLS probe. ADK has no renaming option and declares the dotted names as they are, so override `McpToolset.get_tools` to return shallow copies whose `name` is the alias; ADK still calls each copy's original MCP tool. |

## Investigation guide

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `wyrd.guide` | Read; authenticated | `{topic}` → `{topic, steps, examples}` | Static guidance; reads no tenant data and grants no permission |

`topic` is one of `overview`, `cards`, `verification`, `bifrost_sql`. Each example contains `tool`, `purpose`, and an `arguments` object. Replace angle-bracket placeholders with discovered identities and adjust example time windows. The guide is a workflow aid; `tools/list` remains the current schema authority.

## Cards and verification

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `cards.list` | Read; `cards:read` | List filters and optional cursor → `{items, next_cursor}` | Discover metadata summaries by kind, name, space, version range, lifecycle status, or metadata filter |
| `cards.get_by_ref` | Read; `cards:read` | `{card_ref}` → `{card, created_at, updated_at}` | Resolve exact kind, name, version, and space |
| `cards.get` | Read; `cards:read` | `{kind, card_uid}` → `{card, created_at, updated_at}` | Read one Card when you already have its kind and UID |
| `verification.get_binding` | Read; `cards:read` | `{binding_id}` → binding status | Inspect owner, subject, Verifier, readiness, and last run |
| `verification.get_run` | Read; `cards:read` | `{run_id}` → run status | Inspect execution, result ID, errors, and Operator dispatches |
| `verification.start_run` | Write; `verifier:run` on selected Verifier | `{target, input, idempotency_key?}` → run creation response | Enqueue a bounded manual Drift window; retry key preserves run identity |
| `verification.execute` | Execute; `verifier:run` on exact Verifier | ExecuteVerificationRequest → Judgment | Return a judgment in the same call and store its result; creates no run or Operator dispatch |

`cards.list.filter` is a metadata query string, not a JSON object: for example `labels.env = "prod"`, `labels.tier in ["gold","silver"]`, or `labels.deprecated not exists`, combined with `and`, `or`, and `not`. Fields are `labels.<key>`, `annotations.<key>`, and `attributes.<key>`; a query may hold up to 64 predicates. A syntax error returns `WYRD_QUERY_400_INVALID_SYNTAX`. Preserve `next_cursor` unchanged alongside the same filters. A missing version or malformed cursor is invalid; no implicit newest-version or fuzzy selection occurs. Missing and foreign-tenant Cards are indistinguishable. Relationship edges in `outbound_refs` and `inbound_refs` carry exact identities under `ref`.

`verification.start_run.target` is tagged `binding` (with `binding_id`) or `verifier` (with `verifier_card_uid` and `card_uid`). `input` is tagged `drift_window` with UTC `start` and `end`. Invalid windows, absent targets, or unavailable baselines are refused. `verification.execute` supports the current scoring and direct assertion forms; unsupported execution forms are refused. Follow the [verification procedure](/build-and-verify/verify/run-a-verification-and-inspect-its-evidence/) to read persisted results.

## Bifrost

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `bifrost.list_tables` | Read; `bifrost_table:read` | `{}` → `{tables}` | List tenant-authorized namespace, name, status entries |
| `bifrost.describe_table` | Read; `bifrost_table:read` | `{namespace, name}` → table description | Read user fields, managed fields, partitioning, sorting, and Bloom columns |
| `bifrost.query` | Read; scoped `bifrost_query:read` | `{sql, deadline_ms?, max_rows?, max_bytes?}` → `{columns, rows, terminal}` | Run one complete bounded SELECT over authorized tables |

Schema/table object grants restrict the query to the allowed objects. Gateway request/response payload columns also require `gateway_payload:read`. Catalog listing and schema description require `bifrost_table:read`; query grants separately decide which objects can be read. Sensitive metadata on verification, Eval, or trace columns is descriptive; table query permission governs those columns.

### Query limits

| Bound | Default | Maximum |
| --- | --- | --- |
| SQL text | Required | 65,536 UTF-8 bytes |
| `max_rows` | 1,000 | 10,000 |
| `max_bytes` | 4 MiB | 16 MiB |
| `deadline_ms` | Server policy | 4,294,967,295 ms; minimum 1 |

Use DataFusion SQL: one SELECT, explicit projections, fully qualified discovered names, appropriate time bounds, and LIMIT. Caller identity, tenant, execution path, query class, topology, and plan are server-owned and cannot be supplied as arguments.

`columns` describes the ordered projection; each row is a positional array in that order. Require `terminal.outcome: "success"` before reporting a complete result. Empty rows are absence of matches. Exceeding a row/byte ceiling refuses the result rather than truncating it; errors contain no partial successful rows. See the [Bifrost procedure](/build-and-verify/observe/write-and-query-custom-data-with-bifrost/) for runnable writes and queries.

## Principal credentials

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `principals.list_credentials` | Read; `service_accounts:write` | `{principal_id}` → CredentialListResponse | Read nonsecret metadata including expired/revoked credentials |
| `principals.revoke_credential` | Write; `service_accounts:write` | `{principal_id, credential_id}` → CredentialRevoked | Prevent further token issuance from that credential |

A token already minted from a revoked credential lapses within five minutes. The principal, its roles, and other credentials remain. Unknown or foreign-tenant identities are refused; secret material is never disclosed.

## Operator connections

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `operator_connections.list` | Read; `operators:read` | `{}` → `{connections}` | List redacted connection metadata |
| `operator_connections.get` | Read; `operators:read` | `{connection_id}` → OperatorConnectionView | Read one connection |
| `operator_connections.create` | Write; `operators:write` | `{request: CreateOperatorConnectionRequest}` → OperatorConnectionView | Create provider-tagged Slack, PagerDuty, or HTTP configuration |
| `operator_connections.update` | Write; `operators:write` | `{connection_id, request}` → OperatorConnectionView | Patch configuration, secret, or status; provider/name remain fixed |
| `operator_connections.disable` | Write; `operators:write` | `{connection_id}` → OperatorConnectionView | Retain the connection in disabled state |

Provider-specific invalid configuration, missing connection IDs, and permission denials are refused. Updates preserve omitted fields. Secrets are encrypted at rest and redacted from every response.

## Gateway administration

Gateway tools address resources in the authenticated tenant by exact `name`. They do not invoke a model. Reads require `gateway:read`, replacements/revocation/policy resets require `gateway:write`, and credential/deployment deletion requires `gateway:delete`.

| Tool | Effect / permission | Input → output | Purpose |
| --- | --- | --- | --- |
| `gateway.list_provider_credentials` | Read; `gateway:read` | `{}` → `{items}` of credential views | List redacted sources, states, timestamps |
| `gateway.list_provider_deployments` | Read; `gateway:read` | `{}` → `{items}` of deployments | List model adapters, credential names, capabilities, weights |
| `gateway.get_provider_credential` | Read; `gateway:read` | `{name}` → credential view | Read one redacted credential |
| `gateway.get_provider_deployment` | Read; `gateway:read` | `{name}` → deployment | Read one deployment |
| `gateway.get_fallback_policy` | Read; `gateway:read` | `{}` → fallback policy | Read effective routing fallback |
| `gateway.get_governance_policy` | Read; `gateway:read` | `{}` → governance policy | Read limits, budgets, pricing, unknown-cost policy |
| `gateway.get_capture_policy` | Read; `gateway:read` | `{}` → versioned capture policy | Read payload capture configuration |
| `gateway.put_provider_credential` | Write; `gateway:write` | ProviderCredentialWrite → credential view | Create or rotate an operator/external binding or write-only managed secret |
| `gateway.put_provider_deployment` | Write; `gateway:write` | ProviderDeployment → deployment | Create or replace a deployment |
| `gateway.put_fallback_policy` | Write; `gateway:write` | FallbackPolicy → fallback policy | Replace routing fallback |
| `gateway.put_governance_policy` | Write; `gateway:write` | GovernancePolicy → governance policy | Replace limits, budgets, pricing |
| `gateway.put_capture_policy` | Write; `gateway:write` | CapturePolicyWrite → versioned capture policy | Replace payload capture configuration |
| `gateway.revoke_provider_credential` | Write; `gateway:write` | `{name}` → credential view | Terminally revoke; repeated calls retain that state |
| `gateway.delete_provider_credential` | Delete; `gateway:delete` | `{name}` → `{}` | Delete an unreferenced credential; absence succeeds |
| `gateway.delete_provider_deployment` | Delete; `gateway:delete` | `{name}` → `{}` | Delete a deployment; absence succeeds |
| `gateway.delete_fallback_policy` | Reset; `gateway:write` | `{}` → `{}` | Restore default fallback policy |
| `gateway.delete_governance_policy` | Reset; `gateway:write` | `{}` → `{}` | Restore default governance policy |

Gateway schemas bound managed secret input and validate provider capabilities and policy values. Never paste a provider secret into an investigation prompt. A managed secret is accepted only for writing and never readable again. Credentials still referenced by deployments cannot be deleted. Wrong names, invalid configurations, and missing scopes return structured Wyrd errors.

## Errors and evidence

A tool can return a JSON-RPC error carrying a Wyrd problem, or an `isError` result with a structured problem body. Read its stable `code`, `status`, and remediation rather than treating transport success as operation success. Bifrost runtime refusals use the structured result form; malformed arguments may fail at the protocol boundary.

Common query refusals include permission denial, non-SELECT admission, unknown columns/tables, deadline failure, and `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE`. Retry only when the problem's remediation warrants it. A failed or incomplete query supplied no complete evidence. State uncertainty and absence of retained results explicitly.
