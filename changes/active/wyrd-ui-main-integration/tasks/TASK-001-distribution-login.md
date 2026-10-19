# TASK-001 — Installed and container UI with existing login

Spec: [revision 9, approved](../spec.md). Status: Proposed.  
Dependencies: None.  
Mapped obligations: REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-020, AC-001, AC-002, AC-003, AC-004, AC-005, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

An installed user reaches the existing production login and retained authenticated pages from `wyrd server dev`; official Linux images expose the same app through nginx. This task is independent of deletion and the component library.

## Owners, Scope, Consumers, and Prohibited Changes

Extend existing packaging, signed installation, DevServer, BFF auth forwarding, image/startup and release owners. Native: Darwin aarch64, Linux x86_64/aarch64. OCI: Debian amd64/arm64, Rocky amd64. System Node for native; container-provided Node. Preserve initialization, saved credentials, exact-origin auth, protected cookies and owned-process cleanup. INV-005 strictly excludes Cards, Query, Bifrost, domain clients, generated HTTP clients and analytical addons.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Verified distribution | `scripts/build.sh::bundle`, `image`; CLI `src/server/install.rs::ServerInstaller`, `InstalledServer` | CLI `tests/server_install.rs::bundle` currently makes empty UI; release.yml build-server/build-image consumers | Bundle and validation omit runnable UI | Extend one build owner and existing atomic/signature checks | No new build script or bundled Node |
| Launch and browser origin | CLI `src/server/dev.rs::DevServer.start/run/stop`; UI `BrowserSessions.begin/complete`, `BrowserSession.api`; server `RequestOrigin::from_parts` | CLI `tests/pg_tests.rs`; server `tests/integration/identity_ui_e2e.rs::production_ui_bff_journey`; UI production-auth driver currently bypasses BFF for /auth | One child only and no public auth forwarder | Extend DevServer and forward /auth through BFF to configured Rust upstream | No native nginx or second lifecycle controller |
| Container application | `docker/official/extras/entrypoint.sh`, nginx template; current Debian/Rocky Dockerfiles | `scripts/server/startup-common.sh` and mise startup dev/prod; historical same paths at 7bfc3a8a | Node/UI missing and startup defaults Ubuntu | Reconcile existing three-process routing and output matrix | Historical pattern only; main routes and failure semantics win |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Extend bundle and signed installer validation for production BFF contents and dependencies; retain prior installation on all invalid replacements.
2. Check compatible system Node and payload before owned-child launch, then extend native readiness, failure observation and cleanup.
3. Close public browser /auth forwarding without following redirects or trusting incoming forwarded chains.
4. Reconcile Dockerfiles, entrypoint, nginx, release matrix/tags and platform documentation with selected outputs, including the superseded four-native-target statement in AGENTS.md. The caller's three-native-target decision is already approved in INV-003; do not reopen Intel macOS support.
5. Extend existing native, identity and Linux startup proofs through the actual deployed routes.

## Ordered Implementation Scenarios

### Scenario 1 — Verified package replacement

**Behavior.** A valid package contains a runnable UI; corrupt, missing or incompatible UI never replaces a working install.

**RED.** Extend the existing installer assertions: the current empty-ui fixture must fail the new payload requirements. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test server_install -E 'test(=server_install_preserves_previous_on_failure)'
```

**GREEN.** Keep staged verification and atomic activation; validate actual Node runtime in its owning runtime proof. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Native readiness and lifecycle

**Behavior.** First/repeat startup reaches UI readiness; missing Node, bad payload, port conflict, partial startup, sibling death and interrupt leave no abandoned owned processes or damaged saved state.

**RED.** Extend current installed-CLI journey assertions to require the BFF and sibling cleanup; current single-child startup cannot meet them. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
```

**GREEN.** Extend DevServer; preserve migrate/provision/saved-config order and settle both children on every exit. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 3 — Real login at the public origin

**Behavior.** Tenant/platform login, API-key recovery, callback, refresh and logout work through native BFF and container nginx; hostile origin/state/issuer/tenant remain refused.

**RED.** Remove synthetic /auth bypass from the production identity driver and assert public-origin login; native BFF currently cannot forward it. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --test integration --run-ignored=all --no-capture -E 'test(=identity_ui_e2e::production_ui_bff_journey)'
```

**GREEN.** Add only trusted upstream forwarding/routing and preserve existing auth/session behavior. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 4 — Official image routing

**Behavior.** Pinned Debian amd64/arm64 and Rocky amd64 images serve BFF, Rust HTTP/auth/MCP/metadata/health and gRPC with readiness, streaming and three-process failure cleanup.

**RED.** Extend startup dev/prod probes to require UI and child-failure handling; main images have no Node/BFF. Run this exact focused command (new Vitest title where applicable):

```bash
mise run test:server:startup:dev
mise run test:server:startup:prod
```

**GREEN.** Extend current image and nginx owners with runtime and correct public-origin routing. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- AC-001–AC-005 pass through their distinct native, identity and Linux-image proof owners.
- Native source-free package smoke runs on each supported native runner; Linux OCI proof cannot substitute for Darwin proof.
- No Intel Darwin, Rocky arm64, selected Ubuntu output, test-only environment setting, or duplicated build implementation is added.

## Expected Write Set and Consumer Closure

scripts/build.sh; CLI install/dev and tests; BFF auth routing and identity driver; official Debian/Rocky image/process/nginx owners; release.yml and owning platform/release documentation. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Provision existing Keycloak/Dex/Postgres identity setup from mise `test:identity:journey:inner` before its focused identity command; do not clone that setup into another script or introduce filter variables. Build required CLI/server binaries using the current local-dev owner. Add focused BFF forwarder unit assertions to the existing auth test owner and record their exact command when named. For installed process-lifetime regressions run:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]'
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[exit]'
```

Run `mise run fmt`, `mise run lints`, `mise run check:deps`, UI check/build and affected startup owners. Static proof covers selected release/native matrix, signature coverage, image identities and owning documentation reconciliation. Existing identity assertions are regressions; RED applies only to newly missing routing/readiness/cleanup assertions. Native runtime evidence is required separately on all three native targets, including source-free BFF launch; record tested runtime/version and package digest. Linux image evidence names each selected variant and immutable image/source ID. No cross-platform pass may be claimed from one host.

### Human visual checkpoint

Installed native UI login, signed-in tenant page and settings; repeat via official nginx public origin. Inspect populated signed-in, refusal and upstream-error views. No domain data is needed. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Execution](../../../../architecture/references/languages/implementation-execution.md), [TDD](../../../../architecture/references/languages/spec-driven-development.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
