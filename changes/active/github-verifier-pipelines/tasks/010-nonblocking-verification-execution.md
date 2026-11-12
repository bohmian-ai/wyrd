---
id: TASK-010
kind: implementation
status: proposed
spec: SPEC-github-verifier-pipelines
spec_revision: 7
approved_spec_revision: 7
requirements: [REQ-004, REQ-005, REQ-009, REQ-010, REQ-011, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018]
acceptance: [AC-003, AC-005, AC-006, AC-007, AC-009, AC-010, AC-011, AC-012, AC-013, AC-014]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007]
depends_on: [TASK-001, TASK-003, TASK-004, TASK-006]
blocks: [TASK-005, TASK-007, TASK-008, TASK-009, TASK-006-acceptance]
---

# Execute verification from memory with bounded background persistence

## Outcome, agreement and prerequisite

A direct request and a prepared queued claim use the same verification engine.
Step progress, publication acceptance and terminal selection complete in live
memory without SQL lease confirmations, outbox admission or persistence ACKs.
Direct returns its selected result; queued computation releases its execution
resources while a bounded background owner persists the result and settles the
claim. Persistence outages stop new queued claims when tracking fills, not the
progress of already-running computations or isolated direct execution.

The maintainer agreed the [nonblocking execution design](../design/approved-nonblocking-execution.md)
and requested this task revision. That record owns the selected flow and trade-offs.
It supersedes this task's former cache-only refusal, terminal-on-shed completion,
notification-removal and invented 5% benchmark-tolerance choices.

**Approved contract:** spec revision 7 records the maintainer's explicit approval
of this runtime agreement. Before affected production changes, reconcile the
contradictory owner-document text and dependent packet wording listed below.
This task remains proposed; recording spec approval and reviewing its wording
does not claim implementation feasibility or performance.
Task 004 storage remains approved and is not reopened.

Retain one bounded task. No Bifrost optimization, catalog/read/write-contract
change, cache redesign, new Verifier query surface, standalone gateway redesign,
new queue/WAL/finalizer service or stronger crash durability is authorized.

## Existing owners and connected implementation

Resource-scaling (`change/resource-scaling`, including PR 236; RS below) is the
performance and reuse baseline. Preserve its caches/preparation, ownership moves,
compact results/details, shared schemas, bounded encoding, fair batched claims
and completion, and separate direct/queued execution/workspace/Python/model lanes.
Necessary cold target loads remain preparation; do not turn them into a new 503
and client retry requirement. Preserve existing authorization and credential owners.

### Direct and queued entry into the same engine

| Stage | Direct verify | Queued |
|---|---|---|
| Input and preparation | Existing tenant-authenticated request → input validation, authorization and exact target resolution, including necessary cold loads | Existing admission/queue → batched claim → pinned input/source/policy, accepted authority, original deadline and exact accepted Bifrost recovery |
| Engine handoff | Fresh live RunState, direct resources, request cancellation/deadline | Prepared live RunState, queued resources, local lease-loss token and original deadline |
| Engine work | Validate/mutate state and release dependents from memory; gateway/tool/Python/file IO remains actual work | Same engine and state semantics; no per-transition SQL, outbox staging/reservation or ACK |
| Computation ends | Select stable terminal result, respond and release resources independently of persistence | Select stable terminal result, release resources; background claim tracking continues |
| Background continuation | Bounded evidence publication; response does not certify retained details exist | Bounded terminal retention/retry → accepted canonical terminal evidence → token-fenced batched queue settlement |

`RunState` in `wyrd-server/src/verification/state.rs` remains the live owner;
`vala-verify/src/blocks/engine.rs` retains begin/forget/settle/control semantics.
Remove `RunState::durable`/`StateScribe::confirmed` waits from mutation success,
publication replay and terminal selection. Keep validation, publication call
identity/replay/conflict rules, limits, immutable descriptors, frozen control
choices, lineage, spent budgets and one stable in-memory terminal decision.
Do not introduce a second engine or stateless execution adapter.

