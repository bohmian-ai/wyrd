---
id: BIFROST-R6-T04-REMEDIATION-02-EXACT-BINDINGS-AND-ATTEMPT-EVIDENCE
title: Make repeated-source bindings exact and close immutable-attempt evidence
kind: remediation
mode: REMEDIATE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 6
depends_on: [BIFROST-R6-T04-REMEDIATION-SINGLE-PLANNER-QUERY-RELIABILITY]
requirements: [REQ-002, REQ-003, REQ-007, REQ-011]
invariants: [INV-002, INV-003, INV-006]
acceptance: [AC-002, AC-003, AC-004, AC-007, AC-008]
parent_task: BIFROST-R6-T04-REMEDIATION-SINGLE-PLANNER-QUERY-RELIABILITY
reviewed_candidate: a6baa2f6712058e21be2685c32d69e1e79b5f4b1
planning_base: b96252b0e68a0e0999e95322ec32cf9adb188565
remediates: [FIND-04R-1, FIND-04R-2, FIND-04R-3, FIND-04R-4]
---

# Exact source bindings and immutable-attempt evidence remediation

## Outcome and value

Every physical occurrence of a remote table scan receives a unique
request-local identity and is bound to exactly the destination, tenant/table
binding, tier, schema, projection closure, and predicates planned for that
occurrence. A same-table self-join or repeated CTE therefore cannot overwrite
one occurrence's assignment with another's.

Process helpers fail closed when their requested graph does not settle; they
never return a predecessor graph's physical evidence. The existing public
Oracle journeys prove planning failure, missing pinned data, and selected peer
loss against one immutable cut and one physical build, with structured SDK
errors and no rerun.

Required execution skill: `$wyrd-implement`.

This is the only remediation task for the validated Task 04 review ledger. All
four findings land and verify together. Do not split them, create another task,
or defer one finding into follow-up work.

## Validated finding ledger

The Task 04 review findings were independently validated by the Ponytail
auditor. Its validated recommendations are binding inputs to this task:

- `FIND-04R-1` — `persisted_follower_scan_id(table, tier)` collides when one
  physical plan contains repeated occurrences of the same table and tier.
  `HashMap::insert` silently replaces the earlier assignment, while binding
  validation compares too little authority. Use a unique request-local
  occurrence identity, reject duplicates with `HashMap::entry`, and validate
  the exact planned and bound authority. Prove a same-table self-join or
  repeated CTE plus focused duplicate and mismatch refusals.
- `FIND-04R-2` — `execute_analytical_baseline` can exhaust its settlement wait
  and return retained evidence from the preceding graph. Require the settled
  counter to advance or return the existing `ProcessClusterError`; read
  physical evidence only after that guard passes.
- `FIND-04R-3` — the planning-refusal and missing-object phases accept any
  `Err` and do not observe one build or the immutable cut. Reuse
  `is_transport`, `ValaSdkError::code`, and the existing metric helpers, and
  expose only the narrow test-support build/cut observation the current
  private process protocol lacks.
- `FIND-04R-4` — selected peer loss proves one unsuccessful attempt and one
  survivor activation only indirectly. Observe the active query's existing
  participant-cut fingerprint before killing the peer and prove that the
  terminal attempt retained it without another build.

The auditor did not validate the optional missing-Analytical-handle
fall-through as a current production defect. It is not part of this task.

## Owners, scope, and non-goals

Primary owners:

- `vala-bifrost-redux::oracle::{exec, codec, bindings, Oracle}` owns unique
  planned scan occurrences and exact post-admission binding.
- `OracleQueryAttemptRoster`, `OracleQueryAttemptCut`, and
  `RunningQueryRegistry` already own the immutable attempt identity and cut
  used by the narrow test-support evidence.
- `wyrd-testing::bifrost::process_cluster` and its child own the existing
  private test protocol and bounded process waits.
- `wyrd-testing`'s existing `oracle` journey target owns the public regression
  proofs.

Expected write set is limited to:

