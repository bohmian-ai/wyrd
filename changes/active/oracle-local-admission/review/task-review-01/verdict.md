---
task: ORACLE-LOCAL-T01
verdict: FIX_REQUIRED
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
candidate: fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7
---

# Oracle local admission task review

## Immutable subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`
- Approved specification: `changes/active/oracle-local-admission/spec.md`, revision 2
- Original task: `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`
- Base: `d3888ddae83c833c3eb85edc0ce226eb6debadce`
- Candidate: `fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7`
- Reviewed range: `d3888ddae..fe2fb5f8b`

The later commits `bdbaee4c0`, `c3db2fc9b`, and `e83cf6c8c` are outside the immutable task candidate. Their changes and the verification run from `e83cf6c8c` are not candidate implementation evidence.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001 — admission is pod-local and ignores historical admission rows | Durable admission owner, SQL query/row modules, server boot composition, and public runtime types are removed in `194b37c73`; historical migrations and `AuditDetail::OracleAdmissionRecovery` remain | `capacity::heterogeneous_oracles_ignore_historical_admission_rows` exists; static candidate search finds no production `DelegatedOracleAdmission` or retired request/continuity types | PASS |
| REQ-002 and REQ-004 — bounded local FIFO, tenant rotation, shared class capacity, Interactive floor, and work conservation | `oracle/admission.rs:200-323, 1072-1177` implements per-tenant FIFO queues, a rotating tenant cursor, floor-first Interactive grants, oldest eligible class selection, and a governor-backed final capacity check | Focused unit tests cover the queue algorithm, but the required server journey does not prove FIFO or exact equal-weight order; see FIND-ORACLE-LOCAL-T01-5 | FAIL |
| REQ-003 / AC-002 / INV-001 — one aggregate governed memory root with truthful query ceilings and headroom | `resources.rs:4553-4863` introduces one shared `FairSpillPool` root and query views | Infallible growth can charge bytes above the query ceiling as governed, and class quanta remain resident/synthetic memory ownership; see FIND-ORACLE-LOCAL-T01-1 and FIND-ORACLE-LOCAL-T01-2 | FAIL |
| REQ-005 — CPU, concurrency, memory, and scratch remain distinct | `resources.rs:3386-3470` and `wyrd-server/src/boot/mod.rs:1652-1717` derive slots from local CPU/memory and retain separate scratch and slot ledgers | Follower class memory is still charged before DataFusion reserves bytes, invalid calibrated tenant caps are clamped, and the exact default-feature test target does not compile; see FIND-ORACLE-LOCAL-T01-2, FIND-ORACLE-LOCAL-T01-3, and FIND-ORACLE-LOCAL-T01-6 | FAIL |
| REQ-006 / AC-004 / INV-005 — bounded deterministic Analytical worker selection from the eligible cut | `oracle/participant_cut.rs:97-139, 180-198` sorts, rotates, and bounds the retained remote roster | Selection occurs before class eligibility is checked, while boot advertises Analytical even when disabled; see FIND-ORACLE-LOCAL-T01-4 | FAIL |
| REQ-007 — complete lifetime ownership and fail-closed cleanup | Candidate retains query envelopes, selected reservations, exact cleanup owners, and shared-root poison checks | Static ownership review and specialist review found no additional release, rollback, lock-order, or teardown defect beyond FIND-ORACLE-LOCAL-T01-1 and FIND-ORACLE-LOCAL-T01-2 | FAIL |
| REQ-008 / AC-005 — understandable bounded-cardinality local saturation telemetry | Local byte/slot gauges and selected-worker histogram were added; retired delegated metric names were removed | The metrics contract does not exercise memory refusal, nonzero headroom, or selected-worker count, and selection records remote workers for class-neutral/Interactive rosters; see FIND-ORACLE-LOCAL-T01-4 and FIND-ORACLE-LOCAL-T01-5 | FAIL |
| Non-goals — no replacement coordinator, migration drop, dependency, public SDK/Card surface, or caller-selected query class | Diff adds none of the prohibited surfaces | Diff and manifest inspection | PASS |
| Repository rules — exact focused tests remain runnable and imports stay at module scope | No new dependency or broad abstraction was added | Default-feature lib-test compilation regressed and new function-scoped imports violate `architecture/agent-rules.md`; see FIND-ORACLE-LOCAL-T01-6 and FIND-ORACLE-LOCAL-T01-7 | FAIL |

