# TASK-002 r4 system-resilience review

## Subject and scope

Reviewed the immutable cumulative range
`0569b79702218600c4f9790f45cc03100d5c6f1c` to
`2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`, including
`375d97e67f3affe0d5c59727ef3135b22a459140`, against approved Revision 12,
`TASK-002-cleanup`, and the R2/R3 remediation obligations. `HEAD` and the
candidate both resolved to `2ff7bbfa57737ce8a7a69287e2fa462c313b1a8c`
when the review began. No implementation source, build, or runtime test was
changed or run by this reviewer.

Applicable authority was `AGENTS.md`, `architecture/agent-rules.md`, the
spec-driven-development and maintainer-style references, and the Workflow,
client loading, composite registration, idempotency, and recovery contracts in
`architecture/wyrd-design.md`. Later server-run hosting, process-local run
lifecycle, gateway composition, and release qualification remain outside this
TASK-002 acceptance audit.

The standing direction against unsupported bespoke machinery was applied to
the changed resilience boundaries. The candidate uses the existing Cards HTTP
transport, graph hydrator, Skald executor, tenant transaction, registration
idempotency/reconciliation path, and Tokio's standard blocking pool. It adds no
dedicated pool, loader service, cache, queue, health policy, retry subsystem,
restart protocol, or task-specific gate that would require a DRIFT finding.

## Deployed paths and capability boundaries

| Path | Owners and process/dependency topology | Capability and state boundary |
|---|---|---|
| Authored Workflow load | Caller process: `wyrd_client::Workflow::from_path` -> Tokio blocking pool -> synchronous `wyrd_loader::load`/canonicalization -> `WorkflowBodies` -> Skald hydration (`crates/shared/wyrd-client/src/workflow.rs:27-73,99-120`) | A wholly local bundle needs no server or credential. Filesystem results, bodies, and the eventual Workflow remain call-local; loading writes no durable state and starts no provider/tool operation. |
| Authored external references | The same facade constructs `Cards` only when external refs exist, then `CardGraphHydrator::resolve_external` uses the existing Cards transport and exact registry reads (`workflow.rs:65-71`; `cards/hydrate/workflow.rs:159-173`; `cards/hydrate/graph.rs:339-369`) | Registry availability affects only that load. Exact references, UID assertions, provenance separation, Active checks, and normal read authorization remain the boundary. Already-loaded or wholly local Workflows do not depend on this read path. |
| Registered Workflow load | `Cards::workflow().load` validates the Workflow selector, then Runtime-scope graph hydration reads the exact Workflow/Agent/Prompt closure and no artifact inventories (`workflow.rs:148-190`; `graph.rs:38-54,207-249,311-336`) | Graph state belongs to the loading future and returned Workflow; there is no server or client cache to recover. A refusal returns no partial Workflow and performs no execution or durable mutation. |
| Local Workflow execution | The shared facade delegates directly to the existing Skald Workflow (`workflow.rs:76-95`); Python and Node wrap the same owner | Provider failure, retry, timeout, cancellation, and task drain remain owned by the existing Skald executor. TASK-002 adds no detached executor or durable run registry. Process loss loses a local run, as before. |
| Composite registration | Authenticated card route -> `register_card` -> graph planning -> tenant-scoped `EffectiveSpecs` preflight -> `RegistrationWriter::write` in a new tenant transaction (`wyrd-server/src/components/cards/service.rs:565-597,995-1020,1040-1147`) | PostgreSQL/RLS and canonical audit remain authoritative. Registration performs declarative Workflow validation only: no provider/tool dispatch, secret resolution, or Workflow principal is introduced. |
| Registration recovery | Exact preflight `(CardRef, CardUid)` pairs are rechecked Active with `FOR SHARE` inside the write transaction (`wyrd-sql/src/queries/cards/relationships.rs:17-68`); existing operation replay and post-commit upload/reconciliation retain ownership (`service.rs:634-700,702-832`) | Pre-commit failure/drop rolls back Cards, relationships, operation state, and transaction-local audit. A lost response after commit reconciles through the existing idempotency operation; another replica can read shared PostgreSQL state. No replica-local graph state is retained. |

The cumulative graph and runtime changes therefore do not create a new
deployment role or health dependency. A client-side load or run failure does
not make `wyrd-server` unavailable. A registration dependency failure refuses
that transaction rather than exiting the shared process or disabling sibling
Cards, gateway, Bifrost, or Vala capabilities.

## Failure, interruption, and recovery assessment

