# TASK-001 visual acceptance review

Status: **REMEDIATE**

Reviewed: 2026-09-03

This records the human visual-acceptance review of the delivered TASK-001 SVG
mocks. It does not rewrite TASK-001 or modify its delivered artifacts. The
current mocks remain a rejected first draft and a reference for remediation.
Svelte product-view implementation remains blocked until the remediated mocks
receive explicit human approval.

## Reviewed snapshot

- `brand/renders/product/observe.svg` — SHA-256
  `a559a5e754a9d7cc8548eedaf677847c8b88741a5d162cb8d48a3cc4804186bc`
- `brand/renders/product/changes.svg` — SHA-256
  `b05124a420780f8628f93aaec021f1cf33c1c947e3e8396e60e2f0bf2a0f4ba5`
- `brand/renders/product/README.md` — SHA-256
  `a669ed90f2ce611cb6940acabf7fe011953323d477164102ac4a508a09fd1d61`

These files were reviewed as a working-tree snapshot rather than a committed
candidate. A later formal `$wyrd-task-review` must identify its base and
candidate commits.

## Findings

### FIND-TASK-001-1 — MAJOR — Eval workflow and task inspection lost the proven drill-down interaction

**Observed artifact:** O-09 in `brand/renders/product/observe.svg` renders the
workflow stages and task list beside a permanently narrow selected-task rail.
It does not show the workflow-result drawer and task-inspection experience
required to understand a full evaluation run.

**Authority and evidence:** TASK-001 O-09 requires event → workflow → selected
task inspection and names the OpsML Eval implementation as interaction
research. OpsML's `AgentEvalWorkflowTable.svelte` opens
`AgentEvalWorkflowSideBar.svelte`; the drawer occupies most of the viewport and
`AgentEvalWorkflowContent.svelte` splits workflow stages/tasks from the
selected task detail. `AgentEvalRecordTable.svelte` separately opens the
record-detail drawer.

**Reachable consequence:** A user cannot comfortably move from an Eval event
to a workflow and then inspect task inputs, expected and authorized actual
values, status, score, timing, explanation, operator, and trace context. The
narrow rail forces truncation and makes workflow structure secondary.

**Required outcome:** O-09 must demonstrate the two distinct drill-downs:
record detail and workflow detail. Selecting a workflow opens a large drawer
containing stage/task navigation and selected-task detail. Mobile must provide
an explicit list/detail switch. Workflow and Task remain views of the Eval
event, not new resource routes.

### FIND-TASK-001-2 — MAJOR — The mocks use brand tokens without the approved Workbench visual grammar

**Observed artifacts:** The reviewed Observe and Changes screens repeatedly
use the same small table, outlined cards, narrow right rail, and large unused
lower region. Typography and information are compressed even when the viewport
has available space. Product areas are insufficiently differentiated.

**Authority:** TASK-001 makes `brand/renders/workbench.html` authoritative for
the starting shell, density, controls, tables, panels, charts, and workbench
composition—not only its colors. The reference uses strong scale changes,
large purposeful work regions, prominent selection, filled organizational
bands, legible metrics, and deliberate operational density.

**Reachable consequence:** Pages technically contain the requested data but do
not establish clear hierarchy, operational focus, or a distinctive Wyrd
experience. Propagating the current template would make unrelated work areas
feel interchangeable.

**Required outcome:** Rebuild the representative golden screens from the
composition of `workbench.html`: meaningful scale and hierarchy, larger usable
data surfaces, clear selected states, intentional filled regions, restrained
but visible status emphasis, and no unexplained empty void. Reuse brand tokens;
do not introduce another visual system.

### FIND-TASK-001-3 — MAJOR — Change Requests read as passive administration rather than a familiar review workflow

**Observed artifacts:** CR-03, CR-04, and CR-05 in
`brand/renders/product/changes.svg` distribute Change state across small cards
and passive rails. They do not provide a persistent Change Request header and
obvious lifecycle/review actions. Claim and Verifier progress exists as data,
but it is not readable as the central checks-like workflow.

**Authority and evidence:** TASK-001 requires familiar pull-request hierarchy
and controls, while keeping Wyrd's own theme and separating lifecycle,
verification, and authorization. The approved direction and
`partner-mockup.html` lead with the Change, exact revision, Claims, accepted
Evidence, and current decision. Human review also requires familiar discussion
and approval behavior without copying GitHub's visual identity.

**Reachable consequence:** Product, Data Science, and Engineering users cannot
quickly answer what is changing, which Claims are satisfied, which Verifiers
are running or failing, whether approval is needed, or which action they can
take next.

**Required outcome:** Change detail pages must share a persistent identity and
action area showing title, lifecycle, revision, Claim progress, and Verifier
progress. Provide permission-aware `Approve`, `Request changes`, and `Close
Change Request` controls. Verification must organize Verifier state under its
Claim with immediately scannable Not run/queued/running/pass/fail states;
Evidence and provenance remain drill-downs. Review must include the familiar
comment/reply workflow and review actions. Approval, override, verification,
and lifecycle must remain distinct. Wyrd never exposes a source-code merge
action.

### FIND-TASK-001-4 — MODERATE — Spacing and overflow defects invalidate the layout proof

**Observed artifacts:** Reviewed artboards contain cramped labels and panels,
visual bleeding, inconsistent internal gutters, and large unused regions below
compressed content.

**Authority:** TASK-001 requires each SVG to be directly reviewable at its
declared scale and requires mobile checks for clipped actions, inaccessible
detail, and uncontrolled overflow.

**Reachable consequence:** The SVGs do not reliably prove that the proposed
layout can contain their required content or adapt to the target viewport.
These defects would otherwise be deferred directly into Svelte implementation.

**Required outcome:** At declared artboard scale, text and controls must not
collide, bleed across panels, or depend on unreadably small type. Gutters and
panel alignment must be consistent; dense content must use an intentional
scroll, drawer, tab, or disclosure pattern. Empty space must reflect hierarchy
rather than an undersized universal template.

## Remediation entry gate

Before revising the rest of the mock inventory, produce and obtain explicit
human approval for these three golden screens in both light and dark modes:

1. CR-04 Change Request verification — persistent Change header/actions and a
   checks-like Claim/Verifier workflow.
2. O-09 Eval event — record/workflow drawers with stage, task, and selected-task
   inspection.
3. O-04 Trace search — representative operational Workbench composition with
   filters, trend, records, selection, and detail.

After those screens are approved, propagate their approved shell, hierarchy,
spacing, and interaction patterns across every affected desktop and mobile
artboard. Do not proceed to Svelte product-view implementation before final
mock acceptance.

## Verification limits

This is direct visual artifact review. No application tests or lints were run
or required. The remaining TASK-001 artboards were not individually approved
by this review.
