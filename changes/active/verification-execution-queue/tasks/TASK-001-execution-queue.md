---
id: TASK-001
kind: implementation
status: ready
spec: SPEC-verification-execution-queue
spec_revision: 2
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, INV-001, INV-002, INV-003, INV-004, INV-005, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010, AC-011]
depends_on: []
---

# Durably admit executions and bound their coordination history

## Outcome and Value

Run submit returns only after atomic durable enqueue, live attempts rewrite a
narrow queue, and every execution expires 24 hours after settlement without
breaking group continuity or Operator delivery. This single task owns
REQ-001–012, INV-001–005 and AC-001–011, including public and persisted contract,
consumer, journey, architecture and capacity closure.

**Ready.** The owner approved spec revision 2 as a whole on 2026-10-20.
Execute with `$wyrd-implement`; preserve other working-tree changes and recheck
current owners. There are no other tasks or task dependencies.

## Owners, Scope, Consumers, and Prohibited Changes

Keep durable SQL on `VerifierExecutionQueue` in
`crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs`; the caller of a
`TenantConn` owns its transaction. `VerificationControl::enqueue` in
`crates/wyrd/wyrd-server/src/components/verification/service.rs` is the shared
HTTP/MCP request owner and must await commit. Preserve authorization and target
resolution at these owners, with non-blocking audit outside admission atomicity.

The three execution tables and compact group state have the persisted contracts
in spec REQ-008. `VerificationScheduler` owns bounded cleanup; existing group
poller/settlement owns group state; `VerifierRunner`, `ClaimLoop`, `Completions`
and the Scribe sink retain their remaining responsibilities. Remove
`SubmissionSink`/submission outbox admission. Keep the process-local wake signal
useful after committed enqueue without relying on it for recovery.

Public changes: 202 now means durably queued; different-body key reuse is a
synchronous derive-backed 422; submit evidence reaches Bifrost at settlement;
all execution statuses expire after the same horizon. The compact group-state
row is a new persisted contract; no public group-history API is added. SDK
convenience submit still generates a key per call and preserves it on transport
retry; keyed protocol probes inside language journeys need no new SDK option.

No forward migration/backfill/compatibility route, automatic development DB
reset, permanent execution retention, observation admission, generic queue
framework, new maintenance worker, partitioning, client-owned durable logic,
new dependency/Cargo feature, or dispatch-history cleanup. Dispatch changes are
only retention dependency closure. Direct verify and Window sample submit keep
their current behavior, subject only to shared removal of the inert input
activation marker. No Bifrost durability/query/retention redesign.

## Source-backed Reuse Map

