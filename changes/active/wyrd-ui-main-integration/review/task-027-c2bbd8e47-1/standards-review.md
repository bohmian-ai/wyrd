# TASK-027 cumulative repository standards re-review

Result: **PASS**. Material repository-rule findings: **none**.

## Subject and attribution

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate: `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.
Authority: approved specification revision 12, original
`tasks/TASK-027-view-layout.md`, and prior review's
`TASK-027-R1-layout-state-and-geometry.md`, all under the active change packet.

Reviewed the cumulative changed-file inventory and TASK-027's complete source,
not just commits `b512c2221` and `84d5dd22e`. The base-to-candidate range also
contains upstream Rust, dependency, CI, release, deployment and planning work,
plus separately assigned TASK-003 remediation. Those changes are not attributed
to TASK-027. Shared renderer/loading/error and Chart/CardHeader seams were
inspected as consumers and dependencies; their separate TASK-003 obligations
are not duplicated here. No TASK-027 Rust, Python, SQL, backend permission,
native binding, persistent data or deployment implementation exists in this
range. Consequently their language-specific gates do not apply to this task.

No `.codegraph/` exists per the subject; navigation used Git, `rg`, and source.
HEAD was independently checked as the candidate and tracked source remained
unchanged. Only this report was written. No current peer discovery conclusions
were consulted.

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`.

## Authority coverage

Reference selection followed `architecture/references/README.md`. Governing
architecture was inspected at the applicable Doctrine/Client model sections
of `architecture/wyrd-design.md` and the complete
`architecture/wyrd-doctrine.mdx`; this is presentation work, so Bifrost ingest,
storage, SQL capability and operational authorities are not newly implicated.

| Changed surface | Governing authority | Inspected owners, consumers and proof |
|---|---|---|
| `packages/ui/src/lib/schemas.ts` | AGENTS §§2–3, 9, 15–16; agent-rules generated-output rule; design Client model; doctrine Public surfaces; TypeScript guide Types/API; approved REQ-030/036 | `LayoutSchema`, strict `PanelSchema`, `ViewDocumentSchema`; `view.ts::validateView`, renderer parsing, catalog generation and invalid-layout tests |
| `packages/ui/src/lib/catalog.json` | AGENTS §§11–12; agent-rules source-generated artifacts; testing-workflows Contract/generated-output; REQ-030 catalog | Complete task schema diff; `scripts/catalog.ts` using `z.toJSONSchema`; `mise.toml` codegen lane; unchanged component size-input contracts |
| `packages/ui/src/lib/layout.ts` | AGENTS §§15–16; TypeScript guide Types/API/Style; maintainer-style ownership; UI typed-state guidance | Whole file, `rowsFor`, `place`, `edit`, `rowsHeight`; every production caller in `WyrdView`, focused placement assertions |
| `packages/ui/src/lib/WyrdView.svelte` | AGENTS §§2, 9, 11–12, 15–16; UI skill; its Svelte5, developer-UX, theming, testing references; brand DESIGN | Entire renderer, source/content identity, retained/draft state, gesture lifecycle, measurements, responsive ordering, styles; Chart/CardHeader intrinsic content and host callbacks |
| `packages/ui/src/lib/view.ts` | TypeScript guide typed public API; maintainer-style public contract; UI client/server boundary | `MountViewOptions`, `mountView`, existing `remoteLoader`; emitted `dist/view.d.ts` and `dist/WyrdView.svelte.d.ts`; browser exports |
| `packages/ui/src/app/main.ts` | Design client boundary; doctrine Public surfaces; UI skill local state/accessibility; REQ-036 per-render state | Entire AppBridge bootstrap and existing MCP executor delegation, per-result destroy/mount and callback; no new tool registration or write permission |
| `src/routes/_gallery/+page.svelte` | UI skill; Svelte5 typed local state; brand mode/geometry; UI testing-verification | Changed document and both mode renderers; existing fixture executor; host echo path; recorded visual acceptance |
| `examples/ui-app/src/panels.ts`, `src/routes/view/+page.svelte` | AGENTS §§11–12 examples; TypeScript guide; UI Svelte5 and client boundary | Full changed files, shared dashboard inputs and remote loader; package metadata, UI pnpm workspace membership, fresh canonical workspace check/build evidence |
| `src/lib/ui-library.test.ts` | AGENTS §11; agent-rules runtime ownership/no gate weakening; testing-workflows; spec-driven-development TDD/command precision; UI component verification | Full five TASK-027 cases and helper, TASK-003 shared-renderer regressions, pointer-capture shims, offsetHeight mock and restoration, real-browser evidence distinguished from DOM assertions |
| Active task/remediation evidence | AGENTS §14; spec-driven-development; implementation-execution | Original ordered scenarios, prior validated findings and verdict, remediation authority and appended RED/GREEN/browser evidence, current subject/verification |

