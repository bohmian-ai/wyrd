# TASK-027 independent structured Ponytail validation

Validation complete. Deduplicated ledger: **three retained findings**, all bounded by approved revision 12. No prior FIND ledger was supplied. This report does not implement changes or issue the orchestrator's verdict.

## Subject, evidence and coverage

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate/unchanged HEAD: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Authority: `changes/active/wyrd-ui-main-integration/spec.md` revision 12, REQ-030/036 and AC-015; original `tasks/TASK-027-view-layout.md`; AGENTS.md, agent rules, reference router, spec-driven development, maintainer style, applicable TypeScript and UI testing/interaction guidance. Repository ownership and approved constraints take precedence over tests or implementation descriptions.

Read all six discovery reports, `followup-review.md`, `claims-comparison.md`, `subject.md`, and `verification.md`. Independently inspected cumulative range inventory and complete task-owned UI/example diff, and read the complete owning `layout.ts`, `WyrdView.svelte`, and `view.ts` functions. Traced schemas/catalog projection, all production layout imports, all `onlayout` sites, input/kept/draft writers, placement/style/edit consumers, grip/corner markup and CSS, MCP bootstrap/remount, gallery/example feedback, and all four focused scenario bodies. The cumulative range includes merged unrelated upstream changes; these are provenance context, not TASK-027 implementation. No `.codegraph/` exists. Source was not modified.

The discovery union has exactly three material causes. Standards proposes no material finding; maintainer proposes the callback issue only. Existing return-annotation/task-name style notes are nonblocking and are not promoted into findings. The inline remoteLoader concern was already falsified by installed compiler evidence and has no remaining proposal. No optional framework, new dependency, persistence, or downstream host repair is justified.

## Claim dispositions

| Discovery IDs | Disposition | Final ID |
|---|---|---|
| B027-01, I-027-01, MNT-027-01, SYS-027-2, I-01 | CONFIRMED: one callback serialization cause | FIND-027-1 |
| B027-02, I-027-02, SYS-027-1, I-02 | CONFIRMED: one active-pointer cancellation cause | FIND-027-2 |
| B027-03, FUP027-01 | REVISED: fixed-grid coordinate mismatch confirmed; narrow the claim and closure to distinguish intrinsic content height from occupied rows | FIND-027-3 |

No proposed material finding is rejected. Revisions below prevent an overly broad correction or an assertion that collision push-down must be written into the document.

## FIND-027-1 — CONFIRMED / INCORRECT

**Obligation:** REQ-036 and AC-015; task scenario 3; `onlayout` must receive the whole updated document after each completed edit. Host feedback is optional and MCP edits survive within the current render.

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:134-139`; retained-state/render consumers at lines 95-111. Public projection: `view.ts:27-46`; shipped non-feedback host: `packages/ui/src/app/main.ts:23-26`.

**Independent source trace:** `commit` installs `kept[id] = layout` after clearing the draft. `layouts` overlays every retained entry on parsed input. But callback construction selects only `panel.id === id` from the original parsed panels. Complete A then B without changing input props: rendering uses A-new and B-new; callback two contains A-original and B-new. Reapplying that callback triggers the parsed-document reset and drops A-new. There is no requirement to echo props after every callback; mountView offers only destroy, and MCP itself uses a no-op callback. Thus this is supported production behavior, not a hypothetical misuse or persistence request.

**All shared writers/consumers:** keyboard Enter, grip blur and pointer-up converge on `commit`; keyboard arrows and captured pointer movement produce the draft; Escape/pointercancel discard it. The parsed-document effect is the other writer of retained state. Gallery/example immediately replace `doc` and mask the omission; mountView/MCP rely on retained state. No other callback serializer or layout store already solves the gap.

**Consequence:** a host that accepts the latest advertised whole-document callback can revert an earlier completed sibling edit. Display and public output disagree.

**Selected smallest correction:** retain the existing WyrdView owner and `kept` state. Emit all completed retained authored layouts, including the current completion, over the original panel fields. Leave untouched panels unmodified, including absence of layout. Do not serialize `placements` wholesale: collision displacement and measured auto occupancy remain render-only. Preserve new-input reset, document order, input immutability, and load identity. A host-side merge, mandatory feedback protocol, new store or API is unnecessary.

**Focused closure:** extend the existing editing fixture with a recording handler that does not echo props; complete changes to two different panels, assert callback two contains both completed authored layouts, one call per completion, unchanged input and unchanged load count. Reapply callback two and verify both edits survive without reads. Include an untouched colliding/unplaced sibling to prove render-only displacement/measurement is not persisted. Expected callback layouts describe authored edits, not necessarily every collision-resolved on-screen rectangle.

## FIND-027-2 — CONFIRMED / INCORRECT

**Obligation:** REQ-036 editing and task scenario 3 require cancellable pointer/keyboard move and resize; Escape restores the pre-edit state without reporting completion.

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:145-178`, especially Escape at 153-154 and captured move/up at 167-174; event entries at 258-265 and 281.

