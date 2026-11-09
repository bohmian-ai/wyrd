---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-card-authoring
spec_revision: 3
parent_task: TASK-001
remediates: [FIND-TASK-001-3]
---

# Finish the existing SDK helper panic documentation correction

Route directly to `$wyrd-implement`. Only the remaining mandatory documentation portion of FIND-TASK-001-3 is open; findings 1 and 2 are closed.

Approved spec: `changes/active/card-authoring/spec.md` revision 3.
Original task: `changes/active/card-authoring/tasks/TASK-001-common-rust-authoring.md`.
Original base: `5234961d51798581a026834cfffd5efaadc226b4`.
Prior candidate: `790da3369c1b0107765db234d72c1ae0c4680446`.
Reviewed cumulative candidate / starting point: `c0a06eacdc70249e9c32e44b7670314cf1aefb3a`.
Prior remediation: `../TASK-001-review-01/TASK-001-R1-current-dependencies-and-proof.md`.
Diagnosis and independent closure evidence: [findings-validation.md](findings-validation.md); verdict: [verdict.md](verdict.md).

## Diagnosis and selected correction

The first remediation added the missing docs and panic contracts at most cited locations, but five SDK test wrappers still omit their actual panic behavior. Root AGENTS and `agent_docs/rust.md` explicitly require applicable `# Panics` sections for new private/test helpers, so the existing mandatory gate remains open despite passing cargo doc and Clippy. No production behavior failure is asserted.

All remaining locations are in `sdks/wyrd-sdk-rust/tests/integration/authoring.rs` at the reviewed candidate:

| Helper | Existing behavior that its documentation must cover |
|---|---|
| `release_access`, line 322 | Forwards caller version to `access_verifier`, which expects valid Verifier construction and panics on refusal, including an invalid exact version. |
| `summarized_access`, line 328 | Same version/construction panic through `access_verifier`. |
| `baseline`, line 451 | Constructs the fixture graph through `finance_release` and `fact_step`; missing/unreadable checked-in code source causes the existing `CodeCall::from_file(...).expect(...)` to panic. |
| `outcome`, line 530 | Calls the existing `verifier_building_blocks::requirement`; absence of the requested requirement ID panics, as its callee already documents. |
| `policy_published`, line 794 | Constructs a CardRef through `card_ref(version)` before testing the server result; an invalid exact version panics during the expected decode, rather than returning false. |

Finish the already requested documentation correction by adding short, accurate `# Panics` contracts to these five existing wrappers, linking to the existing documented callee where useful. Keep current purpose documentation. The producer/callee bodies already express the behavior; the missing contract is at the new wrapper that callers read. Deleting wrappers would require unnecessary caller changes, and replacing panics or adding guards would change test behavior without closing a requested runtime defect. The minimum correction is prose at these locations.

## Constraints and acceptance

- Preserve every signature, runtime body, assertion, fixture, error and dependency capture behavior.
- No new helper, checker, test, dependency, public API or broader style pass.
- FIND-TASK-001-1 and FIND-TASK-001-2 remain closed; do not reopen or rewrite their accepted implementation/proof.
- Acceptance: each of the five wrappers has an applicable panic section matching its existing explicit callee/input/fixture path. Independent source inspection confirms that no runtime change entered the diff.

## Focused proof

This is documentation-only closure of an existing mandatory gate. No RED/GREEN runtime test is required and no new test is justified. Inspect the five docs against their existing callees and run:

```bash
mise exec -- cargo fmt --all -- --check
mise exec -- git diff --check
```

Record the new immutable candidate and the source-inspection result. Carry the seven passing focused tests, warning-free cargo doc and scoped Clippy from review-02/verification.md and prior accepted runtime/domain evidence forward. Do not repeat journeys, engine tests, broad gates or comprehensive discovery for these prose changes. The next review checks this residual FIND-TASK-001-3 closure and the actual new diff only.

## Evidence

Candidate: `17c4e76d0b8209381159f514799e3d71fbfe9404` (on `c0a06eacd`). Diff: `sdks/wyrd-sdk-rust/tests/integration/authoring.rs`, 18 insertions, all `///` lines; `git diff c0a06eacd..17c4e76d0 | grep '^[+-][^+-]' | grep -v '^+///'` is empty, so no signature, body, assertion or fixture changed.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| `release_access` panic contract | `# Panics`: as `access_verifier`, including a non-exact `version` | source inspection against `access_verifier` (`Verifier::new(..).expect`) | PASS |
| `summarized_access` panic contract | same contract, same callee | source inspection | PASS |
| `baseline` panic contract | `# Panics`: unreadable story checks, as `fact_step` (`CodeCall::from_file(..).expect`) | source inspection | PASS |
| `outcome` panic contract | `# Panics`: missing requirement `id`, as `verifier_building_blocks::requirement` documents | source inspection | PASS |
| `policy_published` panic contract | `# Panics`: non-exact `version` via `card_ref`, before the server call | source inspection | PASS |
| No runtime change | doc-only diff | diff filter above | PASS |
| Format / whitespace | — | `mise exec -- cargo fmt --all -- --check`; `mise exec -- git diff --check` both clean | PASS |

Carried forward per task: the seven focused tests, warning-free cargo doc, and scoped Clippy from review-02 `verification.md`. Non-goals held: no new helper, test, dependency or API; FIND-TASK-001-1/2 untouched.
