# UI migration task order

Last approved scope revision: 19 (explicit caller approval, 2026-11-02).
Revision 19 authorizes Task 03: real local signup/login, User-owned personal
keys, a seeded local-human dev workflow and exact Observe alignment. Task 03 is
ready for implementation with its approved provision-then-enroll admission,
typed public contracts and security/visual contracts.
The current source/reference audit and researched decisions are in
[auth research](references/task-03-auth-research.md) and
[Observe audit](references/task-03-observe-audit.md).

The TASK-004 replacement contains Task 01 (optional local MockServer), Task 02
(canonical dev workflow and Observe), and Task 03 (approved account/dev and
remaining Observe closure). Task 03 now owns unfinished Task02/R1/R2 functional
and visual obligations. Retired R1/R2/R3 retain evidence only. No implementation or
acceptance is implied. Preserve current source/cleanup and unrelated UI work.
Historical migration baseline and immutable review subjects remain unchanged.

TASK-001 is the independent distribution/login outcome. TASK-002 deletes the old
UI before component work. TASK-003 establishes the reusable foundation with
complete Chart, CardHeader and primitives. TASK-004–009 complete the in-scope component
families; TASK-010 is deferred and is not a shipping dependency; individual component checks stay inside each family task. TASK-011
registers the production MCP App only after the **entire in-scope library** is complete, then proves the real-agent MCP App journey.
Except for the revision-18 early Observe slice, conventional pages start after
that gate and retain their own visual checkpoints.

