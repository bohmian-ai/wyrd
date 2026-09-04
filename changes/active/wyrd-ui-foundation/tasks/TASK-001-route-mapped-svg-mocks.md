---
id: TASK-001
title: Route-mapped SVG product mocks
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-014, REQ-015, REQ-016, REQ-017, REQ-018, REQ-019, REQ-060, REQ-061, REQ-062, REQ-063, REQ-064, REQ-065, REQ-066, REQ-067, REQ-068, REQ-070, REQ-071, REQ-072, REQ-073, REQ-074, REQ-075, REQ-076, REQ-077, REQ-078, REQ-079, REQ-080, REQ-081, REQ-082, REQ-083, REQ-084, REQ-085, REQ-086, REQ-087, REQ-088, REQ-089, REQ-090, REQ-091, REQ-092, REQ-094, REQ-095, REQ-096, REQ-097, REQ-098, REQ-099, INV-008, INV-012, INV-014, AC-003, AC-004, AC-005, AC-007]
depends_on: []
parent_task:
remediates: []
---

# Outcome

Produce the complete reviewable visual contract before Svelte product-view
implementation. A fresh agent must be able to create every mock from this task
and the named authorities without conversation history or new layout decisions.

# Deliverables

Own `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/` and create:

- `home.svg`: two desktop pages, light and dark.
- `cards.svg`: twelve desktop pages, light and dark.
- `observe.svg`: ten desktop pages, light and dark.
- `changes.svg`: seven desktop pages, light and dark.
- `query.svg`: one desktop page, light and dark.
- `mobile.svg`: eight mobile pages, light and dark.
- `README.md`: artboard/route/state/fixture/link/responsive ledger.
- Link the six sheets from `brand/renders/index.html`.

Required total: 32 desktop pages × two themes plus eight mobile pages × two
themes = 80 labeled artboards in six directly viewable SVG contact sheets.

# Authority and references

Resolve conflicts in this order:

1. `changes/active/wyrd-ui-foundation/spec.md` fixes product behavior, routes,
   vocabulary, states, and boundaries.
2. `crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`, `palette.json`, and
   `theme.css` fix tokens, contrast, 5px geometry, and theme behavior.
3. `crates/wyrd/wyrd-server/wyrd-ui/brand/renders/styleguide.html` and
   `workbench.html` fix the starting shell, density, controls, tables, panels,
   charts, and workbench composition.
4. The page register below fixes each artboard's required composition/content.
   The agent may adjust spacing and chart geometry, not product structure.
redacted
   research only. They are not Wyrd visual, route, vocabulary, or contract
   authority and must not be copied.

The Change Request record, Changes routes, shell, and general presentation are
Wyrd-owned surfaces. A Verifier result identifies its actual provider. Use the
`FATHOM` product pill and Fathom treatment only on a panel or judgment actually
produced by Fathom; Fathom does not relabel or visually own the whole Changes
workspace, and a generic or external Verifier is not styled as Fathom.

Also read `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/wyrd-design.md`, and `architecture/wyrd-doctrine.mdx`.

Research-only references:

- Current visual-only Change Request/Verifier direction:
  `changes/active/verified-change-contract/spec.md`. It remains a draft durable
  protocol, but its already-approved product distinctions and vocabulary must
  be reflected without inventing field-level contracts.
- Prior Change Request plan:
  `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/plan.md`
- Prior contract task:
  `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/tasks/01-change-contract-foundation.md`
- Prior control-plane task:
  `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/tasks/02-change-control-plane.md`
- Prior test-Evidence/Bifrost task:
  `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/tasks/03-test-evidence-bifrost.md`
