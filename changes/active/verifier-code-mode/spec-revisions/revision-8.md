---
id: SPEC-verifier-code-mode
revision: 8
status: approved
---

# Tool orchestration in Verifier Agent code

Revision 8 records the maintainer's 2026-11-14 approval of resource-backed
hosted capacity after the clean L1 run failed at c64 on immediate workspace
refusal. Capacity follows one discovered process resource plan, workers are
reused with fresh sessions, admission waits fairly under bounded accounting,
and throughput acceptance is derived from measured CPU cost. See
[Resource-backed capacity](#resource-backed-capacity). Prior revision 7 is
preserved in [revision-7.md](spec-revisions/revision-7.md).

Revision 7 records the maintainer's 2026-11-12 agreement to the alignment
review: the same public verification workload must run through all three SDKs,
both client and server must expose actionable phase/CPU/allocation breakdowns,
and benchmark machinery must be compiled out of production builds behind
`bench`. Merely reporting missing Python/TypeScript phases or keeping dormant
measurement state is insufficient. Minimum forwarding of the existing bench
capability into diagnostic-only builds is permitted; production SDK packages
and their API stay free of benchmark machinery. This supersedes revision 6's
blanket ban on bench feature wiring, not its rejection of duplicate collectors,
synthetic probes or new public SDK benchmark APIs. Prior revision 6 is preserved
in [revision-6.md](spec-revisions/revision-6.md).

Prior benchmark scopes are preserved in [revision 4](spec-revisions/revision-4.md),
[revision 5](spec-revisions/revision-5.md) and [revision 6](spec-revisions/revision-6.md).
The requirements below define the current benchmark and isolation contract.

TASK-001, TASK-002 and their issued reviews retain approved revision-2 authority
in [revision-2.md](spec-revisions/revision-2.md). The earlier benchmark scope is
preserved in [revision-3.md](spec-revisions/revision-3.md). TASK-002B owns import
and contract composition; TASK-003 owns the shared three-language benchmark.
The new projection supersedes older immediate full-result wording only as
stated in the integration reconciliation below; runtime security, cleanup,
record completeness and original-evidence invariants remain.

## Problem and proposed behavior

A Verifier's Agent can already call declared tools and run generated Python
through `code.run`. The model receives descriptions and argument schemas for
those tools today. However, Python cannot call them: the model must receive an
intermediate result and request the next operation itself. Large intermediate
results consume model context even when a small computation could select the
useful evidence.

Add code mode to the existing hosted `code.run`: generated Python may await
`call_tool(name, arguments)` to invoke another tool declared on that Agent.
Monty suspends Python, the server executes the ordinary protected tool call,
and Python resumes with its bounded result. The Agent can gather, combine,
and reduce evidence before returning selected output to the model. Agent
preparation steps and Agent requirement checks receive the same capability.

For example, an Agent inspecting 20 evidence files currently requests reads
and receives their text in its conversation. After this change, one
`code.run` program can read those files through `fs.read`, select the relevant
values and original citation locators, and return only those findings. The
model still makes the final judgment against its declared response format.

The maintainer approved the reviewed revision 2 by requesting implementation
planning on 2026-11-07. This approves the written behavior below; it does not
authorize implementation, a Git commit, or a push.

## Rationale, costs, and limits

Extend the existing hosted tool session and Monty pool. The same declared
tools, typed arguments, accepted execution authority, capability owners, and
recording rules serve direct and Python-originated calls. A parallel
authorization or tool implementation would allow the two paths to diverge.

Two costs are deliberate. A suspended Python session retains its
resident-memory reservation and worker, but releases its runnable CPU permit
during a mediated tool/model wait; resumption competes fairly for runnable
capacity. Healthy subprocesses reset between operations; crashed, cancelled
and failed-reset workers are discarded and settled/reaped before their
reservations are released. Fresh sessions retain no prior Python or tenant
state. This implements the architecture's fresh-session contract, not a
fresh-process requirement. Also, every nested call still produces its own permission
decisions and bounded execution record. Reducing model-visible data does not
reduce the server's accountability obligations.

Keep the current provider-facing tool catalog and schemas. Code mode saves
tokens by keeping intermediate results inside Python and reducing dependent
model turns. On-demand schema discovery is a separate optimization and is
not required here. No universal token or latency improvement is promised;
the acceptance workload below measures a concrete reduction.

The main alternatives are inadequate: the present model/tool loop requires
intermediate data to cross the model boundary; authored `action: code` and
`check: code` do not provide model-generated orchestration of declared tools; a second tool
dispatcher would duplicate security and evidence behavior. General shell,
HTTP, and arbitrary MCP execution are outside this change.

## Scope and terminology

- An **Agent invocation** is one logical Agent step or requirement check,
  including its authored retries within an execution. An **attempt** is one
  run of that invocation.
- A **direct call** is a tool request emitted by the model. A **nested call**
  is a tool request made by Python inside `code.run`.
- A **recipient** is where a successful bounded tool result was delivered:
  `model` for a direct call, or `code` for a nested call.
- An **admitted tool call** has passed invocation-local name resolution and
  argument-boundary checks and entered the common recording/execution path.
  Permission denial can still occur there.

The scope is server-hosted Agent steps and whole-Verifier Agent checks,
including inline, file-authored, and exact registered Agent references, on
immediate and queued executions. Existing restrictions on where Agent checks
may appear do not change. SDKs and HTTP/MCP project the resulting behavior;
they do not execute Monty or implement permission decisions.

Non-goals are new Card kinds, a new Verifier discriminator, new Agent fields,
an execution-mode flag, a new code tool name, tool discovery/search endpoints,
generated per-tool Python wrapper APIs, arbitrary MCP registration, sub-agent
dispatch, Source adapters, shell/network access, new extraction formats,
external mutation, and a durable intermediate-result store. Authored code
actions/checks retain their existing explicit-input execution contract and do
not acquire tool access.

## Functional contract

### Python calls and existing tool definitions

- **REQ-001 — Availability.** Declaring `code.run` enables the Python bridge
  for that invocation without another flag. The Agent still needs the
  existing workspace and ordinary `tool:invoke` grant for `code.run`.
  Existing computation-only programs and the outer `code.run` request and
  response shapes remain valid.
- **REQ-002 — Callable boundary.** `await call_tool(name, arguments)` takes
  an exact dotted built-in tool name and one JSON-object argument value.
  Success returns the existing tool's JSON value as Python values. Python
  can branch, loop, and pass a result to a later call without returning that
  result to the model. No synchronous invocation or alternate callable name
  is part of the supported interface.
- **REQ-003 — Declared targets.** Callable targets are precisely the Agent's
  resolved declared built-ins other than `code.run`. Unknown, undeclared,
  recursive `code.run`, and arbitrary external function calls dispatch
  nothing. Name lookup never falls back to a process-global registry.
  An Agent declaring only `code.run` retains computation and workspace file
  access but gains no other tool.
- **REQ-004 — One tool contract.** Reuse existing descriptions, argument
  schemas, typed parsing, output shapes, pagination, and truncation markers.
  The provider-facing tools remain available for direct calls. The
  `code.run` description explains the bridge, dotted names, error handling,
  and limits using that same resolved catalog; no independently authored
  duplicate tool schemas are introduced.
- **REQ-005 — Value conversion.** Preserve JSON null, booleans, Unicode
  strings, integer values in the existing wire range, finite numbers, arrays,
  and string-keyed objects. Reject unsupported values, cycles, non-string
  keys, out-of-range integers, and non-finite numbers before dispatch rather
  than stringifying, rounding, or dropping them. Bound conversion by the
  argument/result byte limits and existing interpreter memory/deadline
  limits. A nested result over the ordinary tool-result limit is a tool
  failure, never a silently truncated success or partial Bifrost result.

The callable itself grants nothing. Monty supplies execution suspension and
value transport; the server owns tool lookup and dispatch. The public
Python interface is fixed here, while registration primitives, private
dispatcher APIs, and IPC handling remain implementation choices within the
existing owners.

### Authorization and failures

- **REQ-006 — Identical protected execution.** Both origins enter one
  declared-tool recording/execution path. Each actual tool invocation checks
  and audits its exact `tool:invoke` permission and then uses the owning
  capability's ordinary permission checks and tenant-scoped operation.
  Bifrost queries retain Oracle admission, statement-specific request IDs,
  read-only SQL validation, per-table authorization, terminal behavior, and
  read audit. The outer `code.run` permission does not authorize a child.
- **REQ-007 — Fixed authority.** Nested calls use the same accepted caller,
  tenant, delegation, and executor attribution as direct calls. Immediate,
  submitted, scheduled, retried, and recovered executions retain their
  existing authority rules. Arguments and Python state cannot select a
  tenant, credentials, grants, caller, or fallback identity. A declared tool
  or SYSTEM executor grants no additional access.
- **REQ-008 — Repairable errors.** A failed tool call returns no successful
  value. Python receives a catchable `RuntimeError` whose message is the
  compact JSON problem produced by the existing `WyrdError` projection,
  including the catalog code and only safe details. Code may recover by
  inspecting that JSON, never by parsing human prose. Permission-denial and
  domain-error codes retain their existing identity. Unknown and undeclared
  names use `WYRD_AGENT_404_TOOL_NOT_IN_AGENT` without exposing a global
  catalog; a nested `code.run` request uses that same unavailable-target
  result. Invalid bridge arguments use `WYRD_TOOL_422_INPUT`.
- **REQ-009 — Host failures stay host failures.** Deadline, cancellation,
  exhausted invocation call budget, and process-capacity refusal cannot be
  swallowed by Python to continue work or manufacture a verdict. Temporary
  hosted resource occupancy waits under the common tenant-fair admission
  policy (REQ-029) until the existing absolute deadline. When accounted waiting
  capacity is full, immediate execution returns the existing
  service-unavailable 503 with structured resource/reason details and
  `Retry-After: 1`; queued execution defers without consuming an attempt.
  Expiry while waiting or executing uses the existing verification 504.
  Recording pressure retains its existing 429 and audit failure remains
  non-blocking.
  Other uncaught program/tool failures retain ordinary step-failure and
  inconclusive-check semantics. They never judge the subject `failed` merely
  because execution failed. Audit write failure remains non-blocking.

### Bounds, concurrency, and workspace behavior

- **REQ-010 — Shared call budget.** Bound all direct requests and bridge
  requests of an Agent invocation together, across its retries. Count each
  request once, including the outer `code.run`, denied or malformed bridge
  requests, and failed tool calls; do not refund spent requests. Reserve
  requests atomically before dispatch so concurrent calls cannot exceed the
  budget. Successfully spending the final allowance does not itself fail
  the invocation: Python may finish and the model may produce its final
  answer under the existing iteration/time bounds. A subsequent request
  with no allowance is refused before dispatch and terminates the
  invocation; no later tool or model work starts. That refused request does
  not increase the spent count beyond its ceiling. Use the new catalog failure
  `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED`, naming the limit without
  argument contents. An exhausted step fails; an exhausted check is
  inconclusive. A new verification execution receives a fresh budget.
- **REQ-011 — Shared concurrency.** The Agent's existing effective
  `run_config.tool_concurrency_cap` bounds active non-code tool operations
  across direct and nested origins together; retain its existing default of
  8. Python concurrency, including `asyncio.gather`, cannot multiply that
  allowance. Waiting work is bounded by the remaining call budget and
  deadline. Worker-pool capacity separately bounds `code.run`. A suspended
  orchestration call must not hold a permit required by its own children,
  including when the tool concurrency cap is 1.
- **REQ-012 — Time and cancellation.** Worker acquisition, compilation,
  Python execution, host tool waits, workspace coordination, and resumption
  all remain inside the existing per-operation timeout, cumulative Agent
  Python budget, and invocation deadline. These are wall-clock budgets:
  suspended host-tool waits and workspace/worker waits consume Python
  operation time just as running code does. Retry does not restore spent
  Python time or tool requests. Returning, failing, cancellation, timeout,
  and shutdown stop further dispatch, cancel outstanding host work, settle
  or reap the worker, and release session resources. Detached tool tasks
  must not continue beyond the invocation's cleanup. Blocking extraction
  already started may finish its bounded operation before cleanup releases
  the workspace; it must not outlive that cleanup or publish a late result.
- **REQ-013 — Workspace correctness.** Nested `fs.*` calls use the same
  read-only `/inputs` and bounded `/work` as their Python caller. A nested
  `fs.extract` followed by reading its output must work without acquiring a
  workspace lock already held by the suspended program. Concurrent scratch
  mutations are serialized, output paths retain no-overwrite semantics, and
  Python writes plus extraction share the existing scratch/write budgets.
  Reads after an awaited mutation see its completed result. No snapshot
  ordering is promised between independent concurrent calls. Partial scratch
  writes from failed work retain existing per-invocation lifetime; no rollback
  or automatic replay is promised.

Two startup-only host settings extend the existing verification limits:

| Setting | Default | Meaning |
| --- | ---: | --- |
| `WYRD_VERIFICATION_AGENT_TOOL_MAX_CALLS` | 128 | Total direct and bridge requests per logical Agent invocation, across retries |
| `WYRD_VERIFICATION_TOOL_ARGUMENT_MAX_KIB` | 64 | Maximum compact UTF-8 JSON argument-object bytes per direct or nested call, including `code.run` source |

Both are positive, checked integers, with no disabling value or Card/TOML
override. Empty, zero, negative, malformed, or scaled-overflow values refuse
startup with the setting's name. Argument overflow uses
`WYRD_TOOL_422_INPUT` before execution and must not allocate or retain an
unbounded argument record. The ordinary
`WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` default remains 64. The new argument
limit intentionally also bounds existing direct calls; oversized native
programs must be reduced or the host limit raised. No new concurrency option
is introduced.

### Evidence and model visibility

- **REQ-014 — Intermediate visibility.** Successful nested results go only
  to Python. They are not appended as model conversation messages, replayed
  into later model turns, or automatically printed. For that code execution,
  only the existing bounded outer `code.run` value/printed output reaches the
  model. Server-side tool-call records and permission audits remain outside
  the model conversation. A model may request additional direct calls normally.
- **REQ-015 — Complete bounded records.** Record every admitted direct and
  nested call once in Wyrd's server-side execution records, in invocation
  start order. These records contain the tool name, bounded arguments,
  recipient, returned-value digest/byte count, and safe error; they do not
  retain the returned payload or append messages to the model conversation.
  Record permission denial,
  caught tool errors, completed calls preceding a later program failure,
  and calls cancelled after admission. Preserve completed call records
  across authored retries. Retain these records whenever an execution
  produces a result, including a failed step or inconclusive check. An
  admitted call delivering no value has no digest and zero returned bytes;
  it records the safe failure or cancellation. Pre-dispatch target/argument
  rejections produce no leaf record; they still spend the request budget.
  Existing cancellation paths that produce no VerificationResult do not
  acquire a new durable journal or result guarantee.
- **REQ-016 — Honest recipients.** Add `recipient: model | code` to
  `ToolCallRecord`, defaulting to `model` when absent. New records explicitly
  include it. Digest and byte count describe the bounded JSON value actually
  delivered to that recipient; a nested result must not be described as
  model-visible. The outer `code.run` has recipient `model`, and its children
  have recipient `code`. For a call returning no value the field identifies
  the intended recipient. Arrays remain flat and ordered; this change adds
  no call-tree IDs or raw-output retention.
- **REQ-017 — Check retention.** Retain optional/default-empty `tool_calls`
  on the descriptive Agent requirement report, using the same `ToolCallRecord`
  contract as step calls. Under the incoming compact/detail contract these
  reports are read explicitly through execution details; compact judgments
  do not regain descriptive fields. Retain direct and nested calls for Agent
  checks, including valid custom-data checks and inconclusive output/citation
  failures. Non-Agent requirements have no tool calls. Sensitive result
  capture, authorization, redaction, and retention continue to govern these
  records; disabling outcome capture must not expose their arguments through
  another surface.
- **REQ-018 — Evidence integrity.** Preserve original source slots and
  locators through tool results. Derived Python values and `/work` are not
  independent citable evidence. Passing Agent checks still require
  server-validated citations into reachable pinned evidence and the exact
  declared Verdict/data format. Live Bifrost reads remain outside the pinned
  evidence snapshot; code mode does not make them replayable or citable as
  pinned inputs.

## Required boundaries and lasting invariants

| ID | Invariant | Failure prevented |
| --- | --- | --- |
| INV-001 | `wyrd-spec` owns wire/record/error contracts; Vala owns Agent verification orchestration; the hosted server owns tool IO and Monty resources; SDKs project the result. | Durable execution or permission behavior moving into a client or duplicate runtime. |
| INV-002 | Python runs in the existing process pool; all protected tools execute in the parent server under accepted authority. | Credentials, server state, or authorization logic entering the worker. |
| INV-003 | Callable tools remain the invocation's declared read/compute built-ins, with recursive code execution excluded. | Global registry escape, worker recursion, external writes, or an arbitrary application runtime. |
| INV-004 | Permission decisions use the existing non-blocking Scribe audit path and each capability's tenant controls. | A bridge-only authorization bypass, cross-tenant read, or a second audit authority. |
| INV-005 | Portable interpreter, allocator, parent-mediated file/tool access, and deadlines enforce isolation identically on supported platforms. | Introducing credential switching, OS-specific sandboxing, host package loading, raw network, subprocess, or host-file access. |
| INV-006 | Reduced model output does not remove per-call evidence or convert execution failures into subject failures. | Token optimization erasing accountability or producing a false judgment. |

The parent driver must recognize only the installed bridge callable and
approved mounted-file operations. A suspended worker sends a request, never
an authenticated service operation. The server supplies identity and resolves
the permitted tool in the current session, runs its ordinary recorded call,
then returns a bounded value or safe error. The existing pool protocol
already supports host-function and pending-future events; this change does
not introduce another worker transport or tool service.

The concrete way a session binds its dispatcher, coordinates workspace
ownership, converts values, and drives pending futures remains private.
It must satisfy the lifecycle and concurrency guarantees above without
owning-reference cycles or acquiring a second worker for a nested call.

## Public compatibility and authority reconciliation

Existing Card names, `tool_names`, Agent/Prompt references, workspace
declarations, `code.run` payloads, Verdict formats, execution routes, and
direct-tool availability remain. No migration or rewrite of registered Cards
is required. Agents with `code.run` gain the bridge to their already-declared
tools; there is no mode selector. Pure authored code actions/checks do not.

The two record additions are additive: historical records without
`recipient` decode as `model`, and historical requirements without
`tool_calls` decode as empty. Existing digests and result bytes are not
rewritten. New SDKs must read both forms, preserve the new fields on their new owning reports, and project them through
Rust, Python, TypeScript, HTTP and MCP. The compact/detail reconciliation below
identifies the current read surface; historical stored bytes remain unchanged.
Update authored Python stubs and generated TypeScript/contracts from their
owners. Rust struct construction sites must be updated in the same change.

An old server cannot execute the new Python bridge. New prompts/programs
depending on it must only target upgraded verification workers; an old worker
must fail explicitly rather than silently omit calls. The change is not a
claim that old SDKs expose new fields or that mixed server versions provide
equivalent code-mode execution. Rollout and downgrade must follow the
deployment manifest's existing compatible-version rules; no destructive
storage change is introduced.

The following authority corrections are part of the approved change's
required delivery, before or with the behavior they govern:

| Owner | Required reconciliation |
| --- | --- |
| `architecture/wyrd-design.md`, Verifier / Agents, tools, and workspaces | Define `call_tool`, shared budgets/concurrency, nested-call evidence, and the additive requirement records. Replace the worker paragraph's credential-switching and OS-enforced isolation wording with the portable process-pool/interpreter/allocator and parent-mediated access boundary required by root rules. A mediated tool read is not direct worker network access. |
| `architecture/wyrd-security-posture.md`, accepted verification authority | State explicitly that Python-originated tool calls use the same accepted authority and per-call authorization/audit boundaries. |
| Agent tools and runtime-limits docs | Explain direct versus nested execution, error handling, recipient semantics, settings, cleanup, and the limits of token savings. Remove descriptions that imply only steps retain calls or that Python can never request an authorized host tool. |

These corrections supersede the identified prose, not the accepted-authority,
read-only verification, Source, or Bifrost contracts. No Bifrost storage,
query semantics or audit changes. Bifrost's resource derivation and
reservation roots compose with the process plan (REQ-027); explicit cgroup
detection/pressure probing is removed and its consumers reconciled. Oracle's
query-class fairness, deadlines, error codes and queue contract remain owned
by Bifrost and are not replaced by hosted admission. Startup configuration
permits the necessary process ceilings; it adds no Card field, SDK execution
policy or public timing API. This specification makes the replacement
explicit so implementation drift is not treated as new authority.

## Acceptance and user journeys

The AC identifiers below are acceptance obligations, not test names, test
counts, or a requirement for a new test per row. Several obligations describe
different assertions in the same execution. Reuse existing evidence for
unchanged behavior and extend existing tests before adding another test or
server startup. The coverage grouping below identifies where each proof
belongs; a single test may satisfy several AC items.

Public journeys use the real public client, `WyrdTestServer`, repository
Postgres, and the actual Monty worker executable. Provider behavior may be
scripted locally for deterministic requests and to inspect model-visible
messages; the interpreter, permissions, tool owners, and result transport
must be real. No credentials or paid provider are required.

### Functional scenarios

| ID | Situation exercised | Expected observation |
| --- | --- | --- |
| AC-001 | An Agent step and an Agent check each run one Python program containing dependent declared tool calls. | The second call consumes the first result without a model turn between them; the step returns declared data, the check returns a valid cited Verdict, and both retain recipient-tagged call records. |
| AC-002 | The same Agent makes direct calls and nested calls using existing descriptors. Also run a computation-only program and an Agent with only `code.run`. | Direct contracts remain valid; the model receives existing descriptions/schemas plus bridge guidance. Computation remains valid, and undeclared tools are unavailable even if registered elsewhere. |
| AC-003 | Call unknown/undeclared names, nested `code.run`, and an arbitrary external function; submit extra identity fields and unsupported Python values. | No target or recursive worker runs; errors carry the specified safe code, argument conversion is lossless for valid JSON, and invalid values never reach the capability. |
| AC-004 | For direct and nested calls, vary exact tool permission and downstream data permission independently; include same-named data in another tenant and delegated authority. | Both grants remain necessary, a narrow grant stays narrow, foreign/unknown objects retain existing indistinguishability, and each real permission check stages its normal audit decision with unchanged attribution. |
| AC-005 | Catch an ordinary tool error in Python and continue with an authorized call; separately leave it uncaught. | The caught error has a parseable catalog problem, the denied call yields no data, and both calls are recorded. An uncaught error fails the step or makes the check inconclusive without losing earlier admitted records. |
| AC-006 | Lower the request budget, then mix direct, nested, denied and malformed requests, concurrency, and retries; also try to catch exhaustion. | Successfully consuming exactly the allowance can still produce a final answer; the next request is refused and stops further tool/model work with the specified code. Spending never exceeds the budget, retries do not replenish it, and a fresh execution receives a fresh budget. |
| AC-007 | Run concurrent Python tool calls together with direct calls at effective concurrency caps 1 and 2. | Active non-code operations never exceed the shared cap, dependent work completes at cap 1, each query has its own request ID, and independent completion order does not corrupt return values or record start order. |
| AC-008 | Extract a mounted document from Python, read the generated file, and race writes/extractions to the same destination and near the scratch limit. | The awaited output is readable without deadlock, one conflicting destination succeeds, no overwrite occurs, shared write/scratch limits hold, and unselected inputs remain inaccessible. |
| AC-009 | Cancel or time out while awaiting a slow query, during extraction, and with unresolved Python futures; also spend the Python time budget mostly in host waits, retry, then reuse the worker/session capacity. | Host waits consume operation and cumulative wall-clock time without retry replenishment. No further calls dispatch or late results publish; bounded cleanup completes, active work is cancelled or settled, workspace resources are released, and the next invocation runs without prior Python or tenant state. |
| AC-010 | Refuse worker/workspace or downstream hosted process capacity, including a refusal inside a program that catches exceptions; repeat on queued work. | No partial judgment is published, the session propagates Busy, queued attempt accounting is unchanged, and after capacity returns the invocation can run normally. |
| AC-011 | Cross argument, result, output, and conversion limits; return oversized/malformed Verdict data or fabricated/scratch citations; also return valid pinned citations after reduction. | Oversized arguments dispatch nothing; a failed bounded tool return has zero bytes/no digest; existing explicit outer-output truncation markers remain; invalid judgments are inconclusive and valid original citations survive. |
| AC-012 | Read results for successful, inconclusive and retried Agent checks/steps, including a completed nested result hidden from the model and a call cancelled after admission. | Every admitted call is accounted for once, recipient/digest/size match the delivered value or absence, no raw intermediate output is added, and records remain under the ordinary sensitive-result permissions. |
| AC-013 | Decode historical result JSON and new recipient-tagged results through each SDK and served schemas. | Absent fields retain the specified defaults, new fields round-trip, custom requirement data is unchanged, and generated contracts/stubs match the owning Rust contract. |
| AC-014 | Start with invalid values for the two new settings, then run with lowered valid settings. | Invalid configuration prevents readiness naming the setting; valid settings enforce the documented limits without a Card override or disabling sentinel. |
| AC-015 | Run authored code steps/checks and attempt to call the bridge; run ordinary workspace file operations and isolate escape attempts. | Authored code has no bridge; supported file/computation behavior remains, and raw network, host environment/files, subprocesses, packages and undeclared callbacks remain inaccessible on every supported platform. |

### End-to-end journeys and efficiency proof

- **AC-016 — Reduced evidence, supported judgment.** An ML engineer declares
  a Verifier with one Agent preparation step and one Agent check, binds exact
  input/Artifact files, and declares `code.run` plus the required `fs.*` tools.
  Through the shared Rust public-client journey, execute it, read the result,
  and validate its original-evidence citations and tool records. Run the
  efficiency comparison once in this shared coverage, not per SDK. For it,
  use identical descriptors and fixed evidence containing 20 independently
  readable text results of at least 4 KiB each, with small relevant values.
  Compare ordinary direct reads with one Python program reducing those same
  reads. Both must produce the same supported judgment and all leaf-call
  evidence. The code-mode run must use one outer code tool request and one
  response for that reduction, with combined model-visible tool-request and
  tool-result UTF-8 JSON bytes at most 25% of the direct equivalent, including
  the generated program. Count the outer exchange only as model-visible;
  include all nested operations separately in server records. Record provider
  token usage when available, but do not equate byte counts to tokens or
  impose a nondeterministic provider-token threshold. Also exercise three
  dependent operations to prove no intermediate model turn is required.
- **AC-017 — Least-privilege recovery.** A platform engineer runs the same
  declared program with an exact tool grant but missing table permission,
  with table permission but missing tool permission, and with both. The first
  two attempts disclose no protected data and retain ordinary denial records;
  the third succeeds with the same tenant and delegation. Repeat a submitted
  execution through queue recovery using accepted authority, and verify that
  a later grant change does not refresh that authority. Audit-outbox failure
  does not change the permission outcome. Include a foreign-tenant target
  through both direct and nested paths.
- **AC-018 — Bounded failure and subsequent success.** A user submits a
  program that requests a call beyond its budget, then a program cancelled while a
  tool is pending. Neither produces a false failed-subject judgment or
  continues dispatching. The normal execution status/result surface exposes
  the appropriate failure or cancellation. A new authorized invocation then
  succeeds, proving cleanup and budget/session isolation. Include queued
  capacity refusal and recovery without consuming an authored attempt.

Extend the existing verification journeys through Rust, Python, TypeScript,
and MCP/HTTP verification/result surfaces. Each other surface needs a focused
extension that authors the same declaration, executes a nested call, receives
a permission denial, and inspects the projected records through its actual
public API. Success, denial, and readback may be phases of one journey using
the same server. Do not repeat the full server fault/concurrency matrix,
data-reduction comparison, or historical JSON matrix in every SDK.

### Evidence grouping and test reuse

| Coverage group | Acceptance covered together | Smallest credible proof |
| --- | --- | --- |
| Successful orchestration and reduced model input | AC-001–002, AC-012, AC-016; valid-result/citation portions of AC-011 and AC-013 | Extend a shared Rust public-client verification journey. Its step/check executions can also assert record recipients/digests, supported citations, descriptor reuse, model-visible messages, and reduction. These assertions do not need separate server tests. |
| Declared tools, permissions, and error recovery | AC-003–005, AC-017 | Extend accepted-authority server coverage with table-driven direct/nested cases and caught/uncaught branches; use the public-client negative flow to prove exposed denial/result behavior. Reuse existing queue-recovery, tenant-isolation, and audit behavior tests, adding nested execution at the affected seam. |
| Bounded execution, workspace, and cleanup | AC-006–010, AC-018; resource/isolation portions of AC-011 and AC-015 | Extend existing real-worker/workspace tests for the changed bridge seam. Pair them with a public execution-status/recovery flow for user-visible budget/cancellation/Busy behavior. Separate tests only where a distinct fault or lifecycle requires isolation; do not combine all faults into one opaque test. |
| Pure contracts and startup validation | Conversion portions of AC-003/AC-011, historical records in AC-013, AC-014 | Focused unit/schema/configuration tests; no server journey is needed to prove a pure conversion, deserialization default, or rejected startup value. A journey still proves the corresponding externally visible argument/result failure. |
| Other public surfaces | Core success, denial, and record projection from AC-001, AC-004, AC-012–013 | Add phases to existing Python, TypeScript, and MCP/HTTP verification journeys. Keep interpreter and exhaustive authorization/lifecycle proof in shared server coverage. |
| Unchanged behavior and boundaries | Existing direct calls, authored code isolation, resource caps, citations, tenant controls, and audit guarantees within AC-002, AC-004, AC-010–011, AC-015, AC-017 | Reuse the owning regression evidence where the implementation path is unchanged. Add assertions for bridge reachability or changed behavior; do not reproduce the existing subsystem's complete test suite. |

This grouping does not prescribe a private test layout or a fixed number of
tests. Planning must map the obligations to existing tests and identify only
the missing observations. A new integration/journey test must cover a
distinct gap that cannot be expressed clearly by extending the existing
coverage. Retain separate assertions and failure messages within grouped
tests so a failure identifies the violated behavior. SDK wrappers never
emulate the server or interpreter.

### Requirement-to-proof mapping

| Obligations | Acceptance evidence |
| --- | --- |
| REQ-001–004 | AC-001–003, AC-016 |
| REQ-005 | AC-003, AC-011 |
| REQ-006–007; INV-001, INV-004 | AC-004, AC-017 |
| REQ-008 | AC-003, AC-005 |
| REQ-009 | AC-005, AC-006, AC-009–010, AC-018 |
| REQ-010 | AC-006, AC-014, AC-018 |
| REQ-011 | AC-007 |
| REQ-012 | AC-006, AC-009–010, AC-018 |
| REQ-013 | AC-008, AC-011 |
| REQ-014 | AC-001, AC-012, AC-016 |
| REQ-015–017; INV-006 | AC-005, AC-009, AC-012–013, AC-016 |
| REQ-018 | AC-001, AC-011, AC-016 |
| INV-002–003, INV-005 | AC-003, AC-009–010, AC-015 |

The grouping assigns pure logic to unit evidence, actual interpreter behavior
to real-worker integration evidence, and externally visible success,
permission, negative, and result-readback flows to real-client journeys.
Unit evidence cannot substitute for a missing user-visible flow. Static
review proves ownership and absence of a second authorization/runtime
implementation.

## Canonical synchronous benchmark extension

The maintainer's additional outcome is a representative measurement of the
immediate `observe.verify(...)` path, including one Assert requirement and two
concurrent Agent requirements that each use a mock gateway and real `code.run`.
The benchmark must show server-induced latency (`server`) and end-to-end
client–server–client latency (`client`) for each public SDK. Include ALL elapsed
work within the public call, including provider waiting, sandbox execution,
queues, IPC, retries, transport and language conversion. Phase metrics and
profiles explain latency without subtracting it. All SDKs establish total
baselines; retire the former adjusted-client 500 µs target without transferring
it to total latency. This is a local release benchmark, not a universal SLA or
new CI performance gate.

- **REQ-019 — Canonical workload.** Replace L1 `direct-verify-c1` and
  `direct-verify-c64` assert-only fixtures with the same three independent
  whole-Verifier requirements: one Assert and two Agent checks. Each Agent
  uses the ordinary governed gateway against a deterministic local mock
  provider, requests exactly one real `code.run`, then returns a valid cited
  Verdict after observing its result. This is two model exchanges per Agent,
  four gateway exchanges and two code operations per verification. Real
  authorization, Agent orchestration, workspace/mount/pool handling, recording,
  result assembly and SDK transport remain. Use fixed evidence and code;
  retain deterministic passing/failing arrivals and distinct per-application
  Service identities. No nested leaf tool is required in this performance
  fixture; functional bridge coverage remains in TASK-001. Every language runs
  the same canonical fixture against the same Rust server build and settings,
  in separate populations. L2's assert-only
  workload, rates and verdict contract stay unchanged.
- **REQ-020 — Honest latency boundaries.** Preserve existing server elapsed,
  phase and server-overhead residual metrics with their existing meanings.
  Measure total client round-trip immediately around public verify through
  decoded return/error. Include every elapsed part: provider exchange, sandbox
  execution/waiting, authorization, scheduling, compilation, checkout, IPC,
  host tools, retries and decoding. Fixture preparation and post-return
  assertions stay outside. Instrument or profile enough of the path to identify
  latency sources. Label inclusive envelopes, overlapping phases and unobserved
  gaps honestly. Never subtract phases or percentiles from client totals or
  call a sandbox wall-time envelope pure Python CPU.
- **REQ-021 — Qualification and reproducibility.** All three SDKs establish
  total client baselines at both counts without a numeric client objective.
  Preserve the existing c64 server-overhead criterion separately. Keep existing
  release repetitions, sample floors, quiet-host checks, precision, append-only
  records and VALID/INVALID versus met/missed/unsettled semantics for declared
  criteria. Record fixture, timing method, coverage, limits and host/build
  identity. Legacy workloads and adjusted results keep their original meanings.
  Do not hide errors, missing samples or capacity failures, or raise
  constants, alter the canonical workload/fan-out or relax gates to qualify;
  only the resource-backed capacity correction (REQ-027–033) changes capacity.
  Common measurement windows may be extended uniformly when necessary to obtain
  the unchanged sample floor; record that method change. Missing diagnostic coverage leaves diagnosis unfinished without
  invalidating otherwise sound total-time evidence.

- **REQ-022 — Native language callers.** Extend the existing experiment with
  small Rust, Python and TypeScript drivers calling their public SDKs, each at
  1 and 64 in-flight calls against an equivalent separately run Rust server.
  Reuse one workload, provisioning/mock definitions, validity policy, record
  and renderer. Use one client application process per language with the same
  recorded CPU budget: bounded Rust/Node async calls and Python's existing
  synchronous API in prestarted bounded threads. Setup/warmup is outside the
  steady-state timer. Qualifying runs use release-native SDK artifacts and
  verify loaded module identity; a development/testing extension cannot be
  presented as release SDK performance. Record runtime/package/native identities, concurrency and
  all process roles; do not combine language samples or substitute raw HTTP.
- **REQ-023 — Comparable client boundaries.** Retain the public-call-return
  sample defined in REQ-020, measured in the caller language through its normal
  returned result/error. Additionally measure result-ready latency from that
  same start through materialization of the same compact verdict, requirement
  judgments, citations and custom JSON data. This includes deferred property
  conversion, not arbitrary formatting, assertions or an extra server read.
  Record the materialization phase separately. Keep typed custom-data model
  validation outside this untyped comparison; no new typed case is required.
  Both call-return and result-ready retain all elapsed work. SDK conversion
  and shared IO phases are nested evidence, never extra completed requests.
  Correlate durations without serializing opaque cross-process monotonic epochs
  or subtracting unrelated percentiles.
- **REQ-024 — Client and server breakdowns.** Every language's canonical
  benchmark must provide attributable phase latency and CPU/memory diagnosis on
  both sides. Client evidence covers language input/native-boundary work,
  shared-client preparation/serialization, authentication, HTTP attempt/body
  receipt/decoding, and language return/materialization. Server evidence covers
  admission/authorization and input preparation, orchestration/workspace setup,
  Gateway/provider work, sandbox admission/execution/IPC/host waits, and result
  assembly/response. Keep coarse inclusive envelopes labelled; never call them
  pure CPU, add overlapping phases, or derive costs from unrelated percentiles.
  Use the existing shared recorder and profiler owners, with minimal bounded
  language-boundary observations; no independent SDK profiler/recorder systems,
  artificial busy-loop/held-allocation probes or capture-validation framework.
  Diagnostic captures must show real SDK and server CPU hot paths, native and
  Python/V8 allocation sites and allocation volume versus retained memory.
  Relate findings to reached phases, process roles and the relevant invocation
  or measured window; disclose shared-runtime/GC/overlap uncertainty rather than
  assigning shared costs arbitrarily. Peak RSS alone, server-only profiles or
  opaque native calls do not establish client allocation diagnosis. Fixture and
  controller work must be distinguishable from SDK work; mixed process totals
  alone cannot be presented as client-only costs. Record executable commands,
  versions, build identities and inspected artifacts. Missing a required phase,
  language/native allocation domain or attribution boundary blocks diagnostic
  closeout; valid total-latency evidence may still stand. Profilers run separately
  from unprofiled baselines. No complete diagnostic coverage is claimed until
  it has been demonstrated on actual SDK calls.
- **REQ-025 — Incoming contract composition.** Import the committed
  resource-scaling range and its approved benchmark-latency-diagnosis authority
  before benchmarking. Use its compact decisions and explicit details reads,
  one benchmark recorder, client instrumentation and standard role-aware
  profiles/reports. Preserve complete direct/nested step/check records in
  execution details on successful, failed and inconclusive results. Compact
  result access must trigger no details request. Details remain ordinary
  authorized, potentially unavailable retained reads with no new durability
  barrier or judgment rerun. Public journeys migrate assertions to the owning
  read surface on Rust/Python/TS/HTTP/MCP; they still prove every original
  record/citation/permission obligation. This is composition of the existing
  owners, not a new details endpoint, fallback or lifecycle service.

- **REQ-026 — Compile-time benchmark isolation.** Benchmark timers, recorders,
  registries, interval/envelope state, allocation hooks, collector lifecycle and
  harness-only activation/readout are compiled only with the existing `bench`
  capability. Production server, Rust client and Python/TypeScript packages
  must exclude that machinery, not merely leave it disabled. Preserve normal
  operational metrics and telemetry. A zero-state/no-measurement facade may
  preserve common call-site compilation, but must contain no timing, registry,
  collector or allocation logic in the non-bench build. Reuse the shared owner;
  minimum feature forwarding and harness wiring in diagnostic bench builds is
  permitted, with no new public SDK benchmark API or separate SDK product.
  Bench-enabled release artifacts remain local benchmark artifacts, identified
  as such and kept out of wheel/npm publication. Record the feature mode in each
  run; unprofiled means profilers off, not timing observations absent. Production
  package checks must build and import the actual non-bench wheel/addon in
  isolation, inspect its feature/dependency/package boundary and demonstrate
  that benchmark controls/collectors are unavailable. A runtime-disabled test
  or workspace test build with unified bench features is not exclusion proof.

| ID | Invariant | Failure prevented |
| --- | --- | --- |
| INV-008 | Every language population runs the same workflow and provides client/server phase and native/managed CPU/allocation evidence with attributable process/window boundaries. | Treating total latency, RSS, server profiles, missing SDK phases or opaque native calls as complete client diagnosis. |
| INV-007 | Client samples retain the whole public-call elapsed time; bounded phase/profile evidence explains costs without exclusions, overlap sums or new public timing contracts. | Hidden provider/sandbox/queue/conversion costs, double-counting nested work, unbounded request labels or falsely complete attribution. |
| INV-009 | Benchmark machinery exists only in bench builds; production artifacts contain no benchmark measurement/collector state or activation/readout surface. | Shipping dormant recorders, allocator hooks, envelope allocations or accidental bench feature unification in production SDK/server artifacts. |

Instrumentation overhead remains measured. Late completions retain existing
window accounting. Missing, duplicate or impossible total samples invalidate
timing; missing phase evidence is an explicit attribution gap, never zero.
Monty execution counters and its telemetry adapter are not prerequisites.
Reuse parent-side sandbox/pool observations and worker profiles, naming their
inclusive boundaries and coverage. Do not add adapter dependency versions or
patch Monty merely for the retired subtraction method. Keep typed worker/mount
handling and the sole benchmark collector. Correlation stays bounded, with no
permanent request-ID labels, payload/source/credential capture, public timing
field, durable journal, new transport or production timing mode.

| ID | Acceptance evidence |
| --- | --- |
| AC-019 | Real release-server canonical runs through each public Rust, Python and
TypeScript SDK at 1 and 64 clients exercises all three checks, four governed mock exchanges and two real code operations for each acknowledged verification, with correct cited results/records and deterministic passing/failing arrivals. Counts and interleaved clients reconcile; no bypass or canned result can substitute for executed code. |
| AC-020 | Deterministic evidence proves whole-call boundaries, retained provider/sandbox/queue/conversion delays, correct out-of-order/late accounting and invalid total-sample handling. Retained server observations identify both Agent paths; real SDK diagnostic evidence preserves the required client/server phase boundaries and labels overlap. Non-bench compilation excludes envelope measurement state and timer/lock paths. |
| AC-021 | Full L1 evidence records total call-return/result-ready client and preserved server metrics for six language/count populations, at least 1,000 p99 samples per repetition and common integrity rules. No adjusted-client or replacement numeric total-client target is declared. Preserved server criteria retain existing judgment rules; errors, incorrect outcomes or missing total samples cannot qualify. |
| AC-022 | L2 workload/declarations remain unchanged, reports rebuild from schema-3 records and legacy results retain original meanings. README/task help distinguish total client latency, server elapsed/phases and historical server residual; phases never reduce client totals. |

| ID | Acceptance evidence |
| --- | --- |
| AC-023 | Each real SDK run at both counts executes the identical canonical checks/code/provider exchanges, returns the compact decision and explicitly reads original evidence/records through details outside the verify timer. Calls and typed/public consumer migration preserve citations and denial behavior; no language driver delegates timed calls to a Rust-only helper. |
| AC-024 | Deterministic and real SDK-boundary tests distinguish call return and deferred materialization, including error accounting. Real Rust/Python/TypeScript calls retain shared-core/HTTP phase readouts and language conversion boundaries from the existing shared collector in bench builds; missing language readouts cannot satisfy acceptance. One completed user call is counted once; the result-ready comparison uses the same fields and no extra network request. |
| AC-025 | Unprofiled release evidence publishes all six language/concurrency populations with common integrity/sample rules, recorded bench feature mode and the preserved server criterion. Separate real-workload diagnostics establish client and server phase latency, CPU hot paths and native plus Python/V8 allocation/retention evidence, attributed to process/window and relevant phases. Missing mandatory domains block closeout; totals/RSS or idle/synthetic captures cannot substitute. Saved reports retain the phase populations, repetitions, process scopes and coverage metadata without raw artifacts. |
| AC-026 | Focused real SDK/MCP/HTTP composition proof preserves compact decisions, explicit authorized details, complete code-mode records and historical defaults. Missing/not-yet-visible details cannot become empty success or change a completed verdict. Existing code-mode cleanup/capture/security tests remain intact across the import. |
| AC-027 | Isolated non-bench server/Rust/Python/TypeScript builds and actual production wheel/npm import/contents checks exclude benchmark recorder/registry/timing state, allocation hooks and controls. Bench-enabled counterparts still provide the shared diagnostic readouts. Test/dev feature unification is identified and cannot serve as production-isolation proof. Existing production telemetry and SDK behavior remain. |

### Integration and language scope reconciliation

The maintainer's agreement selects native three-language callers, Python
threaded synchronous concurrency, measured Python/TS baselines and separate
call-return/result-ready observations. All client measurements retain all time; the adjusted-client target is retired.
The existing server-overhead criterion remains separate.
There is no new asynchronous Python API, three benchmark frameworks, profiling
production mode, language-specific execution server or extra L2 workload.

The incoming source's approved compact/detail and Gateway governance contracts
remain governed by their narrow architecture and specification owners. This
change adds code-mode report fields to their existing descriptive owner and
migrates its consumers; it does not revive old full default results or remove
record retention. Revision-2 acceptance remains historical evidence; changed
public seams must be re-proven, not carried forward on filename equality.


TASK-003 implements and qualifies this benchmark extension after TASK-002B
integrates the latest approved source while retaining TASK-001 and all its
applicable correctness remediation; it has no dependency on TASK-002's
other-language projection work. TASK-003-R2 owns the resource-backed capacity
correction below and the remaining closeout; no other general runtime
performance redesign is authorized. If a valid run misses the objective, retain
the evidence and diagnose the remaining overhead; do not claim the target or
loosen its boundary.

## Resource-backed capacity

The clean L1 record
`benchmarks/history/verification/direct-verify/20261114T130251Z-4bc727c82.json`
is INVALID. Every c64 refusal reconciles with immediate workspace refusal at
16 active workspaces; the fixture can need 128. Raising that count alone would
leave five effective Python workers and per-operation process destruction.
The record also misses Rust's server-overhead criterion (0.664 ms p99 versus
0.5 ms) and TypeScript c1's 1,000-sample floor (980–997 per repetition).
The reported server CPU excludes interpreter children and cannot establish
available CPU headroom.

