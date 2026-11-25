# Independent findings validation — TASK-001

Result: **FIX_REQUIRED**. One retained finding; discovery proposals MNT-001 and RS-001 describe the same absent contract owner update.

## Immutable subject and inputs

Repository `/home/thorrester/Documents/GitHub/wyrd`; detached source `/tmp/wyrd-task001-review-dd98`; base `7b36f8fc6`, candidate `dd98e9ef78d20e7de47e7af623644fed0ff9075a`. First cumulative task review; no earlier verdict, remediation task or acceptance is carried forward. Read all selected reports, review-scope.md, verification.md, original task at candidate, actual range, applicable root/UI/brand/testing authority, and approved working-tree revision-2 spec. The spec SHA-256 remains `ab12f59e6c2582e711ca4735f1933a1c15f54f3ab33c3c97f7bf0d12e7708e84`. Candidate HEAD and tracked diff remain unchanged. CodeGraph was used before source navigation; final proof comes from candidate source.

User-directed scope: review the implementation while ignoring its recorded BLOCKED status. This removes status-label reasoning, not source obligations. Missing human visual acceptance and the known ungreen integration aggregate remain disclosed verification limits; this audit grants neither human acceptance nor a green integration lane. They are not the cause of the finding below.

## Independently validated specialist applicability and coverage

| Boundary | Selected coverage | Validation |
|---|---|---|
| Exact declared topology, interaction, page/version, errors and non-goals | behavior-rev, complete task-review-behavior.md matrix | Appropriate comprehensive first review. Projection/layout/component/route/test evidence covers current task obligations; matrix candidly distinguishes accepted implementation from outstanding visual proof. |
| New graph projection, geometry and binding-loader ownership | maintainer-rev, maintainer-review.md | Required and complete. Existing layout.ts is a panel grid, while this graph ranks dependencies and routes arrows; existing Prompt UsedBy has a different domain traversal. New code uses existing executor/catalog/view lifecycle rather than substituting a transport or registry. |
| Package/catalog/export/build/dev/test delivery rules | repo-rev, standards-review.md | Required and complete across actual changed layers. No Rust/Python/SQL implementation in range; no new rustdoc/PyO3/transaction gate is hidden by the UI focus. Contract omission below survives source validation. |
| New dependent Card reads and current authorization | security domain-rev, domain-review-security.md | Required and complete. New readVerifier/loadRunsOn orchestrate only cards.get_by_ref under host Context. Read refusal exits before owner reads; failed owners retain explicit problems; report traces Node/MCP/BFF to existing server authorization and tenant SQL owners. No persistent-data/concurrency authority changed. |
| Complex shared-state lifecycle | invariant-rev omitted | Omission justified. VerifierGraph state is local selection/viewport/node arrays. WyrdView only adds component registrations; its existing abort/generation ownership is unchanged and reviewed by behavior. No new shared cache, lock, durable generation or coordinator exists. |
| Material system failure/recovery | system-rev omitted | Omission justified. Diff has no process, service admission, restart, retry or recovery transition. Build NODE_ENV replacement changes browser artifact viability, covered by behavior/repository reports and package build. The dev script adds one normal declaration input to an existing loop. |

All selected reports are present; no missing required specialist was found. No follow-up is required: MNT-001 and RS-001 have one consistent source cause and correction, while behavior/security propose no conflicting defect. No new blocker was discovered during validation.

## Proposal reconciliation

| Discovery ID | Decision | Source-backed rationale |
|---|---|---|
| MNT-001 | CONFIRMED, merged into FIND-TASK-001-1 | Explicit existing brand component-contract rule applies to newly exported components; entries absent at candidate. |
| RS-001 | CONFIRMED, merged into FIND-TASK-001-1 | Same producer/export-to-contract gap, not a second defect. |
| Standards note: exact named-test commands | Bounded evidence-record correction; batch with remediation, no separate finding | Candidate TESTING.md:91–111 explicitly requires named TypeScript tests to carry runner, path and selector, including setup wrapper for live tests. Task Evidence lists five actual selectors but only file-wide command blocks. Add exact runnable selector commands for those recorded tests without changing assertions or claiming a retrospective RED. This is a small required documentation correction; no independent implementation failure or new test harness follows from it. |

## Validated ledger

### FIND-TASK-001-1 — CONFIRMED — VIOLATION

**Discovery sources:** MNT-001, RS-001.

