# Focused follow-up: relative-range representability

**RESOLVED.** INV-RANGE describes a reachable TASK-003 input-validation failure. The claimed root-document failure is outside the existing panel promise boundary. An explicit panel range reaches that boundary as an internal error in WyrdView, and reaches both server endpoints as an unhandled rejected handler promise. These are one shared cause. No additional finding is proposed.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, candidate/HEAD `6aace6e07728e59deb1fcd85ce969b2d0a4260db`. Approved specification revision 13 and original `tasks/TASK-003-shared-ui-foundation.md`. Read subject, claim comparison, both implementation reports, root rules, review skill, workflow and maintainer guide; inspected cumulative changed source for this uncertainty and current complete owner/caller bodies. No CodeGraph index or additional nested rules apply to these UI/example paths. This is a focused discovery pass for INV-RANGE, not final independent validation or a repeat audit of other discovery conclusions.

## Governing obligation

REQ-030 requires shared portable loaders, visible stable error states, and validation against component schemas. Its View document paragraph requires an invalid panel to identify its component/failed field while other panels render. Original TASK-003 scenario 1 explicitly requires invalid input to fail visibly without blocking valid siblings; the acceptance criteria require visible Chart schema errors. The root range belongs to the whole document, so rejecting that document visibly is appropriate; it need not load otherwise valid panels under an invalid shared range. The defect is the absence of that visible rejection, rather than a requirement to keep executing an invalid document. A panel-local invalid range must fail only that panel.

## Producer-to-consumer evidence

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless stated otherwise.

| Path | Source evidence and consequence |
|---|---|
| Shared admission | `packages/ui/src/lib/schemas.ts:56–61` accepts `{last:"1000000000000d"}` because its only value check is positive digits followed by `m`, `h`, or `d`. `RangeSchema` is shared by the root document and Chart input (`:71`, `:213`). There is no numeric/date representability validation. |
| Resolution | `packages/ui/src/lib/loaders.ts:75–78` multiplies by the unit and immediately calls `toISOString()` on `new Date(now - amount)`. At an ordinary current clock, this input subtracts `86,400,000,000,000,000,000` milliseconds, beyond native Date's domain. ISO conversion throws `RangeError`. This is native representability, unrelated to database retention or query-row/byte limits. |
| Root renderer | `WyrdView.svelte:81–89` invokes `start` from an effect with no try/catch. `start` (`:240–247`) aborts old controllers and resolves the root range before mapping any panels. Per-panel rejection handling is installed only inside that later map (`:253–256`). Consequently none of this document's panel loads is scheduled, including an unrelated CardHeader. On replacement, old controllers have already been aborted. The visible invalid-document branch (`:300–301`) tests only schema failure; this input passes the schema, so it cannot display the range problem. No Svelte boundary exists in this renderer. |
| Mount/AppBridge | `view.ts:44–46` mounts without a range-error projection. `src/app/main.ts:23–26` passes tool-result structured content straight into `mountView`; no second admission catches this. Production `ui.show_view` registration belongs to TASK-011 and is not needed for reachability through this required renderer/artifact. |
| Explicit Chart range | `loaders.ts:156–162` resolves `input.range` before any executor call and before producing Chart props. It is async, so this becomes a rejected promise. `loadPanel:256–263` validates the same schema and returns the loader promise without translating this rejection. In WyrdView, the per-panel rejection handler maps it to an internal failure, preserving siblings but failing field-specific validation semantics. |
| Shipped BFF | `src/routes/t/[tenantKey]/ui/load/+server.ts:18–29` validates only the separate inherited absolute context range, then directly awaits `loadPanel`. A valid inherited range does not validate the explicit Chart range. The handler rejects on the same `RangeError`, bypassing its canonical `refuse` branch. This is a framework-level request failure, not a Rust-server/process failure. |
| Custom endpoint | Repository-root `examples/ui-app/src/routes/view/load/+server.ts:8–12` has the same direct await and inherited absolute-range check, so the same explicit Chart input rejects this handler. Credential construction precedes loading; the claim assumes the normal runnable app has its required credential, not that an absent credential proves the defect. |
| Remaining direct resolver caller | Repository-root `examples/ui-app/src/routes/+page.server.ts:7` resolves the authored fixed 24-hour range. It is not an independently attacker-controlled source of this input. Its return-type/behavior must remain valid if the shared resolution boundary changes. |

