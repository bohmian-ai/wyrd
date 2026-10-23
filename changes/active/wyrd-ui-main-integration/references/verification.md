# Verification owners for the UI task packet

This is a planning reference, not another test/build framework. Original source is committed main `8651b96fd`; revision-11 browser/agent ownership was inspected at `cbd4e97c8`. Commands were inspected, not executed in planning.

## Revision-10 remediation proof

The current assignment is
[TASK-001-R1](../review/tasks-001-002-843484f6c/TASK-001-R1-installation-browser-sessions.md).
The setup described below is not currently a standalone identity prerequisite:
`test:identity:journey:inner` owns provider recreate/seed/cleanup and then all
journeys. Remediation must factor that existing lifecycle once so the original
full lane and a focused command use the same owner and cleanup. It must not
copy provisioning into a second script, add a test selector environment
variable, or require the full suite for each task iteration.

Native identity and real nginx identity remain distinct proof paths. Existing
startup-common builds Debian only, so dev/prod startup commands alone do not
establish the selected Debian amd64/arm64 and Rocky amd64 matrix. Record exact
image/source identity and native bundle digest/runtime per runner. Missing
Linux or human evidence remains unverified; it is not permission to weaken a
check or claim another platform passed.

## Existing owners and setup

- UI `package.json` owns `check`, `test` (`vitest run`), and `build`. `vite.config.ts`
  includes `src/**/*.{test,spec}.{js,ts}` and enables real `.integration.test.ts`
  only through existing `WYRD_UI_INTEGRATION`. Do not introduce a test-only setting.
- The exact planned consumer test files in tasks live under that existing include
  owner: `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts`,
  `ui-pages.test.ts`, and `ui-pages.integration.test.ts`. They import public package
  exports, not private implementation modules. These are new planned tests, not
  existing tests. Their exact names are task-owned focused selectors; no private
  helper, fixture structure or production module layout is prescribed.
- TASK-003 wires these proofs into the existing UI runner/setup, including
  Svelte Testing Library, actual package exports, ext-apps AppBridge and public
  `@wyrd/testing::startTestServer`. Each test records actual selected/running cases;
  a selector that only skips tests is not a pass. Use in-owner equivalent command
  correction if source layout changes and record the corrected exact command.
- Real Node-lifetime proof uses the existing TS test-server owner. Inspect
  `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts::serverTest` and
  `sdks/wyrd-sdk-ts/testing` before adding any narrow private fixture support.
  Do not initialize Node/Python lifetimes inside Rust tests or port an engine fixture
  in place of the real SDK/server journey. Fixtures seed data via existing registered
  Cards/verification/observation workflows; no public writes to protected result tables.
- Before a focused Node UI data journey: `mise run ts:build`, `mise run ts:build:testing`,
  `mise run build:server`, install the task's workspace dependencies through its
  existing package owner, and build current UI/library/artifact outputs. Its command
  then uses `scripts/postgres/with-test-postgres.sh` and `db:migrate:inner`, as the
  current `ts:test:integration` owner does. These are setup, not a new test selector API.
- New renderer tests cover T1/T2/T3 and the actual built-artifact T4 fixture host.
  New live component tests cover T5 and derive sanitized populated/empty fixtures.
  New page tests cover real-browser T6. Primitives without reads have no fake loader/T5 test.
  Missing prerequisites defer actual completion; no manufactured RED is required
  for already-correct behavior or static configuration/deletion obligations.

