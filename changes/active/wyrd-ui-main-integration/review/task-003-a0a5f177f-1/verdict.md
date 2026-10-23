# TASK-003 cumulative R2 re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base `695ca914df61a8683059f6f32279ec2148424e06`; immutable candidate `a0a5f177f23960822222b851716460dec02804b3`.
Approved spec `changes/active/wyrd-ui-main-integration/spec.md`, revision 12; original task `tasks/TASK-003-shared-ui-foundation.md` in that packet.
Prior verdicts/ledgers and R1/R2 tasks/evidence reside in `../task-003-ab1d2499f-1/` and `../task-003-c2bbd8e47-1/`.
[subject.md](subject.md) identifies the full cumulative range, scope exclusions, prerequisite and successor attribution. HEAD and tracked implementation source remained unchanged.

## Independent results

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | PASS |
| security domain-rev | [domain-review-security.md](domain-review-security.md) | PASS |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | One retained new finding |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups three proposals under one producer cause and records why no focused follow-up was needed: complete accepted-domain and sibling traces agree without unresolved source/scope conflict. The separate fresh validator independently reproduced both tuple and null/text manifestations, reconciled prior closure and selected one minimal producer correction. No orchestrator-originated finding is included.

## Prior closure and reconciled acceptance

| Obligation / prior ID | Current source and proof | Result |
|---|---|---|
| Explicit/inherited range, FIND-003-1 | loadChart input precedence; mixed-panel brush/reset and fixed-clock assertions | CLOSED |
| Duplicate-column descriptor selection, FIND-003-2 | Shared first-match columnIndex in validation and render; table remains positional | CLOSED |
| Signed chart domains, FIND-003-3 | Finite signed domains and endpoint bar geometry; retained mounted/browser cases | CLOSED |
| Twenty-series threshold/styles, FIND-003-4 | Correct threshold and twenty cue pairs; 5/20/21 tests | CLOSED for prior cause |
| CardHeader outcomes, FIND-003-5 | Named control states and ActionNote; direct/view tests | CLOSED |
| Query-start cancellation, FIND-003-6 | Acquisition/active/post-collect checks reuse SDK close; focused deterministic cases | CLOSED |
| Canonical errors, FIND-003-7 | Generated metadata at every local producer and complete HTTP problems; preserved upstream identity | CLOSED |
| Native public imports, FIND-003-8 | JSON import attribute; native smoke in existing test owner and fresh direct probe | CLOSED |
| Authored versus synthetic overflow caption, FIND-003-9 | Aggregate caption checked against retained names, including qualification text; mounted/rerender tests | CLOSED for prior cause |
| Every accepted authored group/metric pair preserves measured identity before counting/folding | Caption-coercion/concatenation is used as grouping key; distinct tuples and null/text-null sum together | FAIL: FIND-003-10 |
| Portable exports/browser safety, strict catalog/document authority and error states | Full cumulative source/consumer review; fresh focused tests and package build | PASS within recorded scope |
| Exact Card/dependent/paged reads; six SDK/MCP reads and positional result normalization | Existing shared owners traced; historical real-server/Rust evidence and retained focused cases | PASS within recorded paths |
| Tenant session/read/action trust boundaries, supported cancellation and stale-result isolation | Independent security/system traces plus fresh focused cases | PASS within recorded scope |
| Four-host foundation and individual component visual approval | Existing gallery/example/AppBridge proof and supplied 2026-10-20 approval; fresh UI/example builds | PASS within recorded scope, subject to chart defect |
| Non-goals/reference preservation and separate TASK-027 | Attributed cumulative diff; no persistence/Ask/A2UI/production MCP registration/publication | PASS |
| Linux native/image closure and live degraded-terminal parity | Historical source parity/macOS proof only, no new execution | UNVERIFIED, not inferred code findings |

Full original acceptance matrices are in both implementation reports, reconciled above with independent validation. FIND-003-10 is a new diagnosed source before overflow, not a reopening of the correctly repaired threshold or synthetic aggregate caption. All original IDs 1–9 remain closed with current-source evidence.

## Active validated ledger

**FIND-003-10 — CONFIRMED / INCORRECT.** `packages/ui/src/lib/plot.ts:36-45` uses formatted display text as authored grouping identity. Multi-y group/metric tuples with the same concatenation merge, as do actual null and text null, producing false summed metrics before the twenty-series threshold. Independent validator checked schema/real normalization admission, all callers/sibling consumers and executable reproductions. [findings-validation.md](findings-validation.md) preserves every discovery ID, exact source/evidence and selected minimal correction: typed private tuple identity before summing, followed by attributable unique captions from the same producer.

## Verification and disposition

[verification.md](verification.md) records nine exact focused tests, UI/example check/build and native public-import smoke green at unchanged candidate. R2 ten-test results and original/R1 live/Rust/codegen/browser/human evidence remain supplied historical execution. New read-only probes reproduce paths absent from current tests. No generated input changed in R2, so codegen was not repeated. Linux/images and live degraded parity remain unverified at their owning stage.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R3-authored-series-identity.md](TASK-003-R3-authored-series-identity.md) directly to `$wyrd-implement`. No new product/public-contract/durable decision is needed. Review does not implement, merge, push or deploy; later re-review includes original task, all remediation rounds/ledgers and complete cumulative candidate.