- OpsML Card layouts: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/[registry]/card/[space]/[name]/[version]/`
- OpsML Experiment: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/experiment/`
- OpsML Eval: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/card/agent/evaluation/`
- OpsML Eval route/layout: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/agent/[registry]/card/[space]/[name]/[version]/evaluation/`
- OpsML traces: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/lib/components/trace/`
- OpsML Drift: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/[registry]/card/[space]/[name]/[version]/monitoring/`
- OpsML observability entry: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/observability/`
- OpsML agent observability/dashboard: `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/agent/[registry]/card/[space]/[name]/[version]/observability/` and `/Users/stevenforrester/Documents/GitHub/opsml/crates/opsml_server/opsml_ui/src/routes/opsml/agent/[registry]/card/[space]/[name]/[version]/dashboard/`
redacted
redacted
redacted
redacted
redacted
- Earlier Change Request concept: `/Users/stevenforrester/Documents/GitHub/agent-workflows/wyrd/active/verified-changes-trust-layer-v3-20260825/partner-mockup.html`

Additional visual-authority detail lives in
`crates/wyrd/wyrd-server/wyrd-ui/brand/components.json`,
`crates/wyrd/wyrd-server/wyrd-ui/brand/brand-skill.md`, and
`crates/wyrd/wyrd-server/wyrd-ui/brand/renders/landing.html`. Use these to
resolve component anatomy and product-layer treatment, not to add landing-page
styling to the workbench.

Extract only these lessons from research: a shared Card shell with earned
kind-specific content; Experiment metric/parameter/figure/hardware hierarchy;
Eval event → workflow → selected-task inspection; Drift definition beside
calculated reports; URL-first service/correlation filters; observability search
→ trend → records → selected detail; trace waterfall/graph/span detail; metric
discovery/label filtering/chart/values; and read-only dashboard flow.

# Shared visual contract

## Desktop frame

Every tenant-qualified desktop artboard uses the same approximately 1440px
authenticated shell:

- Persistent left sidebar with the Wyrd mark and exactly five primary links:
  Home, Cards, Observe, Changes, Query. Highlight only the current area.
- Top utility bar with visible current tenant, theme control, and principal
  menu. Tenant is static for a single-tenant principal. Annotate one
  multi-tenant variant with a searchable switcher. Space is a page filter,
  never another global switcher.
- Content header with useful breadcrumbs, title, plain-language description,
  earned primary action, and secondary actions.
- Page filters below the header. Every URL filter is also a visible removable
  chip; `View all` clears inherited scope.
- Dense, quiet workbench composition from `workbench.html`: 5px geometry, hard
  shadows, flat colors, restrained accents, no glow or CRT treatment.
- Contextual detail rails only for a selected record/supporting definition.
- Status always has text and, where useful, glyph/pattern. Charts distinguish
  series by labels plus line style/marker/pattern, never color alone.

The `/` tenant entry is authenticated but has no tenant shell because tenant
context has not yet been established.

## Theme and annotation

Every light/dark pair has identical geometry, hierarchy, content, fixture data,
and state. Only theme tokens change. Every artboard includes a small
non-product annotation strip naming its ID, exact route/fixture, viewport,
theme, active URL parameters, material states, link destinations, and
responsive pattern. SVGs need not be interactive; later behavior must be
unambiguous.

## Exact SVG geometry and standalone behavior

- Desktop page artboards use a `1440 × 1024` coordinate space. Mobile page
  artboards use `390 × 844`.
- Every sheet pairs light on the left and dark on the right for the same page.
  Desktop sheets use 96px outer margins, a 64px column gap, and a 96px label
  band above each pair (`viewBox` width 3136). Mobile uses 64px outer margins,
  a 48px gap, and an 80px label band (`viewBox` width 956). Rows are separated
  by at least 64px; sheet height follows the number of rows.
- Give every artboard group a stable fragment ID such as `C-01-light` and
  `C-01-dark`. Product text is at least 12px in artboard coordinates and
  annotations at least 14px. Labels/actions must be readable when one artboard
  is zoomed to browser width.
- Every SVG renders from `file://` without the application, web server, network
  fonts, remote CSS/images, JavaScript, or `foreignObject`. Inline styles,
  token values, definitions, and required vector assets. Use a system-font
  fallback when the approved font is unavailable.
- Include SVG `<title>` and `<desc>` plus visible artboard, route, viewport, and
  theme labels. Meaningful UI copy remains SVG text, not rasterized text.

# Coherent fixtures

Use one fictional tenant, `acme`. Reuse the same Cards, services, principals,
Runs, traces, Eval records, Drift reports, and Change subjects across pages.
Do not use unrelated lorem ipsum.

The central Change Request fixture has three subjects across two repositories:
two exact same-repository transitions connected to two stacked PRs, plus one
exact transition/PR in a second repository. This demonstrates the approved
one-repository/multiple-PR and multiple-repository cases without using branch
names as identity. A separate CR-01 row demonstrates one repository/one PR.
All subjects show exact base/candidate commits. The central fixture also has at
least two plain-language Claims;
required/advisory Verifiers; manual and `on_new_evidence` modes; received and
missing Evidence; not-run/running/pass/fail/stale/carried-forward states; an
authorized override separate from verification; Product, Data Science, and
Engineering participants; and links to related Cards, telemetry, Eval, Drift,
commits, diffs, and providers. These are temporary visual fixtures, not durable
wire contracts.