- `crates/vala/vala-bifrost-redux/src/oracle/{mod.rs,exec.rs,codec.rs,bindings.rs,participant_cut.rs,running.rs}` as required by the concrete implementation;
- `crates/wyrd/wyrd-testing/src/bifrost/process_cluster.rs` and
  `crates/wyrd/wyrd-testing/src/bifrost/process_cluster/child.rs` for private
  process evidence and the stale-evidence guard; and
- the existing Oracle unit/integration tests and
  `crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_activation.rs`; and
- `tasks/05-pre-mcp-buildout.md`, whose prerequisite advances from the first
  Task 04 remediation to this final Task 04 remediation.

Use fewer files when the existing owners allow it. Do not introduce a trait,
registry, generic observer, new crate/module, dependency, configuration knob,
public endpoint, public SDK field, protobuf field, descriptor change, or
production metric label/family. Do not revive query ordinals: one additional
build event is itself the observable second ordinal.

Preserve the single root, single admission, no retry, no fallback, immutable
deadline, tenant isolation, Interactive floor, and joined cleanup delivered by
the parent remediation. Do not change routing policy or supported SQL merely
to make the regression pass.

## Ordered implementation scenarios

### Scenario 1 — Repeated physical scans bind independently and exactly

**Behavior.** Each call to `OracleTableProvider::scan` allocates one unique
request-local occurrence number. Each persisted tier produced by that scan
derives its scan ID from the canonical table, tier, and occurrence. The
placeholder retains enough existing source authority to bind directly to the
one pinned cut and tier it represents; the binder never recovers authority by
parsing or recomputing the scan ID. Every planned occurrence has exactly one
matching assignment, no extra assignment exists, and a duplicate or mismatched
assignment fails before row IO. Maps REQ-002, REQ-003; INV-002, INV-003;
AC-002, AC-003, AC-008.

**RED.** Extend the existing
`oracle::exec::tests::retained_plan_uses_admitted_task_context_only`; do not add
a parallel binding-fixture test. Plan two remote occurrences of the same table
and persisted tier with different aliases and different projection/predicate
closures. Assert their source keys and scan IDs differ and both resolve their
own exact assignment. Also split one occurrence into multiple
`DistributedLeafExec` task variants and assert those variants legitimately
share that occurrence's key while narrowing disjoint file shares. Mutate one
fact at a time and assert binding fails before the fixture's row-IO counter
changes:

1. a duplicate canonical occurrence or assignment insertion, distinct from
   the expected task variants of one occurrence;
2. destination identity, role, fence, or endpoint;
3. tenant/table binding;
4. persisted tier;
5. schema fingerprint; and
6. required-column or predicate closure.

The pre-change candidate must fail because the two occurrences produce the
same scan ID or because one of the mismatches is accepted.

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::retained_plan_uses_admitted_task_context_only)'
```

Extend the existing public
`analytical_activation::single_planner_root_selects_path_and_capacity` journey
with one same-table self-join using distinct aliases, filters, and projections.
Assert exact rows, an Analytical terminal, positive follower work, and the
existing zero-ownership baseline. This is the user-visible regression; a plan
string or source-key unit assertion is not a substitute.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::single_planner_root_selects_path_and_capacity)' --run-ignored=all"
```

**GREEN.** Make the smallest root-cause change:

1. Add one `AtomicU64` occurrence counter to `OracleTableProvider`, initialized
   with the provider. Allocate exactly once per `scan` with a checked atomic
   increment; exhaustion is a planning failure, never wraparound. `Relaxed`
   ordering is sufficient because uniqueness, not memory publication, is the
   invariant.
2. Include the occurrence in `persisted_follower_scan_id`. The Iceberg and hot
   tiers from one occurrence remain distinct through their existing tier tag.
3. Expand the existing `OracleSourceKey::Follower` into the full planned key:
   scan ID, destination, canonical tenant/table binding, persisted tier, schema
   fingerprint, required columns, and predicates. Make
   `RemoteSourcePlaceholderExec` retain or derive that one value instead of
   keeping a second destination-bearing descriptor. Derive or implement the
   exact equality/hash needed to use the full key in the existing map.
