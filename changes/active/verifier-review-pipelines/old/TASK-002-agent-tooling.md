---
id: TASK-002
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: proposed
previous_id: TASK-003
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001]
req: [REQ-021, REQ-023, REQ-031, REQ-046, REQ-047, REQ-052, REQ-053, REQ-054, REQ-055, REQ-056, REQ-057, REQ-058]
inv: [INV-002, INV-003, INV-006, INV-007, INV-010, INV-017, INV-018, INV-019]
ac: [AC-017, AC-018, AC-039, AC-040, AC-046, AC-047, AC-048, AC-049, AC-050]
---

# Agent tools: fs.write, publish and hosted tool selection

## Outcome and Value

Implement the Agent-facing fs.write and publish tools, expose them through existing hosted tool selection and the existing Agent loop. Only Agents receive these tools; standalone code actions keep their JSON output without a publication interface. Task 001 supplies publication persistence; this task owns tool schemas, validation, authorization, invocation binding and dispatch.

## What Changes and What Must Stay True

Task 001 supplies accepted run state, exact File descriptors and Bifrost
recovery. Wire actual Agent tools to it in both direct and queued execution.
The Agent and publication tool own no separate verification state. Successors
consume accepted references from the engine state without inventory rediscovery.
Direct disconnect stops new Agent calls without background queue promotion.

| Implement | Required implementation logic |
|---|---|
| **fs.write** | Add the built-in descriptor and typed arguments path/content. Accept UTF-8 content at a safe /work path, create safe parents, use the shared writer lock/byte budget, refuse /inputs, traversal, unsafe parents and over-limit writes; allow replacing an existing regular file under the same private workspace lock and byte limits. Return path/byte count. Output-only Agents get private scratch without an artificial input mount. |
| **publish** | Add the built-in descriptor and arguments name plus exactly one of path/value; explicit JSON null counts as a value. Validate name [A-Za-z0-9_-]{1,64}. Path may name an existing /work file or entire directory. Host derives execution/item/iteration/attempt and original tool-call identity; call Task 001 backend, await durable acceptance and return only {name,kind}. Do not implement another snapshot/store/replay mechanism. |
| **Hosted selection and permissions** | Extend the existing built-in catalog, AgentToolset and ToolSession for fs.write/publish. Preserve tool_names, built-in identity, tool:invoke scopes and ToolCallRecord. Resolve selected descriptors before the first model call; reject unknown names and provider-name collisions. No remote tool source declaration, transport or new permission identity is needed. |
| **Limits and provenance** | Apply accepted authority plus invoke/domain permission, per-call/cumulative calls/bytes/time, cancellation and bounded retries. Record selected tool/schema, producer indices/attempt, sanitized arguments, digest/bytes/error and retained output references. Large responses page or use durable references. Reviewers use existing filesystem tools over their mounted evidence. |

## Approach

1. Add fs.write/publish contracts, built-in permissions/descriptors and safe scratch access.
2. Wire publication dispatch to Task 001; preserve provider call identity through receipt replay.
3. Extend the existing built-in selection, permission checks and recorded provenance for the new hosted tools.
4. Project tool discovery and run real Agent-loop journeys plus negative authority/limit cases.

## Ordered Implementation Scenarios

### 1. Write workspace text

**Behavior.** An Agent with no input bindings writes /work/review.md, overwrites it and reads the updated UTF-8 text with path/byte count. Table: /inputs, unsafe parent/traversal and over-limit content → refusal/no unintended write. Another invocation using the same path stays isolated.

