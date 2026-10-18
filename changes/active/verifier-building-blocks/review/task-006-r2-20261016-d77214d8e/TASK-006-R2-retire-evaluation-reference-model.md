---
id: TASK-006-R2
title: Retire the remaining evaluation reference model
status: IMPLEMENTED
spec_revision: 11
findings:
  - FIND-TASK-006-4
---

# TASK-006-R2 — retire the remaining evaluation reference model

Route directly to `$wyrd-implement`. This packages the independently validated residual of TASK-006; it adds no product decision or runtime feature. Only a later independent `$wyrd-task-review` can accept the cumulative task.

## Authority and immutable input

- Approved specification: `changes/active/verifier-building-blocks/spec.md`, revision 11.
- Original task: `changes/active/verifier-building-blocks/tasks/TASK-006-verification-vocabulary-cutover.md`.
- Prior remediation: `changes/active/verifier-building-blocks/review/task-006-20261016-380663d83/TASK-006-R1-vocabulary-and-consumer-corrections.md`.
- Reviewed inclusive span: `f711c6834..d77214d8e`; exclusive base `b323327f4cbb690628303d3a6b5249a67af50c9b`; candidate `d77214d8eb119fc26ff114f4ef4a403bc3d3e426`.
- Current independent diagnosis: this directory's `findings-validation.md`, final ledger FIND-TASK-006-4. This directory's `verdict.md` records the cumulative verdict.

Spec revision 11 and the packets' committed trailing notes supersede historical vocabulary-scanner requirements. The scanner was withdrawn by the owner; it must stay absent. Served OpenAPI proof remains required and is already closed in this review.

## Diagnosis — FIND-TASK-006-4, INCORRECT / REVISED

REQ-017/018/022, INV-008 and AC-009/011/016 require current consumers to stop teaching the removed evaluation product. The runtime cutover deletes EvalRow/EvalsView/EvalTask/EvalDetail, their projections and `/observe/evaluations` routes. Current VerifierSpec declares inputs, steps, requirements, decide and context_capture. It does not supply the former task/score/Workflow declaration or canonical evaluation record.

The existing product-reference owner still produces that invalid model. Under `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/`:

| Source | Remaining instruction and direct consumers |
|---|---|
| `README.md:3–7` | Retains route, information-architecture and state-reference authority; only visual treatment is superseded. Ledger entries at 167–175, 524–528 and 584–588 actively describe the obsolete views. |
| `cards.svg:1`, C-08-light/dark | checkout-quality v7 / card_workflow_01 is a read-only Workflow definition with retrieve-context → score-tasks → judge-thresholds → publish-record stages, all-required-tasks-pass gates and evaluation records in Observe. |
| `cards/experiment/outputs.svg:1`, E-17-light/dark | `section=evaluations&task=groundedness`, six scored tasks, failed examples/judges, Workflow/tasks/judges/gates declared on the Card, and a canonical Evaluation run in Observe. |
| `cards/experiment/runs.svg:1`, E-07-light/dark | Repeats the scored-task summary, aggregate/task output projection and canonical Observe instruction. E-04 and E-07 navigate the same Evaluations section. |
| `cards/experiment/outputs.svg:1`, E-16-light/dark; `cards/experiment/mobile.svg:1`, EM-02-light/dark | Direct section navigation still promises that removed E-17 model. |

Current UI-foundation task consumers use this reference package. Its deferred Experiment implementation notes do not make the design historical: they explicitly preserve static design coverage. A maintainer or agent following the current reference is therefore directed to recreate an unsupported declaration/result ontology and canonical Observe workflow. This finding concerns implementation instructions, not a claim that the running Svelte app already renders E-17. R1's source/XML/render checks and disclosed exclusions did not remove that contradiction. Renaming it to verification leaves the cause intact.

The shared cause is incomplete retirement at the reference producer. Individual links are its direct consumers, not separate findings. Prior named-panel retirement and the mobile-navigation collision are already closed.

## Selected correction

Finish retirement in the existing reference package; deletion is sufficient:

