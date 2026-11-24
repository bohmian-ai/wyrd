# TASK-003 independent behavior review

Reviewed base `74034459d468fd0adf277e3b51c32b1a676e8e55` through candidate
`c0900bff523e83d3666658440daa81355067cb34`, using the cumulative Git diff.
HEAD matched the candidate during this pass. The approved working-tree contract
is SPEC-bifrost-write-throughput-and-retention revision 4; the original task is
`tasks/TASK-003-bounded-write-pipeline.md`. Intentionally uncommitted packet and
unrelated benchmark edits are outside the source candidate.

## Scope and authority

Primary scope: REQ-014–020, with detailed examination of coordinator scheduling,
collection, duplicate exclusion, visibility/ACK ordering and deterministic proof.
Failure/custody/lifecycle and collateral hosted Verification changes have separate
specialist assignments. This report does not claim to replace those reports.

Read the repository AGENTS instructions, spec-driven workflow, maintainer style,
TESTING.md, approved spec, original task and Bifrost's Ingest: Scribe authority.
Used CodeGraph before tracing source. Navigation: runtime `try_send` routes an
admitted append into `ShardOwner::run`; `TenantTableRoundRobin` selects requests;
`dispatch_next`/`start_group` plan a group; the retained `GroupStage` future runs
conversion and the existing WAL lane; only `older` starts `fence_group`; completion
inserts, records the committed mirror, publishes inspection and acknowledges.
Generation mutation and recovery remain with the existing owner.

## Proposed findings

These source-local IDs need independent validation; they are not final retained IDs.

### B1 — INCORRECT: ready head completion loses priority to next preparation

- **Authority:** REQ-015 explicitly requires servicing ready head completion
  before new preparation and bounded mailbox/preparation work that cannot starve
  it; REQ-019 and task scenario 2 require proof of this scheduling property.
- **Location:** `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:1540–1561`
  (`run`), `:1606–1610` (`drain_mailbox`), `:4132–4183`
  (`dispatch_next`), `:4070–4096` (`start_group`).
- **Starting conditions/trigger:** N is fencing, its completion becomes ready
  after a mailbox wake, and enough eligible requests for the next group are
  already queued. This is an ordinary concurrent-write state with a finite
  admitted backlog; it does not require limitless resources or overload.
- **Expected:** settle N, publish its rows and ACK before preparing the next
  group. Mailbox processing must periodically give stage completion a chance.
- **Actual:** every ordinary loop iteration first drains the mailbox, then
  calls `dispatch_next`, which synchronously sizes requests, checks retained
  rows/layouts and plans conversion in `start_group`/`plan_group_write`.
  Only afterwards does the biased `select!` poll the ready older stage.
  Bias inside that later select cannot prioritize work performed before it.
  `drain_mailbox` additionally loops to observed queue exhaustion without a
  chunk bound or completion poll; mailbox capacity 256 bounds instantaneous
  occupancy, not how many concurrent sends one drain can consume. Admission
  memory bounds the total retained backlog, but does not restore the promised
  priority or a fixed mailbox service bound.
- **Evidence:** direct control flow above; the production overlap test at
  `:12182–12301` verifies N's ACK while N+1's IO is held, but does not make a
  ready head compete against new preparation. The collection unit test at
  `:10908–10955` tests a predicate in isolation and cannot establish event-loop
  priority.
- **Consequence:** an already-completed durable write can wait behind unrelated
  next-group preparation and a potentially long mailbox drain. This violates
  the explicit completion contract the pipeline was intended to preserve.
- **Smallest testable correction:** fix ordering and bounded mailbox service
  at the existing shard loop, retaining owned stage futures and FIFO/barrier
  behavior. Add a deterministic coordinator scenario with N's completion ready
  and next-group input queued; assert N becomes visible/ACKed before next
  preparation starts, including a finite backlog larger than a service chunk.
  Do not add a scheduler framework or elapsed-time performance threshold.

### B2 — INCORRECT: held requests prematurely satisfy the collection target

- **Authority:** REQ-015 defines collection over eligible requests, with the
  32-request soft target ending only the wait for further arrivals; REQ-016
  excludes identities held by a dispatched group while other tables proceed.
- **Location:** `crates/vala/vala-bifrost-redux/src/scribe/shards.rs:4151–4164`
  (`dispatch_next`), `:3868–3876` (`GroupCollection::due`),
  `:649–682` (`TenantTableRoundRobin::pop_group`).