**Violated obligation:** `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md:24–26` defines standardized pieces by matching contract and implementation. Its Building a new component instructions at lines 259–262 require the contract in `components.json`, built status and test. The candidate UI skill names this brand directory as authority. TASK-001 extends the shared component/catalog owners; no exception waives this required delivery step.

**Exact locations:** new public exports in `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/index.ts:17–19`; two placeable schemas at `packages/ui/src/lib/schemas.ts:521–535`; missing names under `brand/components.json:27` (`components`).

**Producer-to-consumer trace:** `VerifierDefinition.svelte` produces description, graph and selected declaration from typed loader props; it composes `VerifierGraph.svelte`. `RunsOn.svelte` produces exact owner/binding rows and the existing card.open intent. index.ts exports all three. schemas.ts exposes VerifierDefinition/RunsOn through the catalog; loaders.ts and WyrdView register their existing portable delivery path. Separately, brand/DESIGN.md:11 identifies components.json as the machine-readable per-component contract consumed by agents/A2UI. That existing owner has none of the three names. I read the complete two data component bodies and graph props/state owner, actual exports/schema registrations, contract file format and owning design instructions. Read-only Node inspection returned brand=false for all three, catalog=true for VerifierDefinition/RunsOn and false for VerifierGraph. The task diff does not include components.json. Dirty original-workspace entries are outside this immutable subject.

**Reachable consequence and materiality:** an agent or maintainer using the required machine-readable component contract cannot discover the shipped components' intended props, anatomy, token rules or placement constraints, while runtime/catalog consumers can use them. More directly, the candidate fails an explicit mandatory component delivery contract. This is a bounded missing deliverable, not a preference about layout, names or line count. Existing palette/generator and catalog checks do not prove this contract parity: the former tests projections and the latter derives from schemas.ts, not components.json.

**Smallest safe correction:** update only the existing components.json owner with accurate built contracts for VerifierDefinition, RunsOn and VerifierGraph, using its established data shape and actual component behavior. Mark the two catalog panels eligible and the graph primitive ineligible, matching actual catalog placement. Preserve unrelated pending brand changes. Do not add an inventory, generator, validation framework, runtime abstraction, consumer guard or dependency. Deleting the components would delete required task behavior; existing neighboring entries document different components and cannot supply these contracts. Adding three declarative entries is the smallest complete correction and makes the candidate self-contained without changing adjacent runtime, tenant, lifecycle or visual behavior.

**Correction burden:** three data entries in one existing owner, plus the bounded task evidence command correction above. No product/API/security/persistent-data decision, new runtime code or broad test suite is needed.

**Focused closure proof:** at the new immutable candidate, inspect/parse the three entries, compare their built status, props and catalog eligibility against exports/components and catalog.json. Existing component tests already exercise implementation behavior. Run the narrow existing brand tests (`mise exec -- node --test brand/gen-theme.test.mjs` from the UI root) and existing catalog drift check (`mise exec -- node scripts/catalog.ts --check` from packages/ui); use a one-off read-only assertion for the three entries/parity rather than adding a new permanent check solely for this review. Record exact selector invocations for already named tests with the existing package runner and live setup wrapper; do not require full shipped-surface journeys for this contract-only remediation.

## Acceptance and verification reconciliation

Uncontested implementation rows in behavior/security reports have source and test evidence, including exact version/owner reads, read-only graph interaction, same host adapters and superseded-load handling. The orchestrator independently ran both designated unit files (29 tests), built the package/MCP HTML and ran host/package typechecks with no errors/warnings; unchanged native SDK artifacts/dependencies were reused as stated in verification.md. Repository reviewer independently ran six existing brand tests, catalog drift and diff checks. These green checks do not negate FIND-TASK-001-1.

AC-009 human visual acceptance, physical touch behavior, final-theme screenshot comparison and full integration aggregate are not independently certified. Candidate task records focused real-server/Chromium passes and an ungreen broader command; no report converts that into full success. No visual-related source defect was independently established. The user's implementation-only audit can report this validated code/contract verdict without pretending full task completion is accepted.

No prior-finding closure or convergence assessment applies on this first round. Remediation can stay bounded to committed component contracts and the task's exact-command evidence record; it does not need to reopen graph algorithms, runtime flows or unrelated aggregate failures.

Not rerun by this validator: unit/integration/browser/dev workflows or visual comparison. Risk retained: full integration and final visual acceptance remain outstanding outside the source finding.
