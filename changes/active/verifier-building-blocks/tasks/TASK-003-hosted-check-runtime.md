---
id: TASK-003
kind: implementation
status: ready
spec: SPEC-verifier-building-blocks
spec_revision: 6
requirements: [REQ-002, REQ-004, REQ-005, REQ-007, REQ-008, REQ-009, REQ-010, REQ-015, INV-001, INV-003, INV-005, INV-006, INV-007, AC-003, AC-004, AC-006, AC-010]
depends_on: [TASK-001]
---

# Hosted Agent, extraction, and sandboxed code capabilities

## Outcome and Value

The approved action/check boundary has real Agent, extraction, and Python
implementations with bounded resources and attributable evidence. Each can be
exercised without the new graph driver, a database, or a full Wyrd server.

## Owners, Scope, Consumers, and Prohibited Changes

Own the shared Skald-backed invoker extension and the server-hosted workspace,
extraction, built-in filesystem/code tools, and Python sandbox capabilities.
Keep reusable Agent/provider execution in Skald/Vala and tenancy, storage,
credentials, and sandbox policy in server-owned dependencies. Do not move
specialized parser/worker dependencies into foundation or client crates.

Consume task 1's types and execution seams; do not modify task 2's graph/scope
or task 4's SDKs. Task 5 supplies accepted authority and pinned tenant evidence
and composes these capabilities. Exercise them independently through explicit
existing resolver/gateway/executor seams, not AppState-only construction or a
second end-to-end engine. No endpoints, queue writes, schedules, or Operators.

New workspace/worker/parsing capabilities are necessary because the base has
media resolution and a tool-free judge but no sandboxed preparation workflow.
Extend existing storage/media and `Agent::run_prompt` orchestration rather
than writing a replacement Agent loop or transport.

## Source-backed Reuse Map

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Agent invocation | `vala-eval/src/orchestrator/judge.rs::SkaldJudgeInvoker`, `AgentCardResolver`, `PromptCardResolver`, `MediaResolver`; `skald-agent::Agent::run_prompt` | `EvalEngine` composes the judge; existing `integration` target's `orchestrator_judge_skald` module; client `Run::invoke_on` already uses run_prompt | Invoker renders/dispatches directly and caches a single Agent; no step/check policy or tools | Extend existing invoker and shared Agent loop with per-call identity, position, explicit bindings, and bounded execution | Keyed multi-Agent resolution replaces single-Agent assumption; no second runtime or provider client |
| Authorized evidence | server `verification/eval.rs::TenantMedia::resolve`; existing bounded read, `TenantRegistry` resolution | In-module media authorization/body-limit/refusal tests; native-media invoker tests | No scratch workspace, structured Office extraction, or source-slot citation propagation | Extend tenant evidence resolution and materialize only pinned declared inputs | Per-call workspace/extractors are missing capabilities, not replacement registry/storage |
| Tool execution | Skald `Agent::with_tools`/`run_prompt`, `skald-tool::ToolRegistry`/`ToolResolver`/`AgentTool`; server `query/collect.rs::BoundedQuery` and authorized Card operations in `components/cards/routes.rs` | Existing Agent loop, server MCP Card/Bifrost adapters, and Invoker construction in `verification/eval.rs::judge_invoker` | Invoker has no tool resolver, rejects declared tools, and bypasses the loop; bounded workspace/code tools are absent | Supply the complete REQ-008 built-in catalog through existing ToolResolver/ToolRegistry and AgentTool dispatch, automatically resolving declared names with invocation-scoped authority/workspace | Concrete tools are approved behavior; no dynamic plugin platform |
| Python containment | Existing server subprocess/resource facilities, `tempfile`, media bounds; no Monty dependency in the inspected manifests | Existing server media tests and limits; no existing custom-code verifier runtime | Safe isolated code/action/check execution is absent | Add the approved Monty-based worker integration narrowly in hosted runtime | A worker sandbox is required; verify dependency capability rather than treating proposal API sketches as facts |

