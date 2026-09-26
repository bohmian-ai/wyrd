---
id: TASK-002-R5
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 4
requirements: [INV-004, AC-007]
depends_on: [TASK-002-R4]
parent_task: TASK-002
remediates: [FIND-TASK-002-13]
---

# Correct the refresh lookup ordering rustdoc

## Authority and immutable subject

- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-002-tenant-login.md`
- Review verdict:
  `changes/active/oidc-production-readiness/review/TASK-002-r5/verdict.md`
- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`
- Reviewed candidate: `b57d43d501c136591125b98fe78352e657b093b6`
- Validated finding: `FIND-TASK-002-13`

This remediation corrects one stale Rust source contract without changing
runtime behavior, the approved specification, public auth contracts, or the R4
refresh-family serialization model.

## Issue diagnosis

### FIND-TASK-002-13 — refresh lookup rustdoc describes retired ordering

The mandatory Rust documentation rule requires every materially modified item
to explain its current workflow role and invariants accurately. At
`crates/wyrd/wyrd-sql/src/queries/auth/refresh_tokens.rs:122-126`, the public
`refresh_by_hash` helper says the reuse-detection path calls it only after
`consume_active_refresh` returns `None`.

R4 deliberately retired that order to close the ancestor-replay/current-token
rotation race. The only production refresh caller at
`crates/wyrd/wyrd-auth/src/refresh.rs:121-128` now calls `refresh_by_hash`
first, uses the returned immutable principal kind and id to acquire the
tenant-qualified refresh-family transaction lock, and only then classifies the
row by calling `consume_active_refresh`. The helper also remains legitimately
used by the existing refresh-revocation owner, so deleting or specializing it
would be incorrect.

The runtime implementation and deterministic Postgres overlap proof are
already correct. The defect is the stale source contract: a maintainer following
it could restore the exact consume-before-family-lock ordering that R4 removed
and reintroduce the containment race closed by `FIND-TASK-002-12`.

## Intended correction outcome

`refresh_by_hash` accurately documents that it returns a refresh row in any
lifecycle state. Its workflow description states that refresh rotation uses the
returned immutable principal family to take the existing family lock before
active/stale classification, while revocation may use the same helper for row
lookup.

## Decision-complete recommendation

Keep `refresh_by_hash`, all callers, SQL, locks, transaction boundaries, and
tests unchanged. Replace only the stale use/order paragraph in the helper's
existing rustdoc with the current two-caller contract:

- refresh rotation looks up the row first, derives its immutable principal
  kind/id, acquires the existing family lock, and then classifies it;
- refresh revocation may use the helper to locate a row regardless of lifecycle
  state.

This is the minimum safe correction because the shared helper has two real
callers, the existing family-lock owner already implements the approved order,
and no executable defect remains. Do not add a helper, wrapper, trait, lock,
configuration, dependency, or runtime test.

## Constraints and preserved behavior

- Preserve all closures from `FIND-TASK-002-1` through
  `FIND-TASK-002-12`, especially the family-before-classification and
  family-before-connection ordering from R4.
- Preserve `TenantConn`, forced RLS, caller-owned commit/rollback, connection
  provenance, replay-family revocation, and canonical transactional audit.
- Preserve every public HTTP/OpenAPI/schema contract, SQL statement, migration,
  error, generated artifact, and test assertion.
- Do not change executable Rust, imports, dependencies, features, ownership,
  persistence, concurrency semantics, or test infrastructure.
- Do not implement TASK-003 BFF completion or TASK-004 CLI handoff persistence.

## Non-goals

- No runtime refresh fix, new concurrency mechanism, refactor, helper split, or
  caller change.
- No new test: the defect and correction are documentation-only, while the
  existing deterministic Postgres test already proves runtime ordering.
- No unrelated rustdoc cleanup or generated-documentation change.

## Acceptance criteria

| Criterion | Finding closure |
|---|---|
| `refresh_by_hash` no longer claims it follows a failed `consume_active_refresh`; it accurately describes lookup-before-family-lock/classification for rotation and its independent revocation use. | `FIND-TASK-002-13` |
| The complete `RefreshTokens::execute` body and both production `refresh_by_hash` callers agree with the corrected rustdoc. | `FIND-TASK-002-13` |
| The remediation changes only the cited rustdoc and leaves executable behavior, contracts, SQL, tests, dependencies, features, and generated artifacts unchanged. | `FIND-TASK-002-13` |

## Focused proof and broader verification

Directly inspect the corrected rustdoc beside the full
`RefreshTokens::execute` body and both production helper callers. Then run:

```bash
mise run fmt
mise run lints
git diff --check
```

No behavioral test is warranted for this rustdoc-only correction. If the diff
changes executable behavior, a public/generated contract, SQL, a test, a
dependency, or a feature, remove that drift rather than broadening this task.
