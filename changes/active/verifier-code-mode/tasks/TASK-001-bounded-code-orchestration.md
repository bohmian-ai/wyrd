---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-verifier-code-mode
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014, AC-015, AC-016, AC-017, AC-018]
depends_on: []
parent_task:
remediates: []
---

# Bounded tool orchestration inside Agent Python

## Outcome and Value

An Agent can read and reduce evidence with its declared tools inside one
`code.run` program. Today a model must receive each intermediate tool result
before requesting the next operation. Afterwards Python can await dependent
reads and return only relevant findings, while Wyrd retains every admitted
call and validates the final judgment against original evidence.

This task delivers the complete server behavior, wire contract, shared Rust
public-client proof, and governing architecture corrections. TASK-002 depends
on that behavior to complete the other public-language and MCP journeys and
public guidance. Both tasks are necessary for change completion; neither task
file requires the author conversation.

## What Changes and What Must Stay True

Enable `await call_tool(name, arguments)` only in generated Agent `code.run`.
The name is an exact dotted declared built-in other than `code.run`; arguments
are one JSON object. Keep existing provider-safe names, direct tools, Card
fields, request/response shapes and computation-only Python behavior. Reuse
the resolved descriptions and schemas, adding bridge guidance to `code.run`.
Authored code actions/checks acquire no bridge.

The parent server owns dispatch, identity, permissions, IO and recording;
Monty only suspends, transports values and resumes. Both origins pass through
one declared-tool execution/recording path. The outer call and every child
need their own exact tool permission, followed by ordinary capability
permissions under accepted tenant/delegation authority. A nested Bifrost query
still uses `BoundedQuery`, Oracle admission, its own request ID, read-only SQL,
per-table permission, read audit and terminal-safe collection. No partial rows
become success. Audit-outbox failure never changes the permission outcome.

The public and persisted changes are fixed:

| Contract | Required semantics |
| --- | --- |
| `ToolCallRecord.recipient` | Closed wire values `model` and `code`; absent historical value defaults to `model`; newly emitted records include it. Outer `code.run` is `model`, children are `code`. |
| `RequirementResult.tool_calls` | Default-empty record list with the same element contract as step calls; non-Agent checks have none. Preserve it through custom-data decoding. |
| Call results and records | Flat invocation-start order. Digest and byte count describe compact bounded JSON actually delivered to the recipient. No returned value means no digest, zero bytes and a safe error/cancellation. No raw result retention or automatic intermediate model messages. |
| Target and input rejection | Unknown/undeclared/recursive targets use `WYRD_AGENT_404_TOOL_NOT_IN_AGENT`; invalid/oversized inputs use `WYRD_TOOL_422_INPUT`. No target dispatch or leaf record; each request still spends budget. |
| Ordinary tool failures | Catchable Python `RuntimeError` containing the compact existing catalog problem JSON, retaining safe details and code. Uncaught execution errors fail a step or make a check inconclusive. |
| Host stops | Deadline, cancellation, exhausted budget and Busy cannot be caught to continue Python/model work or manufacture a judgment. Busy keeps immediate refusal and queued deferral without consuming an attempt. |
| Startup limits | `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS=128`; `WYRD_VERIFICATION_TOOL_ARGUMENT_MAX_KIB=64`. Positive checked integers only; empty/zero/negative/malformed/scaled-overflow values refuse readiness naming the setting. No Card/TOML override or disabling sentinel. |

The logical invocation includes authored retries. Its request budget counts
the outer code call, direct calls and nested requests including malformed,
denied and failed requests, atomically before dispatch, without refunds.
Spending exactly the last allowance can succeed and allow a final answer;
only a subsequent request terminates work with
`WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED`. Never increment beyond the limit
or start more tool/model work after that refusal. A new execution is fresh.

The effective existing concurrency cap (default 8) covers active non-code
operations across both origins. Code holds only its worker capacity, never a
permit needed by its children. Waiting is bounded by call allowance/deadline;
cap 1 must complete. Worker acquisition, compilation, workspace coordination,
Python, host waits and resumption consume wall-clock operation/cumulative
Python time; retries restore neither time nor request allowance.

Returning, failing, cancellation, timeout and shutdown stop dispatch, cancel
or settle host work, reap/settle the worker and release the session. An already
started blocking extraction may finish its bounded operation, but cleanup
must await it before releasing the workspace and it cannot publish a late
result. Workspace mutations serialize, preserve no-overwrite, share scratch
and write limits between Python/extraction, and become visible after await.
Do not retain the current whole-program workspace writer lock during a child
operation needing that lock. There is no rollback or concurrent-read snapshot.

Keep completed, denied, caught-error and cancelled-after-admission records
across retries and failed/inconclusive results, including rejected Verdicts
and citations. Existing paths producing no result gain no durable journal.
Result capture and sensitive-result permissions must continue protecting
arguments; disabling outcome capture must not leak them on another surface.
Only original reachable pinned evidence supports citations, never scratch or
live Bifrost reads promoted to pinned evidence.

A worker remains occupied while awaiting tools, and each child still incurs
normal authorization/audit/recording costs. No new tool service, transport,
mode flag, runtime, package access, OS-specific sandbox, network access,
call-tree IDs, intermediate store, migration, or Bifrost mechanism change.
Older workers cannot execute bridge programs; require upgraded workers and
retain existing manifest compatibility/downgrade rules.

## Approach

1. Extend the existing record/error contracts and startup limits, update Rust
   constructors/consumers, and reconcile the owning architecture/security text
   before or with the governed behavior.