| Task | Observable outcome | Real task dependencies | External completion dependency |
|---|---|---|---|
| [TASK-001](tasks/TASK-001-distribution-login.md) | Installed and container UI with existing login — **complete** | None | None |
| [TASK-001-R1](review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md) | Correct installation eligibility, implement Rust-owned opaque browser sessions and prove official nginx login | TASK-001 and TASK-002 integrated at `843484f6c`; approved revision 10 | Supported native/Linux image runtime and human inspection remain required acceptance evidence |
| [TASK-002](tasks/TASK-002-remove-old-ui.md) | Remove obsolete UI while retaining login — **complete** | None | None |
| [TASK-003](tasks/TASK-003-shared-ui-foundation.md) | Reusable UI foundation across the four hosts — **complete** | TASK-002 | None |
| [TASK-004](tasks/TASK-004-observe-components.md) | Observe implementation retained; **incomplete until replacement live workflow and individual acceptance close** | TASK-003 implementation; closure by TASK-004-REV-01/02 | No historical PASS substitutes for real data/browser/visual proof. |
| [TASK-004-REV-01](tasks/TASK-004-revision/TASK-01-mockserver.md) | Optional local MockServer via existing OpenAI endpoint configuration; proposed | None | Container runtime; official pinned amd64/arm64 image; real gateway fixture proof. |
| [TASK-004-REV-02](tasks/TASK-004-revision/TASK-02-live-observe-workflow.md) | Canonical 500-request dev:full batch and real shell/Traces/Spans/Logs/Metrics; proposed | TASK-003, TASK-027, TASK-004-REV-01; retain existing Observe code | Current production SDK/source patch, independent review and individual human live/component inspection. |
| [TASK-004-REV-03](tasks/TASK-004-revision/TASK-03-local-users-personal-keys-and-observe.md) | Ready: local human signup/login, personal Settings/API keys, seeded dev:full and exact Observe layout/color closure | TASK-003, TASK-027, TASK-004-REV-01 source; retain current Task02/R1/R2 implementation and evidence | Revision 19 approved 2026-11-02. Unfinished Task02 visual/delivery obligations transfer here; Task02 PASS is not a circular prerequisite. Separate rendered human acceptance remains required. |
| [TASK-005](tasks/TASK-005-card-components.md) | Card browsing and definition component family | TASK-003 | Only if required ArtifactTable actions lack a current SDK/MCP projection: obtain approved platform action contract before calling that action complete. |
| [TASK-006](tasks/TASK-006-query-components.md) | Query component family | TASK-003, TASK-004 | Query History source/lifetime unresolved; independent work may proceed. |
| [TASK-007](tasks/TASK-007-verify-components.md) | Generic verification component family | TASK-003, TASK-004, TASK-005 | Separately specified non-sensitive requirement-outcome read contract; REQ-029 for AttentionList CR rows. Current sensitive outcome is not reclassified here. |
| [TASK-008](tasks/TASK-008-service-components.md) | Service signal and chart authoring component family | TASK-003, TASK-004, TASK-005, TASK-006 | REQ-025 Service charts, with separately approved platform implementation and actual SDK/MCP read/action projections. |
| [TASK-009](tasks/TASK-009-change-repository-components.md) | Change Request and repository component family | TASK-003, TASK-005, TASK-007 | REQ-029 separately approved and shipped CR/repository/webhook/verifier-folder read/write contracts with SDK/MCP projections. |
| [TASK-010](tasks/TASK-010-ask-component.md) | Deferred: Ask and saved-view interaction | None; superseded from active execution | Separate future approved scope; blocks no task here. |
| [TASK-011](tasks/TASK-011-mcp-app.md) | Production MCP App, catalog discovery and connected multi-turn agent/browser journey | TASK-003, TASK-004, TASK-005, TASK-006, TASK-007, TASK-008, TASK-009, TASK-027, TASK-004-REV-02 | Actual supported session/App-host integration and local live credentials; scenario 3 is blocked if absent |
| [TASK-012](tasks/TASK-012-canvas-shell.md) | Superseded: complete shell and browser foundation assigned to TASK-004-REV-02 | TASK-004-REV-02 | Historical scope reference only; all proof transferred. |
| [TASK-013](tasks/TASK-013-card-browsing.md) | Registry browsing and exact Card detail | TASK-004-REV-02, TASK-005 | Missing inventory reference/visual acceptance recorded in the audit. |
| [TASK-014](tasks/TASK-014-agent-page.md) | Agent workspace | TASK-004-REV-02, TASK-013, TASK-004, TASK-005, TASK-007 | None |
| [TASK-015](tasks/TASK-015-prompt-page.md) | Prompt workspace | TASK-004-REV-02, TASK-013, TASK-005 | None |
| [TASK-016](tasks/TASK-016-model-page.md) | Model workspace | TASK-004-REV-02, TASK-013, TASK-004, TASK-005, TASK-007 | None |
| [TASK-017](tasks/TASK-017-data-page.md) | Data workspace | TASK-004-REV-02, TASK-013, TASK-005, TASK-007 | None |
| [TASK-018](tasks/TASK-018-service-page.md) | Service workspace and charts | TASK-004-REV-02, TASK-013, TASK-008, TASK-007, TASK-005, TASK-009 | Requires delivered REQ-025 and production-complete TASK-008; REQ-029/TASK-009 for its Change Requests region; no Service inference from component lists. |
| [TASK-019](tasks/TASK-019-query-page.md) | Query workbench page | TASK-004-REV-02, TASK-006 | Query History source/lifetime unresolved. |
| [TASK-020](tasks/TASK-020-traces-page.md) | Superseded: complete Traces/Spans investigation assigned to TASK-004-REV-02 | TASK-004-REV-02 | Historical scope reference only; all proof transferred. |
| [TASK-021](tasks/TASK-021-logs-page.md) | Superseded: complete Logs investigation assigned to TASK-004-REV-02 | TASK-004-REV-02 | Historical scope reference only; all proof transferred. |
| [TASK-022](tasks/TASK-022-metrics-page.md) | Superseded: complete Metrics investigation assigned to TASK-004-REV-02 | TASK-004-REV-02 | Historical scope reference only; all proof transferred. |
| [TASK-023](tasks/TASK-023-verify-pages.md) | Verify results workbench and Verifier definition | TASK-004-REV-02, TASK-007, TASK-004 | Non-sensitive per-requirement outcome contract is shipped; TASK-007 is production-complete. |
| [TASK-024](tasks/TASK-024-home-page.md) | Home summary page | TASK-004-REV-02, TASK-005, TASK-007, TASK-008, TASK-004 | Applicable REQ-029 attention contracts; all supporting library tasks complete. |
| [TASK-025](tasks/TASK-025-change-pages.md) | Change Request collaboration pages | TASK-004-REV-02, TASK-009 | REQ-029 platform contract and TASK-009 production-complete, including actual read/write host projections. |
| [TASK-026](tasks/TASK-026-repositories-page.md) | Repository connections page | TASK-004-REV-02, TASK-009 | REQ-029 repository/webhook/configuration platform contract and TASK-009 production-complete. |
| [TASK-027](tasks/TASK-027-view-layout.md) | Placed, resizable view panels (revision 12) — **complete** | TASK-003 | None |

