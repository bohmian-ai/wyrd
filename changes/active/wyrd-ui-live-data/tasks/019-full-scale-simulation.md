---
id: TASK-019
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-live-data
spec_revision: 4
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-023, REQ-024, REQ-026]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-006, INV-007, INV-008, INV-011]
acceptance: [AC-019, AC-018]
depends_on: [TASK-018]
---

# TASK-019 — Full-scale simulation of Wyrd and its UI

## Outcome and Value

The UI handles a realistically loaded tenant. Twenty Services are registered with their composed Cards and executed repeatedly, producing traces, logs and metrics, evaluated by Eval and Drift bindings. Every included page shows every Service's evidence correctly and completely, and timings are recorded so load behavior is visible.

Owns AC-019 and REQ-026. AC-018 applies to each page visited at scale. Pages accepted in tasks 003–018 are revisited at scale; a defect found here is fixed in that page's existing owner, and the page's own gate is rerun.
The preceding task is an acceptance-order dependency: the simulation runs only after every page gate and the combined journey are accepted.

## Owners, Scope, Consumers, and Prohibited Changes

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`; server paths are relative to `crates/wyrd/wyrd-server`. Named symbols and tests describe inspected ownership, not a private-file allowlist. Use `wyrd-implement` and, for UI edits, `wyrd-ui`.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Scaled workload generation | The shared workload setup that TASK-009 makes count-parameterized (registration through `wyrd apply` and SDKs, actual executions, OTEL export/flush/shutdown, Eval and Drift bindings, bounded readiness), extended by tasks 010–017 | `production-auth.integration.test.ts` helpers `applyCards`, `authoritative`, `uidOf` and the later page cases' setup; `identity_ui_e2e.rs::production_ui_bff_journey` host | Nothing drives the setup beyond the page gates' two Services, so no proof exists at twenty | Run the same setup with a Service count of twenty from one new browser case | None: no seeding script, SQL insert, bulk-load endpoint or second harness. Scale is a parameter of the existing public-path setup. |
| Page correctness at scale | Live loads accepted in tasks 003–018: `WyrdClient.home/cards/card/observe*`, the Service workspace, and their `+page.server.ts` loads and paging controls | The page cases `live UI initial Home` … `live UI Drift` and the combined journey; inventory cursor paging from TASK-004 | Paging, filters, counts, charts and summaries are proven only on small datasets | Assert full-population coverage and agreement with authoritative reads on the twenty-Service dataset; fix any defect found in the page's existing owner | None: no cache, pre-aggregation store, summary endpoint or per-page fan-out across Services. |
| Journey host and selection | `mise.toml::test:identity:journey` `run_ui` (required-selector check, `WYRD_IDENTITY_FILTER` forwarding), `identity_ui_e2e.rs` | The existing required UI selectors | No simulation case | Add one exact case `live UI full-scale simulation` to the existing UI journey file and its required-selector list | None: no new lane, environment controller or benchmark framework. |

Keep native Card/Verifier identity and exact attribution. Effective tenant/permissions come from the current verified session, never from URL scope; secrets remain server-only. Preserve actual server permission decisions and non-blocking audit staging. The BFF never reads databases/object storage directly or replaces registry/verification/query engines.
Do not add public/persisted contracts, migrations, compatibility routes, signal read APIs, bulk-load or test-only server endpoints, a global service selector, Changes, custom dashboards, raw Query functionality, paid/external provider requirements, or a response-time SLO. Timings are evidence only (INV-008 as clarified by revision 4).

## Required Server and BFF Contracts

[Source-backed route audit](../route-audit.md) defines operation inputs/responses, permissions, structured errors, canonical query schemas and the route-to-task matrix. Reuse those mounted contracts.

| Boundary | Required behavior and contract |
|---|---|
| Existing BFF routes | Every route accepted in tasks 003–018: Home, Cards inventory, Card detail (Model, Data, Agent, Prompt, Service), Observe overview, traces and trace detail, logs, metrics, evaluations and task detail, drift. |
| URL/action inputs | The existing service, time, kind, filter, cursor/page and selection parameters of those pages. |
| Existing server operations | Exactly those the accepted pages already use: Card list/by-UID/by-ref/versions, verification binding/run reads and `POST /v1/query` through the SDK. No new operation. |
| Required integration gap | Prove behavior at twenty Services; repair only defects that scale exposes. |
| Analytical result contract | The canonical result tables each page already validates. Authoritative comparisons use the same scope through the administrator's SDK/HTTP reads. |

Use generated HTTP operation types and SvelteKit-generated route data types. Protected domain reads remain server-only with current session authority.

## Approach

1. In the existing UI journey file, run the shared workload setup with twenty Services through public pathways, then settle it with the setup's bounded readiness over the whole population.
2. Visit every included page in the real browser and compare what is displayed against authoritative reads under equivalent scope, covering every Service and every page of each population.
3. Repeat the restricted-user and other-tenant exclusion checks against the scaled tenant.
4. Record per-page server-render and client-navigation timings with the dataset sizes they were measured on.
5. Fix any scale-exposed defect in the owning page or facade, and rerun that page's own gate.

## Ordered Implementation Scenarios

### Scenario 1 — Twenty Services settle through public pathways

**Behavior.** With a Service count of twenty, the shared setup registers each Service with its composed Model, Data, Agent and Prompt Cards. It executes every Service repeatedly, emitting traces, logs and metrics with a normal flush/shutdown. It produces passing and failing real-time and continuous Eval results and Drift results from fitted baselines with healthy and shifted observations. Readiness waits, bounded, for the whole population and reports useful diagnostics on timeout. Mapped obligations: REQ-016, REQ-017, REQ-026; INV-006, INV-007; AC-019.

**RED.** Add the case `live UI full-scale simulation` to `src/lib/server/auth/production-auth.integration.test.ts` and to the `run_ui` required-selector list in `mise.toml`. Its first assertions check that authoritative reads hold twenty Services with their components, executions, Eval results and Drift reports. Expect failure wherever the setup still assumes two Services or its readiness covers only part of the population:

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI full-scale simulation' mise run test:identity:journey
```

