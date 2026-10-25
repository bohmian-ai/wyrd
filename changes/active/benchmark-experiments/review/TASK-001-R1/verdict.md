# TASK-001 acceptance verdict

**FIX_REQUIRED**

## Immutable subject

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/benchmark-experiments`.
Base: `0739bbf38af7cc05bbfefb09850490c55432ec2d`.
Candidate: `aaa152dd4243d57752b73d3a4e9c140bccc7d171`.
First full review of `git diff <base>...<candidate>`; no prior accepted findings, closure or evidence. Approved specification: `changes/active/benchmark-experiments/spec.md`, revision 5. Original task: `changes/active/benchmark-experiments/tasks/TASK-001-benchmark-experiments.md`.

The candidate remained unchanged. Two pre-existing uncommitted repairs in telemetry/lib.rs and testing/capacity.rs were inspected separately and left unchanged; their diff digest is recorded in verification.md and was rechecked before this verdict. No source edits, commits, benchmark runs, merge or push occurred. Review reports are the only writes.

## Scope, independence and coverage

The full 62-file implementation range was reviewed, not its summary. scope.md records the shared starting navigation map and applicable authority. No touched directory has nested AGENTS. Root rules, approved spec/task, spec-driven workflow, maintainer/Rust/agent-surface guides and relevant architecture/testing references govern. CodeGraph preceded code discovery. Reviewer reports expanded the map independently.

| Selected role | Report | Result and coverage |
|---|---|---|
| behavior-rev | task-review-behavior.md | FAIL; all REQ/INV/AC rows, real engine paths/matrices, historical baselines and proof |
| invariant-rev | task-review-invariants.md | FAIL; exact timing, matching/completeness, collection bounds, helper/resources and work windows |
| maintainer-rev | maintainer-review.md | PASS within scope; existing workflow/launcher/fixture owners reused; no independently retained DRIFT |
| repo-rev | standards-review.md | FAIL; cross-surface mandatory repository rules, bounded rustdoc gate |
| system-rev | system-review.md | FAIL; lifecycle, interruption, drain, profiler/budget and failure propagation |
| followup-rev | followup-review.md | RESOLVED; two specific Verification sibling gaps, independently checked before expansion |
| fresh ponytail-rev | findings-validation.md | FIX_REQUIRED recommendation; independently traced every proposal, materiality and correction burden |

All selected reports are present. No security/data specialist was selected because the range instruments existing boundaries; behavior/system/repository review covered preservation of production security/durability. claim-comparison.md records grouping and follow-up selection. Initially no conflict justified another discovery pass. Validation exposed two bounded sibling coverage gaps (Verification docs and frozen-denominator work); one fresh focused follow-up resolved them. No comprehensive rediscovery or unresolved conflict remains. No convergence assessment applies on this first review.

## Reconciled acceptance matrix

Static acceptance below carries only the source-backed evidence identified in discovery/validation; it is not real qualification.

| Obligation | Implementation/verification evidence | Result |
|---|---|---|
| REQ-001 declaration | Common sealed declaration exists; Gateway replaces timing controls; findings 8/12 | FAIL |
| REQ-002 validity | Shared injected checks pass; helper separation and complete bounded collection missing, 4–6 | FAIL |
| REQ-003 repetitions | Required phase/window declarations and runner tests pass; real proof absent, 12 | FAIL |
| REQ-004 verdict precedence | Shared judgment/dispatch tests and source agree; invalidity does not erase valid failures | PASS within tested/static scope |
| REQ-005 complete stress judgment | Declared phase criteria exist; real overlap proof and lifecycle closure lacking, 9/12 | FAIL |
| REQ-006 accounting | Required Bifrost operands missing, collector charged to driver, post-window work counted, 2/4/7 | FAIL |
| REQ-007 exact server samples | Export representation broken; required collection/association incomplete, 1/2/5/6 | FAIL |
| REQ-008 full fast L1 | Full matrices declared; broken exports force repeated waits; valid complete budget unproved, 1/12 | FAIL |
| REQ-009 L2 | Ingest calibration/mix present; offered query schedule absent and real proof missing, 3/12 | FAIL |
| REQ-010 commands/docs | Required opt-in commands and sequential forwarding present; exact dispatch test passes | PASS static; actual qualification pending 12 |
| REQ-011 baselines | Historical provenance/exclusions inspected; no broken-new-run reference found; limits in behavior report | PASS inspected scope; no baseline change justified |
| REQ-012 regression/noise | Conservative operators/spread and deterministic boundaries pass | PASS tested/static scope |
| REQ-013 history | Append-only/version/privacy tests pass; Gateway control attribution incomplete, 8 | FAIL |
| REQ-014 report | Shared persisted-report tests pass; missing Bifrost facts cannot be regenerated, 2 | FAIL |
| REQ-015 reuse/profiling | One workflow and existing owners accepted; optional save can change budget/drain, 10 | FAIL |
| REQ-016 retirement | Source/consumer inventory confirms obsolete tasks/owners/resource modes retired | PASS static |
| INV-001/002/003 | Inspected production paths preserve checks/audit/durability; original regression execution unavailable, 12 | FAIL proof |
| INV-004 | Accounting and required metric defects, 2/4/7 | FAIL |
| INV-005 | Frozen criteria and INVALID precedence exist; complete timing claims fail, 5/6 | FAIL |
| INV-006/007/008 | Inspected synthetic/privacy, no measured compilation, append-only/version behavior and tests | PASS inspected scope |
| INV-009 | Portable plain-child/sysinfo paths accepted; required Linux execution proof missing, 12 | FAIL proof |
| AC-001/005/007/010 | Exact real measurements, full engine qualification and original production proof absent, 1–7/9/12 | FAIL |
| AC-002/003/006/008/009 | Source plus deterministic baseline/noise/history/dispatch and reuse/retirement evidence accepted within stated limits | PASS inspected scope; does not waive failed obligations |
| Mandatory changed-item rustdoc | Eleven cited lifecycle/test items fail explicit gate, 11 | FAIL |

## Independently validated findings

The complete source evidence, reachability, materiality, burden, selected correction and focused closure proof for each ID are in findings-validation.md. Only confirmed/revised findings are retained here.

| Stable ID | Status/class | Required correction boundary |
|---|---|---|
| FIND-TASK-001-1 | CONFIRMED / INCORRECT | Signed integer timing at shared telemetry producer; real exporter/collector regression |
| FIND-TASK-001-2 | REVISED / MISSING | Required Bifrost operands, settled storage metrics, request-correlated scan/time evidence and persisted projections |
| FIND-TASK-001-3 | CONFIRMED / MISSING | Declared independently scheduled L2 query arrivals under unchanged mix/client bound |
| FIND-TASK-001-4 | CONFIRMED / VIOLATION | Existing Collector as separately measured plain-child helper; honest helper identity/resources/health |
| FIND-TASK-001-5 | CONFIRMED / MISSING | Finite incoming/retained collection and diagnostics; explicit attributable overflow INVALID |
| FIND-TASK-001-6 | REVISED / INCORRECT | Exact offered-write timing correlation rather than boundary-count minima |
| FIND-TASK-001-7 | REVISED / INCORRECT | Bifrost and Verification measured W/B match frozen C/T; retain late work/errors separately |
| FIND-TASK-001-8 | CONFIRMED / MISSING | Preserve shared timing controls in both Gateway declarations |
| FIND-TASK-001-9 | CONFIRMED / INCORRECT | Own/cancel Gateway L2 scheduler and request descendants before drain |
| FIND-TASK-001-10 | CONFIRMED / INCORRECT | Optional capture saving cannot postpone drain or determine workload budget verdict |
| FIND-TASK-001-11 | REVISED / VIOLATION | Batch eleven cited mandatory rustdoc corrections with functional work |
| FIND-TASK-001-12 | REVISED / MISSING | Original final Linux release/production qualification remains required, without a new broad remediation suite |

Exporter/budget reports share one producer cause. Scan evidence is consolidated into Bifrost metrics. Verification follow-up expands existing work-window and documentation causes, not new finding IDs. No proposal was rejected outright; narrowed corrections replace broader suggestions. No material spec revision is needed for selected outcomes; private implementation remains free. A new public/architecture/resource decision beyond approved boundaries still invokes the original stop condition.

## Executed verification and limits

verification.md records exact commands, identities and limits. The pre-existing signed-field export regression passed (1 test). Nine shared runner tests and three exact engine declaration/dispatch tests passed. `mise run lints` passed both checks. `mise run fmt:check` failed only because the outside-candidate export test needs one-line rustfmt layout. No format rewriting occurred during immutable review.

These 13 test passes and Clippy do not prove the committed producer or actual benchmark paths. No real release smoke/L1/L2 or production journey ran here. Prior raw timing artifacts were deleted; no reconstruction is claimed. Original required real Linux proof is unavailable, not presumed never executed. macOS arm64 and Linux arm64 hardware runs remain optional.

Bifrost declares 20 measured L1 steps: 700 seconds mandatory windows, roughly 16 seconds normal collection settling and 20 seconds host checks leave about 164 seconds for setup and other lifecycle work. Malformed export can add about 2,400 seconds of collection waits to full coverage. Fix that cause before attributing setup slowness or reducing fixtures. A valid full run must establish remaining headroom. No baseline contamination from the broken new run was found; no threshold/baseline tuning is justified.

## Remediation handoff

Route `TASK-001-R1-restore-benchmark-evidence.md` directly to `$wyrd-implement` under approved revision 5. It packages findings 1–11, preserves finding 12 as the original final qualification obligation, and includes separately labelled human-directed mandatory smoke, valid-run budget diagnosis and ordered clean-tree L1 runs. Human additions have no retrospective FIND IDs. Original task is not complete; no PASS is implied by a source-only fix.

Review completed without changing its subject. No implementation, smoke or benchmark rerun was performed. No commit/push occurred.