Extend the current run-state/outbox lifecycle so execution records dirty progress
in memory and background work observes it, freezes consistent coalesced snapshots,
accounts retained memory and encodes/delivers them. Reuse existing encoding and
outbox capacity. No whole-state serialization/counting/copy per engine mutation,
no lock held across encoding/IO, and no unlimited snapshot backlog. Snapshot
ownership/accounting must be established before retained copies escape a bounded
owner; an unknown weight must not silently bypass byte limits. Private snapshot,
notification and locking mechanics are implementation-owned, with contention
and memory proof required below.

`components/verification/service.rs` returns the live direct result without
result-slot gating or terminal durability waits. Preserve RS ordinary enqueue's
bounded staging response rather than the added admission waiter. Preserve PR
webhook durable acceptance/deduplication/generation checks; it is a control flow,
not engine progress. Full request staging may refuse work before execution starts.

### Background lease and bounded queued completion

Move target/input/recovery preparation out of executing engine work into the
existing runner preparation owner. Do not reinterpret unavailable recovery as
absence. Register the claim with `LeaseRenewal` before dispatch and retain that
background registration through persistence/completion, without compute permits.

Remove executing callers of `HeldLease::confirm`; delete the method only if unused.
The existing background renewal owner updates local lost/cancellation tokens.
The engine checks those tokens and its deadline without waiting for SQL. Keep
background notification responsiveness; deleting inline checks does not authorize
100-second cancellation polling as the only normal observation mechanism.
Cancellation cannot retract an already-sent remote call.

Keep the existing runner responsible for its claim from preparation through
execution, background persistence and settlement; extend `Completions` within
that lifecycle. Compute permits cover computation, not the entire claim lifetime.
Do not introduce a separate completion manager, retry service or repair workflow.

On the successful path, retain finished queued state for background persistence
retry, then perform token-fenced SQL completion after canonical terminal evidence
is accepted. A shed terminal staging attempt is not successful queue completion.
Preserve the runner's existing error, cancellation and lease-loss transitions and
cleanup when moving persistence out of computation. Permanent recording rejection
must follow existing failure handling rather than retrying as temporary pressure.
Lease loss ends the old owner's retries/settlement and releases its local tracking;
previously submitted IO can still finish. Do not fabricate an ACK or introduce a
new public failure/durability policy. Verify these existing paths through the
changed lifecycle; report a concrete incompatibility if reuse cannot preserve the
approved contract. ACK consumers retain no computation-owned permits and hold
no RunState lock over IO; the existing claim permit remains held as described below.

Reuse `ClaimLoop`'s existing permit to bound each claim's complete lifetime,
including background persistence and settlement. It is acquired before claiming
and already stops new claims when capacity is full. Release computation-owned
workspace, Python and model resources when computation ends; retain the claim
permit until settlement or existing cancellation/lease-loss cleanup finishes.
The claim permit is lifecycle tracking, not a compute resource that must be
released at terminal selection. This preserves `Completions`' existing bound
without detaching it or adding another tracking queue, semaphore or completion
manager. Preserve per-run/outbox byte limits, fair batched claims and group
snapshot capacity semantics. Already-running work can finish while claim
capacity is full, and the isolated direct lane remains available.

Admission, claim, renewal, cancellation and completion mutations use short,
tenant-scoped group transactions and set-based batches. Acquire a connection only
for a ready bounded batch, commit, release it, then notify consumers. Do not hold
connections while gathering a batch or waiting for engine/provider/Bifrost/file
work. Normal low-traffic batches may contain one row. Retain batched retry/error
handling rather than fan out a failed group into unbounded single-row commits.
PostgreSQL derives coordination timestamps; execution deadlines keep original
accepted bounds, not a fresh budget on reclaim.

### Gateway and server-produced publication edges

RS already has cached gateway tenant snapshots, an in-memory quota ledger,
background settlement/capture and background encoding/delivery. Reuse them.
For verification-originated calls, remove the remaining capture-slot admission
wait; capture persistence cannot gate provider dispatch or the engine's answer.
Retain provider/model admission, quota accounting and per-call deadlines, including
model-response time. Keep standalone gateway behavior. Trace actual attribution
before changing `AnyRegistered`'s conditional registry lookup; do not infer that
prepared verification calls take it. Reuse validated execution context where
appropriate, without a public bypass flag or weaker authorization.

