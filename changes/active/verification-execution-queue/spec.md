---
id: SPEC-verification-execution-queue
revision: 2
status: approved
---

# Durable verification admission, execution queue, and retention

## Objective and approval boundary

A successful Run `submit` durably queues work before returning. Narrow live
coordination lets runners drain without repeatedly rewriting wide execution
records. All execution status history expires 24 hours after settlement;
Bifrost evidence and compact group state have independent lifecycles.

Revision 2 is **approved**: the owner explicitly approved it as a whole on
2026-10-20, including request-path transactional admission, submit input
evidence staged at settlement, uniform execution expiry and compact group state.
TASK-001 implements it.

## Required behavior

**REQ-001 — One execution horizon.** Every execution record, including manual,
schedule, group, and terminal refusal records, expires when terminal
(`completed`, `errored`, `cancelled`, `timed_out`) and
`settled_at + 86400 elapsed seconds <= statement_timestamp()`. SQL binds the
duration and computes the cutoff on the PostgreSQL clock. Pending, running and
retrying work never expires. A missing settlement timestamp never authorizes
deletion; require it on new terminal writes.

At the boundary, status returns existing
`WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND`, even before physical cleanup.
Status assembly must not resurrect an expired execution from Bifrost.
Binding `last_execution_id` means latest retained admission; NULL after expiry
is acceptable. It is presentation, never dedupe state or a history guarantee.

**REQ-002 — Transactional request admission and idempotency.** Run `submit`,
through HTTP, MCP and every first-class SDK, authorizes/audits, validates the
request, and admits new work in **one PostgreSQL tenant transaction**: keyed
mapping when present, wide execution record, and live queue row commit together.
Only after successful commit return `202 { execution_id }`. No committed key may
exist without its execution, and no accepted executable record without its queue
row. Denied/invalid requests roll back without reserving a key or creating work.
Every newly accepted execution uses `ExecutionId::new_v7()`; remove
`RequestKey::execution_id` hash derivation. Unkeyed requests also use this
transaction and a fresh ID, without a key row.

Key scope remains `(data_tenant_id, sub_principal_id, act_principal_id,
idempotency_key)`. Keep `RequestKey::new`'s SHA-256 over JCS canonical
`VerificationRequest` bytes as the fingerprint. The key table's unique
constraint and `INSERT ... ON CONFLICT` arbitrate concurrent admission; no
advisory key-lock protocol is needed. A waiter reads the committed winner using
a fresh statement snapshot after a conflicting insert, rather than assuming an
uncommitted row is visible in the insert statement's snapshot.

Within the horizon, same key + same digest returns the original execution and
identical 202 body, including while it runs. Authenticate/authorize each retry;
check mutable target readiness for new admission, not to invalidate replay of
already accepted work. Resolve and validate exact targets before new work commits. Same key + different digest returns
HTTP **422**, synchronously, using the new derive-backed stable code
`WYRD_VERIFICATION_422_IDEMPOTENCY_KEY_REUSED` (proposed, absent today). It creates
no conflict execution, queue row or evidence. Remediation says to retry the
original payload or use a new key for different work; errors disclose no other
principal's key/request. Remove registry-idempotency-conflict handling from
verification submission; retain `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT` for
Card registration, which still uses it.

No duplicate-specific 409 is reachable: the unique insert waits for the
admission winner to commit or roll back; the asynchronous job still running
is not an unfinished enqueue request. A bounded database/request wait can fail
with existing unavailability, not a fabricated in-flight conflict. A rollback
leaves another contender free to admit. Lost responses or uncertain commits are
safe to retry with the same key. Unkeyed transport retries have no dedupe promise.

A key never expires while its execution is pending/running/retrying. Terminal
expiry is exactly its execution's `settled_at + 86400 s`, never key creation
or last-replay time. The mapping is insert-only: no claim, renewal, retry,
settlement or replay updates it. At expiry delete it, or atomically delete and
replace it on next use under ordinary row/unique-constraint serialization.
Concurrent replacement and cleanup must not remove a new mapping; qualify
removal by its mapped execution identity and recheck eligibility under lock.
The expired record may await cleanup but cannot block reuse. Reuse, even with
the same body, accepts one new execution with a new UUIDv7. Cleanup deletes old
mappings and records consistently; there are no orphan reservations,
generations, hashed conflict IDs, or delayed-ACK admission fences.