**Independent source trace:** focus the grip, begin a pointer drag, move, press Escape, move again, release. `key` clears `draft`, but its only mutation cannot remove or invalidate `drag`'s registered closures. The next captured pointermove unconditionally produces a new draft from `from`; pointerup calls `commit`. This requires no host mutation or race. Pointercancel does remove listeners, but Escape never reaches that lifecycle. A grip can already own focus, making the failing path unambiguous. Unfocused starts and the presentation-only corner additionally show that handle keydown alone cannot provide reliable pointer-session Escape handling.

**Sibling coverage:** move grip and resize corner share `drag`; Enter, blur and pointer-up share completion; keyboard-only Escape and pointercancel are distinct cancellation paths. All hosts project this owner. Existing keyboard-only cancellation assertions do not exercise the surviving pointer producer.

**Consequence:** a cancelled interaction reappears and reports completion on release. In feedback hosts that callback replaces the host document; in MCP the unwanted edit remains until the next render.

**Selected smallest correction:** make the active pointer lifecycle owned by the existing renderer cancellable through Escape, with Escape reachable during both move and corner resize regardless of starting focus. End its event producers/listeners and pointer capture as appropriate, discard its draft, and prevent subsequent events from that interaction from committing. Reuse the existing end/cancel responsibility instead of adding gesture machinery or guarding each host. Preserve ordinary keyboard Enter/blur, pointer-up once-per-completion, pointercancel, earlier completed edits and zero reloads. Any active-interaction guard belongs here because this renderer owns the gesture lifetime; a guard in callback consumers would be too late.

**Focused closure:** for move and resize, begin drag, preview movement, Escape, further movement, pointerup; assert original effective layout and no callback. Exercise a previously focused grip and an unfocused pointer start. A fresh independent drag must still complete once; ordinary keyboard blur/Enter and pointercancel must retain their behavior. Use existing component event machinery where possible and the existing browser harness for actual focus/capture behavior; no new dependency or broad journey lane is required.

## FIND-027-3 — REVISED / INCORRECT

**Obligation:** REQ-036's 40 CSS px rows, mixed placed/unplaced documents and content-height unplaced panels; AC-015 valid positions; task scenario 2. The task does not authorize variable row pitch.