2. Extend the invocation/session execution path so direct and nested requests
   share declaration resolution, bounds, stop signals, recording and effective
   concurrency; retain invocation accounting across authored attempts.
3. Drive the existing Monty host-function/future events for the single bridge,
   with bounded lossless JSON conversion and safe catalog error projection.
4. Adapt workspace coordination and cancellation propagation so suspended
   programs can invoke children without deadlock, detached work or late results.
5. Carry records through action/check success, errors, retry and result capture;
   preserve historical decoding and original-evidence validation.
6. Extend existing real-worker, accepted-authority and public-client coverage,
   then run the focused checks below. Keep private APIs and fixtures local to
   the existing owners; do not introduce replacement orchestration.

## Ordered Implementation Scenarios

Each scenario describes a user outcome. Its case table and linked proofs are
supporting checks, not separate implementation tasks or a requirement to put
every case in one test. Work through each scenario in small TDD increments:
select one missing observation, confirm its actual focused selector, obtain
behavioral RED, implement GREEN, then refactor with affected earlier checks
still green. Do not write every test in a scenario before implementing it.
A compile/setup failure is not RED evidence. Keep distinct assertions and
failure messages for each case, even when existing tests share setup.

### Scenario 1 — An ML engineer reduces evidence and receives a supported judgment

**Behavior.** An ML engineer runs a Verifier whose Agent preparation step and
check read declared evidence through Python, combine dependent results and
return selected findings. The model makes the final supported judgment without
receiving intermediate leaf results. The engineer can inspect complete,
recipient-tagged evidence through the real Rust SDK, including historical
results and custom Verdict data.

| Case | Expected observation |
| --- | --- |
| Dependent orchestration | Three dependent operations consume earlier results without an intermediate model turn; the preparation step returns declared data and the check returns the declared Verdict/data shape with valid original-evidence citations. |
| Delivered values | JSON null, booleans, Unicode, nested collections, wire-range integers and finite numbers survive conversion exactly. Digests/byte counts describe compact bounded JSON actually delivered, not raw source data. Oversized returns fail without partial data; outer printed/value output keeps its existing explicit truncation markers. |
| Honest records and visibility | Outer `code.run` records have recipient `model`; child records have recipient `code`. Flat records keep invocation start order, once per admitted call, without raw output retention or automatic printing/replay of child results into later model messages. |
| Result compatibility and capture | Historical absent recipient decodes as `model`, absent check calls as empty; new records explicitly include recipient and survive round-trip/custom-data decoding. Existing result bytes/digests are not rewritten. Immediate and retained results preserve records; disabling outcome capture does not expose arguments elsewhere. |
| Rejected judgment or later program failure | Invalid/oversized Verdict or custom data and fabricated/scratch citations cannot pass. Valid pinned citations survive reduction; live Bifrost reads do not become pinned evidence. Completed records survive a later failure, retries and inconclusive output/citation rejection. A no-value call has no digest, zero bytes and a safe error. Revisit these readback assertions as later failure/retry cases are implemented. |
| Concrete reduction comparison | For the same fixed 20 independently readable text results of at least 4 KiB, direct reads and one Python reduction produce the same supported judgment and leaf evidence. The code-mode reduction has one outer model-visible request/response; combined compact UTF-8 JSON tool request/result bytes, including generated code, are at most 25% of direct. Record provider tokens only if available; infer no token threshold. |

