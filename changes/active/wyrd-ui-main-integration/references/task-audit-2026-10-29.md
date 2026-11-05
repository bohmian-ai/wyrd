# Remaining UI task audit — 2026-10-29

## Outcome and scope

Every remaining original task, TASK-005–026, was read against approved spec
revision 14 and its relevant canvas markup/notes. TASK-004 was inspected for
bounded source-backed remediation. Completed TASK-001–003/027 and their
immutable reviews remain historical inputs; deferred TASK-010 was checked for
scope consistency and remains deferred. This is a planning audit, not a code
review PASS, implementation result or visual acceptance.

The inspected baseline is `d00a9834c50de51629804c3ad6a508f2fb221cc8`.
Source line references below describe that baseline. The original task files
already contain human inspection gates. The defects are missed obligations,
stale authority, incomplete region/consumer mapping and proof that can establish
less than the claimed outcome. More prose alone cannot establish fidelity.

The remaining active task instructions now reference revision 14, preserve the
old migration commit as history, name board regions and distinguish behavioral
checks, visual comparison and individual human acceptance. The spec's product
decisions and completed acceptance records have not been rewritten.

## Evidence required from every affected task

For each owned component or page region, record:

- the approved board and region, plus the applicable explicit spec correction;
- the state, theme, viewport, source/browser identity and fixture provenance;
- the reference crop and implementation screenshot, with discrepancies named;
- the functional/accessibility observation and corresponding proof;
- the human's individual acceptance or the unresolved difference.

For dark populated states, compare the actual layout, typography, spacing,
colours/tokens, cells, controls and formats against the PNG and HTML region.
Light/mobile states preserve the approved structure and use the existing theme
and responsive rules; the dark-only canvas is not evidence that an invented
light design has been approved. Human inspection still covers both themes and
desktop/mobile, plus keyboard/focus, touch, long content and overflow.

A screenshot that demonstrates fit does not establish fidelity. A test PASS
does not establish human acceptance. An omitted control or region cannot be
excused as a private implementation choice. A difference needs either a fix
or an explicit human-approved exception with its scope recorded. Required
components remain unaccepted until those obligations close.

Example names and values in the canvas are not runtime classifications or
fixture provenance. Runtime fields follow current schemas; generic Observe
components do not acquire special behaviour for example span names. Preserve
expressly approved kind-specific behavior, such as the Agent invocation
statistic. Missing data, unavailable metadata, denied reads, truncation and
unsupported aggregates remain visible; no illustrative value becomes a fact.

## Per-task findings and corrections

“Corrected” below means the task instructions were repaired, not that the
task was implemented, reviewed, visually accepted or made ready despite an
unresolved prerequisite.

