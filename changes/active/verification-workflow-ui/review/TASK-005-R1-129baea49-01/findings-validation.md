# TASK-005-R1 independent remediation validation

Recommendation: **PASS**. FIND-TASK-005-1, FIND-TASK-005-2 and
FIND-TASK-005-3 are closed at the reviewed candidate. No new material blocker
was found. Remediation TDD compliance is **PASS on the recorded evidence**;
original TASK-005 TDD compliance remains **VIOLATED** independently.

## Subject and procedure

Reviewed remediation diff from `147c235fd19dd4a400da1515a908db0d5521477d`
to `129baea4983d5b9444993503938e3641f760670b`, with the original
`05fc05138a...129baea49` cumulative source available. Ancestry is continuous.
Read the original TASK-005, approved verification-workflow-ui revision 8,
supporting authorization-model revision 9, prior verdict/validated findings and
specialist evidence, and the R1 task including execution records. Applicable
authorities include root AGENTS, task-review/UI/implementation skills,
spec-driven workflow, TESTING.md, maintainer style, Verifier semantics and
retained-parent security rules. No CodeGraph index exists. The accepted AC-009
differences and bounded original display-field/shared-mark decisions remain
accepted and are not reopened.

This is a fresh independent reviewer context. Source/diff, changed callers,
assertions, actual producer shapes and evidence were checked directly. Root
coordinated fresh executions; this reviewer ran no additional test commands and
changed only this report. The sole acceptance matrix is supplied to the
orchestrator for `verdict.md`, rather than duplicated here.

## Prior finding closure

### FIND-TASK-005-1 — CLOSED

`VerificationExecution.svelte:73` now forms unique recorded requirement IDs;
`:305–307` uses those unique IDs for fallback navigation. Selection remains
`req:<id>`. The existing inspector at `:429–448` still renders every judgment
of that ID, including individual item/iteration coordinates, reasons and
outcomes. No graph topology or definition access is invented.

`execution.ts:319–326` extracts the existing graph requirement-state calculation
into `requirementState` and reuses `attemptsOf` through `requirementAttempts`.
The graph caller at `:310` and fallback caller at component `:306` share it.
Comparison against the prior source confirms the graph's result-first state
semantics and mixed-instance count are preserved; there is no duplicate
orchestration or new state owner.

The new component test at `ui-library.test.ts:4040` uses two legitimate judgments
of one requirement with distinct item indices, including one failed judgment.
It asserts one selectable entry, the counted failure, emitted selection,
refreshed pressed state, both outcome rows, their coordinates/reason and no graph
node. This directly covers the old duplicate-key failure and preserved
instance inspection. Root's fresh exact component run passes both new tests;
the complete UI check passes 115 tests, including existing graph/state tests.

### FIND-TASK-005-2 — CLOSED

`VerificationExecution.svelte:73` includes settled requirement invocation IDs
in the recorded selector. At `:77–79`, selected-result judgments retain
precedence; invocation progress supplies recorded facts when those judgments
are unavailable. At `:450–456` the inspector shows the invocation's actual
outcome, reason, producer coordinates/attempt and non-none disposition. It
does not manufacture statement, actual value, citations, judge, final result or
execution verdict. Existing result judgment rendering remains the first branch.
The required invocation projection is already loaded by the unchanged
`execution.ts::loadExecution`; no new read, API or definition grant is added.

Source tracing confirms the existing engine checkpoints requirement settlement
(`vala-verify/src/blocks/engine.rs:1809–1850`), and the public inspection projection
supplies the producer/outcome/reason independently of final completion
(`verification/service/inspection.rs:205–225`). The new component test at
`ui-library.test.ts:4073` asserts selector ordering, click/refresh selection,
recorded failed outcome/reason/iteration/attempt, absence of the false unjudged
message, and running status with no final verdict.