4. In `bind_execution_sources`, select the pinned cut by the placeholder's
   exact table binding and tier, not by searching for a recomputed scan ID.
   Canonicalize the original placeholder and its `DistributedLeafExec` task
   variants to one occurrence key only after verifying all immutable key facts
   agree. The variants share that occurrence's assignment and continue to
   narrow only their file shares. Two distinct canonical occurrences never
   collapse.
5. Key `follower_assignments` by the full `OracleSourceKey`, not `String`, and
   resolve assignments by that full key. Insert the canonical occurrence with
   `HashMap::entry`; an occupied insertion from a distinct canonical occurrence
   returns `BifrostError::QueryExecutionFailed` and cannot replace the first
   value.
6. In `OracleExecutionBindings::try_new`, require exact canonical
   planned/bound cardinality. Validate the assignment value against its full
   key: scan ID, tenant/table binding, tier/source identity, schema fingerprint,
   required columns, and predicates. Destination equality is enforced by the
   full map key. A mismatch is the existing execution failure and occurs before
   any leaf opens row IO.

**REFACTOR.** Keep `RemoteSourcePlaceholderExec` and `OracleSourceKey` as the
single planned authorities. Remove the old table/tier scan-ID reverse lookup
and any comparison made redundant by the exact validation. Do not introduce a
binding service or another map.

### Scenario 2 — A baseline reports only its own settled physical evidence

**Behavior.** `execute_analytical_baseline` returns physical evidence only when
`AnalyticalSupervisor::settled_graph_count()` advances beyond the value
captured before its query. If the existing bounded wait expires first, it
returns `ProcessClusterError::Child` and never reads or returns retained
evidence from an earlier graph. Maps REQ-007; INV-003; AC-004, AC-008.

**RED.** Add one focused child-module unit test named
`analytical_baseline_rejects_predecessor_evidence_without_new_settlement`.
Drive the final settlement decision with equal before/after counters and a
nonempty predecessor evidence value. Assert the existing child error is
returned. Also assert an advanced counter admits the current evidence. The RED
must fail because the current timeout path does not distinguish those states.

```bash
mise exec -- cargo nextest run --locked -p wyrd-testing --lib -E 'test(=bifrost::process_cluster::child::tests::analytical_baseline_rejects_predecessor_evidence_without_new_settlement)'
```

**GREEN.** After the current bounded polling loop, read the settlement counter
once. If it did not advance, return a non-secret `ProcessClusterError::Child`
that names the analytical baseline settlement timeout. Only then call
`settled_physical_evidence`. Put the comparison in one small private,
synchronous helper only if that is needed to test the branch; do not add a
clock abstraction, fault injector, retry, or new timeout setting.

**REFACTOR.** Keep the counter as the sole freshness authority. Do not clear
global evidence before a query or infer freshness from whether an optional
sort-evidence value is present.

### Scenario 3 — Planning and stale-source failures prove one immutable build

**Behavior.** The public planning-refusal and missing-pinned-object phases each
return the exact structured query-execution failure, use one immutable
membership/source cut, enter the shared physical builder exactly once, and
leave no query, graph, reservation, memory, or scratch ownership. No second
build event means no successor ordinal, repin, replan, or fallback. Maps
REQ-002, REQ-007, REQ-011; INV-002, INV-003, INV-006; AC-003, AC-004, AC-007,
AC-008.

**RED.** Strengthen, rather than duplicate,
`analytical_activation::single_planner_root_selects_path_and_capacity`:

- capture the coordinator's physical-build/cut observation immediately before
  and after each failure phase;
- require a build-total delta of exactly one and one nonempty cut fingerprint;
- require `ValaSdkError::code()` to equal
  `WYRD_VALA_500_QUERY_EXECUTION_FAILED` and `is_transport(&error)` to be
  false;
- retain the existing production telemetry and zero-ownership comparisons;
  and
- prove the next failure phase has a different request-local cut fingerprint,
  so the evidence cannot be a stale predecessor observation.