For `verification/hosted/publications.rs::Publications::complete`, extend the
existing storage owner's validated server-side object put/StoredObjectRef path
so server-produced files do not require an external client upload-session SQL
round trip or inventory ACK. Keep physical IO/error handling, tenant paths,
digest/length validation, exact descriptors and read authorization; inventory
persistence is background work. Do not duplicate storage validation in verification.
External uploads retain their existing durable completion contract.

Bifrost itself is unchanged. Agent tools use its existing contracts. Automatic
execution evidence is published in background; that is not a redesign of Oracle,
Scribe, catalog caching, WAL durability or batch-fence acceptance.

### Loss, recovery, cancellation and downstream handoffs

| Event | Behavior |
|---|---|
| Outbox full / Scribe unavailable | Engine progresses and releases compute resources; background snapshots coalesce and queued terminal state remains within bounded tracking for retry. Full tracking pauses new claims. Direct evidence stays bounded/best effort with existing loss accounting. |
| Crash before persistence | Live/unaccepted progress or terminal state can be lost. Direct is not resumed. Queued work redelivers from exact accepted compatible state with the same ExecutionId/input/source/policy and original deadline; later effects/spend may repeat. No exact accounting of lost spend is promised. |
| Accepted terminal evidence before SQL settlement | Redelivery reuses stable accepted result identity and completes delivery/settlement without rerunning computation. Normal SQL completion follows evidence acceptance. |
| Unavailable recovery | Defer preparation; never fabricate authoritative absence or reset original bounds. Accepted settlements, publications and persisted spend remain authoritative. |
| Cancellation, takeover, supersession | Background lease management updates local cancellation; current queue token/generation fences govern settlement and continuation. Release compute without persistence ACK. Provider writes still validate current access/source. |
| Late old snapshot | Preserve existing accepted revision identities/conflicts/replay behavior and documented allowed old-frozen-revision race. Relocate required SQL ownership checks to existing background batched coordination before persistence; do not claim Scribe already checks a queue token or introduce new Bifrost acceptance semantics. Rejected records stay rejected. |
| Future park/resume | For 007/008, fully blocked work releases compute immediately; bounded background custody persists parked state before retiring delivery. Evidence/approval positive receipts retain their durable/idempotent contracts. |
| GitHub report delivery | 009 consumes retained reports using its independent provider retry lifecycle and current authority. Provider retries never recompute verification; missing evidence is never a fabricated clean review. |

Correct 005 and 007–009's prerequisites and incompatible assumptions, but do not
implement their features here. For 005, validated pinned extraction enters live
RunState and dependent Prompt/workspace consumers can use it immediately; only
actually persisted extraction is reusable accepted recovery evidence. Preserve
exact source/context and immutable object descriptors without an ACK-before-
consumption requirement. Task 006's prior PASS remains historical; reopen maintainer
acceptance until this runtime correction is verified. No delivery_due_at-based
SQL result mirror or new state/terminal table is needed.

### Schema disposition: remove unreleased scaffolding, preserve PR metadata

Compare to `main`, not just RS. `card_declaration_operations` and
`verification_bindings` are **preexisting tables**, created respectively by
`20260601000014_card_declaration_operations.sql:6` and
`20260601000027_verification_bindings.sql:55`; they are not new Task 003 tables.
Keep them. Keep Task 003's `selected_at`, `selection*` and selection-occurrence
columns/constraints/indexes on Cards/declaration operations, and Repository /
`pull_request` binding support. They select trusted PR policy, not engine
execution state, so they are in the explicit PR/GitHub exception. The repository
migration is currently named `20261111000002_repository_cards.sql` after the
resource-scaling merge rename; do not recreate the old conflicting timestamp.

**Delete these unreleased migration files**, remove their runtime query/fixture
references, and fold the one retained built-in `verifier:read` role grant from
the inspection migration into the existing unreleased PR/GitHub migration:

- `crates/wyrd/wyrd-sql/migrations/20261104000000_verifier_execution_invocations.sql`
- `20261104000100_verifier_execution_publications.sql`
- `20261104000200_verifier_execution_cancellation.sql`
- `20261104000300_verifier_execution_events.sql`
- `20261105000000_verifier_execution_inspection.sql`
- `20261106000000_verifier_execution_publication_custody.sql`
- `20261107000000_verifier_execution_state_in_bifrost.sql`
- `20261108000000_verifier_execution_terminal_delivery.sql`
- `crates/vala/vala-sql/migrations/20261107000000_scribe_state_heads.sql`
- `crates/vala/vala-sql/migrations/20261108000000_retire_scribe_state_heads.sql`

The unqualified filenames above are under the first migration directory.
Consequently remove `deadline_at`, `worker_format`, `delivery_due_at`, indexes
`verifier_executions_unfinished_actor`, `verifier_executions_unfinished_subject`,
`verifier_executions_delivery_due`. Keep
`20261108000100_verifier_execution_cancel_notify.sql`, its trigger and the existing
`LeaseRenewal` listener. They provide transactional background cancellation
notification on the existing queue table, without new tables/columns or per-step
SQL. Do not replace this mechanism while removing inline lease confirmations.
Remove create-then-drop invocation/publication/event/state-head scaffolding and
format-1→2 drain logic, not Bifrost's versioned checkpoint compatibility checks.
No tombstone migrations or replacement state columns. This is a deliberate
maintainer-directed squash of changes absent from `main`, before release; it is
not an instruction to alter a deployed migration history. Recreate only isolated
branch test databases for proof; never reset a user's database. The normal SQL
rule against rewriting applied release migrations still stands.

**Keep** `20261108000100_verifier_execution_cancel_notify.sql`,
`20261110000000_github_installations.sql`,
`20261111000002_repository_cards.sql`,
`20261112000000_repository_selection_occurrence.sql`,
`20261113000000_code_review_cache.sql`, and
`20261114000000_pull_request_admission.sql`, including all stated Task 004,
GitHub login-state and PR columns. Keep resource-scaling gateway migrations
`20261111000000_gateway_usage_contributions.sql` and
`20261111000001_gateway_usage_overlap.sql`; they are not introduced by this
runtime correction and must not be removed under a blanket SQL deletion.
Remove a deleted-column reference from retained PR migrations if needed, without
removing PR behavior. Verify a fresh migration preserves the approved GitHub/PR/
Task 004 metadata and retained background cancellation trigger versus `main` plus
the incoming resource-scaling baseline, with no generic execution-state tables
or replacement state/terminal columns.

## Authority and dependent packet alignment

The approved runtime amendment requires these owner/dependent document updates
before affected production changes:

- `github-verifier-pipelines/spec.md` and `contracts.md`: the mapped obligations,
  direct/queued preparation and memory execution, bounded queued terminal custody,
  accepted-versus-unpersisted recovery, and compute-free park/completion handoffs.
- Design supplements `approved-runtime-and-github.md` and
  `approved-deferred-inputs.md`: make the approved correction's precedence explicit
  rather than retaining conflicting engine ACK prerequisites.
- `verifier-review-pipelines/spec.md` and `execution-state-and-durability.md`:
  remove required per-step SQL/progress ACK gates and unconditional preservation
  of unpersisted progress; preserve exact accepted recovery and background queued
  terminal retention/ACK-before-settlement. Mark incompatible historical claims
  superseded, not newly verified.
- `architecture/wyrd-design.md` verification/publication ownership and relevant
  operations recovery/runbook text: describe the agreed flow, bounded custody,
  cancellation observation and unreleased schema cleanup. Align only verification
  descriptions in `architecture/bifrost-design.md` if needed; leave general
  Bifrost architecture/contracts/implementation unchanged.
- Task 005 and its index: require the corrected runtime, distinguish validated
  live extraction from durable recovery custody, and remove checkpoint/Scribe
  ACK-before-dependent-consumption wording while preserving exact source/context.