**Location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/layout.ts:28-35`; `WyrdView.svelte:108-130`, `:164-168`, and `:310-315`.

**Independent source trace:** `place` assigns every intrinsic panel h=1 regardless of content. `boxStyle` renders one grid-row span with no fixed height for that panel. `grid-auto-rows: minmax(40px, auto)` consequently allows that one row to grow to intrinsic content height. Every subsequent row boundary incorporates that growth, while `rowsHeight` and pointer dy use fixed 40+12 pitch. The sole auto measurement in `current` runs only when an edit starts; it cannot maintain normal placement. The producer is the missing occupied extent for auto content, not Chart/CardHeader data loaders or the later fixed panel. No sibling renderer or geometry service exists to reuse; WyrdView already owns DOM width/height observations and pure `place` owns collision resolution.

The focused follow-up's browser evidence is consistent with this complete path: an intrinsic header of 72.1875px followed by fixed y=4 produced top 240.1875px, versus 208px after converting the header to a fixed two-row layout; an intrinsic Chart of 294px produced fixed y=4 top 462px. Computed tracks grew only the intrinsic row. Follow-up checked matching served/built artifact SHA-256 and did not change application CSS/source. This is useful corroboration, not a substitute for source validation. I did not independently rerun its browser session.

**Consequence and revision:** noncolliding authored y coordinates depend on preceding content and can jump after an earlier auto panel becomes fixed; edit snapping and physical placement have different pitch. Do not claim overlap from these examples: stretched CSS tracks do preserve separation. Also do not require a 294px auto Chart to leave a later y=4 panel at 208px after repair: on a fixed grid that content actually collides and must push the later panel down. This distinction narrows B027-03's proof without weakening the actual finding.

**Selected smallest correction:** extend the existing renderer's intrinsic-content measurement and feed its occupied whole-row extent into the existing pure placement/collision step. Keep desktop row tracks at fixed 40px. Reserve enough rows for actual intrinsic content plus the existing gap geometry while retaining the unplaced panel's actual content height rather than stretching its visible box to a rounded authored h. Keep measured occupancy transient and separate from authored Layout; auto content is not limited to the schema's 48-row maximum for explicit layouts. Update that measurement when intrinsic content/available width changes so a late load does not recreate the same defect. All placement, collision, editing-start and responsive consumers must share that geometry authority; do not add per-consumer offsets, component size inputs, a layout framework or persistence.

This resolves existing approved requirements, without a new public/API or persistent-data decision. Changing only CSS to fixed rows would sacrifice required content height; retaining minimum rows sacrifices declared pitch. Neither is a complete correction. No new owner or replacement architecture is needed.

**Focused closure:** reuse existing browser harness for an intrinsic header shorter than y=4 and assert a noncolliding fixed panel starts at `4*(40+12)=208px`. For a tall intrinsic Chart, assert content height remains intrinsic, its occupied rows cover content, and a colliding fixed panel moves to the first clear whole row with no overlap. Assert computed desktop tracks stay 40px, documents remain unchanged, and conversion of auto to fixed does not redefine grid pitch. Include content settling after initial loading and an all-unplaced view; retain narrow fixed-h stacking/scrolling checks. Existing style-string assertions alone cannot prove physical geometry. Keep pure placement collision assertions alongside browser proof; no broad suite or new harness is required.

## Correction scope, proof and limits

All three findings reuse `WyrdView`, its retained state/gesture/measurement responsibilities, and `layout.ts`'s pure geometry. They cannot be deleted as optional behavior because the approved task expressly requires callback completeness, cancellation and mixed fixed-grid placement. Native pointer events, CSS Grid and existing Svelte measurement facilities suffice; there is no reason to add dependencies, alternative hosts, server work, component sizing inputs, saved views or compatibility behavior.

The four exact focused commands and additional regression checks are recorded green in `verification.md`; original broader builds/codegen and human approval remain historical evidence. Existing tests edit only one panel before host feedback, test Escape only without an active drag, and inspect grid style strings in a DOM environment without layout. Their success does not cover the three diagnosed paths. Standards coverage remains PASS and does not resolve these behavioral failures. Follow-up evidence resolves the geometry conflict in favor of the bounded revised finding.

Remediation proof should use the exact focused TASK-027 editing and placement commands from the original task plus narrow affected package/UI checks, and existing browser evidence for geometry and capture. Keep document-contract and narrow scenarios as local regressions where those paths change. Do not require release/Linux builds, production MCP registration, full server journeys or repository aggregates for these renderer corrections.

Skipped implementation, fresh browser execution and broad verification in this validation pass. Risk: the three confirmed/revised renderer behaviors remain incorrect at the immutable candidate until remediated.