The SQL base below is `crates/wyrd/wyrd-sql/src/queries/`; server runtime base is
`crates/wyrd/wyrd-server/src/verification/`. Source describes the current working
tree. Proposed tests in command recipes are additions, not existing symbols.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Admission / key identity | SQL `RequestKey::new`, `RequestKey::execution_id`, `Submission::new`, `VerifierExecutionQueue::accept_submissions`, `KEYED_EXECUTIONS_SQL`; service `VerificationControl::enqueue` | HTTP `components/verification/routes.rs`, server `mcp/verification.rs`, runtime `submissions.rs::SubmissionSink::write`; SQL `pg_verifier_executions` admission/concurrency coverage | Hashed permanent identity, asynchronous conflict execution, acceptance after Scribe ACK | Unique-key transactional insert/replay/422; fresh UUIDv7, commit before 202 | Required key table replaces wide-row key/advisory arbitration; no identity cache, reservation service or generation protocol |
| Executable target | Runtime `cache.rs::VerifierCardCache::resolve`; SQL `verification_target.rs::VerificationTargets` | Submission writer, direct service and runner all use shared resolution | Admission resolution currently runs in writer | Reuse target resolution in request admission; retain runner revalidation | Existing owner handles exact identities/readiness; no second resolver |
| Live attempts and terminal closure | SQL `CLAIM_SQL`, `RENEW_LEASES_SQL`, `COMPLETE_SQL`, `COMPLETE_MANY_SQL`, `RETRY_SQL`, `TERMINATE_SQL`, `RELEASE_SQL`; runtime `ClaimLoop`, `VerifierRunner`, `Completions` | SQL `pg_verifier_executions`, server `pg_verification_runtime` | Wide row rewritten on each attempt; replay writes again | Mutate narrow queue, one terminal record update/delete-live-row closure; retain flush tasks/fallbacks | Required physical split, not a second queue engine |
| Input/result publication | Runtime `results.rs::ResultPayloadBuilder::inputs`/`build`, runner result preparation and `completions.rs::stage`; `scribe_outbox.rs::ScribeWrite` | Shared direct service, runner, Scribe sink and `pg_verification_groups`; `ExecutionInputsTable` schema | Submit currently publishes inputs to activate work; result write has one attribution | Settlement write carries input and result with their distinct original/SYSTEM pairs; no input command/ACK admission | Extend existing result write/ordering owner; no new publisher or evidence log |
| Group continuity | SQL `EXISTING_SNAPSHOT_SQL`, `PREVIOUS_GROUP_SQL`, `CONSUME_GROUP_SQL`, `record_group_activity`, `enqueue_group`, `complete`; runtime `GroupPoller::poll` | Server `pg_verification_groups`, SQL group completion coverage, grouped SDK journeys | Dedupe and previous verdict dereference execution history | Durable consumed watermark at handoff, baseline update/dispatch at settlement | Existing pending is scheduling-only and gets deleted; one small per-binding/group state row is necessary |
| Retention / projections | SQL `STATUS_SQL`, `BINDING_TARGETS_SQL`, `ADVANCE_CURSOR_SQL`; runtime scheduler `cron_loop`, `pass`, `report_depth`; service `get_execution` | SQL execution tests, server runtime/scheduler and SDK handles | No logical expiry or quiet-tenant bounded cleanup | Full BRIN + exact SQL eligibility, bounded scheduler deletion, composed live state and latest retained binding | Existing scheduler/cadence suffice; no partition owner or Bifrost status fallback |
| Dispatch independence | SQL `INSERT_DISPATCH_SQL`; `operator_dispatches.rs::OperatorDispatchQueue` / `CLAIM_SQL`; runtime `OperatorWorker::resolve` | Server `pg_operator_delivery`, SQL dispatch/context tests | Parent FK blocks expiry and claim join supplies inline owner | Freeze owner inside existing JSON, project strict public context, remove FK/join | Necessary dependency closure only; no new table/delivery owner |
| Public contract and journeys | `crates/wyrd-spec/src/error.rs::WyrdError`, service/routes/MCP; shared client `Observe::submit`, verify target `enqueue`, `Execution` | Rust/Python/TS real-time journey files; MCP `verification::pg_tests`; served OpenAPI integration | No new 422, durable-202/expiry/kill/group-state journeys | Source-derived errors/schema descriptions, thin consumer updates, focused real journeys | Existing transports, handles and harness owners suffice |
| Capacity | `capacity/verification/run.rs::Bench`, `report.rs::Report` / `queued_miss`, `workload.rs::Tenant`, `mod.rs` | Existing binary report tests and case-1/2 benchmark; source evidence in long-horizon TASK-004 | Closed-loop only; retries conceal server refusals; old outbox accounting | Mandatory open-loop step, exact count/identity/refusal/drain and normalized SQL evidence | Extend current benchmark/report; no load framework or new task family |

## Approach

1. Confirm revision 2 approval; preserve matching-host before artifacts and
   statement-cost baseline before rerunning a benchmark that overwrites output.
2. Prove fresh-schema split and terminal invariants, including all-table RLS,
   observation removal, immutable record indexes and one terminal update.
3. Replace staging admission with transactional enqueue, unique-key replay/422,
   exact expiry replacement and bounded existing 503; close HTTP/MCP/SDK callers.
4. Stage submit inputs at settlement through the result path, preserve direct
   behavior and attribution, and close group watermark/baseline and dispatch
   dependencies before enabling uniform pruning.
5. Prove scheduler cleanup, public journeys, immediate process-kill survival,
   failure/rollback behavior and all contract/documentation obligations.
6. Extend existing qualification, meet the open-loop/direct/Bifrost controls,
   and record measured evidence with the final architecture updates.

