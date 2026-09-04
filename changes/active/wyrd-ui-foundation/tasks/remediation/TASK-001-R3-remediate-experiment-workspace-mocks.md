---
id: TASK-001-R3
title: Remediate the Experiment Card into a complete experiment workspace
kind: remediation
status: complete
spec: SPEC-wyrd-ui-foundation
spec_revision: 3
requirements: [REQ-015, REQ-016, REQ-017, REQ-080, REQ-118, REQ-119, REQ-120, REQ-121, REQ-122, INV-008, INV-009, INV-014, INV-015, INV-017, INV-018, AC-003, AC-007, AC-009]
depends_on: [TASK-001-R2]
parent_task: TASK-001
remediates: [FIND-TASK-001-EXP-01, FIND-TASK-001-EXP-02, FIND-TASK-001-EXP-03, FIND-TASK-001-EXP-04, FIND-TASK-001-EXP-05, FIND-TASK-001-EXP-06]
---

# Outcome and user value

Replace the single generic Experiment summary mock with a complete, coherent
experiment-registry workspace. The renders must let a user:

- understand the Experiment and its current activity;
- find and filter runs;
- inspect queued, running, completed, failed, and cancelled work;
- understand model-training and agentic/workflow runs without entering separate
  products;
- compare compatible runs;
- inspect scalar metrics, metric histories, table metrics, visuals, files,
  registered Artifact Cards, system measurements, provenance, traces, and Eval
  results; and
- navigate every Experiment subview through explicit, restorable mock URL state.

This task designs static UI behavior only. It deliberately exposes current
server and contract gaps so later Wyrd work can implement the approved product
model. Those gaps do not block or shrink the mocks.

# Validated finding ledger

- **FIND-TASK-001-EXP-01 — Experiment is rendered as a record rather than a
  workspace.** C-06 compresses runs, parameters, metrics, hardware, one figure,
  and relationships into one generic Card body. It cannot support actual
  experiment analysis.
- **FIND-TASK-001-EXP-02 — Experiment subnavigation is missing.** The current
  mock has no Overview, Runs, Compare, Outputs, or Versions workflow and no
  stable way to link to selected analysis state.
- **FIND-TASK-001-EXP-03 — Run lifecycle and inspection are missing.** The mock
  shows only completed runs and omits empty, queued, running with partial data,
  failed, and cancelled states as well as substantial selected-run detail.
- **FIND-TASK-001-EXP-04 — Outputs are flattened or absent.** Table metrics,
  useful figures, file browsing, previews, and registered Artifact lineage are
  absent. “No notebooks” is not an artifact experience.
- **FIND-TASK-001-EXP-05 — Model and agentic runs are not represented.** The
  current surface cannot show training-specific analysis or agentic traces,
  tool calls, tokens, cost, latency, and evaluation results.
- **FIND-TASK-001-EXP-06 — The monolithic Card contact sheet no longer scales
  to detailed kind workflows.** Adding every Experiment state to `cards.svg`
  would make the general Card sheet harder to review and would force later Card
  detail work into one unmaintainable file.

The user validated these findings and directed this remediation on 2026-09-04
after review of OpsML's Experiment UI, the current Wyrd contracts and mocks,
and current MLflow and Weights & Biases experiment-registry patterns.

# Authority and required research

Read before editing:

1. `changes/active/wyrd-ui-foundation/spec.md` revision 3.
2. `changes/active/wyrd-ui-foundation/tasks/TASK-001-route-mapped-svg-mocks.md`.
3. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R1-remediate-product-mock-composition.md`.
4. `changes/active/wyrd-ui-foundation/tasks/remediation/TASK-001-R2-remediate-card-workflows-and-service-composition.md`.
5. `changes/active/wyrd-ui-foundation/reviews/TASK-001-visual-acceptance.md`.
6. `architecture/wyrd-design.md`, especially Card independence, Experiment,
   Run, relationships, observations, and Vala correlation.
7. `architecture/wyrd-doctrine.mdx`.
8. `architecture/bifrost-design.md`, especially observation/query projection.
9. `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`.
10. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/workbench.html`.
11. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/styleguide.html`.
12. Current `C-06-light` and `C-06-dark` in
    `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg`.

Use the following OpsML files as interaction research only. Do not copy OpsML
routes, vocabulary, visual identity, Python-centric assumptions, or defects:

- Experiment navigation:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/layouts/ExperimentCardLayout.svelte`
- Overview and metadata:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/experiment/ExperimentPage.svelte`
  and `Metadata.svelte`
- Metrics and comparison:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/[registry]/card/[space]/[name]/[version]/metrics/+page.svelte`
  and
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/experiment/MetricComparisonTable.svelte`
- Files:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/files/FileExplorer.svelte`
- Figures:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/[registry]/card/[space]/[name]/[version]/figures/+page.svelte`
- Hardware:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/[registry]/card/[space]/[name]/[version]/hardware/+page.svelte`
- Versions:
  `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/VersionPage.svelte`

