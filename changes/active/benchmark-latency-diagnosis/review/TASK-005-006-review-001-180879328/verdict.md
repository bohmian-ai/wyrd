# TASK-005 and TASK-006 — FIX_REQUIRED

Both tasks require correction within their approved behavior. No specification revision is required. Ten independently validated findings remain; a shared documentation finding has non-overlapping task subsets. Route the two self-contained remediation packets directly to `$wyrd-implement`.

## Immutable subject and round

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/resource-scaling`.
Base `0c81715f2ac52efd2b105e454ee309724df1655f`.
Candidate `180879328ac9dde94d7504855e821980e5db0f18`, branch `change/resource-scaling`.
First joint acceptance review of the full three-dot range: 32 commits, 137 files, +12,384/−1,934. Original tasks:

- `changes/active/benchmark-latency-diagnosis/tasks/TASK-005-full-path-benchmark-parity-and-optimization.md` (revision 6).
- `changes/active/benchmark-latency-diagnosis/tasks/TASK-006-gateway-local-governance-and-agent-check-latency.md` (revision 7).

Approved authority: `changes/active/benchmark-latency-diagnosis/spec.md` revision 7, retaining TASK-005 revision-6 contract; repository/nested AGENTS and relevant architecture/security/operations authorities govern above task mechanics. Earlier TASK-001 reviews supply no carried-forward acceptance. Pre-rebase evidence IDs were treated as navigation claims, not proof; the current cumulative diff/source establishes review findings. No historical verdict closure is claimed. Untracked TASK-001-review-002 and other change directories were excluded.

Reviewed source stayed immutable. Final HEAD/source status was checked at the pinned candidate; only this new review directory was added. No implementation, commit, push, merge or deployment occurred.

## Independent coverage and results

The orchestrator recorded contract applicability and the common navigation map before spawning discovery in [review-scope.md](review-scope.md). The fresh validator independently accepted that selection and coverage in [findings-validation.md](findings-validation.md).

| Selected role/report | Why required | Discovery result |
|---|---|---|
| [behavior-rev](task-review-behavior.md) | Full task/range acceptance, measurement and public contract/Agent behavior | FAIL |
| [invariant-rev](task-review-invariants.md) | Complex tenant reservation, acknowledgment and shared-state lifetimes | FAIL |
| [repo-rev](standards-review.md) | Cross-tier Rust/Python/TS/SQL/docs/tooling rules and generated surfaces | FAIL |
| [maintainer-rev](maintainer-review.md) | Substantial recorder/cache/schema/Gateway ownership and reuse | PASS |
| [system-rev](system-review.md) | SQL/listener failure, recovery and accepted-work drain | FAIL |
| [domain-rev-data](domain-review-data.md) | Persistence, migration compatibility, RLS and sync concurrency | FAIL |
| [domain-rev-security](domain-review-security.md) | Details/status permission, audit, trust and tenancy | FAIL (required proof gap; no runtime access defect found) |
| [fresh ponytail-rev](findings-validation.md) | Independent source/materiality/correction validation and coverage reconciliation | Both tasks FIX_REQUIRED |

Concurrency expertise is covered by invariant/data roles; no additional domain is materially uncovered. All required selected reports exist. No agent filled more than one role. No acceptance was inferred solely from test counts or reviewer agreement.

[Claim comparison](claim-comparison.md) grouped shared causes. INV-001/DATA-001 consolidate into one migration finding, revised to cover both read directions. Security's exact missing-detail proof supersedes behavior's general absence PASS row; production permission implementation remains supported. No separate followup-rev was needed: independent source validation resolved these questions without a remaining conflict. Validator introduced no new finding requiring separate validation. No prior-finding closure or convergence assessment applies in this first round.

## Validated finding ledger

The following ledger is the orchestrator's disposition of the independent validator's findings. Exact locations, complete producer-to-consumer evidence, materiality, correction burden and focused proof are preserved in [findings-validation.md](findings-validation.md); the linked remediation packets package those decisions for implementation.

| ID | Status / discovery IDs | Classification and violated obligation | Observable defect/gap and selected correction |
|---|---|---|---|
| FIND-005-1 | CONFIRMED / BEH-005-01 | MISSING; REQ-022/AC-018 | Observer qualification explicitly unresolved and two independent-reference proofs uncompleted. Complete existing sequential proofs and comparable canonical qualification, preserving authorization. |
| FIND-005-2 | CONFIRMED / SEC-001 | MISSING; REQ-026/AC-021 | Unknown ID proof does not exercise known incomplete/lost records or direct recording failure. Extend existing real-server negative-flow proofs; no new availability machinery. |
| FIND-005-3 | CONFIRMED / REPO-001 | VIOLATION; mandatory changed-item rustdoc | Bounded constructor/parser/fixture items lack required intent/Errors/Panics docs. Apply exact task-specific subsets once alongside substantive correction. |
| FIND-006-1 | CONFIRMED / BEH-006-01 | REGRESSION; INV-001/003 | Direct public prompt reassignment exports A or B according to earlier reference reads. Preserve stable original/explicit-retarget shared source for lazy projection. |
| FIND-006-2 | CONFIRMED / BEH-006-02 | MISSING; REQ-033/AC-025 | Final three reductions remain unmeasured. Obtain bounded authorized final-candidate canonical comparison after correctness fixes. |
| FIND-006-3 | REVISED / INV-001, DATA-001 | INCORRECT/REGRESSION; REQ-031 and deployment overlap | New/old persisted current-window/concurrency representations are mutually invisible. Add compatible SQL read projections in both directions through additive migration, retaining distinct writes. |
| FIND-006-4 | CONFIRMED / DATA-002 | INCORRECT; REQ-028/029/031 | Clean eviction replaces in-flight tenant sync authority; late baseline can erase acknowledged budget spend. Pin existing sync identity across wait/IO/install/cancellation. |
| FIND-006-5 | CONFIRMED / SYS-001 | INCORRECT/REGRESSION; REQ-030/INV-001 | Failed/global-generation-fenced reload disappears while old policy keeps serving indefinitely. Retain reload ownership through successful installation and use existing unavailable path when needed. |
| FIND-006-6 | CONFIRMED / SYS-002 | INCORRECT; REQ-031/AC-024/026 | Graceful drain swallows first SQL flush failure and exits before deadline. Retry pending contributions through commit/deadline and accurately report outcome. |
| FIND-006-7 | CONFIRMED / INV-002 | MISSING; REQ-030/AC-024 | Required stale/full refusal/recovery branches unproved. Add bounded owner/seam tests for exact pre-dispatch 503 and background recovery. |

No rejected proposal was softened into advice. No speculative Azurite fix, guaranteed speedup, strict global quota or stronger crash durability is prescribed. A spec revision cannot substitute for these implementation/proof remediations.

## Reconciled acceptance matrix

| Obligation | Implementation / verification evidence | Result |
|---|---|---|
| TASK-005 recorder, process/cohort/profile fidelity, controls and safe reductions; AC-013–017 | Behavior matrix traces recorder→transport/provider/fixture→canonical reports, roles and source-supported reductions; recorded tests inspected | Implementation supported; required lifetime/qualification proof incomplete, FIND-005-1 |
| TASK-005 qualified canonical measurements; AC-018 and AC-022 measurement | Task explicitly records U→M misses/INVALID run and skipped census/calibration; later compact comparison is separately disclosed | FAIL, FIND-005-1 |
| Compact direct/status/wait/typed contract; AC-019 | Shared compact outcome/projection, HTTP/MCP/SDK schemas and migrated consumers; behavior/security/repo reports | PASS on source and reported evidence |
| Faithful descriptive details and unchanged canonical storage; AC-020 | Shared authenticated result reader, authored Check decoding, retained input/output/summary and SDK projections; inspected structural/readback proofs | PASS on source and reported evidence |
| Details authority/absence; AC-021 | Common read authorizer/audit/TenantConn/Bifrost supported; required known-incomplete/lost/recording-failure cases missing | FAIL for proof, FIND-005-2; no runtime access defect established |
| SDK/MCP/HTTP/docs parity; AC-022 contract | Shared handle plus Python/TS thin projections/stubs/generator/served contracts and migrated journeys | PASS on source and reported evidence |
| Exact immutable schema preparation and saved Gateway diagnostics; AC-025 components | Hosted/Skald lifetime and effective-schema selection; Verdict/custom data/repair/citation boundaries retained; canonical phase/wait rows | PASS on source and reported evidence |
| Local quota, persistence and failure lifecycle; AC-023/024/026 | Ordinary same-version atomicity/replay supported, but incompatible migration, synchronization eviction, lost reload, early drain and proof gaps remain | FAIL, FIND-006-3–7 |
| Agent optimization correctness and final gains; AC-025 | Prompt first-read projection regression; final reductions explicitly unmeasured | FAIL, FIND-006-1/2 |
| Owner-native bounded implementation/no extra service | Existing Gateway, recorder/experiment, Skald/Vala owners extended; maintainer/report alternatives evaluated | PASS; no DRIFT retained |
| Production bench-only observations | Fresh normal dependency feature tree has no bench; enabled cfg-false, install/heap cfg-gated, phase clock/histogram short-circuit | PASS |
| Mandatory documentation | Independent bounded changed-item inspection | FAIL, FIND-005-3 (both task subsets) |
| Authorization/audit/tenancy/capture/non-goals | Common authenticated reads; FORCE RLS/TenantConn; unchanged capture reservation and supported cancellation semantics; crash/deadline loss and approximate quotas retained | No separate runtime boundary defect established; findings above still limit acceptance |

## Verification limits and next work

This was a static independent review. Only repo-rev's read-only default feature tree was freshly executed. Prior reported format/lints/codegen/dependency/isolation/Python/TS checks, server PG 593/593, Gateway journeys and post-rebase Skald/Vala/authoring results were inspected as implementation evidence, not rerun. No build, test suite, benchmark, host load, profiler or external provider call was run. Task evidence remains honest about host confounding, c64 noise, missing final measurements and unconfirmed Azurite diagnosis. Those facts neither prove a production regression nor close an explicit acceptance gate.

TASK-005: **FIX_REQUIRED**, findings FIND-005-1–3; [TASK-005-R1-qualified-measurements-and-details-proof.md](TASK-005-R1-qualified-measurements-and-details-proof.md).

TASK-006: **FIX_REQUIRED**, findings FIND-006-1–7 plus its subset of shared FIND-005-3; [TASK-006-R1-governance-lifecycle-and-agent-correctness.md](TASK-006-R1-governance-lifecycle-and-agent-correctness.md).

Remediation review should check these findings, changes since this candidate and affected callers/invariants, carrying forward unchanged validated acceptance. Broad journeys run at integrated change review; the packets prescribe narrow closure proof and retain separate benchmark authorization.