This change makes one resource-backed capacity correction, owned
by TASK-003-R2, followed by the remaining TASK-003/R1 closeout. Resources follow
the process's discovered CPU, memory and filesystem availability, subject to
operator ceilings that only lower discovery. Hosted execution, recording and
Bifrost must not independently promise the same memory or scratch. Reuse
healthy worker processes with fresh interpreter sessions, wait fairly under
bounded accounting, and prove isolation and recovery. The costs are bounded
queue latency, resident memory for suspended sessions, reset work, and shared
accounting across existing owners. No global scheduling service is introduced.

### Requirements

- **REQ-027 — One process plan.** Before starting consumers, discover CPU via
  `std::thread::available_parallelism`, memory and filesystem space through
  portable interfaces. Explicit operator ceilings may only lower discovered
  resources. Fail readiness with an actionable resource/configuration error if
  a safe plan cannot be resolved; do not substitute guessed capacity. Charge
  hosted workspaces, retained inputs, workers, waiting requests and recording,
  plus Bifrost's memory/scratch consumers, under this plan without double
  promises. Account worker baseline, enforced heap, stack, bounded IPC and
  measured headroom; an allocator ceiling is not an RSS ceiling. Replace the
  unevidenced 16-workspace, 2/8-worker, 16-waiter and 2-GiB-pool defaults with
  derived capacity. This includes the HTTP edge's fixed 1,024 in-flight
  request bound and its buffered-body memory: once verifications wait instead
  of refusing, in-flight requests rise, so edge admission must charge the same
  plan and must not shed with an unexplained generic 503 ahead of fair hosted
  admission. Remaining fixed capacity values require evidence, not just
  tests of their arithmetic. Preserve content/security and per-operation limits.
  No explicit cgroups, `/proc`, namespaces, `prlimit`, credential switching,
  platform gating or OS sandbox APIs.
