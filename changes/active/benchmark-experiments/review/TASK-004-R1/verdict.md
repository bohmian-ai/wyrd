# TASK-004 — FIX_REQUIRED

## Immutable subject and approved authority

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
- Base: `0b500d677531ba501924b1c6d412fe14ff9aa5cd`, packet-only parent of first implementation commit `b321c388c`.
- Candidate: `a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
- Full first-review range: `git diff 0b500d677531ba501924b1c6d412fe14ff9aa5cd...a020500e8ea0c39311d508d6b5bfa85ebe90368c`.
- Original task: `changes/active/benchmark-experiments/tasks/TASK-004-server-benchmark-timing.md` at base; candidate adds evidence.
- Governing spec: revision 8 of `changes/active/benchmark-experiments/spec.md`, SHA-256 `66a583cfaf0fb1310b55b3c92987c8bad859d5628ca06de575ca57208b5eea96`, explicitly approved by the maintainer in the preceding session. Approval persists; stale draft metadata/evidence is not a finding. The packet records the same text.

Candidate HEAD and source remained unchanged. Review wrote reports only and preserved pre-existing untracked benchmark records. TASK-003 dependency passed at `71d2c240a`; no TASK-004 acceptance or closure evidence is carried from another task. This is TASK-004's first implementation review, with no remediation convergence assessment applicable.

## Scope and independent coverage

`scope.md` records the full 36-file navigation map, authority, reviewer selection and exact report paths. Five fresh independent discovery reviewers covered the complete task and sensitive boundaries:

| Role | Report | Result / coverage |
|---|---|---|
| behavior-rev | `task-review-behavior.md` | FAIL; comprehensive original task acceptance, actual engine-to-record paths, resource/drain/population/artifact behavior, smoke/failure/recovery/non-goals |
| invariant-rev | `task-review-invariants.md` | FAIL; shared HDR/concurrency/snapshot arithmetic, quantization, component/phase/outcome/count/scan identity and clocks |
| repo-rev | `standards-review.md` | FAIL; full changed-surface authority map and mandatory Rust/dependency/platform/test/contract rules |
| maintainer-rev | `maintainer-review.md` | FAIL; complete existing-owner comparison and collection/artifact replacement workflow |
| security domain-rev | `domain-review-security.md` | FAIL; new local opt-in readout, authentication/classification, bounded labels/privacy and actual API/Forge startup exposure |
| fresh independent ponytail-rev | `findings-validation.md` | FIX_REQUIRED recommendation; source/materiality/correction validation of all proposals, six deduplicated findings |

The system role was not separately selected: behavior covered credible collection/smoke/drain/persistence failures, with invariant/security specialists covering shared state/exposure. No selected reviewer/report is missing. Orchestrator did not validate its own findings. `claim-comparison.md` groups proposals by violated obligation; no additional follow-up was needed because no material unresolved conflict or untraced common cause remained. The validator discovered no new blocker requiring separate validation.

## Validated ledger and correction calibration

| Stable ID | Discovery / validation | Classification and material failure | Selected existing correction owner |
|---|---|---|---|
| FIND-TASK-004-1 | B-001, INV4-01 / REVISED | INCORRECT/MISSING: L1 and Gateway L2 close CPU/work after outstanding requests drain; shared closing scrape also extends CPU past stated wall time | Existing Window/ResourceWindow and affected driver-to-Repetition conversions; freeze at declared admission stop before scrape/drain, retain late work and final latency |
| FIND-TASK-004-2 | B-002, INV4-02 / REVISED | INCORRECT/MISSING: actual producers omit admission-phase/repetition classification; completion slices and unrelated lower-bound traffic can mask missing offered outcomes/scans | Existing scoped client headers, actual unary/native/HTTP/gRPC send contexts, authenticated receivers and final Population cohort selection/reconciliation |
| FIND-TASK-004-3 | B-003, INV4-03, MNT-004-01 / REVISED | MISSING/REGRESSION: successful v2 results discard full interval buckets, ranks/max and structured reconciliation/windows | Existing shared Population-to-artifact/record handoff and target directory; preserve summary-only/v1 reporting and required write-failure invalidity |
| FIND-TASK-004-4 | SECURITY-001 / CONFIRMED | VIOLATION: outer pre-auth HTTP/tonic span setup lets unauthenticated/public/refused metadata mutate authoritative benchmark invalidity | Existing successful-auth context owners activate classification; retain ordinary refusal timing and existing permissions/audits |
| FIND-TASK-004-5 | REPO-004-1 / CONFIRMED | VIOLATION: explicitly mandatory changed-item Rust field/error/panic documentation absent | Bounded documentation corrections in changed owner files, batched with behavior remediation |
| FIND-TASK-004-6 | REPO-004-2 / CONFIRMED | VIOLATION: prohibited truncation/sign-loss as-and-suppress rank conversion | Local checked conversion using existing workspace convention; preserve quantile behavior without a new numeric subsystem |

`findings-validation.md` preserves each retained finding's exact locations, approved obligation, complete producer-to-consumer/sibling tracing, realistic scenario, observable consequence, materiality, burden, selected minimal correction and focused closure proof. Agreement was not treated as proof. No rejected component becomes optional remediation advice.

Validation corrected an important implementation premise: offered native writes use unary InsertBatch through the existing header path, not a long-lived write stream requiring redesign. Rejected correction scope includes rebuilding queues/resource/durability owners, unconditional logical-call/physical-attempt equality despite retries, compulsory target references, tenant/request labels, synchronized clocks, replacement collectors/report frameworks and unrelated qualification work. Runtime HDR ownership is justified by the existing tool recorder's global-recorder/summary limitations; no duplicate-owner DRIFT finding remains. All selected corrections fit approved revision 8; no new specification decision is needed.

## Reconciled acceptance matrix

| Original obligation / gate | Source and verification basis | Result |
|---|---|---|
| Bounded concurrent integer HDR, consistent bucket intervals, zero/reset/overflow/component validity | Registry/interval source; V1 and server seam tests | PASS |
| Real engine monotonic boundaries and per-request excluded-wait calculations | Existing engine owners and actual emission tests; real smoke all dispatched paths | PASS |
| Whole quantization bounds for strict judgments | Current Distribution/Reading/criteria source and threshold tests | Behavioral PASS; conversion rule FAIL under finding 6 |
| Local opt-in read-only bounded loopback snapshot and operational recorder | Config/router/API/Forge source, V3/V7 | PASS |
| Authenticated receiving classification | Pre-auth activation affects authoritative invalid counter | FAIL — finding 4 |
| Warmup drained before next baseline | Actual engine settlement/driver ordering | PASS; preserve during correction |
| Same-stop CPU/work/P and separate drain/final cohort | Real L1/Gateway L2/Window paths do not implement tested synthetic accounting | FAIL — finding 1 |
| Finite offered phase/class/outcome populations and matching scans; incidental work cannot cover loss | Outbound metadata and actual final phase consumers | FAIL — finding 2 |
| Retained full interval distributions, structured rank/max/count/window evidence | No replacement bucket artifact path; summary reduction drops required evidence | FAIL — finding 3 |
| v2 write, v1 read/report, unknown-version refusal and collision-safe summary publication | Shared Record/levels source; V5/V6 and full tooling lane | Existing mechanisms PASS; missing v2 evidence is finding 3 |
| Mandatory real smoke before expensive preparation | Shared Workload/runner/dispatch source; eight real smoke invocations | PASS |
| Missing resource/timing cannot become favorable zero | Resource/Smoke/population validity source and tests | PASS |
| Existing owners, dependency/platform/non-goal constraints and unrelated-change exclusion | Complete diff and selected source/authority coverage | PASS except explicit docs/cast gates |
| Mandatory changed-item rustdoc and numeric conversion rules | Source audit against root rules/Rust guide | FAIL — findings 5/6 |

Full reconciled source/proof coverage is in `findings-validation.md`. Green unit/smoke results establish their exercised mechanisms and do not cover the source-proven failed obligations.

## Verification and limits

`verification.md` records exact independent orchestrator runs: V1 1/1; V2/V3/actual V2a/server metrics 16/16; Gateway 1/1; Gate 2/2; V4/V5/V6 exact selectors 7/7; complete `wyrd-testing --lib --bins` with repository Postgres 91/91, zero skipped, plus both migration prerequisites. `mise run bench:smoke` exited 0 in 85.63 seconds without `--allow-dirty`, passing all eight dispatched invocations (three Bifrost L1 experiments plus other engine/level combinations) with histogram counts, CPU/P and work. Workspace formatting check, dependency gate and range whitespace check passed. Candidate source stayed unchanged.

Workspace lints and historical RED/diagnosis evidence remain implementation-reported; this review did not mutate source to rerun RED or repeat full lints. No full L1/L2 capacity suites, baseline qualification, profiling or platform matrix ran. Those broader obligations remain with integrated change review. Current successful v2 measurements can still misattribute work/latency and lack the full distributions needed to audit the result.

## Verdict and handoff

**FIX_REQUIRED** — **FIND-TASK-004-1**, **FIND-TASK-004-2**, **FIND-TASK-004-3**, **FIND-TASK-004-4**, **FIND-TASK-004-5**, **FIND-TASK-004-6**.

One self-contained remediation: `TASK-004-R1-complete-cohort-timing-evidence.md`, routed to `$wyrd-implement`. No implementation, commit, merge, push or deployment occurred. The next review checks closure, the prior-to-new candidate diff and affected paths, carrying forward source-backed unchanged acceptance rather than restarting discovery.
