---
id: TASK-005-R2
kind: remediation
status: ready
spec: SPEC-verified-change-contract
spec_revision: 36
requirements: [REQ-073, REQ-080, REQ-146, AC-013]
depends_on: [TASK-005]
parent_task: TASK-005
remediates: [FIND-TASK-005-2, FIND-TASK-005-9, FIND-TASK-005-10]
---

# Close TASK-005 round 2 findings

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 36.
- Original task: `changes/active/verified-change-contract/tasks/TASK-005-production-drift-verifier.md`.
- Prior review: `changes/active/verified-change-contract/review/TASK-005-r1`.
- This review: `changes/active/verified-change-contract/review/TASK-005-r2/findings-validation.md` and `verdict.md`.
- Cumulative base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`; reviewed candidate: `81d2221b4d3b1a9fb6bd36c591d421385cc8c67d`.

## Diagnoses and required outcomes

### FIND-TASK-005-2 — binding journey evidence

`AC-013` requires a real Service binding journey with two Services sharing one Trigger, separate runs per binding, and manual binding invocation through its Operator path. The existing Rust Drift journey at `sdks/wyrd-sdk-rust/tests/drift_verification.rs:463-547,974-1028` covers one Service and scheduled dispatch; `pg_verification_routes.rs:318-360` proves only manual enqueue. The three Drift SDK journeys do not prove the two-Service or manual-to-dispatch path. A projection or routing defect could therefore mix, omit, or duplicate results while current tests pass.

Extend the existing Rust SDK/server Drift journey with two active Services referencing the same Trigger and eligible at one occurrence. Prove separate binding and result identities and each binding's configured dispatches. Invoke one binding manually and prove its failed result dispatches; retain the existing direct-run no-dispatch assertion. Reuse the existing Trigger, Service, Operator, generic scheduler, and SDK journey harness. No new scheduler, public API, or test harness is needed. This closes the production path at the client→server→client tier required by `AC-013`.

### FIND-TASK-005-9 — fit work outlives cancellation

`REQ-146` requires shutdown drain then cancellation of remaining work, with its fenced lease released or expired. In `verification/fitter.rs:215-232,258-294`, timeout/shutdown cancels a token and awaits blocking fit before lease settlement. The token is checked during decode and once before `vala_drift::fit_baseline`; PSI/SPC work in `vala-drift/src/baseline/mod.rs`, `psi/mod.rs`, and `spc/mod.rs` can continue without observing it. The existing cancellation test stops before fitting. An expensive fit may exceed the execution timeout or shutdown drain; its lease may expire and be reclaimed while the first process still computes.

Make the existing `vala-drift` fit owner observe the fitter's cancellation signal during potentially long PSI/SPC work, including within a large feature. Preserve the ordinary fit API's output and profile semantics for uncancelled callers, decoded-data budget, shared permit, and SQL attempt fence. The server must await blocking work's cooperative stop before releasing or failing the lease. Do not detach work, add another worker/timeout framework, or move resource ownership.

### FIND-TASK-005-10 — stale ready task authority

The original task's frontmatter still says `status: ready`, `spec_revision: 35`; its outcome, owner/approach, and scenarios at `TASK-005-production-drift-verifier.md:1-43,85,118,158` direct local typed Oracle plans. Approved revision 36 requires a scoped SYSTEM token and fixed SQL through query service, Gate, and local or peer Oracle. The task's later evidence acknowledges that change, but an implementer following the active task would recreate the displaced boundary.

Align that same task's revision and governing prose with revision 36. Keep its original obligations and historical r1 evidence. The existing task is the owner; a replacement packet or implementation change adds nothing to this correction.

## Constraints and non-goals

- Preserve approved revision 36 token scope, audited Gate/query path, tenant isolation, exact subject/series/window SQL, and ordinary result publication.
- Preserve existing scorer results and the shared runtime's claim, permit, dispatch, and fenced settlement behavior.
- Do not add client aggregation, raw scorer reads, user SQL, a Drift scheduler, compatibility surfaces, or a new fit worker.
- Do not modify unrelated pre-existing `Oracle::query_plan` solely because it has no caller.

## Acceptance and proof

| Finding | Acceptance criterion | Focused proof |
|---|---|---|
| `FIND-TASK-005-2` | Two Services sharing one Trigger produce separately attributable Drift results and configured dispatches; a manual binding run dispatches, a direct run does not. | Extend and run the exact Rust `drift_verification` SDK journey through repository-managed Postgres; assert binding IDs, result IDs, and dispatch sets. |
| `FIND-TASK-005-9` | Cancellation after fit work starts stops its blocking computation before fenced release/timeout settlement; uncancelled profiles remain unchanged. | Add the smallest controlled focused fit cancellation check and run its exact `mise exec --` command, then the owning broader lane. |
| `FIND-TASK-005-10` | No active ready instruction names revision 35 or the displaced local typed-plan path. | Read the final task frontmatter, outcome, owner, approach, and scenarios against approved revision 36. No runtime test is needed. |

Use `mise.toml` to select the narrowest owning journey/fit tasks. Record the exact focused command for every named test, then run required format/lints and the touched-surface broader lanes under `mise`. Preserve the cumulative candidate and submit it for a fresh `$wyrd-task-review` against the same base.