- **REQ-028 — Fresh sessions in reused workers.** Reuse Monty's subprocess
  finish/reset lifecycle. Successful reuse proves no Python globals, mounts,
  callbacks, output, accepted authority or allocator/session accounting crosses
  operations or tenants. Discard crashed, cancelled and failed-reset workers.
  Runnable Python is bounded by effective CPUs; suspended sessions retain memory
  but release runnable permits. Memory bounds resident sessions, including idle,
  starting, resetting and reaping workers until ownership is settled.
- **REQ-029 — Fair atomic admission.** Direct and queued hosted execution use
  FIFO within tenant and work-conserving round robin across tenants on each
  replica. A busy tenant cannot bypass another eligible tenant by refilling.
  Bound waiting by accounted retained bytes rather than a guessed waiter count.
  Acquire each required resource set atomically; never wait holding a partial
  acquisition or a permit needed by nested work. Existing resident/scratch
  ownership retained during suspension stays charged; it is not a speculative
  partial acquisition. Keep sufficient unpromised capacity for admitted nested
  work to make progress. No second authorization, worker transport or scheduler
  backed by PostgreSQL.
- **REQ-030 — Arrival deadline and cleanup.** Apply REQ-009's overload
  semantics consistently to HTTP, MCP and SDKs. REQ-012's budgets include
  admission waits; one absolute deadline begins at arrival and is never
  restarted after admission, lookup, checkout or resume. REQ-010's request
  budget and REQ-011's per-Agent non-code tool cap of 8 stay unchanged. Durable
  queue scheduling retains accepted authority, PostgreSQL-owned coordination
  time and existing attempt accounting; no new database semaphore or
  coordination protocol. Keep exact existing
  error codes, structured bounded resource/reason details, recording 429, queued
  deferral without spent attempt, and no silent client replay. Cancellation
  removes waiters, stops mediated work and settles/reaps workers before freeing
  their reservations; no late dispatch, result publication or double release.
