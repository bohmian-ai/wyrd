---
id: SPEC-wyrd-ui-main-integration
revision: 1
status: approved
---

# Wyrd UI main integration

## Objective and user value

Bring the proven UI and live-data work from the `wyrd-server-ui` branch onto
current `main` as a sequence of independently reviewable product surfaces. The
result is a coherent analytics, observability, and verification workbench in
which an authorized user can discover exact registered Cards, inspect their
runtime behavior, query retained evidence, and investigate generic
verification results without encountering mock data presented as live truth or
legacy verification concepts.

The migration must preserve the useful behavior already built while adopting
current Wyrd contracts and the approved visual direction. It is not a branch
merge, a compatibility migration, or permission to revive removed nouns.

## Authority and migration sources

Current repository authority and current `main` behavior constrain the
change. The following older material is implementation and design evidence,
not protocol authority:

- `wyrd-server-ui` at `7bfc3a8a282daf490a1ee649ce9fe74382d4fd35`,
  including its live Card, Observe, Query, Home, Service, BFF, and browser
  journey work;
- `changes/active/wyrd-ui-live-data` on that branch, including task evidence
  for implemented and proposed work;
- the unchanged DreamCanvas reference copied into
  `changes/active/wyrd-ui-main-integration/references/canvas`;
- the existing product SVG renders, Svelte mocks, UI components, routes, auth
  boundary, and tests already present on `main`.

The DreamCanvas files are visual and interaction references. They are not
production HTML, a runtime dependency, typed data contracts, or evidence that
every depicted action has a server-owned capability.

## Scope

This change includes:

- a compact application shell appropriate for an analytics and observability
  workbench;
- live Card inventory and exact Card detail backed by current server contracts;
- shared Card detail presentation plus earned Agent, Prompt, Model, Service,
  and Verifier workspaces;
- live Observe traces, trace detail, logs, and metrics with shared investigation
  controls;
- a live read-only Bifrost Query workbench;
- a top-level Verify workspace over generic verification executions and
  results, including drift-check detail where present;
- live Home, Observe overview, Card evidence, and Service summaries that reuse
  the same telemetry and verification meanings;
- generated or otherwise canonical typed HTTP projections where required by
  the server operations the UI consumes;
- representative mock states, component coverage, real browser journeys, and
  a disposable review environment sufficient for page-by-page visual review;
- integration of existing Changes and Settings routes into the new shell
  without expanding their durable behavior.

### Non-goals

- Rebasing, merging, or replaying the old branch as a unit.
- Reimplementing authentication, public-client login, request-origin handling,
  container topology, or image work already present on `main`.
- Restoring Eval, Task, or Drift as Card kinds, Verifier kinds, result kinds,
  routes, aliases, or parallel UI data models.
- Reintroducing Workflow as a registrable Card kind.
- Adding a UI-owned registry, query engine, verification engine, durable saved
  query history, dashboard builder, or arbitrary view-composition system.
- Browser-direct calls to protected Wyrd routes, Bifrost, databases, object
  storage, or external providers.
- Adding public server behavior solely to reproduce an unsupported canvas
  interaction. In particular, artifact download, global command-palette
  behavior, and other depicted actions ship only when an existing authorized
  server contract supports the complete workflow.
- Replacing working accessibility, responsive behavior, or error handling with
  fixed-size canvas markup.
- Completing or redefining the durable Changes contract.
- Adding compatibility routes or fields for the old UI projections.

## Required observable behavior

### Product shell and navigation

- **REQ-001 — Compact primary rail:** Authenticated tenant routes use one
  persistent desktop primary rail between 80 and 88 CSS pixels wide. It shows
  recognizable icons and visible labels for Home, Cards, Observe, Verify,
  Changes, and Query; Settings is a subordinate destination anchored at the
  bottom. The compact rail is the normal desktop state, not a user preference
  that must be expanded before navigation is understandable.
- **REQ-002 — Utility header:** The top header carries current tenant identity,
  authenticated principal controls, theme/help affordances, route context, and
  any earned global Card lookup. It does not duplicate primary navigation or
  become a second global workspace selector. `space` remains page-local
  filtering rather than global tenancy.
- **REQ-003 — Contextual navigation:** Workspace-specific navigation remains
  inside its owning page: Card Overview/Versions/YAML, Observe signal views,
  Verify result views, Query result modes, and settings sections do not become
  primary rail entries.
- **REQ-004 — Responsive and accessible shell:** At widths where the rail would
  make the workbench unusable, primary navigation becomes a keyboard-operable
  overlay rather than squeezing or covering content. Navigation, menus, tabs,
  disclosures, focus restoration, current location, skip navigation, and
  status remain understandable without hover or color alone. The shell meets
  WCAG 2.2 AA and supports reduced motion.
