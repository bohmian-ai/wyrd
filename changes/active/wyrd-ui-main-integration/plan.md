# UI migration task order

Approved authority: [spec revision 9](spec.md), committed baseline `8651b96fd`.
Planning only; no implementation or task execution is approved by this packet.

TASK-001 is the independent distribution/login outcome. TASK-002 deletes the old
UI before component work. TASK-003 establishes the reusable foundation with
complete Chart, CardHeader and primitives. TASK-004–010 complete coherent component
families; individual component checks stay inside each family task. TASK-011
registers the production MCP App only after the **entire library** is complete.
Conventional pages start after that gate and retain their own visual checkpoints.

| Task | Observable outcome | Real task dependencies | External completion dependency |
|---|---|---|---|
| [TASK-001](tasks/TASK-001-distribution-login.md) | Installed and container UI with existing login | None | None |
| [TASK-002](tasks/TASK-002-remove-old-ui.md) | Remove obsolete UI while retaining login | None | None |
| [TASK-003](tasks/TASK-003-shared-ui-foundation.md) | Reusable UI foundation across the four hosts | TASK-002 | None |
| [TASK-004](tasks/TASK-004-observe-components.md) | Observe component family | TASK-003 | REQ-023 service_card_uid ingest/schema/auth prerequisite for Service filters and Service-scoped variants. |
| [TASK-005](tasks/TASK-005-card-components.md) | Card browsing and definition component family | TASK-003 | Only if required ArtifactTable actions lack a current SDK/MCP projection: obtain approved platform action contract before calling that action complete. |
| [TASK-006](tasks/TASK-006-query-components.md) | Query component family | TASK-003, TASK-004 | None |
| [TASK-007](tasks/TASK-007-verify-components.md) | Generic verification component family | TASK-003, TASK-004, TASK-005 | Separately specified non-sensitive requirement-outcome read contract; REQ-023 for Service-filter variants; REQ-029 for AttentionList CR rows. Current sensitive outcome is not reclassified here. |
| [TASK-008](tasks/TASK-008-service-components.md) | Service signal and chart authoring component family | TASK-003, TASK-004, TASK-005, TASK-006 | REQ-023 correlation (including verification results) and REQ-025 Service charts, with separately approved platform implementation and actual SDK/MCP read/action projections. |
| [TASK-009](tasks/TASK-009-change-repository-components.md) | Change Request and repository component family | TASK-003, TASK-005, TASK-007 | REQ-029 separately approved and shipped CR/repository/webhook/verifier-folder read/write contracts with SDK/MCP projections. |
| [TASK-010](tasks/TASK-010-ask-component.md) | Ask and saved-view interaction component | TASK-003 | Separately specified/shipped Ask endpoint and REQ-031 saved-view resource/read/write/visibility contracts with applicable SDK/MCP actions. |
| [TASK-011](tasks/TASK-011-mcp-app.md) | Production MCP App and catalog discovery | TASK-003, TASK-004, TASK-005, TASK-006, TASK-007, TASK-008, TASK-009, TASK-010 | None |
| [TASK-012](tasks/TASK-012-canvas-shell.md) | Canvas shell and navigation | TASK-011 | None |
| [TASK-013](tasks/TASK-013-card-browsing.md) | Registry browsing and exact Card detail | TASK-012, TASK-005 | None |
| [TASK-014](tasks/TASK-014-agent-page.md) | Agent workspace | TASK-012, TASK-013, TASK-004, TASK-005, TASK-007 | None |
| [TASK-015](tasks/TASK-015-prompt-page.md) | Prompt workspace | TASK-012, TASK-013, TASK-005 | None |
| [TASK-016](tasks/TASK-016-model-page.md) | Model workspace | TASK-012, TASK-013, TASK-004, TASK-005, TASK-007 | None |
| [TASK-017](tasks/TASK-017-data-page.md) | Data workspace | TASK-012, TASK-013, TASK-005, TASK-007 | None |
| [TASK-018](tasks/TASK-018-service-page.md) | Service workspace and charts | TASK-012, TASK-013, TASK-008, TASK-007 | REQ-023 and REQ-025 delivered and TASK-008 production-complete; no Service inference from component lists. |
| [TASK-019](tasks/TASK-019-query-page.md) | Query workbench page | TASK-012, TASK-006 | None |
| [TASK-020](tasks/TASK-020-traces-page.md) | Traces and span investigation page | TASK-012, TASK-004 | REQ-023 for actual Service filter; no service_name stand-in. |
| [TASK-021](tasks/TASK-021-logs-page.md) | Logs investigation page | TASK-012, TASK-004 | REQ-023 for actual Service filter; no inferred Service identity. |
| [TASK-022](tasks/TASK-022-metrics-page.md) | Metrics page | TASK-012, TASK-004 | REQ-023 for actual Service scope/filter where exposed. |
| [TASK-023](tasks/TASK-023-verify-pages.md) | Verify results workbench and Verifier definition | TASK-012, TASK-007, TASK-004 | Non-sensitive per-requirement outcome contract and REQ-023 Service result column are shipped; TASK-007 is production-complete. |
| [TASK-024](tasks/TASK-024-home-page.md) | Home summary page | TASK-012, TASK-005, TASK-007, TASK-008, TASK-004 | REQ-023 and applicable REQ-029 attention contracts; all supporting library tasks complete. |
| [TASK-025](tasks/TASK-025-change-pages.md) | Change Request collaboration pages | TASK-012, TASK-009 | REQ-029 platform contract and TASK-009 production-complete, including actual read/write host projections. |
| [TASK-026](tasks/TASK-026-repositories-page.md) | Repository connections page | TASK-012, TASK-009 | REQ-029 repository/webhook/configuration platform contract and TASK-009 production-complete. |