- **REQ-031 — Observable capacity.** Expose the effective plan, bounded-label
  admission reasons, active/waiting counts and bytes, queue wait (also as a
  named server timing phase, so REQ-020's server-overhead residual keeps its
  meaning and admission waits are neither hidden in nor charged to it),
  runnable/suspended workers, and reset/spawn/reap counts through existing
  operational telemetry. Never label by tenant/request ID. Benchmark collection
  includes worker child CPU/memory and churn, separately from server, client
  and fixture costs; missing readings are unavailable, never zero. Preserve
  non-bench exclusion of benchmark instrumentation.
- **REQ-032 — Measured acceptance.** Measure server plus worker CPU seconds
  per completed canonical verification, then derive and record the c64
  CPU ceiling (effective cores ÷ CPU seconds per verification). The system's
  own measured utilization is not an input to its target. At c64, either
  parent-plus-worker CPU approaches effective cores, or the record shows no
  capacity waiting (load bounded by the 64 closed-loop callers), or it names the
  saturated non-CPU resource with its measurement. Idle CPU alongside capacity
  waiting with no named saturated resource fails: that is an artificial limit. Derive latency
  expectations from measured service/queue behavior, not arbitrary new client
  p99 targets. Freeze the derivation and calibration inputs before final
  qualification. Require zero c64 errors in every SDK/repetition, the unchanged
  server-overhead criterion, and at least 1,000 p99 samples in every population
  and repetition. Preserve all six public SDK populations and exact workload
  counts, clean-source/quiet-host gates and total-time boundaries. No proposed
  2,000/3,200 verifications/s or 50/100-ms targets from the diagnoses are adopted.