## Material findings

### FIND-ORACLE-LOCAL-T01-1 — INCORRECT: infallible growth exceeds the query's governed ceiling

- Violated obligation: REQ-003, INV-001, AC-002, and the task rule that bytes above a query ceiling are process headroom rather than governed allocation.
- Location: `crates/vala/vala-bifrost-redux/src/resources.rs:3669-3707, 4673-4698` at candidate `fe2fb5f8b`.
- Evidence: `OracleMemoryRoot::grow` passes the whole `additional` amount to `reserve_oracle_query_memory_infallible`. That governor operation knows only pod-wide free floor/elastic capacity; it does not receive or apply the query view's remaining ceiling. An idle query with a 256 MiB ceiling can therefore call infallible `grow(300 MiB)` and classify all 300 MiB as governed when the root has room.
- Observable consequence: one query can consume governed capacity beyond its immutable grant and starve sibling queries; `oracle_infallible_bytes` understates the query's required headroom.
- Required correction: keep the shared root and existing charge split, but classify no more than the query's remaining ceiling as governed for infallible growth. Every byte above that ceiling must be retained and released as headroom. Add a focused assertion covering an infallible growth larger than the query ceiling while the pod root still has free capacity.

### FIND-ORACLE-LOCAL-T01-2 — INCORRECT: the retired class memory quantum still consumes and reports resident memory

- Violated obligation: REQ-003, REQ-005, AC-002, and the explicit task instruction to stop charging the 32/64 MiB class quantum as resident query memory.
- Location: `crates/vala/vala-bifrost-redux/src/resources.rs:2557-2618, 3489-3555, 4370-4445, 4930-4959`; `crates/vala/vala-bifrost-redux/src/oracle/admission.rs:687-731, 1152-1161`.
- Evidence: follower acquisition still calls `try_acquire_oracle_memory(class.memory_bytes())` and retains an `OracleMemoryLease` before any DataFusion consumer grows. Leader admission carries `request.memory_bytes` into `OracleQueryResources`, increments `AdmissionClass.memory_used`, and exposes the synthetic total as `reserved_memory_bytes` despite the shared root charging actual reservation only on pool growth. Existing tests at `resources.rs:6292-6324, 7041-7068` affirm the obsolete quantum behavior.
- Observable consequence: followers remove 32/64 MiB from real pod capacity before reserving DataFusion memory, and admission inspection reports synthetic memory rather than aggregate governed use.
- Required correction: slot units alone must govern leader/follower concurrency; leader and follower memory must be charged only by actual shared-root consumer growth. Remove the class-quantum memory lease and synthetic reserved-memory projection, preserving the private query view, scratch ownership, slot ownership, and actual root metrics. Replace the obsolete tests with one proving an idle admitted leader or follower consumes zero governed bytes until its pool grows.

### FIND-ORACLE-LOCAL-T01-3 — INCORRECT: invalid calibrated tenant caps are silently rewritten

- Violated obligation: the approved task's local-capacity rules and AC-005 require invalid Interactive and Analytical tenant caps to be rejected.
- Location: `crates/wyrd/wyrd-server/src/config.rs:785-797`.
- Evidence: `translate_oracle_calibration` uses `clamp` for both tenant limits. Zero or oversized Interactive limits and one-unit or oversized Analytical limits are accepted and silently changed instead of failing validation.
- Observable consequence: operators can boot with invalid calibration evidence while the server runs a different capacity contract than the profile declares.
- Required correction: reject tenant proposal values outside the resolved class bounds; retain zero only when Analytical is disabled. Add focused configuration cases for zero, one-unit Analytical, and above-capacity values.

### FIND-ORACLE-LOCAL-T01-4 — INCORRECT: disabled or class-ineligible workers participate in Analytical selection