Useful external interaction references:

- MLflow Tracking and run comparison:
  `https://mlflow.org/docs/latest/ml/tracking`
- MLflow agentic traces and evaluation:
  `https://mlflow.org/docs/latest/genai/tracing/observe-with-traces/ui`
- Weights & Biases project and run workspaces:
  `https://docs.wandb.ai/models/track/project-page`
  and `https://docs.wandb.ai/models/track/workspaces`
- Weights & Biases Tables and Artifacts:
  `https://docs.wandb.ai/models/tables`
  and `https://docs.wandb.ai/models/artifacts`

Adopt the common workflow—run inventory, selection, inspection, comparison,
and typed outputs. Do not reproduce W&B's configurable dashboard builder or
MLflow/W&B product vocabulary.

# Ownership and write scope

Own only:

- a new directory:
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/experiment/`;
- `C-06-light` and `C-06-dark` inside
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg`;
- `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md`; and
- the product-mock links/count description in
  `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/index.html` when present.

Do not move or rewrite the remaining Card artboards in this task. `cards.svg`
remains the general Card contact sheet; C-06 becomes the concise Experiment
entry/summary and points into the detailed Experiment render package. Future
Card kinds may earn sibling directories under `product/cards/` when their own
detail work actually requires them.

Preserve every existing artboard ID and all approved
`golden-{CR-04,O-09,O-04}.svg` files exactly.

# Non-goals

- No Svelte, BFF, Rust, server, Vala, Bifrost, database, object-store, SDK, or
  production contract implementation.
- No edits to `wyrd-spec`, architecture, migrations, or generated schemas.
- No start, stop, cancel, retry, delete, mutate, or execute controls. The mock
  phase visualizes lifecycle; it does not invent execution authority.
- No new Card kind, run Card, general workflow engine, run registry, dashboard
  builder, saved-view system, report builder, visualization DSL, or plugin
  system.
- No kind-specific production route tree. Detailed renders remain states of
  `/t/acme/cards/card_experiment_01` expressed with query parameters.
- No assumption that every file is an Artifact Card or every chart is a saved
  figure.
- No claim that any mock run or output contract already exists in production.
- No application tests, lints, repository gate, or broad verification command.

# Locked product model for the mocks

## Identity and ownership

- The Experiment Card is an immutable, independently versioned grouping and
  comparison definition.
- A run is operational state associated with an exact Experiment Card UID and
  version. It is not a Card.
- Changing run state or receiving a new output must not visually imply that the
  Experiment Card version changed.
- Every displayed output identifies its producing run.
- Registered Artifact Cards keep their own UID, version, status, and Card
  route. Arbitrary run files do not acquire that identity.
- The browser and SVG fixtures are projections only. The server owns lifecycle,
  derived summaries, ordering, metric directionality, and authorization.

## Information architecture

The Experiment-local primary subnavigation is exactly:

1. `Overview`
2. `Runs`
3. `Compare`
4. `Outputs`
5. `Versions`

The shared Card shell remains visible and owns Experiment identity, Card
version, status, and common Card navigation. The Experiment workspace below it
owns analysis navigation.

Selected-run inspection uses these contextual destinations when applicable:

- `Summary`
- `Metrics`
- `Tables`
- `Visuals`
- `Files`
- `Artifacts`
- `System`
- `Provenance`
- `Traces` for an agentic/workflow run
- `Evaluations` for an agentic/workflow run with Eval results

Do not render disabled tabs for unsupported data. A model-training run does not
need empty Traces navigation, and an agentic run without hardware samples does
not need a decorative System page.

## URL-state contract for mocks

Use the shared route and visible query state:

```text
/t/acme/cards/card_experiment_01?view=overview
/t/acme/cards/card_experiment_01?view=runs&status=running&type=training
/t/acme/cards/card_experiment_01?view=runs&run=run_07&section=summary
/t/acme/cards/card_experiment_01?view=compare&runs=run_07,run_09&baseline=run_07
/t/acme/cards/card_experiment_01?view=outputs&type=metrics&run=run_07
/t/acme/cards/card_experiment_01?view=outputs&type=tables&run=run_07&table=validation-errors
/t/acme/cards/card_experiment_01?view=outputs&type=files&run=run_07&path=logs/train.log
/t/acme/cards/card_experiment_01?view=versions&version=v3
```

Every selected tab, filter, run, comparison set, output, file path, and version
shown in the renders must be visible in the route annotation. Do not design the
encoding as a durable API; it is the mock interaction contract to be reconciled
when the Svelte task is revised.

# Coherent fixture

Use one coherent mock Experiment unless a state explicitly represents an empty
variant:

- tenant: `acme`;
- Experiment Card UID: `card_experiment_01`;
- name: `checkout-ranking-study`;
- Card version: `v4`;
- Card status: `ACTIVE` with a status glyph;
- purpose: improve ranking quality while measuring latency and agent-assisted
  review behavior;
