---
id: TASK-005-R1
kind: remediation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 8
supporting_spec: SPEC-authorization-model
supporting_spec_revision: 9
parent_task: TASK-005
remediates: [FIND-TASK-005-1, FIND-TASK-005-2, FIND-TASK-005-3]
requirements: [REQ-004, REQ-005, REQ-014, AC-002, AC-008]
---

# Keep recorded requirements inspectable without a definition

## Outcome and subject

A reader authorized for retained execution facts can inspect recorded requirements
when the Verifier definition is unreadable or deleted, including repeated judgments
and settled checks while other work continues. The shared status renderer also has
the required component contract and passes its existing gate.

Original task: `changes/active/verification-workflow-ui/tasks/TASK-005-pr-review-sets.md`.
Reviewed base: `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`.
Reviewed candidate: `147c235fd19dd4a400da1515a908db0d5521477d`.
Approved spec revision 8 blob: `e736e3463f8862b7ae0a34dcca9274e23c00dcfa`.
The authoritative diagnoses and evidence are recorded once in
[findings-validation.md](findings-validation.md); this task defines their bounded
correction and observable closure. No production implementation is included in
this review.

## Correction boundary

**FIND-TASK-005-1 — repeated requirement identities.** REQ-004/005 and
AC-002/008 require recorded selection without definition access. A legitimate
fanout/repeat result contains multiple judgments of the same requirement ID,
distinguished by item/iteration. The new fallback in
`packages/ui/src/lib/components/VerificationExecution.svelte` keys every judgment
by that shared ID; isolated rendering reproduces `each_key_duplicate`.
Correct requirement navigation at this existing component owner so supported
instances render reliably and remain inspectable. Its existing requirement
selection groups all judgments by ID; reuse that behavior and preserve every
recorded instance. Do not fabricate graph topology or require definition access.

**FIND-TASK-005-2 — settled checks before a final result.** REQ-004/005 and
AC-002/008 require independently readable recorded facts to remain selectable.
The existing public invocation read already provides a settled requirement's
outcome, reason and producer while the execution remains unfinished. The same
component excludes requirement invocations from its fallback and derives
requirements exclusively from a final result, hiding that progress and reporting
“Not judged yet.” Reuse the bounded invocation data already supplied by
`execution.ts::loadExecution` for truthful recorded requirement selection and
inspection. Preserve selected-result authority when a result exists. Show only
fields actually recorded; do not invent a final execution verdict, full result,
actual value or citations absent from the invocation response.

**FIND-TASK-005-3 — component contract gate.** REQ-014's shared implementation
and the mandatory UI contract rules require contracts to match shipped exports.
The exported `StatusMark` has no `brand/components.json` entry, while the deleted
`VerdictPill` and its references remain. The fresh `mise run codegen:check` fails
the existing component contract assertion. Update the existing brand contract
and affected references to describe the shipped renderer; retire the removed
component's obsolete contract. Use existing generation tooling where applicable;
do not hand-edit generated outputs or suppress/weaken the check. This finding
requires no new runtime behavior.

Shared package changes reach the conventional host, MCP artifact and custom app
through the existing component and loader. Keep the current contracts,
authorization, exact execution/producer selection, completed-result behavior,
PR pin/history semantics, independent definition refusal and accepted visual
decisions. No server API, SDK field, query, permission evaluator, new dependency,
workflow, benchmark or additional visual requirement is authorized.

## Ordered closure scenarios

