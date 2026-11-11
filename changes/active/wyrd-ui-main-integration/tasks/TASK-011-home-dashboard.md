# TASK-011 — Home dashboard

Spec: [revision 28, approved](../spec.md). Status: Proposed; completion requires the dependencies and evidence below.
Dependencies: [TASK-005](TASK-005-cards-and-prompts.md), [TASK-007](TASK-007-verification.md), [TASK-008](TASK-008-card-workspaces.md), [TASK-010](TASK-010-service-workspace.md), and the completed foundation/Observe/view-layout baseline.
Mapped obligations: REQ-019, REQ-020, REQ-022, REQ-023, REQ-030, REQ-034, REQ-062; AC-009, AC-010, AC-011, AC-013, AC-016, AC-017, AC-041; INV-001–INV-007 apply.
Absorbs the former Home task and CardInventorySummary obligation; consumes the earlier delivery tasks' accepted attention, signal, binding and Observe components without reopening their ownership.

## Outcome and Value

The signed-in tenant landing page becomes a working dashboard of actual Cards,
Services, recorded verification attention and observed activity. A reader can
move from a summary into the exact Card, Service, result or Observe investigation
that explains it. Starting `mise run dev:full` supplies populated examples for
this page and a gallery checkpoint for its reusable inventory summary.

## What Changes and What Must Stay True

Own `CardInventorySummary`, its portable loader, catalog/input registration,
fixtures and gallery, together with the dashboard at `/t/[tenantKey]`.
Compose `AttentionList`, `SignalBreakdownTable`, `StatStrip` and applicable
verification/Observe components from their existing package owners.

The Home board's greeting/count summary, attention, Services with recorded
verification, inline Observe and Cards/recent Cards are all required. No region
may disappear because its source is empty, denied or temporarily unavailable.
An empty region explains the registration, observation or binding that would
fill it; failures show the actual stable problem. Pending or absent coverage,
inconclusive results, failed judgments and passes remain distinct. A successful
query or a missing result never becomes a passing verification.

All Card links retain exact kind/name/version and returned UID. Service and
attention scopes use the selected `workload_card_uid`; subject and account
dimensions remain distinct. Summary arithmetic counts recorded facts only.
Any bounded/truncated list retains that limitation rather than presenting a
partial page as a total. Recent ordering comes from returned contract fields;
it is not inferred from semantic versions or fabricated activity timestamps.

The existing browser session supplies the Node executor in server loads.
Package loaders own query planning/shaping; the page owns layout and host
navigation only. Never add a Home aggregation service, direct browser backend
access, credentials in props, frontend judgment logic or a parallel data cache.
Tenant changes or range changes supersede previous loads; a failed panel must
not prevent healthy siblings from rendering or navigating.