# Desktop artboard register

Use these IDs in the sheets and ledger. Routes and query strings are canonical.

## `home.svg` — H-01 through H-02

### H-01 — tenant resolution — `/`

Show three outcomes in one composition: zero tenants gives access/provisioning;
one tenant redirects to `/t/acme`; multiple tenants show a minimal recent-first
chooser. Show no tenant data before resolution.

### H-02 — Home — `/t/acme`

Show Card lookup, `New Change Request`, items requiring the current user's
action, recent Changes, recently viewed Cards, recent work, and compact linked
Cards/Observe/Changes summaries. This is not a telemetry dashboard: no full
charts, custom panels, or Service inventory.

## `cards.svg` — C-01 through C-12

### C-01 — inventory — `/t/acme/cards?kind=Service&space=prod`

Show search; kind/status/owner/label/Space filters; removable `Service` and
`prod` chips; count; dense rows with kind/name/version/space/owner/status/time;
recent Cards; and compact empty/loading/error examples. The kind selector
explicitly shows the complete intended set for this approved change: `Data`,
`Model`, `Artifact`, `Experiment`, `Prompt`, `Agent`, `Workflow`, `Mcp`,
`Service`, `Policy`, `Audit`, `Drift`, `Eval`, `Source`, `Trigger`, `Operator`,
and `Verifier`. Follow the approved addition of Verifier as the seventeenth
kind; the older “16 kinds” architecture text is known follow-up drift, not
permission to omit it. That architecture wording must be corrected outside
this static mock task before Verifier Card implementation. Rows link to the
concrete shared fixture `/t/acme/cards/card_service_01`.

### C-02 — shared detail — `/t/acme/cards/card_generic_01`

Establish the reused shell: kind/name/version/status header, version selector,
metadata, labels, ownership, timestamps, relationships, generated status,
typed read-only Spec, and links to related Cards/filtered Observe pages. Common
navigation must not create kind-specific route trees.

### C-03 — Data — `/t/acme/cards/card_data_01`

Emphasize schema/field types, profile summary, distributions/statistics, splits,
targets, artifacts, and lineage. No data editor.

### C-04 — Model — `/t/acme/cards/card_model_01`

Emphasize task/interface, signature, framework metadata, artifacts, lineage,
deployment relationships, and linked verification/observation.

### C-05 — Prompt — `/t/acme/cards/card_prompt_01`

Emphasize message/content preview, variables and required/default state,
response schema, version, and linked Eval results. No secrets.

### C-06 — Experiment — `/t/acme/cards/card_experiment_01`

Use one page with run summary, parameters, metric comparison, figures,
hardware, and artifacts. Adapt useful OpsML hierarchy, not its routes or names.

### C-07 — Agent — `/t/acme/cards/card_agent_01`

Emphasize capabilities, prompt/model relationships, tools as runtime registry
relationships rather than Card kinds, interfaces, security, and filtered
Observe/Eval links.

### C-08 — Workflow — `/t/acme/cards/card_workflow_01`

Emphasize DAG/ordered stages, inputs, outputs, owners, and governance. This is a
declaration, not one Eval event's execution results.

### C-09 — Service — `/t/acme/cards/card_service_01`

Emphasize components, deployment targets, endpoints, policies, authenticated
principal, and `publishes_to`. Service remains an Observe filter, not hierarchy.

### C-10 — Eval — `/t/acme/cards/card_eval_01`

Show a reusable, subject-less Eval declaration: dataset/source, evaluation
workflow/tasks, pass gate, and linked Trigger context, plus a link to
`/t/acme/observe/evaluations?evalCard=card_eval_01`. Results stay in Observe.
Do not render `subject_ref` or an Eval-owned schedule. Runtime subject identity
comes from a Service/Agent observation; Trigger owns scheduling and filtering.

### C-11 — Drift — `/t/acme/cards/card_drift_01`

Show a reusable, subject-less Drift declaration: method, signal, condition,
baseline/features, thresholds, and linked Trigger/Operator context, plus a link to
`/t/acme/observe/drift?driftCard=card_drift_01`. Keep configuration, raw
observations, and calculated reports distinct. Do not render `subject_ref` or a
Drift-owned schedule. Observations supply runtime subject identity, Trigger
owns wiring/scheduling, and Operator owns reactions.

