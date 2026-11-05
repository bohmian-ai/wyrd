# TASK-003 — FIX_REQUIRED

## Immutable subject

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base `695ca914df61a8683059f6f32279ec2148424e06`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Approved specification `changes/active/wyrd-ui-main-integration/spec.md`, revision 12; original task `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
Scope and excluded/precursor/successor commits are recorded in [subject.md](subject.md). The complete cumulative range and resulting candidate were reviewed; authorized TASK-027 and unrelated upstream/excluded changes are not findings. HEAD remained pinned and tracked implementation source unchanged.

## Independent review results

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL |
| repo-rev | [standards-review.md](standards-review.md) | FAIL |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | FAIL |
| system-rev | [system-review.md](system-review.md) | FAIL |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | PASS |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | Seven retained findings |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups shared causes and records why focused follow-up was unnecessary. The independent validator traced every proposal; no orchestrator-originated finding is included. Discovery agreement was not used as proof.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Six browser-safe exports, schemas/catalog and version/catalog pins | Package exports/type-only imports, strict schemas/catalog generator; recorded builds/codegen and fresh package build/Svelte checks | PASS |
| Exact inherited versus explicit panel bounds | loadChart precedence; independently reproduced query binds | FAIL: FIND-003-1 |
| Positional duplicate results and truthful chart schema checks | Shared normalization/table paths preserve positions; shapeChart conflicts with first-match plot selection | FAIL: FIND-003-2 |
| Complete numeric chart types and approved series cardinality | Signed scale/bar arithmetic and MAX_SERIES=4; source and independent reproduction | FAIL: FIND-003-3/4 |
| Exact Card identity, dependent reads, paging and clipboard | loadCardHeader and component; recorded live and fresh scenarios | PASS for reads/identity |
| All declared CardHeader action results and pending behavior | Version/tab controls ignore supplied action state; Observe uses existing ActionButton | FAIL: FIND-003-5 |
| Stale result isolation and supported cancellation | Renderer generations supported; Node misses abort during awaited stream startup, independently reproduced | FAIL: FIND-003-6 |
| Stable canonical public errors | Uncatalogued UI identities and reduced handwritten endpoint problems | FAIL: FIND-003-7 |
| BFF session tenancy, shared SDK/MCP reads, browser query-only Bifrost | Independent security/source traces and recorded live/Rust evidence | PASS within recorded scope |
| Four-host foundation and individual visual approval | Built artifact, example/gallery and supplied 2026-10-20 approval; fresh package build | PASS within recorded scope; behavioral failures above remain |
| Linux native/image bundle closure; live degraded terminal parity | Darwin closure only; shared JsonResultBuilder source supports degraded projection, no live degraded result | UNVERIFIED, not a platform implementation finding |
| Non-goals and reference preservation | Scoped cumulative diff and reports; no Ask/saved views/A2UI/production MCP registration/npm publication | PASS |

## Validated ledger

Only independently CONFIRMED/REVISED findings are retained; exact locations, producer/caller traces and proof are in [findings-validation.md](findings-validation.md).

| ID | Status | Class | Observable failure |
|---|---|---|---|
| FIND-003-1 | CONFIRMED | INCORRECT | Inherited range overrides explicit panel bounds |
| FIND-003-2 | CONFIRMED | INCORRECT | Duplicate-name validation and rendering select different columns |
| FIND-003-3 | REVISED | INCORRECT | Signed timeseries/bar data yields invalid SVG geometry |
| FIND-003-4 | CONFIRMED | INCORRECT | Five through twenty series fold prematurely |
| FIND-003-5 | CONFIRMED | MISSING | CardHeader version/tab completion state is discarded |
| FIND-003-6 | CONFIRMED | INCORRECT | Abort during Node stream acquisition leaves query collecting |
| FIND-003-7 | REVISED | VIOLATION | Public UI errors bypass canonical catalog/HTTP projection |

No prior finding ledger or remediation closure exists for this first review. No new material product, security or architecture decision is needed for these bounded corrections.

## Verification and disposition

[verification.md](verification.md) separates freshly observed commands from historical task evidence. All seven UI-library tests, both exact TASK-003 scenarios and UI/package checks passed; package artifact rebuilt. The checks do not exercise all retained failures. Historical real-server/Rust/packaging checks were inspected, not rerun. BFF live evidence directly invokes the handler and does not independently prove deployed browser routing. Human approval is honored without treating it as a waiver of semantic defects. Linux/images, live degraded parity and new browser inspection remain unverified here.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R1-foundation-contracts.md](TASK-003-R1-foundation-contracts.md) directly to `$wyrd-implement`; review itself does not implement, merge, push or deploy. Re-review the complete cumulative task candidate against the original task and this ledger after remediation. Missing platform proof must not be relabeled a pass at later completion.
