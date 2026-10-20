Review ID: cohesive-task-readiness
Target: changes/active/wyrd-ui-main-integration/plan.md and tasks/TASK-001 through TASK-026, approved spec revision 9
Gate: Plan
Verdict: Approve
Architecture axis: PASS
Executability axis: PASS
Critical: 0
Major: 0
Task coverage: TASK-001, TASK-002, TASK-003, TASK-004, TASK-005, TASK-006, TASK-007, TASK-008, TASK-009, TASK-010, TASK-011, TASK-012, TASK-013, TASK-014, TASK-015, TASK-016, TASK-017, TASK-018, TASK-019, TASK-020, TASK-021, TASK-022, TASK-023, TASK-024, TASK-025, TASK-026

## Evidence and scope

Independent review of the current 26-task packet, its ordering/obligation map,
approved revision-9 spec, and verification reference. Earlier 61-task reviews
were not used as approval evidence for this target. Applied the
plan-readiness-reviewer rubric, execution, production-risk and public-workflow
references, with current user instructions taking precedence over the stale
four-native-target statement in AGENTS.md.

Checked all task outcomes, boundaries, dependency edges, source-backed reuse,
scenarios, acceptance and visual checkpoints. Compared the complete component
inventory and four hosts with TASK-003–011 and the separate page workflows.
Inspected current UI package/Vitest configuration; SDK WyrdClient.connect,
Cards/verification read surfaces and tests/support/server.ts; server
query/collect.rs and MCP handler; CLI install/dev and current named tests;
Python installed-CLI process tests; and mise identity, Postgres, SDK-build and
startup owners. Confirmed the referenced existing Rust/Python tests and runner
homes from source. Rechecked TASK-003/011 after the parent corrected live MCP
scenario commands to the gated integration owner. No runtime checks ran.

## Findings

No blocking findings.

## Axis assessment

Architecture: PASS. The package, portable loaders and host executors follow the
approved four-host boundary. Node-only SDK imports remain outside browser
exports; authorization and credentials remain on the existing server/session or
MCP caller boundary. Typed actions preserve server-owned behavior. Existing
build, installer, DevServer, nginx and query owners are extended rather than
replaced. TASK-001 remains independent and excludes domain integration; the
foundation owns the first actual analytical SDK consumer and its packaging.
Native outputs and Linux OCI variants are correctly distinguished.

Executability: PASS for the proposed decomposition and conditional ordering.
This does not declare every task immediately runnable against main. Service
correlation/charts, non-sensitive requirement outcomes, Change Requests/Repos,
Ask/saved views and any missing artifact action projection remain explicit
external delivery conditions. Conditional tasks cannot complete by inventing
contracts or presenting unavailable controls as finished. Their conditions also
hold TASK-011 and therefore conventional pages. They require separately approved
platform delivery, not implementation authority from this review.

The grouping is consistent with revision 9: one complete foundation, coherent
component families, production MCP integration, then separate user workflows.
Chart/CardHeader/primitives have one delivery owner. Agent, Prompt, Model,
Traces, Logs, Metrics and Home retain distinct tasks. Related drawer/tab modes
retain sequential human checkpoints inside their workflow task. Every component
still requires individual desktop/mobile, light/dark acceptance; a family-wide
approval cannot replace it. Full-library completion precedes MCP production
registration and all conventional pages.

Focused Vitest titles are expressly planned additions to the existing runner,
not purported existing tests or a replacement selector interface. Managed
setup/build prerequisites are linked. TDD covers changed behavior; deletion and
unchanged login also have static/regression proof. Fixture AppBridge T4 is
available before production registration, avoiding a dependency cycle. Live
T5/T6 and real MCP/native/container proofs remain distinct from fixture tests.

## Task coverage

### Task TASK-001
Outcome and scope: PASS — Distribution/login only; required native and image outcomes stay together.
Ownership and dependencies: PASS — Existing build/installer/DevServer/auth/nginx owners; no domain dependency.
Material decisions: PASS — System Node, exact browser origin, atomic install and owned-process failure cleanup are specified.
Acceptance and verification: PASS — Existing installer/local-dev/identity/startup owners plus distinct native/image and login inspection evidence.

### Task TASK-002
Outcome and scope: PASS — Exact obsolete-UI removal with retained signed-in/settings/login surfaces.
Ownership and dependencies: PASS — Deletion precedes foundation; either ordering with TASK-001 preserves its login proof.
Material decisions: PASS — Delete/keep inventory and plain-markup retained pages follow REQ-032.
Acceptance and verification: PASS — Focused retained-page regression, static import/settings removal, UI check/test/build and existing identity proof.

### Task TASK-003
Outcome and scope: PASS — One complete reusable foundation with Chart, CardHeader and primitives.
Ownership and dependencies: PASS — First analytical consumer closes package/runtime/build, session construction and existing MCP query parameter gap.
Material decisions: PASS — Six approved reads, positional normalization, dependent cancellation, typed actions and browser/server separation are settled.
Acceptance and verification: PASS — T1–T5, real SDK/MCP bound-query proof, custom-app example, built AppBridge artifact and individual component acceptance.