**REQ-003 — Bounded cleanup at the existing owner.** Extend
`VerificationScheduler` in `crates/wyrd/wyrd-server/src/verification/scheduler.rs`
on its existing cron-loop cadence. Each pass has finite discovery, scan/time
and deletion budgets, short transactions, cancellation and repeatable progress.
Run cleanup with no due bindings, no runnable work, no hosted groups, and all
execution permits occupied. Find quiet tenants with only expired history;
the due-work tenant list is insufficient. Errors are traced and retried next
pass without disabling scheduling. Concurrent replicas skip locked rows and
make progress past a locked oldest row. LIMIT alone is not a scan/work bound.

`VerifierExecutionQueue` in
`crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs` remains the SQL owner.
Cross-tenant discovery/statistics use `OperatorPool` and the sanctioned
`// tenant-isolation: cross-tenant OperatorPool` annotation. Destructive tenant
work uses `TenantConn`; the caller owns commit/rollback. Use a **full BRIN
settled_at index**, no partial status predicate, and exact row eligibility.
Prove bounded work with disordered settlement times, not only sorted fixtures.
No partitions, partition manager, new daemon, leader or timer service.

**REQ-004 — Terminal payload clearing.** Every terminal insert/transition sets
`inputs`, `route_overrides`, and `authority` to SQL NULL atomically, including
normal completion, explicit termination, retry exhaustion, exhausted leases,
and refused group creation. Retry/release/defer retain executable payloads.
Preserve attribution, frozen targets, result/error, terminal lease fence,
settlement time and group provenance until record expiry. Capture the evidence
needed by REQ-009 before clearing; do not reconstruct it from terminal NULLs.
A replay reads the already-cleared terminal row without UPDATE, new evidence,
new dispatches, or movement of `settled_at`.

Revise `verifier_executions_run_shape`, `verifier_executions_input_shape`,
`verifier_executions_refused_shape`, `verifier_executions_group_shape`, and
related authority/lease/status CHECKs for the split. NULL authority alone no
longer means pre-admission refusal. Preserve explicit accepted-versus-refused
provenance and strict terminal group snapshot/result shapes. Non-terminal work
requires non-NULL, well-formed authority and its proper executable input shape;
SQL CHECK's unknown result does not prove non-nullness. Do not exempt terminal
rows from all validation.

**REQ-005 — Fresh schema and vacuum.** Edit existing migrations in place, with
no forward migration, backfill, compatibility or upgrade path. Fresh databases
contain no observation-origin executions: remove their admission shape,
observation-only columns, indexes and CHECKs throughout the migration chain.
Existing development databases require explicit owner recreation; this task
must not automatically destroy them.

On record, live queue and keys set per-table
`autovacuum_vacuum_scale_factor = 0`, `autovacuum_vacuum_threshold = 10000`,
`autovacuum_vacuum_insert_scale_factor = 0`, and
`autovacuum_vacuum_insert_threshold = 10000`, with default vacuum cost delay.
These starting thresholds are not a bloat bound or a vacuum-completion promise.
The compact group state uses ordinary per-table maintenance appropriate to its
updates, with measured vacuum progress included in qualification.

**REQ-006 — Independent dispatch survival.** Execution deletion neither
cascades to dispatches nor strands pending/running/retrying delivery. Freeze the
owner Card UID needed for inline Operator resolution as server-only metadata in
existing JSONB `failure_context`; pass only existing public fields to the strict
`OperatorFailureContext` decoder. Remove the execution FK and mandatory parent
join from dispatch claims. Preserve dispatch identity, uniqueness per execution
and Operator, authority, tenant isolation, credentials, retries, deadline and
at-least-once delivery. Do not split or redesign the dispatch queue or implement
dispatch retention. Delivery already past its deadline is still ineligible;
expiry tests must not extend that production deadline.