The pre-change candidate must fail because the private process protocol exposes
neither physical-build count nor cut identity and the journey accepts any
error.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::single_planner_root_selects_path_and_capacity)' --run-ignored=all"
```

**GREEN.** Add one narrow, test-support-only observation at the shared
`build_physical_root` convergence point:

1. Reuse the existing `OracleQueryAttemptCut::fingerprint` canonical encoding
   for the class-neutral roster; do not create a second hash format. The
   fingerprint already excludes query class, so the pre-build roster and the
   finalized participant cut must agree.
2. Immediately before the physical builder can fail, record that fingerprint
   and increment one process-local `AtomicU64` build total. All production and
   inactive test entry points must pass through this same counter; do not
   increment in callers or journeys.
3. Expose only `{ total, latest_cut_fingerprint }` behind `test-support` through
   the existing JSON process control protocol. The journey differences totals
   before/after its serialized query. Do not add request SQL, tenant/table
   identity, an unbounded event list, a public server surface, or a production
   metric label.

**REFACTOR.** Share the fingerprint encoder between roster and cut. Keep the
observation with current Oracle test state; do not create a general telemetry
owner.

### Scenario 4 — Selected peer loss retains the original participant cut

**Behavior.** While the selected follower is paused, the coordinator's active
`RunningQueryEntry` exposes the exact participant-cut fingerprint already owned
by the production registry. Killing that follower yields one structured
terminal failure, one physical build, the same cut fingerprint, one
unsuccessful attempt, no replacement activation, and complete cleanup. Maps
REQ-002, REQ-007, REQ-011; INV-002, INV-003, INV-006; AC-003, AC-004, AC-007,
AC-008.

**RED.** Strengthen the existing
`analytical_activation::selected_peer_failure_is_terminal` journey. After
`await_execute_paused` and before `kill`, read the coordinator's sole active
query cut fingerprint through the private process protocol. Compare it with
the builder's latest cut fingerprint. After terminal failure, require the
build-total delta to remain exactly one and retain the existing assertions for
structured non-transport error, attempt totals, survivor activation, and zero
ownership.

The pre-change candidate must fail because the process protocol cannot project
the active registry's cut fingerprint.

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=analytical_activation::selected_peer_failure_is_terminal)' --run-ignored=all"
```

**GREEN.** Extend the Scenario 3 private observation response with the active
cut fingerprints already held by `RunningQueryRegistry`. In the child, list
the current tenant's active query summaries, resolve their existing entries,
and return their `participant_cut().fingerprint()` values in deterministic
request-ID order. The journey requires exactly one active value at the pause.
Do not retain a second cut copy, add production state, or expose this through a
public query API.

**REFACTOR.** Use one control request/response and one parent helper for both
build and active-cut evidence. Reuse `RunningQueryRegistry::list`, `get`, and
`RunningQueryEntry::participant_cut`; add no parallel registry traversal API if
those calls suffice.

## Implementation and commit protocol

Implement scenarios in order through RED, GREEN, and REFACTOR. Record each
scenario's pre-change failure, focused passing command, and commit in this task
artifact. Scenario 1 may change production behavior before the evidence-only
scenarios; do not weaken an assertion to accommodate an intermediate commit.

If implementation discovers a behavior-changing conflict with the approved
specification, stop for `$wyrd-spec`. Ordinary code-shape choices remain in
this task and do not justify another task.

## Focused verification

Run the exact named commands in each scenario. Then run only the new canonical
Bifrost scopes that cover this write set:

```bash
mise run check:bifrost
mise run test:bifrost:integration:redux
mise run test:bifrost:journey:oracle
git diff --check
```

`test:bifrost:journey:oracle` is the whole Oracle journey binary and is the
only journey lane required here. Do not run `mise run test:e2e`, `mise run
gate`, `mise run test:bifrost`, `mise run test:bifrost:journey`, or `mise run
verify:bifrost`; those aggregate unrelated journey or language surfaces.

No codegen, Python, TypeScript, MCP, or protobuf lane is required because this
task changes no public contract or language projection. If the implementation
does change one, that exceeds this plan's scope and requires review rather than
silently broadening verification.

## Completion evidence