### C-12 — Verifier — `/t/acme/cards/card_verifier_01`

Show versioned identity, purpose, capabilities, accepted normalized Evidence
kinds, result capability, owner/governance, and contextual runtime binding.
Represent deterministic, review-tool, or LLM verifiers without a closed input
world. No secrets.

## `observe.svg` — O-01 through O-10

All pages show grouped subnavigation: `Overview`; `Explore` → Logs, Metrics,
Traces; `Analyze` → Dashboards, Evaluations, Drift. Signal homes are canonical
unfiltered routes. Service, Card, Run, principal, experiment, request, trace,
span, OpenTelemetry, time, status, and signal-specific values are URL filters.

### O-01 — overview — `/t/acme/observe?service=checkout-api&range=1h`

Show shared time/correlation filters; compact Logs/Metrics/Traces/Eval/Drift
summaries; cross-signal attention feed; recent observation activity; and
filtered links preserving context. No Service inventory, full signal charts,
or customizable dashboard.

### O-02 — Logs — `/t/acme/observe/logs?service=checkout-api&level=error&range=1h`

Show stream/service, range, level/status, attributes, text search, active chips,
volume trend, matching records, selected structured detail, and annotated
`Open in Query`. This is guided investigation, not raw SQL.

### O-03 — Metrics — `/t/acme/observe/metrics?service=checkout-api&range=6h`

Show metric discovery, selected metric, label filters, range, chart, labeled
non-color legend, underlying values, and correlated logs/traces links.

### O-04 — Traces — `/t/acme/observe/traces?service=checkout-api&status=error&range=1h`

Show service name/namespace/version/instance, status, error, duration, time, and
attribute facets; active chips; compact trend; results with trace ID/root
operation/service/start/duration/spans/status. Row selection preserves search.

### O-05 — trace detail — `/t/acme/observe/traces/trace_01?service=checkout-api&range=1h`

Show trace identity/service/root operation/start/duration/status/error/span
summary; waterfall; service graph; selected-span attributes/events/links/status/
timing tabs; conditional AI content; and context-preserving `Back to traces`.

### O-06 — dashboard inventory — `/t/acme/observe/dashboards`

Show search/filter, useful folder/tag discovery, owner, updated time, compact
preview/open action, and empty/loading treatment. No create/edit/alert/schedule.

### O-07 — dashboard detail — `/t/acme/observe/dashboards/dashboard_01?range=24h`

Show identity, owner/updated context, global time/variables, read-only panel
grid, labeled charts/tables, and no-data/error panels. No editor or save action.

### O-08 — Eval inventory — `/t/acme/observe/evaluations?service=checkout-agent&status=failed&range=24h`

Show events keyed by `record_id`; filters for Eval Card, subject, service/agent,
principal, `run_id`, status, time; rows with identity/Card/subject/workflow/run/
status/task counts/duration/time. `run_id` is correlation, not identity.

### O-09 — Eval event — `/t/acme/observe/evaluations/eval_record_01?task=groundedness&service=checkout-agent`

Show one event's resolved Eval Card, subject/correlation, lifecycle/pass summary,
stages, task list, and selected task type/status/stage/operator/expected/
authorized actual/score/timing/explanation/trace. `task` selects a pane; no
Workflow or Task resource routes.

### O-10 — Drift results — `/t/acme/observe/drift?driftCard=card_drift_01&service=ranking-api&feature=score&range=30d`

Show applicable Drift Card/subject/principal/service/Run/method/signal/feature/
verdict/time filters; read-only definition; overall verdict; feature selector;
calculated score history; baseline/threshold overlays; feature score/verdict
rows; alerts; correlation links. Label reports; never substitute observations.

## `changes.svg` — CR-01 through CR-07

Use familiar pull-request hierarchy and controls in Wyrd styling. One shared
workspace serves Product, Data Science, and Engineering. Intent/impact/Claims/
state lead; commits/diffs/payloads/bindings/provenance are drilldowns.

### CR-01 — list — `/t/acme/changes?view=needs-attention`

Show a searchable PR-style list, never kanban, with Open/Needs attention/
Verified/Closed filters. Rows show title, owner, lifecycle, repos/PRs, required
Claims satisfied/total, Verifiers needing attention, and activity. Include
draft/open/verified/needs-attention/closed without conflating lifecycle/status.
Include three relationship examples: one repo/one PR; one repo/two stacked PR
subjects; and two repos/one PR per repo.

