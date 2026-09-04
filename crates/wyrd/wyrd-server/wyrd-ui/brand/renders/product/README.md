# Wyrd product mocks — route-mapped SVG contact sheets

Directly viewable SVG contact sheets covering every route in the approved
`wyrd-ui-foundation` specification, each page drawn twice: light and dark. The
two themes carry identical information, hierarchy, geometry, fixture data and
state — only tokens change. Six top-level sheets carry the general product set;
the nested [`cards/experiment/`](./cards/experiment/) package carries the
detailed Experiment workspace behind C-06, the nested
[`cards/service/`](./cards/service/) package carries the detailed Service
operational workspace behind C-09, and the nested [`changes/`](./changes/)
package carries the Change Request review workspace behind CR-01.

| Sheet | Pages | Artboards | Viewport |
|---|---|---|---|
| [`home.svg`](./home.svg) | 2 | 4 | 1440 × 1024 |
| [`cards.svg`](./cards.svg) | 14 | 28 | 1440 × 1024 |
| [`observe.svg`](./observe.svg) | 10 | 20 | 1440 × 1024 |
| [`changes.svg`](./changes.svg) | 7 | 14 | 1440 × 1024 |
| [`query.svg`](./query.svg) | 1 | 2 | 1440 × 1024 |
| [`mobile.svg`](./mobile.svg) | 9 | 18 | 390 × 844 |
| *existing product set* | *43* | *86* | |
| [`cards/experiment/overview.svg`](./cards/experiment/overview.svg) | 2 | 4 | 1440 × 1024 |
| [`cards/experiment/runs.svg`](./cards/experiment/runs.svg) | 5 | 10 | 1440 × 1024 |
| [`cards/experiment/compare.svg`](./cards/experiment/compare.svg) | 1 | 2 | 1440 × 1024 |
| [`cards/experiment/outputs.svg`](./cards/experiment/outputs.svg) | 9 | 18 | 1440 × 1024 |
| [`cards/experiment/versions.svg`](./cards/experiment/versions.svg) | 1 | 2 | 1440 × 1024 |
| [`cards/experiment/mobile.svg`](./cards/experiment/mobile.svg) | 3 | 6 | 390 × 844 |
| *Experiment workspace package* | *21* | *42* | |
| [`cards/service/overview.svg`](./cards/service/overview.svg) | 5 | 10 | 1440 × 1024 |
| [`cards/service/composition.svg`](./cards/service/composition.svg) | 1 | 2 | 1440 × 1024 |
| [`cards/service/definition.svg`](./cards/service/definition.svg) | 1 | 2 | 1440 × 1024 |
| [`cards/service/mobile.svg`](./cards/service/mobile.svg) | 2 | 4 | 390 × 844 |
| *Service workspace package* | *9* | *18* | |
| [`changes/inbox.svg`](./changes/inbox.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/new.svg`](./changes/new.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/overview.svg`](./changes/overview.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/verification.svg`](./changes/verification.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/review.svg`](./changes/review.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/timeline.svg`](./changes/timeline.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/subjects.svg`](./changes/subjects.svg) | 1 | 2 | 1440 × 1024 |
| [`changes/mobile.svg`](./changes/mobile.svg) | 3 | 6 | 390 × 844 |
| *Change Request workspace package* | *10* | *20* | |
| **complete set** | **83** | **166** | |

Every artboard is a group whose id is `<PAGE-ID>-<theme>` (for example
`C-04-dark`), carries a `<title>`/`<desc>` pair, and is labeled in the dashed
annotation strip below it. The sheets carry no image, script or
`foreignObject`; typography loads via a Google Fonts `@import` (Archivo, Space
Grotesk, JetBrains Mono, Fraunces), so an offline viewer falls back to system
faces but keeps identical layout.

The three `golden-*.svg` files are the Gate A reference screens (CR-04, O-09,
O-04) approved before the grammar was applied to the full set; the same pages
also appear inside their sheets. Keep the goldens unchanged as the approval
record.

Fixture tenant is `acme` throughout: one coherent set of Cards, services,
principals, signals, a single Change Request `change_01`, and one query session.

## Artboard ledger

All 83 pages, in sheet order — the 43 general product pages first, then the
21-page detailed Experiment workspace package, then the 9-page detailed
Service workspace package, then the 10-page Change Request review workspace
package.

### `home.svg`

#### H-01 — Tenant resolution

- **Route**: `/`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: zero tenants → access/provisioning panel (Outcome A); unauthorized result inset in Outcome A; one tenant → 302 redirect panel (Outcome B); multiple tenants → recent-first chooser (Outcome C)
- **Links**: Request access → provisioning; Outcome B → /t/acme; chooser row → /t/{tenantKey}; no tenant-scoped link is offered before resolution
- **Responsive pattern**: R-ENTRY

#### H-02 — Home

- **Route**: `/t/acme`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: ordinary single-tenant shell (static tenant in topbar); authorized tenant-switcher variant and attention empty state described in the annotation strip
- **Links**: New Change Request → /t/acme/changes/new; Card lookup → /t/acme/cards; attention row → /t/acme/changes/change_01; Cards summary → /t/acme/cards; Observe summary → /t/acme/observe; Changes summary → /t/acme/changes; recent work → trace/eval/query routes
- **Responsive pattern**: R-HOME

### `cards.svg`

#### C-01 — Card inventory

- **Route**: `/t/acme/cards`
- **URL params**: ?kind=Service&space=prod
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated inventory (6 of 42 shown); loading skeleton, no-matching-results and safe structured error occur at the table position (described in the annotation strip); removable, direct-restorable and cleared filters described in the Filter behaviour rail
- **Links**: row → /t/acme/cards/card_service_01; chip × → same route minus that param; View all → /t/acme/cards; recent Card → /t/acme/cards/{uid}
- **Responsive pattern**: R-LIST

#### C-02 — Shared Card detail shell

- **Route**: `/t/acme/cards/card_generic_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: demonstrated on a Policy fixture (settlement-guardrails) — a kind with no dedicated detail page, so the shared shell is its only rendering; current version v7 selected in the Version table; prior version selection rows v6/v5; relationship empty state and safe Spec error occur at their section positions (described in the annotation strip)
- **Links**: version row → same route ?version=v6; relationship → /t/acme/cards/{uid}; applies_to → the settlement-api Service Card; Links rail → filtered /t/acme/observe pages
- **Responsive pattern**: R-CARD

#### C-03 — Data Card

- **Route**: `/t/acme/cards/card_data_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated schema, profile, distribution, splits and targets; earned absent optional section — no figures attached — noted below the splits table
- **Links**: derived_from/used_by → /t/acme/cards/{uid}; Drift link → /t/acme/observe/drift?driftCard=card_drift_01; Query → /t/acme/query
- **Responsive pattern**: R-CARD

#### C-04 — Model Card

- **Route**: `/t/acme/cards/card_model_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated task/interface, signature, artifacts, deployment relationships and linked verification; earned absent optional section — no Prompt relationship — noted in the rail
- **Links**: trained_on/produced_by/deployed_by → /t/acme/cards/{uid}; Eval link → /t/acme/observe/evaluations?evalCard=card_eval_02; Drift link → /t/acme/observe/drift
- **Responsive pattern**: R-CARD

#### C-05 — Prompt Card

- **Route**: `/t/acme/cards/card_prompt_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: ordered prompt definition dominant — system instructions separate from the conversation, explicit roles on every message, labeled text/image/tool-call content treatment (audio and file parts use the same treatment; unsupported provider parts stay discoverable in Raw definition); provider, model, operation, settings and structured response type; variables and media variables with type, required/default state and where each is used; Response schema (open) and Raw definition (closed) disclosures with copy affordance; no secrets and no execution or playground action
- **Links**: used by → /t/acme/cards/card_agent_01; component of → /t/acme/cards/card_service_01; Eval results → /t/acme/observe/evaluations?evalCard=card_eval_01; Response schema / Raw definition → ⧉ copy
- **Responsive pattern**: R-CARD

#### C-06 — Experiment Card (workspace entry)

- **Route**: `/t/acme/cards/card_experiment_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme` · `checkout-ranking-study` v4 ACTIVE · uid `card_experiment_01`
- **Material states**: concise entry/summary, not the workspace — authored summary (spec `summary_metrics` ndcg@10 0.742 plus the author-pinned `best_run_ref` run_07, declaration content, never a derived winner), run counts by lifecycle (7 completed / 1 running / 1 queued / 2 failed / 1 cancelled), running-now and needs-attention links, five URL-restorable workspace destinations, latest-runs excerpt (3 of 12), relationships; earned absent — no attached notebooks. The detailed workspace is the nested [`cards/experiment/`](./cards/experiment/) package (E-01…E-18, EM-01…EM-03); this page only summarizes and routes into it.
- **Links**: Open workspace / destinations → `?view={overview,runs,compare,outputs,versions}`; run rows → `?view=runs&run={id}&section=summary`; relationships → /t/acme/cards/{uid}; Observe → /t/acme/observe?experiment=card_experiment_01
- **Responsive pattern**: R-CARD

#### C-07 — Agent Card

- **Route**: `/t/acme/cards/card_agent_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: registered Prompt as the primary linked dependency (name, version, provider/model, message and variable counts) with its raised read-only inspection drawer shown open — a CardRef reuse, never a copy; composition strip Prompt → Agent → runtime tools with component-of and publishes-to edges; run limits (max iterations, tool concurrency, recent sessions, timeout); tools remain Skald runtime registrations, never Card kinds; earned absent — no attached artifacts
- **Links**: Inspect Prompt → the raised drawer; Open Prompt Card → /t/acme/cards/card_prompt_01; publishes to → /t/acme/cards/card_eval_01; component of → /t/acme/cards/card_service_01; Traces/Evaluations → filtered /t/acme/observe routes with ?service=checkout-agent
- **Responsive pattern**: R-CARD

#### C-08 — Workflow Card

- **Route**: `/t/acme/cards/card_workflow_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated ordered stages, inputs, outputs, owners and governance; earned absent optional section — no execution results are shown on a declaration
- **Links**: referenced_by/consumes → /t/acme/cards/{uid}; Evaluations → /t/acme/observe/evaluations?evalCard=card_eval_01
- **Responsive pattern**: R-CARD

#### C-09 — Service Card (operational Overview)

