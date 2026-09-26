---
id: TASK-002-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
parent_task: TASK-002
remediates: [FIND-TASK-002-8, FIND-TASK-002-9]
---

# Tenant-login repository-rule corrections

## Authority and immutable subject

- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Prior remediation: `changes/active/oidc-production-readiness/review/TASK-002-r1/TASK-002-R1-tenant-login-corrections.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `8b201627c0a957dccf46649d00c8c205689bc5de`
- Validated findings: `FIND-TASK-002-8`, `FIND-TASK-002-9`

## Outcome

Make the already-correct tenant-login implementation satisfy the repository's mandatory Rust documentation and import-layout rules without changing behavior, contracts, schemas, tests, dependencies, or ownership.

## Issue diagnoses and required corrections

### FIND-TASK-002-8 — Required Rust-item documentation is incomplete

The repository requires substantive rustdoc for every new or materially modified Rust item and requires `# Errors` and `# Panics` sections where applicable. The candidate omits that contract at four exact seams:

- `crates/wyrd-spec/src/auth/oidc.rs:234-293`: the new `Sha256Hex` `Display`, `Serialize`, `Deserialize`, `JsonSchema`, and feature-gated `PartialSchema` method bodies are undocumented; the fallible serialization/deserialization projections do not document their real error conditions.
- `crates/wyrd/wyrd-sql/src/queries/auth/login_state.rs:29-64`: the insert, consume, complete, and redeem SQL constants do not explain their state-transition roles and forced-RLS/one-use invariants.
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3081-3085`: `sign_id_token` can panic on invalid PEM or encoding failure but lacks `# Panics`.
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3362`: the local `Mutation` alias used by the callback-refusal table is undocumented.

These items are reachable through Serde/schema generation, production login/callback/redemption, and the required refusal journey. Green runtime checks do not close a hard source-documentation rule.

Document only those cited items in their existing owners. State the canonical lowercase-hex projection and actual trait error conditions for `Sha256Hex`; the transition, RLS, expiry, and one-use intent appropriate to each SQL constant; the exact panic causes for `sign_id_token`; and the alias's role as the token-response mutation in the refusal cases. This is the minimum correction because the implementation and tests are already behaviorally correct. Do not extract helpers, alter SQL, or broaden documentation work to unrelated source.

### FIND-TASK-002-9 — New imports are hidden inside non-generic functions

The mandatory module-top import rule is violated at:

- `crates/wyrd-spec/src/auth/oidc.rs:216`: `sha2::Digest as _` inside non-generic `Sha256Hex::digest`;
- `crates/wyrd-spec/src/auth/oidc.rs:281`: Utoipa schema builders inside `PartialSchema::schema`; and
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs:3052-3053`: Wiremock matchers and response types inside `mount_mock_provider_advertising`.

None qualifies for the test-submodule or generic-function exception. Move the existing imports to each module's top import block. Keep Utoipa imports feature-gated as needed and alias its `Schema` at import time to avoid collision with Schemars. Preserve all bodies and callers exactly. This reuses existing dependencies and requires no wrapper, helper, or new abstraction.

## Constraints and preserved behavior

- Preserve every closed behavior and finding from TASK-002 and TASK-002-R1, including exact `sub`, `azp`, advertised algorithms, transactional role-sync audit, forced RLS, owner-bound state lookup, and PKCE redaction.
- Preserve the complete public HTTP/OpenAPI/schema contract, SQL text and migration behavior, fixed callback responses, audit operations, test cases, and feature gates.
- Do not add dependencies, lints, documentation checkers, helpers, compatibility paths, public APIs, persistence resources, or runtime tests.
- Do not implement TASK-003 BFF or TASK-004 CLI handoff behavior.
- Do not edit generated artifacts unless `codegen:check` proves an unexpected source-derived change, in which case stop and diagnose rather than accepting drift.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| Every exact item named under FIND-8 has substantive rustdoc describing its actual contract and invariants; fallible projections have accurate `# Errors`, and `sign_id_token` has accurate `# Panics`. No unrelated documentation churn is introduced. | `FIND-TASK-002-8` |
| The three cited function bodies contain no local `use`; their existing names are imported in the owning module import blocks with correct feature gating/aliasing, and behavior and schema output remain unchanged. | `FIND-TASK-002-9` |
| The cumulative diff remains free of behavioral, contract, generated-artifact, test, dependency, and ownership changes beyond these source-rule corrections. | Both |

## Focused proof and broader verification

Static inspection of the cited items is the direct closure proof. No new runtime test is warranted. Run:

- `mise run fmt`
- `mise run lints`
- `mise run codegen:check`
- `git diff --check`

Because the correction must remain documentation/import-only, rerun a broader identity or Postgres lane only if the implementation diff changes executable behavior or generated contracts. Any such change is outside this remediation and must be removed rather than justified by new proof.