## Dependency and completion rules

The displayed order follows the approved build order. Numbering alone creates no
execution dependency: the table and task metadata name the actual edges. Human
acceptance is required before each dependent slice begins. TASK-001/002 may be
implemented in either order; both preserve login and reconcile their overlapping
BFF/test edits when integrated. No component-family task duplicates foundation's
Chart/CardHeader/primitives ownership. Page tasks compose completed components;
they do not reopen loader/executor ownership or copy historical workspaces.

REQ-023, REQ-025, REQ-029, REQ-031, non-sensitive requirement-outcome reads and the
Ask endpoint are **separate platform prerequisites**. This plan identifies their
UI consumers; it does not create underspecified backend implementation tasks.
Affected tasks are conditional, not ready to complete from main alone. Use actual
approved/generated contracts when delivered, and do not guess fixture payloads
for contracts that have not been defined. Artifact action projection gaps likewise
need the applicable approved host capability. No fixture, unavailable action or
mock success satisfies production completion.

TASK-011 requires full T1–T5 and action closure plus individual human desktop/mobile
light/dark acceptance for every inventory component. It owns real production MCP
resource/tools and host-specific security proof. T4 artifact tests are already
built/run in foundation and each family; they do not wait for production registration.
The full library shipping gate is held by every missing component/prerequisite.
TASK-012–026 cannot start simply because one family has finished.

## Obligation ownership

| Spec obligations | Delivery owner and proof |
|---|---|
| REQ-001–006; AC-001–005; INV-003/005 | TASK-001: signed native package/install/lifecycle, public auth and Linux routing; distinct native and image runner evidence |
| REQ-032 | TASK-002: exact delete/keep inventory, removed imports/settings, retained login regression |
| REQ-030; AC-011 | TASK-003 foundation; TASK-004–010 all inventory families; TASK-011 production MCP and package/custom-app regression |
| REQ-007/021 | TASK-012: canvas shell controls and navigation |
| REQ-008/009; AC-006 | TASK-013: inventory and exact-detail workflow with separate checkpoints; TASK-003/005 component proof |
| REQ-010/011/012/013/024; AC-007 | TASK-014 Agent, TASK-015 Prompt, TASK-016 Model, TASK-017 Data, TASK-018 Service; TASK-023 Verifier definition |
| REQ-014–017/028; AC-008 | TASK-019 Query, TASK-020 Traces, TASK-021 Logs, TASK-022 Metrics; TASK-003/004/006 component and read parity proof |
| REQ-018/026/027; AC-009 | TASK-007 components; TASK-023 run/session/drift/definition workflow; TASK-024 summaries of same recorded facts |
| REQ-019 | TASK-024 Home, no bundled extra summary pages |
| REQ-023/025 | External platform delivery; TASK-004/008/018/020–024 consume real Service correlation; TASK-008/018 consume real versioned chart contract |
| REQ-029 | External platform delivery; TASK-009 components, TASK-025 Change Requests and TASK-026 Repos; TASK-007/024 consume attention rows |
| REQ-031 and Ask | External platform delivery; TASK-010 saved-view/Ask component and TASK-012 shell consumer |
| REQ-020; AC-010 | Every task: runnable named visual checkpoint, per-component acceptance and source-backed reuse; existing preservation hashes/source inventory remain intact; independent task readiness review before handoff |
| REQ-022; INV-001/002/004/006/007 | Every consuming task: current contracts, recorded data, credential/tenant boundary, small existing-owner integration, honest errors and focused proof; task acceptance and integrated diff review |

## Verification ownership

Exact focused scenario commands are in each task; managed setup is in
[verification owners](references/verification.md). Task checks stay bounded.
Final `$wyrd-change-review` runs combined first-class surface journeys and the
repository aggregate once against the integrated candidate, plus required
native/image/visual evidence. No task can substitute unit/fixture proof for its
live authorized consumer or claim other-platform success from a local check.