**REQ-007 — Architecture and consumer closure.** During implementation rewrite
`architecture/wyrd-design.md` Run entry points (currently lines 1398–1411) and
Trigger queued creation (currently 1712–1725): submit uses request-path transactional
admission; 202 means durable queueing; direct verify retains its current
behavior. Rewrite the adjacent grouped-verification producer/no-request-SQL
sentences, execution input activation description, status horizons, latest
retained binding presentation, group state and dispatch dependencies too.
Update matching `architecture/bifrost-design.md` execution-input table text
(currently 242–248) and Public surface admission/attribution text (around
1419–1433). Submit input evidence becomes available to Bifrost only after
settlement staging and Scribe delivery, rather than acceptance. Group-end
activity for submit therefore follows settlement evidence ACK; direct activity
is unchanged. Remove `queue_execution` as a persisted schema field and activation
mechanism, including schema producers/consumers/tests and generated descriptions.
Any retained activation metadata is descriptive provenance, never a command.
No architecture file is edited while drafting.

**REQ-008 — Three execution tables plus compact group state.** These selected
new table names are proposed persisted contracts, not existing source symbols:

| Table | Content and lifecycle | Index/write boundary |
|---|---|---|
| `wyrd.verifier_executions` | Wide immutable accepted record and executable payload; one terminal update records result/error/final attempt/fence/settled_at and clears payload. Every origin expires under REQ-001. | Retain two existing identity constraints and applicable immutable binding/origin indexes; remove manual key ownership. No index key, INCLUDE, expression or predicate depends on mutable status/due/lease fields. `fillfactor = 70`; full BRIN on settled_at. |
| `wyrd.verifier_execution_queue` | Narrow live-only identity, state, due/lease times/token, attempts/budget, retry error and necessary coordination timestamps. No inputs, authority, Operators or snapshot payload. | Identity plus due/leased scan indexes. Admission inserts with record; claim/renew/retry/release/defer update only this table; terminal closure deletes it. |
| `wyrd.verifier_execution_keys` | Scoped unique key, canonical request digest and accepted execution UUIDv7. No payload, authority, lease or runner state. | Scoped unique index plus tenant/execution lookup index for retention. Insert once, read for replay/conflict/expiry, delete on expiry; never UPDATE. No reservation-age expiry. |
| `wyrd.verification_group_state` | One row per tenant + exact binding + group ID, with last consumed snapshot digest and source identity, last settled verdict and previous-result identity needed for existing comparison/context. | One composite primary key; mutable state is not indexed. No evidence payload, execution history, or FK to expiring execution/result rows. Lifetime follows the binding; execution expiry never deletes it. |

Enable and force RLS on **every table**, including group state, with existing
tenant-isolation and operator-access policies. Tenant-qualified references must
not introduce cross-tenant dependencies. State is a watermark/baseline, not a
per-execution archive. Its persisted identity uses the existing tenant UUID,
BindingId and validated GroupId. The consumed digest is the existing
`PinnedGroup::digest` (32-byte SHA-256/JCS); the source identity is a 32-byte
SHA-256/JCS fingerprint of exact source bindings and their per-slot ordered
execution/selected-result identities, excluding evidence values. Retain only
these fixed-size fingerprints, not an ever-growing source-ID list. The nullable
baseline pair is the existing closed verdict plus VerificationResultId, both
absent before the first verdict and present together thereafter. It needs no
historical execution row or result lookup to compare verdicts. Its binding lifetime is necessary coordination state;
no execution has permanent retention or an origin exemption.

A non-terminal committed execution has exactly one live row; a terminal record
has none. Status/list/binding readers compose live queue state with record
content so callers see current status/retry errors. Claims/renewals/retries/
release/defer never update the record or key. Terminal replay performs no write.

**REQ-009 — Settlement, evidence and queue semantics.** Preserve batched claims,
separate due/expired scans, `FOR UPDATE SKIP LOCKED`, database-clock leases and
backoff, fresh tokens, attempt refunds/budgets, fair tenant rotation and bounded
claims. Retarget `wyrd.renew_verifier_leases` and
`wyrd.complete_verifier_executions` to the split, retaining execution-ID lock
ordering across batches, single fallback and cross-table closure. Terminal
record update, live row deletion, group comparison/state update and dispatch
creation commit or roll back together. Preserve the four `Completions` flush
tasks (up to 512 per flush), reserved evidence capacity, staging before commit,
and single-settlement fallback. Group locks cover handoff/settlement state,
never Verifier/provider execution.