## Approach

1. Consume the contract/position/provenance handoff; revalidate invoker, Agent
   loop, tenant media, and existing tool dispatch.
2. Extend invocation through the existing Agent loop with exact Prompt/Agent
   identity, route policy, explicit bindings, and response validation.
3. Materialize bounded per-call workspaces and reuse one format extraction
   capability for `extract`, read, and search.
4. Provide the built-in registry, automatic Invoker tool resolution, and
   sandboxed Python capabilities. Bind original-subject/delegated authority
   and workspace per invocation; reuse authorized Rust operations directly.
5. Validate source citations and hand task 5 concrete implementations plus
   explicit construction dependencies; supply integrated regressions without
   running the full service suites in this task.

## Ordered Implementation Scenarios

### Scenario 1 — Agents run on their declared route in either position

**Behavior.** Step Agents can use declared read/compute tools and return data;
check Agents have no tools/workspace and return the Verdict schema. Prompt
bindings, schema, route override, attribution context, and bounded failure
handling match REQ-005/007/015.

**RED.** Add focused invoker/runtime tests with scripted providers and existing
resolver/gateway doubles. Prove the missing loop/position behavior without a
server, SQL, or production credential; record exact test selections.

**GREEN.** Extend the existing invoker and Agent loop, preserving error
redaction, native/gateway compatibility, and per-call effective route.

**REFACTOR.** Consolidate resolution, response validation, and invocation in
the existing owner; do not copy the loop or request compiler into the server.

### Scenario 2 — Declared evidence becomes bounded readable workspace data

**Behavior.** Media/archives/JSON/prior outputs materialize under explicit
paths; reads/search/extraction agree on page/slide/line locations and the
canonical DOCX/XLSX JSON-pointer locators in REQ-004. Every extractable format
returns exactly the locked REQ-004 shape. Inputs are
read-only, work is writable within budget, paths/links cannot escape, and
cleanup runs on success, failure, or cancellation (REQ-008/010).

**RED.** Add isolated focused cases using local bounded evidence and the
existing storage test facilities. Include exact output fixtures for PDF,
PPTX, DOCX, XLSX, CSV, JSON, text, Markdown, and HTML; empty content, merged
table cells, ordered DOCX blocks, sparse XLSX addresses, cached formulas,
serial dates, error cells, and source-locator validation. Also include
wrong digest/format/size, traversal, unsafe links, archive bombs, truncation,
and cancellation cleanup. No full server or managed Postgres is needed.

**GREEN.** Extend existing evidence reading with the smallest necessary
workspace/parser capability; use the same concrete extraction path in actions
and tools. Rerun scenario 1 and the new cases.

**REFACTOR.** Keep byte authorization distinct from local parsing; remove
multiple readers/parsers for the same evidence or ad hoc host path access.

### Scenario 3 — Derived findings remain citable by a tool-less judge

**Behavior.** Workspace paths resolve to original slots; step data can carry
source citations into an explicitly bound downstream call. Valid source
locators are accepted without a second file read; fabricated/out-of-binding
citations cannot create a pass (REQ-009/015).

**RED.** Add focused tests against the concrete citation validator and two
scripted calls through the invoker seam. Fail for missing propagation,
forbidden tool access, invalid locators, or an uncited passed result.

**GREEN.** Validate against pinned original evidence reachable through the
call's declared bindings, preserving citations in ordinary output. Rerun
all earlier cases.

**REFACTOR.** Avoid an evidence wrapper, hidden Prompt injections, or a second
retrieval protocol. Record tool provenance once.

### Scenario 4 — Python cannot escape or contaminate another invocation

**Behavior.** Code actions/checks and code.run are bounded workers with the
approved data/Verdict outputs, no credentials/network/host filesystem or
arbitrary callbacks, no retained interpreter state, and isolated crash/hang
handling. Limits and cleanup satisfy INV-005/007 and spec deployment defaults.