**RED.** Start [public reduction proof](#public-reduction-proof) with the
smallest real step/check program making a dependent declared call; the current
sandbox refuses host calls/futures. Use [pure conversion proof](#pure-conversion-proof)
for exact supported values and size boundaries and [record contract proof](#record-contract-proof)
for historical/new/typed cases as each observation is added. Extend that same
public journey for the reduction comparison and retained readback, and use
[failed record proof](#failed-record-proof) for engine settlement/capture
observations. Current checks discard records and failed attempts lose them.
Do not require all failure cases to be implemented before the first happy-path
GREEN; complete them alongside the later scenarios that introduce those paths.

**GREEN.** Extend the existing session's protected recorded executor and
Monty suspension/resumption to complete the first dependent call. Add the two
additive fields and closed recipient contract at `wyrd-spec`, update affected
Rust constructors/re-exports and carry records through existing Agent/action/
check/engine/result and custom-data decoding paths. Extend the same user
journey incrementally to prove retained readback, validated citations and the
measured reduction; inspect actual scripted-provider messages to prove no
child replay. Regenerate consumed artifacts from their owners to keep the
tree buildable; TASK-002 owns language-facing projection closure, not
permission to leave broken generated consumers here. Rerun affected earlier
checks as each case is completed.

**REFACTOR.** Reuse existing descriptors, flat step-call representation,
result/citation validators and story/provider support. Share conversion and
record propagation within existing owners; add no per-tool Python wrappers,
separate check-record type, benchmark framework, intermediate store or new
transport. Keep private wiring and fixture structure implementation-owned.

### Scenario 2 — A platform engineer recovers from a denied call without widening access

**Behavior.** A platform engineer runs the same declared program with a
missing permission, diagnoses its safe catalog error and reruns with the
required authority. Python may catch an ordinary error and make another
allowed call, but neither direct nor nested execution can escape the Agent's
declaration, accepted tenant or delegated authority.

| Case | Expected observation |
| --- | --- |
| Tool and downstream grants | Vary exact tool and data permission independently for direct and nested calls. Both are required; narrow delegated grants stay narrow; foreign and unknown targets retain existing indistinguishability and disclose no protected value. Each actual check keeps normal attribution/audit, even if the audit outbox fails. |
| Accepted authority after recovery | Queued/recovered execution uses the originally accepted authority and delegation; later grant changes do not refresh it. Python arguments/state cannot select caller, tenant, credentials or fallback identity. |
| Repairable error | A caught ordinary failure is a `RuntimeError` containing parseable safe compact catalog problem JSON and no successful value; the later allowed call succeeds and both admitted calls are recorded. Uncaught failure fails the step or makes the check inconclusive, without erasing earlier records or falsely failing the subject. |
| Unavailable target or invalid input | Unknown/undeclared names, recursive `code.run`, extra identity fields, unsupported/cyclic values, non-string keys and non-finite/out-of-range numbers dispatch no capability. Bridge lookup/input failures use the specified safe codes and no leaf record while spending request budget. Arbitrary uninstalled callbacks keep ordinary interpreter rejection; assert no host dispatch rather than inventing a bridge catalog code. |
| Existing capability/isolation regression | Direct descriptors/contracts and computation-only programs remain valid; an Agent declaring only `code.run` gains no undeclared tool. Authored code has no bridge, host escape or new network/package/credential access. |

**RED.** Extend [accepted-authority proof](#accepted-authority-proof) one
nested grant/recovery or caught/uncaught case at a time, and
[public authority proof](#public-authority-proof) for exposed denial and
recovery. Use [bridge boundary proof](#bridge-boundary-proof) for actual-worker
target/input/isolation rejection and the invalid cases of
[pure conversion proof](#pure-conversion-proof) at the unit tier. Reuse
unchanged queue/audit regression evidence, adding a nested assertion wherever
the bridge changes its path. Rerun Scenario 1's affected success/readback proof.

**GREEN.** Bind children only to the session's resolved catalog and the same
exact tool authorization, typed argument parsing and capability owners under
accepted authority. Preserve typed causes and safe catalog problems, lossless
bounded conversion and ordinary authored-code refusal. Never give a worker
credentials, registry access or server state. Extend record/error propagation
only as required for these observations and rerun earlier affected cases.

**REFACTOR.** Delete bridge-specific duplicate permission, lookup or error
mapping. Keep one declared-tool workflow and reuse the current descriptors
and capability owners; add no alternate authorization path.

### Scenario 3 — An engineer processes evidence within the host's resource limits

**Behavior.** An engineer runs a program that reads and extracts evidence,
including concurrent requests, under operator-configured limits. Direct and
Python-originated work shares one allowance, completes at concurrency cap 1,
and can read awaited extraction output without deadlock or overwrite.

| Case | Expected observation |
| --- | --- |
| Startup configuration | Unset settings use 128 requests and 64 KiB arguments; lowered positive settings apply. Empty, zero, negative, malformed and scaled-overflow values refuse startup naming the setting. The argument ceiling covers direct calls including `code.run` source; the result default stays 64 KiB. No Card override or disabling sentinel. |
| Call budget and retry | Mix direct, outer code, nested, malformed, denied and failed requests. Atomic reservation counts each once without refund or retry replenishment. Exactly the final allowance may still produce a final answer. A subsequent request terminates with the catalog exhaustion code, cannot be caught, never spends beyond the ceiling and starts no later tool/model work. A new execution starts fresh. |
| Bounded arguments | Oversized arguments dispatch nothing and cannot allocate or retain an unbounded argument record. Conversion and result limits remain effective for both origins. |
| Concurrent dependent reads | At effective caps 1 and 2, active non-code operations across direct calls and concurrent Python futures never exceed the shared cap; cap 1 completes. Waiting is bounded by remaining requests/deadline. Out-of-order completion returns each value to the right future and preserves record start order; each Bifrost query has its own request ID. |
| Extraction and scratch writes | Python awaits extraction then reads the completed result. Conflicting writes/extractions cannot overwrite; Python and extraction share scratch/file-write limits; unselected inputs stay inaccessible. Failed partial writes retain only invocation lifetime. Await gives visibility; independent concurrent reads gain no snapshot or rollback promise. |

**RED.** Begin with [startup proof](#startup-proof) for a lowered valid or
invalid new setting, then add one user-visible bound at a time using
[budget proof](#budget-proof), the budget phase of
[public failure proof](#public-failure-proof),
[concurrency proof](#concurrency-proof) and
[workspace bridge proof](#workspace-bridge-proof). Use deterministic barriers
and active-operation observations for concurrency; rerun
[workspace regression proof](#workspace-regression-proof). Current startup
ignores the new settings, direct-loop buffering cannot bound children, and
the whole-operation workspace writer ownership would deadlock extraction.

**GREEN.** Extend `HostedLimits` and checked environment parsing/test-server
consumers, existing retry-spanning invocation accounting and the before-model
stop boundary. Enforce the effective non-code cap at the shared operation
boundary, separately from worker capacity; preserve future association.
Adapt existing workspace/mount coordination for shared mutation accounting,
never holding a permit or lock required by a suspended program's children.
Rerun the authority, conversion, dependent-call and readback cases affected by
each increment. Do not refund failures or add another nested allowance.

**REFACTOR.** Reuse current unit scaling/validation, invocation ownership,
workspace, extractor and no-overwrite behavior. Keep one accounting owner per
logical invocation, with no second parser, copied workspace, rollback layer
or alternative filesystem service.

### Scenario 4 — An operator interrupts or defers work and the next invocation succeeds

**Behavior.** An operator cancels or times out a running program, shuts down
its runtime, or encounters exhausted process capacity. Wyrd exposes the honest
execution outcome, settles its work and resources, then permits a fresh
authorized invocation to succeed without prior Python or tenant state.

| Case | Expected observation |
| --- | --- |
| Query or pending-future interruption | Deterministically interrupt a slow query or unresolved Python futures. Cancellation propagates; no later calls dispatch or late results publish. Host stops cannot be caught to continue or manufacture a judgment. |
| Time spent waiting and retry | Worker acquisition, compilation, workspace coordination, Python, host waits and resumption consume operation and cumulative wall-clock Python budgets. Exhaust the budget mainly in host waits and retry; spent time is not restored. |
| Blocking extraction interruption | Already-started extraction may finish its bounded operation, but cleanup awaits settlement before releasing the workspace. It cannot outlive cleanup or publish a late result. |
| Return, failure and shutdown | Every exit settles/cancels outstanding host work, settles/reaps workers and releases session resources. No detached task or owning-reference cycle remains; later work has fresh capacity/state. |
| Capacity refusal and recovery | Refuse worker/workspace or downstream process capacity, including inside Python that catches ordinary errors. Immediate execution stays unavailable; queued execution defers without consuming an authored attempt. Capacity recovery permits success and does not shut down healthy sibling work. Busy is not a catchable successful tool value. |
| Result/accountability after interruption | A cancelled-after-admission call has no digest, zero bytes and a safe cancellation record whenever a result is produced. Earlier admitted records survive produced failed/inconclusive results and retries. Existing no-result cancellation gets no durable journal guarantee; execution failure never falsely judges the subject failed. |

**RED.** Extend [public failure proof](#public-failure-proof) with a pending-call
cancellation followed by authorized success, and obtain focused RED for each
missing lifecycle observation through
[host-wait cleanup proof](#host-wait-cleanup-proof),
[extraction cleanup proof](#extraction-cleanup-proof) and
[shutdown proof](#shutdown-proof). Extend [capacity proof](#capacity-proof)
and [queued capacity proof](#queued-capacity-proof) for nested refusal,
caught-error attempts and pending siblings. Revisit
[failed record proof](#failed-record-proof) for cancellation/readback.
Current queries create an uncancelled token and in-flight records are dropped;
the existing Busy mechanism must also reach callback/future execution.

**GREEN.** Propagate actual invocation stop/deadline through existing host
owners, settle outstanding work before releasing resources, retain applicable
records and charge wall-clock time across retries. Preserve the existing
session refusal and engine/queue Busy behavior through nested calls and
settle their siblings. Rerun Scenario 3's budget/workspace/concurrency proofs
and earlier public result/authority observations after each change.

**REFACTOR.** Consolidate cleanup and refusal under existing session/pool
and queue ownership, with no detached work, new lifecycle manager, retry queue
or health-state owner. Keep resource recovery proof separate from each fault's
critical assertion so a failure remains diagnosable.

## Acceptance Criteria

| Spec obligations | Owner and proof | Required observation / violation |
| --- | --- | --- |
| REQ-001–004; AC-001–003, AC-015–016 | Hosted bridge, real-worker boundary, Rust public reduction | Dependent declared calls work in steps/checks with unchanged direct catalog; undeclared/recursive/authored-code escape dispatches nothing. |
| REQ-005, REQ-008; AC-003, AC-005, AC-011 | Boundary/conversion and accepted-authority cases | Lossless supported JSON; bounded rejection before execution; safe catchable ordinary catalog errors; oversized return is never partial success. |
| REQ-006–007; INV-001, INV-004; AC-004, AC-017 | Existing dispatcher/capability owners, accepted-authority and public denial cases | Exact tool plus domain grants, tenant/delegation/recovery and non-blocking audit unchanged; Python identity selection or authority refresh is a violation. |
| REQ-009–010; AC-006, AC-010, AC-014, AC-018 | Budget/startup, Busy and public failure cases | Atomic retry-spanning spending, successful final allowance, noncatchable exhaustion/Busy and honest status. No extra model work or spent queued attempt. |
| REQ-011; AC-007 | Shared concurrency case | Active non-code operations stay at cap; cap 1 completes; distinct query request IDs, correct future values/start order. |
| REQ-012; AC-006, AC-009–010, AC-018 | Cleanup, extraction, shutdown and subsequent public success | Wall-clock waits charged across retries, no detached work/late results, settled worker/workspace and new invocation isolation. |
| REQ-013; AC-008, AC-011 | Workspace bridge and existing writer regression | Shared no-overwrite/write/scratch limits, awaited visibility and no lock recursion; no new rollback guarantee. |
| REQ-014–017; INV-006; AC-005, AC-009, AC-012–013, AC-016 | Contract/default, record failure, result capture and public readback | Honest recipients; complete bounded admitted-call records on produced results, including failed steps/inconclusive checks/retries. Capture disabled does not leak arguments. No raw intermediate result retention. TASK-002 completes other-surface projections. |
| REQ-018; AC-001, AC-011, AC-016 | Real SDK reduction/citation cases | Valid original-evidence citations support judgments; fabricated/scratch/live-query-as-pinned evidence cannot pass. |
| INV-002–003, INV-005; AC-003, AC-009–010, AC-015 | Existing worker/isolation regressions plus bridge negative cases and diff review | Same portable pool/allocator/mount boundary, no credentials or raw host capabilities in worker, no second tool/runtime owner. |

## Implementation Reference

These are inspected extension seams, not a file/type allowlist. Revalidate
current symbols before adding machinery.

- `crates/vala/vala-verify/src/blocks/tools.rs`: `AgentToolset::open`,
  `ToolSession::resolve`, `RecordedTool::invoke`, `finished` already connect
  declaration-only registry, recording and capacity refusal. `finished` drops
  in-flight slots and `RecordedTool` is model-only; extend this common path
  for both origins, bounded arguments, recipients and stopped-call completion.
- `blocks/agent.rs::AgentExecutor::invoke` resolves declarations, binds tools
  and races `ToolSession` refusal; `ActionExecutor::execute` carries successful
  records, while `CheckExecutor::execute` discards them. Extend this workflow,
  including output/citation failures, rather than invoking host tools directly
  from Monty around the recording wrapper.
- `blocks.rs::{ActionOutput, CheckOutput, BlockError}` and
  `blocks/engine.rs::{VerificationEngine::attempt, Attempted::settle, Judged::report}`:
  retries recreate calls; failed actions currently settle with an empty list
  and checks have no record field. Extend the existing completion/error seam
  to preserve records across attempts and produced failed/inconclusive results.
  Inspect all constructors, dispatchers and test executors when changing it.
- `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs`:
  `HostedTools::open` binds declared tools to accepted caller/workspace;
  `Invocation::call` authorizes exact tool invocation before typed arguments
  and capability IO. Reuse `BoundedQuery` and the Card readers. BifrostQuery
  currently uses a new uncancelled token; propagate actual cancellation.
  `Invocation::run` holds `Workspace::writer` across the complete operation;
  nested extraction needs that ownership corrected. `descriptor` already
  reuses MCP Card/Bifrost catalogs; extend `code.run` guidance there.
- `hosted/sandbox.rs::{Sandbox::exec, Sandbox::run, CodeExecutor}`:
  the existing event loop resumes mounted OS calls but rejects `FunctionCall`
  and `ResolveFutures`. Use existing checkout/resume events for the bridge;
  preserve no-bridge authored code execution and one checkout per operation.
  `hosted/worker.rs::dispatch` / protocol loop and allocator remain the worker
  owner, not a place for permissions or server credentials.
- `hosted/workspace.rs::{Workspaces::open, Workspace::writer, mounts, extract}`
  already owns admission, selected inputs, scratch limits and extraction.
  Extend shared mutation ownership and cleanup rather than copying mounts.
- `crates/skald/skald-agent/src/loop_runtime.rs` buffers direct tool calls
  with the effective cap. That does not bound children; the verification
  session needs the shared non-code boundary. Change generic Skald behavior
  only if necessary to preserve existing direct behavior under the spec;
  Skald must never depend on Vala or server types.
- `crates/wyrd-spec/src/verification/execution.rs::{ToolCallRecord,
  RequirementResult, RequirementResult::decode_data}` and `src/error.rs`
  own additive fields/defaults/catalog exhaustion. Update all Rust literal
  constructors and existing client/Rust SDK re-exports as needed. Runtime
  schemas and TypeScript wire/error artifacts derive here, not in SDKs.
- `hosted/mod.rs::HostedLimits`, `config.rs::VerificationConfig::apply_limits_env`
  and `set_scaled` own defaults/checked startup parsing.
  `crates/wyrd/wyrd-testing/src/server.rs::WyrdTestServerBuilder` already
  exposes verification limits/runtime/worker overrides; extend only as needed.
- `src/verification/results.rs::StoredOutput` serializes existing requirement/
  step objects into execution outputs. Keep that canonical result reader/write
  flow and its sensitive outcome capture; no storage table/schema migration.
- Existing tests inspected: server `sandbox_worker.rs` exercises actual worker
  executable provisioning, mounts, write budget, runaway code, capacity and
  shutdown; `pg_hosted_artifacts.rs` exercises real accepted-authority tool IO,
  queued Busy and Artifact evidence. Rust SDK `tests/integration/
  verifier_building_blocks.rs` drives `PolicyDesk`/`Deployment`, checked-in
  story Cards, scripted local model responses and typed SDK results. Extend
  these owners; add a sibling module only if required for maintainable size.
- Reconcile `architecture/wyrd-design.md` Verifier/Agents-tools-workspaces
  and `architecture/wyrd-security-posture.md` accepted authority as required
  by the spec, including replacement of credential-switching/OS-enforcement
  prose with the portable boundary. No network privilege is granted to Monty.

## Verification and Evidence

Commands run from the repository root through `mise`, Cargo-backed commands
sequentially. Enable `WYRD_LOG=info,vala_verify=debug,wyrd_server=debug` while
diagnosing failures and retain scrubbed traces. Server integration tests use
the real `CARGO_BIN_EXE_wyrd-server`; SDK journeys use the test server's
installed/exported worker provisioning after `mise run build:server`, never
a guessed build path. Postgres comes from the checked-in wrapper and per-test
fixtures; no external credentials/provider are required.

All names marked **planned** below are proposed additions to the stated
existing package/module/target, not claimed existing tests. After adding one,
confirm it selects exactly the intended test before recording RED. If grouping
or moving a test requires another actual selector, record the corrected,
equally strong command in implementation evidence before using it. Do not
create a test only to preserve a suggested name.

### Record contract proof

Existing test, extend with historical/default/new/typed cases:

```sh
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=verification::execution::tests::verification_result_round_trips_documented_outcome)'
```

### Startup proof

Existing test, extend the existing environment-value table:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::verification_limits_env_applies_and_refuses)'
```

### Pure conversion proof

**Planned** inline unit test in the existing hosted sandbox owner; no worker,
server or database. Exercise supported JSON/Monty values in both directions,
exact wire-range integers/finite floats/Unicode/nesting, unsupported/cyclic
values, non-string keys and bounded encoding refusal. The test constrains
conversion behavior, not a mandated private helper or conversion API. If the
conversion naturally lives in an adjacent existing owner, confirm and record
the equivalent library selector there before RED.

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'
```

### Bridge boundary proof

**Planned** integration test; actual interpreter and hosted declared dispatch,
with conversion/target/argument/result cases and authored-code negative probe:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries)'
```

### Accepted-authority proof

Existing test; extend its direct/nested and recovery cases:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::recovered_authority_bounds_every_agent_tool)'
```

### Budget proof

**Planned** hosted integration test with lowered limits and authored retries:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::agent_tool_budget_spans_origins_and_retries)'
```

### Concurrency proof

**Planned** hosted integration test with deterministic barriers:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_calls_share_non_code_concurrency)'
```

### Workspace bridge proof

**Planned** hosted integration test for awaited extraction and conflicting
mutations, distinct from the unchanged standalone mounted-writer regression:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_calls_share_workspace_mutations)'
```

### Workspace regression proof

Existing real-worker test:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::workspace_writes_share_one_budget)'
```

### Host-wait cleanup proof

**Planned** hosted integration test for query/future cancellation and
wall-clock budgets across retries; assert subsequent worker/session reuse:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation)'
```

### Extraction cleanup proof

**Planned** hosted integration test; deterministic interruption of bounded
blocking extraction and workspace release, with no late publication:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::code_extraction_settles_before_workspace_release)'
```

### Shutdown proof

Existing real-worker lifecycle test, extend pending bridge work as needed:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=sandbox_worker::sandbox_shutdown_settles_checkouts_and_reaps_workers)'
```

### Capacity proof

Existing hosted test, extend nested capacity refusal and caught-error attempt:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::capacity_refusal_is_busy_at_every_hosted_consumer)'
```

### Queued capacity proof

Existing queue test, extend nested refusal and recovered success:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::busy_queued_execution_is_deferred_until_capacity_frees)'
```

### Failed record proof

**Planned** hosted integration test through engine settlement/result capture:
failed and retried action, invalid Verdict/custom data/citations, cancelled
admitted call on a produced result, and capture-disabled non-disclosure.
Keep separate assertion labels for each table case.

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_hosted_artifacts::agent_failure_records_survive_result_settlement)'
```

### Public reduction proof

**Planned** tier-1 Rust SDK journey in the existing story. Use real SDK →
`WyrdTestServer` + repository Postgres + actual worker → SDK, checked-in Cards,
fixed evidence, a local scripted model and typed public result/readback:

```sh
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations)'
```

### Public authority proof

Existing Rust journey; extend nested denial/recovery. The same declaration
will be used by TASK-002's focused language/MCP journeys:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::hosted_tools_need_the_tool_and_domain_grants)'
```

### Public failure proof

**Planned** tier-1 status/recovery flow, with distinct budget and pending-call
cancellation phases and subsequent authorized success:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=verifier_building_blocks::code_mode_failure_releases_session)'
```

### Owner and static checks

After focused proof, run `mise run test:verification:sandbox` for changed
hosted ownership. Run the focused changed Vala modules with
`mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(/^blocks::/)'`
and wire regression with
`mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/^verification::execution::tests::/)'`.
If another behavioral test is added/named, record and run its exact selector,
not only these module filters.

Run `mise run codegen:regen` when contract/error changes require regenerated
outputs, then `mise run codegen:check`, `mise run check:deps`,
`mise run check:tenant-isolation`, `mise run fmt:check`, `mise run lints`,
and `mise exec -- git diff --check`. Architecture/security document updates
use static review against the spec; no manufactured RED for prose or generated
files. TASK-002 owns authored Python and language-facing projection proof;
repair mechanical consumers here when needed for compilation.

Record each scenario's expected behavioral RED, GREEN, affected earlier tests
rerun, final commands/results, historical/new contract evidence and authority
reconciliations. Attach actual model-visible byte totals and judgment/citation/
leaf-record equality for the single shared comparison. Whole public journey
suites and the integrated surface/codegen checks run once at change review,
not once per task; do not run `gate` for this task.

## Material Stop Conditions

Stop for spec/planning authority if correctness needs a changed callable/wire
shape, different defaults/counting/stop semantics, refreshed authority,
credentials or OS-specific isolation in a worker, a new dependency/feature,
a new runtime/transport/storage owner, a migration, weaker cleanup/retention,
or changed acceptance proof. Private dispatch signatures, value conversion
mechanics, lock structure, fixture layout and equivalent focused selectors
remain implementation decisions. Record bounded factual corrections without
weakening proof. An obsolete private name/path alone is not a blocker.

## Authority Links

- [Approved revision 2](../spec.md), including authority reconciliation and
  acceptance/coverage grouping.
- [Repository rules](../../../../AGENTS.md), [execution discipline](../../../implementation-execution.md),
  [testing and exact selectors](../../../../TESTING.md).
- [Verifier authority](../../../../architecture/wyrd-design.md#verifier),
  [accepted authority](../../../../architecture/wyrd-security-posture.md#principal-and-credential-lifecycle),
  [Oracle](../../../../architecture/bifrost-design.md#query-oracle),
  [read audit/terminal contract](../../../../architecture/bifrost-design.md#read-audit-and-terminal-contract),
  [rollout compatibility](../../../../architecture/operations/deployment-and-release.md#rollout-and-version-skew).
- [Rust](../../../../agent_docs/rust.md), [errors](../../../../agent_docs/errors.md),
  [agent surfaces](../../../../agent_docs/agent-surfaces.md).

## Implementation Evidence

Candidate: commits `d57c54781` through `0e51af2a2` on `change/verifier-code-mode`.

### Reuse map (revalidated against source)

| Need | Existing owner reused | Gap closed |
| --- | --- | --- |
| Nested dispatch, authorization, typed parse | Vala `ToolSession`/`Dispatch` recorded path (`crates/vala/vala-verify/src/blocks/tools.rs`) | `ToolBridge` from `ToolSession::with_code` routes `call_tool` through the same recorded path; no second permission/lookup. |
| Suspension/resumption | Monty pool `NameLookup`/`FunctionCall`/`ResolveFutures` events in `Sandbox::exec` (`hosted/sandbox.rs`) | `call_tool` futures resolved through `FuturesUnordered`; safe problem `RuntimeError` for ordinary failures, host stops uncatchable. |
| Budget/argument limits | `HostedLimits` + checked env parsing (`config.rs`) | `agent_tool_max_calls` (128) and `tool_argument_max_bytes` (64 KiB), one retry-spanning reservation shared by both origins. |
| Concurrency | Skald per-turn cap | Shared non-code semaphore in `Dispatch`; `code.run` holds none. |
| `/work` mutation | `Workspace::writer`, `work_room` | Writer taken per Python write OS call, not per program; room checked against the shared ledger. |
| Records | `StepResult.tool_calls`, engine settlement | `ToolCallRecord.recipient`, `RequirementResult.tool_calls`, engine-owned `ToolCalls` surviving failures. |
| Query cancellation | Oracle frame-stream drop guards | `CancellationToken` drop guard in `BifrostQuery`. |

### Acceptance evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| REQ-001–004; AC-001–003, AC-015–016: dependent declared calls in steps and checks, unchanged direct catalog, no escape | `hosted/sandbox.rs` `exec`/`answer`, `hosted/tools.rs` `CodeTool for Invocation`, vala `ToolBridge` | `pg_hosted_artifacts::code_bridge_preserves_declared_tool_boundaries`; `verifier_building_blocks::code_mode_reduces_evidence_and_preserves_citations` | PASS |
| REQ-005, REQ-008; AC-003, AC-005, AC-011: lossless JSON, bounded refusal, catchable safe catalog errors, no partial oversized return | `python_from_json`/`json_from_python`, `answer` keeps a structured failure's own code/status/details | `verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded`; `code_bridge_preserves_declared_tool_boundaries`; `recovered_authority_bounds_every_agent_tool` | PASS |
| REQ-006–007; INV-001, INV-004; AC-004, AC-017: exact tool + domain grants, accepted authority after recovery, no identity selection | Bridge uses the session's accepted `Invocation`; no worker credentials | `pg_hosted_artifacts::recovered_authority_bounds_every_agent_tool` (nested SPANS denial caught as `WYRD_VALA_403_QUERY_FORBIDDEN`, LOGS repair); `verifier_building_blocks::hosted_tools_need_the_tool_and_domain_grants` (nested `bifrost.query` denied `WYRD_PERMISSION_403_DENIED_RBAC`, caught, repaired); `delegated_tools_keep_the_narrower_authority` | PASS |
| REQ-009–010; AC-006, AC-010, AC-014, AC-018: atomic retry-spanning budget, final allowance, uncatchable exhaustion/Busy, honest status | `Dispatch::spend`, halt on Busy/budget | `config::tests::verification_limits_env_applies_and_refuses`; `pg_hosted_artifacts::agent_tool_budget_spans_origins_and_retries`; `capacity_refusal_is_busy_at_every_hosted_consumer`; `code_mode_failure_releases_session` (budget phase: caught refusals still fail the step with `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED`, 1 model + 127 nested records, requirement inconclusive, no further model request) | PASS |
| REQ-011; AC-007: shared cap, cap 1 completes, distinct query IDs, correct futures/start order | Shared semaphore in `Dispatch` | `pg_hosted_artifacts::code_calls_share_non_code_concurrency`; vala `blocks_agent::code_bridge_shares_the_recorded_path` (peak active at caps 1 and 2) | PASS |
| REQ-012; AC-006, AC-009–010, AC-018: wall-clock waits charged across retries, no detached work, settled workers/workspace, fresh invocation | Python-time `Reservation`, drop guards, inline awaits | `pg_hosted_artifacts::code_host_waits_obey_deadlines_and_cancellation`; `code_shutdown_settles_pending_host_work`; `code_mode_failure_releases_session` (stall phase: held query ends at the time limit, pending record no digest/0 bytes, resources released, next investigation calls every tool) | PASS (extraction gap below) |
| REQ-013; AC-008, AC-011: shared no-overwrite/write/scratch limits, awaited visibility, no lock recursion | Per-write writer + `work_room` in `Sandbox::exec` | `pg_hosted_artifacts::code_calls_share_workspace_mutations`; `sandbox_worker::workspace_writes_share_one_budget` (unchanged regression) | PASS |
| REQ-016–017 record contract: recipient, check tool calls, historical decode | `wyrd-spec` `ToolCallRecord.recipient`, `RequirementResult.tool_calls` | `verification::execution::tests::verification_result_round_trips_documented_outcome`; journey retained readback equality of requirements and steps | PASS |
| Concrete reduction comparison | `evidence-reduction` fixture | `code_mode_reduces_evidence_and_preserves_citations`: same hits, failed Verdict, `/n07` citation and data; 20 leaf `fs.read` records identical (args, digest, bytes) across direct/code; model-visible tool bytes direct 96,824 vs code 595 (0.6%) for both step and check; no note text reached the model in code mode. Provider tokens are scripted, not recorded. | PASS |

### RED / GREEN

- Startup: behavioral RED at `config.rs` (new settings ignored) → GREEN.
- Record contract: additive wire fields; RED was compile-level only (no behavioral RED possible for a new field); round-trip/historical-default cases GREEN.
- Conversion: RED with stub refusing supported JSON → GREEN.
- Failed records (vala): `agent_records_survive_failed_steps_and_inconclusive_checks` RED "failed step records left 0 right 1" → GREEN with engine-owned `ToolCalls`.
- Bridge boundary: RED with bridge withheld (`NameError`, panic at printed output) → GREEN.
- Workspace: RED (a) whole-program writer hold, 12 s timeout; RED (b) no room check, `past_room` "ok" → GREEN 1.7 s.
- Concurrency: RED with `code.run` taking a permit, timeout at cap 1 → GREEN.
- Budget: RED without nested spend, sixth request admitted → GREEN.
- Nested failure code: `recovered_authority_bounds_every_agent_tool` extension RED: caught code `WYRD_TOOL_500_CALL` (catalog round trip lost `WYRD_VALA_403_QUERY_FORBIDDEN`) → GREEN after `answer` projects the structured failure's own fields.
- Rust journeys: all three RED with bridge withheld (`NameError: name 'call_tool' is not defined`) → GREEN after restore and `mise run build:server`.
- No separate RED (protect pre-existing mechanisms through nested paths): `code_host_waits_obey_deadlines_and_cancellation`, `capacity_refusal_is_busy_at_every_hosted_consumer` extension, `code_shutdown_settles_pending_host_work`; vala `agent_tool_budget_spans_attempts_without_refund`, `code_bridge_shares_the_recorded_path`.

### Diagnoses

- `busy_tool_ends_the_agent_step_before_a_pending_sibling`: Symptom pending sibling never started. Cause: `Dispatch::spend` refuses calls after halt by design; fixture `BusyCode` failed on first poll. Fix site: fixture yields once (diagnostician agreed).
- `code_host_waits_obey_deadlines_and_cancellation`: Symptom query resources unreleased. Cause (diagnostician): `tokio::pin!` shadowing made `drop(step)` drop only the reference. Fix site: test block scope; production awaits inline and detaches nothing.
- Capacity extension returned Ok: Cause leftover scripted answer from an earlier refused step. Fix site: test consumes it explicitly.
- Concurrency test expectation: `fs.read` from start 0 returns both pieces by design; expectation corrected.

### Corrected selectors

- Shutdown proof: `pg_hosted_artifacts::code_shutdown_settles_pending_host_work` (hosted pending bridge work) instead of extending `sandbox_worker::sandbox_shutdown_settles_checkouts_and_reaps_workers`, which has no bridge.
- Failed record proof: vala `blocks_agent::agent_records_survive_failed_steps_and_inconclusive_checks` (engine settlement, failed/retried step, inconclusive check) plus the `code_mode_failure_releases_session` budget/stall phases (failed step and cancelled admitted call through the public result).

### Gaps (not proved by a dedicated test)

- Extraction cleanup (`code_extraction_settles_before_workspace_release`): no deterministic barrier exists inside blocking extraction. Structural guarantee: the `spawn_blocking` closure owns an `Arc<Workspace>` (and its permit), so the workspace is released only after extraction settles, and its result has no recipient once the future is dropped.
- Queued capacity nested extension: nested Busy reaches the engine as the same `BlockError::Busy` (proved by the capacity extension); queue deferral of Busy is the unchanged engine path proved by `busy_queued_execution_is_deferred_until_capacity_frees`.
- Capture-disabled non-disclosure: records live only in the verification result (`execution_outputs.output`, behind the elevated payload permission); no new surface carries arguments. Not separately tested.
- An uncaught nested failure fails `code.run` as an ordinary tool error returned to the model (existing `code.run` contract), not the step directly; host stops (budget, Busy, deadline, cancellation) do stop the step.

### Commands (final run, all passing)

```sh
mise run test:verification:sandbox                                   # 22 passed
mise exec -- cargo nextest run --locked -p vala-verify --lib -E 'test(/^blocks::/)'          # 30 passed
mise exec -- cargo nextest run --locked -p vala-verify --test integration -E 'test(/^blocks_agent::/)'  # 16 passed
mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(/^verification::execution::tests::/)'  # 8 passed
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=config::tests::verification_limits_env_applies_and_refuses) | test(=verification::hosted::sandbox::tests::code_bridge_json_conversion_is_lossless_and_bounded)'  # 2 passed
mise run build:server
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^verifier_building_blocks::/)'  # 29 passed (shared fixture and retained helper changed)
mise run codegen:check && mise run check:deps && mise run check:tenant-isolation
mise run fmt:check && mise run lints && mise run docs:check
mise exec -- git diff --check
```

### Non-goals

Python/TypeScript/MCP projections, stubs and public docs (TASK-002) unchanged beyond regenerated error catalog and design text. No new dependency, feature, migration, transport, storage owner or OS-specific isolation. `gate` not run.

**Status: IMPLEMENTED**, pending `$wyrd-task-review`.