### Task TASK-004
Outcome and scope: PASS — Observe family, including trace/log/metric variants, without conventional pages.
Ownership and dependencies: PASS — Consumes foundation; Service variants explicitly wait for REQ-023.
Material decisions: PASS — Canonical fields, bounded counts, sensitive payload refusal and honest metrics/degraded data are specified.
Acceptance and verification: PASS — Family state/loader and real observation-to-component proofs; each component and signal variant inspected.

### Task TASK-005
Outcome and scope: PASS — Complete Card browsing/definition family, not stale historical workspaces.
Ownership and dependencies: PASS — Reuses foundation CardHeader; artifact action projection gap is a conditional external dependency.
Material decisions: PASS — Exact references, current forms, recorded badges and authorized action outcomes remain authoritative.
Acceptance and verification: PASS — Registry/action journey plus T1–T5 and individual inspection of all eleven members.

### Task TASK-006
Outcome and scope: PASS — Query editor/catalog/details workflow family.
Ownership and dependencies: PASS — Consumes completed foundation and Observe RecordTable; no second query owner.
Material decisions: PASS — Authored SQL/binds, cancellation, terminal limits and exact reproduction are settled.
Acceptance and verification: PASS — Focused interaction and live query parity/refusal proofs, plus individual gallery acceptance.

### Task TASK-007
Outcome and scope: PASS — Generic verification family with exact recorded identities and drift detail.
Ownership and dependencies: PASS — Reuses Card/Observe families; outcome, Service and CR variants have explicit external conditions.
Material decisions: PASS — Selected execution/result/Verifier, pinned group snapshots and server-owned judgments are preserved.
Acceptance and verification: PASS — Projection and real verification-to-component journeys; each graph/result/requirement/drift component inspected.

### Task TASK-008
Outcome and scope: PASS — Service signals and chart authoring form one reusable family.
Ownership and dependencies: PASS — Depends on relevant Query/Card/Observe components and external REQ-023/025 contracts.
Material decisions: PASS — Service versus producer identity and new-version chart save remain server-owned.
Acceptance and verification: PASS — Cross-Service isolation, authorized save and live family proof; both components separately inspected.

### Task TASK-009
Outcome and scope: PASS — Change/Repos collaboration components follow their shared platform contract.
Ownership and dependencies: PASS — Reuses Card/Verify families; explicitly waits for approved CR/repository/action projections.
Material decisions: PASS — Revision consistency, scopes and real action completion are specified; no mock store port.
Acceptance and verification: PASS — Contract-backed T1–T5 and real actions; individual checks for every collaboration/repository component.

### Task TASK-010
Outcome and scope: PASS — AskPanel and saved-view interaction, without an agent service or persistence layer.
Ownership and dependencies: PASS — Consumes foundation and separately shipped Ask/saved-view contracts.
Material decisions: PASS — Viewer-scoped reruns, visibility, retention and document-only storage follow REQ-030/031.
Acceptance and verification: PASS — Deterministic-provider live journey and component/action proof; fixtures alone cannot close completion.

### Task TASK-011
Outcome and scope: PASS — Production MCP resource/tools close the full-library host workflow.
Ownership and dependencies: PASS — Waits for every library family and its human/live completion; reuses earlier T4 artifact.
Material decisions: PASS — Existing MCP auth, generated catalog validation and caller-scoped reads/actions are retained.
Acceptance and verification: PASS — Both real MCP scenarios use gated integration setup; resource/security proof supplements fixture AppBridge checks.

### Task TASK-012
Outcome and scope: PASS — Canvas shell and navigation remain a dedicated app outcome.
Ownership and dependencies: PASS — Starts after full-library/MCP gate and consumes finished primitives/AskPanel.
Material decisions: PASS — Shell controls stay outside catalog; page shaping and credentials stay at their approved owners.
Acceptance and verification: PASS — Focused composition/T6 and individual rail, top bar, picker, drawer and Ask-entry inspection.

### Task TASK-013
Outcome and scope: PASS — Registry inventory and exact detail are one discovery-to-detail workflow.
Ownership and dependencies: PASS — Consumes shell and completed Card family; leaves kind workspaces separate.
Material decisions: PASS — Fourteen kinds, metadata/status and exact relationship/version identity are settled.
Acceptance and verification: PASS — Real registry T6, with inventory inspected before generic exact detail.

### Task TASK-014
Outcome and scope: PASS — Agent workspace is its own inspectable outcome.
Ownership and dependencies: PASS — Composes completed Card/Observe/Verify components and shell.
Material decisions: PASS — Current Agent fields, invoke_agent attribution and honest absence are explicit.
Acceptance and verification: PASS — Focused Agent composition/T6 and desktop/mobile light/dark Agent-only checkpoint.

### Task TASK-015
Outcome and scope: PASS — Prompt workspace is separate and spec-only.
Ownership and dependencies: PASS — Composes completed Prompt/Card family and shell.
Material decisions: PASS — Portable/native authored forms, derived fields, usage links and version diff; no invented telemetry.
Acceptance and verification: PASS — Focused Prompt composition/T6 and long-content/mobile/version-diff inspection.

