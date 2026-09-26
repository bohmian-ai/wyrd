---
id: TASK-002-R10
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [REQ-006, REQ-007, REQ-008, REQ-013, REQ-014, REQ-015, REQ-016, REQ-017, INV-001, INV-002, INV-003, INV-004, AC-002, AC-003, AC-005, AC-006, AC-007]
depends_on: [TASK-002-R9]
parent_task: TASK-002
remediates: [FIND-TASK-002-23, FIND-TASK-002-24]
---

# Correct the remaining tenant-login Rust documentation contracts

## Authority and immutable review subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Reviewed base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `6e21d8ed00d5159ec71e3f2e2414e80fd16f76af`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r10/verdict.md`
- Validated findings: `FIND-TASK-002-23`, `FIND-TASK-002-24`

Apply `AGENTS.md` section 16, `architecture/agent-rules.md`, and
`architecture/references/languages/rust-core.md`. Route this bounded task
directly to `$wyrd-implement`.

## Outcome

The two materially changed Rust items accurately document their narrowed
token-contract proof and their complete auth-route ownership. Executable
behavior, tests, public contracts, generated artifacts, and every prior
TASK-002 correction remain unchanged.

## Issue diagnosis and required correction

### FIND-TASK-002-23 — the narrowed token-contract test is undocumented

`crates/wyrd-spec/src/auth/token.rs:225-241` contains
`new_grant_variants_reject_unknown_fields`. TASK-002 retired the public
authorization-code grant, and the cumulative candidate materially changed this
test by deleting that input and assertion while retaining the
`jwt-bearer` and `refresh_token` unknown-field checks. The ordinary Rust test
harness still discovers the function, but it is preceded only by `#[test]`.

The hard repository rule explicitly includes materially modified test
functions. Passing assertions cannot prove item-level documentation, and the
neighboring discriminator and retirement tests neither describe nor replace
this surviving two-variant contract.

Add concise rustdoc immediately above the existing test attribute explaining
that the surviving `jwt-bearer` and `refresh_token` request variants reject
unknown fields after authorization-code retirement. Preserve the function
name, body, assertions, and adjacent serde behavior. Do not add another test,
helper, lint, or documentation harness.

### FIND-TASK-002-24 — the auth-routes module contract omits login initiation

The module rustdoc at
`crates/wyrd/wyrd-server/src/components/auth/routes.rs:1-2` says the module
owns token exchange, the OIDC callback, and Card-bound API-key issuance.
`auth_router` now also mounts `POST /auth/login`, and its own item rustdoc
correctly lists all four surfaces. Production `http::router::build_router`
merges this router, so the omitted surface is live and is the public login
boundary introduced by TASK-002.

Update only the module-level rustdoc to include tenant human login initiation
alongside the other three owned surfaces. Reuse the existing description for
those surfaces. Preserve imports, handlers, route paths, governor behavior,
router composition, and item-level documentation. Do not move the contract or
add a documentation checker.

## Constraints and preserved behavior

- Keep the correction documentation-only. No Rust expression, assertion,
  route, schema, generated artifact, dependency, feature, fixture, or task
  wiring changes.
- Preserve the complete closure of `FIND-TASK-002-1` through
  `FIND-TASK-002-22`, including exact OIDC subject identity, trust validation,
  canonical audit, RLS, secret redaction, refresh-family serialization,
  provider-role serialization, token schema wording, scrubbed tracing, router
  behavior, and database-owner pool boundaries.
- Preserve the original task's security, tenancy, transaction, callback,
  issuance, renewal, machine-path, and non-goal boundaries.
- Do not introduce a compatibility route, public API change, abstraction,
  helper, lint, or test harness.

## Acceptance criteria

| Acceptance criterion | Finding |
|---|---|
| `new_grant_variants_reject_unknown_fields` has substantive rustdoc stating that the surviving `jwt-bearer` and `refresh_token` variants reject unknown fields after authorization-code retirement, while its body is byte-for-byte unchanged. | `FIND-TASK-002-23` |
| The auth-routes module rustdoc names tenant human login initiation, token exchange, OIDC callback, and Card-bound API-key issuance, while the module's executable contents are unchanged. | `FIND-TASK-002-24` |
| The remediation diff changes only the two cited rustdoc blocks and introduces no generated, executable, contract, test-body, dependency, feature, fixture, or unrelated documentation change. | `FIND-TASK-002-23`, `FIND-TASK-002-24` |
| `FIND-TASK-002-1` through `FIND-TASK-002-22` remain closed. | Both |

## Focused proof and broader verification

No new runtime test is warranted: both gaps are static documentation
contracts, and the existing test and router bodies must not change.

1. Inspect the final two rustdoc blocks against their unchanged full item
   bodies and callers.
2. Confirm the remediation diff contains only those two comment blocks.
3. Run `git diff --check`.
4. Run `mise run fmt`.
5. Run `mise run lints`.

Record the exact diff inspection and command results as implementation
evidence. A later task review must reassess the complete
`3fc085acf5b3a710d5dc80892bd2e664b3db6174`-to-new-candidate range, not only
the R10 remediation diff.
