---
id: TASK-006
title: Card inventory shared shell and workspace host
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-003, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-080, REQ-081, REQ-092, REQ-095, REQ-100, REQ-101, REQ-127, REQ-128, REQ-129, REQ-130, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-001, AC-003, AC-010, AC-011]
depends_on: [TASK-003]
parent_task:
remediates: []
---

# Outcome and value

Create Card discovery and one stable `/cards/[uid]` host so all focused Card
workspaces can be implemented independently and in parallel without duplicating
routes, shell behavior, or a central kind switch.

# Owner and write set

- Own `/t/[tenantKey]/cards`, `/cards/[uid]`, `src/lib/features/cards/core/**`,
  Card view-model envelopes, and the workspace discovery seam.
- Inventory supports URL-backed lookup plus kind, label, status, and authorized
  Space filters. Detail owns identity, exact version, metadata, relationships,
  common navigation, and typed Spec fallback.
- Define one small typed workspace module contract discovered at build time from
  `src/lib/features/cards/workspaces/*/index.ts`; later workspace tasks add only
  their own folder and tests. Duplicate kind registration fails deterministically.
- Implement the smaller Workflow and Verifier specialized presentations here;
  all other unregistered specializations use the typed Spec fallback.

# Locked decisions and non-goals

- Workspace modules provide presentation, local URL-state parsing, and typed
  fixture projection only. They do not own Card identity, routing, auth, or IO.
- Use SvelteKit/Vite capabilities already installed. No plugin runtime, dynamic
  remote import, schema renderer, per-kind route, editor, or central switch that
  every parallel task must modify.
- Relationships and status arrive from the server projection; the browser does
  not infer lifecycle, lineage, or execution.

# Locked visual implementation authority

- Implement `C-01` Card inventory and `C-02` shared Card detail from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg),
  including the exact filter, version, relationship, Spec, empty,
  unauthorized, loading, and safe-error states in the
  [ledger](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#cardssvg).
- Implement the earned Workflow and Verifier presentations from `C-08` and
  `C-12` inside the same `C-02` identity/version/metadata/relationships shell.
  They are not separate routes or alternate shells.
- `M-01` in
  [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg)
  locks inventory at 390 × 844, including labeled stacked rows, scrolling
  filter chips, retained count/kind/version/status/space/owner, and the open
  five-entry Menu disclosure. All detail presentations follow `R-CARD`:
  primary Card content first and metadata/relationships stacked afterward.
- The host must preserve the accepted Card header, dominant work region,
  subordinate rail, selection treatment, direct links, and URL-restorable local
  state for every workspace. Do not replace the accepted compositions with one
  generic metadata/table page.
- Completion evidence must compare `C-01`, `C-02`, `C-08`, and `C-12` in both
  themes plus `M-01` at 390 × 844, and prove that later workspaces enter the
  unchanged accepted shell.

# Ordered test scenarios

1. Inventory restores filters, covers all registrable fixture kinds, and handles
   empty/unauthorized/error states.
2. Generic detail renders exact identity/version/metadata/relationships/Spec.
3. Workspace discovery selects one local module and rejects duplicate kinds.
4. An absent workspace uses the generic fallback; Workflow and Verifier render
   their earned presentation within the same shell.
5. Long tables/specs and relationship content retain information at narrow width
   in both themes.

# Red-Green-Refactor

Prove inventory, shared detail, then discovery. Keep the registration contract
minimal; do not add extension features that no planned workspace needs.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/workspace-registry.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record the filter matrix, fallback behavior, workspace registration proof, and
responsive shell captures. Stop if parallel workspace additions require route
or core-host edits; repair the seam without turning it into a runtime plugin API.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.

# Execution evidence (2026-09-08)

Status: implemented; all task-named focused tests, the full UI suite, check,
build and token verification pass.

## Scenario cycles

1. Inventory — RED: `CardsInventory.test.ts` failed with 404 on `/t/acme/cards`
   and missing `WyrdClient.cards`. GREEN: added `features/cards/core/types.ts`,
   `mock/cards/{fixtures,project}.ts`, `WyrdClient.cards/card` behind
   `cards:read`, the C-01 inventory route with kind rail (17 registrable kinds
   with counts), URL-backed `q/kind/space/status/label` filters as removable
   chips, recently-viewed rail, and stacked-row `data-l` labels for M-01.
   Cross-tenant isolation is proven at the client seam (research tenant gets a
   truthful zero-row registry and 404 detail) because the local login scenario
   issues a single-tenant acme session, so `/t/research/*` is 403 for the
   journey cookie — the equivalent ObserveJourney research assertion is
   vacuous for the same reason.
2. Generic detail — RED: `CardDetail.test.ts` failed with no `/cards/[uid]`
   route. GREEN: C-02 shared shell (identity header, kind pill, version menu,
   uid/apiVersion line, Spec sections, immutable Versions table with
   `?version=` read-only re-render and 404 on unknown versions, server-managed
   Metadata, server-derived Relationships with absent state, Links & status).
3. Discovery — `workspace-registry.ts` (`buildRegistry` over
   `import.meta.glob('../workspaces/*/index.ts', { eager: true })`) landed
   with scenario 4's GREEN; `workspace-registry.test.ts` pins the contract:
   one module per kind, deterministic duplicate-kind failure naming both
   paths, deterministic malformed-module failure, and the built registry
   holding exactly Workflow and Verifier.
4. Fallback and earned presentations — Workflow (C-08 stages strip, IO,
   governance, execution-results absent note) and Verifier (C-12 purpose,
   accepted evidence table, capabilities, no-secrets note) render inside the
   unchanged shell; `card_mcp_01` proves the generated typed Spec fallback;
   later workspace tasks add only `workspaces/<kind>/index.ts` + component.
5. Narrow width — inventory cells carry `data-l` labels asserted in the
   journey; `cards.css` stacks rows below 600px with sideways-scrolling chips
   (M-01) and the detail keeps R-CARD column collapse below 1000px via the
   shared `.columns` grammar.

## Verification

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts` — 6/6
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts` — 6/6
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/workspace-registry.test.ts` — 4/4
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — 29 files, 151/151
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — 0 errors
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` — success
- `mise run check:tokens` — in sync

## Limitations

- Both-theme screenshot comparisons of C-01/C-02/C-08/C-12 and M-01 at
  390×844 are not captured here; state coverage is proven via rendered-HTML
  assertions and the shared token/ModeProvider mechanics. Capture during
  visual review if required.
- The dashboards preview-panel removal earlier in this session is a recorded
  deviation from the TASK-005 ledger's selected-detail treatment (user
  directive: rows open the dashboard directly).

