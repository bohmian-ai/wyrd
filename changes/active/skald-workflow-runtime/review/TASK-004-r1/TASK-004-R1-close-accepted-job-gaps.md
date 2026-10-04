---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-skald-workflow-runtime
spec_revision: 12
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4, FIND-TASK-004-5, FIND-TASK-004-6, FIND-TASK-004-7, FIND-TASK-004-8, FIND-TASK-004-9, FIND-TASK-004-10, FIND-TASK-004-11, FIND-TASK-004-12, FIND-TASK-004-13, FIND-TASK-004-14]
---

# Close accepted-job ownership, contract, and journey gaps

Implementation skill: `$wyrd-implement`.

## Immutable review inputs

- Approved spec: `changes/active/skald-workflow-runtime/spec.md`, revision 12
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-004-accepted-server-jobs.md`
- Reviewed base: `b90335e0991438e8c28b65ed9c0c4cd80f9b0d56`
- Reviewed candidate: `96e993a16706d2fb759e4cdb7371ff490b198a35`
- Validated diagnosis: `changes/active/skald-workflow-runtime/review/TASK-004-r1/findings-validation.md`

## Outcome

Keep the approved process-local Workflow host and existing Cards, Skald, gateway, Oracle, audit, and Bifrost owners. Close two shutdown-ownership races, preserve the original query deadline during cancel-while-open, publish exact built-in tool declarations, align governing authority, and complete the already-required production-shaped journeys. Do not add a durable queue, actor, scheduler, compatibility surface, lifecycle service, timeout option, tenant simulator, second query engine, or standalone repository check.

## Diagnoses and required corrections

### Preparation ownership and shutdown (`FIND-TASK-004-1`, `FIND-TASK-004-2`)

`WorkflowRuns::admit` publishes a preparing key and consumes capacity before the preparation is registered with `WorkflowRuns.tasks`. Shutdown can therefore close and drain an empty tracker before a late task starts. Inside that task, cancellation can drop the `spawn_blocking` join handle while the blocking closure continues detached from Workflow ownership.

Correct the existing `WorkflowRuns` owner so task registration and reservation visibility are coordinated: observable preparation state must already have a tracked owner, and shutdown-winning admission must publish nothing and return the existing unavailable result. Run CPU-bound graph preparation through the existing tracker-supported blocking ownership so cancellation prevents acceptance but clean drain still waits for the closure. Reuse the current lock, tracker, cancellation token, reservation, and preparation outcome. Do not create another executor or queue.

### Query deadline and owner-loss recovery (`FIND-TASK-004-3`, `FIND-TASK-004-4`)

`RunningQueryControls::open_cancellable` lacks the absolute query deadline. When cancellation wins, it can await remote cancellation's fresh per-peer transport bounds and then the same open future beyond the original deadline. Pass the already-existing absolute deadline into this owner and bound the complete cancel-during-open phase with it. Preserve one cancellation attempt, the same open future, authenticated remote routing, and existing honest incomplete/unavailable/timeout results. Add no timeout setting or retry.

The pod-loss branch of the forwarded Workflow journey skips surviving-owner baseline recovery and a later query, and accepts any `WYRD_VALA_*` error. Extend that existing branch to assert its exact current owner-loss settlement class, no rows/model result, recovery of surviving owner counts, and a later successful query through the surviving topology. Reuse `PeerCluster`; add no new probe or cluster harness.

### Tenant and accepted-authority boundaries (`FIND-TASK-004-5`, `FIND-TASK-004-8`, `FIND-TASK-004-14`)

TASK-004 journeys use one authenticated tenant. Random foreign UUID configuration and same-tenant principals do not exercise credential-derived tenant selection. Use the repository's existing second-tenant provisioning/authentication path in Scenarios 1, 4, and 6 to prove cross-tenant root refusal before work, common unknown/foreign get and cancel responses, and no foreign Card/query data.

Extend the current admission and authority journeys to refuse an inactive root; preserve a pinned run across dependency mutation/deactivation; prove newly granted resources and replay cannot widen accepted authority; and prove a later gateway call still obeys current deployment/credential eligibility. Reuse current registry, token, grant, gateway, and deterministic upstream controls. Do not add an authorization cache or refresh mechanism.

Update only the existing relevant sections of `architecture/wyrd-design.md` and `architecture/wyrd-security-posture.md`. State that an accepted run retains a token-free snapshot of principal attribution and scopes bounded to its graph and deadline; token expiry or later grant change neither cancels nor widens it; later HTTP get/cancel/replay authenticate and authorize afresh; and Cards, Bifrost, and gateway owners still perform their own live per-call decisions/audit where specified.

### Built-in tool declarations and negative paths (`FIND-TASK-004-6`, `FIND-TASK-004-7`, `FIND-TASK-004-10`)

Add substantive item rustdoc to the eight existing `QueryTool` and `CardsTool` declaration methods. Do not add a documentation file, wrapper, lint allowance, or check.

Replace unconstrained object output schemas with exact projections of the existing result owners: `{columns, rows, terminal}` for the bounded query result and the canonical Card envelope for `cards.get`. Reuse existing schema facilities and actual serialization; do not invent a parallel durable contract or generator.

Extend Scenario 4 through the existing real Agent loop to cover oversized SQL and all numeric limits, independently missing tool permissions, inaccessible Card/table objects, and malformed terminal/partial-row failure with no data disclosure and redacted non-retryable errors. Reuse Cards/query authorization, collector, audit, and fault seams.

### Idempotency and lifecycle journeys (`FIND-TASK-004-9`, `FIND-TASK-004-12`)

Extend Scenario 2 with tenant-key isolation, one cancelled matching waiter while peers and preparation survive, shutdown waking all matching waiters exactly once, and lost acceptance-response recovery with exactly one accepted observation and one provider execution. Reuse the preparation gate, current request cancellation/shutdown path, audit/observation evidence, and upstream counters.

Extend Scenario 6 with completion-versus-deadline, GET during whole-snapshot replacement, global oldest-first eviction across tenants/principals, shutdown of an accepted queued run, and inspection of complete terminal snapshots after shutdown. Owner-level tests for the two preparation-tracker fixes must directly exercise the narrow race; the real server journey remains the end-to-end proof.

### Gateway projection journey (`FIND-TASK-004-11`)

The new in-process adapter has separate OpenAI Chat, OpenAI Responses, Anthropic, Gemini, and Vertex projections/decoders, but the journey covers only OpenAI. Extend Scenario 5 with the existing deterministic gateway/provider fixtures to directly cover every required branch, internal Vertex success, pre-upstream incompatible capability refusal, stored fallback order and concurrent fallback isolation, remaining deadline, and separate cancellation. Continue to call `GatewayInvocation`; add no transport, registry, option, or compatibility layer.

### Graph and snapshot limits (`FIND-TASK-004-13`)

Extend Scenario 7 with a deep but admissible graph, a sibling Bifrost query during held preparation, aggregate `max_run_bytes` overflow distinct from step-result overflow, and near-ceiling failure/cancellation/deadline terminalization. Assert stable errors, no retained oversized data, complete terminal snapshots, capacity release, and serviceability of Cards, gateway, and Bifrost. Reuse existing builders/fixtures; do not add a CPU pool, scheduler, transport surrogate, setting, or synthetic host load.

## Preserved behavior and non-goals

- Preserve process-local runs, affinity/restart loss, fixed retention, typed errors, canonical audit, exact Card pinning, captured authority, and current gateway/tool owner boundaries.
- Preserve the native Oracle attempt/driver/exchange/participant lifecycle and governed shared memory accounting.
- Do not restore the deleted Oracle reserved-byte polling, release refusal, poison state, or mechanism-specific tests. Validation found those to be unsupported bespoke DRIFT rather than required structured ownership.
- No durable run persistence, recovery, lease, cross-replica lookup, Workflow principal, bearer retention, runtime policy gate, arbitrary server tool registration, new MCP/language surface, second audit writer, second query engine, or new third-party dependency.
- Do not weaken, ignore, serialize globally, or delete existing tests/checks to obtain green results.

## Acceptance criteria mapped to findings

| Finding | Closure criterion |
|---|---|
| `FIND-TASK-004-1` | No observable reservation exists without tracker ownership; shutdown cannot drain before late preparation ownership. |
| `FIND-TASK-004-2` | Blocking preparation remains counted until completion even after outer cancellation. |
| `FIND-TASK-004-3` | Cancel-during-open completes at the original absolute deadline without a fresh cleanup budget. |
| `FIND-TASK-004-4` | Pod-loss case proves exact settlement, surviving-owner recovery, no rows, and later query availability. |
| `FIND-TASK-004-5` | Authenticated second tenant cannot access first-tenant Workflow, run, Card, or Bifrost data. |
| `FIND-TASK-004-6` | All eight declaration methods have substantive rustdoc. |
| `FIND-TASK-004-7` | Both output schemas match concrete serialized results and are closed where the contract is closed. |
| `FIND-TASK-004-8` | Inactive/pinned/no-widening/live-gateway cases pass through existing journeys. |
| `FIND-TASK-004-9` | Scenario 2 covers every AC-021 tenant/waiter/shutdown/lost-response case with exact counts. |
| `FIND-TASK-004-10` | Scenario 4 covers remaining bounds, independent permissions, inaccessible objects, and terminal integrity. |
| `FIND-TASK-004-11` | Scenario 5 directly proves every in-process dialect/Vertex/fallback/deadline/cancel branch. |
| `FIND-TASK-004-12` | Scenario 6 covers required races, global eviction, queued shutdown, and complete terminals. |
| `FIND-TASK-004-13` | Scenario 7 covers deep graph, all sibling services, aggregate overflow, and terminal reserve. |
| `FIND-TASK-004-14` | Existing design and security authorities agree with REQ-032A and the implementation. |

## Focused and broader proof

Use the exact existing selectors from TASK-004 for Scenarios 1–7 and the existing forwarded Oracle Workflow selector. Add exact owner-level selectors for the preparation tracking race, tracked blocking work, and absolute-deadline open cancellation. Run all commands through the repository's prescribed `mise exec --`/`mise run` forms and repository-managed Postgres wrappers.

After focused Red-Green iteration, run the original task's broader affected lanes: `test:wyrd`, `test:shared`, principals unit/integration, gateway journey, Bifrost, codegen check, client-tier, tenant isolation, unwrap audit, format, lints, and `git diff --check`. Do not add a new repository check; the compiler, source review, existing focused tests, and existing lanes are the closure mechanisms.
