---
task: ORACLE-LOCAL-T01
verdict: FIX_REQUIRED
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
candidate: d3d379cd0b61e7dd1bb6a6e5fd227affa2823a8a
---

# Oracle local admission cumulative re-review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/oracle-local-admission/spec.md`, revision 2
- Original task: `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`
- Prior verdict: `changes/active/oracle-local-admission/review/task-review-01/verdict.md`
- Remediation task: `changes/active/oracle-local-admission/review/task-review-01/ORACLE-LOCAL-T01-R1-close-admission-gaps.md`
- Base: `d3888ddae83c833c3eb85edc0ce226eb6debadce`
- Cumulative candidate: `d3d379cd0b61e7dd1bb6a6e5fd227affa2823a8a`
- Reviewed range: `d3888ddae..d3d379cd0`

The F1 OLAP-deletion commits are a separately authorized sibling task and are
not classified as R1 scope drift. This review still evaluates the resulting
candidate wherever those commits can affect R1 behavior. The implementation
completion summary was not used as evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 — PostgreSQL-free pod-local admission | The cumulative diff removes the durable admission owner, server boot composition, runtime SQL modules, and continuity path while retaining historical decode/migrations | Static diff and consumer inspection | PASS |
| REQ-002 / REQ-004 / AC-003 — bounded FIFO, equal tenant rotation, class sharing, floor protection, work conservation, and absolute queue waits | `oracle/admission.rs:180-241, 1068-1166` owns the tenant ring and grant loop | Focused fairness tests pass, but a later expired FIFO entry can be granted in the same multi-grant pass; FIND-ORACLE-LOCAL-T01-8 | FAIL |
| REQ-003 / AC-002 / INV-001 — one aggregate governed memory root with query ceilings and explicit headroom | `resources.rs:3658-3744, 4547-4770` bounds infallible governed growth by remaining query ceiling, retains the governed/headroom split, and releases headroom first; leader/follower admission no longer debits class memory | Directly reran the shared-root and zero-idle-memory tests; both passed | PASS |
| REQ-005 — CPU, concurrency, memory, and scratch remain distinct | `resources.rs:79-146, 3370-3536` derives CPU/memory slot capacity and separately owns slots, actual pool growth, and scratch | Direct config and resource tests passed; source rustdoc still states that slot admission charges the deleted class-memory quantum; FIND-ORACLE-LOCAL-T01-9 | FAIL |
| REQ-006 / AC-004 / INV-005 — bounded class-capable Analytical workers | `oracle/participant_cut.rs:114-258` filters Analytical-capable remotes, rotates deterministically, bounds by the leader limit, and records zero selected workers for Interactive; `wyrd-server/src/boot/mod.rs:1733,2370-2377` advertises only locally admissible classes | Directly reran bounded/stable selection, incapable-replica exclusion, and class-advertisement tests; all passed | PASS |
| REQ-007 — ownership spans terminal settlement | Query envelopes retain shared memory views, scratch, slots, participant reservations, and fail-closed teardown owners | Static ownership review plus direct shared-root accounting tests found no remaining release imbalance | PASS |
| REQ-008 / AC-005 — local bounded-cardinality saturation telemetry and authority alignment | Metrics are driven from actual governor transitions and selected cuts; retired delegated terms are absent from production metrics/config | Direct metrics contract test passed, but materially changed rustdoc contradicts the implemented memory model; FIND-ORACLE-LOCAL-T01-9 | FAIL |
| Non-goals — no replacement coordinator, migration drop for admission history, dependency, SDK/Card contract, or caller-selected class | No prohibited R1 surface appears in the cumulative diff; separately authorized F1 work is excluded from R1 drift classification | Diff and manifest inspection | PASS |
| Repository rules — exact default-feature tests compile and task-added imports stay at module scope | Test instrumentation uses `cfg(any(test, feature = "test-support"))`; candidate `a6e28b549` restores production imports outside that gate | Default-feature `cargo check` and seven focused tests completed successfully; stale changed rustdoc remains FIND-ORACLE-LOCAL-T01-9 | FAIL |

## Material findings

### FIND-ORACLE-LOCAL-T01-8 — INCORRECT: a later expired FIFO waiter can be granted