- **REQ-033 — Sustained recovery and closeout.** In addition to L1, run a
  sustained hundreds-of-tenants saturation/recovery experiment and a two-replica
  experiment. Show bounded queues/reservations, fair progress, stable
  parent-plus-worker memory/disk/descriptors and recovery after pressure ends;
  record offered load, durations, per-tenant progress and post-drain results.
  Fairness and capacity are replica-local, with no promise of global fairness
  or linear scaling. Close R1's V3 qualification, V4 AC-025 artifact analysis
  and evidence ledger, including legacy report regeneration; prior diagnostic
  smoke captures cannot qualify performance or prove the changed runtime.

| ID | Added invariant / acceptance obligation |
| --- | --- |
| INV-010 | All process consumers reserve from one discovered plan; suspension and cleanup never release still-owned memory or promise RAM/scratch twice. |
| AC-028 | Resource sizing, lowering-only ceilings, overflow/unavailable discovery and joint hosted/recording/Bifrost pressure are proven; impossible plans fail readiness and aggregate reservations never exceed their roots. |
| AC-029 | Real reused workers isolate tenants and sessions; suspend/resume honors CPU versus resident bounds, and failed reset/crash/cancellation cannot return a dirty worker or release ownership early. |
| AC-030 | Deterministic contention proves tenant FIFO/round robin, borrowing idle capacity, atomic accounting, byte-bounded waiting and nested progress. |
| AC-031 | Public SDK/HTTP/MCP journeys prove wait then success, full-queue 503 and retry metadata, arrival-deadline 504, recording 429, cancellation and queued deferral/recovery without spending an attempt or replaying a direct call. |
| AC-032 | Operational telemetry reconciles resource transitions with bounded labels; benchmark parent/child collection covers worker churn without relabelling parent-only historical CPU as total execution CPU. |
| AC-033 | Clean six-population L1 satisfies unchanged integrity/sample/overhead rules and the measurement-derived c64 throughput target; full results and derivation are retained even if targets miss. |
| AC-034 | Sustained hundreds-of-tenants and two-replica evidence establishes local fairness, finite resources and recovery; remaining R1 V3/V4/ledger obligations are accounted for and independently reviewed before TASK-003 closes. |