| Credible failure | Fail-closed boundary and surviving state | Recovery and proof assessment |
|---|---|---|
| Missing/unreadable authored file, invalid bundle, or invalid resolved graph | The individual load returns a structured error before a Workflow is published or any step dispatches. `load_bundle` and all loaded values are private to the task (`workflow.rs:57-73,109-120`). | Correct the bundle and retry. Existing loader and shared-client focused tests recorded in R3 exercise the checked-in bundle and pre-dispatch failures. |
| Loading future is dropped during filesystem IO | The `spawn_blocking` job may finish after abandonment, as Tokio documents and the public rustdoc now states, but its result has no receiver and can neither publish a partial Workflow nor write durable state (`workflow.rs:39-44,57-64`). | A later load starts fresh. The standard Tokio blocking pool is the existing/native mechanism; no cancel token, custom pool, or filesystem worker is warranted. Runtime shutdown/panic is mapped to the Workflow error channel rather than panicking the caller. |
| Loading future is dropped during registry traversal | The active async read future and call-local traversal are dropped. Earlier authorized reads may have their normal server audit effects, but no partial graph escapes (`graph.rs:138-149,207-249,311-369`). | Retry the complete exact load after dependency recovery. There is no resume state or cache to repair. |
| Registry outage, timeout, or transient HTTP refusal | Only the affected external/registered load fails after the existing transport's per-attempt timeout and finite replay-safe retry policy. Graph traversal adds no independent retry loop and reads nodes sequentially. | A caller retries after service recovery. Already-hydrated Workflows and fully local authored loading remain available. No candidate-specific outage test was run; source ownership bounds the claim. |
| Dependency is missing, inactive, denied, cross-tenant, or UID-mismatched | Exact Card reads or `WorkflowBodies::extend_registered` refuse before hydration is returned and before provider/tool dispatch. Sibling bodies cannot satisfy external refs (`cards/hydrate/workflow.rs:98-124,159-200`). | Restore access/dependency state and retry. The recorded Rust/Python/TypeScript journeys exercise the public negative paths and unchanged zero-dispatch boundary. |
| Database/audit failure or cancellation before registration commit | The request fails. `RegistrationWriter` holds one caller-scoped `TenantConn` through audit append, exact dependency recheck, operation reservation, node/relationship writes, stored replay seed, and commit (`service.rs:1073-1147`). Dropping it rolls back uncommitted state and releases locks. | Retry with the stable idempotency key. A failed authorization-audit append cannot become an unaudited successful write. No process abort or global unhealthy transition is introduced. |
| External dependency changes after preflight | Write-time recheck requires the exact UID validated by preflight to remain the Active row, under a lock held through registration (`relationships.rs:17-68`; `service.rs:1087-1091`). A replacement at the same identity is refused rather than silently rebound. | A fresh request reruns preflight against the replacement. Recorded coordinated server/SQL tests cover stale replacement refusal, no partial operation/Card/relationship writes, and lifecycle locking. |
| Process/replica exits during registration | Before commit, PostgreSQL rollback removes uncommitted state and locks. After commit, Cards, relationships, audit staging, and the replay seed are shared durable state; the existing post-commit reconciler owns incomplete blob/upload work. | Retry/replay can land on another replica. The candidate adds no affinity or process-local recovery state to registration. No new crash/rolling-replacement test was run, so this is a preservation assessment of existing owners rather than release qualification. |
| Provider failure, timeout, explicit cancellation, or caller process loss after a local run starts | Existing Skald execution owns terminal results, cancellation and drain. TASK-002 loading does not spawn or retain a second execution task. A local process loss naturally loses the in-memory run and does not imply server restart recovery. | Re-run from the same immutable loaded graph when appropriate. Server-hosted accepted-job recovery and cancellation belong to later tasks and are not inferred from these local journeys. |

## R3 remediation and cumulative regression assessment

- FIND-TASK-002-13 is closed at the system boundary: both loader IO and entry
  canonicalization now run in one `tokio::task::spawn_blocking` operation, while
  pure body preparation and async registry reads remain on their appropriate
  sides of the boundary. The correction introduces no new runtime, dependency,
  or background lifecycle.
- The human-removed FIND-TASK-002-11 implementation in `375d97e67` changes
  malformed Workflow selector diagnostics only. Those selectors still fail
  before registry IO; no recovery, durable-state, retry, or shared-availability
  behavior changes.
- FIND-TASK-002-12's generic Python selector correction likewise remains a
  local pre-IO refusal. The exact-relationship proof changes in all three SDK
  journeys do not change runtime topology.
- The exact-preflight-UID fence, transaction/audit ordering, and existing
  post-commit recovery path remain intact across the full base-to-candidate
  diff. No later remediation weakened cancellation or converted an operation
  error into a process failure.

## Findings

No material system-resilience findings. The independently reviewed failure
boundaries are operation-scoped, partial local state is unpublished, durable
registration remains atomic and replayable, and no unsupported bespoke
resilience mechanism entered the cumulative diff.

## Verification limits

R3 implementation evidence records the focused loader/client tests, all three
real SDK journeys, three PostgreSQL Workflow registration tests, the SQL
lifecycle-lock race test, relevant formatting/lint/type checks, and the repaired
workspace-hack and cumulative whitespace checks as passing. I inspected the
changed owners and material test assertions but did not independently execute
those commands. The evidence does not constitute crash, rolling-replacement,
registry-outage, or host-load qualification; TASK-002 adds no new durable
recovery protocol that would make those broader exercises a completion
requirement. Healthy-path SDK execution is not treated as proof of later
server-hosted Workflow lifecycle behavior.

## Overall result

**PASS.** The candidate keeps filesystem abandonment, registry outage,
dependency change, cancellation, database failure, transaction rollback, and
replica restart effects within their established owners and approved
operation-level boundaries. No material resilience remediation is required.