TASK-001, TASK-002 (with TASK-001-R1 to R3), TASK-003 and TASK-027 are complete:
the caller accepted them on 2026-10-23 after inspecting the gallery. The
remaining TASK-003 review findings were either fixed (FIND-003-16/17 at
`a9be77a51`) or dropped as unneeded complexity ([simplify record](review/simplify-2026-10-23.md)).
Native/image runtime and nginx evidence for TASK-001 stay with final change review.

## Revision 18 baseline and approved Task 03 closure

Execute [Task 01](tasks/TASK-004-revision/TASK-01-mockserver.md), then
[Task 02](tasks/TASK-004-revision/TASK-02-live-observe-workflow.md).
Task 01 adds local development fixtures, not a Wyrd provider feature. Task 02
is the complete user-visible outcome: `mise run dev:full`, canonical current-SDK
application, bounded 500-request workload, actual retained signals and usable
authenticated shell/Traces/Spans/Logs/Metrics. It carries all outstanding R1
component obligations and absorbs TASK-012/020/021/022 completely. These old
page tasks are superseded, not completed proof. TASK-019 retains full Query
workbench scope; Task 02 closes only exact-query inspection/replay navigation.

The caller explicitly replaced internal runtime mocking with optional local
MockServer. Existing endpoint configuration reaches both application and queued
judge calls, so no SDK scope/header, custom Rust provider, verification snapshot,
new retained fields or special billing policy is required. Do not execute old
keep/remove lists or interfere with caller cleanup.

Retired assignments, including their old implementation claims, are preserved:
[R1](references/retired-task-004/TASK-004-R1-canvas-fidelity.md),
[R2](references/retired-task-004/TASK-004-R2-live-dev-workbench.md),
[R3](references/retired-task-004/TASK-004-R3-minimal-runtime-mock.md).
They are not active dependencies or approvals. Task 02 depends on existing
Observe code, not on TASK-004 already being complete; it closes the parent.

This Observe slice retains the early-live exception to unrelated family/MCP
ordering. Full-library/MCP shipping and later domain-page gates are unchanged.
Independent review and separate human desktop/mobile light/dark acceptance
remain required; neither reorganizing tasks nor dispatching HTTP establishes PASS.

## Remaining-task audit and execution limits

The [2026-10-29 audit](references/task-audit-2026-10-29.md) preserves historical
source findings and evidence limits. Task 02 carries the outstanding Observe
component obligations and the complete Logs/Metrics page, host and T6 proof.
Other tasks retain their board regions and applicable component scenarios;
this reorganization marks none of them implemented, reviewed or accepted.

Query History source/lifetime (TASK-006/019) and the missing Cards inventory
reference (TASK-005/013) remain explicit external decisions. StatStrip
presentation remains a verification/acceptance obligation of Task 02 and its
consumers: use existing recorded inputs/approved invocation semantics and
identify the relevant board region, without inventing a statistic or public
prop contract. Existing platform prerequisites remain blocking. Independent
work may proceed; affected regions cannot be silently omitted or redesigned.
Completed records and deferred scope are preserved.

## Active remediation and revision authority