The implementation closeout must provide:

- one commit sequence covering all four finding IDs;
- the RED and GREEN results for both exact named unit tests;
- passing `check:bifrost`, `test:bifrost:integration:redux`, and
  `test:bifrost:journey:oracle` results;
- the same-table self-join's exact result and terminal path;
- the structured code, build delta, and cut fingerprints for planning refusal,
  missing pinned data, and selected peer loss;
- zero retained ownership after every public phase; and
- `git diff --check` output plus confirmation that no public contract,
  dependency, fallback, retry, or second remediation task was added; and
- confirmation that `05-pre-mcp-buildout.md` depends on this remediation task,
  so MCP work cannot begin while these Task 04 findings remain open.

## Implementation evidence

Commit sequence (all four finding IDs, in scenario order):

- `1a4dd587d` fix(bifrost): bind each remote scan occurrence exactly (FIND-04R-1)
- `ea47f2367` test(bifrost): prove a same-table self-join binds each occurrence (FIND-04R-1)
- `51194baf8` fix(bifrost): require new settlement before reading analytical evidence (FIND-04R-2)
- `e107ae87c` test(bifrost): prove each failure phase enters one build against one cut (FIND-04R-3)
- `9981f30de` test(bifrost): prove selected peer loss retains its one built cut (FIND-04R-4)

### Scenario 1 — exact per-occurrence bindings

RED: the extended `retained_plan_uses_admitted_task_context_only` failed because
both occurrences of one table/tier minted the same scan ID, so the second
assignment replaced the first in `follower_assignments`.

GREEN: `OracleTableProvider` allocates one request-local occurrence per `scan`
with a checked `AtomicU64` increment (exhaustion is a planning failure);
`persisted_follower_scan_id(table, tier, occurrence)` mints the ID;
`OracleSourceKey::Follower(Box<FollowerSourceKey>)` carries scan ID,
destination, tenant, canonical table, tier, schema fingerprint, required
columns, and predicates; `RemoteSourcePlaceholderExec` retains a
`PlannedRemoteSource` and derives its key at call time, so `DistributedLeafExec`
task variants derive the identical key; `bind_execution_sources` selects the
pinned cut by the placeholder's exact tenant/table and inserts through
`HashMap::entry` (an occupied slot with a differing assignment is
`QueryExecutionFailed`); `try_new` requires exact planned/bound follower
cardinality and validates each assignment against its full key.
`OracleSourceKey` implements `Hash` manually (discriminant plus table/scan ID)
because `DispatchCandidate` and `ScanPredicate` are not `Hash`; equal keys hash
equally, and the scan ID already separates every distinct key in one plan.

Refusal sweep, one mutated fact at a time, each asserted to fail before the
fixture's row-IO counter moves and with `pool.reserved() == 0`: duplicate
canonical occurrence, swapped assignment, destination node/role/fence/endpoint,
tenant, table, tier, schema fingerprint, required columns, predicates.

```
PASS vala-bifrost-redux oracle::exec::tests::retained_plan_uses_admitted_task_context_only
PASS wyrd-testing::oracle analytical_activation::single_planner_root_selects_path_and_capacity
```

Journey self-join: `SELECT l.id, r.wyrd_row_ordinal FROM <t> l JOIN <t> r ON
l.filter_key = r.filter_key WHERE l.filter_key = 'group_0' AND r.id > 5`
settles `QueryExecutionPath::Analytical` with exactly 8 rows, both followers'
peer body polls increase, and every Oracle pod returns to its ownership
baseline.

Command correction: the process fixture writes only `id` and `filter_key`, so
the self-join projects the managed `wyrd_row_ordinal` rather than the
in-process fixture's `unused_payload`.

### Scenario 2 — a baseline reports only its own settled evidence

RED: `analytical_baseline_rejects_predecessor_evidence_without_new_settlement`
did not compile against `super::settled_analytical_evidence`, because the
timeout path read retained evidence without distinguishing the two states.