- **Route**: `/t/acme/cards/card_service_01`
- **URL params**: `?view=overview&version=v12&range=1h`
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`, `checkout-api` v12, needs-attention
- **Material states**: a signal-driven mini-dashboard in four layers — trends, not tiles or prose, are the evidence. (1) Compact assessment strip: ▲ NEEDS ATTENTION with one plain-language sentence (model-drift over threshold on ranker), the four state channels visibly separated and labeled (Card state ✓ ACTIVE registry lifecycle; deployment ● 6/6 explicit mock projection; operational ▲ server-projected; freshness ✓ 38s), and the v12 deploy named as correlation, never proven cause. (2) Dominant operating dashboard: six chart slots sharing the exact 1h range — req/s, error rate with the 0.50% budget as a positional line, latency with dash+marker-distinguished p50/p95/p99 and the p95 target line, availability against the 99.9% SLO, one saved custom chart (checkout decline rate), and a restrained + Add chart empty slot; every chart carries its latest value, unit, freshness and source. (3) Published intelligence from the v12 declaration's publication bindings: model-drift with the 30d PSI trend crossing the 0.20 threshold to 0.27 on subject ranker (model_primary · Model v12), baseline/sample/missingness and calculation freshness; checkout-agent-eval with the pass-rate trend against ≥ 95% on subject checkout-agent (agent_triage · Agent v6). (4) One supporting context band: server-ordered attention naming the affected component, component chips, state-explaining activity, and inline scope-preserving investigations
- **Links**: every chart → canonical Metrics/Traces with service+version+range preserved; drift/eval intelligence → /t/acme/observe/{drift,evaluations}; attention → its originating drift signal; inline investigate row → all five signals; subnav → `?view={overview,composition,definition}`; no Alerts route; detailed state pages: [`cards/service/`](./cards/service/)
- **Responsive pattern**: R-CARD (realized by M-02/SM-01)

#### C-10 — Eval Card

- **Route**: `/t/acme/cards/card_eval_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: definition inspection — dataset/source, workflow stages with dependencies, pass gate and six-task summary; groundedness selected with a raised task-definition drawer (kind, input selector, context path, operator, expected value, dependency condition, judge Agent, retry policy); selecting the workflow opens equivalent workflow-definition detail; no runtime scores or verdicts on the Card; earned absent — no subject_ref and no Eval-owned schedule
- **Links**: task row → the definition drawer; Open Workflow Card → /t/acme/cards/card_workflow_01; Results → /t/acme/observe/evaluations?evalCard=card_eval_01; runs/consumes/scheduled_by → /t/acme/cards/{uid}
- **Responsive pattern**: R-CARD

#### C-11 — Drift Card

- **Route**: `/t/acme/cards/card_drift_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: Definition and Results visibly distinct — Definition (the Card): PSI method with quantile 10-bin profile, Distribution signal, feature `feature`, baseline txns-2026q3 (Data), fixed threshold 0.20, publisher checkout-api/ranker via model_primary, Trigger/Operator chain, no Drift-owned schedule; Results (Vala/Observe projection, never written to the Card): selected feature dominant, current PSI 0.27 breach, 30d series with the plotted 0.20 threshold and positional tick, baseline and sample/missingness context, last calculation, alert history, calculated-vs-raw distinction
- **Links**: baseline → /t/acme/cards/card_data_01; publisher → /t/acme/cards/card_service_01; fires → /t/acme/cards/card_trigger_01; invokes → /t/acme/cards/card_operator_01; Explore → /t/acme/observe/drift?driftCard=card_drift_01&feature=feature&service=checkout-api&range=30d
- **Responsive pattern**: R-CARD

#### C-12 — Verifier Card

- **Route**: `/t/acme/cards/card_verifier_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated versioned identity, purpose, capabilities, accepted normalized Evidence kinds, result capability, owner/governance and contextual runtime binding; earned absent optional section — no secrets and no closed input world
- **Links**: verifies_for → /t/acme/changes/change_01; Verification → /t/acme/changes/change_01/verification; Changes → /t/acme/changes?verifier=card_verifier_01
- **Responsive pattern**: R-CARD

#### C-13 — Trigger Card

- **Route**: `/t/acme/cards/card_trigger_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: plain-language wiring sentence first, exact configuration after — cron */10 * * * * with America/New_York timezone, Drift observation source model-drift, optional subject filter service=checkout-api (present), linked Operator; source → Trigger → Operator flow strip; reachable publishing context checkout-api / ranker; projected firing history labeled as a runtime projection, separate from the declaration; scheduling lives here, never on the Drift Card
- **Links**: source → /t/acme/cards/card_drift_01; invokes → /t/acme/cards/card_operator_01; reaches → /t/acme/cards/card_service_01; Open Operator → /t/acme/cards/card_operator_01
- **Responsive pattern**: R-CARD

#### C-14 — Operator Card

- **Route**: `/t/acme/cards/card_operator_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: exactly one action — dispatches Workflow runtime with a human-readable input/context template and a Raw JSON disclosure; execution budget (max wall time 15m, tool-call budget 40); inbound chain Drift → Trigger → this Operator → Workflow; credential shown as a redacted reference name only, values never render; recent outcome separated as a runtime projection; Notify and HTTP are alternative action variants of the kind, stated in the ledger, not rendered on this single-action fixture
- **Links**: action → the runtime Workflow Card; invoked_by → /t/acme/cards/card_trigger_01; upstream → /t/acme/cards/card_drift_01; outcome → the Workflow run in Observe
- **Responsive pattern**: R-CARD

### `observe.svg`

#### O-01 — Observe overview

- **Route**: `/t/acme/observe`
- **URL params**: ?service=checkout-api&range=1h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: normal summary across five signals; one no-data signal (Drift, with its own rail panel); tenant-wide Attention and Recent activity feeds labeled as such (the chips scope only the signal tiles and signal pages); inherited scope shown as chips and cleared scope described in the Scope rail
- **Links**: signal summary → /t/acme/observe/{logs|metrics|traces|dashboards|evaluations|drift} preserving service and range; attention row → the same filtered signal route; View all ⨯ → /t/acme/observe
- **Responsive pattern**: R-SIGNAL

#### O-02 — Logs

- **Route**: `/t/acme/observe/logs`
- **URL params**: ?service=checkout-api&level=error&range=1h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: partial results (200 of 412 loaded) in the Records table; no-matches and safe search error occur at the records position (described in the annotation strip); selected log record expanded in the right rail
- **Links**: record row → selected detail rail; rail → trace_01 in Traces, checkout-api Card, Metrics at the same range; Open in Query → /t/acme/query pre-filled with one SELECT
- **Responsive pattern**: R-SIGNAL

#### O-03 — Metrics

- **Route**: `/t/acme/observe/metrics`
- **URL params**: ?service=checkout-api&range=6h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: successful three-series chart with dash+marker per series and underlying values; loading, no-data-series and safe query error occur at the chart position (described in the annotation strip)
- **Links**: metric row → same route ?metric=…; correlate rail → filtered Logs and Traces at the same range; Card link → /t/acme/cards/card_service_01
- **Responsive pattern**: R-SIGNAL

#### O-04 — Traces

- **Route**: `/t/acme/observe/traces`
- **URL params**: ?service=checkout-api&status=error&range=1h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: partial results (50 of 97 loaded); no-matches and safe search error occur at the table position (described in the annotation strip); selected trace highlighted in the table and detailed in the raised panel
- **Links**: row → /t/acme/observe/traces/trace_01?service=checkout-api&range=1h preserving the search; facet → the same route with that parameter added
- **Responsive pattern**: R-SIGNAL

#### O-05 — Trace detail

- **Route**: `/t/acme/observe/traces/trace_01`
- **URL params**: ?service=checkout-api&range=1h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: selected span (ledger.capture) in the rail; error span highlighted in the waterfall and service graph; missing optional AI content shown as an explicit absent section; preserved back-search context in the Back panel
- **Links**: waterfall row → selected span rail; graph node → /t/acme/cards/{uid}; span link → trace_04; Back to traces → /t/acme/observe/traces?service=checkout-api&status=error&range=1h
- **Responsive pattern**: R-DETAIL

#### O-06 — Dashboard inventory

- **Route**: `/t/acme/observe/dashboards`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated inventory with a selected row and preview; loading, empty and safe error occur at the table position (described in the annotation strip)
- **Links**: row / Open dashboard → /t/acme/observe/dashboards/dashboard_01; folder and tag → the same route with that filter applied
- **Responsive pattern**: R-LIST

#### O-07 — Dashboard detail

- **Route**: `/t/acme/observe/dashboards/dashboard_01`
- **URL params**: ?range=24h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: four successful panels (two charts, one table, one bar chart with labels), one no-data panel and one safe-error panel, all inside one read-only grid; no editor or save action
- **Links**: Back → /t/acme/observe/dashboards; variables and range → the same route with the parameter changed; panel legends carry markers and dashes rather than colour alone
- **Responsive pattern**: R-DETAIL

#### O-08 — Eval inventory

- **Route**: `/t/acme/observe/evaluations`
- **URL params**: ?service=checkout-agent&status=failed&range=24h
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated event list with a selected row; no-matches, loading and safe error occur at the table position (described in the annotation strip); filters direct-restore from the URL
- **Links**: row → /t/acme/observe/evaluations/eval_record_01?service=checkout-agent; Eval Card filter → the same route ?evalCard=card_eval_01; View all ⨯ → /t/acme/observe/evaluations
- **Responsive pattern**: R-LIST

#### O-09 — Eval event

- **Route**: `/t/acme/observe/evaluations/eval_record_01`
- **URL params**: ?task=groundedness&service=checkout-agent
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: event page behind a 28% scrim under a 75% workflow drawer; stages completed/failed/running; task rows passed/failed/error/skipped; selected task dominant with score, threshold tick, judge explanation and authorized actual value; redaction note for unauthorized principals
- **Links**: task row → same route ?task={name}; Eval Card → /t/acme/cards/card_eval_01; subject → /t/acme/cards/card_service_04; trace link → /t/acme/observe/traces/trace_01; Back to evaluations → the filtered inventory
- **Responsive pattern**: R-EVAL

#### O-10 — Drift results

- **Route**: `/t/acme/observe/drift`
- **URL params**: ?driftCard=card_drift_01&service=ranking-api&feature=score&range=30d
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: calculated report selected with overall verdict; per-feature pass and fail rows; threshold breach alert panel; + Filter affordance exposing the full subject/principal/Run/method/signal/verdict vocabulary; raw-observation distinction panel; the no-report-for-range alternate state described in the annotation strip
- **Links**: Drift Card → /t/acme/cards/card_drift_01; feature selector → same route ?feature={name}; alert → /t/acme/cards/card_operator_01; correlation → Logs, Metrics and Traces ?service=ranking-api
- **Responsive pattern**: R-DRIFT

### `changes.svg`

#### CR-01 — Change Request list

