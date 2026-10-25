# TASK-007 task-review verdict

**FIX_REQUIRED** — `FIND-TASK-007-1`, `FIND-TASK-007-2`.

The shared ordering correction and retrieval semantics are accepted. Required comparable performance evidence and representative all-table Bloom-policy exploration remain incomplete. No production correctness regression was independently confirmed.

## Immutable subject and round

- Repository: `/Users/stevenforrester/Documents/GitHub/schema-change-task-7`.
- Base: `76e095738d2cf89e10523a92bb9b4f585fa1dd3d`.
- Candidate: `dabe0e7dde58c6e26d584120ae0a48cfede79436`.
- First review of `git diff 76e095738...dabe0e7dd`, all five changed files. No previous TASK-007 acceptance/findings/remediation carried forward; no convergence assessment applies.
- Approved authority: revision-16 `spec.md` and its relevant supplement, original TASK-007 at the base, root AGENTS and applicable architecture/Rust/testing rules.
- Candidate and tracked source remained unchanged. Only this new review-report directory was written. No source edits, commits, merge, push or deployment.

## Independent reviewers and coverage

| Role | Report | Result and coverage |
|---|---|---|
| Fresh behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; full original task acceptance, five-file diff, existing retrieval owners, measurement/population evidence, applicable rules/rustdoc and maintainability. Proposed BEH-001/002. |
| Fresh domain-rev-query-execution | [domain-review-query-execution.md](domain-review-query-execution.md) | PASS; shared physical order/distribution boundary, actual pinned planner/network consumer, source/memory authority and query-local planning failure propagation. No proposed findings. |
| Fresh ponytail-rev | [findings-validation.md](findings-validation.md) | Independently traced source, obligations, existing alternatives, materiality and correction burden; REVISED both proposals into the two retained findings below. No new blocker. |

[scope.md](scope.md) records navigation and conditional specialist selection. No new mutable lifecycle owner or broad cross-tier implementation required another specialist. Behavior review retained standards/maintainability coverage; the query-domain reviewer traced request-local failure propagation. Missing unselected specialist reports are not a coverage gap.

No follow-up reviewer was needed: domain PASS concerns production behavior while behavior FAIL concerns required evidence; there is no material conflict or unresolved production common cause. Independent validation reconciled the policy interpretation and narrowed correction scope. It discovered no new finding requiring another source validator.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Distinct inline/storage/warehouse inventory and existing owners | Committed inventory; TenantEvidence and shared result/trace/window/group/custom owners; storage regressions remain implementation-reported | PASS |
| Comparable final-contract baseline/candidate measurements, per-path timings/work/variance and representative write cost | ScanEvidence drops existing elapsed/process evidence; no paired record; write-byte comparison confounded | FAIL — FIND-TASK-007-1 |
| Representative nine-table exploration, direct-name/version/UID/environment/subject filters, full/partial/empty declarations and physical work | Seven populated producers; gateway empty; audit principal-only; system policy goes full→empty | FAIL — FIND-TASK-007-2 |
| Canonical result, pending absence and no tenant/result substitution or lookback | Unchanged VerificationControl; focused completed-result journey PASS | PASS |
| Window population, boundaries, distinct windows and one combined statement | Existing ObservationWindow; focused window journey PASS; recorded single-cut regression | PASS |
| Group batching, input order and separate source occurrences | Unchanged GroupPoller/Sources; focused group journey PASS | PASS |
| Custom SQL exact ordered answers/top-k and complete-result refusals | Native sort-preserving boundary and appended sanity checker; focused SQL and physical-rule tests PASS | PASS |
| Physical/distributed requirements, single planner and request-local failures | Domain source trace of pinned planner plus distributed root/variant regression PASS | PASS |
| Live/hot/published/retained visibility and missing-Bloom exactness | Existing proofs pass for present cases; extra representative coverage isolated to finding 2 | PASS for preserved behavior |
| Nine default declarations/ten result defaults unchanged | Full diff and default review; no layout or dependency-pin edits | PASS for preservation; cost coverage fails above |
| Whole authorized trace per attempt, retry scope, immutable judgment | Unchanged TASK-006 owner; recorded focused trace regressions and current two-execution collection proof | PASS |
| Tenant/table/column authority, nonblocking audit, typed storage validation, deadlines | Unchanged shared authority/source/collection owners; negative result/custom journeys; query-local failure trace | PASS |
| No cache/index/store/SLO/lookback/default/public/generated contract expansion | Complete five-file diff; Cargo change is comment only | PASS |
| Rust ownership/rustdoc, existing mechanism reuse and credible diagnosis | Discovery and validation reviewed changed items; native merge/checker reused at shared owner | PASS at material gate |

The full source-backed matrix and interpretation of nullable audit, partial policy, base equivalence and process attribution are preserved in [findings-validation.md](findings-validation.md). Green tests establish their present assertions; they cannot close missing measurement/population cases.

## Validated finding ledger

| Stable ID | Discovery / validation | Obligation, evidence and consequence | Selected correction boundary |
|---|---|---|---|
| FIND-TASK-007-1 | BEH-001 / REVISED / MISSING | TASK-007 and AC-017 require comparable stage/latency/work/write observations. Task evidence has no matched per-path base/candidate record; verification_runtime.rs:813–889 discards existing elapsed/process observations. Performance completion claims are unsupported, although no slowdown is established. | Reuse existing telemetry/profile/elapsed owners for repeated bounded comparisons, honest process/write attribution and variance; no forced optimization or new profiler. |
| FIND-TASK-007-2 | BEH-002 / REVISED / MISSING | TASK exploration and AC-008/016/017 require representative nine-table policy proof. verification_runtime.rs:6201–6235,6470–6546,6586–6709,6761–6837 shows empty gateway, limited audit and no partial system policy. Positive gateway loss and required policy cases can escape the current proof. | Reuse real GatewayCapture and existing alter/query/physical-inspection owners for populated accepted-context calls, truthful audit/null/key coverage and one proper-subset policy. |

Exact producer/consumer tracing, source references, materiality, burden and focused closure proof are in the independent ledger. Both are evidence-completeness findings. Reject a writer-specific resource profiler, invented audit subjects, exhaustive subset testing, positive pruning quota, new ingestion API or required speedup as scope expansion. No rejected proposal creates remediation work; minor style observations carry no finding ID.

## Verification and limits

Review-time commands/results are recorded in [verification.md](verification.md):

- All 27 live/analytical unit tests passed.
- Exact new ordering/sanity/distributed-plan regressions and profile test passed (4/4).
- Server build and isolated Postgres migration prerequisite passed.
- All five focused completed-result/window/group/custom-query/exploration journeys passed (34.833 seconds).
- Committed range `git diff --check` passed.

The full Oracle journey lane, TASK-006 full proof set, hosted Artifact regressions, format, lints and tenant-isolation are implementation-reported, not repeated here. A macOS linker compact-unwind warning did not fail compilation/tests. No platform/capacity qualification or synthetic load was run or required. Current correctness assertions are credible; the residual risk is missing required measurement/population evidence.

## Handoff

One bounded remediation task: [TASK-007-R1-complete-performance-evidence.md](TASK-007-R1-complete-performance-evidence.md), routed to `$wyrd-implement` when implementation is requested. Preserve the accepted ordering fix, authority and retrieval semantics. Next review checks these two IDs and the changed measurement/fixture paths against this candidate, carrying forward unaffected acceptance evidence. No spec revision is required for the selected corrections.