1. Delete C-08 and E-17 artboards in both themes and their positive ledger/state/route/annotation entries.
2. Remove E-07's obsolete scored-task summary, repeated aggregate/task output projection and Evaluation-run-in-Observe instruction. Preserve unrelated recorded run metrics and linked content.
3. Remove direct Evaluations-section destinations from E-04/E-07/E-16/EM-02 and their ledger/annotations. Do not leave a destination to an absent section or merely rename its old task model.
4. Reconcile only directly affected counts, matrices and references. With these two page deletions, the validated expected counts are cards12 pages/24 artboards; Experiment outputs8/16; Experiment package20/40; general set38/76; complete set77/154. Preserve unaffected IDs.

This removes the invalid source instruction and its actual consumers together. Do not add replacement panels, a new projection or downstream warnings. Where an otherwise retained check link remains useful, reuse the existing current Verifier Card destination without inventing a new route.

Expected write set: the product README and four SVGs listed above. Any additional file must be a demonstrated direct reference consumer of these deletions, not an opportunity for broader cleanup. Separate UI-foundation planning packets need no change.

## Preserve and exclude

Preserve Experiment lifecycle/session/tool/token/cost/latency metrics, ordinary offline-evaluation descriptions, training/metric/file/trace/provenance/output content, current Verifier/Changes references, runtime Service/Card identity and the corrected four-link navigation. Preserve backend engine/decide semantics, authorization, tenant projection, SDK contracts and data/lifecycle owners.

Do not require C-01 general kind/filter cleanup, C-14 or Service runtime_workflow/Workflow-dispatch removal, or E-07's unrelated review-loop v3 Workflow link removal. Independent tracing did not establish those as dependencies of the retired evaluation model; REQ-017 excludes repeating general Workflow removal. Do not rename ordinary metrics or standard GenAI projections.

No new Experiment workspace, runtime, Card kind, Operator action, result panel, API, compatibility/history/recovery/conversion/GC mechanism, dependency, renderer framework, name ban, script or scanner. Nothing has shipped; no extra migration work belongs here. Do not reopen closed findings 1–3 or unrelated TASK-005 work.

## Acceptance mapped to FIND-TASK-006-4

- C-08/E-17 no longer positively declare the retired model; their ledger/state/route entries and direct annotations are retired with them.
- Retained E-07 no longer projects the obsolete scored-task model or promises its canonical Observe record. E-04/E-07/E-16/EM-02 have no corresponding Evaluations-section destinations.
- Affected counts and IDs agree with actual XML; unaffected content and IDs remain intact. Source inspection distinguishes obsolete product instructions from ordinary metrics and unrelated Workflow debt.
- Retained edited desktop/mobile artboards are readable in both themes, with no orphaned controls or layout damage. Existing390px mobile layout remains readable.
- The write set is reference-only, with no runtime/API/security/data change and no reinstated vocabulary scan.

## Focused proof

Use existing source/XML/offline-render procedures; no permanent test script or new harness is required. Read affected ledger, XML and current Verifier/result destinations together and record the semantic closure, not just word absence. Parse the four edited SVGs and inspect actual artboard IDs/counts against README:

```bash
mise exec -- /usr/bin/python3 -c 'import xml.etree.ElementTree as E; from pathlib import Path; p=Path("crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product"); [E.parse(p/f) for f in ("cards.svg", "cards/experiment/runs.svg", "cards/experiment/outputs.svg", "cards/experiment/mobile.svg")]'
git diff --check
```

Render and view retained modified E-04/E-07, E-16 and EM-02 in both themes with installed offline Chrome. Existing dimensions are runs3136×6060, outputs3136×10852 and Experiment mobile956×3344; use the edited dimensions if compacted. Example:

```bash
mise exec -- /usr/bin/google-chrome --headless --no-sandbox --disable-gpu --disable-background-networking --host-resolver-rules='MAP * ~NOTFOUND' --hide-scrollbars --window-size=3136,6060 --screenshot=/tmp/task006-r2-runs-after.png "file://$PWD/crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/experiment/runs.svg"
```