Applicable Change Request attention remains dependent on the separately
approved, shipped platform contract consumed by TASK-009/010. Do not fabricate
rows or call a partial dashboard complete while required attention is blocked.
Ask, saved views, new summary pages and durable layout persistence remain out of
scope. Preserve existing sign-in, Settings, Observe and exact-query navigation.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Home.dc.html](../references/canvas/Home.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Reuse the existing inventory read and portable loader/executor boundaries for the summary.
2. Add generated catalog/view coverage and individually inspect the inventory summary in the gallery.
3. Compose every Home board region using the same package components and authorized loaders.
4. Extend the existing development setup with any missing representative registrations/results; use public workflows and report readiness truthfully.
5. Prove summary reads and the real signed-in dashboard journey, then obtain separate component and page visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Reusable inventory summary

**Behavior.** The component renders counts and recent registered Cards from the
actual registry projection, with exact navigation intents. Populated, empty,
loading, denied and error props work through direct component use and a
catalog-validated `WyrdView`. Invalid panel input remains a local error; long
names and narrow widths remain usable. No data becomes a zero only when the
successful source actually establishes zero.

**RED.** Add planned [inventory summary proof](#inventory-summary-proof). Assert
all states, actual loader requests and returned paging/identity semantics,
validated navigation intents and catalog rendering. The missing
component/catalog/loader must fail those observations; scenario 2 also checks
the actual built AppBridge artifact through its existing integration owner.

**GREEN.** Implement the package component and portable loader over existing
Card reads; include it in the public exports, catalog, view renderer, built
artifact and gallery. Run this scenario before mounting its first Home consumer.

**REFACTOR.** Reuse the inventory projection and host intent conventions already
delivered by Cards; keep private module and fixture choices implementation-owned.

### Scenario 2 — Authorized summaries and independent failures

**Behavior.** With real registered Cards and recorded signal/verification data,
Home's loaders return the selected tenant's inventory and summaries. Two
Service releases sharing a component/account remain distinct. Empty data,
missing coverage, inconclusive/failing/passing results and source denials stay
distinct; another tenant's rows and credentials never appear. A failed attention
read leaves readable inventory and signals available. Superseding a range or
tenant prevents stale updates and stops dependent reads after refusal.

**RED.** Add planned [authorized summary proof](#authorized-summary-proof)
against `WyrdTestServer` and repository Postgres, using public registration,
observation and verification workflows. Assert actual Node and MCP read
equivalence, denial/cross-tenant boundaries, exact identities, recorded verdict
semantics and sibling isolation. Also render the summary in the actual built
AppBridge artifact with labeled fixture tool replies (T4). That artifact check
cannot substitute for the separate real authorized reads (T5).

**GREEN.** Compose existing shared loaders and close any summary-specific
read-shaping gap at that owner. Record sanitized populated/empty fixtures from
the real results. Rerun this scenario and the inventory summary proof.

**REFACTOR.** Remove duplicated summary shaping in favor of its existing package
owner; do not move data behavior into the route or add a dashboard service.

### Scenario 3 — Working signed-in Home

**Behavior.** A human signs in through public local login and reaches all Home
regions at `/t/[tenantKey]`. Selecting a Card, Service, attention result or
Observe summary opens its exact investigation, preserving applicable range.
Changing range refreshes time-aware regions consistently without stale
overwrite. Desktop/mobile and keyboard navigation work in both themes.
The normal development launch presents representative real content after setup;
if setup fails, it reports that failure rather than substituting fixture props.

**RED.** Add planned [authenticated dashboard proof](#authenticated-dashboard-proof)
using the current real-browser BFF lifecycle, public human enrollment/sign-in,
and real server data. Assert every board region and navigation target, range
reads, empty/denied/error/cross-tenant states and absence of serialized secrets.
The current identity-only landing page cannot satisfy the test.

**GREEN.** Mount the completed shared components on the tenant landing route
using session-bound server loads and existing host actions. Extend the existing
development seed/setup only where its data does not exercise the dashboard.
Rerun all scenarios and manually exercise the normal development launch.

**REFACTOR.** Keep layout/control state in the route and all domain shaping in
the shared package; preserve existing shell/login/Settings behavior.

## Acceptance Criteria

| Obligation | Required observation and proof |
|---|---|
| REQ-019/022; AC-009 | Every Home board region renders its actual source, truthful absence or stable problem. Scenario 2 rejects inferred passing coverage; scenario 3 rejects missing regions. |
| REQ-030; AC-011 | CardInventorySummary has public exports, loader/catalog/view/T1–T5/gallery coverage and its working Home consumer. Scenarios 1–2 include the built artifact, not a source-only substitute. |
| REQ-023; AC-016/017 | Real authorized reads retain canonical catalog fields, exact releases and truthful null attribution. Scenario 2 proves tenant/permission and shared-component separation. |
| REQ-034/062; AC-013/041 | Real-browser human sign-in/navigation/interactions succeed against the production BFF/server; normal dev:full exposes representative live content without manual database edits. Scenario 3 plus launch evidence. |
| REQ-020; AC-010; INV-001–007 | Individual inventory-summary and Home desktop/mobile light/dark acceptance, keyboard/focus/touch/overflow evidence; no page queries, invented contracts or credential leakage. |

## Implementation Reference

Paths describe existing owners and affected consumers, not an implementation allowlist.

- **Registry:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards.list` and the Node
  executor's `cards.list` branch already provide authorized paging. Reuse the
  TASK-005 inventory loader's discovered semantics rather than introducing a
  second registry/count source. `tests/support/server.ts::serverTest/declared`
  provides real registrations for the existing UI integration runner.
- **Portable projection:** `packages/ui/src/lib/loaders.ts::loadPanel`,
  `schemas.ts::components`, `scripts/catalog.ts` and `WyrdView.svelte` already
  own input validation, catalog generation and dispatch. Extend them for the
  summary; close public package exports, `dist` contents and view fixtures.
- **Current consumer:** `src/routes/t/[tenantKey]/+page.svelte` is currently
  an identity landing page. Its tenant layout and
  `src/lib/server/auth/browser-sessions.ts::BrowserSession.client` already own
  navigation/session scope. Observe server loads demonstrate
  `loadPanel(..., {executor: nodeExecutor(session.client())})`.
- **Proof:** `src/lib/ui-library.integration.test.ts::appBridgeView/mcpOver`
  owns built-artifact and actual MCP executor support.
  `src/lib/ui-pages.integration.test.ts::startBff/humanSignIn` and its
  `serverTest` extension own the production BFF/Playwright lifecycle. Extend
  these owners rather than copying deployment/auth setup.
- **Development:** `mise.toml::dev:full` and
  `scripts/dev/example-app.sh::setup/batch` own local setup/workload. Preserve
  the production SDK path, optional MockServer and fresh-database semantics;
  add only missing representative public registrations/results and readback.

## Verification and Evidence

Commands below select **planned additions** to existing runners. Confirm actual
selectors after adding the tests, before RED; zero-selected/skipped cases are
not proof. Use [managed setup](../references/verification.md): current SDK and
testing addon, server and UI/package/artifact builds, repository Postgres and
the already installed Playwright engine. No new selector environment or runner.

### Inventory summary proof

T1/T2/T3 component, loader and view assertions:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^card inventory summary preserves registry facts and states$'
```

### Authorized summary proof

T4 actual built-artifact rendering and separate T5 real Node/MCP server reads,
with sibling/permission/identity assertions:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^home summaries retain authorized recorded facts$"'
```

### Authenticated dashboard proof

T6 production-browser page journey through public human login:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated home dashboard$"'
```

### Scoped checks and visual checkpoint

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
mise run dev:full
```

Run codegen only when catalog/generated inputs change; never hand-edit outputs.
The normal launch's login output is authoritative. Visit `/t/dev`, follow every
summary destination, and inspect `/_gallery` for CardInventorySummary. Record
source/browser identity, setup/readback success, board region, state, viewport,
theme and sanitized screenshots/traces. Inspect Home populated/no coverage,
failed-source, denied and mixed recorded verdicts separately; use the gallery
for fixture-only loading/error variations. Individual component and page human
acceptance are separate from automated passes. Pause before the next dependent
slice until that inspection occurs. Final integrated cross-surface lanes belong
to change review, not this task's iteration loop.

## Material Stop Conditions

A missing approved Change Request attention projection or a prerequisite that
prevents a required summary from becoming live blocks that region and whole-task
completion. Complete independent work and report the exact dependency; do not
guess domain payloads or omit the region. A new durable aggregation/count API,
credential boundary or judgment policy needs specification authority. Ordinary
private layout/helper/test choices remain implementation-owned. Missing human
or platform proof remains unverified, never an inferred pass.

## Authority Links

- [Approved spec](../spec.md), [plan and coverage](../plan.md), [approved canvases](#approved-canvases), [audit rules](../references/task-audit-2026-10-29.md).
- [Repository rules](../../../../AGENTS.md), [execution](../../../implementation-execution.md), [TDD](../../../spec-driven-development.md), [testing](../../../../TESTING.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [Bifrost](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [UI skill](../../../../.agents/skills/wyrd-ui/SKILL.md).