- Tasks 007–009 and their index: require 010, use background park/ACK handoffs and
  independent provider delivery without new SQL delivery fields. Reconcile 006
  acceptance status without erasing historical evidence.

Read security posture at the affected authorization/storage/fencing owners.
Maintain current permissions, RLS and provider-write authority checks. Spec revision 7 is approved. Architecture updates remain task work; do not
implement against contradictory owner text or imply those updates already landed.

## Connected proof

Runtime obligations map to approved REQ-017–018 and AC-012–014, alongside
the existing protected behavior identified below.
Each proof uses existing test owners and runners.
New tests use existing binaries; implementers record actual names/focused commands
before RED. No new benchmark framework, SQL-call production hook or test binary.

| Proof | Trigger → required observation |
|---|---|
| O1 — direct result/details (REQ-017, AC-012; REQ-016, INV-002,004) | Existing HTTP WAL-full/missing-details test remains unchanged: compact passing result returns despite unavailable retained details; existing after-drain comparison still holds. Preserve user-visible result/detail split and ordinary cold preparation behavior. |
| O2 — shared memory engine (REQ-017, AC-012; REQ-015–016, INV-005–006) | Unit tests hold persistence ACKs across begin/settle/control/publication replay/terminal selection: operations complete from memory. Real direct and queued SDK journeys hold model transport, fill outbox, release provider response: dependent executes and result is selected with computation-owned workspace/Python/model resources released before drain; queued claim permits remain held for background persistence/settlement, while direct execution admission is released with its response. Inspect/measure that no outbox staging or whole-state encoding occurs on engine transitions. |
| O3 — background lease/group commit (REQ-017, AC-012; REQ-004,015, INV-001,005–006) | Hold background renewal/completion replies while prepared executions run: no engine SQL attempts, local-token cancellation still stops further dispatch. Use existing tracing/instrumentation or a narrowly scoped test-only counter to distinguish preparation/background SQL from execution; do not require zero SQL across the entire cold direct request or across Agent Bifrost tool internals. Concurrent completions/renewals use tenant batches, promptly release connections, and do not fall back to one transaction per row under group failure. |
| O4 — bounded terminal custody (REQ-017, AC-012; REQ-015, INV-002,004–005, AC-010) | With real SQL/Scribe seams, block persistence, complete queued work, and fill tracking including already-running claims: finished results remain retained, computation-owned resources are free and existing ClaimLoop permits remain held, bounding running plus completing claims without another tracker; per-run/outbox byte limits hold, exhausting claim permits pauses new claims, and direct work still succeeds. Restore persistence: the same results are retained, then token-fenced queue completion commits in batches. No engine recomputation and no terminal-on-shed completion. Exercise the existing recording-error path and verify permanent rejection does not create endless retries or leak tracking. |
| O5 — recovery/fences/park handoff (REQ-004–007,009–011,015; AC-003–007; INV-001–007) | Crash/reclaim before acceptance permits only unpersisted work to repeat under original identities/bounds. Accepted terminal evidence before SQL completion is reused without computation. Race cancelled/obsolete claim and background persistence/settlement: accepted records retain existing conflict/replay semantics and obsolete authority cannot resume or publish current reviews. After terminal selection with ACK held, existing lease-loss/cancellation cleanup releases the existing claim permit without settling a replacement claim. Exercise cancellation through the retained notification trigger/listener and preserve its responsiveness. Task 005 later proves live extraction feeds Prompt/workspace without a persistence ACK and accepted extraction recovers exact source/context. 007–009 later prove parked ACK waits retain no compute permits and provider retries never recompute. |
| O6 — schema/consumer closure (REQ-018, AC-014; REQ-005,016, INV-001,007) | Fresh isolated migrations/SQL integration preserve permitted GitHub/PR/Task 004 deltas and the retained background cancellation trigger versus main plus RS, without generic execution-state tables or replacement state/terminal columns. Retain role grants, RLS, selection/supersession and extraction stores; compile deleted-column consumers and retain the cancellation notification migration/trigger/listener; these add no generic execution-state table or column. No Bifrost runtime changes or new generic execution-state tables/columns. |
| O7 — resource-scaling performance (REQ-018, AC-013; REQ-015, AC-010) | Full matched L1/L2 verification experiments on immutable RS and candidate, same host/method/profile/rates/features, three repetitions. Report p99 overhead, throughput, server CPU efficiency and peak memory with source IDs and all repetitions. Meet existing targets including direct p99 overhead <500 us at 64 clients and L2 backlog <=1 s offered work, drain <=60 s, recovery <=30 s; preserve the RS baseline. Healthy runs reconcile retained evidence and have no unexplained loss/refusals. No invented 5% regression allowance; noisy/invalid/unsettled comparisons or missed targets are missing proof, not PASS. |

