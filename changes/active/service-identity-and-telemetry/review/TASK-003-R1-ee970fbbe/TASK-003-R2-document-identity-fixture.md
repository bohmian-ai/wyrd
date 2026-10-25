---
id: TASK-003-R2
kind: remediation
status: implemented
spec: SPEC-service-identity-and-telemetry
spec_revision: 16
parent_task: TASK-003
remediates: [FIND-TASK-003-6]
reviewed_candidate: ee970fbbef1511607a71c89ac16802b34724d422
---

# Document the changed identity-test fixture

Route directly to `$wyrd-implement`. The five behavior findings are closed; this task completes the remaining explicit changed-item documentation gate. It requires a few rustdoc lines on one existing helper and no executable change.

## Authority and acceptance context

- Approved specification: `changes/active/service-identity-and-telemetry/spec.md`, revision 16, with its normative supplements.
- Original task: `changes/active/service-identity-and-telemetry/tasks/TASK-003-system-schemas-and-attribution.md`.
- Original base: `ca2ac4a37e229a8f95667c073db11c70ad7ecb8b`.
- Original reviewed candidate: `d1d02cced07582a8f817d3f5651e7895b96d046d`.
- First remediation: `changes/active/service-identity-and-telemetry/review/TASK-003-d1d02cced/TASK-003-R1-preserve-accepted-origin.md`.
- Current candidate: `ee970fbbef1511607a71c89ac16802b34724d422`; its six corrective commits follow record-only `89449aa39be209b0fe34e2cfbdef87110d6f5ce8`.
- Current validated closure: `findings-validation.md` and `verdict.md` beside this task. Preserve all prior evidence and stable IDs; no new finding is introduced.

Root AGENTS says every new or changed Rust item requires rustdoc and that missing rustdoc blocks merge. `agent_docs/rust.md` explicitly includes private/test helpers regardless of visibility and requires a Panics section where an item can panic. This is a mandatory gate, not a request to change naming, blank-line style or test structure.

## FIND-TASK-003-6 diagnosis

At candidate line 1333 in `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs`, the nested helper has this signature:

```rust
fn rows(value: i64, workload: &str, request: &str, receipt: i64) -> RecordBatch
```

It is inside the changed logical-identity unit test. R1 materially changed its signature and constructed schema/array by adding the stable workload input. It constructs each one-row fixture consumed by that test's identity comparisons and ends in `RecordBatch::try_new(...).expect("identity batch")`. It has no item-local rustdoc or Panics section. Documentation on the containing test does not attach to this nested function.

The original documentation inventory is otherwise corrected, and the functional fixes and their focused proofs passed review. Green format/lint/test results cannot establish this all-visibility documentation requirement. The consequence is the remaining explicit merge-gate failure; no runtime defect or likely fixture panic is alleged.

## Selected correction and preserved behavior

Add brief rustdoc directly to this existing helper explaining its single-row identity fixture, caller-controlled stable payload/workload and volatile request/receipt inputs. Add an accurate Panics section for failure to construct the declared fixture RecordBatch. No Errors section is needed because it returns a batch rather than Result.

The existing local fixture already owns the behavior. A local documentation addition is smaller than deleting the helper and duplicating its uses or rewriting accepted proof. Preserve its signature, body, callers, assertions and test placement. Do not add a new helper, check, test, abstraction, dependency, suppression or runtime guard. Do not rename existing tests, revisit the prior corrected inventory, modify accepted ownership/replay behavior or reopen closed findings.

## Acceptance and focused proof

FIND-TASK-003-6 closes when the nested helper itself has meaningful purpose and accurate Panics rustdoc, and the correction diff contains no executable or assertion change and introduces no undocumented item. Source and diff inspection are the direct proof; there is no new behavior requiring RED/GREEN or another runtime test.

Run the existing non-mutating formatting task:

```bash
mise run fmt:check
git diff --check
```

Retain the already independently passing exact identity/Rows test evidence in this review's `verification.md`. No codegen, Postgres, journey suite or broad aggregate is required for documentation-only closure. Apply an existing lint check only if the actual correction introduces an applicable lint concern; do not add another check script or waive any gate.

The next review scope is this item-local documentation delta and closure of the existing FIND-TASK-003-6. Carry forward source-backed closure of FIND-TASK-003-1 through -5 and the accepted original task behavior unless the actual correction changes executable code or new material evidence invalidates that acceptance.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| The nested `rows` helper has purpose and accurate Panics rustdoc. The diff has no executable or assertion change and adds no undocumented item. | `crates/vala/vala-bifrost-redux/src/scribe/preprocess.rs`: the `rows` helper in `logical_identity_ignores_server_receipt_columns_only` documents its single-row fixture, its stable payload/workload and volatile request/receipt inputs, and a `# Panics` section for batch construction | Source inspection; every added diff line is `///`; `mise run fmt:check` passes; `git diff --check` is clean | PASS |

Commands:

```bash
mise run fmt:check
git diff --check
```

Non-goals: no change to signature, body, callers, assertions, test placement, test names or behavior. No new helper, check, test, dependency or suppression. FIND-TASK-003-1 to FIND-TASK-003-5 are untouched. No lint concern was introduced (docs only), so no lint run was needed.
