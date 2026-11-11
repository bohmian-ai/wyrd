# TASK-012 — Production MCP App and connected investigation

Spec: [revision 28, approved](../spec.md). Status: Proposed; final completion is conditional on the full-library, histogram-projection and supported live-host prerequisites below.
Dependencies: [TASK-005](TASK-005-cards-and-prompts.md), [TASK-006](TASK-006-query-workbench.md), [TASK-007](TASK-007-verification.md), [TASK-008](TASK-008-card-workspaces.md), [TASK-009](TASK-009-changes-and-repositories.md), [TASK-010](TASK-010-service-workspace.md), [TASK-011](TASK-011-home-dashboard.md), and completed foundation/Observe/view-layout work.
Mapped obligations: REQ-020, REQ-030, REQ-035, REQ-036, REQ-051 (revision-21 MCP carry-forward), REQ-062; AC-010, AC-011, AC-014, AC-015, AC-016, AC-041; INV-001–INV-007 and applicable histogram fidelity/estimate constraints apply.
Absorbs the former production MCP App task. Component-specific catalog, loader, executor, action, package and built-artifact proof stays with each paired delivery task; this task proves the complete production/connected host.

## Outcome and Value

A user asks an investigation question in an MCP App-capable agent harness and
receives an interactive Wyrd view, refines it in the same conversation, and
changes its time range without visiting the conventional UI or building an
application. The existing `/mcp` endpoint exposes the complete component catalog
and one embedded App resource. Reads and authorized actions run as the viewer.

## What Changes and What Must Stay True

Finish production MCP registration and security/interaction closure on the
current handler, reusing any integration already delivered incrementally:

| Surface | Required contract |
|---|---|
| Resources discovery/read | Advertise resources without changing the sole supported protocol/auth lifecycle. Serve `ui://wyrd/view`, MIME `text/html;profile=mcp-app`, from the self-contained package artifact built by `scripts/build.sh` and embedded at compile time. |
| `ui.catalog` | Read tool returning the generated `catalog.json` for the same built component/artifact identity. No second handwritten catalog. |
| `ui.show_view` | Read tool taking the existing validated view document. Reject invalid version/catalog/component/input/layout with a stable derive-backed Wyrd error; valid output contains the document in `structuredContent` and `_meta.ui.resourceUri = "ui://wyrd/view"`. It executes no arbitrary input URL, JavaScript or tool name. |
| Installed agent transport | The existing `wyrd mcp proxy` stdio bridge advertises and forwards resource list/read as well as tools/prompts. Preserve upstream resource metadata, content, protocol errors, caller identity and existing protocol negotiation; no proxy-owned catalog or credentials. |
| Embedded App | Consume actual `ui/notifications/tool-result` documents, render through the existing `WyrdView`, and load through the existing MCP executor with current-caller `tools/call`. Honor `hostContext` theme and inline/fullscreen. No credential exposure, direct backend fetch, additional endpoint or CSP `connectDomains`. |
| Host actions | Validate catalog-declared JSON-safe intents, dispatch only supported capabilities under actual caller permissions and return pending/success/denied/error/unavailable state. Read executors stay read-only. Unavailable controls never complete a required production action. |

Every active inventory component/action must already have its package exports,
generated types/catalog, direct use, portable loader, Node/MCP parity, gallery,
individual human acceptance and first conventional consumer where required.
Final complete-library acceptance waits for all real platform prerequisites;
the preceding slices' production adapter work does not waive this gate.
Verify the runnable custom-app example and published package contents still
work without npm publication. Deferred Ask, saved views, A2UI export, a new
production conversation service and durable layout persistence stay excluded.

Preserve accepted view-layout behavior: a host with a layout handler receives
whole updated documents after pointer/keyboard edits; without it layout remains
read-only. Keep grid bounds/collision behavior, panel-local overflow, narrow
stacking and documents without layout. MCP layout edits last for that render.

The live runner extends the existing coding-readiness entry point only with
`--journey mcp-ui` and an explicit opt-in mise entry. Omitting the choice retains
the original coding objective, evidence, credential isolation, deadline,
cancellation and cleanup. The agent conversation and browser App host must share
the actual MCP connection and tool results. A copied document, fixture tool
reply, model self-report, exit status or independent CLI/browser run cannot
establish this journey. A repository test host may prove its own integration;
commercial-client claims require exercising that client through supported APIs.

### Histogram carry-forward