## Ordered Implementation Scenarios

Each block is one behavioral TDD scenario. Commands Q1–Q9, J1–J4 and B1 below
identify planned additions in inspected existing targets. Regression commands
R1/R2 select existing tests. Add the test, confirm nonzero collection, run its
expected RED, implement GREEN, then rerun prior affected scenarios before
refactoring. The implementer owns fixture/private code structure.

### Scenario 1 — Live attempts leave the wide record untouched

**Behavior.** REQ-004/005/008/009, INV-001/002, AC-001/007/008. Fresh schema has
record/live/key split plus group state with forced RLS and correct reloptions.
Runnable shapes reject NULL/malformed authority/inputs. Claims, renew/retry/
release/defer mutate queue only; terminal settlement clears payload and deletes
live state atomically, once. Refused group inserts are strict terminal records.
There are no observation-only columns/indexes/CHECKs or executable observation
origin. Replay reads Applied without UPDATE; stale/mismatched lease changes none.

**RED.** Add Q1 to assert fresh migration/catalog shape, tuple-level record
stability during attempts, terminal clearing and atomic rollback, replay/fence,
renewal/completion lock-order races and group/refused decodability. Current
schema lacks the split and clearing. R1/R2 are regression proofs with no
manufactured failure.

**GREEN.** Implement the selected persisted schema in the existing migrations,
retarget queue mutations and composed projections, and preserve batching,
lease/attempt/fairness mechanics and terminal transaction closure. Prove the
full BRIN and HOT eligibility plus actual HOT updates when page space fits.

**REFACTOR.** Remove obsolete wide-row mutations and observation migration
artifacts. Keep existing SQL/runtime owners, flush counts and fallback; rerun
Q1, R1/R2 and affected owner cases. No duplicate queue abstraction.

### Scenario 2 — A 202 is durable, keyed admission is atomic

**Behavior.** REQ-002/008/009/012, INV-001/002/004, AC-005/007/010. One tenant
transaction admits key/record/queue before 202, keyed or unkeyed. Identical
concurrent requests replay the committed winner; different canonical body
returns 422 synchronously, with no conflict record. Equivalent JCS bodies
replay. The key lives through all non-terminal ages, expires at settlement +
86400 seconds, and concurrent reuse/cleanup creates one fresh UUIDv7 without
removing the new mapping. Bounded pool/DB/unique-wait failures use existing 503.

**RED.** Add Q2 for two-transaction unique-index arbitration, winner rollback,
committed replay, different digest, scope isolation, just-before/at/after expiry,
concurrent expiry cleanup and no half-admission. Add Q3 for real request commit
barriers, unavailable/saturated pool, request cancellation and uncertain response
retry. Current 202 precedes durable enqueue and conflicts settle asynchronously.

**GREEN.** Move admission to the awaited shared service and existing SQL owner;
use ON CONFLICT and fresh winner-read snapshots. Authorized replay remains
valid when mutable target readiness changes; only new work rechecks readiness. Remove hash identity, advisory
key arbitration and request submission outbox. Add new derive-backed 422 and
remove this path's registry 409 while retaining Card registration use. Keep
finite pool acquisition and database waits with registry 503; no submit 429 from
staging and no evidence/Scribe dependency on the request path.

**REFACTOR.** Remove obsolete handoff/boot/shutdown/counter/caller assumptions,
preserving shared target resolution and commit notification. Default to one
request transaction; admission group commit is justified only by target
measurements and must still answer after commit. Rerun Q1–Q3/R1/R2.

### Scenario 3 — Submit evidence publishes at settlement, direct stays intact

**Behavior.** REQ-004/007/009/012, INV-001/004, AC-001/008/011. Before settlement
submit has durable executable inputs but no Bifrost input row. Terminal closure
captures input evidence before clearing, with result when one exists, through
the same settlement write path. Inputs retain the submitting pair and recording
time; results use accepted subject/SYSTEM. Terminal failure/exhaustion has input
evidence without a fabricated verdict. Stale/replayed settlement stages nothing.
Direct verify keeps its current execution/staging/errors and cannot queue work.

