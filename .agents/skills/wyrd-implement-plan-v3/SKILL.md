---
name: wyrd-implement-plan-v3
description: Execute an approved Wyrd v3 plan through complete implementer verification, independent candidate acceptance, autonomous remediation, serial integration, compact resumable state, and terminal integrated review.
---

# Wyrd Implement Plan v3

Coordinate approved task packets; do not implement delegated source changes,
rerun their full verification, independently reinterpret reviewer findings, or
become another planner. Own DAG scheduling, isolated worktrees, leases,
generations, immutable identity, reviewer dispatch, remediation routing, serial
integration, recovery, and cleanup. Never invoke v1/v2.

## Establish execution

Accept stable `request_id`, repository root, plan directory, and integration
ref. Bind the ID to exact intent, plan, plan-review, packet digests, and source
revision. Acquire an atomic lease recording controller owner/process,
heartbeat, and deadline. A replacement takes over only after proving the owner
absent and lease expired.

Read the approved plan authorities and repository rules. Require a matching
`APPROVE` plan review. If the integration ref descends from source revision,
inspect only changed forecast paths/contracts/proof seams. Repair and reapprove
only affected task contracts when an assumption changed; unrelated commits do
not invalidate the plan.

## Compact durable artifacts

Use `<git-common-dir>/wyrd-v3/<request_id>/` for durable content-addressed
artifacts and a recorded `mktemp -d` root for expendable worktrees/caches.
Persist atomic `state.yaml` containing only current scheduling/recovery facts:

- request and intent/plan/review/packet digests;
- source/integration SHA and integrated task IDs;
- per task: current state, generation, dependency status, parent/candidate/diff
  tuple, proof/review refs, retry count, and superseded-candidate lineage;
- current lease, material blocker or infrastructure pause refs; and
- compact artifact/event refs needed for recovery.

Do not store full commands, logs, packet/AC prose, implementation reports,
review bodies, repeated queues, or nested evidence in state. Derive ready and
running sets from task/dependency state. Record history as append-only compact
events (`event_id`, task/generation, transition, timestamp, artifact refs), not
copied state snapshots.

`locator.yaml` is expendable and contains only active process ownership,
deadline/timeout, worktree/cache paths, and artifact refs. It never contains
commands or logs. Full command output exists exactly once as a content-addressed
blob. Proof artifacts contain command ID, cwd, timeout, timestamps, exit status,
result, output ref/digest, and only a short causal failure excerpt. Command text
is derived from its immutable packet/remediation source.

Use a small canonical candidate manifest, proof artifact, and review artifact.
Implementation traces use `{ac_id, evidence_refs}`; reviews record each AC once.
Parity rows reference per-row evidence records rather than copying nested
evidence. Terminal review references task candidate/proof/review identities.

Preserve safety-critical identity: request/task/generation; applicable packet,
plan, and intent digests; original parent; candidate/target SHA; binary diff
digest and sole-parent assertion; prohibited-write result; supersession;
review verdict/findings; leases/process ownership; complete compact command
metadata; and one-to-one AC/row traceability.

Anchor each live candidate at
`refs/wyrd-v3/<request_id>/<task_id>/<generation>`. Verify the ref on resume;
delete it only after integration, supersession, or acknowledged cleanup.

Task states are `READY`, `RUNNING`, `CANDIDATE`, `IN_REVIEW`, `APPROVED`,
`INTEGRATED`, `SUPERSEDED`, `CONTRACT_REPAIR`, `MATERIAL_BLOCKED`, and
`INFRA_PAUSED`. Persist each transition before dependent dispatch.

On resume, verify all bindings and candidate refs, preserve valid candidates
and approvals, terminate only expired/orphaned owned processes, clean only
owned expendable resources, reclassify orphaned `RUNNING` tasks, derive
readiness, and continue. Age alone never proves abandonment. A completed run
returns its compact result/tombstone without reexecution.

## Schedule for accepted integration