### CR-02 — new/draft — `/t/acme/changes/new`

One progressive form, not wizard: title/intent, impact, owners/teams, one or
more subjects, Claims, required/advisory Verifiers, manual or
`on_new_evidence` mode. Allow incomplete `Save draft`; show inline validation
and adjacent billable-mode warning. Saved draft links to Overview.

This page creates or saves a draft only. It must not show Evidence upload,
unmatched Evidence, or an action implying that Evidence can be accepted before
an existing Change Request revision and one exact subject have been resolved.

### CR-03 — Overview — `/t/acme/changes/change_01`

Lead with what/why/impact/owners/teams. Show immutable revision and lifecycle;
all subjects with repo/provider/exact base/candidate/optional PR; Claims; and
separate verification and authorization/override summaries. Link Verification,
Review, Timeline, subjects, Cards, and providers.

### CR-04 — Verification — `/t/acme/changes/change_01/verification`

Organize by Claim. Show required/advisory Verifiers, received/missing Evidence,
mode, run status, result/verdict, Claim resolution, and decision explanation.
Demonstrate Not ready/Not run/Verifying/Needs attention/Verified; pass/fail;
stale/carried-forward; and override. Keep Evidence, execution, verdict, Claim,
lifecycle, provenance, and authorization separate. Include cases/provenance/
inputs/provider drilldowns and billable manual Run/Rerun warnings.

Include an expanded Evidence detail inside this artboard. It shows
`TestRunEvidence`; producer channel (`local`, `CI/CD`, or `Wyrd CLI`); producer
execution identity; normalized run execution status and derived aggregate
outcome; normalized cases with stable key, outcome, duration/failure summary,
and retry attempt; OS/version/architecture; language runtime and test-tool
versions; container image identity/digest when applicable; dependency snapshot
descriptor; media type, digest, and byte size;
and content-addressed attachment state for a log, lockfile, environment
snapshot, or SBOM. Show the resolved Change revision/subject and immutable
manifest before a Verifier consumes it. Do not display credentials, arbitrary
environment variables, or vendor-native adapter contracts.

Evidence shown here is already bound to the existing Change Request revision
and exactly one subject. Upload initialization with no exact match is rejected;
an ambiguous match requires explicit revision selection and accepts no bytes
until resolved. Unmatched Evidence is never retained as a pending UI state.
Show both supported initialization paths: an explicitly supplied revision ID
that the server validates against the subject, and a CI producer that omits the
revision hint while supplying provider-neutral repository, exact base/candidate
commits, and optional PR context. The latter visibly resolves to the one exact
existing revision/subject before upload begins; label the resulting revision ID
`Resolved by Wyrd`, not producer-authored.

Within the expanded detail, include one compact remote-attachment provenance
sequence:

`provider context → exact revision/subject match → bounded URL retrieval →
digest and byte-size verification → copied to Wyrd-managed immutable storage`

The URL is labeled a temporary retrieval location, never attachment identity.
The digest is the durable content identity. A private URL shows a registered
Connection/`SecretRef` label with no credential or signed query string. Also
show expired URL and digest-mismatch as rejected/incomplete states; a Verifier
cannot consume the manifest until required content is imported and verified.

Show Verifier Card identity/version separately from the tenant-specific runtime
binding. Binding details may show enabled/health state, endpoint or command,
SecretRef disclosure summary, and non-secret configuration digest; never put
binding health, mutable configuration, or secrets on the immutable Card.

Include this compact state storyboard with prior history retained:

1. A new revision/commit makes only results whose declared inputs changed
   `Stale`; it makes the affected requirement not-current but does not execute
   a Verifier or incur cost by itself.
2. Matching new Evidence arrives. For `automatic` requirements (user-facing
   label: `On new evidence`) whose declared inputs are ready, the run becomes
   queued/running and may incur cost. Manual requirements remain `Not run`.
3. Exact unchanged input digests are idempotent and do not create a duplicate
   paid run. A genuine CI retry has a new producer execution and Evidence ID
   and may produce a new run.
4. Completion updates result/verdict, then Claim resolution, then the aggregate
   revision decision. Do not collapse these into one badge.
5. Manual `Run` or `Rerun` requires explicit billable confirmation.

The override example visibly reads `Claim unresolved`, `Override authorized`,
and `Revision not Verified`, with the annotation `Override is authorization,
not a Verifier pass.`