**RED.** Add Q4, exercising normal/grouped settlement, explicit termination,
retry exhaustion and exhausted-lease terminal paths with real Scribe reads and
fault controls. Current submit stages inputs before admission; its result write
carries no corresponding input. Force settlement rollback/retry and pre-ACK loss
at the existing outbox boundary; assert no stronger evidence durability claim.

**GREEN.** Extend owned result writes and settlement/fallback paths, reserving
enough existing evidence capacity before effects. Remove marker schema and
Scribe submission callbacks; keep input-before-summary delivery, distinct
attribution and group ACK notifications. Direct inline behavior and Window
sample path remain unchanged. Terminal payload clearing never outruns capture.

**REFACTOR.** Remove activation-as-command and obsolete Scribe admission tests;
replace their assertions with the new boundary tests, not weaker coverage.
Reuse result builder and sink ordering; rerun Q1–Q4 and direct regressions.

### Scenario 4 — Compact group state outlives execution status

**Behavior.** REQ-001/008/009/010/012, INV-001–004, AC-002/003/005/008. Identical
consumed activity does not rerun after its execution is physically pruned. New
snapshot with changed verdict compares against retained group state and
creates one transition dispatch. First/equal verdict emits none. Distinct
snapshots may overlap and settle out of order; settlement order owns baseline,
without regressing the separate admission watermark. Refusal without snapshot
fabricates no consumed identity. Schedule cursor semantics survive pruning.

**RED.** Add Q5 in SQL and Q6 at the real server group owner. Age/physically
prune executions; re-ACK identical source activity and then introduce changed
source evidence. Current dedupe/comparison reads execution rows and loses both.
Cover concurrent handoff, overlapping settlements, replay/rollback, refusal,
source-identity mismatch and foreign tenant/binding/group isolation.

**GREEN.** Use one forced-RLS per-binding/group state row, update consumed
watermark with handoff/pending consumption and baseline with group settlement/
dispatch creation. Use ordinary locking at existing owners; persist only the
small baseline/context identities needed, with no FK to expiring executions.
Keep state through execution cleanup and remove it with its binding lifecycle.

**REFACTOR.** Remove historical execution scans as continuity dependencies;
retain useful in-flight/retained dedupe constraints, not a permanent history
substitute. Rerun Q1–Q6 and existing group/scheduler coverage.

### Scenario 5 — Uniform expiry progresses even without due work

**Behavior.** REQ-001/002/003/005/007/010/011, INV-001–005, AC-002/003/004/007/011.
Every terminal origin expires at the database horizon even with cleanup lag;
status becomes 404 and binding latest-retained may be NULL. No origin exemption.
Scheduler discovers quiet-history tenants, skips locked rows, bounds scans and
transactions, runs without group hosting/free execution permits, and resumes
after cancellation/errors. Live work and missing-settlement records stay safe.

**RED.** Add Q7 with SQL boundary/concurrency/scan assertions and Q8 with real
scheduler passes. Expect absent cleanup and unbounded status visibility today.
Use database-relative aging, disordered settlements, locked oldest row,
concurrent sweep and isolated errors; no day-long sleeps or host stress.

**GREEN.** Extend scheduler cadence and existing SQL owner with finite discovery,
scan/time and delete budgets; use exact eligibility plus full BRIN, TenantConn
and OperatorPool. Delete mapped expired keys consistently without touching new
replacement mappings or group state. Filter public status/latest-retained
before physical deletion. Prove plans, progress, relation size and vacuum signals.

**REFACTOR.** Reuse cancellation/RLS/lock skipping. Rerun Q1–Q8; no generic
maintenance service, B-tree expiry index, partitions or group-history exemption.

### Scenario 6 — Expiring a parent preserves independent Operator work

**Behavior.** REQ-006/009, INV-002/004, AC-006/008. Pending/retrying/running
Card and inline Operator delivery can claim/deliver/settle independently of
parent deletion with frozen identity/context/owner and existing tenant,
credential, retry and deadline behavior. Completion replay adds no reaction.

**RED.** Add Q9 at the SQL owner and D1 at the real server delivery owner.
Current FK blocks deletion; dropping it alone loses claims through the parent
join. Include strict failure-context decode and inline resolution after pruning,
not merely dispatch counts. Use private test aging to exercise deletion while
the dispatch's unchanged production deadline still permits delivery.