Remove the `verifier_submissions` in-process request outbox and Scribe-ACK-driven
submit admission, its boot/shutdown/metrics dependencies, and the
`queue_execution` activation marker. Submit request handling performs no Scribe
evidence work and reserves no Scribe/submission slot; non-blocking audit/trace
staging keeps its existing independent behavior. Thus submit has no staging-full
429. A saturated pool waits only its existing configured finite acquire timeout
(`pool.rs`, `WYRD_DB_ACQUIRE_TIMEOUT_SECS`); PostgreSQL/pool unavailability,
bounded admission database-wait timeout or commit failure maps to existing
`WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE`. Existing process-service unavailability
remains 503. No unbounded in-memory retry backlog or new public capacity knob.
A failure before commit accepts nothing; an ambiguous commit never returns 202
without confirmation, and keyed retry resolves the possibly committed winner.

At settlement stage submit's pinned input evidence **with the result through
the existing result write path**, using owned/pre-clear values and original
input-recording time, execution/Run/group/source identity and submitting pair.
Queued result attribution remains accepted subject/SYSTEM. The combined write
must preserve these distinct pairs. Terminal outcomes without a verdict stage
the admitted input evidence without inventing a result. Exhausted-lease and
retry-exhaustion paths must capture inputs before clearing too. Replay/stale
settlement stages nothing new. Preserve input-before-summary delivery ordering
and existing group notification retry behavior at the Scribe sink.

This makes accepted work durable in PostgreSQL; it does not make the settlement
Scribe outbox durable. Settlement never waits for Scribe: evidence not yet ACKed
can still be lost on abrupt death or expired shutdown, while durable work and
terminal status survive. Retained Bifrost evidence keeps its ordinary retention,
permissions and best-effort visibility; no query-completeness barrier is added.
Direct `verify`, including its input/result staging, identity, no request-path
Postgres writes, errors and no-dispatch semantics, stays exactly as today, apart from
removal of the inert activation field from the shared table schema.

The inspected pre-execution Bifrost requirement is the admission coupling being
replaced (`wyrd-design.md:1398–1411`, `1712–1725`;
`bifrost-design.md:1419–1433`). Neither the result/input table definitions nor
Durability and visibility imposes an independent requirement to publish inputs
before evaluating them. Therefore settlement staging is selected; no new durable
outbox is required. If implementation uncovers an additional hard pre-execution
evidence invariant, cite it and return for a semantic revision using a standard
transactional outbox row in the admission transaction, never in-memory
pre-admission staging.

Per-request transactions are the default. Only if measured capacity requires
it, reuse `completions.rs`'s bounded flush-task/group-commit pattern for admission
statements: each request's key/record/queue remain atomic in one tenant
transaction and every response waits for that commit. Bounded waiting/failure
must still use existing 503. This is an implementation choice, not a spec duty.

**REQ-010 — Group continuity without execution history.** Extend the existing
SQL/group owners with the REQ-008 state row. Handoff serializes per binding/group,
checks last consumed digest **and exact source identity**, and consumes unchanged
pending activity without re-enqueueing after execution rows expire. Advance the
consumed watermark atomically with successful handoff and pending consumption;
retained/in-flight execution dedupe protects overlapping identical snapshots.
A refusal without a pinned snapshot never fabricates a consumed digest.

In the group settlement transaction, lock the existing binding/group state,
compare against the last settled verdict, record the new baseline/result
identity, and create on-verdict-change dispatches atomically. First verdict
establishes a baseline; equal verdict emits none; changed verdict emits once per
configured Operator, including to/from inconclusive. Keep settlement ordering
when distinct snapshots overlap or settle out of admission order. An older
completion must not regress the separately advanced consumed watermark. Replay
changes neither baseline nor dispatches. The baseline is small data held in the
state row, not a dereference of a deleted execution or an expired Bifrost result.
Prove the same activity dedupes after physical expiry and the next changed
verdict compares against this state. Schedule dedupe continues to use locked
binding cursor advancement in the enqueue transaction; pruning never resets it.