| Task | Audit disposition and correction |
|---|---|
| [004](../tasks/TASK-004-observe-components.md) | Existing implementation has five confirmed omission groups. [TASK-004-R1](retired-task-004/TASK-004-R1-canvas-fidelity.md) owns gallery/facet/list/waterfall/detail corrections and renewed per-component evidence. StatStrip conflict remains explicit below. |
| [005](../tasks/TASK-005-card-components.md) | Corrected whole-board-only mapping for all eleven components. Cards inventory's missing approved reference remains explicit. |
| [006](../tasks/TASK-006-query-components.md) | Corrected Query component regions and AC-016 discovery/description/layout/revision/obsolete-SQL proof. History source/lifetime remains unresolved. |
| [007](../tasks/TASK-007-verify-components.md) | Added revision-14 waiting/direct absence/exhausted queue/late-span and exact citation cases to existing executable scenario; preserved real host proof and existing execution readback. |
| [008](../tasks/TASK-008-service-components.md) | Removed contradictory unshipped `service_card_uid` premise; delivered workload/subject correlation is consumed while REQ-025 remains a separate real prerequisite. Added Service/Home breakdown and ChartEditor regions. |
| [009](../tasks/TASK-009-change-repository-components.md) | Corrected all twelve component regions and historical-reference precision. Missing REQ-029 contracts still block live inputs/actions. |
| [010](../tasks/TASK-010-ask-component.md) | No material gap found: superseded/deferred Ask and saved-view work stays excluded from implementation and dependencies. Original disposition retained. |
| [011](../tasks/TASK-011-mcp-app.md) | Corrected missing TASK-027 dependency and explicit REQ-036 layout/AC-016 host proof. Existing real agent, follow-up, actual browser and viewer-permission requirements retained. |
| [012](../tasks/TASK-012-canvas-shell.md) | Corrected shell/TimePicker regions and executable picker cases; real journey uses retained authenticated tenant route instead of an unspecified new page. |
| [013](../tasks/TASK-013-card-browsing.md) | Corrected generic detail boundaries; missing Cards inventory reference is disclosed rather than claimed matched. Inventory and detail remain separate checkpoints. |
| [014](../tasks/TASK-014-agent-page.md) | Corrected region/authority mapping and AC-017 ownership proof. Approved Agent-specific invocation semantics retained. |
| [015](../tasks/TASK-015-prompt-page.md) | Corrected Prompt message/variables/usage/version regions; comparison empty state explicit and telemetry excluded. |
| [016](../tasks/TASK-016-model-page.md) | Corrected Model region and ownership mapping; absent attributed telemetry, denied reads and recorded drift remain distinct. |
| [017](../tasks/TASK-017-data-page.md) | Corrected Data regions and serialized `splits` spelling; existing exact baseline/Verifier/requirement semantics retained. |
| [018](../tasks/TASK-018-service-page.md) | Corrected missing Deployment/relationships/Change Requests consumer closure and TASK-005/009 dependencies. REQ-029 now explicitly blocks the live CR region alongside the existing chart prerequisite. |
| [019](../tasks/TASK-019-query-page.md) | Corrected workbench controls and canonical-discovery cases; preserved declared range binds, unchanged authored SQL and existing result owners. Shares unresolved History obligation with TASK-006. |
| [020](../tasks/TASK-020-traces-page.md) | Added exact cross-workload/untagged/out-of-list-range evidence navigation, bounded failure and restored source-state proof. Names remain generic/data-driven; Main/Spans/TraceDrawer regions explicit. |
| [021](../tasks/TASK-021-logs-page.md) | Added Wrap lines, bounded paging/page size, column controls and attribute Filter/Exclude/Open trace cases. TASK-004-R1 closes known missing shared behavior before library acceptance; TASK-021 owns route composition/host wiring/T6, with no invented totals or page-owned SQL. |
| [022](../tasks/TASK-022-metrics-page.md) | Corrected point-only framing with explicit series Latest/Max/Mean table and chart visibility synchronization. TASK-004-R1 closes missing reusable Chart/RecordTable/portable-loader behavior before the library gate; TASK-022 composes accepted components. Metric shape/temporality/precision and unavailable aggregates stay truthful. |
| [023](../tasks/TASK-023-verify-pages.md) | Added AC-019 waiting/outcome/late-span/citation cases, named Verify variants and corrected an unverified prerequisite-delivery assertion into a requirement to verify delivery. |
| [024](../tasks/TASK-024-home-page.md) | Added executable whole-Home section coverage: greeting/counts, attention, Services/Verification, inline Observe, Cards/recent Cards. Reuse existing summary owners. |
| [025](../tasks/TASK-025-change-pages.md) | Corrected four-tab region mapping, including approved historical Audit/file references. Server-owned state and REQ-029 prerequisite retained. |
| [026](../tasks/TASK-026-repositories-page.md) | Corrected Connected/configuration/folder region mapping and actual authorized action proof after REQ-029 delivery. |

## Material evidence behind corrections

- [Spec REQ-020 and REQ-030](../spec.md) already require board fidelity,
  WyrdView fixture galleries and individual human acceptance (baseline lines
  359–371 and 988–995). TASK-004's original checkpoint is at lines 79–85.
- Revision-14 waiting and exact evidence navigation are in spec lines 464–506;
  AC-016–019 are at 1267–1270. TASK-007/023's old broad scenarios omitted these
  cases. Current `VerificationExecutionState` has separate status and optional
  result (`crates/wyrd-spec/src/verification/execution.rs:166–195`); UI retry or
  judgment ownership is unnecessary.
- TASK-008 originally contradicted itself at lines 15/21/27: delivered
  workload attribution versus an unshipped old `service_card_uid`. Current
  `ServiceSpec` still lacks chart persistence; correcting attribution does not
  remove the separate REQ-025 prerequisite.
- [Logs](canvas/Logs.dc.html) has Wrap lines (baseline line 95), paging
  (109), and attribute/trace actions (116–122). TASK-021's generic scenario
  omitted them. [Metrics](canvas/Metrics.dc.html) has the series table at
  108–114; a point table cannot substitute for that interaction.
- [Service](canvas/Service.dc.html) includes Deployment, Change Requests and
  Relationships (70–71). TASK-018's exclusive composition list omitted their
  existing component consumers. Its dependency correction introduces no new
  server capability or Service deployment-management workflow.
- TASK-011's original dependency list omitted completed TASK-027 even though
  the plan required it. The correction preserves approved view layout in the
  actual embedded host rather than inventing a second renderer.

