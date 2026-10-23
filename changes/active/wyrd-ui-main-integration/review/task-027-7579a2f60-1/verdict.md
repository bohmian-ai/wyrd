# TASK-027 cumulative R3 re-review — FIX_REQUIRED

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
Cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; immutable candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`.
Approved specification `changes/active/wyrd-ui-main-integration/spec.md` revision 13; original task `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md` (revision-12 layout obligations unchanged by revision 13).

[subject.md](subject.md) records attribution, navigation, authorities and prior packets. The complete base-to-candidate range was reviewed. Fresh range-diff confirms all previously reviewed implementation maps unchanged through the rebase; the only unequal mapped commit changes packet links. Old upstream and independent TASK-003 changes, including the three named post-rebase TASK-003 commits, are excluded as findings unless they break layout behavior. No such unrelated issue is included. HEAD, tracked source and tests remained unchanged; this review writes only this new packet. The concurrent TASK-003 packet is separately owned.

## Independent review results

| Role | Report | Result |
|---|---|---|
| behavior-rev | [task-review-behavior.md](task-review-behavior.md) | FAIL, B027-1 |
| invariant-rev | [task-review-invariants.md](task-review-invariants.md) | FAIL, I027-R4-01 |
| repo-rev | [standards-review.md](standards-review.md) | PASS |
| maintainer-rev | [maintainer-review.md](maintainer-review.md) | PASS |
| system-rev | [system-review.md](system-review.md) | FAIL, SYS-027-1 |
| interaction domain-rev | [domain-review-interaction.md](domain-review-interaction.md) | FAIL, INTERACTION-1 |
| fresh focused followup-rev | [followup-review.md](followup-review.md) | RESOLVED |
| fresh ponytail-rev | [findings-validation.md](findings-validation.md) | One independently confirmed finding |

All required reports are complete. [claims-comparison.md](claims-comparison.md) groups the four proposals into one cause. A fresh follow-up was required for the newly exposed generation transition and the unresolved choice between local projection and broader synchronization. It established supported direct-host scope and the sufficient local correction. Standards/maintainer PASS results concern their respective rules/structure and do not falsify the runtime transition. The fresh validator independently reproduced the actual candidate and resolved every proposal against source; agreement was not counted as proof. No orchestrator-originated finding is included.

## Reconciled acceptance and prior closure

| Obligation | Source / verification evidence | Result |
|---|---|---|
| Strict bounded whole-unit layout, width rejected, view-only catalog publication | Existing schema/generator and contract assertions; fresh read-only catalog check in standards report | PASS |
| Fixed 12-column/40px grid, mixed/unplaced measured content, collision push-down without document rewrite | layout.ts geometry, native height binding, current tests and historical physical geometry | PASS; FIND-027-3 CLOSED |
| Complete retained sibling callback and immutable authored input | commit overlays all own completed entries; non-echo/replay/untouched sibling assertions | PASS; FIND-027-1 CLOSED |
| Pointer/keyboard cancel and completion stop producer; availability and teardown cleanup | Shared release before commit early return, cancel, availability/document pre-effects and unmount; terminal assertions and historical native capture proof | PASS; FIND-027-2 CLOSED for original/revised causes |
| Arbitrary accepted panel IDs across retained layouts and changing measured heights | Own-entry reads, raw immutable computed-key writes and Map; prototype-key regression assertions | PASS; FIND-027-4 CLOSED |
| Authored nested values or replacement reference reset local edits/held gestures; unchanged toggles preserve edits | given plus full authored signature at existing reset owner; native reactive proxy tests | PASS; FIND-027-5 CLOSED |
| Layout-only edits/echo do not reload, content changes retain existing load lifecycle | Separate layout-stripped content and stable loader, abort/settle owners; current assertions and focused variant controls | PASS for existing load owner |
| Components fill/scroll/redraw without size inputs | Shared fill/Chart dimensions, unchanged catalog size boundary, historical gallery/artifact evidence | PASS within supplied physical proof |
| Narrow full-width `(y,x)` order, explicit height and no controls, including document reload | Initial narrow state passes; actual candidate replacement with changed IDs throws at ordered comparator | FAIL: FIND-027-6 |
| Shared gallery/example/MCP hosts, per-render MCP retention and light/dark desktop/mobile presentation | Complete host traces, supplied check/build/integration and historical browser/human acceptance | PASS within recorded host evidence; direct Svelte narrow transition fails above |
| No persistence/add-remove controls/collapse/per-viewer memory/dependency/backend or public-boundary expansion | Attributed cumulative diff and independent coverage | PASS |

Full original obligation matrices are preserved in both implementation reviews. Prior findings remain closed for their diagnosed causes from current source, not from green checks alone. FIND-027-6 requires no retained edits or active gesture and is separate from R3's corrected authored-reset dependency.

## Active validated ledger

| Stable ID | Status / class | Summary and correction boundary |
|---|---|---|
| FIND-027-6 | CONFIRMED / REGRESSION | Replacing a narrow view with different valid panel IDs sorts old loaded rows against new placements and throws; repair the existing narrow projection using current placement membership |

Exact location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:137-139`. Current placements follow parsed input synchronously; loaded panels change in the existing post-render effect. The comparator dereferences absent placements for obsolete IDs. At 500px, valid a/b -> c/d replacement throws `Cannot read properties of undefined (reading 'y')`. This is positively supported custom-Svelte document replacement, not new reader add/remove functionality. It interrupts the mounted view, not the server.

The [validated ledger](findings-validation.md) contains the complete writer/caller trace and minimal correction. Use the existing narrow reading-order join and Map membership, retain every current loading/error/invalid-panel row, and preserve wide order and the existing IO/reset/gesture owners. No new product, API, architecture, security or persistent-state decision is needed.

## Verification, limits and disposition

[verification.md](verification.md) distinguishes evidence provenance. Caller-supplied candidate checks are green: package/app check zero errors, tests 40/40, build, real-Postgres integration 2/2. Fresh read-only checks include cumulative whitespace and catalog parity. Discovery, follow-up and validator actual-source memory-only mounts reproduce the unmodified candidate failure. Independent fresh-process source-string variants demonstrate that the local correction preserves replacement/removal/recovery, same-ID refresh, layout-only read counts, current loading/errors, old abort/stale rejection and wide behavior. Those variants are not a persisted fix or regression test. Physical browser geometry/focus/capture, theme and human qualification remain historical evidence; no fresh browser sweep, broad gate, Linux/release or deployment proof is claimed.

Verdict: **FIX_REQUIRED**. Remediation: [TASK-027-R4-narrow-document-replacement.md](TASK-027-R4-narrow-document-replacement.md), routed directly to `$wyrd-implement`. Review does not implement, commit, push, merge or deploy. A subsequent review must retain original task, all prior ledgers/remediations and the complete cumulative candidate.