### Architecture owner obligations

TASK-003-R2 must reconcile these owning sections before or with their behavior:

- `architecture/wyrd-design.md`, **Verifier → Agents, tools, and workspaces**:
  retain fresh sessions and portable isolation; state reusable processes,
  runnable versus resident admission, tenant fairness, arrival deadlines,
  overload/retry metadata and settled cleanup. State the cross-product process
  resource boundary here; keep Bifrost mechanics in their owner.
- `architecture/bifrost-design.md`, **Query: Oracle → Admission and memory**:
  replace the independent “detected process or pod” envelope / fixed non-Bifrost
  headroom examples with composition under the discovered process plan and
  lowering-only ceilings. Remove cgroup assumptions; account recording and
  hosted reservations. Preserve Oracle class semantics and existing supported
  memory floor unless separately approved. Reconcile explicit capacity overrides
  that currently replace resource derivation outright.
- `architecture/bifrost-design.md`, **Ingest: Scribe → Shards and hierarchical
  ownership** and **Resource and failure invariants**: preserve complete-vector
  admission and once-only release while composing with the process root and
  recording reservations; document portable pressure/accounting instead of
  cgroup probes. Fixed queues/worker counts that remain need supporting evidence
  or derived bounds, not independent promises.

## Inspected owners and delivery verification