## Unresolved obligations and execution limits

### StatStrip presentation — TASK-004-R1, 014, 018 and 024

The inventory says “1–5 numbers with sparkline” (spec baseline line 918).
The approved [Agent](canvas/Agent.dc.html), [Service](canvas/Service.dc.html)
and [Home](canvas/Home.dc.html) regions show captioned bars/lines, tiles without
trends, and inline summaries respectively. Current StatStrip always renders
sparkline tiles. The required board regions are known; the inventory's wording
does not explain their relationship.

Recommended clarification: require all three approved visual presentations,
with recorded trends only where that region shows them. Preserve existing
statistic/ownership definitions and leave private rendering structure open.
A published catalog/input change, if required, needs an explicit approved
contract; no new variant API is silently approved by this audit. Resolve this
wording before declaring those component regions complete. Independent
gallery/facet/list/waterfall/detail repairs can proceed.

### Query History — TASK-006 and 019

[Query](canvas/Query.dc.html) shows History (baseline line 73;
`canvas.json:228`), but REQ-014 and the task packet do not establish its source,
retention or authorization semantics. A bounded search of the current UI,
TypeScript SDK entrypoint and MCP Bifrost owner found no history capability.
That search is not proof that no owner exists anywhere.

Recommended next decision: define whether History is current-workbench-session
executions or a separately owned durable capability. The smallest candidate is
session-only executed-query history with no new storage, but it remains a
proposal. Identify an existing owner or approve the observable lifetime before
implementing this region. Other Query behavior can proceed independently.

### Cards inventory reference — TASK-005 and 013

TASK-013's existing checkpoint explicitly acknowledges that no dedicated PNG
exists, while REQ-008 requests inventory “to the canvas.” Generic Card detail
has references, but those cannot prove an unprovided inventory layout.
Present a concrete inventory design for explicit visual acceptance or obtain
the intended reference. Keep this checkpoint unaccepted until then; do not
report that a nonexistent board was matched.

Existing REQ-025, non-sensitive outcome-readback and REQ-029 prerequisites
remain delivery gates for their consumers. Updating task prose does not prove
those dependencies shipped. TASK-004-R1 closure remains part of TASK-004
acceptance wherever downstream tasks depend on that family.

## Already-settled questions

The following do not need renewed product approval:

- Use Workload (`workload_card_uid`) and Card (`subject_card_uid`) per REQ-023;
  preserve resource service labels as separate observed data.
- Span names come from data; canvas example names are not runtime cases.
  Current waterfall code does not contain the proposed operation-name colours.
- REQ-028 already prohibits estimated histogram percentiles. Recorded summary
  quantiles may be shown; a canvas p90 estimate does not override that rule.
- Query range behavior follows explicit declared binds, not illustrative
  automatic rewriting of authored SQL.
- REQ-029 already retains historical Change Request references. Lack of a
  standalone Audit/Files PNG is a reference-mapping problem, not permission
  to invent or discard those tabs.

## Runtime audit: arbitrary names and ordinary HTTP telemetry

This additional static audit covers runtime TypeScript/Svelte under
`crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/`, app `src/lib` consumers,
their rendering/formatting conditions and host adapters. Fixtures, example
canvas names and generated bundles are not evidence of runtime name rules.
Canonical span nullability was checked in the owning table schema. Source
runtime references below are relative to `packages/ui/src/lib/` unless a
repository-root path is supplied.

**No implemented span/service/model/tool/metric-name colour classification was
found in this scope.** The proposed `chat`/`invoke_agent` colour scheme in the
conversation is not implemented. This is a source-audit conclusion, not a
claim that every remaining task is implemented or that rendered fidelity passed.