**GREEN.** Freeze missing owner UID inside existing JSON storage; project only
existing template fields to the strict decoder, remove parent FK/join and keep
the worker and dispatch columns/indexes otherwise intact.

**REFACTOR.** Remove parent lookups that no longer supply anything. Rerun
Q1–Q9/D1 and relevant dispatch regressions; no dispatch split/retention work.

### Scenario 7 — Every public surface proves durable 202 and the new horizon

**Behavior.** REQ-001/002/007/008/009/010/012, INV-001–004,
AC-003/005/007/010/011. Rust/Python/TypeScript SDK and HTTP/MCP journeys prove
all REQ-012 cases: keyed replay, changed-body 422, concurrent reuse running once,
fresh ID after expiry, status 404, immediate post-202 process-kill survival, and
group dedupe plus next verdict comparison across physical expiry. Old Bifrost
evidence stays attributable to old ID. Permission denial/tenant isolation and
direct verify/Window behavior remain. New 422/state contracts have actual
catalog/OpenAPI/MCP proof, not only prose.

**RED.** Add J1–J4 in existing journey targets. Each language journey drives its
real SDK, keyed protocol requests in that language and actual execution/Bifrost
readback; J4 discovers/acts/observes through MCP. Pause claim/evaluation through
private harness controls so the acknowledged execution is still pending at the
immediate kill, retain the same database, kill the server process without
graceful drain, restart it, then observe execution completion under original ID.
Current in-memory staging loses that work. A graceful restart or in-process
shutdown is insufficient crash proof. Add the smallest process-control support
under existing `wyrd-testing` owners and language test support if needed; no
production kill/age route or new harness framework.

**GREEN.** Close service/routes/MCP, shared client/error descriptions, generated
sources and language wrappers/stubs as actually affected. Regenerate from source;
verify served 202/422/503/404 contracts and runtime MCP. Complete REQ-007/012
architecture rewrites, group state/visibility/retention and vacuum recipe.

**REFACTOR.** Keep clients thin and aging/process controls test-only. Rerun
J1–J4 and preceding owner cases; do not add an SDK key API, polling workaround,
client durable state or history fallback to conceal wrong server semantics.

### Scenario 8 — Sustained capacity is measured without retry-hidden refusals

**Behavior.** REQ-008/011/012, INV-005, AC-008/009/011. Exactly 360,000 scheduled
arrivals at 6,000/s for 60 seconds, after warmup and at 64 slots, have zero
client/server errors/refusals, sustain terminal rate and drain queue/Scribe
within 60 seconds. Existing case-1 p99 <1 ms and case-2 steps pass. Per-row
claim/settle statement cost decreases. Bifrost controls pass within spec tolerance.

**RED.** Add B1 under existing report tests: otherwise successful SDK retries
cannot pass with a server refusal (including DB/pool 503), count mismatch,
late/missed arrival, driver saturation or drain failure. Add a deterministic
pacing assertion in this focused proof; current closed-loop accounting lacks
guaranteed-rate judgment. Do not generate load as a unit-test substitute.

**GREEN.** Extend Bench/Report and existing case 2 with mandatory fixed-arrival
step, 10-second terminal/backlog slices and identity/verdict reconciliation.
Replace removed submission-outbox metrics with real admission/error evidence;
record warmup-through-drain logs/counter deltas including hidden 503s and Scribe
pressure. Measure admission/claim/settle costs and actual HOT/index/vacuum stats,
preserve baselines, then run sequential matching-host qualification controls.

**REFACTOR.** Reuse fixture, metrics/traces and report. Rerun B1 and benchmark
commands. No lower offered rate/criteria, extra slots, concurrent host load,
missing-baseline improvement claim or noisy-evidence PASS.

## Acceptance Criteria