State unit tests below currently encode ACK-wait semantics; adapt those assertions
to memory progress while retaining coalescing/concurrency/conflict coverage.
Terminal-delivery integration tests retain background ACK-before-SQL-settlement
proof, add compute-permit release and bounded tracking, and allow loss before
acceptance on process death. Keep accepted checkpoint/publication/replay tests.
Replace only obsolete SQL format-migration scaffolding assertions with retained
Bifrost format compatibility checks; do not delete security/recovery coverage.

## Verification and handoff

The following existing selectors and mise tasks were source-checked during this
revision, not executed. Run Cargo commands sequentially. New/renamed tests need
actual focused selectors recorded by implementation; existing named tests run
through their exact commands, not only the aggregate lanes.

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib \
  -E 'test(=verification::state::tests::siblings_record_progress_while_a_revision_is_held) | test(=verification::state::tests::held_changes_share_the_next_revision_across_cancelled_waiters) | test(=verification::state::tests::a_refused_revision_releases_nothing)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-server --test integration --test-threads=1 \
  -E 'test(=pg_verification_routes::details_report_absence_when_retained_records_are_missing)'

mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked \
  -p wyrd-server --test integration --test-threads=1 \
  -E 'test(=pg_durable_verifier_work::invocations_resume_from_bifrost_with_original_bounds) | test(=pg_durable_verifier_work::publications_recover_exact_descriptors_from_run_state) | test(=pg_durable_verifier_work::batched_checkpoints_recover_exactly_the_accepted_batch) | test(=pg_durable_verifier_work::cancelled_work_keeps_checkpoints_and_fences_stale_writes) | test(=pg_durable_verifier_work::terminal_delivery_survives_every_commit_boundary) | test(=pg_durable_verifier_work::cancellation_writer_race_preserves_terminal_delivery) | test(=pg_durable_verifier_work::state_format_changes_preserve_existing_custody)'

mise run test:sql
mise run test:wyrd
mise run test:vala
mise run test:rust-sdk:journey
mise run test:bifrost:journey:server
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise run docs:check

mise run bench:l1:verification
mise run bench:l2:verification
```

Run benchmarks at full default coverage, sequentially on a quiet host, using
`benchmarks/README.md` validity and measurement rules. Existing diagnostic
`--allow-dirty` output cannot replace final immutable-candidate qualification.
Use an isolated baseline checkout or later benchmark workflow; do not checkout,
stash, reset or finish/repair this current merge to obtain a baseline. No Git
state change, benchmark run or production implementation is part of revising
this task. Never reset a user database for schema proof.

Integrated review owns complete Python/TypeScript/MCP regression lanes. If this
implementation changes a projection, include that surface's focused journey here;
no public API/type change or manufactured binding churn is selected.

Map O1–O7 to actual commands/results, separate live retention from durable
acceptance, and present the RS comparison before renewed Task 006 acceptance.
A benchmark exit zero alone does not prove that its performance targets passed.

Independent comprehension: one bounded read-only reader checked the current
spec/contracts, task packet and revised flow. Its Task 005 extraction-custody
alignment finding was corrected and independently rechecked; no material
comprehension gap remained. A bounded independent recheck confirmed the later
ClaimLoop-permit reuse and retained cancellation-notification revisions agree
with spec revision 7, including O2/O4/O5/O6. This is not implementation feasibility,
correctness or performance verification; owner-document reconciliation remains pending.
