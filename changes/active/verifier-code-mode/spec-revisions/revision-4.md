---
id: SPEC-verifier-code-mode
revision: 4
status: approved
---

# Tool orchestration in Verifier Agent code

Revision 4 records the maintainer's 2026-11-11 agreement to the cross-language
benchmark proposal and request for planning. It extends the canonical benchmark
to Python and TypeScript alongside Rust, preserving Rust's existing objective
and establishing measured language baselines. It reconciles code-mode evidence
readback with the compact-result / explicit-details contract already approved
in the incoming resource-scaling authority. No implementation or Git action is
authorized by this record.

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

Two costs are deliberate. A Python worker remains checked out while its
program awaits tools, so code mode can trade worker occupancy for fewer model
round trips. Also, every nested call still produces its own permission
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
  swallowed by Python to continue work or manufacture a verdict. Capacity
  refusal retains the current Busy behavior: immediate execution is
  unavailable; queued work is deferred without consuming an attempt.
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
query, admission, or audit mechanism changes. This specification makes the replacement
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
client–server–client latency (`client`) for each public SDK. Rust retains its
adjusted round-trip p99 objective strictly below 500 µs at 1 and 64 clients
after removing only mock-provider and Python interpreter execution time. Python
and TypeScript establish measured baselines, not an inherited 500 µs promise.
This is a declared local release-benchmark objective, not a universal network
SLA, a change to verification semantics or a new CI performance gate.

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
  Measure raw client round-trip immediately around the public verify call,
  through decoded result/error, with fixture preparation and post-return
  verdict assertions outside that timer. Add adjusted client p50/p95/p99 by
  removing only the same request's completed mock-provider HTTP exchange
  windows and actual sandbox interpreter execution. Provider exchange means
  the existing send-through-bounded-body-read window; gateway authorization,
  admission, route/credential lookup, capture/accounting and response decoding
  remain counted. Code execution means the existing Monty execution clock,
  paused during host suspension; compilation, worker startup/checkout, IPC,
  mounted-file service, coordination and record/result work remain counted.
  Whole Agent blocks and whole Sandbox::exec calls are not these exclusions.
  Account for concurrency by the wall-clock union of exclusions, including
  provider/code overlap, once per request before computing percentiles.
  Never subtract aggregated percentiles or unpaired cohort samples.
- **REQ-021 — Qualification and reproducibility.** Both Rust L1 client counts
  declare `client-adjusted-p99` strictly `< 0.5 ms`; Python and TypeScript
  establish measured baselines under the same integrity rules, without a new
  numeric latency objective; preserve the existing c64
  server-overhead objective. Use existing full release-run repetitions,
  sample floor, quiet-host checks, histogram/reading precision, append-only
  records and VALID/INVALID versus met/missed/unsettled semantics. Keep raw
  client latency diagnostic, and label adjusted latency as an observed
  wall-time residual, not a zero-cost-work prediction. Record actual fixture,
  timing method and server limits/host/build identity; legacy assert-only
  results are a different workload, never retroactively recomputed. Use the
  existing benchmark server geometry and report capacity errors honestly;
  do not silently increase limits or exclude queue waits to meet the target.
  A trustworthy missed/unsettled result does not establish the 500 µs goal.

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
  Rust's approved adjusted target continues to apply to its call-return sample,
  not a silently substituted result-ready boundary. SDK conversion/shared IO
  phases are nested evidence, never extra completed requests. Use same-request
  duration/exclusion bounds across processes; never serialize monotonic epochs
  or subtract unrelated percentiles.
- **REQ-024 — Full client diagnosis.** Publish client/server latency,
  throughput, errors/cancellations/late calls, CPU and RSS by language and
  process role. Keep the source's sole benchmark backend in every opted-in
  process and ordinary clients free of recording overhead beyond the disabled
  check. Retain shared Rust-core phases and add only bounded observations for
  language work outside that core. Profile native and Python/JS execution and
  allocation in diagnostic runs, with a known-work/known-allocation probe
  proving the claimed coverage. Rust allocator evidence does not establish
  Python/V8 heap coverage. Report overlap and partial coverage explicitly, not
  an invalid sum. Unprofiled runs establish published timing baselines; a
  profile failure cannot satisfy the required diagnostic deliverable even if
  timing remains valid. Reuse installed tools first; additional tooling must
  remain opt-in development/profiling support, not shipped dependencies.
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

| ID | Invariant | Failure prevented |
| --- | --- | --- |
| INV-008 | Language populations execute the same declared workflow under recorded process/concurrency/resource settings; runtime and native profiling coverage is proven and never conflated. | Publishing Rust-only or partial-runtime evidence as Python/TS performance, hiding deferred conversion, fixture CPU or missing native/managed allocations. |
| INV-007 | Every adjusted sample pairs one client call with its own bounded transient execution timing, excludes only documented work and retains honest precision/coverage; ordinary deployments keep existing behavior and no durable timing store/new public verification contract is introduced. | Cross-request subtraction, double-counting parallel work, hiding server costs, unbounded request-label cardinality, or declaring a false latency pass. |