**REQ-011 — Qualification and operational evidence.** Extend the existing
verification capacity benchmark with an open-loop **6,000 submissions/s for
60 measured seconds**, after warmup, at 64 execution slots with the same
2 KiB/four-assertion fixture. Schedule exactly 360,000 arrivals independently of
response latency. Report/fail missed or late arrivals, driver saturation,
dropped work or hidden retries/refusals; do not lower offered rate. Require
zero client-visible errors/refusals and zero server refusals from warmup through
drain, including database/pool/edge 503s hidden by SDK retries and Scribe pressure.
Remove obsolete submission-outbox counters/drain checks; absence of that counter
is not proof of zero admission refusals. Reconcile accepted execution identities,
terminal verdicts, live queue and remaining Scribe outbox. Drain all work and
Scribe writes within 60 seconds of the last scheduled arrival. Report 10-second
terminal-rate/backlog slices proving sustained target throughput, not burst-only
intake. Existing case-2 1/4/16/64-client steps still pass.

Keep direct case-1 `wyrd_verification_overhead_seconds{mode="direct"}` p99
strictly below 1 ms from at least 1,000 observations in each existing step.
Save matching-host before/after claim/settle statement time, affected rows,
distributions and total SQL time per execution, separately from acquire, commit
and flush wait. Existing `claim`/`complete_many` spans carry `statement_us` and
`batch`; count shared time once per batch and divide by rows. Report batch and
single/group fallbacks, admission latency/cost, index inventory, HOT/tuple,
heap/index size and host/workload settings. Require lower normalized claim and
settle cost; never invent missing baseline measurements. Preserve outputs before
the benchmark overwrites its directory.

Run integrated Bifrost correctness and matching-host write/query capacity
controls before/after, sequentially without competing benchmark load. Every
control passes; reject more than 5% throughput loss or p99 increase in paired
repeat measurements; noise requires remeasurement. The specified removal of
activation metadata and submit visibility timing is the only Bifrost contract
change; no Scribe ACK/durability, query, retention or admission change.

Expose/document existing PostgreSQL signals: `pg_stat_all_tables.n_dead_tup`,
`last_autovacuum`, `autovacuum_count`, `n_tup_upd`, `n_tup_hot_upd`, and
`pg_total_relation_size` with heap/index breakdown. Alert on rising dead/live
ratio and bytes while drained over consecutive sweeps, inspect vacuum progress,
and report cleanup progress/failure/oldest eligible age. Avoid tenant/execution
metric labels and invented metric names. Prove cleanup keeps up under sustained
churn; reloptions alone prove nothing about throughput or physical size.

**REQ-012 — Durable design rule and contract proof.** Implement the design rule
in `architecture/wyrd-design.md`: rewritten hot rows are narrow, live-only and
lightly indexed; durable records are written once and updated at most once;
retention has a bound and an existing owner. Document request commit durability,
key horizon, compact group state, BRIN/HOT tradeoff, vacuum signals and the
REQ-007 architecture rewrites as one coherent lifecycle.

The new group-state persisted contract and new 422 public contract require
fresh-schema/catalog/RLS proof, source-driven codegen, served OpenAPI
(`mise run test:principals:integration`), runtime MCP catalog/error proof, and
Rust/Python/TypeScript SDK-to-real-server journeys. Each surface covers keyed
replay, changed-body 422, concurrent reuse running once, reuse after expiry with
a fresh ID, status 404 after expiry, 202 surviving immediate server kill/restart,
and group dedupe/comparison across execution expiry. Keep HTTP and MCP parity,
foreign-tenant isolation, permission denial, Bifrost old/new execution attribution,
and direct verify/Window submit regression coverage. Existing SDK convenience
submit generates its own key; keyed protocol probes may use the existing public
HTTP request inside each language journey, followed by that SDK's execution
handle and Bifrost reads. No new SDK key configuration API is required.

## Expected index writes per lifecycle

