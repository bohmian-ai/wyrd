---
id: TASK-001-R2
title: Finish cross-path authorization ordering and test documentation
kind: remediation
status: ready
spec: SPEC-object-scoped-rbac
spec_revision: 3
original_task: changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md
base: 1c4b518dbb69dd10b9107c06e8ccb410c67c5449
reviewed_candidate: 1e5ed56664ed87ba6ac7a30587138abdf137904c
prior_candidate: 3de6aed2f86b4bb039e85c7f62c0a48e12602282
findings: [FIND-TASK-001-1, FIND-TASK-001-6]
---

# Finish TASK-001 remediation

Required execution skill: `$wyrd-implement`.

## Authorities and immutable inputs

- Approved specification: `changes/active/object-scoped-rbac/spec.md`, revision 3
- Original task: `changes/active/object-scoped-rbac/tasks/TASK-001-object-scoped-bifrost-rbac.md`
- Prior verdict: `changes/active/object-scoped-rbac/review/task-001-codex/verdict.md`
- Prior remediation: `changes/active/object-scoped-rbac/review/task-001-codex/TASK-001-R1-close-rbac-contract-gaps.md`
- Cumulative re-review: `changes/active/object-scoped-rbac/review/task-001-r1-codex/verdict.md`
- Original base: `1c4b518dbb69dd10b9107c06e8ccb410c67c5449`
- Reviewed cumulative candidate: `1e5ed56664ed87ba6ac7a30587138abdf137904c`

Preserve every accepted part of the cumulative candidate and correct only the
two remaining findings.

## Diagnosis and correction

### FIND-TASK-001-1 — typed-plan authorization must precede admission

The R1 change correctly moved the shared resolved-object decision into
`OraclePlanner::attempt_protected_cut`, before reader authority, revalidation,
materialization, and source IO. Raw-SQL paths prepare that cut before admission
and now satisfy REQ-002.

`Oracle::query_plan` has the opposite outer ordering. It acquires an
`AdmittedQueryGuard` in `crates/vala/vala-bifrost-redux/src/oracle/mod.rs` before
calling `execute_typed_plan`; `execute_typed_plan` then calls
`prepare_typed_cuts`, which is where the object decision occurs. An uncovered
typed plan can therefore enter or wait in global/tenant admission and retain
admitted capacity before it is refused. The current reader-authority test calls
the planner directly and cannot detect this ordering.

Use the existing typed-cut preparation and execution owners. Prepare and
authorize the complete typed cut from the verified context before requesting
Oracle admission, then pass that same protected cut into execution. Do not
prepare, authorize, pin, or materialize it a second time. This matches the
existing raw-SQL ordering and closes the shared REQ-002 boundary without a new
checker, authorization path, or abstraction.

Add focused proof through the typed-plan entry that an uncovered table returns
the stable query-forbidden error before any admission observation changes, and
that the same plan with exact authority still admits and proceeds. The proof
must exercise `Oracle::query_plan`, not only `OraclePlanner`.

### FIND-TASK-001-6 — attach the malformed-identity test's rustdoc correctly

In `crates/shared/wyrd-runtime/src/permission.rs`, the sentence describing a
structurally valid but empty object identity is currently attached to
`bifrost_scope_is_rejected_on_a_non_read_action`. The following cumulative new
test, `malformed_scope_identity_is_rejected_at_decode`, has no rustdoc.

Put the malformed-identity intent/invariant comment immediately above that
test, leaving the non-read-action test with only its action-specific comment.
No other documentation change is needed.

## Constraints, preserved behavior, and non-goals

- Preserve all original TASK-001 invariants, accepted permission shapes,
  role-resolution behavior, payload separation, distributed digest binding,
  stable errors, durable public denial audit, and role journey behavior.
- Preserve one `PermissionSet`, one synchronous `PermissionCheck`, the existing
  prepared Bifrost identity, and the existing Oracle admission owner.
- The typed plan must be prepared exactly once and authorized before admission;
  do not move authorization back after materialization or duplicate it around
  admission.
- Add no store, lookup, cache, checker, trait, factory, dependency, public API,
  compatibility path, error, builtin role, or alternate query route.
- Do not refactor unrelated Oracle planning/admission behavior or document
  unchanged tests.
- Do not run `mise run gate`, `mise run test:bifrost`, or another complete
  platform/family suite.

## Acceptance criteria

- **FIND-TASK-001-1:** Every `Oracle::query_plan` request authorizes the complete
  catalog-resolved typed scan set before entering or acquiring Oracle admission.
- **FIND-TASK-001-1:** An uncovered typed plan returns
  `WYRD_VALA_403_QUERY_FORBIDDEN` with no admission observation, reader guard,
  materialized cut, accepted-read audit, peer dispatch, or source row; the same
  plan with exact table authority proceeds through the ordinary admitted path.
- **FIND-TASK-001-1:** Typed execution consumes the already-authorized protected
  cut without repeating catalog preparation, object authorization, or
  materialization.
- **FIND-TASK-001-6:**
  `permission::tests::malformed_scope_identity_is_rejected_at_decode` has concise
  intent/invariant rustdoc immediately above it, and the neighboring action test
  retains only its own correct documentation.
- Every original TASK-001 criterion and every R1-closed finding remains passing.

## Focused and broader verification

Run Cargo-backed commands sequentially through `mise`.

1. Run one exact nextest expression for the new typed-plan ordering test. It
   must exercise `Oracle::query_plan` and directly observe that denial precedes
   admission.
2. Re-run the exact reader-authority materialization-order test and exact
   real-server scoped-role journey.
3. Re-run the exact scoped permission validation, payload/digest, peer-authority,
   resolver, and revocation/cache tests recorded by the R1 report.
4. Inspect the cumulative diff to confirm every new test has intent/invariant
   rustdoc.
5. Run `mise run fmt`, `mise run lints`, `mise run codegen:check`,
   `mise run check:client-tier`, `mise run check:pyo3-scope`,
   `mise run docs:check`, and `git diff --check`.

Record every specifically named test with its exact fully qualified
`mise exec -- cargo nextest run --locked` selector and run count. Preserve the
known base-identical `check:design-sync` and `check:unwrap-audit` failures as
out-of-scope evidence unless this remediation changes their causes.
