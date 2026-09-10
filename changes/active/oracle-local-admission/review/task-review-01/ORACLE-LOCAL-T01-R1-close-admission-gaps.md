---
id: ORACLE-LOCAL-T01-R1
title: Close Oracle local-admission acceptance gaps
kind: remediation
status: ready
skill: wyrd-implement
spec: changes/active/oracle-local-admission/spec.md
task: changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md
base: d3888ddae83c833c3eb85edc0ce226eb6debadce
reviewed_candidate: fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7
findings:
  - FIND-ORACLE-LOCAL-T01-1
  - FIND-ORACLE-LOCAL-T01-2
  - FIND-ORACLE-LOCAL-T01-3
  - FIND-ORACLE-LOCAL-T01-4
  - FIND-ORACLE-LOCAL-T01-5
  - FIND-ORACLE-LOCAL-T01-6
  - FIND-ORACLE-LOCAL-T01-7
---

# Close Oracle local-admission acceptance gaps

Required execution skill: `$wyrd-implement`.

## Subject and authority

Implement against the cumulative candidate beginning at `fe2fb5f8ba5e1eb09e2faf8a7bf909e34a8c61b7`, preserving the approved behavior in `changes/active/oracle-local-admission/spec.md` revision 2 and the original task at `changes/active/oracle-local-admission/tasks/01-simplify-oracle-admission.md`. The next review covers the complete cumulative range from base `d3888ddae83c833c3eb85edc0ce226eb6debadce`.

## Diagnosis and intended correction

### Governed memory is not yet bounded by each query ceiling

FIND-ORACLE-LOCAL-T01-1: `OracleMemoryRoot::grow` classifies infallible bytes only against pod-wide free floor/elastic capacity. It does not apply the query view's remaining ceiling, so bytes above a query's immutable grant may remain governed rather than becoming explicit process headroom. The existing shared-root test exercises infallible growth but never distinguishes the two charge components.

Keep `OracleMemoryRoot`, the query view, and `OracleMemoryCharge` as the existing owners. For every infallible growth, the governed component must be bounded by both the pod's currently available governed capacity and that query's remaining governed ceiling; the remainder is retained as headroom and later released headroom-first. This closes the gap without another pool, owner, or configuration surface.

### Class quanta still masquerade as resident memory

FIND-ORACLE-LOCAL-T01-2: follower acquisition retains an `OracleMemoryLease` for the 32/64 MiB class quantum before any DataFusion reservation, while leader admission retains and reports the same quantum as reserved memory. This contradicts the selected separation: slots govern concurrency, actual shared-pool growth governs memory, and scratch remains independently leased.

Remove query-class quantum memory ownership and synthetic memory reporting from leader and follower paths. Reuse the existing aggregate slot ledger, query memory view, actual root-growth accounting, and scratch owner. An admitted query or follower with no live pool reservation must consume zero governed query bytes; its first actual consumer growth must be the first memory charge.

### Calibration accepts a different contract than the profile declares

FIND-ORACLE-LOCAL-T01-3: calibrated tenant limits are clamped into range. Invalid evidence therefore boots with silently rewritten values.

Make calibration translation reject an Interactive limit outside `1..=total_local_units`. When Analytical is enabled, reject limits outside `ANALYTICAL_QUERY_SLOT_UNITS..=analytical_slots`; when disabled, require zero. Preserve the existing v2 names and do not add aliases or migration behavior.

### Worker eligibility and advertised class capacity disagree

FIND-ORACLE-LOCAL-T01-4: every pod advertises Analytical even when its local split disables it, and remote bounding happens before class compatibility. Class-neutral/Interactive roster construction also records selected remotes.

Publish `supported_classes` from the installed local split: Interactive remains supported; Analytical is present only when the pod can admit its two-unit minimum. Once the physical root derives Analytical, select the configured deterministic subset from the pinned live, authorized, Analytical-capable remote roster. That single list remains the authority for reservation, dispatch, metrics, and cleanup. Preserve stable node ordering, UUID rotation, zero-worker local execution, the leader, deadlines, security checks, and the no-successor terminal policy.

### Cross-boundary proof is weaker than the accepted task

FIND-ORACLE-LOCAL-T01-5: the server journey observes eventual wins from nondeterministic races but not FIFO or exact equal-weight rotation. The metrics test does not produce memory refusal, nonzero headroom, or selected-worker evidence, and RED results were not recorded.