These are reuse evidence and affected seams, not prescribed private APIs or
a file allowlist:

| Owner | Existing workflow and required extension |
| --- | --- |
| `crates/wyrd-spec/src/card/agent.rs` | Owns existing declared built-ins and run-config concurrency; reuse them without new Agent fields. |
| `crates/vala/vala-verify/src/blocks/agent.rs`, `blocks/tools.rs` | Resolves declarations and wraps tools in bounded records; extend complete dispatch/record propagation for nested calls and check/failure results. |
| `crates/wyrd/wyrd-server/src/verification/hosted/tools.rs` | Binds accepted caller, checks `tool:invoke`, and invokes existing owners; add bridge binding and shared bounds without bypassing that workflow. |
| `hosted/sandbox.rs`, `worker.rs`, `workspace/` | Own pool checkout, host events, mounts, and scratch serialization; permit mediated callbacks while preserving portable isolation and lifecycle. Existing blanket FunctionCall rejection and full-operation workspace writer ownership cannot remain unchanged. |
| `crates/wyrd-spec/src/verification/execution.rs`, `src/error.rs` | Own the additive record fields, check records, and new budget failure; project through the existing client and every SDK. |
| `crates/wyrd/wyrd-server/src/config.rs`, `wyrd-testing` | Own host settings and real-server configuration/executable provisioning; extend the existing facilities. |
| Existing Card reads, `BoundedQuery`, Oracle, permission/audit owners | Preserve full read/authorize/admit/terminal workflows; no direct database or storage shortcut from Monty. |
| SDK contract projections, runtime schemas, docs | Keep Rust, Python stubs, generated TypeScript, HTTP/MCP and machine-readable guidance aligned. |