**RED.** Add focused isolated worker and refusal tests. Pure policy branches
use library tests; tests that launch the real worker use the existing owning
runtime's appropriate component-test target. Do not initialize CPython/PyO3
or Node inside Rust tests. A real containment failure is a RED, not a reason
to mock away the boundary or replace it with a full server journey.

**GREEN.** Integrate the smallest verified worker configuration and parent-
serviced filesystem policy that meets every required boundary. Rerun earlier
cases. Record exact component commands, feature set, dependency version, and
actual isolation evidence; no full service integration suite runs here.

**REFACTOR.** Remove redundant wrappers and speculative runtime options.
Do not ship execution with a known escape or cross-call state leak.

### Scenario 5 — The shared Python pool stays bounded under load

**Behavior.** Apply spec Deployment limits exactly: 2 warm / 8 maximum workers,
256 MiB allocation / 384 MiB process / 2 GiB worker-group limits, 10-second
end-to-end Python operations, and bounded waiting under remaining deadlines.
Only Python operations acquire workers; model waits and Agent checks do not.
Fresh sessions and output/scratch budgets remain isolated across calls.

**RED.** Add focused actual-pool/worker cases for concurrent saturation,
waiter-limit refusal, deadline/cancellation while waiting or servicing mounts,
allocation/process/group memory breaches, output flooding, and worker recovery.
Verify unrelated model-only invocation does not check out a worker. Use normal
bounded test concurrency, never synthetic host load or raised thread counts.

**GREEN.** Enforce the configured limits in the owning runtime, including
parent-side deadlines/output bounds and OS memory/security enforcement. Expose
capacity to the existing background admission owner; do not add a second queue.
Rerun earlier isolation and output cases; record the pinned dependency API and
actual host enforcement evidence.

**REFACTOR.** Keep one cohesive pool owner and per-call sessions; remove
unbounded waiters or worker reservations spanning model calls.

### Scenario 6 — Declared built-ins resolve and execute with original authority

**Behavior.** A registered Agent declaring `bifrost.query` and `code.run`
executes both through the hosted Invoker without client tool registration or
MCP. Resolve the complete closed REQ-008 catalog before the first model call,
attach only declared tools, and reuse the existing Agent loop. Every call, including `code.run` and filesystem tools, requires invoke
permission under Wyrd's general global or exact scope permission rule before
execution. Protected capabilities then
apply their existing domain checks independently. Calls retain original-subject or valid delegated permissions, tenant, actor, and
attribution; SYSTEM execution never grants authority. Registered built-in
names are supported; arbitrary client callables remain non-registrable.

**RED.** Add focused Invoker/tool-registry cases with scripted model tool calls
and injected existing capability owners. Cover all catalog names, unknown
names, undeclared calls, missing workspace, check-position refusal, denied
Card/query access, no tool grant despite domain access, tool grant without
domain access, a non-covering permission scope, global invoke permission with restricted
tables, multi-table access outside scope, denied code/filesystem invocation,
narrowed delegation, forged authority in arguments, and
interleaved invocations for distinct subjects/tenants/workspaces. Prove that
runtime resolution needs neither a Python decorator nor an MCP transport.
Author real protected-owner and queued retry/restart regressions for the final
merged pass; local doubles cannot prove actual authorization enforcement.

**GREEN.** Extend the existing Invoker with the existing tool resolution and
loop machinery. Supply server-built executable tools bound to the accepted
caller and invocation workspace/deadline. Enforce tool invoke permission with
the existing checker/audit before dispatch, retaining domain checks inside
the capability owner; do not authorize only once at Agent construction. Reuse BoundedQuery, authorized Card
operations, and the shared Monty pool rather than calling MCP or duplicating
services. Rerun earlier Agent, isolation, citation, and pool cases.