- targets: Model `ranker`, Agent `checkout-agent`, Service `checkout-api`;
- inputs: Data `txns-2026q3`, Prompt `capture-review`;
- evaluation: Eval `quality`;
- 12 runs with a visible mixture of queued, running, completed, failed, and
  cancelled state;
- `run_07`: completed `training`, the baseline/best compatible model run;
- `run_09`: completed `training`, compatible comparison candidate;
- `run_10`: failed `training` with retained partial metrics and logs;
- `run_11`: running `workflow` agentic run with partial trace/tool/Eval data;
- `run_12`: cancelled `offline_eval` run with explicit cancellation context.

Use `RunKind` vocabulary where technical kind is shown: `training`,
`inference`, `offline_eval`, `drift_check`, `import`, `workflow`,
`remediation`, or a labeled external kind. User-facing copy may explain the
kind in plain language.

Comparison uses only `run_07` and `run_09`. Do not compare the agentic workflow
run against training runs.

# Locked visual and interaction grammar

- Follow `brand/DESIGN.md` and the calmer workbench treatment exactly.
- Use one dominant analytical work region per artboard, with subordinate
  context and progressive disclosure.
- Do not repeat the same KPI, state, or explanation in multiple equal panels.
- Use dense tables only where scanning and comparison are the actual task.
- Preserve Wyrd's 5px radius, border, hard-shadow, typography, token, and
  light/dark rules. No gradients, blur, glass, glow, decorative chart noise, or
  hardcoded colors.
- Status uses text plus a glyph. Multi-series charts use line/marker patterns
  as well as color. Thresholds and objectives use positional markers when
  applicable.
- Charts require title, measure/unit, domain or axes, legend when needed,
  producing run, and enough plotted detail to be meaningfully reviewed.
- Lead with names and meaning. UIDs, hashes, raw payloads, provider IDs, and
  storage paths are secondary inspection detail.
- No collision, clipping, bleed, accidental empty lower half, illegible dense
  text, or uncontrolled page-width overflow.

# Required desktop artboards

Create paired 1440 x 1024 light/dark artboards with the following stable IDs.

## E-01 — Overview, active Experiment

Question: **What is this Experiment doing, and what needs attention?**

- Select `Overview`.
- Show purpose, exact Card version, linked targets/inputs, run counts by state,
  current best compatible run, objective metric, recent activity, and concise
  active/failure attention.
- Make current experiment progress/results dominant; keep raw definition and
  environment detail subordinate.
- Link the best run, running run, failed run, and linked Cards.

## E-02 — Overview, no runs

Question: **What exists before the first run arrives?**

- Use the same Experiment definition with zero run records.
- Explain that no runs or outputs have been recorded without inventing an
  execution button.
- Preserve targets, default parameters, and expected run/output context.

## E-03 — Runs inventory

Question: **Which runs should I inspect or compare?**

- Select `Runs`.
- Make a searchable/filterable run table dominant.
- Show selection, name/ID, kind, lifecycle glyph/text, start time, duration,
  initiator, source revision, important parameters, objective values, output
  counts, and linked target.
- Demonstrate URL-backed `status=running` and `type=training` filters plus the
  clear-filter behavior in the annotation ledger.
- Rows link to selected-run inspection; compatible selected runs enable the
  Compare destination.

## E-04 — Queued/running run with partial output

Question: **What is happening now, and what evidence already exists?**

- Select `run_11` and show current stage, elapsed time, lifecycle timeline,
  partial metrics, trace/tool progress, partial output counts, environment,
  and the absence of a final verdict.
- Never treat missing future outputs as zero or failure.
- Do not show cancel/retry controls.

## E-05 — Failed and cancelled run inspection

Question: **Where did execution stop, and what remains useful?**

- Make failed `run_10` the selected dominant record.
- Show failure stage, safe error summary, timing, retained partial metrics,
  logs/files, provenance, and followable related run/source links.
- Include cancelled `run_12` as a clearly separate selectable lifecycle row or
  inset; cancellation is not failure.

## E-06 — Completed model-training run

Question: **How did this model run perform and what produced it?**

- Select `run_07`, `Summary`.
- Show objective/best metric, parameters, linked Data/Model/Service Cards,
  execution environment, source revision, duration, output inventory, and
  compact training/validation history.
- Provide clear destinations to Metrics, Tables, Visuals, Files, Artifacts,
  System, and Provenance.

## E-07 — Completed agentic/workflow run

Question: **How did the agentic execution behave?**

- Present a completed projection of the `run_11` workflow fixture without
  changing its identity across lifecycle-state artboards.
- Show linked Agent, Prompt, Service, Workflow, and Eval Cards; sessions/traces;
  tool calls; latency; tokens; cost; errors; and evaluation summary.
- Provide Traces and Evaluations contextual navigation in addition to common
  outputs.

## E-08 — Compatible run comparison

Question: **Why is one compatible run better or different?**