Existing real-worker and accepted-authority evidence lives in
`crates/wyrd/wyrd-server/tests/integration/sandbox_worker.rs` and
`pg_hosted_artifacts.rs`; these are useful seams, not substitutes for new
end-to-end journeys. Reuse `WyrdTestServer` and installed/exported executable
paths; never infer a worker path from a build-directory layout.

The existing focused lane is `mise run test:verification:sandbox`.
Implementation planning must supply exact selectors for every new or changed
named test, and place public journeys in the existing owning SDK/MCP lanes
under `TESTING.md`. Source/contract delivery also requires
`mise run fmt:check`, `mise run lints`, and `mise run codegen:check`, plus the
applicable Python/TypeScript stub, type, and package checks. Changed public
docs run through `mise run docs:check`. Broad `gate` execution is not required
for drafting this specification. No test result is claimed by this draft.

## Decisions and approval

Revision 4 records the maintainer's explicit “i agree” followed by
`$wyrd-plan` on 2026-11-11, approving the presented three-SDK benchmark approach,
Python/TS measured baselines, the then-preserved Rust target, native threaded/async
callers, result-readiness observations and import/result-details reconciliation.
The written requirements above capture those agreed outcomes without granting
implementation, commit, merge or push authorization. Profiling tool/private
runner mechanics were implementation choices under that historical revision; revision 6 narrows the required diagnostics and prohibits a custom collection subsystem.


Revision 3 records the maintainer's explicit benchmark requirements and request
to create TASK-003 on 2026-11-08, following the proposed scope. Approval of
the extension fixes REQ-019–021, INV-007 and AC-019–022; it neither changes
revision-2 runtime behavior nor claims a met performance objective.

Revision 5 records the maintainer's explicit 2026-11-12 direction to measure
ALL client time, retain attribution through metrics/profiles, and revise the
task. This supersedes revision 3/4 client subtraction and its target;
[revision 4](spec-revisions/revision-4.md) preserves the prior authority.

Revision 6 records the maintainer's explicit request to plan cleanup based on
the reviewed recommendations, including fixing production disabled-path costs
now and rejecting benchmark-only SDK builds. This supersedes mandatory synthetic
coverage probes and integrated language collectors; it does not abandon useful
latency/CPU/allocation diagnosis or authorize claiming incomplete evidence as
complete. It authorizes planning only, with no commit, push or task PASS.

Revision 7 records the maintainer's explicit agreement to the alignment review
and renewed wyrd-plan request. This requires usable breakdowns on both sides
and compile-time production exclusion now; revision 6's permissive phase gaps
and blanket bench-wiring prohibition no longer apply. Minimal bench-only reuse
of the shared collector is earned by these requirements. No duplicate recorder,
new public SDK benchmark API, commit or push is authorized. Profiler selection
and private harness wiring remain implementation decisions whose coverage must
be demonstrated, not assumptions in lieu of evidence.

Revision 8 records the maintainer's explicit 2026-11-14 approval of the
combined capacity design after two independent diagnoses: one portable process
resource plan with lowering-only ceilings, reused workers with fresh sessions,
runnable CPU separated from resident memory, tenant-fair byte-bounded waiting,
an arrival deadline and measurement-derived acceptance. It supersedes REQ-009's
immediate Busy refusal and the ban on runtime capacity changes only for that
correction. Private placement, measured formulas and fixtures remain
implementation decisions. No commit or push is authorized.

There are no unresolved material design choices in this revision. Approval
accepts the existing `code.run` extension, awaited `call_tool` interface,
unchanged provider-visible catalog, shared authority/dispatch, call and
argument limits and their defaults, concurrency/lifecycle semantics,
additive recipient/check records, authority corrections, and required
functional/journey evidence. Private Rust wiring and test fixture structure
remain implementation decisions.

Revision 2 is approved for planning by the maintainer's 2026-11-07 instruction
to `wyrd-plan`. Implementation tasks remain proposed until separately accepted.
This approval authorizes no Git commit or push and does not claim that code,
tests, or an implementation-readiness review are complete.

## References and revision history

Normative grounding:

- [Wyrd doctrine and Verifier contracts](../../../architecture/wyrd-design.md#verifier)
- [Accepted authority and tenant security](../../../architecture/wyrd-security-posture.md#principal-and-credential-lifecycle)
- [Bifrost query ownership](../../../architecture/bifrost-design.md#query-oracle)
- [Bifrost read audit](../../../architecture/bifrost-design.md#read-audit-and-terminal-contract)
- [Deployment compatibility](../../../architecture/operations/deployment-and-release.md#rollout-and-version-skew)
- [Specification workflow](../../spec-driven-development.md)
- [Testing tiers and lanes](../../../TESTING.md)
- [Agent-facing contracts](../../../agent_docs/agent-surfaces.md)
- [Error projection](../../../agent_docs/errors.md)

External background, already researched for this proposal; these sources do
not define Wyrd's contracts:

- [Code mode explanation](https://lucumr.pocoo.org/2026/10/6/codemode/)
- [Pi code mode](https://pi.dev/docs/latest/codemode)
- [Cloudflare code mode](https://blog.cloudflare.com/code-mode)
- [FastMCP code mode](https://gofastmcp.com/servers/transforms/code-mode)
- [Pydantic AI Harness code mode](https://pydantic.dev/docs/ai/harness/code-mode/)

| Revision | Date | Status | Change |
| --- | --- | --- | --- |
| 1 | 2026-10-30 | draft | Initial decision-complete proposal for declared-tool calls from Monty, shared authorization and limits, honest call evidence, and functional/user-journey acceptance. |
| 2 | 2026-10-30; approved 2026-11-07 | approved | Clarify server records versus model-visible payloads, require grouped acceptance and existing-test reuse rather than one test per AC, and run the reduction comparison once in shared coverage. Maintainer requested planning of this reviewed revision. |
| 3 | 2026-11-08 | approved | Add the maintainer-requested TASK-003 canonical L1 synchronous workload and adjusted client p99 objective. Revision-2 runtime obligations and issued tasks/reviews retain their original authority. |
| 4 | 2026-11-11 | approved | Maintainer accepted the cross-language benchmark proposal and requested planning: latest source composition, explicit details record projection, native Rust/Python/TS callers, comparable result readiness and language/native profiles; Python/TS establish baselines while Rust retains its objective. |
| 5 | 2026-11-12 | approved | Maintainer requires ALL elapsed client time plus latency attribution. Retire adjusted-client subtraction/500 µs target, establish all-SDK total baselines, preserve separate server criteria; Monty counters/adapter are no prerequisite. |
| 6 | 2026-11-12 | approved | Maintainer requests cleanup of partial TASK-003: remove custom client profiling/probes, fix disabled server envelope overhead now, keep common SDK benchmarks and useful real-workload diagnostics; prohibit special SDK builds/features/exports. |
| 7 | 2026-11-12 | approved | Maintainer requires the same three-language benchmark, actionable phase/CPU/native-and-managed-memory diagnosis on both client and server, and compile-time bench-only machinery. Permit minimal existing-bench wiring for diagnostics, preserve production API/package isolation and remove duplicated collectors/probes. |
| 8 | 2026-11-14 | approved | Maintainer approved resource-backed capacity after the clean L1 run failed on immediate workspace refusal: one portable process resource plan, reused workers with fresh sessions, runnable/resident separation, tenant-fair byte-bounded waiting, arrival deadline, measurement-derived acceptance, sustained multi-tenant and two-replica evidence. |