- **Route**: `/t/acme/changes`
- **URL params**: ?view=needs-attention
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: truthful Needs-attention subset — only the 3 matching records (including the draft change_03), each with a server-projected reason badge (Your review requested / Required Evidence missing / Changes requested) and the expected person or team; lifecycle (open and draft), verification, required-Claim progress, repos/PRs, last activity and owner stay separate and visible; search covers title, owner, service, repository and PR identity with a projection-backed + Add filter; no-match, unauthorized and WYRD_CHANGE_502 safe-error states all keep the view and search and never fall back to the unfiltered list; the reason vocabulary and search scope are documented in the two lower contract panels
- **Links**: row → /t/acme/changes/change_01; New Change Request → /t/acme/changes/new; view → /t/acme/changes?view={open|needs-attention|verified|closed}; Retry keeps ?view=needs-attention
- **Responsive pattern**: R-LIST

#### CR-02 — New Change Request

- **Route**: `/t/acme/changes/new`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: operable progressive form — editable title/intent/impact fields, a pull-request-URL input with Resolve that yields repository · provider · PR · exact base/candidate (the URL is convenience, never identity), a manual exact-commit path, per-subject Edit/Remove and Repair on the invalid acme/ranking row with its inline error, + Add subject / + Add Claim / + Add Verifier actions, Required and Manual/On-new-evidence selects, the adjacent billable-mode warning, editable owners and teams, an earned-absent Evidence panel (nothing uploads before a revision and one exact subject resolve), and Save draft always available on an incomplete draft
- **Links**: Save draft → /t/acme/changes/change_06 (Overview); Resolve → subject preview row; verifier name → /t/acme/cards/card_verifier_01; Discard → /t/acme/changes
- **Responsive pattern**: R-FORM

#### CR-03 — Change Request Overview

- **Route**: `/t/acme/changes/change_01`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: shared Change header (title, identity line, four separately labeled state channels — lifecycle · verification · approval · override — one server-projected next-action line, permission-aware Review changes ▾ and the ⋯ overflow) plus a server-projected Action summary naming the exact revision-7 blockers (CLAIM-2 Evidence missing → m.linden; CLAIM-3 PII review failed → j.reyes) and the current user's available action; intent/impact, the #subjects table (stacked PRs in one repository plus a second repository), Claims with derived resolution, immutable revision panel with prior links, and separate Verification and Decisions rail panels
- **Links**: tabs → /t/acme/changes/change_01/{verification,review,timeline}; Subjects → this route #subjects; subject row → /t/acme/changes/change_01/subjects/{subjectId}; prior revision → same route ?revision=rev_06
- **Responsive pattern**: R-CHANGE

#### CR-04 — Change Request Verification

- **Route**: `/t/acme/changes/change_01/verification`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: shared Change header; verification-state panel (✕ Not verified, complete passed/failed/pending aggregate, plain-language cause); honest action eligibility — Not ready exposes View missing Evidence and never Run now, the ready manual billable spot check exposes Run now behind the billable confirmation, verifying/queued expose run links and cannot be started again, the failed rerunnable manual PII review exposes Rerun behind the raised billable confirmation panel, stale stays provenance rather than verdict; the Run-eligibility rail states the server-decided rules; override never converts a failure
- **Links**: View missing Evidence → the Evidence prerequisite; View run / queue position → run inspection; View findings → the failure report; Rerun → the billable confirmation; verifier → /t/acme/cards/card_verifier_01
- **Responsive pattern**: R-CHANGE

#### CR-05 — Change Request Review

- **Route**: `/t/acme/changes/change_01/review`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: shared Change header; Change-level comment composer with live @mention autocomplete posting against revision 7; the CLAIM-2 thread open with its inline reply composer; the resolved/reopened source thread with Edit history and Reopen; the Evidence-anchored thread carrying the WYRD-COMMENT-STALE-REVISION recovery that preserves the draft; and the raised Submit review panel stating the exact revision with optional summary and the three decisions (Comment · Approve · Request changes) and each decision's effect — approval never verifies or merges
- **Links**: Claim anchor → Verification; source anchor → /t/acme/changes/change_01/subjects/subject_api; Reload, keep draft → re-anchored composer; Submit review → the recorded decision; edit history → the comment revision list
- **Responsive pattern**: R-CHANGE

#### CR-06 — Change Request Timeline

- **Route**: `/t/acme/changes/change_01/timeline`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: shared Change header and next-action line stay available; revision, commit/PR, CI/Evidence, run/result, Claim, decision/override and discussion events in one chronology with a typed destination per row; the TYPE column names each event's source — audit rows are server-derived and immutable, discussion rows are Review activity, never merged; the header's ⋯ overflow is shown open with the authorized Close Change Request (records a closure reason, never closes provider PRs); explicitly not an audit administration view
- **Links**: revision → same route ?revision=rev_07; commit/PR → the subject drilldown then the provider; evidence and run/result → the Verification tab; discussion → the Review thread
- **Responsive pattern**: R-CHANGE

#### CR-07 — Change subject detail

- **Route**: `/t/acme/changes/change_01/subjects/subject_api`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: shared Change header with the persistent Review changes action; subject identity strip (exact commits, provider, PR, stacked-base relationship) with return to the complete subject list and next-subject navigation; changed-file list with visible position (file 1 of 6) plus Prev/Next file buttons on the diff; read-only unified diff with the resolved anchor on rank.rs:118 (Open thread) and an open Start-discussion composer anchored to rank.rs:121 that creates a Wyrd revision-aware source discussion; no edit, merge, branch mutation, suggested-change application or persisted Viewed state
- **Links**: ← All subjects → Overview #subjects; changed file → same route ?file=…; Open thread → /t/acme/changes/change_01/review; View in provider ↗ → the github PR
- **Responsive pattern**: R-DIFF

### `query.svg`

#### Q-01 — Query workbench

- **Route**: `/t/acme/query`
- **URL params**: none
- **Viewport**: 1440 × 1024 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: empty, running, succeeded, cancelled and failed execution in the Execution states panel; the failed sample carries the structured safe error WYRD-QUERY-COLUMN-UNKNOWN; the Safety panel states authorization, one-SELECT, timeout, function, row and byte ceilings; catalog → schema → table → column explorer with spans selected
- **Links**: Run → executes and populates Results (same route, session state); Cancel → stops the running execution; tabs → Results / Query details / History panes; catalog table → inserts into the editor and loads Columns; Logs Open in Query → this route with the statement prefilled
- **Responsive pattern**: R-QUERY

### `mobile.svg`

#### M-01 — Cards inventory (mobile)

- **Route**: `/t/acme/cards?kind=Service&space=prod`
- **URL params**: ?kind=Service&space=prod
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: populated inventory with count, kind, version, status, space and owner retained on every stacked row; removable filter chips that scroll sideways; the open Menu disclosure shown as a compact inset
- **Links**: row → /t/acme/cards/card_service_01; chip ⨯ → the same route with that filter removed; Menu link → the five primary areas
- **Responsive pattern**: R-LIST

#### M-02 — Service Overview (mobile)

- **Route**: `/t/acme/cards/card_service_01`
- **URL params**: `?view=overview&version=v12&range=1h`
- **Viewport**: 390 × 844 (light + dark)
- **Material states**: the complete C-09 mini-dashboard stacked with nothing dropped — assessment strip (▲ NEEDS ATTENTION, latest change as correlation), four labeled state channels, six metric trend rows each carrying a sparkline trend, latest value, freshness and a canonical link (four standard + saved custom + Add chart), drift/eval intelligence with score trends and identified subjects, attention naming the affected component (ranker · model_primary · Model v12), component chips, state-explaining activity and all five scope-preserving investigations
- **Links**: nav chips → `?view={overview,composition,definition}`; every trend row → its canonical Observe destination with service+version+range preserved; attention → originating drift signal; detailed narrow charts: SM-01 in [`cards/service/`](./cards/service/)
- **Responsive pattern**: R-SVC realizes C-09

#### M-03 — Change creation (mobile)

- **Route**: `/t/acme/changes/new`
- **URL params**: none
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: the operable narrow creation flow (same anatomy as CRWM-01) — title/intent fields, three subject rows with the inline error and Repair on the invalid row, + Add subject actions, Claims collapsed but present, the billable warning, progress states and a sticky Save draft / Discard footer that never hides
- **Links**: Save draft → the new Change Request Overview; + Add subject → PR URL or exact commits; Discard → /t/acme/changes; detailed package: `changes/`
- **Responsive pattern**: R-FORM

#### M-04 — Change review (mobile)

- **Route**: `/t/acme/changes/change_01/review`
- **URL params**: none
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: the complete narrow review flow (same anatomy as CRWM-02) — mini shared header with separate lifecycle/verification/approval badges and the next-action line, the Change-level composer, the open thread with reply composer and stale-revision recovery, the resolved thread with Reopen, and the full Submit review panel keeping all three decisions and the exact revision
- **Links**: thread anchors → Verification / subject_api; Submit review → the recorded decision; detailed package: `changes/`
- **Responsive pattern**: R-CHANGE

#### M-05 — Trace detail (mobile)

- **Route**: `/t/acme/observe/traces/trace_01?service=checkout-api&range=1h`
- **URL params**: ?service=checkout-api&range=1h
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: selected error span; missing optional AI content stated explicitly; preserved back search context; header and filters kept; the waterfall scrolls while the graph and span tabs stack
- **Links**: Back to Traces → /t/acme/observe/traces?service=checkout-api&range=1h; card → /t/acme/cards/card_service_01; span row → the same route with that span selected
- **Responsive pattern**: R-DETAIL

#### M-06 — Eval detail (mobile)

- **Route**: `/t/acme/observe/evaluations/eval_record_01?task=groundedness`
- **URL params**: ?task=groundedness
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: Workflow and Task presented as two views of one event; passed, failed, error and skipped task rows; the selected task preserved in the URL; unauthorized actual-value redaction; correlation and technical detail stacked after the summary
- **Links**: Task/Workflow view → same route ?view={workflow|task}; task row → same route ?task={name}; trace → /t/acme/observe/traces/trace_01; card → /t/acme/cards/card_eval_01
- **Responsive pattern**: R-EVAL

#### M-07 — Drift (mobile)

- **Route**: `/t/acme/observe/drift?driftCard=card_drift_01&feature=score&range=30d`
- **URL params**: ?driftCard=card_drift_01&feature=score&range=30d
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: all three filters visible as chips; the calculated report selected; feature pass and fail rows; a threshold-breach alert; a labeled no-report gap in the range; the calculated-report versus raw-observation distinction stated; dense rows scroll
- **Links**: chip ⨯ → the same route without that filter; feature row → same route ?feature={name}; Drift Card → /t/acme/cards/card_drift_01; Model → the related Model Card
- **Responsive pattern**: R-DRIFT

#### M-08 — Query (mobile)