The direct `resolveRange` callers are WyrdView, loadChart and that direct-component example. `loadPanel` is shared by WyrdView's local executor path and the two endpoint handlers. `remoteLoader` merely transports validated requests and interprets response bodies; adding host-specific arithmetic checks there would duplicate the source owner. No second existing relative-range validator/resolver was found. The existing absolute schema already uses native Date parsing for its ordering refinement; canonical `inputProblem`/`wyrdProblem('validation', …)` and renderer/loader error states already supply the required error mechanisms.

## Attribution and claim resolution

The complete cumulative diff adds the schema, resolver, renderer and endpoints in the foundation. `git show b0a05de50:<path>` independently shows the original foundation commit already contains both the digit-only schema and direct Date ISO arithmetic; its original WyrdView `start` resolves before mapping panels with the same promise boundary. TASK-027's later layout work does not introduce or own this cause. PR #205/main is unrelated. This is also separate from closed FIND-003-1: explicit precedence is currently correct; representability was never checked.

The behavior review's broad schema-validation PASS does not address this counterexample. INV-RANGE narrows that result: ordinary ranges and syntactically invalid ranges are handled, but this syntactically admitted relative range is not. The reports need not disagree about ordinary-path behavior.

## Read-only runtime check

Ran an inline, artifact-free `mise exec -- node --input-type=module` check in the UI directory against the already-built public `wyrd-ai-ui/loaders` entry. No build, package, test fixture or source was written. Assertions passed for all of:

- `RelativeRangeSchema`, `ViewDocumentSchema` and `ChartInputSchema` accept the counterexample.
- `resolveRange(range, Date.parse('2026-10-22T00:00:00Z'))` throws `RangeError`.
- `loadPanel('Chart', input, {executor})` rejects with `RangeError` and calls the executor zero times.

The printed exported resolver body matches the candidate's source arithmetic and unguarded ISO conversion. This corroborates the shared validation/resolution failure; it is not a newly executed mounted-browser test or endpoint integration test. Root sibling suppression and handler propagation are established by the inspected source order/boundaries, not claimed as runtime reproductions.

## Smallest correction boundary and closure proof

Retain one shared range admission/resolution authority in the foundation, using native Date representability to reject an unresolvable relative value before ISO conversion can escape. Reuse the current range schemas and canonical field-specific validation/error projection. Ensure all consumers get that same result: visible document validation for an invalid root range, visible field-specific Chart failure with valid siblings preserved for an explicit panel range, and canonical endpoint error/props rather than rejected handlers. Extend the existing owner; no new range service, arbitrary duration/retention cap, configuration, error family or repeated per-host numeric checks is justified. Private function signatures and local mechanics remain implementation-owned. Preserve one shared now for inherited bounds, valid ordinary/absolute ranges, panel precedence, exact SQL/binds, cancellation and TASK-027 layout behavior.

The smallest proof uses the existing rendering/load tests and existing endpoint harness: root counterexample gives a visible canonical validation error and no reads/unhandled effect failure; one explicit invalid Chart range gives a field-specific error while a valid sibling loads; BFF and custom endpoint counterexamples return their existing honest error representation without an uncaught rejection. Retain normal fixed-clock and precedence assertions. No new harness, live server, broad suite or production policy is needed for this input boundary.

Skipped: mounted-browser and endpoint runtime reproductions, builds and suite reruns. Risk: the accepted counterexample still bypasses visible validation and prevents root-document panel loading at this candidate; final finding status belongs to independent Ponytail validation.
