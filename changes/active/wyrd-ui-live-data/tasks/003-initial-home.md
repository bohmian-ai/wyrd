---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 3
requirements: [REQ-003, REQ-004, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008, INV-009, INV-010, INV-011]
acceptance: [AC-002, AC-018]
depends_on: [TASK-002]
---

# TASK-003 — Initial Home with live registry data

## Outcome and Value

Home renders an empty tenant and subsequently registered Cards from real data, with working links and no fixture activity.

Owns AC-002, AC-018. AC-018 is a per-page obligation; AC-017 typed consumer/schema evidence also applies.
The preceding task is an acceptance-order dependency: the user requires one page gate before the next. Shared software dependencies are identified in the reuse map; this order does not authorize unrelated rewrites.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Initial Home with live registry data | UI `WyrdClient.home/cards`, Home `+page.server.ts`, `mockHome`; server `list_cards_http` | Home rendering and links; generated list contract; Cards inventory projection; existing mock Home/Observe journey tests | Home still uses fixture summaries, including illustrative totals/activity. | Load registry-backed Home content through the existing server facade and route; show truthful unavailable operational sections until their live owners land. | None: retain the existing facade, page and view model boundary. |
| Authenticated loading and presentation | `hooks.server.ts`, `BrowserSession.api/context`, existing `+page.server.ts` loads and `WyrdClient` | Production auth integration tests; current route/component consumers; generated operation types | Current mock domain methods do not provide this page’s authoritative asynchronous live data | Extend the current server-only facade/load and retain SvelteKit-generated page types | No duplicate session, transport, contract owner or page-data schema. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, a global service selector, Changes, custom dashboards, raw Query functionality or paid/external provider requirements. Existing foundation tasks are historical context only. The audited extra Changes/dashboard/GenAI/styleguide routes are outside this page-gate scope.

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts; do not generate types from this planning table or create a parallel `/ui/*` API family.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF route | GET /t/{tenantKey} — existing +page.server.ts. |
| URL/action inputs | Current tenant; server load already awaits home(). Register via SDK, then navigate/refresh with targeted invalidation. |
| Existing server operations | GET /v1/cards → ListCardsResponse; optional bounded GET /v1/cards/by-uid/{kind}/{card_uid} for displayed metadata. |
| Required integration gap | Connect live facade; keep authoritative Card identity/status and truthful unknown totals. Do not require future operational page queries before this initial gate. |
| Analytical result contract | Operational summaries become live in task 018. Interim unavailable sections contain no fixtures. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority. SvelteKit loads call the facade directly; no internal proxy HTTP hop or page-specific JSON handler is currently required. Browser OAuth redirects/device approval retain their existing protocol paths. Test inputs, returned shape/status/media type, forbidden/missing identities and URL restoration—not merely that a route returns 200.

## Approach

1. Connect the minimum registry reads through the current authenticated BFF using generated operations.
2. Map truthful Card summaries/counts and unavailable sections without presenting a page subset as a global total.
3. Keep primary content in server-rendered Home data and refresh after real registration through normal navigation/invalidation.

## Ordered Implementation Scenarios

### Scenario 1 — Empty then populated Home

**Behavior.** The same tenant starts empty, registers Cards via SDK and returns to Home with corresponding authorized content and valid links. Mapped obligations: REQ-003, REQ-004, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025; AC-002, AC-018.

**RED.** The planned live acceptance case must detect fixture content or failure to reflect registration. First add the smallest failing projection/load/filter assertion to planned focused case `live UI initial Home projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/wyrd.test.ts -t '^live UI initial Home projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI initial Home`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Wire registry presentation through the existing server load and preserve the component layout. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

### Scenario 2 — Truthful unavailable and denied content

**Behavior.** Out-of-scope or not-yet-integrated operational sections contain no fabricated data; denied/expired/deep-link requests show the right safe state. Mapped obligations: REQ-003, REQ-004, REQ-008, REQ-018, REQ-019, REQ-021, REQ-023, REQ-024, REQ-025; AC-002, AC-018.