**REFACTOR.** Share immutable catalog/schema data; keep mutable authority and
workspace state out of global registrations or cached Agents. Do not build a
new plugin framework, authentication mechanism, or orchestration loop.

## Contract Handoff From TASK-001

- Implement Agent, code, and extract execution as
  `vala_eval::blocks::ActionExecutor` / `CheckExecutor`, returning
  `vala_eval::blocks::BlockError` (`Retryable` for transient provider or
  transport failure; `Terminal` for malformed structured output, provider
  rejection, or anything another attempt cannot clear). Executors do not
  report timeouts; the engine owns the deadline.
- The LLM judge is not kept (owner decision). Build the Agent block executor
  on `skald-agent`'s `Agent::run_prompt` and the existing Agent/Prompt Card
  resolvers directly. Do not wrap, extend, or convert from
  `SkaldJudgeInvoker`, `JudgeInvoker`, or `JudgeError`; add no
  `From<JudgeError>` or other bridge. If a resolution helper you need lives
  only in `vala-eval/src/orchestrator/judge.rs`, move it into the Agent block
  executor's module so TASK-005 can delete the judge file whole.

## Acceptance Criteria

- Real implementations exercise through the shared capability seams without
  task 2's driver, database state, or live credentials.
- Agent/Prompt versions, route, input digests, tool use, runtime and extractor
  identities survive into the approved provenance shape.
- Every extraction output matches REQ-004, with no duplicate counts or image
  helpers; native visual inputs remain separate from text/structure extraction.
- Format fidelity is text/structure only; legacy formats and incompatible
  native media are refused with actionable safe errors.
- Every REQ-008 built-in resolves automatically through the hosted Invoker;
  only declared tools attach and protected calls preserve original-subject or
  valid delegated authority without cross-call context leakage.
- Workspaces and worker execution satisfy all limits and isolation rules.
- Task 5 receives the concrete implementations and explicit construction
  requirements; no test-only production engine or hidden AppState dependency.

## Expected Write Set and Consumer Closure

Existing Vala orchestrator/invoker internals, Skald runtime integration only
where needed, hosted verification runtime/evidence/tool owners and tests in
`wyrd-server`. Own server sandbox/parser dependency additions, corresponding
workspace manifest/version and lockfile updates, and their narrow boundary
checks. Coordinate shared Vala root exports with task 2; do not independently
change foundation contracts or publish new SDK behavior.

## Verification and Evidence