Count ordinary per-tuple B-tree insertions, not heap writes, WAL records,
BRIN summaries, queue deletes or later vacuum removals. For a keyed manual
execution with one claim, no retry/renewal/binding/dispatch, current migrations
provide ten indexes in total but only four apply to its pending/running row:
two identity indexes, manual-key unique and one queue partial index. Current
acceptance 4 + non-HOT claim 4 + terminal 3 = **11** insertions; the terminal
row matches neither live predicate.

Selected path: **one admission transaction** inserts record identity entries 2,
queue identity + due 2, and key scoped identity + execution lookup 2; claim
writes queue identity + leased 2; terminal HOT record update adds 0 ordinary
entries and deletes queue. Total **8**, plus BRIN summary maintenance. Unkeyed
manual: current 3 + 3 + 2 = **8**, selected 2 + 2 + 2 = **6**. There is no separate
reservation transaction or age index. Replay/conflict inserts no execution or
queue entry and updates no existing record/key.

Scheduled/group/binding indexes add applicable initial entries; group state
adds its primary-key entry on first use and updates its unindexed state thereafter.
Report these costs separately, including any non-HOT state update. Retained
snapshot uniqueness is not the post-expiry group watermark. Retry/reclaim/
release/renew only churn narrow queue indexes. HOT depends on same-page space;
fillfactor is not a guarantee. If terminal update falls back to non-HOT, count
all applicable record-index entries and report the actual HOT ratio. A B-tree
on settled_at or mutable index predicate/INCLUDE invalidates this budget and
is prohibited. PostgreSQL 16 permits HOT with summarizing BRIN indexes.

## In-place migration closure and inspected sources

Migration paths below are relative to `crates/wyrd/wyrd-sql/migrations/`.
All exist today; new tables above are proposed additions within this chain.

| Existing migration | Required closure |
|---|---|
| `20260601000029_verifier_runs.sql` | Base record/live queue shape, RLS/reloptions and dispatch FK removal; eliminate observation origin/input shape and observation-record index. |
| `20260601000031_verifier_run_idempotency.sql` | Replace wide-row key columns/index with insert-only key table, forced RLS, lookup index and reloptions. |
| `20260601000033_verifier_run_observation_ordinal.sql` | Remove observation ordinal column, historical UPDATE, shape CHECK and unique index; it may be retained as an empty historical migration file. No observation compatibility obligation. |
| `20261016000000_verifier_executions.sql` | Reconcile names, strict terminal/executable shapes, payload clearing, full BRIN, fillfactor and origin constraints; remove historical observation cancellation/exemption assumptions. |
| `20261018000000_verification_group_pending.sql` | Preserve pending cadence; introduce compact group-state table, tenant-qualified binding ownership and forced RLS; revise affected terminal/input/group constraints. |
| `20261019000000_verification_refused_shape.sql` | Distinguish strict pre-admission refusal from scrubbed accepted terminal history, without historical observation columns. |
| `20261022000000_verifier_execution_batches.sql` | Queue/record closure in both batch functions, lock order, replay, payload capture and group fallback. |

`20261003000000_verifier_run_results.sql` and
`20261008000000_drop_verifier_run_results.sql` describe transient result storage;
do not restore it or add a PostgreSQL result-history table.