**RED.** Drive the real model tool-call loop; assert descriptor, response and readback plus unchanged forbidden destinations. Expected RED: fs.write is absent from the hosted descriptor/dispatch catalog. Run [proof 1](#proof-1); names are planned until added/listed.

**GREEN.** Implement fs.write at the workspace/tool owner using shared lock, confinement and accounting; allow scratch-only sessions.

**REFACTOR.** Reuse filesystem/path rules, not a second writable workspace implementation. Keep this and earlier scenario tests green.

### 2. Publish Agent-chosen content

**Behavior.** Agent writes arbitrary filenames/nested directory and calls publish with logical review name; separately publish JSON and null. Tool returns {name,kind}; a downstream Agent discovers exact retained content via manifest. Missing/both sources, invalid name, foreign path or missing path refuse. After publish, overwrite the original workspace file: downstream retained content stays unchanged. Final output alone creates no publication. A standalone code action returning the same JSON has no write/publish tool or named publication; extraction evidence is persisted by the engine without an Agent tool call.

**RED.** Assert actual Agent tool requests/responses and mounted bytes/JSON, including empty directories and no fabricated filename. Expected RED: publish and downstream publication binding are absent. Run [proof 2](#proof-2); names are planned until added/listed.

**GREEN.** Add typed publish schema/dispatch and delegate snapshot, integrity and accepted manifest to Task 001.

**REFACTOR.** Keep durable logic in Task 001; tool dispatch only binds/validates/calls it. Keep this and earlier scenario tests green.

### 3. Replay tool calls safely

**Behavior.** Lose publish acknowledgement and replay original tool call: same receipt/one publication. Another call with the name conflicts within the attempt; a fresh Agent attempt can reuse it. Forged tenant/producer fields refuse. Missing required report blocks its consumer; producer final output can still exist.

**RED.** Drive provider tool-call replay and inspect attempt-attributed entries; cancellation before acceptance must not return success. Expected RED: tool-call receipt replay is not wired through the Agent loop. Run [proof 3](#proof-3); names are planned until added/listed.

**GREEN.** Pass host-derived attempt/call identity to Task 001 and preserve typed refusal/receipt semantics through the Agent loop.

**REFACTOR.** Reuse the normal recorded-tool wrapper and backend replay key. Keep this and earlier scenario tests green.

### 4. Resolve selected hosted tools

**Behavior.** A reviewer receives only its selected hosted tools. Unknown tool names and provider-name collisions refuse before model use. Existing built-in declarations, permissions and records retain their behavior.

**RED.** Assert selected descriptors, zero model calls for rejected selections and existing scoped identities in grants/provenance. Expected RED: the new write/publish descriptors are absent from hosted selection. Run [proof 4](#proof-4); names are planned until added/listed.

**GREEN.** Extend the existing built-in catalog and hosted resolution for the new tools, preserving permission identity and collision checks.

**REFACTOR.** Keep ToolResolver/ToolRegistry and one Agent loop. Keep this and earlier scenario tests green.

### 5. Authorize every tool operation

**Behavior.** Remove invoke grant or underlying File permission between calls.
The next call refuses without source leakage and active work honors revocation;
accepted authority never expands on retry. This task closes current invoke/File
authority cases. Task 004 introduces repository/integration revocation and proves
its policy without new tools; Task 005, which depends on both tasks, closes the
real hosted-tool/repository combination after accepted PR evidence.

**RED.** Public negative journey uses explicit principals and checks exact denial plus protected operation count/content absence. Expected RED: the new hosted tools do not yet enforce their invocation/resource checks. Run [proof 5](#proof-5); names are planned until added/listed.

**GREEN.** Intersect accepted invoke/domain authority and current disablement at each protected operation; audit decisions non-blocking.

**REFACTOR.** Reuse existing resource permissions rather than tool-specific policy engines. Keep this and earlier scenario tests green.

### 6. Bound tool work

**Behavior.** Exceed call/result/cumulative budgets, delay/cancel a hosted call and page large output. Model messages remain bounded; another invocation progresses; persistent spend survives recovery and sanitized tool/schema/producer/digest/error provenance remains inspectable.

**RED.** Assert counters, message size/page cuts, competing invocation progress and absence of secrets in retained records. Expected RED: the new hosted tools have no integrated persistent invocation accounting. Run [proof 6](#proof-6); names are planned until added/listed.

**GREEN.** Use existing RecordedTool/cancellation and Task 001 budget/history owners; release unrelated capacity during waits.

**REFACTOR.** Share result limits and retries instead of per-source loops. Keep this and earlier scenario tests green.

### 7. Publish through direct and queued execution state

**Behavior.** Through the real Agent loop publish file/tree/JSON/null directly
and queued, consume accepted references in the next step with inventory lookup
unavailable, and replay a lost tool reply. Restore queued run state and inspect
the same manifests, exact bytes and receipt identities without PostgreSQL
publication payloads. Direct disconnect causes no later Agent dispatch.

**RED.** Assert real tool requests, existing {name,kind} response, recovered exact
descriptors and unchanged attempt scopes. Current tools are not connected to
shared recovered state. Run [lifecycle proof](#lifecycle-proof).

**GREEN.** Use existing shared File upload/completion, return its publication
information through hosted execution, and let the engine retain/checkpoint it
before tool success. Preserve current permissions, call identity and limits.

**REFACTOR.** Keep one Agent loop/ToolSession and upload owner; no stateful
publication service, new response shape or extra writer.

## Acceptance Criteria

| Obligation | Required proof |
|---|---|
| REQ-046–047/031 contribution; AC-039–040 | Dedicated fs.write, publish and replay journeys prove the actual Agent contract, arbitrary sources/names, durable receipt and downstream discovery; Task 001 owns backend proof. |
| REQ-021; INV-010; AC-018 | Hosted selection proof covers selected descriptors, unknown-name/collision refusal and preservation of existing built-in identity, scopes and records. |
| REQ-023; INV-002/003/006/007; AC-017 contribution | Per-call authority and limit proofs cover resource grants, source injection, server-only credentials, cancellation and sanitized attribution. Actual fork source acquisition is explicitly Task 005. |
| REQ-032 contribution | Mirror the write/publish/discovery/denial journey on Python/TS/CLI/MCP; inspect the same typed content and stable errors, not SDK-owned storage. |
| Revision-18 durability obligations in metadata; DUR consumer seam | Lifecycle proof closes this capability against the shared Bifrost state foundation with unchanged identities, bounds, current permissions and exact evidence. Task 001 owns loading, acceptance, replay and old-consumer replacement. |

## Implementation Reference

HostedTools/ToolSession and AgentExecutor own tool dispatch and authority.
Shared File completion produces exact descriptors. Task 001 owns accepting
those results into run state and recovering them through Bifrost. Wire these
whole workflows together without a stateful publication handler or new transport.

| Inspected owner / consumers | Reuse and precise gap |
|---|---|
| HostedTools::open/Invocation/descriptor; Workspace::writer/read; tools/pg_tests | Current hosted tools are built-in-only and require an input workspace for filesystem tools. Add write/publish and scratch-only sessions here; existing tests cover grant/domain/budget refusals. |
| AgentToolset::open; ToolSession::resolve; RecordedTool; AgentSpec::tool_names | Reuse the built-in selection and recording chain for the new hosted tools. Keep existing built-in identities; |
| PermissionScope::Tool(BuiltinTool); ToolCallRecord::tool; provider_tool_name | Extend the existing catalog and permission matching for the added built-ins; preserve wire behavior and provider-name collision refusal. No remote identity union is needed. |
| skald-tool::ToolRegistry/ToolResolver; skald-agent loop | Already resolves AgentTool values and rejects duplicate names. Reuse these owners, no new registry or Agent executor. |

## Verification and Evidence

All new names below are **planned**. Existing packages, targets, runners and Postgres setup are listed in [shared verification](verification.md). After adding a test, list/confirm its actual selector before RED; zero selected tests is not proof. The RED paragraph states the missing assertion, not a claim that a test has run. Use focused owner tests plus real public journeys; no credentialed provider or synthetic host load.

### Proof 1

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::agent_writes_private_workspace_files)"'
```

### Proof 2

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::agent_publishes_file_directory_and_json)"'
```

### Proof 3

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::agent_publish_retries_use_host_identity)"'
```

### Proof 4

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::selected_hosted_tools_resolve_before_model_calls)"'
```

### Proof 5

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::tool_calls_recheck_permissions_and_revocation)"'
```

### Proof 6

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=hosted_agent_tools::tool_limits_release_capacity_and_preserve_provenance)"'
```

### Lifecycle proof

New selector is **planned**. Use the existing target/setup below, add/list the actual
test before RED, and record a nonzero selection plus the missing-behavior failure and
closure result. Extend the task's existing Python/TS/MCP public-closure case table with
its recovery outcomes rather than adding full suites; Task 006 uses existing
combined-surface cases. No runtime proof is claimed during planning.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all --test-threads=1 -E "test(=agent_tools::publications_follow_shared_run_state)"'
```

### Public surface closure

Mirror write/publish/discovery, descriptor refusal and invoke/domain denial; include runtime file/directory/JSON readback. The Rust journey above covers the same server contract; add HTTP/OpenAPI assertions at existing route tests and CLI assertions through its in-process command surface. Planned language/MCP selectors:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_hosted_agent_tools.py::test_agent_publishes_file_directory_and_json'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/hosted-agent-tools.test.ts -t "^agent_publishes_file_directory_and_json$"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all --test-threads=1 -E "test(=verification::pg_tests::hosted_agent_tools_agent_publishes_file_directory_and_json)"'
```

Apply only the [shared owner checks](verification.md#owner-checks) for this write set; record real command/results during implementation.

## Material Stop Conditions

No second storage/snapshot/replay implementation, Agent loop or policy engine. Preserve built-in wire behavior. Remote MCP sources, connections, transports and permission identities are outside this change.

## Authority Links

[Approved spec, revision 18](../spec.md) · [Durability decisions 1–5](../execution-state-and-durability.md) · [Task ownership/dependencies](README.md) ·
[Publication contract](../output-publication.md) · [PR
extraction](../pull-request-extraction.md) ·
[Orchestration](../example-orchestration.md).

[Wyrd design](../../../../architecture/wyrd-design.md) · [Bifrost design](../../../../architecture/bifrost-design.md) · [Security](../../../../architecture/wyrd-security-posture.md) · [Implementation execution](../../../implementation-execution.md) · [Testing](../../../../TESTING.md). Apply nested AGENTS.md at each implementation owner.
