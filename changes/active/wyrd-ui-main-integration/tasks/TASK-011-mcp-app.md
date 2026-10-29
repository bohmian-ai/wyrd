# TASK-011 — Production MCP App and catalog discovery

Spec: [revision 11, approved](../spec.md). Status: Proposed.
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-004](TASK-004-observe-components.md), [TASK-005](TASK-005-card-components.md), [TASK-006](TASK-006-query-components.md), [TASK-007](TASK-007-verify-components.md), [TASK-008](TASK-008-service-components.md), [TASK-009](TASK-009-change-repository-components.md).
Mapped obligations: REQ-030, REQ-020, AC-011, AC-010, REQ-035, AC-014; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

The existing MCP endpoint serves the complete UI App resource, validates and shows view documents, and exposes the finished catalog to agents. A real agent can answer an investigation question with an interactive view, refine it on a follow-up and reload its time range entirely within the connected agent/App host.

## Owners, Scope, Consumers, and Prohibited Changes

Full-library gate first: every component/action/live prerequisite and individual visual acceptance in the in-scope TASK-003–009 is complete. TASK-010/Ask/saved views are deferred and not prerequisites. Own resources capability, ui://wyrd/view, ui.show_view, ui.catalog, embedded artifact/catalog and production host verification. Reuse earlier built artifact and MCP executor; do not recreate them. Current MCP protocol/auth remain unchanged.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, production test switches or replacement selectors. REQ-035 permits the minimal test-only real-browser driver and connected App host plus an explicit UI journey choice on the existing readiness CLI; no generic harness framework.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| MCP adapter | `crates/wyrd/wyrd-server/src/mcp/mod.rs::WyrdMcpHandler.get_info/catalog/call_tool`; server Cargo.toml existing jsonschema | Existing cards/bifrost/verification descriptors and dispatch; TASK-003 T4 artifact host | Main advertises tools only and has no UI resource/tools | Extend handler and embed scripts/build.sh outputs | No second MCP endpoint, alternate auth or duplicated catalog/renderer |
| Live agent/session lifecycle | `sdks/wyrd-sdk-python/tests/agent_readiness/live.py::run_agent/exercise/main/preflight/agent_environment/Redactor`, `observe.py::Observer.observe` | `tests/unit/test_agent_readiness.py` exit/deadline/cancellation/redaction checks; `test:agent-readiness:smoke:live` | One objective then stdin closes; no continuation or browser-linked App lifecycle | Extend this owner with a bounded UI journey and actual follow-up/session continuation, preserving default coding-readiness semantics | Conversation proof is required; no new production agent service or second environment controller |
| Owned deployment/docs | `live.py::wyrd_served/docs_served`, `tests/integration/test_local_development.py::local_deployment` | `exercise` reuses local deployment; existing installed-CLI journeys and local-dev mise lane | UI proof does not yet record a connected browser/agent session | Reuse current deployment, checkout docs, local credentials and artifact/redaction owners | No new server lifecycle, local secret format or credential store |
| Real-browser App host | UI `src/lib/ui-library.integration.test.ts::appBridgeView/mcpOver`, `vite.config.ts`, current UI manifests | Existing TASK-003 built artifact fixture and separate authenticated MCP read tests | jsdom patches message sources and uses fixture App replies; no browser driver is installed | Extend existing ext-apps bridge into a real browser with real MCP forwarding and live agent result delivery | Actual rendering/session interaction needs a test-only browser driver; do not assume CLI agents display Apps |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Verify full-library acceptance before production MCP integration.
2. Embed existing generated catalog and complete self-contained HTML through authoritative build owner.
3. Extend resource/tool discovery and stable document validation on the current MCP handler.
4. Exercise real authenticated MCP reads/actions in a real-browser AppBridge host and production resource/tool paths, including host theme/display modes; retain deterministic negative proofs.
5. Extend existing readiness execution for a UI-only multi-turn journey, forwarding actual agent tool results through the connected host to the built app and browser actions back through the authenticated connection.
6. Record conversation, resource/tool, browser and authorized server evidence for each exercised host/agent; preserve the original coding-readiness lane.

## Ordered Implementation Scenarios

### Scenario 1 — Discover and validate UI resources

**Behavior.** Current authenticated MCP discovery exposes resources and two UI read tools; invalid view documents return stable errors, valid documents return structuredContent and correct resource metadata.

**RED.** Add focused MCP resource/tool contract journey; current tools-only handler has none. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-011: Discover and validate UI resources$"'
```

**GREEN.** Use embedded generated catalog with existing jsonschema and derive-backed error catalog. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Sandboxed UI as the caller

**Behavior.** A real browser receives actual production tool-result notifications and renders the requested view in inline/fullscreen and host theme; tool reads/actions use current caller scopes, with denied/errors visible and no direct network/credential exposure.

**RED.** Add focused real-browser MCP App journey over the built artifact and real authenticated MCP forwarding; jsdom/fixture T4 or separate executor reads alone cannot prove production wiring. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-011: Sandboxed UI as the caller$"'
```

**GREEN.** Reuse complete browser artifact/executor and current /mcp lifecycle without protocol downgrade or connectDomains. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 3 — Agent investigation and follow-up in one connected host

