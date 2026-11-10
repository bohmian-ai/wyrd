---
id: TASK-002
spec: SPEC-verifier-review-pipelines
spec_revision: 18
status: complete
previous_id: TASK-003
supplement: SUPPLEMENT-verifier-execution-state-and-durability@2
depends_on: [TASK-001-R4]
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

Each scenario is a user journey. The cases beneath it are verification work,
not additional implementation phases. Task 001 remains the state/storage owner.

### Scenario 1 — An Agent publishes work for a downstream step

**Behavior.** A developer selects the hosted write/publish tools for a Verifier.
Its Agent writes and revises a report in private scratch, publishes it under an
arbitrary logical name, and a downstream Agent discovers and reads the exact
accepted content. Exercise file, nested directory, JSON and explicit null
variants through the real Agent loop. The receipt remains `{name,kind}`; later
scratch changes cannot alter the publication. Final output alone creates no
publication, and standalone code receives no write/publish tools.

| Test case within this journey | Expected observation and focused proof |
| --- | --- |
| Scratch-only Agent, overwrite/readback, isolated attempts; `/inputs`, unsafe paths/parents or excessive bytes | Safe private writes succeed; invalid writes leave forbidden locations untouched. [Workspace cases](#proof-1). |
| File/tree/JSON/null; missing/both sources, invalid name, missing/foreign path; empty directories and arbitrary filenames | Exact immutable content and existing typed refusals; downstream discovers actual paths rather than a fabricated report filename. [Publication cases](#proof-2). |
| Lost reply and original-call replay; another call reuses a name; fresh attempt reuses the name; forged producer identity | One original receipt, same-attempt conflict, separate attempt attribution and host-owned identity. Cancellation before acceptance returns no success; a missing required publication blocks its consumer. [Replay cases](#proof-3). |
| Selected tools, unknown names, provider-name collisions and existing built-ins | Only permitted descriptors reach the model; invalid selection dispatches no model call. [Selection cases](#proof-4). |
| Initially narrowed/delegated authority; repeated protected calls; execution cancellation | Every call checks its invoke and underlying domain grant against the accepted execution authority; a missing grant refuses without protected content. Later role-grant changes affect subsequent issuance, not the accepted snapshot. Cancellation prevents later publication acceptance and new Agent work. [Permission cases](#proof-5). Principal-revocation cancellation follows the existing security owner; this journey does not claim to revoke a principal or credential. Repository/integration policy belongs to Task 004; Task 005 closes its real-tool combination. |
| Call/result/cumulative limits, delayed/cancelled calls and large responses | Bounded messages/work, competing invocation progress, conserved spend and sanitized provenance. [Limit cases](#proof-6). |
| Direct and queued runs, unavailable inventory lookup, lost tool reply, queued recovery and direct disconnect | Accepted references feed successors and survive restoration without rediscovery; disconnect starts no later Agent work. [State integration cases](#lifecycle-proof). |

**RED.** Extend the [public publication journey](#proof-2) to cover the complete
write → publish → downstream-consume flow against a real server, with the
supporting cases above at their existing test tiers. Missing tool selection,
write/publish dispatch or successor content makes the journey fail. Confirm
planned selectors before RED; Task 001's existing proofs are regression evidence,
not a reason to manufacture another foundation failure.

**GREEN.** Extend existing hosted descriptors/selection and safe workspace writing;
wire original call/attempt identity and shared upload results into Task 001's
accepted run state. Await its checkpoint acknowledgement before the existing tool
receipt. Preserve current permission checks, budgets and normal recorded-tool flow.

**REFACTOR.** Keep one Agent loop, ToolSession, upload lifecycle and shared writer.
Remove repeated glue while the journey and its negative cases remain green;
the Agent and publication handler gain no independent verification state.

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

Proof numbers identify supporting tests, not additional implementation scenarios.
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

## Implementation Evidence

Status: **IMPLEMENTED**. The candidate is commits `973b78e5e..75c9bfa9e` on
`task/verifier-review-pipelines/TASK-002`. The Rust journey command is the Proof
command above with `-E "test(=hosted_agent_tools::<name>)"`. A final run of all
seven was `test(/^hosted_agent_tools::/)`: 7 passed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-046–047/031; AC-039–040: fs.write over private scratch | `wyrd-server` `verification/hosted/{tools,workspace}.rs` (`Builtin` fs.write, `Workspace` writer and byte budget, scratch for each invocation attempt); `BuiltinTool::FsWrite`/`writes_scratch` in `wyrd-spec` `card/agent.rs` | Proof 1 `hosted_agent_tools::agent_writes_private_workspace_files` PASS. Unit `workspace_writes_private_scratch` covers over-budget writes and unsafe parents. | PASS |
| REQ-046–047/031; AC-039–040: publish file, tree, JSON, and null; typed refusals; downstream discovery | `Builtin` publish dispatches to Task 001 `Publications` and awaits acceptance; the receipt is `{name,kind}`; fixtures in `fixtures/cards/hosted_agent_tools/` | Proof 2 `hosted_agent_tools::agent_publishes_file_directory_and_json` PASS (empty dirs through the manifest `directories`; Unicode and spaced filenames; later scratch mutation is not visible downstream) | PASS |
| Replay keyed by call ID; fresh attempt; forged producer; cancellation; missing publication blocks the consumer | `AgentTool::invoke_call(call_id, ..)` in skald; `BlockCall.attempt` → `Producer` (`vala-verify` `blocks/agent.rs` `Role::Step`); the host derives the producer | Proof 3 `hosted_agent_tools::agent_publish_retries_use_host_identity` PASS | PASS |
| REQ-021; INV-010; AC-018: selection, unknown names, collisions, repeated names, existing built-ins | `AgentToolset::open`/`ToolSession::resolve`; validate.rs refuses unknown and repeated names (`"{name} is selected more than once"`) | Proof 4 `hosted_agent_tools::selected_hosted_tools_resolve_before_model_calls` PASS (`WYRD_LOADER_400_INVALID_ENVELOPE` with an `agent_tools` diagnostic; no model request) | PASS |
| REQ-023; INV-002/003/006/007; AC-017: per-call authority, narrowed/delegated authority, cancellation, no leakage | Each call is checked for `tool:invoke` on its exact built-in plus the domain grant against the accepted authority | Proof 5 `hosted_agent_tools::tool_calls_recheck_permissions_and_revocation` PASS; `tools/pg_tests.rs` unit grants and forged arguments | PASS |
| Limits and provenance: paged large results, released capacity, sanitized records | `RecordedTool`/`ToolCallRecord` (digest, bytes, error, producer indices/attempt) | Proof 6 `hosted_agent_tools::tool_limits_release_capacity_and_preserve_provenance` PASS | PASS |
| Lifecycle: direct and queued runs, restart recovery, lost reply, direct disconnect | Task 001 run state consumed unchanged; consumer step `retries: 1` in the fixture | `hosted_agent_tools::publications_follow_shared_run_state` PASS (restart feeds the consumer without rerunning the drafter; a dropped direct verify made exactly 2 drafter requests in a 5s window) | PASS |
| REQ-032 contribution: Python/TS/MCP mirrors; HTTP/OpenAPI/CLI | `test_hosted_agent_tools.py`; `hosted-agent-tools.test.ts`; `wyrd-mcp` `verification::pg_tests::hosted_agent_tools_agent_publishes_file_directory_and_json` | Python: 2 passed (`test_agent_publishes_file_directory_and_json`, `test_tool_calls_need_their_own_grant`). TS: 1 passed. MCP: 1 passed. Existing HTTP/OpenAPI coverage: `pg_openapi_contract.rs`, `pg_verification_routes.rs`, `pg_durable_verifier_work.rs`. Existing CLI coverage: `durable_verifier_work.rs::execution_pages_are_stable_and_tenant_scoped` | PASS |
| Docs and contracts | `architecture/wyrd-design.md` `tool_names`; `docs/.../agent-tools-and-authorization.md`; regenerated `llms-full.txt` and TS `card-types.ts`; authored `.pyi` stub | `mise run codegen:check` 0 | PASS |

Owner checks: `mise run fmt` and `mise run lints` 0; `mise run codegen:check`,
`check:deps`, and `check:tenant-isolation` 0; `cargo nextest run -p vala-verify
--lib` passed 60/60; `py:lints` and `py:typecheck` 0; `ts:format:check`,
`ts:lints`, and `ts:typecheck` 0; `git diff --check` clean.

Deviations and limits:

- **Selector.** The Lifecycle selector is
  `hosted_agent_tools::publications_follow_shared_run_state`, not the planned
  `agent_tools::`. That test was added to the `hosted_agent_tools` journey file
  together with Proofs 1–6.
- **TDD order.** The core production change (`973b78e5e`) was written before
  the journeys. The journeys were then written against it, and each was
  confirmed to select one test.
- **Contract change.** `code.run` now gets private scratch without a declared
  workspace, like `fs.write`. Reading tools can read a scratch writer's files.
  The validate.rs message is now "a workspace requires an fs.*, code.run, or
  publish tool".
- **Revocation limit.** A grant removed while a delivery is in flight is not
  rechecked live. Each call is bounded by the authority accepted for the
  delivery, and revocation is proved through cancellation and
  narrowed/delegated authority. No new mechanism was invented.
- **Unit-level proofs only.** Over-budget writes and the `/work` byte budget
  are proved by unit tests only.
- **Code actions.** Standalone code actions get no tools by construction, so
  they receive neither write nor publish.

Non-goals stayed excluded: no remote MCP source, transport, or permission
identity; no second snapshot, store, or replay; no new Agent loop, registry, or
policy engine. No unrelated files changed.

Remediation R1 (review `task-002-75c9bfa9e`): the lifecycle row above relied on a
5s-window SDK observation and private state; that proof now lives at the server
tier (`pg_durable_verifier_work::{agent_publications_reach_a_recovered_consumer,
dropped_agent_work_makes_no_later_call}`) and the SDK journey keeps public
outcomes only. The `/work` budget is now also proved in the real Agent loop. The
Revocation limit wording is routed to `$wyrd-plan` as a task correction. The
original TDD-order admission stands. Evidence:
`review/task-002-75c9bfa9e/TASK-002-R1-publication-integrity-and-proof.md#remediation-evidence`.

## Planning authority correction — 2026-11-08

`$wyrd-plan` corrects the permission scenario above under the existing approved
security authority: [accepted execution authority](../../../../architecture/wyrd-security-posture.md)
is not refreshed by later grant changes; each protected call checks its own
permission against that snapshot. Role-grant changes affect subsequent issuance.
Principal revocation cancels unfinished executions through the existing owner;
explicit cancellation and initially narrowed/delegated authority are the
observations supplied by Proof 5, not a principal/credential revocation test.
The earlier sentence promised immediate refusal after removing an Invoke/File
grant between calls; that private instruction contradicted the security owner
and is superseded by the corrected scenario. No live refresh or new permission
identity is authorized, and the approved specification is unchanged.

This is a documentation-only task correction with static authority comparison;
TDD does not apply. Existing permission/cancellation assertions remain the proof
of the scoped runtime behavior. Historical implementation evidence, the original
production-before-RED admission and the first review's verdict/findings remain
preserved; their prior all-PASS labels do not substitute for independent review.
