# Verification owners for the UI task packet

This is a planning reference, not another test/build framework. Source is committed
main `8651b96fd`; scripts/commands below were inspected, not executed in planning.

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
  New page tests cover T6. Primitives without reads have no fake loader/T5 test.
  Missing prerequisites defer actual completion; no manufactured RED is required
  for already-correct behavior or static configuration/deletion obligations.

## Existing specifically named regressions

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
mise exec -- cargo nextest run --locked -p wyrd-cli --test server_install -E 'test(=server_install_preserves_previous_on_failure)'
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
The full library waits for every component/action/live prerequisite; fixture readiness
is never silently promoted to production completeness. MCP production sandbox/security
proof in TASK-011 supplements (and is not replaced by) AppBridge fixture tests.
