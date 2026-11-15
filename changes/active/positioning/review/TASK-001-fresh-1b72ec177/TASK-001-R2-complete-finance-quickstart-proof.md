---
id: TASK-001-R2
kind: remediation
status: ready
spec: SPEC-positioning
spec_revision: 5
requirements: [REQ-005, REQ-006, AC-004, AC-005, INV-003, INV-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-4, FIND-TASK-001-6, FIND-TASK-001-7]
---

# Complete the finance Quickstart update and its language proof

## Outcome and authority

A reader who follows the numbered Quickstart can reproduce the changed-policy
verdict. The Python and TypeScript journeys demonstrate that same update and
preserve the original execution's policy identity. Complete these bounded
corrections together with the remaining mandatory Rust module documentation.

- Approved specification: `changes/active/positioning/spec.md`, revision 5.
- Original task: `changes/active/positioning/tasks/TASK-001-positioning-and-finance-onboarding.md`.
- Original base: `443a79337fe33dfbfe9ccc64e01d10369d469131`.
- Reviewed candidate: `1b72ec1774f68a8eefc6f4f25304dfc5aee1248f`.
- Prior candidate: `40bcb4a072dcbb33bee734f9b1c3ff0f63cf0718`.
- Diagnosis: this directory's `findings-validation.md` and `verdict.md`.
- Route directly to `$wyrd-implement`; review does not implement this task.

This fresh full review uses current approved revision 5. Older prescriptions
to remove approved SDK helpers or add application error-handling scaffolding
are superseded and are not part of this task.

## Diagnosis and selected correction

### FIND-TASK-001-7: restore the facts after the negative exercise

Quickstart step 3, `docs/src/content/docs/get-started/quickstart.md:117`, tells
the reader to set recipient authorization false, then destination restriction
false. Step 5 at lines 143–157 changes only the PDF and promises the regulator
request now passes. The edited destination is still false. All three programs
reuse that destination, and the unchanged destination assertion correctly
forces the overall verdict to failed even when the new policy permits the
purpose. Existing revision tests start from the original true facts and miss
the sequential guide state. This prevents ordinary readers from reproducing
the required AC-005 follow-on.

Add an explicit instruction restoring both `authorized` and `restricted` to
true after the negative exercise and before later reruns. The guide is the
shared producer of the invalid reader state; fix it there for every language.
Regenerate the coupled agent text through the existing docs generator. Do not
weaken the assertion, change the PDF, introduce reset machinery or modify the
program to hide incorrect supplied facts. One guide instruction is sufficient.

### FIND-TASK-001-4: prove the taught update in Python and TypeScript

`sdks/wyrd-sdk-python/tests/integration/test_docsite_quickstart.py:103–152`
proves original-policy execution and unchanged reruns. Its revised-file case
at lines 214–228 only proves explicit-version mutation refusal. The two tests
in `sdks/wyrd-sdk-ts/wyrd/tests/integration/finance-release.test.ts:68–102`
never replace the policy PDF or inspect execution history. Neither reproduces
the published auto-versioned update. Rust's existing actual-program revision
journey proves that path against the shared owners, but does not satisfy
AC-005 and the mandatory real-SDK journey requirement on each taught surface.
This is an explicit proof gap, not a demonstrated runtime defect.

Extend the existing actual-program journey in each missing language using its
existing reader, provider and server fixtures. Preserve unchanged-rerun proof,
replace the downloaded PDF with the existing substantive revised asset, and
rerun the actual finance program. Prove that the regulator request changes
from failed to passed and that the selected Artifact, Verifier and Service
advance from 0.1.0 to 0.1.1. Keep an execution from before the change and read
its public details after the update to prove its exact Verifier and policy
Artifact/file selection still name the original versions. Inspect the new
selection through existing public SDK Card/execution reads. The current Rust
revision journey and existing SDK execution-detail helpers supply the pattern.

Use existing `run_quickstart`/reader-program execution and ordinary SDK
verification/inspection capabilities; no production result fields, output
changes, test harness, dependency or SDK/runtime modification is needed.
Bounded eventual-publication handling may be reused for explicit test
inspection only. Do not add waits to normal verification. Two focused
extensions at the existing language journey owners are the smallest complete
correction; deleting the follow-on would remove approved behavior.

### FIND-TASK-001-6: document the new Rust module

`examples/finance-release/finance_release.rs:4` introduces
`mod finance_service;`, but `finance_service.rs:1` has no inner module rustdoc
and the module declaration has no outer rustdoc. Individual struct/function
docs do not attach to the module. Root AGENTS and the Rust guide require
rustdoc on every new item regardless of visibility; this remains a mandatory
merge gate despite having no runtime consequence.