## Existing specifically named regressions

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_preserves_previous_on_failure)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'
```

The production BFF identity test needs the real repository-managed identity environment.
The setup is owned by `mise.toml` `test:identity:journey:inner`: Compose Keycloak/Dex
recreation, seeded users, ordinary Postgres migration lifecycle, UI build and TLS fixture.
Use that existing setup unchanged; do not copy it into another script or add target/filter
variables. During task iteration, with that setup provisioned, run exactly:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

The existing `WYRD_UI_FILTER` selects tests only within this original identity driver;
it is not a new general selector surface. New component/page journeys use ordinary
Vitest exact selectors and the existing managed test server instead. TASK-001 extends
identity proof through actual native/nginx routing; its current synthetic gateway
function does not prove the new production route. Installed native lifecycle proof
also reuses `test_local_development.py` under the existing `test:local-dev:journey`
owner when process interrupt/exit assertions need a Python process lifetime.

Linux image proof belongs to `test:server:startup:dev` and `:prod`; their shared
startup-common.sh builds through scripts/build.sh and records image/source identity.
TASK-001 reconciles its current Ubuntu default with the approved Debian/Rocky outputs.
Native Darwin and Linux artifact proof is separate and uses the correct native runner.

## Required evidence per component

Record exact selected commands and real outcomes, TDD RED/GREEN/refactor where behavior
changes, stable refusal codes, sanitization/fixture provenance and T1–T5 applicability.
Record desktop/mobile and light/dark human acceptance individually, including each
component in a grouped task, with keyboard/focus, touch, long content and overflow.
An unchanged/static item receives static/regression proof, not a fake failing test.
The in-scope library (excluding deferred Ask/saved views) waits for every component/action/live prerequisite; fixture readiness
is never silently promoted to production completeness. MCP production sandbox/security
proof in TASK-011 supplements (and is not replaced by) AppBridge fixture tests.

## Revision-11 primary deployed journeys

**Conventional UI (REQ-034/AC-013).** TASK-012 and each TASK-013–026 scenario 2
retain their existing exact `ui-pages.integration.test.ts` Vitest selector but
must drive a real browser against the production BFF/server. Calling a page
load/POST function or simulating cookies over HTTP is supporting proof only.
TASK-011 supplies the first real-browser test tooling; TASK-012 closes deployed
page lifecycle and auth integration, and each page owns its workflow and
applicable range/identity/negative assertions. Browser automation may be headless;
record actual engine/version, navigation, interactions and visible/server results.
Minimal test-only browser dependencies are authorized; no new production/test
environment switch or alternate selector framework is authorized.

Inspect/reuse `identity_ui_e2e.rs::start_bff/production_ui_bff_journey` for real
BFF lifecycle and `test_local_development.py::local_deployment` for installed
production UI and owned database. The former currently launches only
`production-auth.integration.test.ts`; it does not automatically run the library
or page files. Foundation `ui-library.integration.test.ts::appBridgeView` uses
jsdom, patched postMessage/ResizeObserver and fixture App tool replies; its
separate real SDK/MCP reads do not establish a connected browser journey.
Do not duplicate these owners or describe their present proof as a browser run.

**Agent/MCP App (REQ-035/AC-014).** Existing live ownership:
`sdks/wyrd-sdk-python/tests/agent_readiness/live.py`, `observe.py`,
`tests/unit/test_agent_readiness.py`, and `test:agent-readiness:smoke:live`.
The current CLI supports `--agent`, `--timeout`, `--artifacts` and one objective
only; no browser, follow-up or UI scenario flag exists. TASK-011 explicitly adds
`--journey mcp-ui` plus an opt-in mise entry using this owner. Default coding
readiness stays unchanged. Planned focused live command after that extension:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m tests.agent_readiness.live --journey mcp-ui --agent claude --agent codex
```

This is a planned interface, not a command claimed to work today. Use existing
`py:setup`, `docs:install`, built CLI/server/tests, current production UI/library
and checkout wheel/docs prerequisites from the inspected readiness/local-dev
owners. The UI addition must close the UI build/browser-install prerequisites
in its opt-in mise entry without changing the default coding objective. Local
credentials use existing environment/untracked `mise.local.toml` or existing
Codex login isolation; never read/copy secrets into artifacts or browser state.

Unit regression selector and original runner regressions are listed exactly in
TASK-011. Credential-free doubles prove continuation/cleanup decisions only.
Deterministic real-browser/real-MCP tests force malformed inputs, denials, empty
and failed reads, cross-tenant and supersession cases without paying a model to
reproduce every failure. The live sequence must still prove actual agent
catalog discovery, generated views, same-conversation follow-up and connected
browser range interactions. Missing host/credentials are explicit BLOCKED
outcomes and fail live acceptance, never silently skip selected agents.

Record source, agent, host, browser versions; sanitized prompts/follow-ups and
actual resource/tool exchanges; screenshots/browser traces and range changes;
viewer-scoped request/results and server evidence; process deadline/cleanup
outcomes. No fixed wording/tool-count assertion or model self-report replaces
observable success. A repository test host must use the real connected agent
session and MCP lifecycle; fixture outputs copied into an independent browser
cannot count. Commercial-client claims require that client to be exercised.

Final change review owns combined evidence for both primary journeys. The
live lane remains opt-in and outside unattended CI/`gate`; absence of its proof
blocks AC-014 even when the aggregate passes. Other in-scope platform and human
inspection dependencies remain unchanged. Ask/REQ-031 are deferred.