Worker timing may expose execution-duration counters rather than absolute
monotonic instants. Preserve their observed turn envelopes and precision;
use conservative exclusion/residual bounds when overlap placement is unknown,
without serializing opaque cross-process Instant values or inventing interval
positions. A straddling bound is unsettled, and missing/duplicated/unmatched,
overflowed or impossible evidence invalidates the run instead of being clamped
or dropped. Instrumentation overhead remains in the measured path. Requests
finishing after the window retain the existing late-arrival accounting and
cannot contaminate another window's adjusted population.

The existing `bench` build may use the installed Monty telemetry adapter to
observe already-transmitted execution counters. Keep this within existing
pool/telemetry ownership, benchmark-only and without remote export; do not
replace typed checkout/resume/mount handling with a raw protocol driver, alter
program results to smuggle timing, or add a transport, dependency version,
service, durable journal, public timing field or production mode. Bounded
per-request correlation extends the existing benchmark collector, with no
request IDs as permanent metric labels and no source/payload/credential data.

| ID | Acceptance evidence |
| --- | --- |
| AC-019 | Real release-server canonical runs through each public Rust, Python and
TypeScript SDK at 1 and 64 clients exercises all three checks, four governed mock exchanges and two real code operations for each acknowledged verification, with correct cited results/records and deterministic passing/failing arrivals. Counts and interleaved clients reconcile; no bypass or canned result can substitute for executed code. |
| AC-020 | Focused deterministic evidence proves request pairing, disjoint/overlapping gateway/code exclusions, scalar-only worker timing bounds, per-request residual-before-percentile math, exact timer boundaries and invalid evidence handling. Real-server smoke proves instrumentation reaches both Agent paths and survives actual transport/decoding. |
| AC-021 | Full L1 evidence records raw/adjusted client and preserved server metrics per language, at least 1,000 p99 samples per repetition and both strict Rust adjusted objectives; Python/TypeScript baselines use the same validity policy without an added latency threshold. Equality misses and uncertain bounds are unsettled; refusals, incorrect outcomes or lost timing/evidence cannot yield a qualifying run. The target is attained only when both criteria are met under existing benchmark judgment rules. |
| AC-022 | L2's workload/declaration regression stays unchanged, the benchmark report rebuilds from its schema-3 record, legacy records remain readable with their original fixture/timing meanings, and changed README/task descriptions accurately distinguish raw client, adjusted client and old server residual. |

| ID | Acceptance evidence |
| --- | --- |
| AC-023 | Each real SDK run at both counts executes the identical canonical checks/code/provider exchanges, returns the compact decision and explicitly reads original evidence/records through details outside the verify timer. Calls and typed/public consumer migration preserve citations and denial behavior; no language driver delegates timed calls to a Rust-only helper. |
| AC-024 | Deterministic and real native-boundary probes distinguish call return, deferred materialization and shared-core/HTTP attempts, including errors/cancellation. One completed user call is counted once; the result-ready comparison uses the same fields and no extra network request. |
| AC-025 | Unprofiled release evidence publishes separate six language/concurrency populations with common integrity/sample rules and both retained Rust objectives. Diagnostic runs retain attributable client/server/fixture profiles and known-work/allocation coverage proofs for Python/JS and native work. Missing mandatory profile coverage is explicit unfinished proof, never a fabricated success. Saved reports retain each population/repetition without raw artifacts. |
| AC-026 | Focused real SDK/MCP/HTTP composition proof preserves compact decisions, explicit authorized details, complete code-mode records and historical defaults. Missing/not-yet-visible details cannot become empty success or change a completed verdict. Existing code-mode cleanup/capture/security tests remain intact across the import. |

### Integration and language scope reconciliation

The maintainer's agreement selects native three-language callers, Python
threaded synchronous concurrency, measured Python/TS baselines and separate
call-return/result-ready observations. It preserves the Rust numeric objective.
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
other-language projection work. It does not authorize a general runtime
performance redesign. If a valid run misses the objective, retain the evidence
and diagnose the remaining overhead; do not claim the target, loosen its
boundary, or substitute runtime capacity changes without their owning task.

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
Python/TS measured baselines, preserved Rust target, native threaded/async
callers, result-readiness observations and import/result-details reconciliation.
The written requirements above capture those agreed outcomes without granting
implementation, commit, merge or push authorization. Profiling tool/private
runner mechanics remain implementation choices that must prove coverage.


Revision 3 records the maintainer's explicit benchmark requirements and request
to create TASK-003 on 2026-11-08, following the proposed scope. Approval of
the extension fixes REQ-019–021, INV-007 and AC-019–022; it neither changes
revision-2 runtime behavior nor claims a met performance objective.

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