**RED.** Extend the case with a restricted session and raw initial HTML assertions; fixtures or leaked content fail it. First add the smallest failing projection/load/filter assertion to planned focused case `live UI initial Home projection`, using typed wire/analytical values and the inspected UI test runtime:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/wyrd.test.ts -t '^live UI initial Home projection$'
```

Also retain this scenario’s full user-visible assertions in the independent live browser case `live UI initial Home`. Its environment-owning command is the page acceptance gate below, not the command for every local iteration. If the missing behavior exists only across real auth/server/browser boundaries, demonstrate RED with that filtered page gate; a unit test does not substitute for it.

**GREEN.** Handle empty/unavailable/denied states explicitly and revalidate requests in the established session boundary. Rerun this case with all earlier scenario assertions retained. For shared behavior, rerun the focused consumer regression below.

**REFACTOR.** Simplify within the existing owner; reuse current controls, types and helpers. Preserve validation, error handling and earlier scenario evidence. Already-correct branches receive regression proof, not an artificial failure.

## Acceptance Criteria

- Both ordered scenarios pass using the applicable owners, including exclusions and edge/failure branches; the separate live page acceptance gate proves all user-visible branches against real data.
- Every required BFF input reaches the identified upstream operation/query with correct scope; generated response/schema mapping and denied/missing/error behavior are recorded in the page evidence.
- The page is accepted independently against SDK/HTTP/Bifrost evidence in the shared disposable setup with mock identity/content disabled. Domain evidence is registered/emitted/evaluated through public pathways, never inserted as prepared results.
- Primary content or truthful empty/error state exists in initial HTML before hydration. URL controls invoke SvelteKit server loads; hydration does not redundantly fetch initial data. Protected upstream calls remain server-only. Preloads are read-only/bounded and every navigation revalidates session authority.
- Context is visible, changeable and clearable where applicable. Continuation links preserve applicable service/time/selection state; relationship links select the target’s identity. Filters, pagination and counts agree with authoritative reads under equivalent settled scope.
- Missing, denied, unavailable and schema-invalid data never silently falls back to fixtures. Work remains bounded and supported terminal/visibility semantics are preserved.
- Record a human usability pass or concrete corrected issues with scrubbed local screenshots; review control discoverability, scope clarity, back/refresh behavior, accessible interaction and error recovery.

## Expected Write Set and Consumer Closure

Home load/rendering and WyrdClient registry presentation, generated consumers, shared setup Card registration and focused browser case.

Close callers, tests, fixtures/setup, package/lock/config and runtime consumers of each changed owner. Shared/private helpers may move only when that earns a smaller cohesive implementation. The existing identity host must discover and forward the planned exact case selector; adding a split test file includes discovery changes so the command cannot select zero tests.
When changing a shared projection or method to asynchronous live IO, update every in-scope load/caller and retain mock/component behavior. Analytical result schemas are not HTTP DTOs: validate canonical columns/types before interpretation and reuse existing SDK decoding.

## Verification and Evidence

Focused RED/GREEN iteration:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/wyrd.test.ts -t '^live UI initial Home projection$'
```

The planned focused test module belongs to the existing `WyrdClient` server-facade owner; the inspected Vite include pattern discovers it. Introduce it as pure projection/load-seam proof, not another environment lifecycle.

Page acceptance (run the filtered real browser case after the local behavior is coherent; it must still cover every user-visible scenario):

The following exact journey case is planned under the inspected test owner/lane; introduce its assertions before implementing missing behavior. Correct existing branches are verification-only evidence. This is a mixed task when a generic implementation already satisfies part of the page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI initial Home' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

Run `mise run codegen:check` when HTTP generation/consumption changes, and record which generated operation types the page consumes. Identify canonical analytical columns/types and prove schema/terminal mapping where used.
For actual Rust changes, run `mise run fmt` and `mise run lints`; for changed dependency/owner boundaries run `mise run check:deps`. Add only the narrow owning format/lint/test checks justified by the actual write set.
Do not run full identity/SDK journey suites or a broad aggregate as this task’s gate. Integrated verification belongs once to change review. Existing authentication/audit behavior needs regression proof, not manufactured RED.
Retain initial-response/network evidence, expected data identities and exclusions, readiness bounds and a scrubbed usability review. Record page pass/fail and mapped obligations; do not claim an isolated page proves the whole change.

## Material Stop Conditions

Stop for specification revision if the required outcome genuinely needs a new public/persisted contract, migration, permission/audience change, durability/freshness guarantee, engine behavior or ownership exception. Do not silently substitute a new read API or unbounded scan for a missing contract.
Report a concrete blocker if repository-managed setup cannot supply the required real evidence safely or a canonical schema cannot support truthful required presentation. Reversible private mapping/tooling choices remain implementation-owned; correct mock assumptions rather than inventing domain state.

## Authority Links

- [Server/BFF route audit](../route-audit.md)

- [Approved specification, revision 3](../spec.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Protocol doctrine](../../../../architecture/wyrd-doctrine.mdx)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)


## Implementation Evidence

**Reuse map (revalidated).** Owner `WyrdClient.home()` (`wyrd-ui/src/lib/server/wyrd.ts`) now has a live branch. It reads `GET /v1/cards` (`list_cards_http` → `ListCardsResponse`/`CardSummary`, generated `wyrd-http.d.ts`) through the existing `BrowserSession.api`, which `hooks.server.ts` now passes to the facade. Upstream refusals map through the existing `problemKind`. The Home `+page.server.ts` load, `HomeView`, the page layout and the mock projection are retained. Gap: Home had no live source. Extension: one private `liveHome` projection with a typed `CardLifecycleStatus → tone` map. There is no new route, transport, session or schema.