| Spec criteria | Task proof / accountable owners |
|---|---|
| AC-001 / AC-008 | Scenarios 1/3: SQL schema/lifecycle, runner/Completions/Scribe evidence closure; Q1/Q4/R1/R2 and measured tuple/index proof. |
| AC-002 / AC-004 | Scenarios 2/5: SQL key/expiry and scheduler quiet-tenant bounded progress; Q2/Q7/Q8. |
| AC-003 | Scenario 4/7: SQL/group owners and every journey retain dedupe/comparison across physical expiry; Q5/Q6/J1–J4. |
| AC-005 / AC-010 | Scenarios 2/7: shared service, HTTP/MCP, SDK runtime owners; Q2/Q3/J1–J4, including actual process kill/restart. |
| AC-006 | Scenario 6: SQL and Operator worker; Q9/D1. |
| AC-007 | Scenarios 1/2/7 and static proof: new state/422, migrations, generated artifacts, served OpenAPI, MCP, consumer typing, both architecture docs. |
| AC-009 | Scenario 8: benchmark owner; B1 and preserved matching-host capacity/SQL cost evidence. |
| AC-011 | Scenarios 3/5/8: evidence owner, scheduler and benchmark; Q4/Q7/Q8, vacuum recipe/stats, integrated Bifrost correctness and paired controls. |

REQ-001: scenarios 4/5/7; REQ-002: 2/5/7; REQ-003: 5;
REQ-004: 1/3; REQ-005: 1/5; REQ-006: 6; REQ-007: 3/5/7;
REQ-008: 1/2/4/8; REQ-009: 1/2/3/4/6/7; REQ-010: 4/5/7;
REQ-011: 5/8; REQ-012: 2/3/4/7/8. Every INV-001–005 applies to
its mapped scenario and final diff inspection; none is waived by lower-tier tests.

## Expected Write Set and Consumer Closure

Paths are guidance, not a private helper/file allowlist:

- The seven existing migrations enumerated in the spec, SQL execution and
  dispatch owners and their integration/in-module plan tests. Follow
  `crates/wyrd/wyrd-sql/src/postgres.rs::WyrdPostgres::validate_schema` and
  `crates/wyrd/wyrd-sql/src/schema_check.rs` for catalog-driven forced-RLS proof
  on all tables, including group state. Do not widen a checker allowlist.
- Server `components/verification/service.rs`, `routes.rs`, `mcp/verification.rs`,
  runtime `verification/` owners, Scribe outbox, boot/state/shutdown callers and
  metrics references to removed submission outbox. Keep target/cache/readiness
  ownership shared with direct and runner. No unrelated cache redesign.
- `crates/vala/vala-bifrost-redux/src/tables/verification/execution_inputs.rs`
  and its actual schema consumers: remove marker and activation behavior;
  preserve evidence values, sensitivity, correlation and source-driven schema
  registration. No Bifrost engine redesign.
- `crates/wyrd-spec/src/error.rs`, affected request/response schema descriptions,
  served OpenAPI registrations and MCP descriptions/errors; source-driven
  generated error/type/example outputs, shared `wyrd-client` observe/verify
  consumers and first-class SDK wrapper/stub/docs as affected. No hand-edited
  generated artifacts or parallel error mapping.
- Existing SQL/server execution/group/route/Operator integration targets;
  `crates/wyrd/wyrd-testing/src/verification.rs` (`VerificationFixture`) and
  existing real-server/process test support. Align fixture SQL/projections to
  split tables; test-only controls must not ship in the production wheel/addon.
- `sdks/wyrd-sdk-rust/tests/integration/verify_in_real_time.rs`;
  `sdks/wyrd-sdk-python/tests/integration/test_verify_in_real_time.py`;
  `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`;
  `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/verification.rs` and their existing
  private language test support. Crash proof may extend existing process
  hosting at `crates/wyrd/wyrd-testing/src/release_server.rs`; no new public
  lifecycle surface is authorized.
- `crates/wyrd/wyrd-testing/src/bin/capacity/verification/` (`mod.rs`, `run.rs`,
  `report.rs`, `workload.rs`) and existing benchmark task description/selection
  in `mise.toml` as necessary; no new task family.
- `architecture/wyrd-design.md` and `architecture/bifrost-design.md` per
  REQ-007/012, during implementation only. `scripts/check_tenant_isolation.py`
  is inspected enforcement, not permission to weaken boundaries.

## Verification and Evidence

