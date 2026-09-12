# TASK-006 task-review verdict

## Immutable subject

- Approved spec: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-006-coordinate-audit-publication.md`
- Base: `32a0aafecbda96a86103cd389e42728ea33e0c54`
- Candidate: `358cf636eda48ea31a3416e2c5b21b30873f83dd`
- Verdict: **FIX_REQUIRED**

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Frozen bound survives tail growth and competing freezes | Persisted bound and locked freeze | SQL concurrency test | PASS |
| Crash replay reuses deterministic batch identity | Tenant/range UUID and Scribe fence | Repeated publish test | PASS |
| Stale completion preserves newer bound | Monotonic watermark and matching-bound clearance | SQL stale-settle test | PASS |
| Settlement is atomic and idle staging drains | One tenant transaction | SQL/server tests | PASS |
| Concurrent tenants publish independently | Sweep awaits each tenant serially | Tenant-scoped SQL is not progress proof | FAIL |
| No transaction spans Scribe IO | Separate freeze/read/settle transactions | Source inspection | PASS |
| Correct roles and direct local Scribe; no Gate audit event | `from_state` and `ingest_frame` | Server journey | PASS |
| Gate bypass deleted without replacement public API | Public partial-cycle methods added | Source visibility | FAIL |
| Architecture describes frozen bound | Primary revised sections | Text inspection | PASS, but derived docs conflict |
| Required combined real-server replay race | Tail appended after settlement; competition fenced out | SQL covers only a separate seam | FAIL |
| Exactly one existing-harness multi-pod dedup scenario | Existing Scribe binary/harness | Focused scenario | PASS |
| No lease, claim table, migration, compatibility path, dependency, or harness | Diff inspection | N/A | PASS |
| Repository standards | Independent audit | `standards-review.md` | FAIL |

## Material findings

### FIND-TASK-006-1 — VIOLATION: partial durable workflow stages are public

`wyrd-server` publicly exports `audit::publication`; candidate `AuditPublisher::publish_range` and `settle` are public. `settle(tenant, seq_hi)` can advance the watermark and delete staged rows without proving Scribe acceptance. This replaces the deleted Gate bypass with a more dangerous public internal seam and violates INV-008. Keep the full publisher workflow as the production boundary and expose crash testing only through existing sanctioned test support.

### FIND-TASK-006-2 — INCORRECT: one slow tenant blocks all later tenants

`audit/publication.rs:117-137` iterates tenants and awaits the entire Scribe cycle serially. A delayed tenant prevents every later tenant from publishing, contradicting independent concurrent tenant progress. Use bounded concurrency within the existing `AuditPublisher`; prove with two tenants that holding one Scribe publication does not prevent the other settling.

### FIND-TASK-006-3 — MISSING: the required combined retained-history race is not exercised

In `wyrd-testing/tests/bifrost/server/audit_publication.rs:188-207`, the test holds the chain-head transaction across both Scribe calls, preventing tail append and competing freeze/settlement. It settles before adding the tail. Revise this existing journey so a tail grows above a committed frozen bound, a competing attempt and crash replay reuse it, retained rows stay unique, and staging drains.

### FIND-TASK-006-4 through FIND-TASK-006-9 — VIOLATION: repository standards fail

The independent audit's `STD-006-1` through `STD-006-7` are incorporated, with `STD-006-5` already represented by FIND-TASK-006-1. The remaining stable IDs map as follows: `FIND-TASK-006-4` RLS predicates; `-5` raw pool; `-6` signature imports; `-7` rustdoc; `-8` contradictory docs; `-9` missing docs check. Apply the testable corrections in `standards-review.md`.

## High-risk boundary review

The independent specialist found no additional TASK-006 security/durability defect: frozen-range serialization, tail exclusion, stale settlement, global Scribe dedup, tenant scoping, and direct-Scribe transaction boundaries passed source audit. This does not close the acceptance and standards findings above.

## Verification limits

The audit relied on immutable source and supplied runs. TASK-006's embedded 18/21 Scribe record was later superseded by 21/21 closeout evidence. No test result closes the missing combined interleaving or serial-tenant progress issue. `docs:check` is absent.

## Prior-finding closure

No prior TASK-006 review verdict was supplied.
