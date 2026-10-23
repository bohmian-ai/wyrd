Review ID: plan-readiness-revision-11
Target: changes/active/wyrd-ui-main-integration/spec.md revision 11 and changed plan/task/verification obligations
Gate: Plan
Verdict: Approve
Architecture axis: PASS
Executability axis: PASS
Critical: 0
Major: 0
Task coverage: TASK-004, TASK-005, TASK-006, TASK-007, TASK-008, TASK-009, TASK-010, TASK-011, TASK-012, TASK-013, TASK-014, TASK-015, TASK-016, TASK-017, TASK-018, TASK-019, TASK-020, TASK-021, TASK-022, TASK-023, TASK-024, TASK-025, TASK-026

## Evidence and scope

Independent static review of the working-tree revision-11 delta: spec.md, plan.md, references/verification.md and TASK-004 through TASK-026. TASK-003 is unchanged; this review does not reissue historical implementation or readiness verdicts. User alignment and the explicit wyrd-plan request authorize the revised outcomes. Existing conditional platform prerequisites remain conditional.

Read the plan-readiness rubric, execution-readiness and public-workflow references; repository agent rules, spec-driven execution/testing guidance and relevant protocol doctrine. Checked changed task scenarios, dependencies and selectors against the UI package.json/vite.config.ts, current ui-library.integration.test.ts AppBridge/MCP owners, tests/integration/identity_ui_e2e.rs BFF lifecycle, live.py run_agent/main/environment lifecycle, test_agent_readiness.py regressions and mise.toml live lane.

Current AppBridge proof uses jsdom and fixture answers. The Rust identity driver selects production-auth.integration.test.ts, not library/page tests. The live runner sends one objective and closes stdin; it has no current --journey option or browser integration. The revised packet describes these gaps accurately and assigns their closure, rather than claiming existing proof.

## Findings

No blocking findings.

## Axis assessment

Architecture: PASS. Production MCP behavior stays on the existing authenticated handler and shared artifact/renderer. Browser rendering does not create an agent service or new durable source of truth. Ask/saved-view deferral is consistent across scope, inventory, shipping order and consumers. Viewer authorization, sandboxing and tenant boundaries remain required.

Executability: PASS for the reviewed revision delta. TASK-011 owns the connected agent/App/browser journey and minimum test tooling; TASK-012 owns deployed page/auth closure; each later page owns its browser workflow. Planned commands are labeled additions to existing runner owners. The repository test-host allowance settles the material acceptance boundary: actual agent conversation, real MCP results/identity and browser App lifecycle must be connected, without asserting commercial-client compatibility. Choosing supported test adapters is reversible. Unsupported integration and absent credentials block live acceptance; they do not permit fixture substitutes. Remaining backend prerequisite gates are explicit and unchanged.

## Task coverage

### Task TASK-004
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-005
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-006
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-007
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-008
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-009
Outcome and scope: PASS — Revision metadata updated; existing component outcome and scenarios remain intact.
Ownership and dependencies: PASS — Existing foundation/family dependencies and explicit platform prerequisites remain unchanged.
Material decisions: PASS — No new material component contract introduced; deferred Ask is excluded by parent specification.
Acceptance and verification: PASS — Existing focused scenario commands and per-component inspection remain; this delta does not claim runtime success.

### Task TASK-010
Outcome and scope: PASS — Ask and saved views explicitly superseded from active delivery.
Ownership and dependencies: PASS — No implementation dependency remains; renderer and MCP owners are retained.
Material decisions: PASS — Deferral is an explicit approved scope disposition, not an invented backend contract.
Acceptance and verification: PASS — Static proof strategy checks inventory, dependencies and preserved acceptance; manufactured RED is correctly omitted.

### Task TASK-011
Outcome and scope: PASS — Production discovery/App delivery plus connected multi-turn agent investigation and range reload have observable criteria.
Ownership and dependencies: PASS — Extends existing MCP handler, readiness runner, deployment lifecycle and AppBridge; no second production harness.
Material decisions: PASS — Same-session evidence and repository-host allowance are explicit; external host compatibility is claimed only when exercised.
Acceptance and verification: PASS — Three ordered scenario blocks include deterministic real-browser/MCP negatives, a planned exact runner regression, and opt-in live command; original coding-readiness regressions are named correctly.

### Task TASK-012
Outcome and scope: PASS — Conventional shell has real-browser navigation and truthful interaction proof; Ask controls are excluded.
Ownership and dependencies: PASS — Reuses TASK-011 browser tooling and existing BFF/deployment/auth owners; page-specific work remains downstream.
Material decisions: PASS — Public auth and viewer-scoped real data are required; jsdom, direct handler imports and cookie simulation cannot satisfy acceptance.
Acceptance and verification: PASS — Existing exact T6 selector owns the new browser workflow; narrow setup/build requirements are supplied by verification guidance.

### Task TASK-013
Outcome and scope: PASS — A user searches and pages through Cards, opens an exact version, and follows exact relationships/artifacts/YAML without losing identity.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-014
Outcome and scope: PASS — The exact Agent version shows its registered Prompt, execution route, runtime tools, composition, bindings and attributable activity.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-015
Outcome and scope: PASS — The exact Prompt version is readable as messages, derived variables, response format, using Agents and version diff.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-016
Outcome and scope: PASS — The exact Model version shows declared interface, signature, artifacts, relationships, actual attributed signals and recorded drift.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-017
Outcome and scope: PASS — The exact Data version exposes schema, stats, splits, target columns, SQL and recorded per-feature drift tied to its baseline references.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-018
Outcome and scope: PASS — An exact Service version has its component Overview, isolated signals, versioned Charts, Versions and YAML.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-019
Outcome and scope: PASS — A user discovers a table, describes it, executes an authored bounded read-only query and switches between table/chart results.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-020
Outcome and scope: PASS — A user filters traces or flat spans, opens an exact trace drawer and investigates its waterfall and span details without losing list position.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-021
Outcome and scope: PASS — A user time-bounds and filters canonical logs and opens the real record body/attributes with honest sensitive-payload refusal.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-022
Outcome and scope: PASS — A user reads canonical metric points over a selected range with truthful chart/table behavior for the actual metric shape.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-023
Outcome and scope: PASS — A user finds failing recorded results, opens exact run/session/drift detail, follows requirement citations, and opens the exact Verifier definition.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-024
Outcome and scope: PASS — Home summarizes actual attention, Service signal breakdown, Card inventory and observed activity without confusing no data with passing verification.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-025
Outcome and scope: PASS — A user browses Change Requests and investigates a revision, its Subjects, Verifiers, conversation, review, audit and changed files.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

### Task TASK-026
Outcome and scope: PASS — A user manages real repository webhook connections, the wyrd.toml location and its Verifier folder through the approved server contract.
Ownership and dependencies: PASS — Existing page/family dependency closure is preserved; shared browser and deployed-process support comes from TASK-011/012.
Material decisions: PASS — REQ-034 fixes real-browser public login, identity/range behavior and negative flows without prescribing private fixtures.
Acceptance and verification: PASS — Scenario 2 retains its exact focused Vitest selector and adds real navigation, visible/server results and relevant range/filter behavior; human page acceptance remains separate.

## Limitations

No production edits, builds, browser runs, live model calls or paid tests. This is readiness of the revision delta, not proof that new --journey mcp-ui or browser support exists today. Supported agent continuation/App bridging must be demonstrated during TASK-011; missing support blocks AC-014 and therefore its dependent completion. A repository test host establishes only its own behavior. Existing external backend prerequisites and individual human visual checkpoints remain delivery risks.