**Decisions.**
- Home reads only the server's default first page (50). A page with `next_cursor` shows `N+ registered` and a "more are registered" detail, never an inferred total; the `ponytail:` note marks the ceiling.
- Registered Cards are shown newest first within that page, with links to `/t/{tenant}/cards/{card_uid}`.
- Attention and the Observe summary render as explicitly unavailable (`null` section/value). Changes (out of scope) and "Pick work back up" (no evidence source) are omitted in live mode, along with the New Change Request action.
- Live mode leaves authority to the server; the mock branch keeps its existing permission gate.
- "Recently viewed Cards/Seen" was renamed "Registered Cards/Registered", because the UI has no view history.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Scenario 1: an empty-baseline tenant, then Cards registered via SDK, shows on Home with valid links | `wyrd.ts` `home()`/`liveHome`; `+page.svelte` | `live UI initial Home`: Home equals the authoritative `GET /v1/cards` before and after, and the `wyrd apply` Prompts appear with `card_uid` links; `live UI initial Home projection` | PASS |
| Scenario 2: unavailable sections show no fixtures, and denied gets the safe state | `null` sections and `Unavailable` summary; server 403 → `reject('denied')` | Raw HTML has no fixture strings and shows the "Not available yet"/unavailable text. A no-Role principal gets 403 `WYRD_PERMISSION_403_DENIED_RBAC` with no Card names. Another tenant's reader sees only its own Cards. Unit test covers 403 and the no-session 502 | PASS |
| BFF input reaches the upstream with the session scope; generated mapping; denied/error behavior | `session.api('get','/cards')`, `components['schemas']['ListCardsResponse']` | Projection test (call recorded as `get /cards`); `pnpm check` 0 errors; `mise run codegen:check` exit 0 | PASS |
| Primary content in initial HTML; client navigation by server load; hydration does not refetch; no browser `/v1` calls | Existing `+page.server.ts` | Browser case: initial `GET` HTML contains Cards; nav-link click fetches `/t/{t}/__data.json` with zero browser `/v1/` requests; reload issues no `__data.json`; deep link then Back restores Home | PASS |
| No fixture fallback when missing/denied/unavailable | Live branch has no mock path | Unit test (no session → 502, 403 → 403); `journey.test.ts` dev live-mode asserts 502 with no fixtures | PASS |
| Usability pass | Screenshots `target/debug/wyrd-ui-live-login/home-{before,after,denied}.png` (local, not checked in) | Reviewed; issues below | PASS (issues recorded) |

**Usability issues (not fixed; outside this page's minimal scope).**
1. At 1280px the Cards table's fixed `min-width: 740px` pushes the Registered column behind a horizontal scroll. The same was true of the mock "Seen" column.
2. A denied Home renders the root error page without the Shell; "Return to tenant entry" is the only recovery. This was already noted in TASK-002.
3. The page subtitle ("what changed, and where to pick work back up") overstates content while those sections are unavailable.
4. The header shows a raw principal UUID (known from TASK-002).

**"Empty" baseline.** Every harness tenant is seeded with its bootstrap owner's Service Card, so the starting state is that one authoritative Card rather than zero. The zero-Card empty state is proven in the projection unit test.

**Card links.** Links target the exact `card_uid`. The Card detail and inventory pages remain mock-backed until TASK-004/005.

**Commands.**
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/wyrd.test.ts -t '^live UI initial Home projection$'`: RED (403 from the mock permission gate), then GREEN.
- `vitest run src/lib src/routes`: 182 passed.
- `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI initial Home' mise run test:identity:journey`: 1 passed, 5 skipped.
- `pnpm run check`: 742 files, 0 errors.
- `pnpm run build`: ok.
- `mise run codegen:check`: ok.
- `git diff --check`: clean.

**Gate-run diagnoses.**
- `wyrd apply <dir>` refused two independent Prompts: `WYRD_LOADER_400_INVALID_ENVELOPE`, multiple roots. A directory is one Card tree, so each file is now applied separately.
- The CLI refused a world-readable `credentials.toml` (`unsafe_store`). It is now written with mode 0600.
- `locator('aside')` matched the nav and the summaries. The summaries are now selected by accessible name.
- `goBack()` resolved before the URL changed. The test now waits on `waitForURL(home)`, a condition wait.

**Not run.** `fmt`/`lints` (the Rust change is a two-line test-host fixture) and `check:deps` were skipped at the user's direction ("dont do lints"). `$wyrd-task-review` was not run.

**Non-goals.** No public/persisted contracts, migrations, compatibility routes, Changes, dashboards or Query work. No unrelated files were changed.