- **Route**: `/t/acme/query`
- **URL params**: none
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: catalog drawer placed above the editor and results; Run and Cancel retained; the succeeded execution state with time, rows and bytes; all three result tabs kept; two-axis result overflow contained inside the Results panel; the Menu carries navigation only, never page actions
- **Links**: Run → executes in place; Cancel → stops the running execution; tabs → Results / Query details / History; catalog drawer → selects a table and inserts it into the editor
- **Responsive pattern**: R-QUERY

#### M-09 — Agent Prompt inspection (mobile)

- **Route**: `/t/acme/cards/card_agent_01`
- **URL params**: ?inspect=prompt
- **Viewport**: 390 × 844 (light + dark)
- **Fixture**: tenant `acme`
- **Material states**: the C-07 Prompt selection as a full-width raised sheet with explicit ✕ Close and ‹ Back — roles, content types, variables, provider/model and the direct Prompt link reachable without page-level horizontal overflow; dense raw definition scrolls inside its own region
- **Links**: ✕ Close / ‹ Back → /t/acme/cards/card_agent_01; Open Prompt Card → /t/acme/cards/card_prompt_01
- **Responsive pattern**: R-CARD (contextual full-width sheet, realizes C-07 Prompt inspection)

### `cards/experiment/` — detailed Experiment workspace package

The 21-page realization of the C-06 workspace: one Experiment Card route
(`/t/acme/cards/card_experiment_01`) whose entire state lives in query
parameters, so every view is URL-restorable and shareable. Subnavigation is
exactly Overview · Runs · Compare · Outputs · Versions. All desktop pages are
1440 × 1024, pattern R-EXP; the package's `mobile.svg` pages are 390 × 844.
Fixture: tenant `acme`, `checkout-ranking-study` v4 ACTIVE, authored
`summary_metrics` ndcg@10 with `best_run_ref` run_07, 12 runs. Ranking is a
viewer act: sort/rank direction lives in the URL (`sort=`/`rank=`/`dir=`),
never on the Card — an Experiment declares no objective. Mock runtime
values are always labeled server/Vala/Bifrost projections or mock fixtures —
never Card declaration fields.

#### E-01 — Overview, active Experiment (`overview.svg`)

- **URL params**: `?view=overview`
- **Material states**: authored summary — spec `summary_metrics` (ndcg@10 0.742, auc, logloss) and the author-pinned `best_run_ref` run_07, labeled declaration content and never a derived winner; run counts 7 completed / 1 running / 1 queued / 2 failed / 1 cancelled with glyph+text badges; run_10 failure attention band; recent activity (5 of 12); empty comparison stated as an earned absent; purpose/targets, authored definition and metadata rails
- **Links**: run_07/run_10/run_11 → `?view=runs&run={id}&section=summary`; targets/inputs/eval → /t/acme/cards/{uid}; All runs → `?view=runs`; Compare → `?view=compare&runs=run_07,run_09&baseline=run_07`

#### E-02 — Overview, no runs (`overview.svg`)

- **URL params**: `?view=overview`
- **Material states**: zero run records; declaration, targets and default parameters fully preserved; result, activity, outputs and comparison stated as absent (not zero, not loading); no execution button invented — the page states how runs arrive (SDK/CLI/Trigger) without offering to start one
- **Links**: targets/inputs/eval → /t/acme/cards/{uid}; subnavigation remains fully routable

#### E-03 — Runs inventory (`runs.svg`)

- **URL params**: `?view=runs&sort=ndcg_10&dir=desc` (filter demo: `&status=running&type=training`)
- **Material states**: dominant 12-run leaderboard viewer-sorted by ndcg@10 ▾ (`sort=ndcg_10&dir=desc` in the URL, never Card truth; runs without the sorted metric sort last by recency) — 7 completed, 1 running (workflow), 1 queued, 2 failed, 1 cancelled — with selection checkboxes, kind, lifecycle glyph+text, started/duration, initiator, source, LR, ndcg@10 and logloss metric columns plus a metrics ▾ column chooser, output count and target; run_07 selected-row highlight; a ＋ add filter affordance names the richer vocabulary (metric ≥/≤, parameter, source revision, data version, initiator); the URL-filter-state inset shows active chips producing a true zero-result state that replaces the inventory (run_11 is running but is a workflow run) with clear-filter restore — filtered chips never sit above unfiltered rows; 2-run selection enables Compare
- **Links**: run row → `?view=runs&run={id}&section=summary`; Clear filters → `?view=runs`; Compare selected → `?view=compare&runs=run_07,run_09&baseline=run_07`

#### E-04 — Queued/running run (`runs.svg`)

- **URL params**: `?view=runs&run=run_11&section=summary`
- **Material states**: running agentic workflow run at stage tool-loop with a 7-stage lifecycle timeline; partial metrics/traces/files labeled live/so-far; pending outputs never rendered as zero or failure; recorded lifecycle event log; queued run_08 inset (no stage, duration or outputs exist yet); no execution controls
- **Links**: Follow live traces → `&section=traces`; linked Agent/Prompt/Eval/Service → /t/acme/cards/{uid}

#### E-05 — Failed and cancelled inspection (`runs.svg`)

- **URL params**: `?view=runs&run=run_10&section=summary`
- **Material states**: failed at stage train with the safe `WYRD-RUN-RESOURCE-EXHAUSTED` summary; stage strip queued/setup/train (failed)/eval (not reached); retained partial metrics through step 1,400 (never extrapolated) and kept logs/checkpoints; cancelled run_12 presented separately with its recorded reason — cancellation is not failure; retry/mutate stated absent by design
- **Links**: retained files → `?view=outputs&type=files&run=run_10`; run_12 → `?view=runs&run=run_12&section=summary`; source/Observe links in the rail

#### E-06 — Completed model-training run (`runs.svg`)

- **URL params**: `?view=runs&run=run_07&section=summary`
- **Material states**: completed + pinned-best badges (the authored `best_run_ref`, stated as declaration content, not a derived winner); train/validation ndcg@10 history with dash+marker series; executed parameters vs declared defaults; output inventory with per-type counts (14 metrics · 2 tables · 5 visuals · 9 files · 2 artifacts); consumed/produced/deployed Card links; run-section destinations Summary/Metrics/Tables/Visuals/Files/Artifacts/System/Provenance — no disabled tabs
- **Links**: sections → `?view=outputs&type={t}&run=run_07` and `&section={s}`; Cards → /t/acme/cards/{uid}; Compare → `?view=compare&runs=run_07,run_09&baseline=run_07`

#### E-07 — Completed agentic/workflow run (`runs.svg`)

- **URL params**: `?view=runs&run=run_11&section=summary`
- **Material states**: completed design projection of the same run_11 fixture shown running in E-04 (identity unchanged); sessions/tool calls/tokens/cost/p95/errors; labeled per-stage time bars; Eval quality 0.87 PASS projection; sections gain Traces and Evaluations; tables/visuals/hardware sections are not rendered rather than shown empty
- **Links**: Traces → `&section=traces`; Evaluations → `&section=evaluations`; Agent/Prompt/Workflow/Service/Eval Cards → /t/acme/cards/{uid}; canonical exploration → /t/acme/observe

#### E-08 — Compatible run comparison (`compare.svg`)

- **URL params**: `?view=compare&runs=run_07,run_09&baseline=run_07&rank=ndcg_10&dir=max`
- **Material states**: run_07 marked baseline; the table compares Card identities explicitly — declaration version (v4/v4), eval population (holdout · cutoff 0.42), source revision, image digest and input Data version as linked Card rows — alongside parameter/environment rows flagged differs/same; better/worse renders only under the viewer-selected lens (`rank=ndcg_10&dir=max` in the URL) — every other delta stays neutral and no metric is privileged by the Card; the aligned-history panel carries a restrained metric selector (ndcg@10 ✓ · logloss · val gap · p95_infer_ms — any recorded history; direction comes from the lens) with histories aligned by recorded step and the shorter run_09 ending at step 1,400 (never stretched or index-aligned); a full-width COMPATIBILITY BASIS panel states the invariants held by run_07/run_09 (declaration v4, kind training, eval population holdout, input Data, target, completed with recorded metrics) and names each exclusion with its reason (run_11 workflow kind — no step-aligned training metrics; run_05/run_10 failed with no final metrics); run selection editable via chips without any dashboard builder; per-run output difference summary
- **Links**: chips edit `runs=`/`baseline=`; metric chips → `&metric={name}`; Card rows → /t/acme/cards/{uid}; per-run outputs → `?view=outputs&type=metrics&run={id}`; run ids → `?view=runs&run={id}&section=summary`

#### E-09 — Metrics output (`outputs.svg`)

- **URL params**: `?view=outputs&type=metrics&run=run_07&metric=ndcg_10`
- **Material states**: 14-metric selector with scalar/history kinds; step-domain chart with dash+marker series, max-recorded positional marker, and an explicit recorded gap at step 200 (never zero-filled or index-aligned); exact-values escape hatch listing the last 5 recorded steps with timestamps (max value flagged at its recorded step)
- **Links**: metric row → `&metric={name}`; output tabs → `&type={t}`; run chip → `&run={id}`

#### E-10 — Table-metric output (`outputs.svg`)

- **URL params**: `?view=outputs&type=tables&run=run_07&table=validation-errors`
- **Material states**: validation-errors (1,204 rows · 6 typed columns) with schema panel, category search, score-descending server sort, page 1/27 pagination, and a selected rank-swap row expanded with label/prediction/score/source lineage; structured runtime output — not a Card
- **Links**: table list → `&table={name}`; source_row → txns-2026q3 (Data) Card; output tabs → `&type={t}`

#### E-11 — Visual output (`outputs.svg`)

- **URL params**: `?view=outputs&type=visuals&run=run_07&visual=confusion-matrix`
- **Material states**: 5-item labeled gallery (learning curve, confusion matrix, calibration, feature importance, score distribution), each item carrying a distinct representative thumbnail so the gallery scans visually; focused confusion matrix with title/caption/media/dimensions/created/producing run and download/open; uploaded stored visuals explicitly distinguished from UI-derived metric charts
- **Links**: gallery item → `&visual={name}`; Download/Open on the stored object; output tabs → `&type={t}`

#### E-12 — Files output (`outputs.svg`)

- **URL params**: `?view=outputs&type=files&run=run_07&path=logs/train.log`
- **Material states**: lazy tree (collapsed directories load on expand); selected `logs/train.log` preview with path/type/size/digest/producing run; sanitized text only — HTML is never rendered; `ranker.onnx` shown as an explicit no-inline-preview state with download and digest
- **Links**: tree node → `&path={path}`; Download/Copy path affordances; output tabs → `&type={t}`

#### E-13 — Registered Artifact output & lineage (`outputs.svg`)