- Select `Compare`, `run_07` and `run_09`, with `run_07` marked baseline.
- Compare parameter differences, scalar metrics, correctly step-aligned metric
  histories, environment, provenance, linked Cards, and output differences.
- Show neutral deltas when directionality is unknown. Only the declared
  objective may identify better/worse.
- Keep run selection/edit affordance visible without building a configurable
  dashboard.

## E-09 — Metrics output

Question: **How did measured values change through this run?**

- Select `Outputs > Metrics`, `run_07`.
- Show metric selector, scalar summary, training/validation history, step/time
  domain, exact values/table escape hatch, legend patterns, and objective
  marker where applicable.
- Do not align series by array index or replace missing values with zero.

## E-10 — Table-metric output

Question: **Which structured examples explain the aggregate result?**

- Select `Outputs > Tables`, table `validation-errors`, `run_07`.
- Show table identity/schema, producing run, row count, search/filter,
  pagination, sortable columns, typed values, and a selected-row detail.
- Demonstrate useful domain content such as label, prediction, score, error
  category, and source record—not a generic key/value table.

## E-11 — Visual output

Question: **What do the saved rich outputs reveal?**

- Select `Outputs > Visuals`, `run_07`.
- Show a labeled gallery containing at least a learning curve, confusion
  matrix, and one appropriate rich output.
- Select one visual for focused inspection with title, caption, producing run,
  media type, dimensions/size, created time, and download/open affordance.
- Distinguish uploaded visual output from a UI-derived metric chart.

## E-12 — Files output

Question: **Which raw run files can I inspect?**

- Select `Outputs > Files`, path `logs/train.log`, `run_07`.
- Use a lazy tree/list and substantial preview region.
- Show path, type, size, digest or immutable identity when available, producing
  run, preview/download behavior, and an explicit unsupported/too-large state.
- Never render unsanitized HTML or silently make an unsupported file inert.

## E-13 — Registered Artifact output and lineage

Question: **Which durable outputs were promoted to Artifact Cards?**

- Select `Outputs > Artifacts`, `run_07`.
- Show populated registered Artifact Cards with name, kind/type, version,
  status, producer run, linked Model/Service, and direct Card route.
- Show concise input-to-run-to-artifact lineage.
- Include the absent state language for runs with files but no registered
  Artifact Cards. Do not call those files artifacts.

## E-14 — System measurements

Question: **Did the execution environment constrain this run?**

- Select `run_07`, `System`.
- Show CPU, memory, accelerator when applicable, and network or disk only when
  useful; include time range, units, sampling context, and run stage overlay.
- Prefer a few decision-useful aligned charts over OpsML's fixed four-chart
  template.

## E-15 — Provenance and reproducibility

Question: **Can I identify and reproduce what produced this run?**

- Select `run_07`, `Provenance`.
- Show initiator, source provider/repository/commit, dirty-state indicator,
  runtime/language versions, container image digest, dependency snapshot or
  lockfile descriptor, command/entry point, parameters, input Card versions,
  and output lineage.
- Secrets and unrestricted environment variables never appear.

## E-16 — Agentic trace inspection

Question: **Where did the agentic run spend time or fail?**

- Select the agentic run, `Traces`.
- Use a trace inventory plus selected execution tree/waterfall and focused span
  detail.
- Distinguish agent, LLM, tool, retrieval, and error spans with text/kind as
  well as color.
- Show inputs/outputs progressively, tool calls, token/cost/latency, errors,
  and direct link to canonical Observe trace detail.

## E-17 — Agentic evaluation inspection

Question: **How was this run evaluated and which examples failed?**

- Select the agentic run, `Evaluations`.
- Show linked Eval Card/workflow, task summaries, aggregate score/verdict
  projection, and selected failed example/task result.
- Keep Eval definition on the linked Card and canonical runtime exploration in
  Observe; link to both.

## E-18 — Experiment Card versions

Question: **How did the Experiment definition change?**

- Select `Versions`.
- Compare Experiment Card versions, not run lifecycle.
- Show version, status, created time/author, description of definition change,
  target/default-parameter changes, and direct version selection.
- Explicitly state that run transitions do not create Card versions.

# Required narrow-width artboards

Create paired 390 x 844 light/dark artboards.

## EM-01 — Run inventory and filters

- Preserve Experiment identity, primary subnavigation, active filters, lifecycle
  glyph/text, run kind, objective value, and selection.
- Use an ordered list or horizontally scrollable table; do not hide columns
  without making their detail reachable.

## EM-02 — Selected run inspection

- Show the selected running or completed run as a full-width detail flow with a
  visible return to Runs.
- Preserve lifecycle, parameters, links, output destinations, and partial/final
  distinction.

## EM-03 — Output inspection

- Demonstrate one table or file output with explicit return to Outputs,
  producing run, horizontal scrolling where irreducible, and no page overflow.

# Render package and ledger

Create the minimum SVG files that keep each contact sheet reviewable. The
recommended split is:

```text
product/cards/experiment/
|- overview.svg     # E-01, E-02
|- runs.svg         # E-03 through E-07
|- compare.svg      # E-08
|- outputs.svg      # E-09 through E-17
|- versions.svg     # E-18
`- mobile.svg       # EM-01 through EM-03
```

The implementer may split `outputs.svg` once more if it becomes materially
hard to render or review. Do not invent a build framework or split one artboard
per file.

Every page appears in identical light/dark geometry:

- detailed Experiment package: 21 pages / 42 artboards;
- existing product set remains 43 pages / 86 artboards;
- complete product set after this task: 64 pages / 128 artboards.

Update `product/README.md` with:

- links and counts for every nested Experiment sheet;
- the complete E-01 through E-18 and EM-01 through EM-03 route/state ledger;
- a clear distinction between general C-06 and the detailed Experiment
  package;
- URL parameter and responsive mappings;
- fixture identities and output semantics; and
- a deferred production-gap ledger matching this task.

# Deferred production contract gaps — record, do not implement

The mocks intentionally get ahead of current production behavior. Record every
item below in the render ledger and implementation handoff. None is a reason to
stop or reduce the static design:

1. `ExperimentSpec` currently models only type, description, target refs,
   defaults, `run_refs`, summary metrics, best-run ref, Artifact refs, and free
   details. It does not define the full workspace.
2. `RunRef` currently exposes only UID, `RunKind`, optional space, and labels.
   It has no lifecycle, timestamps, initiator, source, parameters, environment,
   outputs, or failure data.
3. Wyrd architecture says there is no run registry and does not yet define the
   server-owned Experiment run projection needed by this UI.
4. `best_run_ref` and summary-metric ownership are not settled as authored
   declaration versus server-derived projection.
5. No typed Experiment query service, DTO, HTTP/gRPC route, or Bifrost table
   currently supplies this workspace.
6. The typed Metrics query currently omits run/Card correlation from its
   filters and response even though physical observation data can carry it.
7. `experiment_id` correlation is declared in one place but is not wired into
   observation system columns, ingest, or query.
8. Files and heavy Card manifests are not associated with individual run
   records through a standard contract.
9. Registered Artifact Cards, arbitrary files, table outputs, figures, and
   derived UI charts lack explicit run association and role semantics.
10. No typed visualization descriptor defines chart/figure metadata,
    directionality, compatible comparison, or preview safety.
11. Model and agentic data can be inferred from existing metric, trace, GenAI,
    Eval, log, and observation domains, but no Experiment projection joins them
    today.
12. Run execution controls and their authorization/audit contract are
    intentionally absent and remain future work.

Do not hide these gaps by putting invented fields into a Card Spec panel. Mock
runtime values must be labeled as server/Vala/Bifrost projections or mock
fixtures, distinct from the Experiment Card declaration.

# Ordered visual scenarios

Execute one scenario at a time:

1. Establish the dedicated Experiment render package and update C-06 into a
   concise entry point without disturbing other Card artboards.
2. Design Overview and the first-run empty state.
3. Design the run inventory and restorable filters.
4. Design queued/running, failed, cancelled, completed model, and completed
   agentic inspection.
5. Design compatible run comparison with truthful alignment and directionality.
6. Design Metrics, Tables, Visuals, Files, and registered Artifacts as distinct
   run-associated outputs.
7. Design System and Provenance inspection.
8. Design agentic Trace and Evaluation drilldowns with canonical Observe links.
9. Design Experiment Card version history distinct from run history.
10. Prove run and output navigation at narrow width.
11. Reconcile every sheet, route, fixture, link, state, theme, count, and
    deferred-gap ledger entry.

# Visual RED, GREEN, REFACTOR

For each scenario:

- **RED:** identify the current C-06 limitation or missing detailed artboard and
  name the finding it demonstrates.
- **GREEN:** add the smallest render and ledger change that visibly satisfies
  the scenario with the existing Wyrd visual grammar.
- **REFACTOR:** reuse shell, navigation, run-row, chart, table, drawer, and
  output anatomy only where the user performs the same work. Do not flatten
  different outputs or model/agentic workflows into a universal dashboard.

# Verification and evidence

This is static visual-contract work. Do not run application tests, linters,
formatters, code generation, or the repository gate.

Required evidence:

- direct rendered inspection of C-06 and E-01 through E-18 in both modes;
- direct rendered inspection of EM-01 through EM-03 in both modes;
- matching light/dark geometry, content, selected state, and route annotations;
- comparison charts align by declared step/time domain;
- metric status/deltas do not rely on color or invent directionality;
- every table, visual, file, and Artifact identifies its producing run;
- every registered Artifact retains Card identity and direct navigation;
- no clipping, collision, bleed, illegible text, accidental whitespace void,
  or uncontrolled overflow at declared scale;
- all existing artboard IDs and golden files remain unchanged except the owned
  content inside C-06;
- nested-sheet and complete-set counts equal 21/42 and 64/128 respectively;
- README walk proves every subnavigation destination, URL state, lifecycle,
  fixture, link, responsive behavior, and deferred production gap; and
- explicit human acceptance of the Experiment mock package.

# Completion and stop conditions

Complete only when the entire detailed Experiment package, revised C-06,
ledger, counts, and direct visual evidence are present and the user explicitly
accepts them.

Stop and return to `$wyrd-spec` only if the mock design requires changing the
approved Experiment workspace behavior, Card/run/output identity rules, route
constraint, or acceptance obligations in revision 3.

Do not stop because current Wyrd server or contract behavior is missing. Record
that gap exactly and continue with mock design. Do stop before implementing or
silently resolving any deferred server, `wyrd-spec`, Vala, Bifrost, storage,
ingest, query, lifecycle, authorization, or execution decision.

After human mock acceptance, reconcile the later Svelte Card-workspace task to
specification revision 3 and create separate specification work for the
deferred production Experiment contracts.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`