The following are **implementation-time recipes, not drafting results**.
Planned new test names below do not exist yet. Package/target/module homes are
verified from current manifests and test roots; after addition confirm exact
collection and nonzero selected count, then record RED/GREEN/prior reruns.
On failure rerun with `WYRD_LOG=info,wyrd_server=debug,wyrd_sql=debug` before
diagnosis. All Cargo-backed execution is sequential. Use database-relative
aging and deterministic barriers, not 24-hour sleeps or synthetic host load.

### Focused SQL and server proof

Every command includes the repository-managed Postgres wrapper and fresh
migration smoke task. R1/R2 are existing regression tests; all Q/D additions
extend the confirmed `integration` binary and module named in their selector.

Q1 — `pg_verifier_executions::execution_queue_lifecycle`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::execution_queue_lifecycle)"'
```

Q2 — `pg_verifier_executions::transactional_keys_replay_conflict_and_expire`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::transactional_keys_replay_conflict_and_expire)"'
```

Q3 — `pg_verification_runtime::submit_waits_for_durable_admission`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::submit_waits_for_durable_admission)"'
```

Q4 — `pg_verification_runtime::submit_inputs_publish_at_settlement`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::submit_inputs_publish_at_settlement)"'
```

Q5 — `pg_verifier_executions::group_state_survives_execution_expiry`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::group_state_survives_execution_expiry)"'
```

Q6 — `pg_verification_groups::group_state_preserves_dedupe_and_verdict_after_expiry`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_groups::group_state_preserves_dedupe_and_verdict_after_expiry)"'
```

Q7 — `pg_verifier_executions::retention_is_bounded_tenant_safe_and_preserves_dedupe`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::retention_is_bounded_tenant_safe_and_preserves_dedupe)"'
```

Q8 — `pg_verification_runtime::scheduler_reaps_terminal_history_without_due_work`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_verification_runtime::scheduler_reaps_terminal_history_without_due_work)"'
```

Q9 — `pg_verifier_executions::dispatches_survive_execution_expiry`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::dispatches_survive_execution_expiry)"'
```

D1 — `pg_operator_delivery::delivery_survives_execution_expiry`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(=pg_operator_delivery::delivery_survives_execution_expiry)"'
```

R1 — `pg_verifier_executions::a_claim_takes_up_to_its_limit_in_one_statement`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::a_claim_takes_up_to_its_limit_in_one_statement)"'
```

R2 — `pg_verifier_executions::a_batch_completes_many_executions_like_complete`:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(=pg_verifier_executions::a_batch_completes_many_executions_like_complete)"'
```


### Focused public journeys

Build changed runtime artifacts once before these commands:

```bash
mise run build:server
mise run py:setup
mise run ts:build
mise run ts:build:testing
```

J1 — planned Rust SDK journey in the current `wyrd-ai` integration target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E "test(=verify_in_real_time::execution_queue_retention_horizon)"'
```

J2 — planned Python journey in the current integration file:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-python && mise exec -- uv run python -m pytest -q -m integration tests/integration/test_verify_in_real_time.py::test_execution_queue_retention_horizon'
```

J3 — planned TypeScript journey in the current Vitest integration file:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cd sdks/wyrd-sdk-ts/wyrd && mise exec -- pnpm exec vitest run tests/integration/verify-in-real-time.test.ts -t "^execution queue retention horizon$"'
```

J4 — planned runtime MCP discover/act/observe journey in the current `mcp` target:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E "test(=verification::pg_tests::execution_queue_retention_horizon)"'
```

B1 — planned deterministic guaranteed-rate accounting/pacing proof in the
current capacity binary report module (no Postgres):

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity -E 'test(=verification::report::tests::guaranteed_rate_rejects_hidden_refusals)'
```

Each J1–J4 must prove the full REQ-012 matrix, including hard-kill/restart and
group continuity, through its own public surface. Existing in-process fixture
restart methods are not proof of abrupt process death. Reuse the repository's
real process hosting and retained database support; add only necessary private
controls, absent from production packages. HTTP keyed probes supplement actual
SDK submit/handle/readback, never replace it. MCP also asserts discoverable
write scopes, catalog descriptions and the structured 422 problem.

### Owner regression and static proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --test integration -E "test(/^pg_verifier_executions::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-server --features test-support --test integration -E "test(/^(pg_verification_runtime|pg_verification_groups|pg_verification_routes|pg_operator_delivery)::/)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sql --lib -E "test(/^queries::verifier_executions::pg_tests::/)"'
mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
mise run codegen:check
mise run test:principals:integration
mise run py:format
mise run py:lints
mise run py:test:unit
mise run ts:format
mise run ts:lints
mise run ts:typecheck
git diff --check
```

