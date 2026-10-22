---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-guided-setup
spec_revision: 6
requirements: [REQ-038, AC-020]
depends_on: [TASK-001-R1]
parent_task: TASK-001
remediates: [FIND-TASK-001-6]
---

# Document the saved terminal field

## Authority and reviewed subject

Use [approved spec revision 6](../../spec.md), [original TASK-001](../../tasks/TASK-001-guided-setup-and-asgi.md), [R1 task/evidence](../TASK-001-2026-10-21/TASK-001-R1-close-guided-setup-gaps.md), the [current validation ledger](findings-validation.md), and [current verdict](verdict.md).

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`.
Original base: `ebd75dc224854413d17cb06b70da8181b78cde6b`.
R1 implementation start: `00b6148919b9ccd5f958f3b621ab6a89c6372001`.
Reviewed candidate: `796377a1d02f5a0a80fd905cc18fbbbb2d49bb4a`.
Prior findings FIND-TASK-001-1 through -5 are closed; the approved guided-mode addition passes its task-stage acceptance. This task addresses one new documentation omission, not a reopened runtime finding.

## Diagnosis and correction

**FIND-TASK-001-6:** `crates/wyrd/wyrd-cli/src/terminal.rs:107` introduces `SavedTerminal(Termios)` with meaningful type documentation but no rustdoc on its sole tuple field. `agent_docs/rust.md` explicitly requires documentation on new/materially changed fields regardless of visibility; root AGENTS and the task-review skill preserve missing rustdoc as a blocking exception. Documenting the type and its methods does not document the field itself.

The existing `SavedTerminal::save` captures standard-input attributes before the owned coding agent starts; `restore` uses that retained snapshot after agent exit/settlement. `Agent` and `DevServer::{guide,stop}` already own that lifetime correctly. The consequence is failure of the explicit documentation gate, with no alleged runtime defect. Existing clippy/GREEN journeys cannot establish presence of this field's rustdoc.

Add accurate rustdoc to the existing tuple field describing the captured standard-input attributes and restoration role. Reuse the existing type, representation and lifecycle. Do not rename it, convert it to a named-field struct, change behavior, add a helper/check, expand process ownership, or undertake a documentation sweep. Type-level wording and the other closed corrections need no change.

## Acceptance and proof

Static source inspection must show meaningful rustdoc attached to that exact field, while save/restore and all consumers remain behaviorally unchanged. This directly closes FIND-TASK-001-6. No new executable behavior is introduced, so TDD and a new test are inapplicable.

Run only the edited-source checks:

```bash
mise run fmt:check
mise exec -- git diff --check
```

Record the source inspection and actual check results. No Cargo tests, Postgres journeys, paid agents, platform matrix, broad docs/SDK aggregate or gate rerun is required for this field-documentation correction. Full cumulative acceptance remains the later review subject; combined change-review gates keep their existing scope. Route directly to `$wyrd-implement`; this task does not authorize a commit or source change during review.

A fresh independent comprehension reader received only this task and spec
revision 6 and found no blocking missing explanation or contradiction. It did
not inspect source or claim implementation verification.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-6: meaningful rustdoc on the `SavedTerminal` tuple field | `crates/wyrd/wyrd-cli/src/terminal.rs:107-111` documents the field: standard input's attributes read by `save` before the child started, written back by `restore` | Source inspection: the doc comment is attached to the field itself. `mise run fmt:check` and `git diff --check` both pass | PASS |
| Behavior unchanged | The diff touches only the doc comment and the field's layout. Type, representation, `save`, `restore` and consumers are unchanged | `git diff --stat`: one file, +5/-1, comments plus formatting | PASS |

Non-goals kept: no rename, no named-field conversion, no new helper or check, no documentation sweep. No Cargo tests were run, as the task directs.
