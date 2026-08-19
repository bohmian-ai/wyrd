---
name: wyrd-implement-plan-v3
description: Execute a Wyrd plan produced by wyrd-plan-v3 through a wall-time-optimized rolling DAG, adaptive concurrency, resource-aware focused verification, risk-routed candidate review, autonomous remediation, serial integration, resumability, and cleanup. Use when Codex must implement, continue, finish, or test a complete approved plan without cold rehearsals or exhaustive preflight.
---

# Wyrd Implement Plan v3

Execute task packets; do not become another planner or implement delegated
source changes. Own rolling-DAG scheduling, verification capacity, autonomous
reconciliation/remediation, integration, resumability, and cleanup. Route roles
through `.agents/model-routing.md`. Minimize expected wall-clock time to
accepted integrated code; concurrency and worker utilization are subordinate
scheduling tools. Never invoke v1/v2.

## Establish execution

Accept a stable caller-supplied `request_id`, repository root, plan directory,
and integration ref. The caller must reuse `request_id` to resume the same
execution; reject an ID already bound to different plan or intent bytes.
Atomically acquire `lease.yaml` in the request directory before reading or
mutating execution state. Record a unique controller owner ID, host/process
identity, heartbeat, and lease deadline; renew it while active. Only one owner
may hold an unexpired lease. A replacement may take over only after verifying
the recorded owner is absent and the deadline expired, then records the
takeover before recovery.

Read `intent.md`, `plan.md`, every packet, `plan-review.yaml`, `AGENTS.md`,
`architecture/agent-rules.md`, `.agents/model-routing.md`, and task authorities.
Recompute intent, plan, and packet digests and require an `APPROVE` review bound
to those bytes and the plan's `source_revision`.

Resolve the integration SHA. Execute when it equals `source_revision`. For a
descendant, inspect only changes to forecast paths, named owners/contracts, and
verification seams. When that bounded check finds a changed assumption,
autonomously invoke bounded plan repair to update `source_revision` and only
affected contracts, recompute digests, rerun plan review, and invalidate only
affected tasks/dependents. Unrelated commits do not invalidate the plan.

Create durable compact controller state at
`<git-common-dir>/wyrd-v3/<request_id>/state.yaml`. Create an expendable
work/cache root with `mktemp -d` and record its exact path plus every owned
worktree, process group, verification command, start time, and timeout in the
sibling `locator.yaml`. Atomically persist in durable state:

- intent, plan, review, and task digests;
- controller state and generation for each task;
- parent/candidate/diff tuple and superseded candidates;
- proof artifact and classification, review disposition/verdict, and retry count;
- task-stage timestamps, queue durations, candidate invalidation causes, and
  concurrency-throttling decisions with their observed bottleneck;
- integrated tasks, remaining dependencies, ready/running queues; and
- retained blocked or infrastructure-pause evidence.

Anchor every live candidate at
`refs/wyrd-v3/<request_id>/<task_id>/<generation>` before removing its
worktree. Verify the ref equals the persisted candidate SHA on resume. Delete
that exact ref only after integration, supersession, or acknowledged terminal
cleanup; a stored SHA or reconstructed diff is not an identity-preserving
substitute.

Use task states `READY`, `RUNNING`, `CANDIDATE`, `PROOFED`, `REVIEWED`,
`INTEGRATED`, `SUPERSEDED`, `CONTRACT_REPAIR`, `MATERIAL_BLOCKED`, and
`INFRA_PAUSED`. Persist each transition before dispatching dependent work.

On resume, resolve the locator by `request_id` before creating resources. Load
and verify intent/plan/task digests, source/integration SHAs, candidate tuples,
proof artifacts, and generations. Preserve valid `CANDIDATE`, `PROOFED`, and
`REVIEWED` work. First terminate recorded expired or orphaned owned process
groups, capture their bounded evidence, remove their owned worktrees/caches,
and prune; then reclassify orphaned `RUNNING` work as `READY`, recompute
dependencies and queues from integrated tasks, and continue. Recreate an
expendable root when it is missing while retaining durable state.

At controller startup, sweep v3 request directories. Reclaim a run only when
its durable state says complete, its owner process is absent and its recorded
deadline has expired, or the caller explicitly marks that exact `request_id`
abandoned. Capture resumable evidence before deleting owned resources; never
infer abandonment from age alone. Every spawned command has a recorded timeout
and owned process group so a hung proof cannot survive recovery indefinitely.
If the state is `COMPLETE`, return the recorded output rather than execute
again. Retain its compact tombstone and output digest until caller
acknowledgement or a declared bounded retention deadline; only then remove its
candidate refs and request directory.

## Minimize time to accepted integration