The sibling real-server journey at `ui-library.integration.test.ts:2431` drives
public GitHub review admission and a real approval decision. The new fixtures
`approvals/staged.yaml` and `staged-repository.yaml` attach `reviewed` to one
approval while a second approval keeps the execution waiting. A separate
Repository-family viewer loads it through the real shared Node executor,
cannot populate the Verifier definition, receives no result and receives the
settled requirement invocation. The actual shared component renders that
selected judgment and attempt while preserving waiting/no-verdict facts.
This is connected producer-to-consumer proof, not a mock substituting for a
server dependency. Root freshly ran the exact three-test selector: all three
passed, including existing deleted-definition and subject-only publication
preview/download paths.

The fixture change is a sufficient bounded test choice. The unchanged signoff
fixture has whole-Verifier requirements, which wait until all steps complete
(`engine.rs:2413`); step-attached requirements settle after their own action
(`wyrd-spec/src/card/verifier/mod.rs:427`). Those sources explain the recorded
initial empty invocation trace and the independent diagnostician's stated
cause/fix site. Changing a widely consumed signoff fixture was unnecessary.
The new fixtures add no production lifecycle behavior or new user workflow.

### FIND-TASK-005-3 — CLOSED

`brand/components.json:974` now describes built, non-placeable `StatusMark`,
its shipped props, actual consumers, semantic tokens and existing mapping rules.
The deleted `VerdictPill` contract and its obsolete StatusPill/Badge references
are removed. Its replacement in Badge metadata points to the shipped renderer.
The component export and catalog capabilities themselves are unchanged.

The existing brand test at `brand/gen-theme.test.mjs:125` still verifies
component/export/catalog parity; it was not weakened or bypassed. Root's fresh
`mise run codegen:check` passes all seven brand checks and generated-output
checks. There is no new component placement capability or hand-edited generated
artifact. No runtime change is needed for this finding.

## Proof, TDD and carried evidence

Fresh current-candidate evidence in `verification.md` records `ui:check` PASS
(115 tests, package/host each zero Svelte errors/warnings), exact new component
selector PASS (two tests), codegen PASS (seven brand checks), production UI build
PASS, testing/server prerequisites PASS, and exact real-server selector PASS
(three tests, 20 unselected, 10.97 seconds). The commit-range whitespace check
passes. No uncompleted execution is used as acceptance evidence.

R1 records the expected duplicate-key failure and missing unfinished-requirement
entry before their production correction, with the actual command and observed
GREEN. The third scenario reuses the independently recorded red mandatory gate.
These are behavior/gate failures, not compilation/setup failures. The corrected
definition-lookup test setup is distinguished from RED. Therefore remediation
TDD is PASS on the available recorded history. The later real-server seam test
and its old-component mutation establish assertion strength; they are not
retroactive test-first evidence. Original TASK-005 remains VIOLATED, as required.

Unchanged original rows for discovery/navigation, frozen membership/bounds,
retained authorization, provider/wire projections and accepted visual behavior
carry forward from the prior source-backed reports. The remediation changes no
specification, architecture, server, SDK, SQL, permission evaluator, query,
credential transport or durable lifecycle. The existing shared component reaches
Node/MCP/custom/conventional consumers without a new host branch; rebuilt
artifact and original host evidence remain applicable. Reopened coverage is
limited to the changed shared inspection/state owner and contract metadata.
Prior reports newly committed with R1 are historical review evidence, not new
product source or new requirements.

## Boundary selection and conclusion

One fresh remediation reviewer is sufficient under the task-review skill:
this is bounded shared presentation/helper/fixture correction with no changed
security, persisted state, recovery topology, wire contract or major owner
replacement requiring an additional specialist. The prior domain/repository
specialist evidence stays applicable to unchanged boundaries. New fixture
admission and permission paths are proven through existing production owners.
No new blocker was discovered that would require another independent validation.
All three prior stable finding IDs close; no replacement finding or further
remediation task is required.

HEAD remained `129baea4983d5b9444993503938e3641f760670b` at the final source check;
only the new review directory was untracked. No production changes, commits or
pushes were made by this reviewer.

Skipped: full journey/library integration suites, benchmarks and new visual
captures. Those omissions do not leave a changed-path proof gap; original
visual rulings and focused current proof remain bounded to their assertions.