- **REQ-005 — Analytical workspace:** Tables, charts, editors, trace timelines,
  detail drawers, and Card metadata rails receive the remaining viewport width
  without horizontal clipping at supported desktop widths. Contextual panels
  move, collapse, or stack at narrower breakpoints without hiding their data.

### Server-backed UI boundary

- **REQ-010 — One server-only boundary:** SvelteKit server loads, actions, and
  earned authenticated server handlers call the existing server-only Wyrd/BFF
  boundary. Browser components never receive upstream credentials or call
  protected Wyrd or Bifrost routes directly.
- **REQ-011 — Current typed contracts:** UI operations project the current
  mounted Wyrd HTTP and Bifrost contracts. Closed request and response shapes
  are typed from their owning contract or a deterministic generated
  projection; presentation models map from them without maintaining a second
  durable vocabulary. Generated output is never hand-edited.
- **REQ-012 — Truthful modes:** Mock mode uses explicit fixtures. Live mode uses
  only authorized server evidence and never falls back to fixtures after an
  upstream refusal, error, missing record, or empty result. Loading, empty,
  denied, unavailable, stale, partial, and failed states remain distinct.
- **REQ-013 — SSR and URL state:** Primary page content or its truthful state is
  present in the server-rendered response. Search, filters, time ranges,
  pagination, selected versions, and investigation context use shareable URL
  state where appropriate and survive direct navigation and browser
  back/forward behavior.
- **REQ-014 — Tenant and permission fidelity:** Effective tenancy comes only
  from the authenticated server boundary. Every page preserves current
  authorization semantics, and protected verification or telemetry payloads
  remain unavailable without their required permission even when row metadata
  is visible.

### Cards

- **REQ-020 — Live inventory:** `/t/{tenant}/cards` lists authorized Cards from
  the current fourteen-kind registry contract with deterministic ordering,
  truthful filtering, and bounded pagination. It distinguishes page size from
  an authoritative total and never depends on the new canvas for an inventory
  design that the canvas does not define.
- **REQ-021 — Exact shared detail:** `/t/{tenant}/cards/{uid}` renders the exact
  selected Card and version, including identity, kind, lifecycle state,
  description, registration metadata, labels, relationships, declaration, and
  available versions. Direct links, missing versions, denied access, and
  relationship navigation are truthful and never float to another version.
- **REQ-022 — Shared Card composition:** Card detail uses the canvas-directed
  composition: a compact identity band; Overview, Versions, and YAML views; a
  dominant kind-specific work region where earned; and a compact relationship
  and metadata rail. It removes repetitive field-box presentation, full
  unabridged hashes in primary content, and explanatory chrome that competes
  with the Card itself while preserving access to exact raw declaration data.
- **REQ-023 — Earned workspaces:** Agent detail leads with Prompt → Agent →
  Model composition, invocations, and bound Verifiers; Prompt detail leads
  with its ordered messages, variables, model settings, and consuming
  relationships; Model detail leads with interface, artifacts, production
  behavior, and applicable verification evidence. Service and Verifier retain
  earned specialized presentations aligned to current `main`. Other kinds use
  the shared typed presentation until a distinct workflow earns more.
- **REQ-024 — Card-scoped behavior:** Card telemetry and verification summaries
  use exact authorized Card identity and current table semantics. Missing Card
  correlation is shown as missing rather than inferred from names, labels, or
  a containing Service.

### Observe and Query

- **REQ-030 — Shared investigation controls:** Observe traces, logs, and
  metrics use one coherent time-range, filter-expression, zoom, refresh, and
  cancellation model. Controls remain URL-backed where shareability matters,
  reset incompatible paging or selected detail state, and do not issue stale
  reads after a newer investigation supersedes them.
- **REQ-031 — Traces:** Users can inspect bounded trace lists and an exact trace
  with spans, hierarchy, events, links, timing, status, attributes permitted by
  their access, and correlation navigation. Duration and status filtering,
  deterministic paging, missing attributes, and failed traces remain truthful.
- **REQ-032 — Logs:** Users can inspect bounded log lists and record detail with
  severity, time, resource information, protected payload handling,
  deterministic paging, and trace correlation when present.
- **REQ-033 — Metrics:** Users can inspect actual counter, gauge, and
  distribution behavior with correct units, temporality, rate or quantile
  derivation where supported, grouping, gaps, zoom, and freshness. The UI does
  not invent zeroes or merge incompatible series.