### CR-05 — Review — `/t/acme/changes/change_01/review`

Show findings/discussion with familiar author/time/reply/@mention/edit-history/
resolve/reopen. Anchor threads to Claim, source line, Evidence, and Verifier
result. Include all three teams. Keep stable comment identity/edit revisions;
separate approval/authorization from verification; no source editing/merge.

### CR-06 — Timeline — `/t/acme/changes/change_01/timeline`

Chronologically show revision, commit/PR movement, CI/Evidence arrival, verifier
run/result, Claim resolution, decision/override, and discussion. Each row has
actor/time/type/explanation/destination. Distinguish audit from discussion.

### CR-07 — subject detail — `/t/acme/changes/change_01/subjects/subject_api`

Show repo/provider/optional PR, exact base/candidate, commits, changed files,
read-only unified diff, anchored-review links, and `View in provider`. No edit,
merge, branch mutation, or duplicate provider review system.

## `query.svg` — Q-01

### Q-01 — query workbench — `/t/acme/query`

Show searchable catalog→schema→table→column explorer; one unsaved SQL editor;
Run/Cancel; execution state/timing/safety ceilings; Results/Query Details/
History panes; dense result table; query ID/status/time/rows/bytes/safe error;
and session-only history. Demonstrate empty/running/success/cancelled/failed in
compact samples. No mutations, loading, writes, admin, saved worksheets,
multi-tabs, charts, sharing, collaboration, or query profiles.

Visible safety labels state: authorized read-only query, exactly one `SELECT`,
timeout ceiling, function restrictions, row ceiling, and byte ceiling.

# Mobile artboard register

Use the exact 390 × 844 artboard. Current tenant remains visible. REQ-098's
approved accessible mobile navigation intentionally overrides the generic
brand example that stacks the sidebar and says “no hamburger.” Render a labeled
`Menu` button in the closed shell plus a compact inset of its open disclosure.
The open panel contains the same five links, current-area state, tenant
identity, and close affordance with no mobile-only nouns. Preserve route,
selection, authorization, filters, and actions.

- **M-01 Cards inventory** — `/t/acme/cards?kind=Service&space=prod`: compact
  header, scrolling filters/chips, labeled stacked rows; retain count/kind/
  version/status/space/owner.
- **M-02 Card detail** — `/t/acme/cards/card_service_01`: Service fixture;
  primary Spec first; metadata/relationships below; version and Observe links
  reachable without hover.
- **M-03 Change creation** — `/t/acme/changes/new`: same progressive form;
  collapse optional sections only; keep Save draft and cost warning visible.
- **M-04 Change review** — `/t/acme/changes/change_01/review`: anchor above
  comments; retain reply/mention/history/resolve/reopen/approval; allow dense
  source anchors to scroll with repo/file/line context.
- **M-05 Trace detail** — `/t/acme/observe/traces/trace_01?service=checkout-api&range=1h`:
  preserve header/filters/back URL; waterfall scrolls; graph/span tabs stack.
- **M-06 Eval detail** — `/t/acme/observe/evaluations/eval_record_01?task=groundedness`:
  Workflow and Task are tabs/sections of one event; preserve `task`; stack
  correlation/technical detail after summary.
- **M-07 Drift** — `/t/acme/observe/drift?driftCard=card_drift_01&feature=score&range=30d`:
  visible filters; stack definition/verdict/history/features/alerts/links;
  dense rows scroll rather than lose columns.
- **M-08 Query** — `/t/acme/query`: catalog drawer above editor/results; retain
  Run/Cancel/state/time/all three result tabs; contain two-axis result overflow.

# Responsive pattern matrix

`README.md` must list all 32 desktop IDs individually and assign one pattern:

| Pattern | Narrow behavior | Routes/pages |
|---|---|---|
| R-ENTRY | Centered identity/choice, no tenant shell | H-01 |
| R-HOME | Mobile header/menu; summaries stack | H-02 |
| R-LIST | Accessible filters; labeled rows or controlled table overflow | C-01, CR-01, O-06, O-08 |
| R-CARD | Primary Card first; metadata/relationships stack | C-02–C-12 |
| R-SIGNAL | Visible time/filters; trend then records then detail | O-01–O-04 |
| R-DETAIL | Identity/filters first; dense visual scrolls; rail stacks | O-05, O-07 |
| R-EVAL | Workflow/Task are views of the same event | O-09 |
| R-DRIFT | Definition/results stack without merging | O-10 |
| R-FORM | One scrolling form; safe actions remain reachable | CR-02 |
| R-CHANGE | Plain summary first; technical sections stack | CR-03–CR-06 |
| R-DIFF | Subject first; commits/files/diff use controlled overflow | CR-07 |
| R-QUERY | Catalog drawer; results controlled overflow | Q-01 |