## Post-approval audit remediation (user-directed, 2026-09-08)

Two independent persona audits (ML engineer; platform operator + AI agent)
were run at the user's request; the user then directed "Address all issues",
authorizing deviations from the approved `cards.svg` mock where the mock
itself carried the redundancy.

Bug fixes:
- Search form no longer wipes active filters — hidden inputs carry
  kind/space/status/label/owner through a `q` submit.
- Rows actually sort newest-updated first; `CardRow` gained `updatedAt` (ISO)
  backing the sort and `<time datetime>` semantics.
- Kind-rail counts now respect every other active filter (and the All-kinds
  number sums those counts), so rail numbers agree with what clicking shows.
- `?version=` now renders that version's declared Spec via fixture
  `versionSpecs`; a prior version without one renders a truthful absent
  state, and earned presentations never render for prior versions.
- `WorkflowSpec` stages and `SpecSections` entries key by index so duplicate
  labels cannot throw.
- List-route error state surfaces `remediation` and drops the Retry action on
  403. Detail-route loads keep throwing — `+error.svelte` renders the
  structured problem (audit finding closed as no-change-needed).

Redundancy removals (mock deviations, authorized):
- Detail header version dropdown removed — the Versions table is the single
  switcher; header shows static `vN (current)`; dead `version-menu` CSS
  deleted.
- "Links & status" rail panel removed (status and actions already live in the
  header); `links` dropped from `CardDetail` and fixtures.
- Bottom "read-only projection" placard, "Selecting a prior version…" note,
  and "SECTIONS WITH NO DATA" notice removed; the uid line and viewingPrior
  banner remain the single statements of those facts.
- Spec sections no longer restate relationships/metadata: Policy `applies_to`,
  Workflow relationship `consumes` duplicate, and governance `owner` removed;
  generated generic spec reduced to `space` only.
- "Row → the Card detail" table note removed; "Owners & governance" renamed
  "Governance".

Value/usability:
- Owner filter added (`owner=unowned` sentinel for empty owners); space,
  status and owner cells are filter links; chips carry human labels and the
  `q` chip is dropped (the input shows it).
- Empty tenant (`total === 0`) renders its own truthful state distinct from
  no-match.
- Detail fixture names now match their rows (`checkout-quality`,
  `pii-review`); interim enriched details added for `card_model_01`,
  `card_data_01`, `card_service_01` until their focused workspace tasks land.

Verification (all green):
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts src/lib/features/cards/core/CardDetail.test.ts src/lib/features/cards/core/workspace-registry.test.ts` — 18/18
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test` — 29 files, 153/153
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` — 0 errors
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` — success

Deferred: pagination/sort params (YAGNI at fixture scale); whole-row click
already covered by Table's delegated row link.