- Violated obligation: REQ-006, INV-003, INV-005, AC-004, and the task rules to avoid exposing an Analytical class that cannot admit a query and to select from the pinned eligible roster.
- Location: `crates/wyrd/wyrd-server/src/boot/mod.rs:1725-1735`; `crates/vala/vala-bifrost-redux/src/oracle/participant_cut.rs:97-139, 180-233`.
- Evidence: boot always publishes `supported_classes: [Interactive, Analytical]`, even when `analytical_slots == 0`. The roster bounds remotes in `freeze` before `finalize(query_class)` checks class support. The selected-worker histogram is also recorded during this class-neutral freeze.
- Observable consequence: deterministic rotation may select a small replica that cannot admit Analytical work and fail a query even when an unselected capable replica exists; Interactive preparation can report a nonzero selected-worker count.
- Required correction: advertise only locally admissible classes and perform bounded remote selection from the class-compatible eligible roster after the physical root fixes the query class. Preserve stable node ordering, UUID rotation, the leader, the configured bound, and the single selected list used by reservation, dispatch, metrics, and cleanup. Add a mixed-capability test proving a disabled/unselected replica cannot affect the query and Interactive selection reports zero workers.

### FIND-ORACLE-LOCAL-T01-5 — MISSING: required journey and telemetry evidence is incomplete

- Violated obligation: AC-003, AC-005, the task's ordered scenarios 3 and 5, and its completion-evidence section.
- Location: `crates/wyrd/wyrd-testing/tests/bifrost/oracle/capacity.rs:1456-1744`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:5719-5801`; task evidence appended at `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`.
- Evidence: the public journey only proves that both tenants win at least once across four races; it records neither per-tenant FIFO completion nor exact equal-weight rotation. The metrics test admits and drops one Interactive query but never causes or asserts a memory refusal, nonzero infallible headroom, or a selected-worker sample. The task evidence explicitly substitutes unit tests for the journey's FIFO obligation and records no RED results.
- Observable consequence: the cross-boundary scheduler ordering and required operational signals can regress while all recorded evidence remains green.
- Required correction: use the existing schema-stall and queue-inspection choreography to enqueue identified requests, release within the bounded wait, and assert public completion order for FIFO and equal tenant rotation. Extend the existing metrics test to exercise and assert memory refusal, nonzero headroom, and Analytical selected-worker count. Record the exact RED and GREEN results required by the original task.

### FIND-ORACLE-LOCAL-T01-6 — REGRESSION: the task's exact default-feature lib-test command no longer compiles

- Violated obligation: the task's exact named-test commands, AGENTS.md section 11, and the completion standard requiring targeted checks to pass.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs` and `oracle/admission.rs` test-support instrumentation consumed by `oracle/analytical.rs` unit tests.
- Evidence: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::analytical_worker_selection_is_bounded_and_stable)'` fails before selection with 11 `E0599` errors. Default-feature lib tests reference `graph_leases_activated_total` and `runtime_inspection`, but those methods exist only under `feature = "test-support"`.
- Observable consequence: none of the task's default-feature focused Redux lib-test commands can run as written; the later all-feature aggregate cannot substitute for them.
- Required correction: reuse the repository's existing `cfg(any(test, feature = "test-support"))` pattern so Rust unit-test instrumentation exists under `cfg(test)` while remaining feature-gated for external journey consumers. Re-run every exact named Redux lib-test command from the task.

### FIND-ORACLE-LOCAL-T01-7 — VIOLATION: new imports were placed inside functions

- Violated obligation: `architecture/agent-rules.md` requires imports at module top and permits no general function-scoped test-import exception.
- Location: `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:5258-5260`; `crates/vala/vala-bifrost-redux/src/resources.rs:5966-5974`; `crates/wyrd/wyrd-server/src/config.rs:4536-4538`.
- Evidence: all listed imports were introduced by the task commits inside test functions.
- Observable consequence: the changed modules no longer follow the repository's explicit dependency-visibility rule.
- Required correction: move these imports into the owning test modules' top-level import blocks without changing behavior.

## Verification limits

- The reported `mise run verify:bifrost` 9/9 result was obtained from `e83cf6c8c`, after two commits outside the immutable task candidate changed test lanes and Forge readiness.
- Direct review reran the first exact focused Redux command from the task. It failed to compile with 11 `E0599` errors, so the `&&`-chained remaining focused commands were not executed.
- `git diff --check d3888ddae..fe2fb5f8b` passed.
- A separate DataFusion specialist reviewed the shared-root locks, rollback, release accounting, pool semantics, query ceiling, headroom, and teardown. It confirmed FIND-ORACLE-LOCAL-T01-1 and FIND-ORACLE-LOCAL-T01-2 and found no additional material defect in those boundaries.

## Prior-finding closure

No prior task-review verdict exists for this immutable candidate.

## Verdict

`FIX_REQUIRED`