The applicable reference files were read, including
`languages/spec-driven-development.md`, `languages/implementation-execution.md`,
`languages/maintainer-style.md`, `languages/typescript-guide.md`,
`languages/testing-workflows.md`, and `architecture/patterns.md`. The local UI
skill and its Svelte5, developer UX, theming and testing references supply the
frontend rules; `brand/DESIGN.md` supplies the actual token and geometry contract.

## Rule assessment

| Exact applicable rule | Result | Source evidence |
|---|---|---|
| AGENTS §2 and design Client model: server owns durable behavior; UI remains a client | PASS | Layout remains presentation state. `WyrdView:150–155` emits a declarative document; `main.ts` retains nothing outside the mounted render. No storage, registry, policy, audit or credentials are added. Existing executors remain data owners. |
| AGENTS §15: reuse current owners and fix shared causes | PASS | Validation extends `PanelSchema`; pure placement is beside its existing renderer. Measurements feed `place` rather than creating offsets per consumer. Commit and cancellation remain renderer-owned. No replacement store, loader, transport, framework or dependency is added by this task. |
| UI Svelte5 rules: typed props, unknown at untrusted boundaries, pure derived state, local mutable state | PASS | `WyrdView:30–45` types callbacks and accepts `doc: unknown`; `:56` parses through Zod. `:95–127` separates authored edits from measured placements using local state and pure derivations. Callback types match `MountViewOptions` and generated declarations. |
| UI Svelte5 anti-pattern: do not mutate props or introduce global stores | PASS | `commit:150–155` constructs a new outer document and changed panel values, overlaying retained edits only. `place` copies explicit layouts before collision resolution. Measurements are a separate local record; gallery/example explicitly replace their own raw state. |
| agent-rules generated-artifact prohibition; AGENTS §11 contract proof | PASS | `catalog.json` is generated by `scripts/catalog.ts` from the schema owner; no generator fork. Fresh orchestrator `mise run codegen:check` passes. Generated `dist` declarations expose optional layout and callback consistently; they are build products, not authored parallel contracts. |
| TypeScript guide: preserve browser/native boundaries, avoid `any`, type public contracts | PASS, nonblocking convention note below | New layout code imports only a schema type. `mountView` and callback use the existing `ViewDocument`; no SDK runtime dependency enters geometry. Added operations have narrow numeric/layout types. There is no new public invalid-call or declaration mismatch. |
| UI skill and brand DESIGN: semantic tokens, flat geometry, complete light/dark behavior | PASS | `WyrdView:329–410` uses existing background/text/muted/declare/font tokens, visible focus and active outlines. No hex, external fonts, shadow, gradient, lift or palette fork. `:350–359` keeps fixed 40px desktop tracks and task-required 12px gaps; narrow rows use content height. |
| UI accessibility: keyboard equivalents, useful names, visible focus, responsive layout | PASS | Move button `:298–305` has a name/instructions and arrows/Shift-arrows/Enter/Escape; corner offers the same resize operation through that button. Focus CSS is explicit. `:93–94,125–135,297,320` derive read-only narrow rendering from container width. Pointer Escape now reaches focused or unfocused starts. |
| UI verification: observers/subscriptions clean up; use native component facilities | PASS | Intrinsic height uses Svelte's `bind:offsetHeight` at `:295`. Held gesture cleanup `:197–214` removes all installed listeners and releases capture; document replacement calls cancel `:103–109`; unmount cleanup `:110` releases any active gesture. No independent observer lifetime is introduced. |
| AGENTS §11 and UI testing reference: test in owning runtime; do not weaken gates | PASS | Vitest/Svelte tests exercise TypeScript/component behavior. Height mock is restored in `finally`; pointer shims fill missing jsdom methods rather than bypassing production logic. Original assertions remain and new callback/cancellation/geometry assertions add coverage. No skipped required test, weakened assertion or lint suppression is added. |
| spec-driven-development TDD: expected behavioral RED then GREEN; pre-existing regression may supply RED | PASS with evidence limit | Remediation records missing sibling edits, revived gesture, and one-row intrinsic occupancy before correction. Geometry RED came from the prior review's concrete failing measurement/probe rather than a second implementer rerun; the reference expressly permits pre-existing RED. Current focused tests and supplied physical measurements prove GREEN. No new test churn is required solely to repeat already-recorded failure. |
| AGENTS §11: exact focused commands and narrow complete verification | PASS | Current `verification.md` records five exact anchored TASK-027 selectors, TASK-003 regressions, fresh UI check/build, example workspace check/build and codegen. UI check reports zero errors/warnings. These are the original task's scoped gates; no Rust aggregate is claimed or required for this frontend-only write set. |
| AGENTS §11/design doctrine 20 journey priority | PASS for bounded task scope | AC-015 expressly requires schema/collision/handler tests plus real-browser gallery/AppBridge evidence, with human gallery acceptance. Those evidence classes are supplied. This geometry change adds no SDK/server workflow; existing backend journeys and deferred production MCP App journeys remain separate change-review obligations, not substituted by these component tests. |
| AGENTS §16 and agent-rules permanent-source wording | PASS with nonblocking notes | New callback and geometry comments explain meaning/lifetime. Rustdoc hard gate is N/A because no task-owned Rust item changes. Task-ID test names remain a wording-only convention issue, below the task-review blocking threshold. |
| SQL capability, tenancy, audit, SSRF, PyO3 and native lifecycle rules | N/A | No corresponding task-owned signature, field, query, authentication check, network destination, foreign runtime or durable write changes. Reading shared components does not attribute unrelated upstream implementation to TASK-027. |