Run `mise run py:typecheck` when Python authored stubs/exports/public typing
change; `mise run ts:napi:check` when native declaration sources change. Regenerate
from sources before drift checks. Served OpenAPI and runtime MCP are mandatory
because the 422 and 202 meanings change; codegen alone proves neither.

Architecture prose, migration edit-in-place closure, removal of observation-only
artifacts and dependency inventories use static/diff proof, not a manufactured
RED. Runtime fresh-schema/RLS/index/reloptions assertions and public contract
journeys prove executable effects. No docs-site lane is needed for architecture
Markdown alone. Newly added named tests beyond these recipes require their own
exact collected commands in execution evidence.

### Capacity qualification

Save output directories before a subsequent run overwrites them. Run separately
on matching hosts, before and after, with no concurrent benchmark load:

```bash
mise run bench:verification:capacity
mise run bench:bifrost:write-capacity
mise run bench:bifrost:query-capacity
```

The verification task currently defaults to cases 1/2 with closed-loop
1/4/16/64-client windows. Implement the mandatory new open-loop step in case 2;
there is no existing open-loop CLI switch to cite. Retain reports, samples,
server logs, admission/refusal deltas, arrival lag, terminal identities/verdicts,
10-second throughput/backlog slices, drain, normalized claim/settle SQL cost,
and catalog/HOT/heap/index/vacuum evidence. Preserve direct p99 and paired
Bifrost thresholds exactly as specified. The removed submission-outbox series
must not conceal server refusals. Missing baseline or noisy controls are
inconclusive, never a measured improvement/PASS.

Integrated change review runs the spec's full capability/language journey
aggregates once on the candidate, including Bifrost correctness. This task
prescribes only its focused new journeys and owner regressions; it does not
repeat the repository-wide gate or every language journey suite.

## Material Stop Conditions

- No execution before explicit human approval of revision 2 as a whole.
  Changes to transactional 202, key scope/horizon, synchronous 422, group-state
  semantics, evidence visibility/durability, uniform expiry or targets require
  a semantic spec revision and renewed approval.
- If an independently required pre-execution Bifrost invariant is discovered,
  cite its authority and return for a standard transactional-outbox revision;
  never restore the in-memory pre-admission path. Current inspected authority
  permits the selected settlement publication once REQ-007 is rewritten.
- If BRIN/fillfactor cannot support bounded cleanup and terminal HOT eligibility
  under disorder/load, report the measured conflict; do not replace with a
  mutable B-tree, partition movement, unbounded scans or relaxed horizon.
- If another reader/persisted dependency needs cleared payload or retained
  execution history, finish its trace and report the material conflict instead
  of adding exemptions/history tables or cascading dispatch deletion.
- No capacity PASS without zero hidden refusals, original rate/slots/fixture,
  reduced measured per-row claim/settle costs and paired Bifrost controls.
- Keep exactly this one task. Reversible private code, fixtures and measured
  admission group commit remain `$wyrd-implement` choices inside the approved
  contracts; no new lifecycle owner or task packet.

## Authority Links

[Specification revision 2 — draft, awaiting explicit approval](../spec.md).
Repository authorities: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`,
`architecture/bifrost-design.md`, `architecture/references/README.md`, and the
focused references in the spec, particularly
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/implementation-execution.md` and
`architecture/references/languages/testing-workflows.md`.
Measured diagnostic context:
`changes/active/long-horizon-verification/tasks/TASK-004-capacity-qualification.md`,
“Settlement group commit and runner throughput”. Source evidence is not proof
that this proposed implementation meets capacity. Execution uses `$wyrd-implement`.