| Owner/condition | Evidence and disposition |
|---|---|
| Chart series colours/markers/dashes | `components/Chart.svelte:29–37,234–242` cycles cues by series index. It assigns no semantic role to an arbitrary label. Retain generic distinction and legend cues. |
| Chart series grouping | `plot.ts:35–80` groups typed values and disambiguates captions. The special “other” text avoids a collision with an actual label; it does not classify a span or service. |
| Waterfall names, service and errors | `observe.ts:435–464` copies recorded names, preserves null service and uses numeric status for errors; `components/TraceWaterfall.svelte:100,227–234` renders null service explicitly and colours errors by status. No operation-name branch. |
| RecordTable and FacetRail canonical statuses | `components/RecordTable.svelte:23,148–149` and `components/FacetRail.svelte:40` use OTLP numeric status values. Arbitrary service/span values are text, not status-name heuristics. The authored-SQL exception is a defect below. |
| Message handling | `components/RecordDetail.svelte:15–25,89–105` checks exact optional GenAI attribute keys; absent attributes become an empty map. Recorded values are not inferred from span names. |
| HTTP spans and optional fields | `observe.ts:310–315,435–464` imposes no GenAI predicate on trace reads. The owning `crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs:70–86,103–132` requires canonical identity/name/time/duration columns while service, HTTP, GenAI and exception promotions are nullable. Their absence is valid data, not permission to drop the span. |
| StatStrip selection | `schemas.ts:236–241` requires the caller's explicit stat list. `observe.ts:144` limits only the expressly approved Agent invocation statistic by `gen_ai_operation_name`; no Agent/model statistic is automatically added to an HTTP view. Token-unit and currency handling use their actual metering fields. |
| Status and verdict primitives | `components/StatusPill.svelte:16–22` takes an explicit tone; `components/VerdictPill.svelte:6–11` uses the typed verdict. Labels do not determine tone. |
| Host parity | `WyrdView.svelte` selects these shared components; `node.ts`, `mcp.ts`, browser `view.ts` and `../app/main.ts` adapt reads/mounting without separate name-based styling. The conventional, self-service and MCP surfaces share the affected owner. |

### Confirmed violations added to TASK-004-R1

**OBS-004-6 — Arbitrary SQL aliases acquire telemetry semantics.**
RecordTable distinguishes SQL input at line 46, but its headings, group label
and cell branches at 126–153 still apply canonical field interpretation.
For example, `SELECT 2 AS status_code, 1000000 AS duration_nano` becomes an
error badge and a duration instead of the returned numeric values. The SQL
schema branch (`schemas.ts:204–214`) declares no telemetry signal, and the
loader preserves authored SQL (`observe.ts:283–287`). This violates REQ-030's
schema-driven SQL result behavior (spec baseline 744–748). Remediation must
distinguish explicit canonical signal semantics from arbitrary query aliases.
Canonical status rendering remains valid for actual signal inputs.

**OBS-004-7 — Display captions become group identities.**
RecordTable lines 69–77 key the group map by `cellText`; `observe.ts:407–412`
rounds durations and displays null as `∅`. Values `1000001` and `1000002`
nanoseconds can merge into one formatted duration group, and null can merge
with the literal string `∅`. Required group-by behavior must preserve distinct
typed values while formatting their captions. This is a general workbench
data-identity defect, not an LLM-specific colour rule.

Both findings have deterministic source paths and required outcomes; they
remain implementation work in R1. No runtime/test edits are retained.

### Observed limitation, kept separate from confirmed violations

Every span currently opens RecordDetail on Messages (`RecordDetail.svelte:27–29`).
An ordinary HTTP span therefore initially shows “No GenAI messages recorded”;
its Attributes/Events/Raw tabs still work and missing attributes are handled.
REQ-028 describes messages for LLM spans, while the inventory also requires
the Messages section and honest empty states. This is an applicability/initial
presentation question; the audit does not label it a proven name classifier
or silently prescribe a new tab contract. HTTP-only evidence must be included
in the R1 inspection so that a GenAI-heavy fixture set cannot hide this behavior.

Planned regression coverage in R1 includes ordinary HTTP spans without GenAI
attributes, null service/Card attribution, arbitrary/unseen/misleading names,
actual-status changes independent of names, exact-value interactions, generic
query aliases and typed-group collisions. A known-name fixture-only test is
insufficient. No standard GenAI attribute is assumed to exist on every span.

## Verification and limits

The audit used parallel bounded read-only inspections of every assigned task,
the relevant spec slices, board HTML/notes and narrow existing owners. It did
not audit all implementation code or compare rendered pixels. Confirmed
TASK-004 source omissions are listed in its remediation task; the pasted
“115 gaps” and “18 decisions” were not accepted as a validated ledger.

Document verification passed: `git diff --check`, local Markdown link
resolution, coverage of all 22 remaining original tasks, revision-14 references
in active tasks, complete Behavior/RED/GREEN/REFACTOR blocks, unchanged approved
spec and acyclic task dependencies. These checks cover the changed Markdown,
not the docs-site or application runtime.

One independent read-only reader inspected the spec and current proposed tasks
together using the shared comprehension guidance and plan-readiness rubric. It
found a shared Logs/Metrics allocation conflict with the full-library gate. That
was repaired by assigning known missing reusable behavior to TASK-004-R1 before
library acceptance; the reader rechecked those sections and confirmed closure.
No further comprehension contradiction was reported. Execution remains
conditional on the unresolved obligations and real prerequisites above; this
is not a readiness PASS.

No runtime test, browser result, human acceptance or implementation completion
is inferred from this audit.
