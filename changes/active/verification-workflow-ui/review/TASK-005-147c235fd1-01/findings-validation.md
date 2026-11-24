# TASK-005 independent findings validation

Recommended verdict: **FIX_REQUIRED**. Behavioral acceptance fails on two concrete recorded-inspection paths and one mandatory component-contract gate. Historical TDD compliance remains **VIOLATED**, separately; none of the corrections below requires retroactive RED evidence or production churn to repair history.

## Immutable subject and independent procedure

Base `05fc05138a0a72d59a2fd9b96fae5e6e4ad6f285`; candidate `147c235fd19dd4a400da1515a908db0d5521477d`. HEAD remained at that candidate on this validator's final source check. Approved contract: verification-workflow-ui revision 8; supporting authorization-model revision 9. Original task: `changes/active/verification-workflow-ui/tasks/TASK-005-pr-review-sets.md`. Read source/diff and primary behavior, domain, repository, scope and verification reports independently. Applicable authority includes root AGENTS, spec-driven workflow, testing and maintainer guidance, retained-parent security authority, the UI skill and the original task's scoped scenarios. The current maintainer's AC-009 differences and display-field/shared-mark decisions are accepted.

The primary and both selected specialist reports exist. Selection is adequate: primary covers navigation, exact historical identity, shared presentation and local maintainability; domain covers managed authorization, query completeness and retained inspection; repo covers admission identity and wire/SDK propagation. No additional recovery/system or owner-abstraction specialist is necessary: no new recovery topology or substantial owner replacement appears in this task. Existing Oracle scan rewriting and domain retained-parent checks were inspected, as were set-loader/history/member queries, PR route loading/navigation, public invocation shapes/checkpoint production and affected assertions. No new blocker was discovered by this validator.

## Proposal reconciliation

| Discovery proposal | Decision | Retained identity |
|---|---|---|
| Primary B1 and DOMAIN-1, same repeated-requirement-key cause | CONFIRMED; consolidate into one finding | FIND-TASK-005-1 |
| DOMAIN-2, preterminal settled requirement invocation omission | CONFIRMED with correction limited to actual available progress fields; never synthesize a final result | FIND-TASK-005-2 |
| Primary B2, missing exported component contract | CONFIRMED | FIND-TASK-005-3 |
| Repository specialist: no proposed finding | Reviewed coverage and unchanged propagation; no retained finding | — |

The single reconciled acceptance matrix is incorporated into `verdict.md` by the orchestrator from this validator's assessments; it is not duplicated here.

## FIND-TASK-005-1 — INCORRECT: repeated requirement IDs break definition-unavailable rendering

**Authority:** approved REQ-004/005/014, AC-002/008; original task scenario 4. Repeated/fanout work is supported, and definition refusal must preserve independently authorized recorded selection.

**Exact location:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/components/VerificationExecution.svelte:302` (keyed fallback navigation); affected consumer is every host using this shared component.

**Starting conditions / trigger:** An authorized reader cannot read the Verifier definition, or the definition was deleted. A completed execution has two recorded instances of one step requirement with the same requirement ID and different fanout item or repeat iteration. Opening it, or refreshing into that result, enters the new fallback.

**Expected / actual:** Recorded facts and all judgments remain selectable without graph/YAML access. Instead, the fallback uses the authored requirement ID as a unique Svelte key for every instance. Legal repeated IDs collide. Checked rendering throws `each_key_duplicate`; the keyed reconciliation has no unique identity for these entries. Existing requirement selection is by `req:<id>`, and the inspector deliberately groups all result judgments sharing that ID, so the bug is in navigation, not the stored result.

**Independent evidence:** `wyrd-spec/src/verification/execution.rs:646–660` defines requirement ID and optional item/iteration coordinates. Real engine tests at `crates/vala/vala-verify/src/blocks/engine/tests.rs:3543–3553` and `:3784–3793` assert repeated IDs with distinct coordinates. Component lines 58/75 already count unique IDs and group judgments, while line 302 keys every raw instance by ID. This validator independently rendered the built component with the existing settled fixture, refused definition and two item instances: rendering throws `each_key_duplicate` (isolated assertion passed). The primary independently reproduced the same failure.

**Consequence:** Normal supported repeated/fanout executions cannot reliably be inspected by Repository-only readers or after definition deletion.

**Smallest testable correction:** Keep one stable requirement navigation entry per ID consistent with existing selection, while retaining every item/iteration judgment in the inspector. Reuse this component's existing result and selection owners; do not change server contracts or grant definition access. Focused component proof must render a refused/deleted definition with repeated IDs, select the requirement and show distinct instance judgments across refresh. Preserve no fabricated graph topology.

## FIND-TASK-005-2 — MISSING: settled requirement progress disappears before the final result

**Authority:** approved REQ-004/005/014, AC-002/008; original task scenario 4. Without a definition, recorded requirements and bounded invocations remain selectable; independently available progress must not disappear. This does not authorize synthesizing a final judgment.

**Exact locations:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/components/VerificationExecution.svelte:71`, `:75`, `:298–304`, `:446–448`.