- **Starting conditions/trigger:** N holds batch A while fencing. Its table
  queues 31 retries of A; an independent table queues one eligible batch B.
  The predecessor is incomplete and the first eligible collection deadline
  has not expired.
- **Expected:** B forms a short overlapping collection and can await arrivals
  until its fixed deadline, target/cap, or predecessor completion. The blocked
  A requests do not belong to the collected group and cannot satisfy its target.
- **Actual:** `scheduler.len()` is 32, so `due` returns true immediately and
  clears the deadline. Only afterwards does `pop_group(held_identities)` filter
  the held table, dispatching B alone. The shortcut comment acknowledges that
  queued counts include held identities, but does not preserve the specified
  collection semantics.
- **Evidence:** `len` increments on every scheduler push and decrements only on
  pop (`:615–620`, `:679`); `pop_group` skips tables with held front identities
  (`:655–665`). The existing collection test passes counts directly; the
  cross-slot retry test uses only three requests. Neither exercises the target
  and held-identity intersection.
- **Consequence:** supported retry bursts force small WAL groups even though
  the configured collection conditions are unmet, weakening the agreed batching
  behavior for the performance-sensitive path.
- **Smallest testable correction:** have the existing scheduler/collection
  owner base the target and first-request deadline on the eligible collected
  group, preserving held-table FIFO and other-table progress. Add controlled
  tests for 31 held retries plus B, later eligible arrivals, unchanged deadline,
  and predecessor-complete dispatch. This does not require a benchmark.

### B3 — DRIFT: new architecture prose changes the agreed soft target into a cap

- **Authority:** REQ-015 and the finalized proposal explicitly require draining
  ready input above 32 to existing limits (64 requests); the task requires an
  aligned architecture update. AGENTS requires correcting contradictions in
  the owning document.
- **Location:** `architecture/bifrost-design.md:925–927`.
- **Starting conditions/trigger:** more than 32 eligible requests are already
  ready when the overlapping group collects.
- **Expected:** the owner states that 32 ends waiting for new input, while
  already-ready input drains to existing limits.
- **Actual:** the new text says the slot collects “up to 32 requests or until
  1 ms ... whichever comes first.” That describes a hard cap. Production
  `pop_group` instead drains to `MAX_GROUP_ITEMS == 64`, consistent with the
  approved spec but inconsistent with the newly edited owner.
- **Consequence:** contributors applying the narrower architecture authority
  would cap groups at 32 and regress the approved batching behavior.
- **Smallest correction:** correct this bounded architecture paragraph to
  distinguish waiting from draining and name the existing cap. Content review
  is sufficient; no production change is required for this finding.

### B4 — EVIDENCE_GAP: connected healthy multi-tenant/table pipeline proof

This is maintainer-confirmed required closure coverage relayed during this review,
not a newly inferred product promise.

- **Authority:** REQ-015's preserved tenant/table fairness and per-table FIFO,
  REQ-018's tenant/table ownership, REQ-019's real-boundary deterministic proof,
  and task scenario 2's controlled multi-tenant/table input through the scheduler.
- **Location:** pipeline PG tests in `scribe/shards.rs:12182–13200`, alongside
  scheduler-only fairness tests at `:10800–11100`.
- **Missing proof:** the healthy repeated-overlap test uses one tenant and one
  table. The cross-slot retry case uses another table within that tenant. The
  real two-tenant test proves partial-fence failure and fail-closed recovery,
  rather than healthy multi-tenant/table scheduling. Pure scheduler fairness
  assertions do not observe interaction with two-slot ownership, collection,
  publication or resource settlement.
- **Smallest correction:** add a deterministic real production shard/WAL/PG
  scenario streaming several groups across the same production shard with
  distinct tenants, multiple tables, repeated slot reuse and competing ready
  inputs. Repeated reached/release handshakes must prove interleaving and
  independent ACKs there, alongside tenant/table progress and per-table FIFO,
  fence/visibility order, correct rows per tenant/table, cross-slot identity
  exclusion and eventual tenant/table charge settlement through the connected
  pipeline. Reuse `PipelineFixture` and the existing stage holds;
  production changes are required only if the test exposes a defect. No
  benchmark or elapsed-time threshold is necessary.