- Violated obligation: REQ-002, AC-003, the required absolute queue-wait bound,
  and the task rule that an expired waiter rejects without retaining resources.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:223-241,768-770,1089-1147`.
- Evidence: `prune_expired` removes expired entries only while they are each
  tenant queue's current head. `grant_waiters` calls it once before a loop that
  may issue several grants. A same-tenant live head can therefore conceal a
  later waiter whose caller deadline is already expired; after the live head is
  popped, the loop selects and allocates resources to the expired waiter without
  another deadline check. Its receiver and deadline sleep are then both ready,
  so `tokio::select!` may accept the grant.
- Observable consequence: a request can be admitted and begin owning slot and
  scratch resources after its absolute admission deadline, violating bounded
  overload behavior and transiently delaying live work.
- Required correction: keep deadline enforcement in `OracleAdmission` and make
  every grant decision reject entries whose absolute deadline has passed,
  including entries exposed after an earlier FIFO head is granted in the same
  scheduling pass. Add deterministic focused proof with an older live head and
  a later already-expired waiter in one tenant queue.

### FIND-ORACLE-LOCAL-T01-9 — VIOLATION: changed rustdoc still claims slot admission reserves memory

- Violated obligation: REQ-005, AC-005, AGENTS.md section 16's requirement that
  materially modified Rust documentation explain actual invariants, and R1's
  removal of class-quantum memory ownership.
- Location: `crates/vala/vala-bifrost-redux/src/resources.rs:39-64,79-95,1717-1756,3302-3333`; `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:1172-1178`.
- Evidence: the candidate correctly charges governed memory only when a shared
  pool consumer grows, but these comments call the 32/64 MiB value an
  "admission charge", say a slot unit "actually charges" it, describe
  `OracleResourceRequest.memory_bytes` as root memory demand, and reference
  "Task 01 accounting" in production source. The changed `oracle_worker_slots`
  documentation was introduced by this task and now contradicts
  `try_acquire_oracle`, whose own corrected comment says admission reserves no
  memory.
- Observable consequence: the resource owner's public/internal contract tells
  maintainers and generated documentation that admission consumes memory which
  the runtime deliberately no longer consumes, obscuring the safety boundary
  R1 was required to clarify.
- Required correction: make the touched resource and admission rustdoc describe
  the value only as a sizing/grant/partition quantum, actual shared-pool growth
  as the first governed memory charge, and slots as concurrency-only. Remove the
  task reference from production documentation. Do not change runtime behavior
  or add another memory field/owner.

## Prior-finding closure

| Prior finding | Closure evidence | Result |
|---|---|---|
| FIND-ORACLE-LOCAL-T01-1 | Infallible growth passes the query's remaining governed ceiling and retains excess as headroom; focused shared-root test passed | CLOSED |
| FIND-ORACLE-LOCAL-T01-2 | Leader/follower class-memory leases and synthetic memory totals are removed; zero-idle-memory test passed | CLOSED |
| FIND-ORACLE-LOCAL-T01-3 | Calibration rejects invalid enabled/disabled tenant caps; focused config test passed | CLOSED |
| FIND-ORACLE-LOCAL-T01-4 | Boot advertises the installed split and selection filters class-capable remotes before bounding; three focused tests passed | CLOSED |
| FIND-ORACLE-LOCAL-T01-5 | Candidate contains a real authenticated FIFO/rotation journey and metrics proof for refusal, headroom, and selected workers | CLOSED |
| FIND-ORACLE-LOCAL-T01-6 | Default-feature Redux lib tests compile; default-feature `cargo check` and focused test run passed | CLOSED |
| FIND-ORACLE-LOCAL-T01-7 | Task-added imports are in module import blocks; no cited function-scoped imports remain | CLOSED |

## Verification limits

- Direct review ran `mise exec -- cargo check --locked -p vala-bifrost-redux` successfully. It emitted unused-code warnings; none establishes an additional R1 acceptance failure, and the separately reviewed F1 sibling owns several affected surfaces.
- Direct review ran five default-feature Redux tests covering shared memory, zero idle memory, worker selection, and metrics; all passed.
- Direct review ran the server calibration and supported-class tests; both passed.
- Direct review ran the two named admission fairness/follower tests; both passed, but neither covers FIND-ORACLE-LOCAL-T01-8's non-monotonic per-request deadline ordering.
- `git diff --check d3888ddae..d3d379cd0` passed.
- The complete Postgres-backed Oracle journey and full `verify:bifrost` lane were not rerun during this static re-review.

## Verdict

`FIX_REQUIRED`