**Starting conditions / trigger:** A queued execution has checkpointed a requirement while another branch or later iteration is still running or waiting. The parent-authorized reader has no readable Verifier definition. The invocation page returns the settled requirement's producer, outcome and reason; status has no final result. Open the fallback, or an exact `node=req:<id>` link.

**Expected / actual:** The existing recorded progress remains selectable and its available outcome/reason/exact producer can be inspected, alongside unfinished execution status. Instead, `ran` explicitly excludes settled requirement invocations; fallback requirement navigation reads only the final result; the requirement inspector also reads only that result. No selector appears. An exact requirement URL says “Not judged yet.” even when its invocation was judged, and the recorded reason is absent.

**Independent producer-to-consumer evidence:** `crates/vala/vala-verify/src/blocks/engine.rs:1809–1850` checkpoints a requirement settlement before returning it; repeat engine tests at `engine/tests.rs:3767–3783` show one iteration's checks finish before the next action. `crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs:205–225` maps these records to `InvocationProgress::Requirement` and a reason, and `:778–815` returns the accepted invocation page without requiring terminal completion. Public `ExecutionInvocation` at `wyrd-spec/src/verification/execution.rs:1167` carries exact producer/outcome/reason. `execution.ts:206–212` already loads these records without a definition. The component drops them as described above. This validator independently rendered running status, a settled requirement invocation and refused definition: no recorded selector, false “Not judged yet.”, and absent recorded reason were all observed in a passing falsification assertion.

**Consequence:** A restricted reader loses an already recorded check until all work completes; the inspector makes an incorrect availability claim during an ordinary unfinished workflow.

**Smallest testable correction:** Use existing bounded invocation progress for requirement navigation and available progress inspection when a selected result does not supply it. Preserve exact producer/instance coordinates, unfinished execution status, disposition and final selected-result authority. Show only the invocation fields actually delivered; do not fabricate actual values, citations, judge details, statements, final overall verdicts or declared topology. No new API or full-result synthesis. Add focused component assertions for selection/refresh and a focused real-server consumer proof for the supported checkpoint state using existing journey/harness owners; broad suites are unnecessary.

## FIND-TASK-005-3 — VIOLATION: shared renderer export has no component contract

**Authority:** root AGENTS required checks/generated-contract rules; UI skill component steps 1/5; original TASK-005 verification requires `mise run codegen:check`.

**Exact locations:** `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/index.ts:35`, `brand/components.json` (missing StatusMark; retired VerdictPill entries at 974/998/1275); gate `brand/gen-theme.test.mjs:125–135`.

**Trigger / expected / actual:** Running required codegen verification should establish exported-component/brand/catalog parity. The export now names `StatusMark`, but the brand contract has no such component and still describes deleted VerdictPill and a VerificationExecution dependency on it. The required gate fails with uncontracted `['StatusMark']`. This is an observed mandatory-gate failure, not a subjective documentation preference.

**Independent evidence:** Source reads confirm the export and missing/stale entries. Root's candidate-bound `mise run codegen:check` exited 1 at line 131. This validator independently ran `mise exec -- node --test --test-name-pattern='component contracts list exactly' brand/gen-theme.test.mjs` from the UI root; one selected test failed at the same assertion.

**Consequence:** The candidate cannot pass its required generated/component-contract check; published component metadata contradicts the shipped renderer.

**Smallest testable correction:** Update the existing brand contract owner for the shared renderer and affected dependency/source references; retire the deleted component's contract. Keep the renderer non-placeable unless existing authority requires otherwise; no new catalog capability is implied. Regenerate only relevant existing outputs and run the focused brand test plus `mise run codegen:check`. Do not suppress or weaken the test. This finding needs no production behavior change.

## Executable validation and proof limits

This validator used an isolated temporary Vitest harness outside the repository, importing the current built component and existing fixtures. Command from `crates/wyrd/wyrd-server/wyrd-ui`: `mise exec -- pnpm exec vitest run --config /tmp/wyrd-task005-validation-38jbvfiv/vite.config.ts -t 'validator reproduces'`; 2 selected falsification tests passed (1.59 seconds). The tests assert the observed failures, not successful product behavior. No tracked source/test change occurred. Temporary harness paths are diagnostic context, not durable regression coverage; remediation must add ordinary owner tests.

Root's fresh `ui:check` passes 113 tests and both package/host checks, Python typechecking passes, and the selected real-server library run passes 5 tests with 17 unselected. Those tests establish current/history/set/authorization/host/deleted-definition happy and negative cases but do not exercise repeated requirement keys or preterminal requirement selection. Root's full `codegen:check` fails as above. Unchanged Rust/provider projection evidence can carry forward because later commits alter UI presentation only. Recorded Alice/other browser evidence remains applicable to its assertions, with current unit/component proof for shared marks; the accepted AC-009 differences are not reopened. The full library integration suite and broad shipped-surface aggregates were not rerun; focused checks are appropriate to this task review and do not excuse the concrete failures.

No authorization leak, false complete-set claim, display-field source-identity change or new workflow owner was found in the reviewed paths. No extra benchmark, redesign, endpoint, SDK field, permission check or recovery guarantee is required by remediation. No new finding needs additional independent discovery. Review source remains immutable; only this report was written.