# Execution evidence — 2026-09-04

Static visual-contract work; per this task no application tests, linters,
formatters, codegen, or gate were run. All verification is direct rendered
inspection (headless Chrome screenshots of the SVGs, both modes).

## Delivered

- New `product/cards/experiment/` package: `overview.svg` (E-01, E-02),
  `runs.svg` (E-03…E-07), `compare.svg` (E-08), `outputs.svg` (E-09…E-17),
  `versions.svg` (E-18), `mobile.svg` (EM-01…EM-03) — 21 pages / 42 artboards,
  paired light/dark, desktop 1440 × 1024, mobile 390 × 844, same sheet
  grammar (id groups, title/desc, clip, frame, dashed annotation strips) as
  the existing set.
- `cards.svg` C-06-light/C-06-dark replaced with the concise Experiment
  entry/summary (lifecycle counts, server-derived best-run objective,
  running/attention links, five URL-restorable destinations, latest-runs
  excerpt, relationships, earned-absent notebooks); strip texts updated. All
  other artboard IDs byte-preserved: 28 artboards / 28 unique before and
  after; `golden-*.svg` untouched.
- `product/README.md`: sheet table with nested-sheet links and counts
  (21/42 package, 43/86 existing, 64/128 complete), rewritten C-06 entry with
  the entry-point vs detailed-package distinction, full E-01…E-18 +
  EM-01…EM-03 route/param/state/link ledger, R-EXP responsive pattern +
  EM realizations, Experiment fixture and output-semantics section, and the
  12-item deferred production-gap ledger from this packet.
- `renders/index.html`: Experiment-workspace nav card added; Cards and Ledger
  card texts updated with the new counts.

## Visual scenario cycles (RED → GREEN → correction)

1. Package + C-06 entry — RED: single generic C-06, no workspace. GREEN:
   6 nested sheets generated; C-06 spliced (splice is idempotent; a first
   splice dropped the C-06-dark label text — fixed by reinserting it).
2. Overview + empty state — E-01/E-02. Correction: recent-activity row
   height 38→34 (last row collided with the "All runs" link).
3. Run inventory + filters — E-03 with an honest 0-match filter demo
   (running run_11 is a workflow, not training). Corrections: NDCG@10/OUT
   column anchors widened (LR value collided with end-anchored NDCG@10);
   row height 34→33 (12th row collided with the footnote).
4. Run inspection — E-04 (running + queued inset), E-05 (failed + cancelled),
   E-06 (completed model), E-07 (completed agentic, labeled design projection
   of the run_11 fixture). Corrections: E-04 gained a recorded lifecycle-events
   panel (≈226 px whitespace void); E-05/E-07/E-14 absent-inset texts split to
   two lines (rail-width overflow caught by the text-width audit).
5. Compare — E-08, baseline chips, step-aligned histories, objective-only
   direction. Correction: Δ column moved 520→496 and "no direction"
   shortened to "neutral" (text clipped at the panel edge).
6. Outputs — E-09 (metric gap never zero-filled), E-10 (typed table,
   server sort, pagination), E-11 (stored visuals vs UI-derived charts),
   E-12 (lazy tree, sanitized preview, explicit no-preview), E-13 (registered
   Artifacts keep Card identity; files never called artifacts).
7. System + Provenance — E-14 (three decision-useful charts, stage bands,
   positional peak marker), E-15 (full reproducibility record, no secrets).
8. Agentic drilldowns — E-16 (span kinds by label + bar + position),
   E-17 (Eval definition/Observe linked, not duplicated).
9. Versions — E-18 (v1–v4 immutable; runs never create versions).
10. Narrow width — EM-01…EM-03 (scrollable subnav, back affordances,
    contained horizontal scroll, no page overflow).
11. Reconciliation — generator overflow audit: 0 findings; artboard counts
    42/86/128 verified by id extraction; README walk covers every
    destination, URL state, lifecycle, fixture, link, responsive behavior
    and deferred gap.

## Bounded scope expansion