### Task TASK-016
Outcome and scope: PASS — Model workspace remains separate from Agent and Prompt.
Ownership and dependencies: PASS — Composes finished Card/Observe/Verify components.
Material decisions: PASS — Interface/artifacts and only real attributed signals/recorded drift are specified.
Acceptance and verification: PASS — Focused Model T6 and separate present/missing telemetry and drift inspection.

### Task TASK-017
Outcome and scope: PASS — Data workspace projects current declared schema/stats and recorded feature drift.
Ownership and dependencies: PASS — Reuses Card/Verify loaders and shell; no new Data telemetry/schema owner.
Material decisions: PASS — Exact baseline_ref discovery and Verifier/requirement-scoped feature reads follow the approved decision.
Acceptance and verification: PASS — Focused Data T6 and schema/stats/splits/SQL/drift state inspection.

### Task TASK-018
Outcome and scope: PASS — Service Overview and Charts are one exact-version workflow with separate checkpoints.
Ownership and dependencies: PASS — Consumes completed Service/Verify/Card components; external correlation/chart contracts remain conditions.
Material decisions: PASS — Service isolation and viewer-authorized new-version save are fixed.
Acceptance and verification: PASS — Focused Service T6 and sequential Overview/Charts inspections, including save refusal.

### Task TASK-019
Outcome and scope: PASS — Query page closes discovery, authored execution and table/chart display.
Ownership and dependencies: PASS — Composes finished Query/Observe components and shell.
Material decisions: PASS — SQL/binds and terminal semantics are inherited without page-level reshaping.
Acceptance and verification: PASS — Focused T6 and both Query/QueryChart inspections, including cancelled/denied/degraded states.

### Task TASK-020
Outcome and scope: PASS — Trace/span investigation is one list-to-drawer workflow.
Ownership and dependencies: PASS — Consumes completed Observe family and shell; actual Service filter waits for correlation delivery.
Material decisions: PASS — Root-row counts, real names, bounded detail and pause/new-row behavior are explicit.
Acceptance and verification: PASS — Focused trace T6 and sequential list/span/drawer checks; no merged Logs/Metrics task.

### Task TASK-021
Outcome and scope: PASS — Logs are a separate inspectable page.
Ownership and dependencies: PASS — Consumes completed Observe components and shell; real Service filter is conditional.
Material decisions: PASS — Canonical severity/body/attributes and sensitive-detail refusal are preserved.
Acceptance and verification: PASS — Focused logs T6 and long-body, denied, narrow list/detail inspection.

### Task TASK-022
Outcome and scope: PASS — Metrics are a separate inspectable page.
Ownership and dependencies: PASS — Consumes completed Chart/RecordTable and shell; actual Service scope remains conditional.
Material decisions: PASS — Metric type, recorded shapes and temporality govern display; no fabricated quantiles or zeroes.
Acceptance and verification: PASS — Focused metrics T6 and actual-shape/missing-data/mobile overflow inspection.

### Task TASK-023
Outcome and scope: PASS — Verify workbench and exact Verifier definition form a coherent investigation workflow.
Ownership and dependencies: PASS — Reuses finished Verify/Observe family and shell; outcome/Service prerequisites remain conditions.
Material decisions: PASS — Run/session/drift identities, pinned reviews and plain versus result-colored graph behavior are specified.
Acceptance and verification: PASS — Focused T6; separate run, session, drift and plain-definition human checkpoints.

### Task TASK-024
Outcome and scope: PASS — Home alone owns this summary outcome.
Ownership and dependencies: PASS — Consumes finished attention, Service breakdown, inventory summary and stats; supporting external contracts remain conditions.
Material decisions: PASS — No data/coverage, failed reads, inconclusive results and passes remain distinct recorded facts.
Acceptance and verification: PASS — Focused Home T6 and its own summary inspection; no other summary pages bundled.

### Task TASK-025
Outcome and scope: PASS — Change Request collaboration pages close one revision-centered workflow.
Ownership and dependencies: PASS — Consumes completed collaboration family and shell; no backend CR implementation authority.
Material decisions: PASS — Server revisions, multi-repository/PR scope and authorized actions follow delivered contracts.
Acceptance and verification: PASS — Focused real T6 plus sequential list, Overview, Verification and Audit/Files checkpoints.

### Task TASK-026
Outcome and scope: PASS — Repository connections remain a separate management workflow.
Ownership and dependencies: PASS — Consumes finished repository components and shell; platform connection/webhook contracts remain conditions.
Material decisions: PASS — Server-owned health/configuration and viewer-authorized writes keep secrets out of page data.
Acceptance and verification: PASS — Focused real T6 and connected/disconnected/error/denied mobile/keyboard inspection.

## Limitations

Static readiness review only. Planned tests have not been implemented or run;
no native artifact, image, MCP App or visual acceptance is claimed. External
platform contracts were assessed as declared conditions, not invented or
approved here. Their delivery and human per-component acceptance still hold
the full-library gate. No task/spec/implementation file was changed by this
review. Approval assesses this task decomposition, not implementation readiness
of an unmet external prerequisite or permission to publish.