Use focused TDD for each concrete capability. Library commands below are owner
slices, not full suites; record exact per-case commands after discovering the
actual test names. Collect any new real-worker target before executing it,
and record its exact `mise exec -- cargo nextest run` target/selection.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/^verification::/) and not test(/::pg_tests::/)'
mise exec -- cargo nextest run --locked -p vala-eval --lib
mise exec -- cargo clippy --locked -p wyrd-server -p vala-eval --all-features --lib -- -D warnings
mise run fmt
mise run check:deps
```

Narrow these slices to actual changed modules during iteration. Do not run
full server/Vala integration targets or SDK journeys here. The existing
invoker integration regressions and hosted cross-boundary proofs are updated
now and run with the fully merged candidate.

## Material Stop Conditions

If verified Monty/runtime capabilities cannot meet the approved isolation or
limits, stop rather than silently accept risk or substitute host Python.
New tools, network/write capabilities, unsupported native routes, new public
input/citation formats, a new Agent loop, or moving specialized dependencies
to foundation/client owners requires spec authority.

## Authority Links

- [Approved specification r6](../spec.md), [supporting proposal](../proposal.md),
  [worked examples](../example.md), and [flow diagram](../architecture/verification-flow.html).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [Wyrd design](../../../../architecture/wyrd-design.md), and
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Execution and final proof coordination](README.md). Ready after owner-approved
  completion of TASK-001; execute independently alongside the other parallel tasks.

## Implementation Evidence

Commits: `689109c79`, `50597593c`, `d7f23fe5b`, `688e34d3c`, `57c69c357`
(Scenario 1 landed before these).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Agents run on declared route; step tools, tool-less checks, Verdict schema | `vala-eval/src/blocks/agent.rs`, `blocks/tools.rs` | `blocks_agent::*` (11 cases, incl. `route_override_replaces_declared_route`, `check_agent_refuses_tools`, `step_agent_records_declared_tool_calls`, `step_agent_refuses_unknown_tool`) | PASS |
| Extraction matches REQ-004; legacy/wrong formats refused | `verification/hosted/extract.rs`; `impl ActionExecutor for TenantEvidence` | `extract::tests::*` (9), `evidence::tests::extract_action_returns_locked_shape` | PASS |
| Bounded workspace: read-only inputs, budgeted work, no escapes, cleanup | `verification/hosted/workspace.rs` | `workspace::tests::*` (4) | PASS |
| Citations validated against reachable pinned evidence; uncited pass → inconclusive | `EvidenceReader::cited`, `extract::locates` | `evidence::tests::citations_must_locate_reachable_evidence`, `blocks_agent::check_agent_keeps_only_valid_citations` | PASS |
| Python isolation: no host files/env/callbacks, mounts only, no cross-call state, identical on every platform | `crates/wyrd/wyrd-monty-worker`, `verification/hosted/sandbox.rs` | `sandbox_worker::sandbox_reaches_only_its_mounts`, `sandbox_runs_entrypoints_without_shared_state` | PASS |
| Pool limits: workers, 16 waiters, 256 MiB alloc / 384 MiB per process / 2 GiB pool, 10 s op incl. wait, output bound, recovery | `Sandbox` (admission semaphore, `live_workers`, `timeout_at`), `HostedLimits` | `sandbox_worker::sandbox_contains_runaway_code`, `sandbox_bounds_workers_and_waiters` | PASS |
| Built-ins resolve with exact/global `tool:invoke` + audit before dispatch; domain checks retained; forged args refused; workspace required | `verification/hosted/tools.rs` (`audit::authorize`, MCP descriptors reused) | `tools::tests::*` (4) | PASS |

Commands (all passing):

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --offline -p wyrd-server --lib -E 'test(/verification::hosted::/)'"   # 21 passed
mise run test:verification:sandbox   # builds the worker; 5 passed
mise exec -- cargo nextest run --offline -p vala-eval --test integration -E 'test(/^(blocks_agent|orchestrator_judge_skald)::/)'
mise exec -- cargo clippy --offline -p wyrd-server -p vala-eval --all-features --lib --tests -- -D warnings
mise run lints   # includes wyrd-monty-worker
mise run fmt
mise run check:deps
mise run check:workspace-hack
git diff --check
```

Material limits and handoff to TASK-005:

- The worker is a workspace member kept out of the workspace-hack; the
  workspace moved to Rust 1.99.0 (monty 1.1.0 does not compile on 1.98) and
  pyo3 0.29 (monty's pyo3 `links = "python"`). `mise run build:monty-worker`
  builds it; TASK-005 owns the worker-path configuration, server wiring, and
  the container build.
- Memory is bounded portably (macOS and Linux, x86_64 and arm64): Monty's
  allocator ceiling per operation and `HostedLimits::live_workers` (pool ÷
  per-process ceiling) worker admission. No OS-specific mechanism is used.
  Warm workers are not replenished after use (monty-pool spawns on demand).
- Artifact code and evidence are read through the authorized registry; see
  the R1 remediation evidence.
- Oracle table refusal through `bifrost.query` is proven by
  `pg_hosted_artifacts::recovered_authority_bounds_every_agent_tool`.
- TASK-005 constructs `HostedTools::new(state, caller, evidence, workspaces,
  sandbox, limits)`, `CodeExecutor::new(sandbox, evidence)`, and
  `Sandbox::start(worker_path, limits)`.