`cards.svg` C-04 (Model Card) referenced the producing experiment by the
stale name `exp-rank-12` (4 occurrences, light + dark). Renamed to
`checkout-ranking-study` for fixture consistency with this packet's locked
identity; both text slots verified to fit (left line ends ≈958 < 1016 panel
edge; rail line ends ≈1298 < 1402).

## Verification results

- Rendered inspection of C-06, E-01…E-18, EM-01…EM-03 in both modes;
  light/dark geometry, content, selection and annotations match.
- Comparison charts align by recorded step; the shorter run_09 series ends
  at step 1,400; only ndcg@10 claims better/worse.
- Every table, visual, file and Artifact identifies its producing run;
  registered Artifacts keep uid/version/status and direct Card routes.
- No clipping, collision, bleed, void, or uncontrolled overflow remains at
  declared scale after the corrections above.
- Counts: experiment package 21/42; existing set 43/86 (all IDs unique and
  preserved); complete set 64/128. `golden-*.svg` unchanged.
- `git diff --check` clean; the entire `renders/` tree is untracked new work
  on `change/surfaces-oracle-integration` (no tracked-file drift).

## Deferred gaps

All 12 production contract gaps from this packet are recorded verbatim in
`product/README.md` § "Deferred production contract gaps — Experiment
workspace"; gaps #8 and #9 are additionally surfaced on E-15 and E-13. None
was implemented or silently resolved.

## Status