## Primary coverage matrix

| Requirement / scenarios | Implementation and assertion evidence | Primary result |
|---|---|---|
| REQ-014 / 1–2: repeated two-slot WAL/fence overlap and ordering | `GroupPipeline` stores older/newer only; `dispatch_next` disallows dispatch during WAL work or with newer occupied; `start_oldest_fence` gates fencing. `later_groups_sync_while_earlier_groups_fence_and_ack_independently` uses real PG fence blocking and a post-durable WAL sync hold over four groups, checks each durable fence count and visible row count, then releases each slot. | PASS for inspected overlap/order mechanism; lifecycle specialist covers retained/rotating states. |
| REQ-015 / 1–2: independent ACK, visibility and mirror | Normal completion calls insertion, `record_committed`, `publish_snapshot`, then ACK (`:4454–4478`). The real coordinator stream test asserts N ACK and visible rows while N+1 sync completion remains held. | PASS for normal independent ACK; FAIL for completion priority (B1) and eligible collection target (B2). |
| REQ-015 / 2: deadline, target, ready drain, fairness | `GroupCollection` keeps first deadline, immediate idle dispatch; `pop_group` keeps 64 cap and tenant/table service. Pure timer assertions cover fixed deadline/idle/target; held-identity unit assertions cover FIFO and preserved turn. More than 32 ready eligible items still drain to 64 by source. | FAIL for B2; architecture correction B3; connected healthy multi-tenant/table proof missing (B4). |
| REQ-016 / 2–3: identity and acceptance | `held_identities` spans both dispatched groups; scheduler skips a held table front without skipping later arrivals in that table. Real PG retry test checks one visible copy, two durable fences for two identities, four WAL submissions and two syncs. Existing `fence_group` continues tenant batching, retained-version resolution and SQL arbitration; no normal-path preflight SQL was added. | PASS for inspected identity/normal durability paths; failures/recovery reconciled with specialist. |
| REQ-017 / 3: explicit failure ownership | Inspected group result states, refusal, older-only fence and retained-owner control flow; task names real two-slot partial-fence, conflict, unresolved COMMIT and younger IO-failure assertions. | Specialist-dependent: no independent primary acceptance claim for the complete failure matrix. |
| REQ-018 / 3: resources and lifecycle | Pipeline retains owned futures and state; pending metadata projects generation bytes; mutation barriers use existing owner. Task includes charge-release/retention assertions and real coordinator barrier cases. | Specialist-dependent for complete lifecycle/custody coverage. |
| REQ-019 / 1–4: deterministic progress proof | Production coordinator, real WAL and PG, observed sync reached/release, no elapsed speed threshold. Stream test detects serial writes and ACK join barriers. Fixed-input submission counts demonstrate absence of redundant WAL work only. | FAIL insofar as B1/B2 lack the required behavior/proof; successful overlap evidence does not measure production throughput. |
| REQ-020 / 5: final maintainer benchmark | Task explicitly leaves benchmark pending and provides the prescribed command and sole baseline. No benchmark was run during this review. | UNRESOLVED final qualification, deliberately deferred by the current maintainer instruction. Not a code-review defect or demand to benchmark before review. |
| Repository journey/check authority / 4 | Task records Scribe 53 passed/1 skipped, OTLP 21, Redux integration 1080, final Redux lib 1010 three times at candidate, fmt/lints, and collateral SDK controls checks. Existing end-to-end consumers still reach this owner; no Bifrost public API changed. | Available reported execution evidence plus assertion/source review; collateral scope assessed separately. |

## Evidence and process limits

This pass independently inspected candidate source and assertions, not raw retained
test execution logs. It ran no Cargo command, full journey lane or benchmark;
reported run counts remain implementation evidence. No source, task or spec was
changed. Read-only identity check confirmed HEAD remains the candidate.

Behavioral review recommendation: **FIX_REQUIRED** if independent validation
retains B1/B2, the bounded B3 correction and maintainer-confirmed B4 coverage.
Final acceptance still needs specialist
reconciliation and fresh independent validation.

TDD compliance: **VIOLATED**, as explicitly recorded for rotation, deferral,
FlushAll, shutdown, cancellation, conflict, post-fence retry, later IO failure and
held-identity fairness code written before those tests. Later passing tests cannot
repair that history; no historical RED recreation or production churn is requested.