- **URL params**: `?view=outputs&type=artifacts&run=run_07`
- **Material states**: 2 registered Artifact Cards (ranker-model v12, holdout-report v3) each keeping uid/version/status and a direct Card route; lineage strip Data → run_07 → Artifact → Model → Service; run_09 shown with files but no Artifacts — files are never called artifacts; the association contract is flagged as deferred gap #9
- **Links**: Artifact rows → /t/acme/cards/card_artifact_{02,05}; lineage nodes → their Card routes; run_09 files → `&type=files&run=run_09`

#### E-14 — System measurements (`outputs.svg`)

- **URL params**: `?view=runs&run=run_07&section=system`
- **Material states**: three decision-useful charts (CPU %, accelerator %, memory GiB) sharing one 41-minute domain with setup/train/eval stage bands; 10 s sampling context; positional memory-peak marker at 19.8 of 24 GiB; network/disk explicitly omitted as not decision-useful — no fixed four-chart template
- **Links**: sections → `&section={s}`; environment detail in the rail

#### E-15 — Provenance and reproducibility (`outputs.svg`)

- **URL params**: `?view=runs&run=run_07&section=provenance`
- **Material states**: full reproducibility record — initiator/CLI, repo + commit + branch, clean dirty-state indicator, python/sdk/cuda versions, container image digest, uv.lock digest, entry point, executed parameters, exact input Card versions and produced Model/Artifact lineage; secrets and unrestricted env vars never appear; the run-association contract is flagged as deferred gap #8
- **Links**: Source/Data/Model/Artifact/declaration → Card routes; compare provenance → `?view=compare&runs=run_07,run_09&baseline=run_07`

#### E-16 — Agentic trace inspection (`outputs.svg`)

- **URL params**: `?view=runs&run=run_11&section=traces&trace=tr_2201&span=sp_09`
- **Material states**: 6-session inventory with the erroring trace selected; execution tree distinguishes agent/llm/tool/retrieval/error spans by text label + kind bar + waterfall position (never color alone); selected retry span shows latency/attempt, truncated input, error output and progressive disclosure; full payloads remain in Observe
- **Links**: trace row → `&trace={id}`; span row → `&span={id}`; canonical detail → /t/acme/observe/traces/tr_2201

#### E-17 — Agentic evaluation inspection (`outputs.svg`)

- **URL params**: `?view=runs&run=run_11&section=evaluations&task=groundedness`
- **Material states**: aggregate 0.87 PASS projection with 6 task verdicts (5 pass / 1 fail); failed groundedness task selected with example input/expected/actual/judge note; the definition stays on the Eval Card and canonical exploration stays in Observe — both explicitly linked, neither duplicated
- **Links**: task row → `&task={name}`; quality v2 → /t/acme/cards/card_eval_01; example span → `&section=traces&trace=tr_2201&span=sp_11`; Observe evaluation run ↗

#### E-18 — Experiment Card versions (`versions.svg`)

- **URL params**: `?view=versions&version=v3`
- **Material states**: v1–v4 immutable declarations with status/created/author/change summary; v3 selected showing target additions (+Agent/+Prompt/+Eval) and default diff (hold_cutoff 0.38 → 0.42); the explicit identity rule that run transitions and newly arriving outputs never create Card versions; run-to-version association listed per version
- **Links**: version row → `&version={v}`; View declaration → `?view=overview&version=v3`

#### EM-01 — Run inventory and filters (mobile) (`cards/experiment/mobile.svg`)

- **URL params**: `?view=runs&sort=ndcg_10&dir=desc` (state inset: `&status=running&type=training`) · realizes R-EXP at 390 × 844 · desktop source E-03
- **Material states**: Experiment identity/version/status preserved; horizontally scrollable subnav; viewer-sorted run list (`sort=ndcg_10&dir=desc` in the URL) keeps lifecycle glyph+text, kind, metric value and selection, with a ＋ add filter affordance; the active-filter state lives in a dashed STATE INSET whose chips produce a true zero-result state that replaces the list (never rendered above unfiltered rows) with clear-filter restore; hidden columns stated reachable through run detail — never silently dropped
- **Links**: run row → `?view=runs&run={id}&section=summary`; Compare → `?view=compare&runs=run_07,run_09&baseline=run_07`; Clear → `?view=runs`

#### EM-02 — Selected run inspection (mobile) (`cards/experiment/mobile.svg`)

- **URL params**: `?view=runs&run=run_11&section=summary` · realizes R-EXP at 390 × 844 · desktop source E-04
- **Material states**: visible ‹ Back to Runs; running lifecycle with stage list; partial metrics labeled so-far/not-final; linked Agent/Prompt/Eval Cards; applicable section chips only
- **Links**: Back → `?view=runs`; sections → `&section={s}`; linked Cards → /t/acme/cards/{uid}

#### EM-03 — Output inspection (mobile) (`cards/experiment/mobile.svg`)

- **URL params**: `?view=outputs&type=tables&run=run_07&table=validation-errors` · realizes R-EXP at 390 × 844 · desktop source E-10
- **Material states**: visible ‹ Back to Outputs; producing run identified; the irreducibly wide table gains an explicit horizontal scroll region (source_row reachable) with pagination — no page-width overflow; selected row keeps its typed detail
- **Links**: Back → `?view=outputs&type=tables&run=run_07`; row → row selection; page controls → server-side pages

### `cards/service/` — detailed Service operational workspace package

The 9-page realization of the C-09 operational workspace: one Service Card
route (`/t/acme/cards/card_service_01`) whose entire state lives in query
parameters. The Service-local primary subnavigation is exactly
Overview · Composition · Definition — Overview is the default and dominant;
version selection stays in the shared Card header and never becomes a tab.
Desktop pages are 1440 × 1024, pattern R-SVC; the package `mobile.svg` pages
are 390 × 844. Fixture: tenant `acme`, `checkout-api` (`card_service_01`) v12,
`range=1h`, now 15:42; components ranker (Model v12), checkout-agent (Agent
v6), fraud-review (Agent v2), ranker-shadow (Model v11), runtime (Workflow
v1); quality wiring ranker → model-drift (Drift v3, PSI 0.27 > 0.20 breach on
feature `feature`, baseline txns-2026q3) and checkout-agent →
checkout-agent-eval (Eval v4, pass 96.4%).

URL-state contract (the UI interaction contract, not a new durable server
API):

```text
/t/acme/cards/card_service_01?view=overview&version=v12&range=1h
/t/acme/cards/card_service_01?view=composition&version=v12
/t/acme/cards/card_service_01?view=definition&version=v12
```

State separation is absolute — four channels, never one collapsed badge:
Card state (labeled `card state`, registry lifecycle, never runtime health);
deployment state (shown only with source/freshness, here an explicit mock
server projection, otherwise Unknown/absent); operational state
(server-projected plain language plus glyph and its reasons — the browser
never calculates one from KPIs, an active Card, or alert presence); and data
freshness (always visible beside the assessment; missing/stale/unauthorized/
partial/failed data is never zero and never healthy). The selected version and
visible range scope every operational value; selecting historical `v11`
rescopes both declaration and observations and renders an explicit no-data
state rather than v12 health.

Overview is a signal-driven mini-dashboard in four layers: a compact
assessment strip (never the dominant body), a dominant operating dashboard of
six chart slots sharing the exact selected range (trends are the evidence —
latest value, unit, freshness and source on every chart; budgets, SLOs and
thresholds as positional lines; multi-series latency distinguished by dash and
marker as well as color; gaps stay gaps), published Drift/Eval intelligence
projected from the selected version's declaration bindings with score trends
against visible thresholds, and one supporting context band (server-ordered
attention naming the affected component, component chips, state-explaining
activity, inline scope-preserving investigations). Stale, partial,
unauthorized and failed states appear at the affected chart, never as a page
takeover; custom charts are opt-in (one saved chart plus a restrained Add
chart empty slot — configuration contract deferred, see the gaps below).

Canonical investigation stays owned by Observe — the workspace links out with
scope preserved (`service`, `serviceVersion`, `range` arrive as visible
removable filters; ordinary browser Back returns; View all removes inherited
scope). There is no Alerts route, no Service-owned signal route tree, and no
alert resource: each attention item links to its originating canonical signal.
Overview answers what is happening now; Composition answers how the Service is
assembled, measured and wired for reaction (declaration only — a declared edge
never proves a runtime execution or observation occurred); Definition answers
what the exact declaration and runtime wiring define.

#### S-01 — Service Overview · healthy (`overview.svg`)

- **URL params**: `?view=overview&version=v12&range=1h`
- **Material states**: the same mini-dashboard with nothing wrong — ✓ OPERATING NORMALLY as a server projection with current evidence: error trend well under budget, latency around target, availability above SLO, drift PSI 0.11 stable under 0.20, eval passing; attention is an earned absent stated from current signal data — healthy is never inferred from an active Card or from missing data
- **Links**: identical to C-09 — state changes what the charts honestly show, never the dashboard structure or navigation

#### S-04 — Service Overview · stale / no recent data (`overview.svg`)

- **URL params**: `?view=overview&version=v12&range=1h`
- **Material states**: observations stop at 15:17 — every chart's series ends in a labeled gap (`— · last 15:17` / "gap — not zero"), freshness ▲ 25m OLD, deployment ○ UNKNOWN, attention ◔ cannot evaluate (unknown is not none), components all ◔ stale, drift/eval calculations clearly aged; nothing is zeroed, carried forward, or inferred healthy
- **Links**: identical to C-09; Observe investigations stay available over the historical window

#### S-05 — Service Overview · partial · authorization (`overview.svg`)

- **URL params**: `?view=overview&version=v12&range=1h`
- **Material states**: drift unauthorized for j.reyes — the drift intelligence card renders NOT AUTHORIZED (requires `observe:drift`; subject identity stays visible; never zero or healthy) and the inline Drift investigation names its required scope; operational ◑ PARTIAL names the hidden signal; attention marked possibly incomplete; every authorized chart and the eval card stay current and usable
- **Links**: identical to C-09 minus the unauthorized Drift investigation, which names its required scope

#### S-06 — Service Overview · safe backend failure (`overview.svg`)

- **URL params**: `?view=overview&version=v12&range=1h`
- **Material states**: the operational projection failed (`WYRD_OBS_504_QUERY_TIMEOUT` · `wy_req_9f2`) — each affected chart slot renders a named NOT LOADED state; Retry in the assessment strip reloads only the failed charts; ✕ LOAD FAILED is a load error, not a health verdict; independently sourced drift/eval intelligence, attention and context remain rendered; Card identity, header and subnav unaffected
- **Links**: identical to C-09; Retry reloads only the failed operational content

#### S-07 — Service Overview · historical v11 selected (`overview.svg`)