| Current owner / source | Inspected behavior and selected extension |
|---|---|
| SQL `VerifierExecutionQueue`, `Submission::new`, `RequestKey::new` / `execution_id`, `accept_submissions`, `KEYED_EXECUTIONS_SQL` in `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs` | Wide queue, hashed IDs and asynchronous conflict records become atomic request admission and scoped insert-only keys. |
| Same SQL owner: `STATUS_SQL`, `BINDING_TARGETS_SQL`, `ADVANCE_CURSOR_SQL`, `EXISTING_SNAPSHOT_SQL`, `PREVIOUS_GROUP_SQL`, `CONSUME_GROUP_SQL` | Compose live state/filter expiry; preserve schedule cursor; replace history-dependent group dedupe/comparison with state. |
| `crates/wyrd/wyrd-server/src/components/verification/service.rs::VerificationControl::enqueue`; `routes.rs` beside it; `crates/wyrd/wyrd-server/src/mcp/verification.rs` | Shared synchronous staging becomes awaited commit and synchronous 422/503 on HTTP/MCP. |
| `crates/wyrd/wyrd-server/src/verification/` — `submissions.rs::SubmissionSink`, `runner.rs::VerifierRunner`, `claims.rs::ClaimLoop`, `completions.rs::Completions`, `scheduler.rs::VerificationScheduler`, `groups.rs::GroupPoller::poll`, `results.rs::ResultPayloadBuilder` | Remove request outbox, retain existing runtime/cleanup owners, stage submit evidence at settlement through the result path. `operators.rs::OperatorWorker::resolve` must read frozen owner independently. |
| SQL `INSERT_DISPATCH_SQL` and `crates/wyrd/wyrd-sql/src/queries/operator_dispatches.rs::CLAIM_SQL` | Freeze owner UID in dispatch JSON and remove parent FK/join. |
| `crates/wyrd/wyrd-server/src/scribe_outbox.rs`; `crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs::ExecutionInputsTable` | Remove submission handoff/marker; preserve per-table evidence ordering, attribution and ACK-driven group notifications. |
| `crates/wyrd/wyrd-sql/src/pool.rs`; `crates/wyrd-spec/src/error.rs`; `crates/wyrd/wyrd-server/src/state.rs::registry_db_error` | Existing finite acquire policy and registry 503; new derive-backed 422. Card registration still uses registry 409 in `crates/wyrd/wyrd-server/src/components/cards/service.rs`. |
| `crates/wyrd/wyrd-testing/src/bin/capacity/verification/` — `run.rs::Bench`, `report.rs::Report`, `workload.rs::Tenant`, `mod.rs` | Extend existing closed-loop benchmark, report and fixture with mandatory open-loop qualification; no new load framework. |

## Invariants, scope and non-goals

- **INV-001:** Never expire non-terminal work, erase executable payload early,
  weaken lease fences, or make settlement replay mutate terminal state.
- **INV-002:** Forced RLS and existing tenant connection boundaries govern all
  tables and operations. Audit remains non-blocking authorization history;
  engine maintenance is not a new permission decision.
- **INV-003:** Pruning never resets schedule cursors or group dedupe/verdict
  state. No observation admission, retained-origin exemption or execution history
  table is introduced.
- **INV-004:** Direct verify and Window sample submission behavior remain;
  Bifrost evidence is independent of execution pruning. No new endpoint,
  compatibility alias, client-owned durable logic, dispatch retention or
  Scribe durability/query-completeness promise.
- **INV-005:** Background work and admission waits are bounded and replica-safe;
  capacity claims require measured proof, never inferred savings or synthetic
  host load.

Out of scope: a generic queue library, partitioning, gateway/Forge redesign,
new maintenance services, and an Operator queue split. Reuse existing concrete
owners and installed dependencies; use standard transactional queue and
watermark patterns throughout.

## Acceptance and credible evidence