**GREEN.** Generalize only what the twenty-Service run exposes in the shared setup (identities, names, readiness scope). Keep its public pathways and deterministic local provider. Rerun this case and the page gates whose setup changed.

**REFACTOR.** Keep one setup with a count parameter, not a separate scale copy. Preserve the page gates' small datasets and their assertions.

### Scenario 2 — Every page shows every Service's evidence correctly

**Behavior.** On the settled dataset:
- **Home:** summaries and links agree with the underlying pages.
- **Cards inventory:** paging covers all registered Cards without overlap, and kind/metadata filters match authoritative reads.
- **Detail pages:** every Service's workspace shows its exact composition and only its own evidence.
- **Observe overview:** lists all twenty Services.
- **Traces, logs and metrics:** traces and logs page through each Service's full population without overlap. Metrics series match retained points for chosen Services and windows.
- **Evaluations and drift:** list every Service's results, with counts, pagination and drilldowns agreeing with authoritative reads.

Nothing silently truncates, times out or falls back to fixtures. Mapped obligations: REQ-004–015, REQ-018, REQ-019, REQ-023, REQ-024, REQ-026; INV-008, INV-011; AC-018, AC-019.

**RED.** Extend the same case page by page with presence and exclusion assertions against authoritative reads. For paged populations, also assert the union of settled pages and that no page repeats rows. A failing assertion identifies the page and owner the scale exposes:

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI full-scale simulation' mise run test:identity:journey
```

**GREEN.** Repair each exposed defect in the page's existing owner (its facade method, load, projection or paging control), keeping per-page work bounded independently of the Service count. Rerun this case and the repaired page's own gate, for example `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Logs' mise run test:identity:journey` using that page's exact accepted selector.

**REFACTOR.** Simplify within the existing owners. Do not add caches, pre-aggregation or per-Service query fan-out to make a page pass.