Continuously enqueue every task whose actual direct dependencies are
integrated. Available worker capacity is a ceiling, not a utilization target.
Dispatch a `READY` task when its expected critical-path savings exceed context
acquisition, dispatch, resource contention, proof, review, reconciliation,
candidate invalidation, and integration costs. Full plan decomposition does not
require every ready packet to run immediately.

Before runtime evidence exists, default to dispatching substantial `READY` work
whose dependencies are satisfied and whose forecast paths do not overlap while
implementation, proof, and integration capacity are healthy. Throttle only for
a concrete signal: forecast or actual path collision, an imminent integration
likely to invalidate the candidate base, a growing proof or integration queue,
observed CPU, memory, disk, or cache pressure, or recent reconciliation and
invalidation cost. Use recorded task-stage and queue timings to adapt subsequent
dispatches; do not invent pseudo-precise duration estimates.

Prioritize integrating approved candidates that release successors, work on
the current critical path, tasks that release multiple dependents, and proof,
review, or remediation that unblocks integration. Then dispatch substantial
independent work whose setup and integration costs are amortized. Do not wait
for an unrelated task merely to assemble a batch or fixed wave, but leave slots
idle when another dispatch would likely delay acceptance because the remaining
work is too small, an imminent integration would invalidate its base, shared
seams make reconciliation likely, proof or integration is the bottleneck, or
host and cache contention would slow active work. Reassess after every material
state or capacity change.

Forecast misses are normal. Workers may expand into necessary task-local paths.
Schedule optimistically from forecasts, then reconcile actual changed paths.
Overlapping prohibited paths do not conflict because neither task may write
them. Never ask the user to approve incidental scope, ordinary conflicts,
consumer refreshes, generated artifacts, or mechanical repairs.

## Implement, prove, risk-route review, and integrate

Give each worker its packet, integration parent, clean worktree, prohibited
scope, implementation skill, and routed tier. Workers run only light
`verification.worker` commands at cohesive boundaries. Route an unexpectedly
heavy worker command through the controller verification lanes.

Treat implementation/review slots, heavy verification capacity, and serial
integration as distinct but interacting resources. Dispatch unrelated ready
implementation or review work during verification when doing so improves
expected acceptance time; throttle it when verification backlog, host
contention, reconciliation pressure, or serial integration is the actual
bottleneck. Default to one heavy Cargo or database command at a time, increasing
only when host capacity is demonstrated. Permit independent targeted Python,
Node, and static checks concurrently when their aggregate footprint fits the
host. A worker awaiting a heavy lane does not reserve an active model slot when
the harness can release it. For candidate proof, store a bounded artifact containing
candidate SHA, exact command, worktree, timestamps, exit status, and stdout/
stderr; digest it and classify `PASS`, `CODE_FAILURE`, or `INFRA_UNAVAILABLE`.

Process candidates continuously:

1. Run focused candidate proof for every candidate. Independently review only
   candidates whose actual diff touches a public/generated contract, shared
   composition or reconciliation seam, durable behavior, security, tenancy,
   audit, migration, or materially expands into another owner; also route a
   review when proof or reconciliation evidence is suspicious or ambiguous.
   Dispatch `$wyrd-review-v3` immediately for those candidates so review can
   overlap proof. For an isolated mechanical or owner-local leaf with coherent
   scope and `PASS` proof, record `review_disposition: NOT_REQUIRED` plus the
   reason and integrate without spending a reviewer slot. Review is not a
   default per-task gate.

   Never run a mutable-tree, pre-commit, or "pre-seal" defect review. For each
   routed candidate generation, run exactly one substantive independent review
   of the immutable candidate commit. Candidate existence, parent, packet,
   report, and diff-digest checks establish that review's identity; they are
   not a second review. Reuse that review when the candidate tuple and packet
   bytes are unchanged, and require a fresh review only for a replacement
   candidate or changed review identity.
2. On `CODE_FAILURE` or `RESUME_IMPLEMENTATION`, dispatch a remediation worker
   from the original task parent with rejected SHA and exact findings. Require
   one complete replacement commit and supersede the old candidate.
3. On `TASK_CONTRACT_REPAIR_REQUIRED`, autonomously invoke the bounded packet-
   revision mode of `$wyrd-plan-v3`, rerun `$wyrd-plan-review-v3`, invalidate
   only that task and dependents, and resume. Escalate only if repair exposes a
   new material decision.
4. On `INFRA_UNAVAILABLE`, perform one repository-managed recovery and retry
   the same candidate. If infrastructure remains unavailable, persist
   `INFRA_PAUSED`, reclaim unrelated resources, and return resumable state
   without requesting product authority.