- **URL params**: `?view=overview&version=v11&range=1h`
- **Material states**: version scoping demonstrated in the rendering, not prose — the header chip reads `v11 (historical) ▾` and the URL carries `version=v11`; every chart slot renders explicit NO v11 OBSERVATIONS IN RANGE, intelligence states that no v11 calculation or result exists (never inferred from v12 results), attention is unknown, saved custom charts state that none apply to v11, and components defer to the v11 declaration
- **Links**: identical structure; Definition → `?view=definition&version=v11`

#### S-02 — Service Composition (`composition.svg`)

- **URL params**: `?view=composition&version=v12`
- **Material states**: the accepted deterministic R2 graph moved intact to this selected destination as the dominant work region — inputs/definitions → runtime composition → measurement → reaction lanes; every node keeps kind, human name, version, status and its own Card route; capture-review renders once with two authored aliases; publication edges read publishes-to · through checkout-api v12, never global ownership; the Drift → Trigger → Operator → Workflow reaction chain stays labeled mock-only; model-drift selected with the raised drawer preserving graph context; declaration only — never claims a runtime execution or observation occurred and repeats no operational KPI
- **Links**: node → /t/acme/cards/{uid}; drawer → /t/acme/cards/card_drift_01 and /t/acme/observe/drift?driftCard=card_drift_01&service=checkout-api; subnav → `?view={overview,composition,definition}`

#### S-03 — Service Definition (`definition.svg`)