## Prior finding closure

These are source-backed closure assessments relevant to the repository rules,
not a substitute for the independent task acceptance reviews.

| Prior ID | Current evidence | Assessment |
|---|---|---|
| FIND-027-1 | `WyrdView:150–155` merges all `kept` entries with the draft, reports only authored edits over parsed panels and retains the merged map. `ui-library.test.ts:688–715` exercises a non-echoing host, two siblings, untouched unplaced/colliding panels, reapplication, immutability and no reload. | Closed: output and local state share one owner; measured/collision placements do not leak into untouched authored panels. |
| FIND-027-2 | `cancel:159–162`, `drag:185–215` and unmount/document cleanup terminate the event producer, not only its preview. `ui-library.test.ts:717–760` exercises focused/unfocused move and unfocused resize, Escape followed by more movement/release, fresh completion and pointercancel. Historical browser capture/focus evidence is appended to R1. | Closed for the reported producer lifecycle: cancellation removes listeners before later events can recreate the draft. |
| FIND-027-3 | `layout.ts:22,34–41` converts measured height to occupied rows. `WyrdView:115–123,129–142,295,350–359` shares measured placement across rendering/edit start while desktop pitch stays fixed and auto boxes retain intrinsic height. Pure tests and measured DOM spans are at `ui-library.test.ts:622–645`; R1 records physical tracks, short/tall content, async settling, width changes and mobile. | Closed: geometry authority consumes actual content extent; measurements remain transient and auto occupancy is not capped by authored h≤48. |

## Nonblocking conventions and evidence limits

- `layout.ts:22` and `:52` infer exported return types instead of declaring
  `number`/`string`, contrary to the TypeScript guide's explicit-return
  convention. Types remain precise in generated declarations; no invalid call
  or behavioral/public-contract defect results. This does not justify a style
  remediation round.
- TASK-027 test names contain task IDs despite agent-rules' permanent-source
  wording rule. The original task prescribes those exact selectors, and the
  existing file follows that convention. This is a nonblocking naming note;
  no production code carries task-specific behavior.
- The example is explicitly a member of the UI pnpm workspace. A direct-cwd
  package-resolution observation does not establish a regression against the
  canonical `--filter @wyrd/example-ui-app` path, which freshly passes both
  check and build. TASK-027 introduces no workspace/dependency change.

No builds were repeated by this role. The orchestrator's fresh verification
and historical R1 browser evidence are identified separately. No fresh browser,
Linux bundle/image, real Postgres/MCP journey or live degraded-terminal parity
was executed here. Those omissions do not establish a task-owned repository
rule violation; integrated host/deployment evidence remains change-review scope.

Overall repository-standards result: **PASS**; no material findings proposed.