Revision 21 deferred the public view-document projection of metrics to MCP
delivery. The shared histogram loader exists, but a catalog/input/action
projection for it is not approved by consolidation. Before activating the
histogram scenario, obtain an approved projection compatible with the existing
`MetricInput`/`loadMetricSeries` workflow and current generated/catalog consumers.
No implementer may invent a public metric panel contract from screenshots or
silently treat SQL-only Chart support as histogram completion. This is an
explicit remaining prerequisite, not permission to alter the conventional
Metrics contract or defer the obligation again without approval.

## Approved Canvases

This task reuses the component designs from the exact approved canvases below in the MCP App host. Preserve those designs while adapting to the host viewport, theme and inline/fullscreen presentation; this task does not authorize a separate MCP visual redesign. Retain the individual component comparisons and acceptance evidence from their owning tasks.

- [Cards.dc.html](../references/canvas/Cards.dc.html)
- [Prompt.dc.html](../references/canvas/Prompt.dc.html)
- [Query.dc.html](../references/canvas/Query.dc.html)
- [QueryChart.dc.html](../references/canvas/QueryChart.dc.html)
- [Verify.dc.html](../references/canvas/Verify.dc.html)
- [VerifySession.dc.html](../references/canvas/VerifySession.dc.html)
- [VerifyDrift.dc.html](../references/canvas/VerifyDrift.dc.html)
- [Verifier.dc.html](../references/canvas/Verifier.dc.html)
- [Agent.dc.html](../references/canvas/Agent.dc.html)
- [Model.dc.html](../references/canvas/Model.dc.html)
- [Data.dc.html](../references/canvas/Data.dc.html)
- [Changes.dc.html](../references/canvas/Changes.dc.html)
- [ChangeDetail.dc.html](../references/canvas/ChangeDetail.dc.html)
- [ChangeVerification.dc.html](../references/canvas/ChangeVerification.dc.html)
- [Repos.dc.html](../references/canvas/Repos.dc.html)
- [Service.dc.html](../references/canvas/Service.dc.html)
- [ServiceCharts.dc.html](../references/canvas/ServiceCharts.dc.html)
- [Home.dc.html](../references/canvas/Home.dc.html)
- [Main.dc.html](../references/canvas/Main.dc.html)
- [Spans.dc.html](../references/canvas/Spans.dc.html)
- [TraceDrawer.dc.html](../references/canvas/TraceDrawer.dc.html)
- [Logs.dc.html](../references/canvas/Logs.dc.html)
- [Metrics.dc.html](../references/canvas/Metrics.dc.html)
- [TimePicker.dc.html](../references/canvas/TimePicker.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Reconcile complete-library component/action/live/human evidence and close current package/custom-app regressions.
2. Embed the generated artifact/catalog through the authoritative build and extend the current MCP resource/tool dispatch.
3. After its projection is approved, expose existing metric/histogram loading through the shared view renderer without duplicating algorithms.
4. Prove the built production resource in a real-browser AppBridge host with real current-caller MCP forwarding and deterministic negatives.
5. Extend the existing readiness lifecycle for supported multi-turn continuation, actual tool-result delivery and browser interactions.
6. Run the opt-in live journey and inspect the actual App; record only exercised agent/host compatibility.

## Ordered Implementation Scenarios

### Scenario 1 — Production discovery and document validation

**Behavior.** Authenticated MCP discovery advertises the resource and read
tools, and the resource/catalog identities match the build. A valid document
returns the required structured content/metadata. Invalid component/input,
version, catalog and layout fail with stable structured errors and cannot
dispatch arbitrary actions; an invalid panel in an otherwise usable document
retains the existing panel-error/sibling behavior where the validator permits it.

**RED.** Add planned [production discovery proof](#production-discovery-proof)
using the actual MCP endpoint and built server. Assert resources/list/read,
tool discovery/calls, MIME, metadata, built identity, existing protocol behavior
and invalid-document refusals. Add the [compiled proxy resource proof](#compiled-proxy-resource-proof) through the installed agent transport and retain its existing shared-auth regression. A direct server resource test cannot establish the stdio host path; current tools/prompts-only advertisement cannot pass.

**GREEN.** Extend the existing handler and build embedding with generated
catalog validation and derive-backed errors. Retain unchanged authentication
and write-scope dispatch. Reuse preceding integration rather than replace it.

**REFACTOR.** Share existing handler/build/catalog owners; avoid a second
endpoint, validator, renderer or error-projection implementation.

### Scenario 2 — Metrics and histograms in authored views

**Behavior.** Once the separately approved projection is supplied, a valid
authored metric/histogram view uses the shared metric loader and the same
recorded data/series identity as conventional Metrics. It preserves current
explicit-bucket mean/rate/distribution and labeled estimated percentiles,
reset/missing/zero handling, incompatible-series separation and precision.
Unsupported kinds and denied/error/truncated reads stay truthful. A range
change uses consistent bounds and never accepts superseded results.

**RED.** Add planned [metric view proof](#metric-view-proof) only after the
approved projection is packet-local. Exercise its schema/catalog/direct
renderer/loader and built artifact, then real MCP reads in scenario 3. Assert
parity with recorded conventional loader output; a generic SQL Chart does not
satisfy the missing metric projection.

**GREEN.** Connect the approved catalog projection to existing
`loadMetricSeries` and histogram transformations. Close generated inputs,
public exports, gallery and actual built-artifact rendering; rerun discovery
and metric proof. No duplicate metric query/shaping owner.

**REFACTOR.** Keep transport in executors and histogram semantics in the
existing shared Observe owner. Private host adapters remain implementation-owned.

### Scenario 3 — Sandboxed App as the caller

**Behavior.** A real browser receives production tool-result notifications and
renders actual resource HTML in inline/fullscreen and host themes. Panel reads
and supported actions use the actual MCP caller's scopes. Force malformed
inputs, empty data, table/tenant denial, failed reads/actions and overlapping
range requests: visible stable problems remain local, healthy siblings remain
usable, denied prerequisites trigger no dependent reads and stale completions
cannot replace newer state. No secrets or backend network access enter the App.
Exercise accepted pointer/keyboard layout behavior and narrow stacking.

**RED.** Add planned [sandboxed production proof](#sandboxed-production-proof)
with the existing real-browser lifecycle and ext-apps bridge, forwarding actual
authenticated MCP calls to the real server. Assert resource delivery, viewer
identity, requested bounds, visible states, sibling isolation and cleanup.
Fixture-only T4 and separately successful executor tests cannot supply GREEN.

**GREEN.** Reuse the built App, MCP executor and established browser lifecycle
with supported AppBridge forwarding. Rerun all applicable earlier scenarios;
keep refusals request/panel-local without terminating the shared server.

**REFACTOR.** Reuse owned browser/process cleanup and credential isolation;
never weaken the sandbox or add connectDomains to make forwarding work.

### Scenario 4 — Connected agent investigation and follow-up

**Behavior.** For every selected agent, a natural-language investigation causes
catalog discovery and a generated view that renders in the connected real
browser host. A same-conversation follow-up refines it. A browser range change
reloads affected panels with real viewer-scoped reads, consistent bounds and
correct visible results. Missing credentials/continuation/host support is
reported BLOCKED per affected agent, never skipped or claimed from doubles.
Deadline, interruption, server failure and failed App reads settle owned
processes/requests without changing other agents' evidence or the coding lane.

**RED.** Add planned [continuation regression](#continuation-regression) with
credential-free process/host doubles asserting two turns, actual result delivery,
failure reporting and owned cleanup. The current runner closes stdin after
one objective. Then use the [opt-in live proof](#opt-in-live-proof): doubles
cannot establish the live catalog/conversation/browser outcome.

**GREEN.** Extend existing readiness/session/report owners with the smallest
supported test integration. Capture sanitized initial/follow-up exchanges,
actual tool/resource messages, browser rendering/range interactions and server
readback. Rerun scenarios 1–3, the runner regressions and every selected live
agent. An unsupported host remains BLOCKED.

**REFACTOR.** Keep existing deployment/docs/process/redaction owners; do not
introduce a production agent runtime or generic harness/controller framework.

## Acceptance Criteria

| Obligation | Required observation and proof |
|---|---|
| REQ-030; AC-011/016 | Complete generated catalog, package/artifact identity and supported production resource/tools agree. Scenario 1 including compiled stdio proxy proof, plus preceding per-component T1–T5/action/visual evidence; custom-app example still builds and runs with viewer-owned server credentials. |
| REQ-051 carry-forward | Approved metric projection renders real metric/histogram content using shared semantics; scenarios 2–3 prove portable view and live read parity. Without that approval this obligation and task completion remain blocked. |
| REQ-030/036; AC-015 | Actual browser App follows host themes/modes, existing layout behavior and viewer scopes; scenario 3 proves negative cases, sibling isolation, tenant separation and no secret/direct-network escape. |
| REQ-035; AC-014 | Each selected real agent demonstrates discovery, generated rendering, contextual follow-up and browser range reload in one connected session with independently checked tool/browser/server evidence. Scenario 4; no fixture, prose or exit-only substitution. |
| REQ-020/062; AC-010/041; INV-001–007 | Component acceptance and preceding conventional dev:full checkpoints remain intact. Actual MCP App receives separate desktop/mobile light/dark human inspection. Missing runtime/human evidence stays unverified. |
| Existing readiness contract | Original coding objective/default invocation, redaction, deadline/child cleanup, cancellation and opt-in-only live scheduling remain unchanged. Focused runner regressions below. |

## Implementation Reference

Paths identify existing owners and consumers, not a private-file allowlist.

- **Installed agent path:** `crates/wyrd/wyrd-cli/src/mcp/proxy.rs::McpProxy`
  `get_info/list_tools/call_tool/list_prompts/get_prompt/run` currently bridges
  only tools/prompts. Host installation in the CLI's `mcp/hosts.rs` routes
  agents through this compiled stdio bridge; close resource capability,
  pagination/list and read forwarding here too. Reuse `upstream_error`, the
  authenticated upstream transport and current lifecycle. Existing
  `tests/cli.rs::mcp_proxy_discovers_and_reads_with_shared_auth` and
  `tests/mcp_journey.rs::{Proxy::spawn,list_tables_through_proxy}` drive the
  compiled proxy against `WyrdTestServer::start_bound`; extend that owner,
  preserving permission refusals, protocol negotiation and cleanup.
- **MCP:** `crates/wyrd/wyrd-server/src/mcp/mod.rs::WyrdMcpHandler`
  `get_info/catalog/get_tool/list_tools/call_tool` owns current tools/prompts
  advertisement, per-caller write discovery and dispatch. Existing cards,
  Bifrost and verification descriptors are the read precedents. The current
  handler advertises no resources; extend it using existing `jsonschema` and
  error/catalog ownership, not an alternate auth or protocol path.
- **Artifact/catalog:** `packages/ui/package.json` already exports components,
  loaders, view, catalog, Node/MCP executors and `wyrd-view.html`.
  `packages/ui/scripts/catalog.ts`, `vite.app.config.ts`, `src/lib/mcp.ts` and
  `WyrdView.svelte` own the reusable build/runtime boundaries. `scripts/build.sh`
  remains the sole distribution build owner. Close embedded paths and actual
  runtime package contents, not just source exports.
- **Histogram reuse:** `packages/ui/src/lib/observe.ts::MetricInput`,
  `loadMetricSeries`, `HistogramProps`, `histogramQuery` and `histogramWindows`
  already serve the conventional Metrics layout. `schemas.ts::ChartInput`
  currently expresses SQL charts, so the missing approved metric catalog
  projection is explicit; do not manufacture a compatible-looking schema.
- **Browser/MCP proof:** `src/lib/ui-library.integration.test.ts::appBridgeView`
  uses jsdom/fixture tool replies and `mcpOver` provides actual authenticated
  MCP reads. Neither is connected-browser proof alone.
  `src/lib/ui-pages.integration.test.ts::startBff` and its Playwright fixture
  already own production BFF/server/browser cleanup; reuse that lifecycle for
  the real AppBridge test instead of introducing another runner.
- **Live agent:** `sdks/wyrd-sdk-python/tests/agent_readiness/live.py`
  `run_agent/exercise/main/preflight/agent_environment/Redactor` owns local
  configuration, one-objective execution and sanitized reports;
  `observe.py::Observer.observe` owns evidence readback. Its current CLI has no
  journey choice or connected continuation. Extend that owner and its existing
  `tests/unit/test_agent_readiness.py` callers without changing default coding
  semantics. `wyrd_served/docs_served` and the local-development test owner
  provide server/docs lifetime; no new environment controller or secret store.
- **Custom consumer:** `examples/ui-app/src/executor.ts::executor` already
  wraps `nodeExecutor(WyrdClient.connect())` server-side, and the example has
  direct/view-document pages. Reuse it for package/custom-app regression.

## Verification and Evidence

New selectors below are **planned additions** to inspected existing runners.
Confirm their actual names after adding them, before RED; zero-selected/skipped
tests are not proof. [Managed setup](../references/verification.md) supplies
current SDK/testing addon, server, UI/package/artifact builds and repository
Postgres. Reuse installed Playwright browser support. Do not create a selector
environment. Scenario 2 waits for its approved public projection.

### Production discovery proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^production mcp discovers and validates ui resources$"'
```

### Compiled proxy resource proof

The first selector is a **planned** root test in the existing CLI integration
target; the second is the existing shared-auth regression. Build current
CLI/server/UI artifacts through existing owners. Reuse the CLI journey's
managed Postgres/all-migrations setup, `Proxy::spawn` and real bound server:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E "test(=mcp_proxy_discovers_and_reads_ui_resources)"'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli -E "test(=mcp_proxy_discovers_and_reads_with_shared_auth)"'
```

Assert host initialization exposes resources; list/read `ui://wyrd/view` and
call `ui.catalog`/`ui.show_view` through compiled stdio, preserving the server's
MIME, metadata, artifact/catalog identity and structured errors. Assert rejected
resource/caller and upstream failure remain truthful, tools/prompts and the
existing protocol bridge work, and EOF/interruption cleans up the proxy.
Direct HTTP/server-only proof cannot close this consumer.

### Metric view proof

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^metric views reuse recorded histogram semantics$'
```

### Sandboxed production proof

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^sandboxed production app uses the current caller$"'
```

### Continuation regression

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_mcp_ui_journey_continues_the_conversation_and_delivers_tool_results
```

### Opt-in live proof

This is a planned extension of the inspected entry point, not an existing flag:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m tests.agent_readiness.live --journey mcp-ui --agent claude --agent codex
```

The task adds an explicit opt-in mise entry with current readiness build/docs/
wheel/server and UI/browser prerequisites. Preserve the original default lane;
the live UI lane must stay outside default tests, `gate` and unattended CI.
Local credentials remain in existing environment/untracked local configuration
or isolated agent login, never fixtures, reports or browser state. Record
source/agent/host/browser identities, sanitized prompts/follow-ups/tool messages,
actual resource delivery, screenshots/traces, bounds and authorized server
results, deadlines and cleanup. Claim only the exercised host/agents.

### Existing runner regressions

These exact tests exist in the inspected pytest owner:

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_agent_exit_status_is_reported_with_a_redacted_transcript
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_agent_past_its_deadline_is_settled_with_its_children
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_cancellation_settles_the_agent
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_no_gate_or_ci_path_reaches_the_live_lane
```

### Scoped checks and visual checkpoint

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- pnpm --dir examples/ui-app check
mise exec -- pnpm --dir examples/ui-app build
mise run fmt
mise run lints
mise run check:deps
mise run codegen:check
mise run py:format
mise run py:lints
mise run dev:full
```

Apply Rust/build/generated checks to the actual MCP/build changes and Python
checks to changed readiness code. Do not run broad aggregate/journey suites
per task; integrated change review owns combined first-class surface proof.
For manual inspection, use normal `dev:full` data and the existing CLI host
installation/auth flow to connect the supported host to that running server.
Retain its usable conventional routes and gallery, including any newly approved
metric projection; do not require hidden manual writes or copy credentials into
the App. Inspect the actual `ui://wyrd/view` resource in the supported connected host,
with real authenticated tools, inline/fullscreen, desktop/mobile and both
themes; verify keyboard/focus/touch/long-content/overflow and placed/resizable
layout. Gallery fixture evidence supplements that host inspection. Preserve
individual component evidence from the earlier slices; this checkpoint does
not replace it. Missing live/human evidence cannot become PASS.

## Material Stop Conditions

Missing platform projections/components/actions or individual human acceptance
block full-library completion. The histogram scenario cannot activate without
an approved catalog/input/action projection. Complete independent production
resource/security work while reporting these dependencies precisely.

Before connected-agent GREEN, establish supported continuation and an actual
connected App host. Unsupported host automation or missing live credentials
blocks that agent's proof, not permission/fixture substitution. A solution
requiring a production agent service, new auth boundary or material external
contract returns to specification. Private test adapter/lifecycle wiring remains
implementation-owned where it preserves the stated boundary and proof.

## Authority Links

- [Approved spec](../spec.md), [plan and coverage](../plan.md), [verification owners](../references/verification.md), [approved canvases](#approved-canvases).
- [Repository rules](../../../../AGENTS.md), [execution](../../../implementation-execution.md), [TDD](../../../spec-driven-development.md), [testing](../../../../TESTING.md).
- [Wyrd design](../../../../architecture/wyrd-design.md), [security](../../../../architecture/wyrd-security-posture.md), [UI skill](../../../../.agents/skills/wyrd-ui/SKILL.md), [Python rules](../../../../sdks/wyrd-sdk-python/AGENTS.md).