5. After two consecutive failed implementation generations without a new
   diagnosis, dispatch deep diagnostic analysis. Return its concrete resolution
   to a normal worker. Continue unless diagnosis identifies a material choice.
6. As soon as a candidate has `PASS` proof and either `APPROVE` or a recorded
   `NOT_REQUIRED` review disposition, reconcile its actual paths and contract
   assumptions against already integrated and currently eligible candidates.
   When predecessors and conflicts permit, serially merge the immutable
   candidate into the integration root without rewriting its reviewed commit,
   and record the resulting integration commit separately. Use stable task ID
   only to break ties among simultaneously integrable candidates. Never wait
   for an unfinished unrelated task. If the merge is not clean or would require
   replay, rebase, or cherry-pick, discard that generation and build one fresh
   replacement candidate on the current integration SHA, then prove it and
   repeat risk routing.
7. Immediately enqueue successors released by that integration. If a prior
   integration invalidates a candidate assumption, refresh only that candidate
   from the new integration SHA and repeat proof plus risk routing.
8. After integration, rejection, or supersession, remove the clean worktree and
   contained task-local caches, then run `git worktree prune`.

After every integration, completion, failure, contract repair, or capacity
change, recompute readiness, the critical path, and the current bottleneck
before dispatching more work. A candidate conflict blocks only the conflicting
candidates; proof, review, and implementation for unrelated tasks continue when
their expected wall-time benefit exceeds their added coordination cost.

Consume every worker and reviewer terminal outcome:

- `INVALID_REQUEST` repairs the dispatch identity or enters bounded task-
  contract repair.
- `FORBIDDEN_WRITE` enters contract repair when the prohibition is mechanically
  wrong; otherwise persist `MATERIAL_BLOCKED` with the exact required decision.
- `MANDATORY_PROOF_UNAVAILABLE` enters infrastructure recovery or task-contract
  repair according to the evidence.
- `AUTHORITY_REQUIRED`, `MATERIAL_CONFLICT`, and reviewer
  `ORCHESTRATOR_DECISION_REQUIRED` persist `MATERIAL_BLOCKED` with the exact
  decision that cannot be inferred.
- `REVIEW_BLOCKED` rebuilds the review tuple or artifact and retries once; a
  repeated infrastructure failure becomes `INFRA_PAUSED`.
- `TASK_CONTRACT_REPAIR_REQUIRED` follows the bounded revision and reapproval
  flow above.

When actual changes expose a required repository check not predicted by the
plan, add the narrowest repository-defined check justified by the diff,
diagnostic, or reconciliation result. Record it as forecast-derived proof. Do
not add speculative broad checks or run `pre-pr` outside `AGENTS.md` criteria.

## Close out and reclaim resources

Run plan-declared integrated checks and forecast-derived checks. Run
`$wyrd-review-and-plan-v3` only for public
contracts, security, tenancy, migrations, durable behavior, genuinely
cross-owner behavior, or explicit requirements. Ordinary multi-task work does
not earn specialist terminal review.

Split reversible terminal findings by disjoint owner. Bind each group to an
existing primary packet plus the terminal review artifact and dispatch
terminal-remediation `$wyrd-implement-v3` workers from the current integration
SHA. Run focused proof for every resulting candidate and apply the same
risk-routed candidate-review rule; do not duplicate terminal review with an
automatic review of every remediation leaf. Cleanly merge each eligible
immutable commit while preserving its candidate SHA, sequencing only actual
overlaps. Rerun the terminal review after integration. Invoke planning only for
a new material decision.

For terminal `TASK_CONTRACT_REPAIR` findings, invoke the bounded
`task_contract` repair bundle, retain every integrated task's historical packet
digest, and dispatch any required additive correction from the current
integration SHA. Rerun affected proof, any risk-routed candidate review, and
terminal review; do not invalidate or relabel already integrated commits.

Before returning, stop/wait for workers, remove clean worktrees, and prune. On
success, atomically persist `COMPLETE`, the returned result and its digest,
release the lease, and delete the exact expendable root and locator while
retaining the compact completion tombstone as specified above. On `MATERIAL_BLOCKED`,
`INFRA_PAUSED`, or interruption, capture each affected task's binary diff,
status, necessary untracked evidence, report, and binding; remove stale
worktrees and preserve compact durable state plus locator. Retain a dirty worktree only
when compact evidence cannot represent material state. Never run `cargo clean`,
delete shared caches, remove user/unrecorded worktrees, or delete user artifacts.

Report integrated commits, elapsed implementation time, proof and review queue
time, reconciliation or rework time, integration wait time, the observed
critical path and bottleneck, candidate invalidations, intentional concurrency
throttling and its wall-time rationale, proof/review, remediation, material
decisions or resumable pause, and cleanup.