- **REQ-034 — Read-only Query workbench:** `/t/{tenant}/query` exposes the
  authorized Bifrost catalog, one SQL editor, bounded typed parameters and time
  controls where applicable, run/cancel state, terminal-safe errors, tabular
  results, charting over returned columns, query details, browser-local recent
  history, copy, and CSV of returned rows. It represents the canonical
  read-only query contract and does not imply mutation or partial-success
  results.

### Verification

- **REQ-040 — Generic Verify overview:** `/t/{tenant}/verify` presents retained
  `vala.verification.results` rows using current generic verification
  vocabulary. Users can filter and group by verdict, Verifier, failed
  requirement, Service or owner, subject Card, application Run, and window
  where the available columns support that distinction. Execution status and
  verdict are never conflated; inconclusive never counts as passed.
- **REQ-041 — Generic result detail:** A result detail view presents the exact
  Verifier, subject, owner/binding, execution, application Run or window,
  terminal status, verdict, declared-order requirement outcomes, reasons,
  actual values, citations, preparation steps, evidence provenance, attempts,
  timing, and permitted dispatch information available in the current result
  contract. It does not branch into Eval, Task, or Drift result page types.
- **REQ-042 — Drift-check expansion:** When a generic result contains a drift
  requirement, that requirement may expand its current feature detail and
  history from `vala.drift.result_features`, joined by result and requirement
  identity. Missing baseline, insufficient data, inconclusive execution, and
  failed drift remain distinct. A window Verifier is not presented as manually
  runnable.
- **REQ-043 — Sensitive outcomes:** Result metadata remains usable when the
  principal may list results but cannot read sensitive `outcome` detail. The
  UI requests and decodes protected payloads only after authorization and
  explicit projection, and shows a denied-detail state rather than guessing
  requirement content.

### Cross-surface integration and staged review

- **REQ-050 — Shared meanings:** Home attention, Observe overview, Card
  evidence, and Service summaries reuse the same Card, telemetry, and generic
  verification projections as their canonical detail surfaces. They do not
  independently reinterpret result columns or invent health from lifecycle
  state, missing data, or display-name matching.
- **REQ-051 — Investigation continuity:** Links among Home, Cards, Observe,
  Verify, Query, and Service workspaces preserve only applicable tenant,
  Card, Run, trace, time, Verifier, result, and window context. A destination
  consumes or explicitly declines every exposed selector; URLs alone never
  become authorization authority.
- **REQ-052 — Independently reviewable slices:** The migration remains usable
  after each outcome-complete slice. A slice brings its required route,
  server projection, presentation, mock states, focused tests, and applicable
  real browser journey together; it does not land dead shared UI machinery for
  a hypothetical later page. Each slice has representative desktop and mobile
  visual evidence and awaits human visual acceptance before dependent visual
  work proceeds.
- **REQ-053 — Review environment:** The final integrated candidate provides a
  disposable repository-managed environment with enough registered Cards,
  services, telemetry, queryable tables, passing/failing/inconclusive
  verification, permission states, and sparse/dense data to inspect every
  included surface. No fixed synthetic Service count substitutes for the
  required states.

## Invariants and required boundaries

- **INV-001 — Protocol authority:** Current `architecture/wyrd-design.md` and
  current typed contracts win over the old branch, old packet, canvas, mocks,
  and implementation drift. Wyrd has fourteen registrable v1 Card kinds and
  one generic Verifier model.
- **INV-002 — Headless truth:** The UI projects server-owned behavior. Nothing
  required for registration, observation, query, verification, policy, audit,
  or operation exists only in the UI.
- **INV-003 — Bifrost boundary:** Analytical reads use the canonical Bifrost
  query contract through the existing shared client/server boundary. The UI
  does not add a Vala serving surface, direct SQL connection, or client-side
  warehouse implementation.
- **INV-004 — Tenant and sensitive-data isolation:** Tenant identity,
  authorization, object scope, and sensitive payload access are decided by the
  server. Browser state and URL selectors never widen them.
- **INV-005 — No compatibility vocabulary:** No compatibility aliases, routes,
  fields, fixtures, navigation, or presentation models preserve removed Eval,
  Task, Drift-kind, Workflow-kind, `subject_card_uid`, or `implementation`
  semantics.
- **INV-006 — Visual reference, production implementation:** The preserved
  canvas governs the included pages' hierarchy and visual intent except for
  its horizontal artboard navigation. Production UI remains responsive,
  semantic, accessible Svelte using the established Wyrd brand and reusable
  components; DreamCanvas runtime markup and duplicated inline styles do not
  ship.
- **INV-007 — Source preservation:** The old branch and packet remain
  historical evidence and are not rewritten by this change. Ported behavior
  is re-established under current owners and contracts rather than treated as
  authoritative because it existed before.
