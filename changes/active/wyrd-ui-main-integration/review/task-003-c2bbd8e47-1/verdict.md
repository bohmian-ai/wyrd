# TASK-003 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base `695ca914df61a8683059f6f32279ec2148424e06`; immutable candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.
Approved spec `changes/active/wyrd-ui-main-integration/spec.md` revision 12; original task `tasks/TASK-003-shared-ui-foundation.md` in that packet.
Prior verdict/ledger and R1 task/evidence: `../task-003-ab1d2499f-1/`.
Full cumulative subject, attribution/exclusions and starting map: [subject.md](subject.md). The candidate and tracked implementation source stayed unchanged.

## Independent evidence

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | PASS |
| security domain-rev | [domain-review-security.md](domain-review-security.md) | PASS |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | Two retained findings; original seven closed |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups proposals and records the follow-up trigger: competing producer-caption versus renderer-key corrections for the same aggregate collision. Focused follow-up and final independent validation selected the existing producer's label boundary, with no new public identity field. The orchestrator includes only independently CONFIRMED/REVISED findings.

## Prior findings and reconciled acceptance

| Obligation / prior finding | Candidate implementation and proof | Result |
|---|---|---|
| Explicit panel range precedence, FIND-003-1 | Shared loadChart selects input range first; exact brush/reset and relative-range assertions | CLOSED |
| Duplicate-column binding, FIND-003-2 | Validation and stat/plot share first-match columnIndex; both conflicting type orders and table preservation tested | CLOSED |
| Signed chart geometry, FIND-003-3 | Signed finite domains/zero baseline/nonnegative rects and label floor; mounted tests plus recorded browser evidence | CLOSED |
| Twenty-series threshold/style coverage, FIND-003-4 | MAX_SERIES=20 and twenty marker/dash pairs; 5/20/21 mounted tests | CLOSED for original cause; distinct caption defect below |
| CardHeader action outcomes, FIND-003-5 | Named states reach controls/ActionNote and authoritative selection; direct and WyrdView tests | CLOSED |
| Startup cancellation, FIND-003-6 | Acquired aborted stream closes before collect; active and post-collect checks preserve existing SDK owner | CLOSED |
| Canonical public errors, FIND-003-7 | Generated canonical metadata reused by all local producers and complete HTTP responses; canonical tests/codegen pass | CLOSED for original cause; new native import regression below |
| All supported public exports work from package contents | Native Node public loaders/MCP imports fail during JSON module linking | FAIL: FIND-003-8 |
| Overflow series render truthfully for arbitrary accepted text | Real `other` survives ranking alongside identically captioned synthetic sum, breaking keyed loops and attribution | FAIL: FIND-003-9 |
| Browser/native separation, strict document/catalog authority and isolated panel lifecycle | Complete source/consumer review, package build and focused checks | PASS within inspected scope |
| Exact registry reads/dependencies/paging, six SDK/MCP reads, server/session tenancy and upstream error identity | Cumulative source traces and historical real-server/Rust evidence; no new domain/security finding | PASS within recorded scope |
| Four-host foundation and original individual visual approval | Original gallery/example/AppBridge evidence and supplied 2026-10-20 human acceptance; fresh UI/example builds | PASS within recorded scope, subject to current defects |
| Non-goals and unrelated changes | Scope-attributed cumulative diff: no saved views/Ask/A2UI/production MCP registration/npm publication; TASK-027 reviewed separately | PASS |
| Linux native/image closure and live degraded Node/MCP parity | No execution evidence supplied; shared collector source supports parity only | UNVERIFIED, never inferred PASS |

Full original obligation matrices and exact sources are preserved in both implementation reports; this matrix reconciles them with independent validation. The signed-domain initial RED was supplied by the prior reviewer and honestly recorded; current meaningful regression assertions support closure without manufacturing another failure cycle. The example's README explicitly uses its owning UI workspace and --filter; direct-cwd resolution is not retained as a finding.

## Validated ledger

| Stable ID | Status | Class | Correction boundary |
|---|---|---|---|
| FIND-003-8 | CONFIRMED | REGRESSION | Standard JSON import attribute at contract.ts runtime import; native public-export proof |
| FIND-003-9 | REVISED | INCORRECT | Collision-safe aggregate caption at timeSeries producer; all sibling render/text consumers |

[findings-validation.md](findings-validation.md) records source-local discovery IDs, full producer/caller traces, independent reproductions, smallest complete corrections and rejected correction alternatives. Original IDs 1–7 remain closed; these are independently diagnosed new causes rather than relabeled prior findings.

## Verification and disposition

[verification.md](verification.md) records all nine exact selectors passing, UI/example workspace checks/builds and codegen:check green, with unchanged HEAD/tracked source. Behavior reviewer independently reran the ten-test file. Native import and caption probes reproduce paths those bundled tests miss. Historical live infrastructure, Linux/images and live degraded-terminal parity were not rerun or promoted to passes.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R2-native-import-and-series-captions.md](TASK-003-R2-native-import-and-series-captions.md) directly to `$wyrd-implement`; no new material design decision is required. Review makes no source changes, merge, push or deployment. Next review must retain original task, both remediation rounds, prior ledgers and the complete cumulative candidate.