# Link annotations

Annotate tenant resolution/switching; primary and Observe navigation; Card row
to detail and Card to filtered Observe; cross-signal context preservation;
trace detail/back; Eval event/task query selection; Drift Card/results; Change
list/new to Overview and Overview to all tabs/subjects/Cards/providers; Review
anchors; Query Run/Cancel/tabs/catalog drawer; and Logs `Open in Query`.

# Material states ledger

Show these states inside the assigned compositions; do not defer placement to
the mock author or create duplicate full pages. `README.md` records the exact
panel/inset that demonstrates each:

| Artboard | Required states |
|---|---|
| H-01 | zero tenants/access or provisioning; one-tenant redirect; multiple-tenant chooser; unauthorized result |
| H-02 | ordinary single-tenant shell; annotated authorized tenant-switcher variant; attention empty state |
| C-01 | populated inventory; loading rows; no matching results; safe structured error; removable/direct-restored/cleared filters |
| C-02 | current version, prior version selection, relationship empty state, safe Spec error |
| O-01 | normal summary, one no-data signal, one needs-attention signal, inherited and cleared scope |
| O-02 | partial results, no matches, safe search error, selected log record |
| O-03 | loading chart, successful series, no-data series, safe query error |
| O-04 | partial results, no matches, safe search error, selected trace row |
| O-05 | selected span, error span, missing optional AI content, preserved back-search context |
| O-06 | populated, loading, empty, safe error dashboard inventory |
| O-07 | successful, no-data, and safe-error panels in one read-only dashboard |
| O-08 | populated event list, no matches, loading, safe error, direct-restored filters |
| O-09 | workflow running/completed stages; passed/failed/error/skipped task rows; selected task; unauthorized actual-value redaction |
| O-10 | calculated report selected; feature pass/fail; threshold breach alert; no report for range; raw-observation distinction |
| CR-01 | draft/open/closed lifecycle rows; Open/Needs attention/Verified/Closed filter states; all three repo/PR relationship examples |
| CR-02 | incomplete draft, inline validation, manual mode, On-new-evidence mode, billable warning, Save draft |
| CR-03 | current revision, prior revision link, separate verification and authorization, multi-subject relationships |
| CR-04 | Not ready, Not run, queued/running/complete, Needs attention, Verified; required/advisory; missing/received Evidence; passed/failed/inconclusive; stale/carried-forward; manual/automatic; duplicate suppression; billable rerun; unresolved Claim plus authorized override plus not-Verified revision; expanded Evidence and binding detail |
| CR-05 | open/resolved/reopened threads; mention; immutable edit history; stale expected-revision error; separate approval state |
| CR-06 | revision/commit/PR, CI/Evidence, run/result, Claim, decision/override, and discussion events |
| CR-07 | stacked-PR subject relationship, selected changed file, unified diff, source anchor, provider link |
| Q-01 | empty, running, successful, cancelled, and failed execution; structured safe error; safety ceilings |

The remaining specialized Card artboards show populated primary content and at
least one earned empty/absent optional section without inventing a new state.
Every mobile artboard shows the selected desktop state plus its required narrow
overflow/stacking behavior.

# Prohibited shortcuts

- No Svelte, BFF, auth, Nginx, server, or durable protocol implementation.
- No new design system, font, tokens, library, or rendering pipeline.
redacted
- No top-level Claims, Evidence, Inbox, Connections, Evaluations, Drift, Logs,
  Metrics, Traces, or Dashboards.
- No Service observation hierarchy or browser-created correlation.
- No kind-specific Card routes, Eval Task/Workflow resources, or Drift-method
  routes.
- No independently composed themes or color-only status.
- No implementation nouns dominating ordinary Change summaries.

# Ordered execution

1. Read the authorities and inspect the named visual/research references.
2. Create `README.md` first with all 40 page IDs, routes, states, links, and
   responsive mapping. This is the completeness ledger.
3. Establish shared SVG definitions using current brand tokens.
4. Complete `home.svg`, `cards.svg`, `observe.svg`, `changes.svg`, `query.svg`,
   then `mobile.svg`.