Reuse the existing schema-stall, queue inspection, and release choreography. Hold capacity, enqueue identified tenant requests in a known order, observe that they are queued, then release before the existing bounded wait expires and assert completion order. This must prove per-tenant FIFO and one-grant equal tenant rotation through real authenticated server requests without a new public API or a longer production wait. Extend the existing metrics contract through the real shared root and Analytical cut so it asserts a refusal series, nonzero headroom, and bounded selected-worker observation. Record every original task RED and GREEN command result.

### Default unit-test compilation and import placement regressed

FIND-ORACLE-LOCAL-T01-6: default-feature Redux lib tests consume instrumentation compiled only with `test-support`, so the task's exact named commands fail with `E0599`.

Reuse the repository's established `cfg(any(test, feature = "test-support"))` boundary for instrumentation needed by both internal Rust unit tests and external test-support consumers. Do not add a feature or change the approved exact commands.

FIND-ORACLE-LOCAL-T01-7: new function-scoped imports violate the repository rule that a module's imports form its dependency manifest.

Move the new imports to the existing owning test-module import blocks. Do not suppress or weaken the rule.

## Constraints and preserved behavior

- Keep PostgreSQL out of query admission and retain historical migrations and decode-only audit history.
- Keep one `OracleAdmission` scheduler and one `BifrostResourceGovernor` slot ledger; add no scheduler, semaphore, lease, cache, coordinator, factory, trait, or dependency.
- Keep one shared Oracle `FairSpillPool` root and private query ceiling views.
- Preserve authentication, authorization, tenant identity, audit WAL, physical-root query-class derivation, result limits, spill formats, object reads, deadlines, terminal failure, and fail-closed cleanup.
- Preserve independently enforced scratch, slot, CPU parallelism, queue, and selected-worker bounds.
- Preserve the deterministic selected-worker algorithm and single-attempt policy after moving it to the class-eligible cut.
- Keep changes within the original task's owners and test surfaces.

## Non-goals

- No specification revision, public API, SDK, Card, migration, compatibility alias, deployment topology, or distributed admission mechanism.
- No unrelated Forge readiness or test-inventory work.
- No broad refactor of Oracle planning, dispatch, or resource ownership beyond these findings.
- No new test framework or generalized scheduling fixture.

## Acceptance criteria

- AC-R1-001 / FIND-ORACLE-LOCAL-T01-1: infallible growth above a query ceiling charges at most the ceiling as governed, records the remainder as headroom even when the pod root has free bytes, and shrink/drop restores every counter.
- AC-R1-002 / FIND-ORACLE-LOCAL-T01-2: a newly admitted leader and follower consume slot/scratch ownership but zero governed query memory until a pool consumer grows; actual growth and release reconcile the shared root exactly.
- AC-R1-003 / FIND-ORACLE-LOCAL-T01-3: zero/oversized Interactive caps, one-unit/oversized enabled Analytical caps, and nonzero disabled Analytical caps are rejected rather than clamped; valid bounds remain unchanged.
- AC-R1-004 / FIND-ORACLE-LOCAL-T01-4: an Analytical-disabled pod does not advertise Analytical; bounded selection chooses only Analytical-capable remotes; an incapable unselected replica cannot affect success; Interactive work records zero selected workers.
- AC-R1-005 / FIND-ORACLE-LOCAL-T01-5: the real authenticated server journey deterministically proves per-tenant FIFO and exact equal-weight rotation in addition to bounded overload, floor protection, borrowing, and both-class progress; the metrics contract observes refusal, nonzero headroom, and selected-worker count.
- AC-R1-006 / FIND-ORACLE-LOCAL-T01-6: every exact default-feature Redux lib-test command named in the original task compiles and executes the intended single test without selecting zero tests.
- AC-R1-007 / FIND-ORACLE-LOCAL-T01-7: no task-added function-scoped import remains.

## Focused proof and broader verification

Run every exact named RED command from the original task sequentially, including the default-feature Redux lib-test commands. Add the narrow focused cases required by AC-R1-001 through AC-R1-005 to the existing owning tests rather than creating new test binaries.

Then run:

```bash
mise run check:bifrost-resource-governance
mise run test:bifrost:integration:redux
mise run test:bifrost:integration:sql
mise run test:bifrost:integration:server
mise run test:bifrost:journey:oracle
mise run codegen:check
mise run verify:bifrost
mise run fmt
mise run lints
git diff --check d3888ddae83c833c3eb85edc0ce226eb6debadce..<new-candidate>
```

Record the exact focused commands, selected test names, exit status, and direct behavioral evidence in the original task before requesting cumulative re-review.