- **INV-008 — Truthful failure:** Refusal, timeout, cancellation, partial
  availability, missing correlation, stale data, and empty results are visible
  states and never become fabricated success or mock fallback.

## Expensive-to-reverse decisions

1. Wyrd uses a compact persistent left rail, not horizontal global product
   navigation, for supported desktop workbench widths.
2. The utility header owns tenant/principal/global utilities; page-local
   navigation owns workflow modes.
3. Verify is a top-level product destination alongside Home, Cards, Observe,
   Changes, and Query; Settings remains subordinate.
4. The canvas is the visual authority for Home, Agent, Prompt, Model, Observe,
   Query, and Verify page composition, but not for protocol vocabulary, data
   availability, accessibility, responsive behavior, or the global shell.
5. Verification UI has one generic overview and one generic result-detail
   model. Drift is requirement detail, not a parallel product ontology.
6. Old branch code is migration evidence, not a merge base or compatibility
   contract.
7. Human visual acceptance occurs between dependent visual slices; automated
   tests remain required and do not substitute for that acceptance.

## Acceptance criteria

- **AC-001:** At desktop and mobile review widths, an authenticated user can
  navigate every included area with the compact rail or responsive overlay,
  identify tenant and route context, operate the shell by keyboard, and use
  the analytical workspace without clipped primary content.
- **AC-002:** The Card inventory and exact Card detail journey runs against a
  real server and proves current Card kinds, pagination, direct/versioned
  detail, missing/denied states, relationships, and Agent, Prompt, Model,
  Service, and Verifier presentations.
- **AC-003:** Real emitted traces, logs, and metrics appear through the Observe
  journeys with bounded queries, filtering, paging, time controls, correlation,
  cancellation, sparse data, and protected payload behavior.
- **AC-004:** The Query journey discovers authorized tables, runs and cancels a
  valid read-only query, renders typed results and a chart, exports returned
  rows, and presents rejection or terminal failure without partial success.
- **AC-005:** The Verify journey presents passed, failed, and inconclusive
  generic results; separates execution state from verdict; opens permitted
  requirement/step detail; handles denied sensitive outcome access; and
  expands drift feature detail only for a drift requirement.
- **AC-006:** Home, Observe overview, Card evidence, and Service summaries
  agree with their canonical Card, signal, and verification detail views for
  the same retained data.
- **AC-007:** Mock and live modes render explicit loading, empty, unavailable,
  denied, stale, partial, failure, sparse, and dense states without cross-mode
  fallback.
- **AC-008:** Every outcome-complete slice has focused component/server tests,
  current Svelte type checking, applicable generated-contract checks, a real
  browser journey for new user-facing live behavior, and valid desktop/mobile
  visual evidence accepted before its dependent visual slice begins.
- **AC-009:** The integrated review environment demonstrates every included
  route and required state without paid providers, customer infrastructure,
  or external credentials.

## Evidence classes

- Focused Svelte component and server-boundary tests run through the repository
  toolchain.
- Svelte type checking and production build for the UI package.
- Deterministic generated-contract drift checks when contract projections are
  added or changed.
- Served OpenAPI contract tests when mounted operations or their schemas change.
- Real authenticated browser journeys against `WyrdTestServer`, repository
  Postgres, and Bifrost for each live capability.
- Exact tenant/permission negative journeys for protected rows and sensitive
  verification or telemetry payloads.
- Desktop and mobile screenshots from the representative review environment,
  inspected for the approved slice against the preserved canvas where one
  exists.
- The final capability-scoped repository verification lanes identified from
  `mise.toml`; broad `mise run gate` is reserved for the integrated change only
  if its final write set remains cross-cutting and no complete capability gate
  exists.

## Open material decisions

None. Reversible choices such as component boundaries, exact icons, breakpoint
values, local presentation helpers, chart implementation, and the TypeScript
generation tool remain implementation-owned within the requirements above.

## Revision history

- **Revision 1 — 2026-10-18 (approved):** Defines the current-main UI integration,
  compact rail, canvas authority, generic verification experience, staged
  visual review, and preservation of the old implementation as migration
  evidence. Approved by the user on 2026-10-18.

## Authority links

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/references/README.md`
- `architecture/references/doctrine/positioning-and-vocabulary.md`
- `architecture/references/doctrine/architecture-constraints.md`
- `architecture/references/architecture/patterns.md`
- `architecture/references/domain/telemetry-observations.md`
- `architecture/references/domain/evaluation.md`
- `architecture/references/domain/drift-monitoring.md`
- `architecture/references/languages/spec-driven-development.md`
- `architecture/references/languages/testing-workflows.md`