Repeat for outputs.svg and Experiment mobile.svg with the appropriate dimensions/separate temporary files, then inspect affected retained artboards. Deleted panels need no replacement render. Record font and pixel-coverage limits. Add the focused proof/evidence to this task.

For this asset/README-only correction, no backend suite, UI runtime build/test suite, generator, full gate or user-journey aggregate is required. Already closed served-contract checks need not be rerun for unrelated asset deletions. Full integrated qualification remains change-review work. If executable code becomes necessary, stop scope expansion and explain why the reference-only approach is insufficient before prescribing a larger change.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| C-08/E-17 retired with ledger, state, route and annotation entries | `cards.svg` C-08 light/dark deleted, later sheet shifted up 1290, height 17118→15828; `outputs.svg` E-17 deleted, height 10852→9644, title "Outputs, system, provenance, traces"; README C-08/E-17 sections, R-CARD/R-EXP and C-08 states rows removed | No `C-08`, `E-17`, `section=evaluations` or `Evaluations` match under the product reference package; no C-08/E-17 artboard ids | PASS |
| E-07 no longer projects scored tasks or a canonical Observe evaluation record | `runs.svg` E-07: EVALUATION SUMMARY panel, "Evaluations · 6 tasks" chip, "Evaluation run in Observe ↗" link and 0.87 score removed; TIME BY STAGE widened into the freed space; badge now "✓ VERIFIER QUALITY · PASS"; annotation and README E-07 entry updated | E-07 light/dark offline render | PASS |
| E-04/E-07/E-16/EM-02 have no Evaluations-section destination | Run-section tab removed in E-04, E-07, E-16 (light/dark); EM-02 chip removed (light/dark); README links updated | Source search above; renders of E-04, E-16, EM-02 | PASS |
| Counts and IDs agree with XML; unaffected IDs intact | README: cards 12/24, Experiment outputs 8/16, Experiment package 20/40, general 38/76, complete 77/154, 61 desktop pages | Unique `*-light`/`*-dark` ids (each artboard id appears twice): cards 48→24, outputs 32→16, Experiment 80→40, all 308→154; HEAD 316→158 (−4 artboards = 2 pages × 2 themes) | PASS |
| Retained edited artboards readable in both themes | Layout reflowed only where panels were deleted | Renders: E-04, E-07, E-16, C-09 (first page after the shift), C-14 (end of sheet) at 3040×1220; EM-02 at 860×920; no orphaned controls, overlap or clipping | PASS |
| Reference-only write set; no runtime/API/data change; no scanner | Write set: product `README.md`, `cards.svg`, `cards/experiment/{runs,outputs,mobile}.svg` | `git diff --stat`; four SVGs parse with `xml.etree`; `git diff --check` clean | PASS |

Commands:

```bash
mise exec -- /usr/bin/python3 -c 'import xml.etree.ElementTree as E; from pathlib import Path; p=Path("crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product"); [E.parse(p/f) for f in ("cards.svg", "cards/experiment/runs.svg", "cards/experiment/outputs.svg", "cards/experiment/mobile.svg")]'
git diff --check
mise exec -- /usr/bin/google-chrome --headless --no-sandbox --disable-gpu --disable-background-networking --host-resolver-rules='MAP * ~NOTFOUND' --hide-scrollbars --window-size=3040,1220 --screenshot=<out>.png "file://<viewBox-cropped copy>.svg"
```

Preserved: ordinary `offline_evaluation` run kinds, the C-06 "evaluation" run-kind label, the review-loop v3 Workflow link, and C-14 Workflow dispatch (excluded by this task).

Limits: renders were taken from temporary viewBox-cropped copies of the edited SVGs, one artboard pair per image, using offline Chrome's fallback fonts; every pixel of the full sheets was not inspected. The separate `changes/active/wyrd-ui-foundation` planning packet still names C-08 and E-17; this task excludes it from the write set.