The immutable [TASK-001/002 review](review/tasks-001-002-843484f6c/verdict.md)
reviewed revision 9 at `843484f6c`. It returned SPEC_REVISION_REQUIRED for
TASK-001, retained FIND-TASK-001-1 and FIND-TASK-001-2, and found no implementation
defect in TASK-002 (human acceptance still pending). Do not rewrite that review
or treat revision 10 as retroactive PASS.

[TASK-001-R1](review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md)
is the single current assignment for both bounded findings and the caller's
separate REQ-033 server-session decision. It supersedes TASK-001's confidential-
client/encrypted-cookie premise and TASK-002's session-owner keep constraint
only where revision 10 explicitly changes them. Preserve original task/evidence
files as history. Revision 11 separately supersedes only Ask/saved-view scope and downstream journey/ordering obligations; other obligations remain. TASK-003 remains scope-compatible; revision 12 adds TASK-027 for view layout, and TASK-003 corrects its own view-document `version`/`catalogId` drift as an implementation fix; TASK-003's request-scoped session seam is corrected for REQ-033 and must
use the remediated boundary once implemented, without onboarding domain clients
in remediation. Existing human checkpoints still govern dependent execution.

## Dependency and completion rules

The displayed order follows the approved build order. Numbering alone creates no
execution dependency: the table and task metadata name the actual edges. Human
acceptance is required before each dependent slice begins. TASK-001/002 may be
implemented in either order; both preserve login and reconcile their overlapping
BFF/test edits when integrated. No component-family task duplicates foundation's
Chart/CardHeader/primitives ownership. Page tasks compose completed components;
they do not reopen loader/executor ownership or copy historical workspaces.

REQ-023 (workload/subject correlation) is delivered on main. REQ-025, REQ-029 and non-sensitive requirement-outcome reads are **separate platform prerequisites**. REQ-031 and Ask are deferred, not active prerequisites. This plan identifies their
UI consumers; it does not create underspecified backend implementation tasks.
Affected tasks are conditional, not ready to complete from main alone. Use actual
approved/generated contracts when delivered, and do not guess fixture payloads
for contracts that have not been defined. Artifact action projection gaps likewise
need the applicable approved host capability. No fixture, unavailable action or
mock success satisfies production completion.

TASK-011 requires full T1–T5 and action closure plus individual human desktop/mobile
light/dark acceptance for every in-scope inventory component (TASK-003–009). It owns real production MCP
resource/tools and host-specific security proof, reusing R2's browser lifecycle under approved revision 15. T4 artifact tests are already
built/run in foundation and each family; they do not wait for production registration.
The in-scope library shipping gate is held by every missing component/prerequisite; deferred Ask/saved views are excluded, not fixture-completed.
Revision 14 required the full-library/MCP gate before all conventional pages. Approved revision 18 makes only TASK-004-REV-02’s absorbed shell/Observe slice an early exception; remaining page work retains that gate.

## Obligation ownership