- **URL params**: `?view=definition&version=v12`
- **Material states**: declaration-first and human-readable before wire detail — purpose, entry_point (`acme.checkout.app:app`, imported by the deploy image, never by Wyrd) and derived runtime identity; seven runtime-aliased component references with versioned Cards (two aliases → one capture-review Card, distinct occurrences, one identity); publication bindings split by subject semantics: component-level `components[].publishes_to` (subject is the component's Card, observed through checkout-api v12: ranker → model-drift, checkout-agent → checkout-agent-eval) versus service-level `publishes_to` (subject is the Service itself — declared absent for v12, stated not hidden); principal bound on first deploy contact plus composed Policy checkout-guardrails; server-managed metadata; server-derived relationships; immutable version list with the v11 rescope rule; the raw typed spec is closed by default behind a real ▸ disclosure row (line count, validation, Copy YAML) and expands inline; credentials and secret values never appear
- **Links**: components/publication/policy/reaction → /t/acme/cards/{uid}; versions → `?version={v}`; subnav → `?view={overview,composition,definition}`

#### SM-01 — Service Overview at narrow width, detailed (`mobile.svg`)

- **URL params**: `?view=overview&version=v12&range=1h`
- **Material states**: the detailed narrow dashboard — the four standard charts render full-width with axes, positional budget/SLO/target markers and dash+marker-distinguished p50/p95/p99, all sharing the exact 1h range; drift/eval intelligence keeps score trends, subjects and calculation freshness; attention (with the affected component), component chips, activity and the five investigations stack below — scrolling, never dropping
- **Links**: identical to M-02 — chart rows and intelligence link to canonical Metrics/Traces/Drift/Evaluations with scope preserved

#### SM-02 — Service Composition at narrow width (`mobile.svg`)

- **URL params**: `?view=composition&version=v12`
- **Material states**: the complete accepted graph as ordered relationship lanes — runtime composition, inputs/definitions, measurement, reaction — every node and edge label preserved, lanes scroll rather than dropping content; selected model-drift opens the full-width raised sheet with an explicit ‹ Back to Composition; mock-only reaction continuation stays labeled; declaration only
- **Links**: nodes → /t/acme/cards/{uid}; sheet → /t/acme/cards/card_drift_01; nav chips → `?view={overview,composition,definition}`

### `changes/` — Change Request review workspace package

The 10-page realization of the CR-01…CR-07 review workspace. The contact
sheet `changes.svg` and this package are drawn from the same builders — there
is never a second divergent design; the package adds the three critical
narrow-width flows. Desktop pages are 1440 × 1024; `changes/mobile.svg` pages
are 390 × 844. Fixture: tenant `acme`, `change_01` "Raise checkout ranking
cutoff for high-risk carts", immutable revision 7, opened by r.okafor,
3 subjects in 2 repositories (stacked PRs #4412/#4413 in acme/checkout plus
#221 in acme/ranking; identity is always the exact base → candidate commit
pair), 3 required Claims, verification 1/3 satisfied, approval 1/3, CLAIM-3
override authorized (does not verify).

Shared-header contract — CRW-03 through CRW-07 (and CR-03…CR-07) carry one
structurally identical Change header: title and intent identity, change id ·
immutable revision · author · age · subject/repository count, four separately
labeled state channels (lifecycle, verification, approval, override — never
conflated), one server-projected next-action sentence, the permission-aware
`Review changes ▾` action with Close Change Request in the ⋯ overflow, and
the exact five-item local navigation `Overview · Verification · Review ·
Timeline · Subjects`. `Subjects` is not a new route — it links to the
Overview `#subjects` section, and each selected subject keeps
`/changes/change_01/subjects/{subjectId}`.

Locked interaction rules the package demonstrates:

- **Truthful attention** (CRW-01): a view shows only its matching records;
  every row carries a server-projected reason and expected actor; empty,
  unauthorized and failed states never leak the unfiltered list.
- **Creation resolves to exact identity** (CRW-02): a PR URL resolves into
  repository · provider · PR · exact base/candidate commits; the URL is input
  convenience, never truth; drafts save incomplete at any point.
- **Run-action integrity** (CRW-04): Not ready exposes its prerequisite,
  never Run now; running or queued work links and cannot start again; Run
  now/Rerun always sit behind an explicit billable confirmation; approval and
  override never turn a failure into a pass.
- **One submitted decision** (CRW-05): review ends in one submission —
  Comment, Approve, or Request changes — stated against the exact revision;
  approval is human judgment, recorded separately from every Verifier result.
- **Source discussion without source authority** (CRW-07): line-anchored
  discussions are Wyrd revision-aware anchors; there is no edit, merge,
  branch mutation, suggested-change application or persisted Viewed state —
  providers keep source editing and merge authority.

#### CRW-01 — Change attention inbox (`inbox.svg`)

- **Route**: `/t/acme/changes` · **URL params**: `?view=needs-attention`
- **Material states**: identical to CR-01 — the truthful subset, per-row reason + expected actor, no-match/unauthorized/safe-error without fallback, and the reason-vocabulary and search-scope contract panels
- **Links**: identical to CR-01
- **Responsive pattern**: R-LIST

#### CRW-02 — New Change Request (`new.svg`)

- **Route**: `/t/acme/changes/new` · **URL params**: none
- **Material states**: identical to CR-02 — the operable progressive form with PR-URL resolution, repairable invalid subject, Claim/Verifier controls, billable warning and ever-present Save draft
- **Links**: identical to CR-02
- **Responsive pattern**: R-FORM

#### CRW-03 — Change Overview (`overview.svg`)

- **Route**: `/t/acme/changes/change_01` · **URL params**: none
- **Material states**: identical to CR-03 — shared header, server-projected action summary, #subjects target, derived Claim resolution and separated rail decisions
- **Links**: identical to CR-03
- **Responsive pattern**: R-CHANGE

#### CRW-04 — Change Verification (`verification.svg`)

- **Route**: `/t/acme/changes/change_01/verification` · **URL params**: none
- **Material states**: identical to CR-04 — honest not-ready/verifying/queued/failed/stale actions with the billable Rerun confirmation
- **Links**: identical to CR-04
- **Responsive pattern**: R-CHANGE

#### CRW-05 — Change Review (`review.svg`)

- **Route**: `/t/acme/changes/change_01/review` · **URL params**: none
- **Material states**: identical to CR-05 — composer, inline reply, stale-revision recovery and the Submit review decision panel
- **Links**: identical to CR-05
- **Responsive pattern**: R-CHANGE

#### CRW-06 — Change Timeline (`timeline.svg`)

- **Route**: `/t/acme/changes/change_01/timeline` · **URL params**: none
- **Material states**: identical to CR-06 — one chronology, typed destinations, audit/discussion separation under the shared header
- **Links**: identical to CR-06
- **Responsive pattern**: R-CHANGE

#### CRW-07 — Change subject review (`subjects.svg`)

- **Route**: `/t/acme/changes/change_01/subjects/subject_api` · **URL params**: `?file=src/capture/rank.rs`
- **Material states**: identical to CR-07 — file navigation with position, anchored threads, the revision-aware Start-discussion composer and the persistent review action on a read-only diff
- **Links**: identical to CR-07
- **Responsive pattern**: R-DIFF

#### CRWM-01 — Change creation at narrow width (`mobile.svg`)

- **Route**: `/t/acme/changes/new` · **Viewport**: 390 × 844 (light + dark)
- **Material states**: every creation section stacks — fields, three subject rows with inline error and Repair, add-subject actions, collapsed-but-present Claims, billable warning, progress, sticky Save draft / Discard footer
- **Responsive pattern**: R-FORM

#### CRWM-02 — Change review at narrow width (`mobile.svg`)

- **Route**: `/t/acme/changes/change_01/review` · **Viewport**: 390 × 844 (light + dark)
- **Material states**: mini shared header with separate state badges and next-action line; composer; open thread with reply and stale recovery; resolved thread with Reopen; full Submit review panel with all three decisions and the exact revision
- **Responsive pattern**: R-CHANGE

#### CRWM-03 — Subject review at narrow width (`mobile.svg`)

- **Route**: `/t/acme/changes/change_01/subjects/subject_api` · **Viewport**: 390 × 844 (light + dark)
- **Material states**: identity and exact commits in the header; file chips with position and Prev/Next; the diff scrolls sideways in place with both anchors; a line-anchored Start-discussion composer; provider link; sticky Review changes action; no edit or merge
- **Responsive pattern**: R-DIFF

## Responsive pattern matrix

Every one of the 66 desktop pages is assigned exactly one pattern.

| Pattern | Narrow behavior | Desktop pages |
|---|---|---|
| R-ENTRY | Centered identity/choice, no tenant shell | H-01 |
| R-HOME | Mobile header/menu; summaries stack | H-02 |
| R-LIST | Accessible filters; labeled rows or controlled table overflow | C-01, O-06, O-08, CR-01, CRW-01 |
| R-CARD | Primary Card first; metadata/relationships stack | C-02, C-03, C-04, C-05, C-06, C-07, C-08, C-09, C-10, C-11, C-12, C-13, C-14 |
| R-SIGNAL | Visible time/filters; trend then records then detail | O-01, O-02, O-03, O-04 |
| R-DETAIL | Identity/filters first; dense visual scrolls; rail stacks | O-05, O-07 |
| R-EVAL | Workflow/Task are views of the same event | O-09 |
| R-DRIFT | Definition/results stack without merging | O-10 |
| R-FORM | One scrolling form; safe actions remain reachable | CR-02, CRW-02 |
| R-CHANGE | Plain summary first; technical sections stack | CR-03, CR-04, CR-05, CR-06, CRW-03, CRW-04, CRW-05, CRW-06 |
| R-DIFF | Subject first; commits/files/diff use controlled overflow | CR-07, CRW-07 |
| R-QUERY | Catalog drawer; results controlled overflow | Q-01 |
| R-EXP | Card identity + horizontally scrollable subnav persist; the dominant work region stacks above rails; irreducibly wide run/output tables scroll inside their own panel; hidden columns stay reachable through run detail | E-01 … E-18 |
| R-SVC | Card identity, exact version, visible range and the three-item local nav persist; assessment and attention precede compact evidence; tables become labeled rows; composition renders as ordered lanes that scroll; selected-node detail becomes a full-width raised sheet | S-01, S-02, S-03, S-04, S-05, S-06, S-07 |

Mobile artboards realize these patterns at 390 × 844:

| Mobile page | Realizes | Desktop source |
|---|---|---|
| M-01 | R-LIST | C-01 |
| M-02 | R-SVC | C-09 Service Overview |
| M-03 | R-FORM | CR-02 |
| M-04 | R-CHANGE | CR-05 |
| M-05 | R-DETAIL | O-05 |
| M-06 | R-EVAL | O-09 |
| M-07 | R-DRIFT | O-10 |
| M-08 | R-QUERY | Q-01 |
| M-09 | R-CARD (full-width contextual sheet) | C-07 Prompt inspection |
| SM-01 | R-SVC | C-09 Service Overview |
| SM-02 | R-SVC | S-02 Composition |
| CRWM-01 | R-FORM | CRW-02 |
| CRWM-02 | R-CHANGE | CRW-05 |
| CRWM-03 | R-DIFF | CRW-07 |
| EM-01 | R-EXP | E-03 |
| EM-02 | R-EXP | E-04 |
| EM-03 | R-EXP | E-10 |

Mobile navigation follows REQ-098, which intentionally overrides the generic
brand example that stacks the sidebar and forbids a hamburger: the closed shell
carries a labeled `Menu` button, and M-01 shows a compact inset of the open
disclosure with the same five links, the current-area state, tenant identity and
a close affordance. No mobile-only noun is introduced, and the Menu never
carries a page action.

## Material states index

Where each required state is demonstrated.

| Artboard | Panel / inset that demonstrates the required states |
|---|---|
| H-01 | zero tenants → access/provisioning panel (Outcome A); unauthorized result inset in Outcome A; one tenant → 302 redirect panel (Outcome B); multiple tenants → recent-first chooser (Outcome C) |
| H-02 | ordinary single-tenant shell (static tenant in topbar); authorized tenant-switcher variant and attention empty state described in the annotation strip |
| C-01 | populated inventory (6 of 42 shown); loading skeleton, no-matching-results and safe structured error occur at the table position (described in the annotation strip); removable, direct-restorable and cleared filters described in the Filter behaviour rail |
| C-02 | demonstrated on a Policy fixture (settlement-guardrails) — a kind with no dedicated detail page, so the shared shell is its only rendering; current version v7 selected in the Version table; prior version selection rows v6/v5; relationship empty state and safe Spec error occur at their section positions (described in the annotation strip) |
| C-03 | populated schema, profile, distribution, splits and targets; earned absent optional section — no figures attached — noted below the splits table |
| C-04 | populated task/interface, signature, artifacts, deployment relationships and linked verification; earned absent optional section — no Prompt relationship — noted in the rail |
| C-05 | ordered prompt definition dominant — system instructions separate from the conversation, explicit roles on every message, labeled text/image/tool-call content treatment (audio and file parts use the same treatment; unsupported provider parts stay discoverable in Raw definition); provider, model, operation, settings and structured response type; variables and media variables with type, required/default state and where each is used; Response schema (open) and Raw definition (closed) disclosures with copy affordance; no secrets and no execution or playground action |
| C-06 | concise workspace entry — lifecycle run counts, authored summary (summary_metrics · best_run_ref), running + needs-attention links, URL-restorable workspace destinations, latest-runs excerpt; earned absent — no attached notebooks; every detailed state lives in the `cards/experiment/` package ledger below |
| C-07 | registered Prompt as the primary linked dependency (name, version, provider/model, message and variable counts) with its raised read-only inspection drawer shown open — a CardRef reuse, never a copy; composition strip Prompt → Agent → runtime tools with component-of and publishes-to edges; run limits (max iterations, tool concurrency, recent sessions, timeout); tools remain Skald runtime registrations, never Card kinds; earned absent — no attached artifacts |
| C-08 | populated ordered stages, inputs, outputs, owners and governance; earned absent optional section — no execution results are shown on a declaration |
| C-09 | needs-attention operational mini-dashboard — compact assessment strip (four separated state channels, latest change as correlation), dominant six-slot chart grid sharing the exact range (budget/SLO/target as positional lines, dash+marker-distinguished latency percentiles, saved custom chart + restrained Add-chart slot), drift/eval intelligence with score trends vs thresholds and identified subjects, and one supporting context band; healthy/stale/partial/failure/v11 renderings live on S-01/S-04/S-05/S-06/S-07 |
| C-10 | definition inspection — dataset/source, workflow stages with dependencies, pass gate and six-task summary; groundedness selected with a raised task-definition drawer (kind, input selector, context path, operator, expected value, dependency condition, judge Agent, retry policy); selecting the workflow opens equivalent workflow-definition detail; no runtime scores or verdicts on the Card; earned absent — no subject_ref and no Eval-owned schedule |
| C-11 | Definition and Results visibly distinct — Definition (the Card): PSI method with quantile 10-bin profile, Distribution signal, feature `feature`, baseline txns-2026q3 (Data), fixed threshold 0.20, publisher checkout-api/ranker via model_primary, Trigger/Operator chain, no Drift-owned schedule; Results (Vala/Observe projection, never written to the Card): selected feature dominant, current PSI 0.27 breach, 30d series with the plotted 0.20 threshold and positional tick, baseline and sample/missingness context, last calculation, alert history, calculated-vs-raw distinction |
| C-12 | populated versioned identity, purpose, capabilities, accepted normalized Evidence kinds, result capability, owner/governance and contextual runtime binding; earned absent optional section — no secrets and no closed input world |
| C-13 | plain-language wiring sentence, exact schedule/source/filter/Operator configuration, flow strip, reachable publishing context and projected firing history separated from the declaration |
| C-14 | one Workflow action with a human-readable input template and Raw JSON disclosure; execution budget; inbound chain; redacted credential reference; separated recent outcome; Notify/HTTP variants stated, not rendered |
| O-01 | normal summary across five signals; one no-data signal (Drift, with its own rail panel); tenant-wide Attention and Recent activity feeds labeled as such (the chips scope only the signal tiles and signal pages); inherited scope shown as chips and cleared scope described in the Scope rail |
| O-02 | partial results (200 of 412 loaded) in the Records table; no-matches and safe search error occur at the records position (described in the annotation strip); selected log record expanded in the right rail |
| O-03 | successful three-series chart with dash+marker per series and underlying values; loading, no-data-series and safe query error occur at the chart position (described in the annotation strip) |
| O-04 | partial results (50 of 97 loaded); no-matches and safe search error occur at the table position (described in the annotation strip); selected trace highlighted in the table and detailed in the raised panel |
| O-05 | selected span (ledger.capture) in the rail; error span highlighted in the waterfall and service graph; missing optional AI content shown as an explicit absent section; preserved back-search context in the Back panel |
| O-06 | populated inventory with a selected row and preview; loading, empty and safe error occur at the table position (described in the annotation strip) |
| O-07 | four successful panels (two charts, one table, one bar chart with labels), one no-data panel and one safe-error panel, all inside one read-only grid; no editor or save action |
| O-08 | populated event list with a selected row; no-matches, loading and safe error occur at the table position (described in the annotation strip); filters direct-restore from the URL |
| O-09 | event page behind a 28% scrim under a 75% workflow drawer; stages completed/failed/running; task rows passed/failed/error/skipped; selected task dominant with score, threshold tick, judge explanation and authorized actual value; redaction note for unauthorized principals |
| O-10 | calculated report selected with overall verdict; per-feature pass and fail rows; threshold breach alert panel; + Filter affordance exposing the full subject/principal/Run/method/signal/verdict vocabulary; raw-observation distinction panel; the no-report-for-range alternate state described in the annotation strip |
| CR-01 · CRW-01 | truthful Needs-attention subset (3 matching rows only, including the draft change_03) with server-projected reason badges, expected actors and per-row owner; separate lifecycle/verification/claims/repos/activity columns; no-match, unauthorized and WYRD_CHANGE_502 states all keep the view and search — the unfiltered list never appears; reason-vocabulary and search-scope contract panels |
| CR-02 · CRW-02 | operable creation — PR-URL Resolve to exact commits, per-subject Edit/Remove/Repair with the inline error on acme/ranking, + Add subject/Claim/Verifier, Required and Manual/On-new-evidence selects, billable warning, editable owners, earned-absent Evidence, Save draft on an incomplete draft |
| CR-03 · CRW-03 | shared header with four labeled state channels and the next-action line; server-projected Action summary naming blockers, actors and the current user's action; #subjects table with stacked PRs plus a second repository; derived Claim resolution; immutable revision with prior links; separate Verification and Decisions rails |
| CR-04 · CRW-04 | verification state with aggregate and plain cause; not-ready exposes View missing Evidence (never Run now); ready manual exposes Run now behind billable confirmation; verifying/queued link only; failed rerunnable PII review exposes Rerun behind the raised billable confirmation; stale stays provenance; the Run-eligibility rail states the server-decided rules; override never converts a failure |
| CR-05 · CRW-05 | Change-level composer with @mention autocomplete; inline reply composer in the open thread; resolved/reopened thread with Edit history; WYRD-COMMENT-STALE-REVISION recovery preserving the draft; Submit review panel with the exact revision, optional summary and the three decisions with stated effects |
| CR-06 · CRW-06 | one chronology under the shared header with a typed destination per row; audit rows server-derived and immutable, discussion rows Review activity, never merged; the open ⋯ overflow shows the authorized Close Change Request; not an audit administration view |
| CR-07 · CRW-07 | subject identity with exact commits and stacked relationship; file list with position and Prev/Next navigation; read-only diff with the resolved 118 anchor and the open revision-aware Start-discussion composer on 121; persistent Review changes; no edit, merge or Viewed tracking |
| Q-01 | empty, running, succeeded, cancelled and failed execution in the Execution states panel; the failed sample carries the structured safe error WYRD-QUERY-COLUMN-UNKNOWN; the Safety panel states authorization, one-SELECT, timeout, function, row and byte ceilings; catalog → schema → table → column explorer with spans selected |
| M-01 | populated inventory with count, kind, version, status, space and owner retained on every stacked row; removable filter chips that scroll sideways; the open Menu disclosure shown as a compact inset |
| M-02 | the complete mini-dashboard stacked at 390 × 844 with nothing dropped — assessment strip with the latest change as correlation, four labeled state channels, six metric trend rows (latest value + freshness + link each), drift/eval intelligence with score trends and subjects, attention naming the affected component, component chips, activity and all five scoped investigations; SM-01 shows the same dashboard with full-width detailed charts; composition at this width lives on SM-02 |
| M-03 · CRWM-01 | stacked operable creation — fields, subject rows with inline error and Repair, add actions, collapsed-but-present Claims, billable warning, progress, sticky Save draft / Discard |
| M-04 · CRWM-02 | mini shared header with separate state badges and next-action line; composer; open thread with reply and stale recovery; resolved thread with Reopen; full Submit review panel with all three decisions and the exact revision |
| M-05 | selected error span; missing optional AI content stated explicitly; preserved back search context; header and filters kept; the waterfall scrolls while the graph and span tabs stack |
| M-06 | Workflow and Task presented as two views of one event; passed, failed, error and skipped task rows; the selected task preserved in the URL; unauthorized actual-value redaction; correlation and technical detail stacked after the summary |
| M-07 | all three filters visible as chips; the calculated report selected; feature pass and fail rows; a threshold-breach alert; a labeled no-report gap in the range; the calculated-report versus raw-observation distinction stated; dense rows scroll |
| M-08 | catalog drawer placed above the editor and results; Run and Cancel retained; the succeeded execution state with time, rows and bytes; all three result tabs kept; two-axis result overflow contained inside the Results panel; the Menu carries navigation only, never page actions |
| M-09 | the C-07 Prompt inspection as a full-width raised sheet with ✕ Close and ‹ Back; roles, content types, variables, provider/model and the direct Prompt link preserved; raw definition scrolls in its own region |
| CRWM-03 | narrow subject review — identity and exact commits in the header; file chips with position and Prev/Next; the diff scrolls sideways in place with both anchors; line-anchored Start-discussion composer; provider link; sticky Review changes; no edit or merge |

## Experiment fixture and output semantics

One coherent fixture backs the entire `cards/experiment/` package:

- **Identity** — tenant `acme`, Experiment Card `checkout-ranking-study`,
  uid `card_experiment_01`, `apiVersion wyrd/v1`, current version v4 ACTIVE
  (v1–v4 are immutable declarations; E-18 shows their history). Authored
  `summary_metrics`: ndcg@10, auc; `best_run_ref`: run_07 (pinned). Owner
  `m.linden`.
- **Targets and relationships** — targets ranker (Model), checkout-agent
  (Agent), checkout-api (Service); consumes txns-2026q3 (Data) and
  capture-review (Prompt); evaluation quality (Eval); produced ranker v12
  (Model) with registered Artifacts ranker-model v12 and holdout-report v3.
- **Runs** — 12: run_01…run_03 training (completed), run_04 drift_check,
  run_05 failed training, run_06 offline_eval, run_07 pinned-best training
  (ndcg@10 0.742, 41m, source a41c9e2), run_08 queued, run_09 comparison
  training (0.735, ends at step 1,400), run_10 failed training with retained
  partials (`WYRD-RUN-RESOURCE-EXHAUSTED`), run_11 running agentic workflow
  (Trigger-initiated, shown completed as a labeled design projection in E-07),
  run_12 cancelled offline_eval.
- **Output semantics** — Metrics, Tables, Visuals and Files are run-associated
  runtime outputs; registered Artifacts are Cards with their own uid, version,
  status and route, and files are never called artifacts. Every table, visual,
  file and Artifact identifies its producing run. Run lifecycle and outputs
  are server/Vala projections and never mutate the Card declaration or create
  Card versions. Recorded metric gaps stay gaps (never zero-filled); shorter
  histories end early (never stretched or index-aligned); better/worse renders
  only under a viewer-selected lens carried in the URL (`sort=`/`rank=` +
  `dir=`) — the Card declares no objective, and ranking across any recorded
  metric is the viewer's act.

## Deferred production contract gaps — Experiment workspace

The Experiment mocks intentionally get ahead of current production behavior.
These twelve gaps are recorded for later specification work; none reduces the
static design, and no invented field was placed in a Card Spec panel — mock
runtime values are labeled server/Vala/Bifrost projections or mock fixtures.

1. `ExperimentSpec` models only type, description, target refs, defaults,
   `run_refs`, summary metrics, best-run ref, Artifact refs, and free details —
   not the full workspace.
2. `RunRef` exposes only UID, `RunKind`, optional space, and labels — no
   lifecycle, timestamps, initiator, source, parameters, environment, outputs,
   or failure data.
3. Wyrd architecture has no run registry and does not yet define the
   server-owned Experiment run projection this UI needs.
4. `best_run_ref` and summary-metric ownership are not settled as authored
   declaration versus server-derived projection; the mocks draw both as
   authored declaration content and keep ranking a URL-carried viewer act.
5. No typed Experiment query service, DTO, HTTP/gRPC route, or Bifrost table
   currently supplies this workspace.
6. The typed Metrics query omits run/Card correlation from its filters and
   response even though physical observation data can carry it.
7. `experiment_id` correlation is declared in one place but is not wired into
   observation system columns, ingest, or query.
8. Files and heavy Card manifests are not associated with individual run
   records through a standard contract (surfaced on E-15).
9. Registered Artifact Cards, arbitrary files, table outputs, figures, and
   derived UI charts lack explicit run association and role semantics
   (surfaced on E-13).
10. No typed visualization descriptor defines chart/figure metadata,
    directionality, compatible comparison, or preview safety.
11. Model and agentic data can be inferred from existing metric, trace, GenAI,
    Eval, log, and observation domains, but no Experiment projection joins
    them today.
12. Run execution controls and their authorization/audit contract are
    intentionally absent and remain future work (no start/stop/retry/mutate
    affordance anywhere in the package).

## Deferred production contract gaps — Service workspace

The Service mocks intentionally project approved future UI behavior. These
eight gaps are recorded for later specification and implementation work; none
reduces the static design.

1. The current production Service/Card APIs provide no single typed Service
   operational-workspace projection joining deployment, traffic, error,
   latency, Eval, Drift, attention, component, freshness and recent-change
   context.
2. The authoritative owner and vocabulary for an aggregate operational state
   remain production integration work; the browser must not derive them.
3. Deployment state and latest-change correlation require server-owned data
   sources and may be unavailable independently from observation data.
4. Typed Observe query contracts must consistently accept and return exact
   Service Card-version correlation where the stored observation supports it.
5. A server-owned attention projection is needed to combine references to
   relevant Eval, Drift, trace and log findings without inventing an Alert
   resource or a browser-side severity model.
6. Production relationship projections must expose semantic edge kind and
   authored occurrence/path for the accepted Composition view.
7. The later Svelte/BFF Service task must reconcile its view model, URL state,
   partial authorization, stale/no-data and safe-error behavior to
   specification revision 4 before implementation.
8. Custom dashboard charts are opt-in and their configuration must not live
   only in browser state. A specification revision must establish ownership
   (tenant/user/team/Service), persistence across Service Card versions, the
   supported query and aggregation vocabulary, authorization and audit
   behavior, validation/limits/safe failure, and headless access to the same
   saved configuration. A full dashboard builder, arbitrary SQL, drag-and-drop
   layout and a dashboard marketplace are out of scope.

## Deferred production contract gaps — Change Request workspace

The Change mocks project approved future UI behavior. These seven gaps are
recorded for later specification and implementation work; none reduces the
static design.

1. The production Change Request contract and BFF projection must provide the
   authoritative attention reason, expected actor/team and current-user
   action — the browser must not infer them.
2. Pull-request URL resolution needs a server-owned provider-neutral
   operation validating authorization and returning repository, provider,
   optional PR and exact base/candidate identity; the URL never replaces the
   exact subject identity.
3. Production creation needs typed add/update/remove operations for subjects,
   Claims, Verifier requirements, owners and teams that preserve retry safety
   and incomplete drafts.
4. Review composition, replies, mentions, decisions, resolve/reopen and stale
   revision rejection require tenant-scoped transactional control-plane
   operations; mocks and Bifrost are not their authority.
5. Source discussions require a stable revision-aware anchor projection and a
   provider diff projection without granting source mutation or merge
   authority.
6. Review requirements, approval counts, blocker explanation and the next
   authorized action require server-owned projection; the UI must not derive
   a synthetic mergeability or readiness state.
7. The later Svelte/BFF Change task must reconcile its route loads, form
   actions, authorization, CSRF, focus management, drafts and safe errors to
   the accepted Change mocks before implementation.

## Approved visual grammar (Gate A)

Every artboard follows the grammar approved on the three golden screens:

- **Shell** — full-height 212px sidebar (logo tile + `bohmian` wordmark) with
  five nav entries (Home, Cards, Observe, Changes, Query); topbar to its right
  carries the WYRD product pill, mono breadcrumbs (current segment bold),
  tenant chip, principal menu and theme control. Mobile replaces the sidebar
  with an approved Menu button that carries navigation only.
- **One question per page** — one dominant work region, subordinate context in
  a right rail, one selected detail. A decision is never repeated across
  panels; human-readable names precede technical identifiers.
- **Altitude** — panels are quiet (3px hard shadow) by default; a raised panel
  (6px shadow, optional colored top bar) marks the page's single attention
  surface; subordinate blocks are flat (2px border, no shadow); earned-absent
  sections are dashed insets that say why they are empty.
- **Status** — borderless tint-pill badges pair a glyph with text; strong
  variants add a status-colored border. Selection is a brand-soft row wash
  plus a 4px brand-strong left marker.
- **Type roles** — Archivo prose, Space Grotesk titles and numerals,
  JetBrains Mono for identifiers, data, badges and nav; Fraunces only in the
  wordmark.

## Reading the sheets

- Open an `.svg` directly in a browser; each is a single scrollable page.
- Light artboards sit on the left, dark on the right, one page per row.
- The dashed strip under each artboard is its annotation: id, route, viewport,
  theme, fixture, URL params, material states, links and responsive pattern.
  Alternate states (loading, empty, safe error) are described there — the
  product viewport itself shows one coherent state, never a strip of
  demo panels.
- Status never uses colour alone: every badge pairs a glyph (`✓ ! ✕ ● ○`) with
  its text, thresholds carry a positional tick, and chart series vary dash and
  marker as well as hue.

These are a visual contract, not an implementation. No Svelte view may be built
from them until the sheets and this ledger are explicitly accepted.