Continuously derive tasks whose direct dependencies are integrated. Capacity
is a ceiling, not a target. Dispatch ready work only when expected critical-path
savings exceed context, contention, review, reconciliation, invalidation, and
integration cost. Throttle for concrete path collision, imminent invalidation,
review/integration backlog, or observed host/cache pressure. Do not use fixed
waves or wait for unrelated batching.

Forecast paths are coordination hints; prohibited paths are hard boundaries.
Reconcile actual changes after implementation. Ordinary consumer closure,
generated artifacts, conflicts, and mechanical repairs need no user approval.

## Candidate lifecycle

1. Give `$wyrd-implement-v3` the packet, original parent, clean worktree,
   generation, prohibited scope, and any unchanged reviewer remediation
   contract. The implementer gathers context, edits code/tests, runs every
   required command, fixes failures, and returns an immutable candidate only
   with compact `PASS` proof.
2. The controller validates packet binding, candidate existence, sole parent,
   diff digest, clean worktree, prohibited writes, proof artifact identity, and
   ref anchoring. It does not rerun the full task suite or treat coordination as
   acceptance.
3. Dispatch an independent `$wyrd-review-v3` for every candidate generation.
   The reviewer examines actual code/diff and tests against every AC, validates
   proof, and may run targeted checks only when evidence is missing or
   suspicious. Review—not successful command execution—is acceptance
   authority.
4. On approval, reconcile actual paths and serially merge the immutable commit
   without rewriting it. Record the separate integration SHA. Never wait for
   unrelated unfinished work.
5. On reversible findings, route the reviewer's bounded remediation contract
   unchanged to an implementer. The controller does not redesign or replan
   ordinary findings. The replacement starts from the original/current bound
   parent, fully implements the task plus assigned findings, reruns all task and
   finding checks, supersedes the rejected generation, and receives fresh
   independent review.
6. A mechanically defective packet enters bounded task-contract repair and
   reapproval. Only a genuinely undecided material product, public/durable
   contract, owner, dependency, security, tenancy, audit, migration, or
   acceptance choice enters replanning/user authority.
7. Infrastructure failures receive one repository-managed recovery and retry.
   Persistent infrastructure failure becomes resumable `INFRA_PAUSED`, not a
   product blocker. Repeated code failure without new diagnosis triggers deep
   diagnosis, then returns concrete findings to the implementer.
8. After integration, supersession, or rejection, remove owned clean worktrees
   and task-local caches and prune. Enqueue newly released successors
   immediately. An invalidated candidate is rebuilt from the current integration
   SHA, fully reverified, and freshly reviewed.

Implementation, review, resource-heavy commands, and serial integration are
distinct resources. Default to one heavy Cargo/database command at a time;
allow independent lighter work when measured capacity permits. A waiting worker
need not reserve a model slot.

## Terminal acceptance

After all tasks integrate, run plan-declared integrated checks and
`$wyrd-review-and-plan-v3`. It reviews all integrated code against intent, plan,
and packets, validates task proof/review artifacts by reference, and checks
cross-task seams. It never embeds or blindly reruns full task proof; targeted
reruns are allowed only for missing, contradictory, or suspicious evidence.

Route each reversible terminal remediation contract unchanged to
`$wyrd-implement-v3`, grouped only by actual owner/write overlap. Each worker
fully verifies its candidate; every candidate receives `$wyrd-review-v3`;
integrate approved candidates serially, then rerun terminal review. Task-
contract defects use bounded repair. Invoke planning only for material
decisions.

## Close and report

On success, persist `COMPLETE`, integration result/digest, compact task
candidate/proof/review refs, terminal-review ref, and cleanup status; release
the lease and remove exact owned expendable resources/locator. Retain a compact
tombstone until acknowledged retention expiry. On block, pause, or interruption,
persist only the compact bindings/artifact refs needed to resume and retain a
dirty worktree only when those artifacts cannot represent material state.

Never run `cargo clean`, delete shared caches, remove unowned worktrees, alter
Git identity, or delete user artifacts. Report integrated commits, approvals,
remediation/material decisions or resumable pause, current bottleneck, and
cleanup—by artifact reference, without reproducing commands, logs, state, or
acceptance prose.