5. Walk every page/state/link/pattern against the ledger.
6. Present all six sheets and ledger for explicit user review. Stop before UI
   implementation.

Reuse SVG definitions only when files stay directly viewable. Do not build a
rendering framework for six static artifacts.

# Verification and evidence

Direct artifact review only; do not run application tests or lints for this
static mock task:

- Open every SVG at its declared scale.
- Confirm 64 desktop and 16 mobile artboards.
- Confirm theme pairs have identical information and geometry.
- Walk `README.md` against every ID, route, state, link, and pattern.
- Check mobile for clipped actions, hidden tenant, lost filters, inaccessible
  detail, and uncontrolled page-width overflow.
- Record direct artifact links/screenshots for user acceptance.

# Completion and stop conditions

Complete only when all six sheets and the ledger satisfy this task and the user
explicitly accepts them. TASK-002 and all Svelte view tasks remain blocked.

Return to specification work if drawing exposes a missing route, durable state,
or materially different workflow. Ask before changing brand tokens, geometry,
navigation, or this page register. Never let a research reference silently
settle a Wyrd product decision.

# Required implementation skills

- `$wyrd-implement`
- `wyrd-ui`

# Execution evidence

Static-artifact task: direct artifact review only, no application tests or
lints, per the verification section above.

## Delivered

`crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/`

| Artifact | Pages | Artboards | Bytes |
|---|---|---|---|
| `home.svg` | H-01, H-02 | 4 | 113,988 |
| `cards.svg` | C-01…C-12 | 24 | 829,142 |
| `observe.svg` | O-01…O-10 | 20 | 706,771 |
| `changes.svg` | CR-01…CR-07 | 14 | 506,583 |
| `query.svg` | Q-01 | 2 | 88,232 |
| `mobile.svg` | M-01…M-08 | 16 | 304,815 |
| `README.md` | 40-page ledger | — | 36,776 |

`brand/renders/index.html` gains a "Product mocks" section (section 00) linking
all six sheets and the ledger.

## Verification performed

- **Artboard census** — 80 groups present, ids `<PAGE-ID>-<light|dark>`, each
  with `<title>`/`<desc>` and an in-frame `id · viewport` label. 64 desktop
  (1440 × 1024), 16 mobile (390 × 844).
- **Sheet geometry** — desktop sheets `viewBox="0 0 3136 …"`, mobile
  `viewBox="0 0 956 …"`; 96/64 desktop and 64/48 mobile margins and gaps;
  96/80 label bands; ≥64px row separation.
- **Theme parity** — every `-light`/`-dark` pair carries an identical `<desc>`
  apart from the theme word, identical geometry, identical fixture data and
  identical state; only tokens differ.
- **Self-containment** — no `foreignObject`, `<image>`, `<use>`, `href=`,
  `src=`, `xlink:href`, `@import`, `<script>` or `@font-face`; every `url()` is
  a local `#clip-*` reference. Fonts are declared with system fallback stacks.
  The only `http://` string is the SVG namespace (plus one fixture endpoint
  drawn as visible text, which is content rather than a dependency).
- **Legibility** — no product text below 11px; annotation strips at 14px.
- **Ledger coverage** — every page id, route and responsive pattern in the six
  registers resolves in `README.md`; all 32 desktop pages are individually
  assigned exactly one of the 12 responsive patterns.
- **Overflow sweep** — estimated text extents were checked against each
  artboard's frame. Two matches remain and are intentional: O-03's correlation
  URLs are only over the estimate because `&` is stored as `&amp;`, and M-07's
  third filter chip is deliberately clipped to demonstrate the scrolling chip
  row (annotated as such on the artboard).
- **Visual review** — every sheet was rendered to PNG in both themes and read
  at full scale; collisions found this way (panel head/meta overlaps,
  annotation overflow, waterfall label collisions, rail/panel overlap, mobile
  row and note overflow) were corrected and re-rendered.
- **Mobile checks** — tenant identity is visible in every closed shell; no
  action is behind the Menu; filters remain visible and removable; dense
  regions (waterfall, diff, results, feature rows) scroll inside their panel
  and the page never scrolls sideways. REQ-098's labeled `Menu` button plus the
  open-disclosure inset (M-01) intentionally overrides the generic brand
  "stack the sidebar, no hamburger" example.

## Limitations

The `Verifier` kind is drawn as the seventeenth registrable Card kind, per this
change's approved specification, alongside the 16 native kinds in AGENTS.md.