Add concise attached module rustdoc describing this declaration/input owner's
role. A short inner module comment in the existing file suffices. Do not add
runtime tests, a checker, helpers or unrelated documentation cleanup.

## Preserved behavior and non-goals

Keep the approved typed inputs and verdict predicates, simple typed observe
functions, reason output, exact PDF mount, three independent requirements,
YAML parity, provider disclosure and application-owned decisions. Preserve
explicit-version mutation refusal and negative/uncertain cases. No server,
security, tenancy, storage, migration, public API, architecture, release or
tag change is authorized by this remediation. Do not expand the educational
corpus or redesign the landing page.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| 7 | Following step 3 restores both facts before step 5; the later input state matches the passing output; generated agent text agrees |
| 4 | Each Python/TS actual program retains ordinary rerun proof, uses the substantive revised PDF, shows the regulator outcome change and exact new identities, and reads an old execution after the update with unchanged original policy attribution |
| 6 | The new finance_service module has attached rustdoc; existing Rust formatting remains clean |

Confirm nonempty test collection. Adding missing proof of existing correct
behavior may pass immediately; record verification-only evidence. If the new
scenario exposes an actual implementation defect, follow RED/GREEN/REFACTOR
for its correction. Never manufacture a failure. Documentation-only edits use
static/source and generator proof without a runtime test for a sentence.
Use only these focused
journey selectors, adjusting an existing title only if necessary and recording
the actual command:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/finance-release.test.ts -t '^runs the finance release Quickstart from a reader.s directory$'"
mise run docs:generate
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts
mise run docs:check:commands
mise run fmt:check
mise run py:lints
mise run py:typecheck
mise run ts:lints
mise run ts:typecheck
```

Run Cargo-backed prerequisites sequentially. Use static inspection of the
corrected numbered steps and existing passing Rust revision evidence to check
that the guide's state now matches its expected output. No broad aggregates,
full language suites or repeated full journeys are required for this bounded
remediation; integrated change review owns them. Record the new immutable
candidate, exact selectors, observed results and remaining limits.

## Implementation evidence

Candidate commits on `docs` after `1b72ec177`: `32667e6cf` (guide and
rustdoc), `f55de625d` (Python journey), `86e030f69` (TypeScript journey).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-7: step 3 restores both facts before step 5; agent text agrees | `docs/src/content/docs/get-started/quickstart.md` step 3 ends "Set both back to `true` before you continue."; regenerated `docs/public/llms-full.txt` | Static: step 5 now starts from `authorized`/`restricted` true, matching its passing output and the Rust revision journey `finance_release_policy_versions_remain_attributable`; `mise run docs:generate` leaves no diff; `indexes.test.ts` 7 passed; `docs:check:commands` exit 0 | PASS |
| FIND-4 (Python): unchanged rerun kept; revised PDF rerun; regulator failed→passed; Verifier/Artifact/Service 0.1.1; earlier execution names 0.1.0 Verifier and policy file | `test_docsite_quickstart_verifies_and_repeats` extended; `policy_file` helper reads the `follows-policy` mount; `support.details` for the bounded execution read | Python selector below: 1 passed (verification-only; existing behavior) | PASS |
| FIND-4 (TypeScript): same observations | `runs the finance release Quickstart from a reader's directory` extended with an in-test original-policy execution, revised rerun, Card reads, `support/server.details` | TS selector below: 1 passed, 1 skipped; whole file also run: 2 passed | PASS |
| FIND-6: `finance_service` module rustdoc | Inner `//!` doc in `examples/finance-release/finance_service.rs` (also shown on the Quickstart page) | `mise run fmt:check` exit 0 | PASS |

Commands run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_docsite_quickstart.py::test_docsite_quickstart_verifies_and_repeats"  # 1 passed
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/finance-release.test.ts -t '^runs the finance release Quickstart from a reader.s directory$'"  # 1 passed, 1 skipped
mise run docs:generate                                         # no further diff
mise exec -- pnpm --dir docs exec vitest run src/lib/indexes.test.ts  # 7 passed
mise run docs:check:commands                                   # exit 0
mise run fmt:check                                             # exit 0
mise run py:lints && mise run py:typecheck                     # exit 0
mise run ts:lints && mise run ts:typecheck                     # exit 0
git diff --check 1b72ec177..HEAD                               # clean
```

Non-goals held: no PDF, program, assertion, SDK, server, migration, or
architecture change; explicit-version refusal and negative/uncertain cases
untouched. Limits: the TypeScript reader directory is file-scoped, so the
later `judges each finance requirement on its own` test now declares against
the revised policy; its verdicts do not depend on policy content and it
passed. The Python and TypeScript journeys each now hold the original-run,
rerun, and update proof in one test, as this remediation directs.
