# TASK-003 cumulative re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Immutable candidate: `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`.
Cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533` (parent of `ff4145ac6`).
Authority: [approved spec revision 13](../../spec.md), [original TASK-003](../../tasks/TASK-003-shared-ui-foundation.md), applicable repository/architecture owners. [subject.md](subject.md) records navigation, history and exclusions.

Verdict: **FIX_REQUIRED**. Route [TASK-003-R7-range-contract-and-sql-binds.md](TASK-003-R7-range-contract-and-sql-binds.md) directly to `$wyrd-implement`. Two bounded independently validated input-planning defects remain; no specification revision is required.

## Immutable cumulative subject

The review covers the complete base-to-candidate TASK-003 foundation, not just R6 commits `6eb137e7b` and `21dab5d11`. It includes the original task, all six prior packets, FIND-003-1..14 and R1..R6 remediations. The latest fix locates changed range admission/resolution and client test rustdoc. Unrelated PR #205/main content and separately accepted TASK-027 layout are excluded as findings; shared foundation seams remain covered. The original task's historical revision-10 link is interpreted under approved revision 13, including deferred Ask/A2UI context. No source or test files were modified.

## Independent reports and reconciliation

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL; BEH-1 proposed |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL; retain FIND-003-13 |
| repo-rev | [standards-review.md](standards-review.md) | PASS; no material standards findings; -14 closed |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS; no material maintainer findings |
| system-rev | [system-review.md](system-review.md) | FAIL; SYS-1 retains FIND-003-13 |
| security/tenancy domain-rev | [domain-review-security.md](domain-review-security.md) | PASS; no material domain findings |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED; range postcondition and revised SQL reachability |
| fresh structured ponytail-rev | [findings-validation.md](findings-validation.md) | FIX_REQUIRED; -13 CONFIRMED, -15 REVISED |

All required discovery reports and independent validation are complete. [claims-comparison.md](claims-comparison.md) explains why a fresh focused follow-up was necessary: behavior/maintainer closure of the former Date exception did not address the absolute-schema postcondition identified by invariant/system reviews; the unique SQL claim also needed source and valid-server-path tracing. Follow-up corrected the SQL example to include a Bifrost table because the literal-only example independently violates the existing SQL floor. Validation independently confirmed the corrected path, rejected the original example as sufficient end-to-end proof, and retained the shared planner defect. Findings are grounded in source, not agreement counts. No unresolved conflict or orchestrator-originated finding remains.

## Reconciled acceptance matrix

Detailed matrices and source locations remain in the implementation reports; this matrix reconciles their exceptions using independent validation.

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Six approved package exports, generated schema/catalog dispatch, browser/native separation, mountView | Package source/generator/entries, retained import/catalog assertions and supplied UI checks | PASS |
| Root/explicit relative ranges resolve into the existing valid absolute contract or canonical field error on every host | R6 fixes Date-invalid exception but admits Date-valid expanded-year output refused by AbsoluteRangeSchema; source probes and caller traces establish local/HTTP divergence | FAIL; FIND-003-13 |
| Explicit/inherited range precedence, first-position duplicate columns, one shared clock | Shared loader/context and shaping/plot column owner; fixed-clock/duplicate assertions retained | PASS for valid ranges; -1/-2 closed |
| Runtime authored read-only SQL remains unchanged with correct typed range binds | chartParams raw regex treats quoted/comment text as placeholders and prevents otherwise supported range-free table load before executor | FAIL; FIND-003-15 |
| Complete Chart types/states, signed plots, twenty series and distinct typed/aggregate captions | Chart/plot/loaders and retained signed/5-20-21/caption/tooltip assertions | PASS; -3/-4/-9/-10 closed |
| Exact flat Card identity, version paging/failures, copied reference and host-controlled actions | Existing get/list, shared formatter, header state and action consumers; retained negative/action tests | PASS; -5/-12 closed |
| Six authorized SDK/MCP reads, positional normalized terminal-safe results and shared exact budgets | Existing domain facades, shared JsonResultBuilder/admission, protocol/settlement owners and supplied real-server proof | PASS within recorded limits |
| Lossless original Number ceiling validation and refusal releases stream | Native f64 parse before shared admission after stream take; R5 native-backed refusal/closed-stream/fresh-query proof retained | PASS; -11 closed |
| Failed prerequisites stop reads; acquisition/active/late cancellation and stale-result fencing | Node/native/loader/renderer lifecycle source and retained focused negative assertions | PASS; -6 closed |
| Canonical problems and standard public Node import | Generated problems, full endpoint envelopes, standard JSON import attribute and retained smoke/assertions | PASS at prior diagnosed causes; -7/-8 closed |
| Credentials, tenancy, session authority, server ownership and packaging | Request-scoped private selected session, existing Rust query-only construction, adapters and existing build owner; supplied/historical proof | PASS within recorded limits |
| Components/primitives, gallery, custom app and AppBridge artifact; individual visual checkpoints | Cumulative source, supplied tests and recorded historical human/artifact acceptance | PASS within recorded limits, except shared range/SQL failures above |
| Rustdoc and generated parity | Required client test Panics section added without changing assertions; standards coverage includes changed Rust helpers/tests | PASS; -14 closed |
| INV-001..007, non-goals and attribution | Existing durable/security/query owners retained; no new Ask, saved-view store, A2UI adapter, generic writes, per-component routes, production MCP registration or publishing | PASS except bounded validated behavior defects |

## Active validated findings ledger

| Stable ID | Status / classification | One-line summary |
|---|---|---|
| FIND-003-13 | CONFIRMED / INCORRECT | Accepted relative ranges can resolve outside the existing RFC 3339 absolute contract, producing different behavior across local/MCP and HTTP hosts. |
| FIND-003-15 | REVISED / INCORRECT | Literal/comment SQL parameter text falsely requires a range and blocks valid range-free table queries before execution. |

[findings-validation.md](findings-validation.md) preserves exact locations, discovery IDs, violated obligations, full producer/sibling traces, consequence, selected minimum correction and focused closure proof. FIND-003-13 stays at shared schema/resolution with existing canonical validation paths; do not widen the date contract or add host guards. FIND-003-15 stays at existing chartParams recognition, preserving Rust SQL authority and authored bytes; no new parser service, dependency or public input is needed. One R7 task packages both corrections in the portable input-planning write set.

## Prior-finding closure

FIND-003-1..12 remain **CLOSED** with current-source evidence in the independent validation table: precedence, positional columns, signed plotting, twenty-series styling, host actions, cancellation, canonical errors, native imports, aggregate captions, typed grouping, original Number budgets and exact Card formatting. FIND-003-14 is **CLOSED**: the changed client test now documents its panic conditions and its assertions are unchanged. FIND-003-13 is **ACTIVE** at its original shared range invariant: fixing the old `1000000000000d` exception does not prove that accepted `100000000d` resolves within AbsoluteRangeSchema. FIND-003-15 is new, cumulative-foundation-attributable and distinct from earlier range precedence. No closed issue was renumbered.

## Verification and limits

[verification.md](verification.md) separates supplied green candidate UI check/test 40/40/catalog, fmt/lints, focused client nextest and real-server integration 2/2 from fresh read-only Git/source and artifact-free source probes. Those probes reproduce invalid absolute output and false parameter recognition; source ordering proves propagation. The supplied green tests miss the counterexamples and do not override source evidence.

No builds/test suites/codegen were rerun under the packet-only write restriction. Historical human, Darwin and host artifact evidence remains historical; renewed Linux/images, deployed-browser, live degraded-result or cross-platform qualification is not claimed. No broad aggregate/full journey requirement is added to pure remediation; combined journey proof belongs to change review.

## Packet completion

A separate bounded reader reviewed R7 and spec revision 13 without code or other reports for that pass. It accurately explained the problem, selected correction owners and limits, invariants, observable proof and requested implementation, and found no material ambiguity or contradiction. No additional approval or design round is required.

Final Git checks confirmed HEAD remains `21dab5d11e484ffceaa4718fc7370c7d6c67dc75`, tracked and staged diffs are empty, and this packet is the only untracked path. All thirteen packet reports/task artifacts exist and packet whitespace checks pass; cumulative Git diff whitespace check also passed. Only this requested packet directory was written. No source/test changes, staging, commits or pushes occurred.
