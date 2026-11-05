# TASK-003 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Immutable candidate `6aace6e07728e59deb1fcd85ce969b2d0a4260db`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`). Authority: approved `changes/active/wyrd-ui-main-integration/spec.md` revision 13 and original `tasks/TASK-003-shared-ui-foundation.md`.

[subject.md](subject.md) records the cumulative subject, navigation and attribution. This round includes all previous FIND-003-1..12, prior verdicts/remediations and R5 commits `0d60b2b68` / `6aace6e07`. Unrelated PR #205/main and separately accepted TASK-027 layout are excluded as findings; shared foundation seams remain reviewed. The latest fix closes original native Number admission. Two independently validated findings remain elsewhere in the cumulative foundation.

## Independent review and reconciliation

| Role | Report | Discovery result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | PASS; no material proposal |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL; INV-RANGE |
| repo-rev | [standards-review.md](standards-review.md) | PASS; NOTE-STD-1 classified non-blocking |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS; no material proposal |
| system-rev | [system-review.md](system-review.md) | PASS; no material proposal |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | PASS; no material proposal |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED; range reachability/attribution/callers established |
| fresh structured ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; one CONFIRMED and one REVISED finding |

All required reports are complete. [claims-comparison.md](claims-comparison.md) records why follow-up was needed: the unique range proposal exposed a reachable path not explicitly traced in the broad validation PASS conclusions. Source and artifact-free public-entry assertions resolved it. Independent validation then confirmed the path, narrowed ordinary-path acceptance and revised the standards note under the explicit mandatory rustdoc gate. The documentation correction is batched with the behavioral remediation, without a style-only round. No unresolved authority conflict, new material decision or orchestrator-originated finding remains.

## Reconciled acceptance matrix

Both implementation reports preserve detailed matrices. This reconciliation incorporates the independently validated exceptions and proof limits.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Approved portable exports, generated catalog/schema authority, browser/native separation, mountView | Public package entries, type-only imports, generator, renderer and supplied check/codegen/import proof | PASS |
| Invalid view/panel ranges fail visibly through canonical validation with sibling isolation | Relative schema admits an unrepresentable Date; root resolution precedes panel error boundaries; Chart and endpoints propagate the same invalid conversion | FAIL; FIND-003-13 |
| Ordinary explicit/inherited range precedence, duplicate-column binding and exact SQL/binds | Input-first shared loader, first-match columnIndex, exact executed input actions; retained fixed-clock/duplicate assertions | PASS; FIND-003-1/-2 CLOSED |
| Signed plots, twenty distinct series and authored/aggregate identities | Signed geometry, marker/dash pairs, typed tuple grouping and collision-free captions used by plot/legend/tooltip; retained mounted assertions | PASS; FIND-003-3/-4/-9/-10 CLOSED |
| Exact flat Card reads, version paging, copied identity and host-controlled actions | Existing get/list owners, canonical shared formatter, version refs without stale UID, action-state consumers and retained tests | PASS; FIND-003-5/-12 CLOSED |
| Six authorized reads and bounded positional terminal-safe JSON parity | Existing SDK/MCP operations, shared JsonResultBuilder and admission, retained framing/settlement owners; reported real-server proof | PASS within recorded proof limits |
| Original Number admission refuses wrapping/fractional budgets and releases streams | Native Option<f64> -> lossless parse_ceiling -> shared admit_ceilings after stream take; native-backed RED/GREEN, same-stream refusal/fresh-query assertions | PASS; FIND-003-11 CLOSED by R5 |
| Acquisition/active/late cancellation, stopped dependent reads and stale-load fencing | SDK return/native notification, loader abort checks and renderer controllers; retained focused negative assertions | PASS; FIND-003-6 CLOSED |
| Canonical problems and standard native imports | Generated catalog problems, full endpoint refusal payloads and JSON import attribute; retained import/problem assertions | PASS; FIND-003-7/-8 CLOSED; range bypass separately retained |
| Credentials, tenancy, durable ownership and native production packaging | Private selected Rust session transport, existing auth/read/query-only owners and bundle extension; supplied integration/historical packaging evidence | PASS within recorded proof limits |
| Complete Chart/CardHeader/primitives, gallery, custom app and AppBridge artifact | Current components/host source, supplied checks/tests and recorded individual human acceptance | PASS within recorded proof limits |
| Rustdoc and generated parity | Native/queue/client source documentation and generated declarations inspected; one newly added test lacks required Panics content | FAIL; FIND-003-14 |
| Non-goals/reference scope | No Ask service, saved-view store, A2UI adapter, generic writes, per-component routes, production MCP registration or publication introduced by foundation | PASS |

## Active validated ledger and prior closure

| ID | Status / classification | One-line summary |
|---|---|---|
| FIND-003-13 | CONFIRMED / INCORRECT | Accepted unrepresentable relative ranges throw before visible document validation or reject Chart/endpoint loading instead of returning field-specific errors. |
| FIND-003-14 | REVISED / VIOLATION | The newly added browser-session query-only regression lacks the mandatory Panics rustdoc section. |

The full source locations, producer/sibling traces, obligation, consequence, selected correction and focused closure proof are in [findings-validation.md](findings-validation.md). FIND-003-13 originates in the original TASK-003 foundation, independently confirmed against `b0a05de50`, and is distinct from closed range-precedence FIND-003-1. Its correction belongs at existing shared range admission/resolution, using native Date validity and canonical validation states; no arbitrary retention cap or per-host numeric checks are required. FIND-003-14 is documentation-only and shares the same remediation task.

FIND-003-1 through FIND-003-12 are **CLOSED** with current-source evidence in the validation ledger. In particular, R5 preserves the incoming JS Number, rejects invalid originals before integer admission, and releases the taken stream. Defaults, maxima, output-overflow errors, cancellation and terminal behavior remain intact. No earlier issue is renumbered or reopened.

## Verification, disposition and workspace

[verification.md](verification.md) separates supplied green candidate fmt/lints/codegen/N-API/TS-unit/Clippy, UI check/test 40/40 and real-server integration 2/2 from fresh read-only source/Git and follow-up public-entry checks. Builds/test suites were not rerun under the packet-only write restriction. The focused check corroborates schema admission and RangeError with zero reads; mounted root-effect and endpoint consequences are source-traced. Historical human/Darwin evidence remains historical; renewed Linux/images, deployed-browser and live degraded-result qualification are not claimed.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R6-range-validation.md](TASK-003-R6-range-validation.md) directly to `$wyrd-implement`. One bounded task packages both validated findings; no specification revision is needed. Subsequent review must reassess the cumulative original task.

A separate bounded reader reviewed the remediation and applicable specification without code or author discussion, and accurately explained its problem, approach, invariants, proof and requested execution. No comprehension gap or contradiction remained.

Final Git checks confirm HEAD remains `6aace6e07728e59deb1fcd85ce969b2d0a4260db`, tracked and staged diffs are empty, and this packet is the only untracked path. The cumulative diff whitespace check passed. Only reports and the remediation task in this requested directory were written; no source/test changes, staging, commit or push occurred.
