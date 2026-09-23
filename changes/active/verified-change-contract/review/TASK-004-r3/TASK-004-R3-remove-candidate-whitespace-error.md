---
id: TASK-004-R3
kind: remediation
status: proposed
spec: SPEC-verified-change-contract
spec_revision: 33
parent_task: TASK-004
remediates: [FIND-TASK-004-9]
---

# Restore the candidate-wide clean-diff proof

## Authority and subject

- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 33
- Original task: `changes/active/verified-change-contract/tasks/TASK-004-generic-verification-runtime-and-results.md`
- Reviewed base: `9431906eeb1c7b67a0efcec09487fd1d848f70a8`
- Reviewed candidate: `29721b7e33854633b025b25948fd5d2eaebe7bfd`
- Validated ledger: `changes/active/verified-change-contract/review/TASK-004-r3/findings-validation.md`

## Outcome

Make the cumulative TASK-004 candidate pass its required clean-diff check
without changing implementation behavior or any prior review conclusion.

## Issue diagnosis and required correction

### `FIND-TASK-004-9` — candidate-wide clean-diff verification fails

TASK-004 explicitly requires `git diff --check`, and the repository completion
standard requires every applicable check to pass without waiving a failure as
pre-existing. The exact cumulative command currently exits 2:

```text
git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8 29721b7e33854633b025b25948fd5d2eaebe7bfd
changes/active/verified-change-contract/review/TASK-004-r2/findings-validation.md:160: new blank line at EOF.
```

The affected R2 validation artifact ends with one superfluous blank line after
its final verification-limit item. Although this changes no executable
behavior, it is part of the reviewed range and prevents the mandatory proof
from passing.

Delete only that final blank line. Preserve every word and conclusion in the
R2 review record. This is the complete correction: no production code, test,
API, task requirement, check, or other formatting change is needed.

## Preserved behavior and non-goals

- Preserve every TASK-004 runtime, persistence, security, publication, SDK,
  MCP, schema, migration, deployment, and documentation behavior.
- Preserve prior findings `FIND-TASK-004-1` through `FIND-TASK-004-8` as
  closed and preserve the R2 ledger's substantive content.
- Preserve the owner-directed deletion of the uncalled
  `VerifierRunQueue::new` and `ResultPublisher::endpoint` methods.
- Do not edit production code, generated artifacts, tests, checks, manifests,
  or any other review artifact.
- Do not weaken, omit, or scope down `git diff --check`.

## Acceptance criterion

| Finding | Acceptance criterion |
|---|---|
| `FIND-TASK-004-9` | The R2 validation artifact contains no extra blank line at EOF, every substantive byte remains unchanged, and exact base-to-remediated-candidate `git diff --check` exits zero with no output. |

## Focused and broader proof

Run the exact proof against the new remediation commit:

```bash
git diff --check 9431906eeb1c7b67a0efcec09487fd1d848f70a8 <remediated-candidate>
```

Inspect the remediation diff and confirm it removes only the final blank line
from `changes/active/verified-change-contract/review/TASK-004-r2/findings-validation.md`.
No runtime, compile, lint, codegen, or journey rerun is warranted for this
Markdown-only whitespace correction because the candidate behavior is
unchanged and the focused check directly exercises the complete gap.

Route this task directly to `$wyrd-implement`. The next review must inspect the
complete original base-to-remediated-candidate range.