COMPLETE pending explicit human acceptance of the Experiment mock package
(required by this task's completion conditions). No commit created.

## Self-review pass — 2026-09-04 (post-evidence)

A second full review before hand-off found and fixed seven defects the first
pass missed (it had relied on reduced-scale light-column screenshots and a
generator audit that covered only `absent()` bodies):

1. E-05 — "Open files → ?view=…" link clipped the RETAINED FILES & LOGS panel
   edge; split into a short link plus a query line.
2. E-07 — "1 failed (groundedness)" overflowed the EVALUATION SUMMARY panel;
   split to two lines.
3. E-07 — the OUTPUT INVENTORY trailing note overflowed the panel into the
   rail; moved to its own full-width line.
4. E-04 — "Follow live traces" link clipped 21px; shortened to the
   `&section=traces` short form used elsewhere.
5. E-15 — the third lineage link (holdout-report v3) clipped 40px; moved to a
   second row (panel 122→144); also the PARAMETERS & INPUT CARDS head label
   collided with its right subtitle — subtitle shortened.
6. EM-01…EM-03 — annotation-strip lines were authored at desktop width and
   overflowed the 828px mobile strip by up to 637px; rewrapped
   (mobile.svg height 3144→3324).
7. E-16 — the span-detail head label collided with its subtitle (shortened to
   "SPAN SP_09 — TOOL RETRY"), the EM-03 rows head likewise ("scrolls ‹ ›"),
   and the agent row's full-width waterfall bar overlapped its own duration
   label (bar scale reduced).

Two systematic scans were added and re-run to zero real findings across all
42 experiment artboards plus C-06 (both modes): (a) panel-aware text-edge
overflow, (b) same-baseline start/end anchored pair collision. Remaining
flags are estimator false positives verified fitting in renders (E-08 sans
caption, E-08 chip-internal text). Also visually verified this pass: E-02,
E-05, E-07 at full scale, C-04's renamed producing-experiment references,
E-06 dark, and the rewrapped mobile sheet. Counts unchanged: 42 experiment
artboards, 28 in cards.svg, goldens untouched, `git diff --check` clean.

## Remediation evidence — 2026-09-04 (findings FIND-TASK-001-R3-DS-01…07)

All seven reviewer findings implemented as bounded generator edits; the
strong areas (run inspection, typed tables, files, provenance, trace/eval
drilldowns, artifact identity, versions) were left untouched. All sheets
regenerated (21 pages / 42 artboards, overflow audit 0) and re-verified.

- **DS-01 (HIGH, E-08)** — comparison validity is now explainable. The
  comparison table gained explicit Card-identity rows: declaration version
  (v4/v4), eval population (holdout · cutoff 0.42), and source/image/input
  Data rendered as linked Card rows. A full-width COMPATIBILITY BASIS panel
  states the invariants held by run_07/run_09 (declaration v4 · objective
  ndcg@10 on holdout · kind training · input Data · target · completed with
  the objective recorded) and names each exclusion with its reason: run_11
  (workflow kind, no ndcg@10 on the declared holdout), run_05/run_10
  (failed; no final objective recorded). The chip strip states the listing
  rule ("only runs meeting the compatibility basis below are listed").
- **DS-02 (HIGH, E-03/EM-01)** — active filters now produce a true
  zero-result state. Desktop: the filter demo moved into a "URL FILTER
  STATE — INSET" panel whose chips render a dashed 0-of-12 empty box that
  explicitly replaces the inventory, with Clear filters restore; the main
  table is the unfiltered `?view=runs` inventory and never sits under
  active chips. Mobile: same contract as a dashed STATE INSET below the
  list (following the M-01 inset precedent), zero-result box + clear link.
- **DS-03 (MOD, E-03)** — inventory is sorted by the declared objective
  (ndcg@10 ▾ header + panel subtitle; runs without it sort last by
  recency, stated in the footnote) and a single "＋ add filter ▾"
  affordance names the richer vocabulary: objective ≥/≤ · parameter ·
  source revision · data version · initiator. EM-01 mirrors the ordering
  and the affordance.
- **DS-04 (MOD, E-08)** — the aligned-history panel gained a restrained
  metric selector (ndcg@10 ✓ · logloss · val gap · p95_infer_ms · "any
  recorded history · &metric={name}"). Step alignment and objective-only
  directionality preserved (caption: run_09 ends at step 1,400; only
  ndcg@10 carries direction).
- **DS-05 (MOD, E-09)** — exact-values table now contains five visible
  rows (steps 1,200–2,000, rowh 25, panel h 196) inside the panel border
  in both themes; the 1,800 row flags "0.742 · best".
- **DS-06 (MOD, E-11)** — each gallery item draws a distinct
  representative thumbnail: curve, 2×2 matrix, dashed-diagonal
  calibration, horizontal bars, histogram.
- **DS-07 (MINOR, E-14)** — the memory peak label moved below the
  threshold line, clear of the eval stage band; verified at 1:1 in both
  themes per the finding.

Verification: regenerated cleanly; both systematic scans (panel-edge
overflow, same-baseline pair collision) report zero findings across all 42
artboards + C-06 in both modes. Screenshots reviewed at 1:1-equivalent for
E-03, E-08, E-09, E-11, E-14, EM-01 light, and E-03/E-08/E-14/EM-01 dark.
Two defects found and fixed during this pass's own review: the E-03 filter
vocabulary line overflowed its panel (trimmed) and the EM-01 selection-bar
caption grazed the Compare button (shortened to "full columns in run
detail", re-shot clean). README ledger entries for E-03, E-08, E-09, E-11,
EM-01 updated to match; deferred-gap ledger unchanged (the new affordances
map onto existing gaps 5–6). Counts unchanged: 21/42 experiment, 43/86
existing, 64/128 total; goldens untouched; `git diff --check` clean.

## Fixture override — 2026-09-04 (human direction: no declared objective)

Steven overrode the fixture's "objective ndcg@10 maximize" premise: an
Experiment declares no objective, runs record metrics, and data scientists
explore and rank across multiple metrics. Redrawn to a multi-metric shape
grounded in the actual `ExperimentSpec` (summary_metrics + authored
best_run_ref; no objective field):

- **E-01 / C-06** — "CURRENT RESULT" (server-derived) became "AUTHORED
  SUMMARY": spec `summary_metrics` (ndcg@10 0.742, auc, logloss) plus the
  author-pinned `best_run_ref` run_07, labeled declaration content, never a
  derived winner. The definition rail now lists summary_metrics and
  best_run_ref as authored fields.
- **E-03 / EM-01** — the inventory is a viewer-sorted leaderboard:
  `?view=runs&sort=ndcg_10&dir=desc` carried in the URL, ndcg@10 + logloss
  metric columns with a metrics ▾ column chooser, footnote stating the sort
  is viewer state and never Card truth; filter vocabulary now `metric ≥/≤`.
- **E-06** — "✓ BEST FOR NDCG@10" became "★ PINNED BEST" with the rail note
  "best_run_ref is authored declaration content — not a derived winner".
- **E-08** — direction comes from a URL lens (`&rank=ndcg_10&dir=max`): the
  ndcg row is "ndcg@10 · lens ↑", better/worse renders only under that lens,
  all other deltas neutral; the compatibility basis dropped the objective
  invariant (now declaration v4 · kind training · eval population · input
  Data · target · completed with recorded metrics) and exclusions were
  reworded (run_11 — no step-aligned training metrics).
- **E-09** — "objective, maximize" removed; the marker/kv/exact-values say
  "max recorded 0.742" and "0.742 · max"; the selector row reads
  "history · summary".
- **E-18** — v2 change summary now "summary → ndcg@10".

Verification: regenerated clean (21 pages / 42 unique artboards, overflow 0);
edge + pair scans across all experiment sheets and cards.svg report zero
findings; screenshots reviewed for E-01, E-03, E-06, E-08, E-09, C-06,
EM-01 light and E-01/E-03 dark. The definition rail grew 168→190 to hold
its two new rows. README updated (fixture intro, E-01/E-03/E-06/E-08/E-09/
EM-01/C-06 entries, output-semantics rule, deferred gap 4 now records that
the mocks draw summary_metrics/best_run_ref as authored and keep ranking a
URL-carried viewer act). No "objective" language remains in any rendered
SVG, index.html, or the ledger. Counts unchanged; goldens untouched;
`git diff --check` clean. Human acceptance still pending; nothing committed.
