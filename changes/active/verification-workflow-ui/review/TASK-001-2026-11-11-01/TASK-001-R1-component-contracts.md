---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-verification-workflow-ui
spec_revision: 2
parent_task: TASK-001
remediates: [FIND-TASK-001-1]
---

# Include the delivered Verifier component contracts

Route directly to `$wyrd-implement`.

Approved spec: `changes/active/verification-workflow-ui/spec.md`, revision 2 (approved workspace authority SHA-256 `ab12f59e6c2582e711ca4735f1933a1c15f54f3ab33c3c97f7bf0d12e7708e84`). Original task: `changes/active/verification-workflow-ui/tasks/TASK-001-verifier-definition.md`. Original base `7b36f8fc6`; reviewed candidate `dd98e9ef78d20e7de47e7af623644fed0ff9075a`. Review context: this directory's `verdict.md`, `findings-validation.md` and selected discovery reports. No source implementation changes were made by review.

## Diagnosis and intended outcome

FIND-TASK-001-1 (VIOLATION) identifies a missing required deliverable, not a graph runtime defect. Candidate exports VerifierDefinition, RunsOn and VerifierGraph at `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/index.ts:17–19`; schemas at `packages/ui/src/lib/schemas.ts:521–535` additionally expose the first two through the generated catalog. Their implementations and portable loaders work, but none is in candidate `brand/components.json` under its components owner. `brand/DESIGN.md:24–26,259–262` explicitly requires matching built component contracts. Agents and maintainers consulting that required machine-readable inventory cannot obtain the shipped components' contracts. Existing palette tests and catalog generation do not establish brand-contract parity.

The original workspace already holds three proposed entries inside an unrelated pending rewrite. Those bytes were excluded from the committed review subject. The corrected candidate must include the three accurate contracts in the existing owner, so a fresh checkout contains the complete delivery without borrowing workspace state.

## Selected correction and limits

Extend only the existing `brand/components.json` component inventory with built contracts for VerifierDefinition, RunsOn and VerifierGraph, matching actual props, anatomy, states and token/usage constraints. Reuse the existing entry format. Catalog eligibility is true for VerifierDefinition and RunsOn, false for the directly exported VerifierGraph primitive. Read their current complete component bodies and corresponding schemas/exports before finalizing contract data. Reuse accurate pending entries where appropriate while preserving all unrelated user edits; do not revert or include an unrelated brand rewrite as task remediation. Record the exact resulting delivery diff so review can distinguish the bounded correction.

This is three declarative entries, not a new owner or validation system. Deleting components would remove required task behavior; neighboring contracts describe different components. No generator, framework, consumer guards, backend/SDK changes, dependencies or runtime correction is justified.

Batch the validated small documentation correction: the original task Evidence names tests but does not give each exact runnable selector command required by `TESTING.md:91–111`. Record runner, owning file and `-t` selector for those existing named tests; live selectors must retain their existing managed-Postgres/environment wrapper and prerequisite instructions. Do not invent results or retrospectively claim RED execution. Adding command records alone does not require rerunning live journeys.

Preserve graph layout/interaction, exact version behavior, catalog/loader/executor routing, credential and tenant boundaries, and all existing assertions. No visual redesign, human visual acceptance, aggregate integration repair, new permanent parity gate or broad test suite belongs to this remediation. This task does not certify the outstanding visual/integration evidence.

## Acceptance

- FIND-TASK-001-1: a fresh immutable candidate contains all three built contracts in the existing inventory, with accurate implementation props and catalog flags matching generated placement.
- No unrelated source or brand rewrite enters the remediation diff; pending user edits remain preserved.
- Original task records exact focused invocations for its existing named tests, including live setup prerequisites, without changing past verification claims.

## Focused proof

This is a declarative contract/documentation correction; static parity evidence replaces runtime TDD. At the new immutable candidate, parse the existing inventory and compare the three entries' names, built status, props and catalog eligibility with actual component implementations, exports and generated catalog. A one-off read-only assertion is sufficient; add no permanent check solely for this review.

From the repository root, run:

```bash
mise exec -- node --test crates/wyrd/wyrd-server/wyrd-ui/brand/gen-theme.test.mjs
```

From `crates/wyrd/wyrd-server/wyrd-ui/packages/ui`, run:

```bash
mise exec -- node scripts/catalog.ts --check
```

Also check the bounded remediation diff for whitespace and preserved unrelated edits. These are the narrow existing lanes for this data/documentation write set. Do not require full unit, real-server, browser or shipped-surface journey suites for unchanged runtime behavior; integrated change review owns broader journeys.