1. **Repeated judgments remain readable** (FIND-TASK-005-1;
   REQ-004/005/014, AC-002/008). In the existing `ui-library.test.ts` owner,
   render an execution with an unavailable definition and legitimate judgments
   sharing an ID across item/iteration instances. Select the recorded requirement
   and refresh its data. Rendering must succeed, selection must remain coherent,
   and all recorded instance judgments remain inspectable. No graph/YAML is
   invented. Narrow tier: component regression, existing Vitest runner.
   - **Evidence — PASS.** Test "a requirement judged once per item stays one
     selectable entry with every judgment when the definition is refused"
     (`src/lib/ui-library.test.ts`): two `classified` judgments (item 0
     passed, item 1 failed), definition refused; asserts one entry
     `Requirement classified 1 of 2 failed`, the `req:classified` intent,
     the pressed entry after the host's refreshed props, both judgments with
     their item coordinates and reason, and no `[data-node]`.
     Command [C1]: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t 'stays one selectable entry with every judgment|stays readable from its recorded invocation'`.
     RED: `Svelte error: each_key_duplicate`. GREEN: 2 passed.
   - Fix: fallback navigation lists each recorded requirement ID once
     (`VerificationExecution.svelte` `judgedIds`); its state word comes from
     the new `execution.ts::requirementState`, extracted from
     `executionStates` so the graph and the fallback share one owner. The
     inspector's existing `judgments` grouping keeps every instance.
2. **A settled check remains visible while peers continue** (FIND-TASK-005-2;
   REQ-004/005/014, AC-002/008). With an unavailable definition, unfinished
   execution, no selected final result, and a recorded settled requirement
   invocation, select that requirement and refresh. Its available outcome,
   reason and exact producer remain readable; unfinished status and absent final
   verdict remain truthful. Use a focused component assertion and extend the
   existing real-server no-definition journey in
   `ui-library.integration.test.ts` for this checkpoint-to-consumer state.
   Do not substitute disconnected mocked data for the required real-server seam.
   - **Component evidence — PASS.** Test "a requirement settled while other
     work runs stays readable from its recorded invocation when the definition
     is refused" (`src/lib/ui-library.test.ts`, [C1]): running execution, no
     result, `cards.get_by_uid` refused, one settled `has-answer` invocation
     (iteration 2). Asserts the entry `Requirement has-answer failed` before the
     step entries, the select intent, the pressed entry after refresh, the
     recorded outcome, reason and `iteration 2 · attempt 1`, no
     "Not judged yet.", status `running` and "no verdict recorded".
     RED: the list lacked the requirement entry (`expected [ 'Step read-photo
     succeeded', …(2) ] to deeply equal [ …(4) ]`). GREEN: as [C1].
     Test-setup correction before RED: an unfinished execution reads its
     definition by UID, so the refusal moved from `cards.get_by_ref` to
     `cards.get_by_uid`.
   - Fix: when the selected result has no judgment for the requirement, the
     inspector shows its settled requirement invocations
     (`execution.ts::requirementAttempts`): only outcome, reason, exact
     invocation and a non-`none` disposition. When a result exists, its
     judgments stay the only source.
   - **Real-server evidence — PASS.** Test "TASK-005: a requirement judged
     while its execution waits stays readable to a Repository reader who
     cannot read the Verifier" (`src/lib/ui-library.integration.test.ts`):
     a real GitHub review admits `staged-signoff`. Deciding its `review`
     approval judges the attached `reviewed` requirement while `release`
     keeps the execution `waiting`. A Repository-family viewer's
     `loadExecution` returns `waiting`, no result, the definition not
     populated and the requirement invocation `[reviewed, passed, 1]`. The
     rendered component shows the pressed `Requirement reviewed` entry, the
     judgment `passed` with `attempt 1`, no "Not judged yet.", status
     `waiting` and "no verdict recorded".
     Command [I1]: `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t 'a requirement judged while its execution waits|member whose Repository policy and Verifier Card were deleted|a reader of the subject but not the Verifier'`.
     GREEN: 3 passed, 20 skipped. Prerequisites were run one after the other
     first: `mise run ts:build:testing`, then `mise run build:server`.
     Mutation: with the pre-fix component restored, the journey fails
     (`Unable to find … button … /Requirement reviewed/`); the source was then
     restored.
   - Mechanical deviation: the existing no-definition journeys cannot reach
     this state. `holdPublications` records no requirement, and the deleted
     member's policy has no requirement that settles before the run ends. So
     the proof is a sibling journey in the same owner file, with a new fixture
     pair (`fixtures/cards/approvals/staged.yaml`,
     `staged-repository.yaml`). The shared `signoff` fixture is unchanged
     because docs and the Rust, Python, TypeScript, MCP and CLI journeys embed
     it.
   - Diagnosis (first journey attempt, `signoff` fixture).
     **Symptom:** after the CI evidence, the invocation page was
     `items: []`. **Evidence:** the DIAG dump showed `waiting` with the
     `signoff` approval pending. `crates/wyrd-spec/src/card/verifier/mod.rs:66`
     says whole-Verifier requirements are judged after every step, and
     `crates/vala/vala-verify/src/blocks/engine.rs:2413` gates
     `Node::Requirement` on all steps being complete. **Cause:** `green` is a
     whole-Verifier requirement, so it cannot be judged while the approval
     waits. The server and UI behave as the contract says. **Fix site:** the
     test fixture. A step-attached requirement is judged after its own step
     (`mod.rs:427`). A fresh read-only diagnostician, given only the
     command, trace and diff, reported the same cause and fix site and listed
     the `signoff` fixture's other callers.
     A second setup correction: the status omits `waiting` when it is empty.
3. **The shipped renderer matches component contracts** (FIND-TASK-005-3;
   REQ-014 and mandatory contract gate). The narrow brand contract test and
   `mise run codegen:check` pass with StatusMark documented and deleted component
   references retired. Existing shared-mark tests retain their current outcomes,
   including green completed status.
   - **Evidence — PASS.** `brand/components.json`: a `StatusMark`
     contract (built, `catalog: false`, its four consumers, tokens taken from
     its stylesheet, props `value`/`badge?`/`label?`/`title?`, rules taken
     from its source comment). The `VerdictPill` contract is retired, and
     `VerdictPill` is removed from `StatusPill.consumers` and from
     `Badge.implementedBy` (now `StatusMark`). No generator reads
     `components.json`, so nothing was regenerated.
     Command [B1]: `mise exec -- pnpm exec node --test --test-name-pattern='component contracts list exactly' brand/gen-theme.test.mjs`
     (from the UI root). RED was the gate failure the review recorded
     (`['StatusMark']` had no contract). GREEN: 1 passed.
     `mise run codegen:check`: PASS, 7 of 7 brand checks.
     `mise run ui:check`: PASS, 115 of 115 tests (113 before plus the two new
     ones; the existing shared-mark tests, including the green completed
     status, are unchanged); package and host svelte-check report 0 errors and
     0 warnings.

## Runners and evidence

Read the `wyrd-implement` and `wyrd-ui` skills before implementation. Follow the
original task's test-first discipline for new regressions; record actual focused
selectors and RED/GREEN results. Historical TDD remains **VIOLATED** and is not
repairable through manufactured RED or production churn.

Run component selectors using the existing runner, with the new actual names:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/ui-library.test.ts -t '<actual focused selector>'
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t '<actual focused no-definition selector>'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec node --test --test-name-pattern='component contracts list exactly' brand/gen-theme.test.mjs
mise run ui:check
mise run codegen:check
```

Prepare existing addon/testing/server prerequisites sequentially through their
mise tasks before the real-server run; do not overlap Cargo work. Run the UI build
for affected bundle/artifact verification and retain focused existing terminal
no-definition/PR-host checks as applicable. Full shipped-surface journey suites
remain integrated change-review scope. Return the changed candidate and evidence
to the maintainer; do not initiate a review, commit or push automatically.

### Verification record

- Candidate: base `147c235fd`, then commits `5a3f80c59` (contract),
  `07e79577f` (component), and the journey/fixture commit after it. The
  maintainer asked for frequent commits.
- `mise run ui:check`: PASS (115/115). `pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`: PASS.
  `mise run codegen:check`: PASS. `git diff --check 147c235fd..HEAD`: PASS.
- Diff audit: 7 files, all in this task's scope: `brand/components.json`,
  `VerificationExecution.svelte`, `execution.ts`, the two UI test files and
  the two new approval fixtures. No server API, SDK field, query, permission,
  dependency, workflow or visual change was made.
- Not run: full shipped-surface journey suites and the full library
  integration suite. Both are change-review scope.
- TDD: the three new regressions ran RED before their production fix
  (FIND-1 and FIND-2 in [C1]; FIND-3 was the existing gate's failure). The
  real-server journey came after the component fix, as a seam proof, and the
  mutation check above shows its assertions are strong enough. Historical
  TASK-005 TDD remains **VIOLATED**.