| Spec obligations | Delivery owner and proof |
|---|---|
| REQ-001–006/033; AC-001–005/012; INV-003/005 | TASK-001 plus TASK-001-R1: signed install/lifecycle, Rust-owned browser sessions and native/nginx auth; distinct native and image runner evidence |
| REQ-032 | TASK-002: exact delete/keep inventory, removed imports/settings, retained login regression |
| REQ-030; AC-011 | TASK-003 foundation; TASK-004–009 in-scope inventory families; TASK-011 production MCP and package/custom-app regression |
| REQ-007/021 | TASK-004-REV-02 under approved revision 15, absorbing TASK-012: canvas shell controls and navigation |
| REQ-008/009; AC-006 | TASK-013: inventory and exact-detail workflow with separate checkpoints; TASK-003/005 component proof |
| REQ-010/011/012/013/024; AC-007 | TASK-014 Agent, TASK-015 Prompt, TASK-016 Model, TASK-017 Data, TASK-018 Service; TASK-023 Verifier definition |
| REQ-014–017/028; AC-008 | TASK-019 full Query; TASK-004-REV-02 owns exact-query inspection and complete Traces/Spans/Logs/Metrics, absorbing TASK-012/020/021/022; shared component and first-class read proof retained |
| REQ-018/026/027; AC-009 | TASK-007 components; TASK-023 run/session/drift/definition workflow; TASK-024 summaries of same recorded facts |
| REQ-019 | TASK-024 Home, no bundled extra summary pages |
| REQ-023/025 | REQ-023 is delivered on main and TASK-004/008/018/020–024 consume its `workload_card_uid`/`subject_card_uid`; REQ-025 is external, consumed by TASK-008/018 |
| REQ-029 | External platform delivery; TASK-009 components, TASK-025 Change Requests and TASK-026 Repos; TASK-007/024 consume attention rows |
| REQ-031 and Ask | Deferred from this change; TASK-010 superseded; no shell controls or shipping dependency |
| REQ-034; AC-013 | TASK-004-REV-02 establishes real-browser page proof under approved revision 15; TASK-013–026 each own their route/workflow, applicable range changes and negative flows |
| REQ-035; AC-014 | TASK-011 extends existing agent-readiness ownership for a connected live-agent/browser MCP App investigation, follow-up and range reload; deterministic host negatives supplement it |
| REQ-020; AC-010 | Every task: runnable named visual checkpoint, per-component acceptance and source-backed reuse; existing preservation hashes/source inventory remain intact; independent task readiness review before handoff |
| REQ-022; INV-001/002/004/006/007 | Every consuming task: current contracts, recorded data, credential/tenant boundary, small existing-owner integration, honest errors and focused proof; task acceptance and integrated diff review |
| REQ-037–040; AC-020–023 | TASK-004-REV-01: local initialized MockServer. TASK-004-REV-02: canonical current-SDK batch, full authenticated shell/Observe and safe shared predicate consumers; revision 18 |

## Verification ownership

Exact focused scenario commands are in each task; managed setup is in
[verification owners](references/verification.md). Task checks stay bounded.
Final `$wyrd-change-review` runs combined first-class surface journeys and the
repository aggregate once against the integrated candidate, plus required
native/image/visual evidence. No task can substitute unit/fixture proof for its
live authorized consumer or claim other-platform success from a local check.

## Revision-11 execution and acceptance

TASK-003's existing foundation scope, exact scenarios and individual human
checkpoints remain intact. Do not reopen it to implement a browser harness,
Ask or a live agent; correct actual foundation defects only if discovered.
Under revision 11, TASK-011 owned the first browser tooling and TASK-012 reused
it. Approved revision 15 supersedes that sequencing: TASK-004-REV-02 owns the first
real-browser/BFF lifecycle, and TASK-011 plus later pages reuse it. TASK-011
retains production App-specific proof; page tasks extend their focused T6
selectors rather than creating another runner.
An installed browser driver is test infrastructure, not an optional production
setting. No existing jsdom or HTTP-only check is relabeled as real-browser proof.

TASK-011 adds a focused UI journey selection to the existing coding-readiness
CLI and an opt-in mise entry, leaving the original default objective/evidence
unchanged. Its actual agent and browser must share the connected conversation,
MCP results and caller identity. Repository source currently proves neither
CLI App rendering nor follow-up continuation; supported host/agent integration
must be demonstrated. A minimal connected test host may establish that host's
behavior under REQ-035, never untested external-client compatibility. Missing
credentials/support produce BLOCKED, not a passing fixture replacement.

At final integrated change review require both REQ-034 and REQ-035 evidence
against the candidate, including a fresh gated live run or explicitly verified
unchanged candidate evidence. The paid lane remains outside `gate` and default
CI: aggregate success alone cannot satisfy AC-014. Human component/page visual
acceptance remains separate. No Ask/saving implementation is required for either
primary journey, and no new production agent service is authorized.

Independent [revision-11 readiness review](review/plan-readiness-revision-11/review.md)
returned **Approve**, architecture PASS and executability PASS, with no blocking
findings. Its scope is this planning delta, not implementation/runtime acceptance;
existing platform and human inspection prerequisites remain.