### Scenario 3 — Isolation at scale, and recorded timings

**Behavior.** Another tenant's reader and a restricted principal see none of the scaled tenant's Cards, rows or details on any visited page or by URL manipulation. The case records each page's server-render time (initial HTML) and client-navigation time (`__data.json`) with its dataset size. The timings are captured as evidence and do not gate the run. Mapped obligations: REQ-003, REQ-018, REQ-026; INV-001–004, INV-008; AC-019.

**RED.** Add the exclusion assertions and timing capture to the same case. The exclusion assertions fail if scale exposes a leak; the timing capture fails only if it cannot record a page.

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI full-scale simulation' mise run test:identity:journey
```

**GREEN.** Fix any leak at the owning authorization or facade boundary. Write the timing table to the task evidence, with screenshots kept under the existing local evidence directory.

**REFACTOR.** Reuse the existing tenant, reader and no-role principals and login helpers.

## Acceptance Criteria

- The twenty-Service dataset is produced only through public registration, execution, OTEL export and verification pathways, and settles under bounded readiness with diagnostics on timeout.
- Every included page shows every Service's authorized evidence. Every paged population is covered completely without overlap, and filters, counts, charts and Home summaries agree with authoritative reads under equivalent settled scope.
- No page silently truncates, times out, renders a failed query as empty, or falls back to fixtures. Per-page work is bounded independently of the Service count except where displayed rows are per-Service and limited by page size.
- AC-018 holds for each page at scale: primary content is in the initial HTML, URL controls drive server loads, and the browser makes no direct `/v1` calls.
- Another tenant and a restricted principal see nothing protected from the scaled tenant.
- A timing table (page, dataset size, server-render time, navigation time) and scrubbed screenshots are recorded in the task evidence, together with a human usability note on how each page reads at scale (density, scanning, paging, chart legibility).
- The page gates still pass on their small datasets with their assertions intact.

## Expected Write Set and Consumer Closure

- The UI journey file: the new case plus any count generalization of the shared setup helpers.
- `mise.toml`: the required UI selector list.
- Page/facade owners only where scale exposes a defect, each with its page gate rerun.

No new lane, script, endpoint, fixture store or benchmark dependency. A longer per-case timeout for this one case is an implementation choice.

## Verification and Evidence

Simulation acceptance:

```bash
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI full-scale simulation' mise run test:identity:journey
```

For each page owner repaired, also run that page's accepted exact selector through the same lane, plus:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
```

Add the focused vitest projection case of any repaired owner by its exact `-t '^…$'` selector. Run `mise run codegen:check` only if generated HTTP consumption changes. For actual Rust changes, run `mise run fmt`, `mise run lints` and the owning crate lane.
Do not run the full identity/SDK journey suites or a broad aggregate as this task's gate; the integrated change review runs them once.

## Material Stop Conditions

- **Spec revision required:** stop if completeness at twenty Services needs a new public/persisted contract, a server-side summary/count/bulk endpoint, a migration, or a permission change. Do not substitute an unbounded scan or per-Service fan-out.
- **Concrete blocker:** report one if the repository-managed environment cannot produce or settle the twenty-Service dataset within its supported resource and time limits. Include the measured setup time and the stage that stalls rather than reducing the count.
- **Recorded, not fixed:** timing findings are evidence. Making them pass/fail requires a spec revision that sets thresholds.

## Authority Links

- [Approved specification, revision 4 (REQ-026, AC-019)](../spec.md)
- [Server/BFF route audit](../route-audit.md)
- [Repository standards](../../../../AGENTS.md)
- [Agent rules](../../../../architecture/agent-rules.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Security posture](../../../../architecture/wyrd-security-posture.md)
- [Spec-driven development](../../../../architecture/references/languages/spec-driven-development.md)
- [Implementation execution](../../../../architecture/references/languages/implementation-execution.md)
- [Testing workflows](../../../../architecture/references/languages/testing-workflows.md)