| ID | Required outcome | Evidence class / owner |
|---|---|---|
| AC-001 | All terminal paths clear payload atomically; live NULL shapes fail; retry/release preserve payload; fresh schema has no observation execution artifacts. | SQL integration and catalog assertions. |
| AC-002 | Every terminal origin expires at the exact database cutoff, keys follow mapped settlement, non-terminal/missing-settlement rows are protected, and bounded concurrent cleanup respects locks/RLS. | SQL integration with database-relative aging and disordered histories. |
| AC-003 | Identical group activity dedupes after executions are physically deleted; next changed verdict compares to retained state, including overlapping/out-of-order settlements; schedule cursor prevents replay. | SQL + real-server group/scheduler coverage and public journeys. |
| AC-004 | Cleanup discovers quiet history and progresses with no due/group work and full execution capacity; cancellation/failure recovers; BRIN/reloptions/vacuum behavior is observable. | Scheduler integration, SQL plans/catalog and sustained statistics. |
| AC-005 | HTTP/MCP and all SDKs prove keyed replay, synchronous different-body 422, concurrent once-only admission, fresh ID after expiry, 404 at expiry, and old/new Bifrost attribution. | Real SDK/server/Postgres and MCP discover/act/observe journeys. |
| AC-006 | Dispatches claim/deliver/settle independently after parent pruning, preserving inline/Card context, identity and tenant/deadline rules; replay adds none. | SQL + existing server Operator delivery owner. |
| AC-007 | Architecture, generated contracts, served OpenAPI, runtime MCP, client descriptions and typing reflect new 422/state/202/visibility contracts; no forward migration. | Static/diff, fresh schema, codegen, served-contract, typing and boundary proof. |
| AC-008 | Only live queue changes during attempts; record has one terminal HOT-eligible update; batched/single/fallback/fairness/reclaim mechanics remain; measured index inventory matches budget. | SQL lifecycle/concurrency, server runtime and tuple statistics. |
| AC-009 | 360,000 open-loop arrivals, zero client/server refusals, sustained 6,000/s terminal rate, drain ≤60 s, lower normalized claim/settle costs; existing direct/case-2 steps pass. | Saved matching-host before/after capacity artifacts. |
| AC-010 | 202 follows atomic key/record/queue commit and survives immediate process kill; rollback/uncertain response and concurrent expiry/reuse are safe; pool/DB saturation returns bounded existing 503, with no submission staging path. | SQL concurrency, real process kill/restart, every public journey and bounded pressure tests. |
| AC-011 | Submit input/result evidence stages at settlement with correct distinct attribution and ordering; direct behavior remains; Bifrost controls pass within tolerance; vacuum/cleanup keep up. | Server evidence faults, real Bifrost reads, integrated correctness, paired capacity and database stats. |

TASK-001 owns all REQ-001–012, INV-001–005 and AC-001–011. Its focused proof
recipes distinguish planned additions from existing tests. Integrated review
runs `mise run verify:bifrost`, `mise run test:sql`,
`mise run test:operators:integration`, `mise run codegen:check`, and
`mise run test:principals:integration` once on the candidate, plus affected
language validation. `verify:bifrost` owns format/Clippy/dependency/tenant checks;
do not repeat its components as extra final aggregates. Performance is separate:
`mise run bench:verification:capacity`, `mise run bench:bifrost:write-capacity`,
`mise run bench:bifrost:query-capacity`, with preserved before/after output.

## Approval

No open material decisions. The owner approved revision 2 as a whole on
2026-10-20; TASK-001 is ready for `$wyrd-implement`.

## Revision history and authorities

- Revision 1 (2026-10-20): draft queue split and retention packet, one proposed task.
- Revision 2 (2026-10-20): draft transactional admission, synchronous 422, uniform execution expiry and durable group watermark/baseline; approved by the owner.

Repository authority: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md` (Verifier, Trigger, Operator, Coordination clock),
`architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md` (verification
tables, Durability and visibility, Public surface),
`architecture/references/README.md`,
`architecture/references/doctrine/architecture-constraints.md`,
`architecture/references/architecture/patterns.md`,
`architecture/references/domain/evaluation.md`,
`architecture/references/domain/analytical-operations-reliability.md`, and
`architecture/references/languages/` — `spec-driven-development.md`,
`implementation-execution.md`, `testing-workflows.md`.

Primary technical grounding (checked during drafting):
[Stripe key replay and pruning](https://docs.stripe.com/api/idempotent_requests),
[IETF Idempotency-Key draft-07 §2.7, work in progress, not an RFC](https://www.ietf.org/archive/id/draft-ietf-httpapi-idempotency-key-header-07.html#section-2.7),
[Solid Queue same-database transactional integrity](https://github.com/rails/solid_queue#jobs-and-transactional-integrity),
[River transactional enqueueing](https://riverqueue.com/docs/transactional-enqueueing),
[Oban transactional insertion](https://oban.hexdocs.pm/Oban.html#insert/4),
[PostgreSQL 16 read-committed ON CONFLICT visibility](https://www.postgresql.org/docs/16/transaction-iso.html#XACT-READ-COMMITTED),
and [HOT summarizing-index exception](https://www.postgresql.org/docs/16/storage-hot.html).
These support the selected patterns; they do not establish achieved Wyrd capacity.