**Behavior.** A real agent discovers the catalog, generates an initial view for
an investigation question, and refines it after a follow-up in the same
conversation. Its actual `ui.show_view` results render in the connected host's
real browser. A browser-driven range change reloads affected panels using actual
viewer-scoped reads and consistent bounds, with no stale overwrite. The user
never visits the conventional UI or builds an application. REQ-035/AC-014 apply.
Deterministic scenario 2 covers injected negatives; live-model outcomes are
judged from tool/browser/server evidence, not the final prose or process exit.

**RED.** Add a focused credential-free runner regression for actual follow-up
continuation and result delivery. The existing one-objective runner cannot
satisfy it. Planned addition to the inspected pytest unit owner:

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_mcp_ui_journey_continues_the_conversation_and_delivers_tool_results
```

After the existing live prerequisites, exercise the real UI journey with this
planned extension of the inspected readiness entry point (not an existing flag):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m tests.agent_readiness.live --journey mcp-ui --agent claude --agent codex
```

`--journey mcp-ui` is the one bounded CLI selection added by this task. Omitting
it preserves the current coding-readiness objective and evidence semantics.
Each selected agent needs actual supported session continuation and connected
App-host integration; record BLOCKED for missing credentials/host support,
never a skip or a substituted independent browser fixture. A minimal repository
test host is permitted only under REQ-035's same-session contract; its evidence
does not establish compatibility with an unexercised commercial client.

**GREEN.** Extend existing readiness lifecycle/launch/report owners and the
AppBridge host with the smallest supported session integration. Capture the
actual initial question, follow-up, tool results and browser range interaction;
assert visible outcomes against seeded real server data. Preserve independent
credential isolation, deadlines, cancellation, cleanup and original readiness
success criteria. Rerun the focused runner regression, scenarios 1–2, and this
live journey. A real host integration gap stays BLOCKED.

**REFACTOR.** Reuse existing process, local-deployment, docs, redaction and
artifact owners rather than copying them into a second harness. Keep private
browser/agent adapter shape implementation-owned; do not introduce a production
chat runtime or weaken browser sandboxing to make the test pass.

## Acceptance Criteria

- All catalog components are available in embedded artifact and generated catalog identities agree.
- Invalid input never gains arbitrary tool/URL/code dispatch; real read/action scope failures remain visible.
- REQ-035/AC-014 pass for each selected agent with actual multi-turn context, connected real-browser rendering, range reloads and independently asserted tool/browser/server evidence. A model claiming success, a CLI exit, copied documents or fixture-only App answers do not pass.
- Existing coding-readiness behavior and local credential isolation remain intact; the paid UI journey is explicitly gated and never transitively added to default tests, gate or unattended CI.
- Custom-app npm package contents and runnable example still work; no actual npm publication required.

## Expected Write Set and Consumer Closure

Existing readiness runner/unit tests, local-deployment consumers, bounded CLI journey selection and opt-in mise lane; test-only browser/host support and dependency manifest/lock closure; existing MCP handler/descriptors/resource dispatch/tests; scripts/build.sh embedding/build closure; generated embedded artifact/catalog and exact production App journey under current UI runner. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. No new environment selector. The planned readiness CLI journey choice and minimal real-browser dev dependency are expressly authorized for actual consumers, not selection convenience.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

Run `mise run fmt`, `mise run lints`, `mise run check:deps` and `mise run codegen:check` for MCP/build/generated changes. Add any necessary handler-only Rust checks under the existing server owner; record exact selectors before naming them. Actual resource/tool journey remains the focused production App test above.

For changed Python runner code run `mise run py:format`, `mise run py:lints`
and the exact unit scenario command above. Preserve these inspected regressions:

```bash
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_agent_exit_status_is_reported_with_a_redacted_transcript
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_agent_past_its_deadline_is_settled_with_its_children
mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/unit/test_agent_readiness.py::test_no_gate_or_ci_path_reaches_the_live_lane
```

The UI journey must have an explicit opt-in mise entry using the same existing
readiness owner and setup; the focused `--journey mcp-ui` command above selects
only this task's live workflow. Record actual source/agent/host/browser versions,
sanitized transcripts/tool messages, browser screenshots/traces, visible range
changes and server evidence. Existing readiness credentials stay in environment
or untracked `mise.local.toml`/existing login, never committed fixtures or browser
state. Unit doubles force process failures only; they never establish live-agent
or browser completion. Inspect the exercised host's supported automation boundary
before claiming integration. Missing support blocks scenario 3, not scenarios 1–2.

### Human visual checkpoint

Actual MCP App resource in AppBridge with real authenticated tools; inline/fullscreen, desktop/mobile light/dark. Inspect representative composed view and host-specific behaviors; individual component acceptance is already required. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

No current source proves that Claude Code or Codex CLI is an App-capable host.
Before scenario 3 GREEN, establish actual session continuation and a connected
browser host through supported APIs. If that needs an unapproved production
agent service, auth boundary or external contract, return to specification;
otherwise implement the reversible test adapter locally. Report unsupported
hosts and missing live prerequisites as BLOCKED, without claiming equivalence
from separate CLI/browser runs.

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