GREEN: one private synchronous helper `settled_analytical_evidence(before,
after, evidence)` returns `ProcessClusterError::Child` naming the analytical
baseline settlement wait when the counter did not advance;
`execute_analytical_baseline` reads `settled_physical_evidence()` only through
it. No clock abstraction, fault injector, retry, or timeout setting was added.

```
PASS wyrd-testing bifrost::process_cluster::child::tests::analytical_baseline_rejects_predecessor_evidence_without_new_settlement
```

### Scenario 3 — failure phases prove one immutable build

RED: the journey accepted any `Err` and the private protocol exposed neither a
build count nor cut identity.

GREEN: `canonical_fingerprint` was extracted from
`OracleQueryAttemptCut::fingerprint` and is now shared with the new
`OracleQueryAttemptRoster::fingerprint`, so the pre-build roster and finalized
cut agree. `build_physical_root` takes the roster and, before anything in the
build can fail, calls the `test-support` `record_physical_build`, which
increments one process-local `AtomicU64` and stores the latest digest;
`physical_build_observation_for_test()` reads the pair. Both entry points
(production classification and the inactive attempt lease) pass through that
one call. `ControlRequest::PhysicalBuildEvidence` /
`ControlResponse::PhysicalBuilds(PhysicalBuildEvidence)` expose `{ total,
latest_cut_fingerprint, active_cut_fingerprints }` through the existing private
JSON protocol; the parent helper is `ProcessNode::physical_build_evidence`.

The journey's `expect_single_build_failure` helper drives both failure phases
and requires: no settlement, `ValaSdkError::code() ==
"WYRD_VALA_500_QUERY_EXECUTION_FAILED"`, `is_transport(&error) == false`, a
build-total delta of exactly one, and a nonempty cut fingerprint. The two
phases' fingerprints are then required to differ, so neither can be a stale
predecessor observation. The existing production telemetry and zero-ownership
comparisons are retained unchanged.

### Scenario 4 — selected peer loss retains the original cut

RED: the process protocol could not project the active registry's cut
fingerprint.

GREEN: the same control response carries
`RunningQueryRegistry::list(tenant)` → `get` → `participant_cut().fingerprint()`
in the registry's own request-ID order; no second cut copy and no new registry
traversal API. After `await_execute_paused` and before `kill`, the journey
requires exactly one active fingerprint and requires it to equal the builder's
latest cut. After the terminal failure it requires the build-total delta to be
exactly one, and retains the existing structured non-transport error, attempt
totals (one started, zero succeeded), single survivor activation, zero live
leases, and idle-ownership assertions.

```
PASS wyrd-testing::oracle analytical_activation::selected_peer_failure_is_terminal
```

### Focused verification

- `mise run check:bifrost` — `cargo fmt --all -- --check`, the scoped Clippy
  lane, `check:bifrost-oracle-deploy`, `check:bifrost-resource-governance`, and
  `check:object-store-pin` all pass. `check:tenant-isolation` fails, and fails
  identically at the task's `planning_base` (`b96252b0e`) on
  `crates/vala/vala-sql/{migrations/20260910000019,migrations/20260910000022,src/queries/oracle_admission.rs,src/queries/scribe_batch_commits.rs}`
  — none of which this task writes. Pre-existing, out of scope, not worked
  around.
- `mise run test:bifrost:integration:redux` — 960 tests run, 960 passed.
- `mise run test:bifrost:journey:oracle` — 22 tests run, 22 passed.
- `git diff --check` — clean; working tree clean.

### Scope confirmation

No public contract, endpoint, SDK field, protobuf field, descriptor,
dependency, configuration knob, trait, registry, generic observer, crate, or
module was added, and no production metric label or family was introduced. No
fallback, retry, repin, or replan path exists. No query ordinal was revived.
The single root, single admission, immutable deadline, tenant isolation,
Interactive floor, and joined cleanup are unchanged. No second remediation task
was created; all four findings land here.

`tasks/05-pre-mcp-buildout.md` declares
`depends_on: [BIFROST-R6-T04-REMEDIATION-02-EXACT-BINDINGS-AND-ATTEMPT-EVIDENCE]`,
so MCP buildout cannot begin while these findings remain open.
