---
id: TASK-004
spec: SPEC-local-server-bootstrap@4
depends_on: [TASK-001, TASK-002, TASK-003]
maps: [REQ-001, REQ-004, REQ-005, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, REQ-014, REQ-015, INV-001, INV-003, INV-004, AC-001, AC-004, AC-005, AC-007, AC-008]
---

## Outcome and Value

`wyrd server dev --tenant <slug>` takes an installed CLI and external
Postgres to a ready local server, saved tenant credential and client
endpoint, and selected usable MCP hosts. A second run is safe and does not
silently upgrade or reissue credentials.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| Server operator lifecycle | `wyrd-server::main::{migrate,setup,run}` | `platform_admin_e2e`, `test:server:startup`, server docs | CLI does not compose migration, ready server, setup, and shutdown | Orchestrate these existing server-owned operations in `wyrd-cli` | Foreground process lifecycle is genuinely new CLI state |
| Client settings and secrets | `GlobalConfig::load_from`, `CredentialsFile`, `SavedLogins`, `ClientConfig::from_global_with_overrides` | CLI `client::from_global`, Python/TypeScript installed CLI and SDK journeys | No first-use write of local endpoint and tenant key | Extend existing protected config/credential owners | No parallel credential file |
| Local host choice | `wyrd mcp install` from TASK-003 | MCP connectivity and host contract journeys | Local flow does not offer the selected hosts | Compose the approved installer after readiness | No second host editor |
| Local resource defaults | `mise.toml dev:backend` macOS memory handling; server config | Local-development docs and server startup lane | Installed CLI lacks the same local memory input | Carry the existing safe local setting into the CLI journey | No Bifrost config layer |

The CLI must not perform DDL, mint credentials itself, bypass server
authorization, manage Postgres/storage services, or store secrets in argv,
logs, or host files. Do not change the normal server boot path into a
migrating path.

## Approach

1. Prove missing first-use and retry behavior through the installed CLI and
   a real server.
2. Compose verified installed server selection, one-off migration, foreground
   startup, readiness, and server-owned setup.
3. Save tenant credentials and loopback endpoint through the existing
   protected Wyrd files, preserving unrelated content and reporting
   partial progress/recovery.
4. Offer TASK-003 host selection after readiness and verify a selected MCP
   read; complete Python, Rust, TypeScript, and CLI fresh-process journeys.
5. Reconcile local setup and troubleshooting docs with delivered behavior.

## Ordered Implementation Scenarios

### Scenario 1 — Fresh local setup

**Behavior.** With one database URL, the command installs a
server only when none exists, migrates, starts, waits for readiness, provisions
through the server, saves config and credential, and leaves a foreground
server usable by a fresh client. (REQ-004, REQ-005, REQ-007–REQ-009, AC-001)

**RED.** Add `server_dev_first_run_is_client_ready` in the existing
`wyrd-cli` integration target; it fails because `server dev` is absent.
Run `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_dev_first_run_is_client_ready)'"`.

**GREEN.** Compose the approved owners and rerun the exact journey. Prove
fresh CLI and Rust SDK use through the same server, then prove the installed
Python and TypeScript surfaces in their runtime-owned tests. Add
`local_development::cli_bootstrap_saved_config_connects` in the existing Rust
SDK integration target and run
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-sdk-rust --test integration -P journey --run-ignored=all -E 'test(=local_development::cli_bootstrap_saved_config_connects)'"`.

**REFACTOR.** Keep process lifecycle in the CLI and durable operations on
the server; remove only duplicated orchestration while journeys stay green.

### Scenario 2 — Retry, interruption, and partial progress

**Behavior.** Repeating setup neither upgrades the binary nor rotates an
active tenant key. Missing inputs, insufficient migration privileges, failed
migration/readiness, or failed local persistence report stable, redacted
recovery guidance. Interrupting the CLI shuts down its server. (REQ-010,
REQ-015, INV-004, AC-004, AC-007)

**RED.** Add `server_dev_repeat_and_partial_failure_preserve_state` in the
confirmed CLI target; it fails at the first absent refusal or recovery path.
Run `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_dev_repeat_and_partial_failure_preserve_state)'"`.

**GREEN.** Complete retry, failure, and process settlement behavior and
rerun both exact scenario tests.

**REFACTOR.** Keep the state transitions explicit and avoid a second
credential or config persistence mechanism.

### Scenario 3 — Selected MCP host can ask Wyrd

**Behavior.** After the server is ready, the local flow offers detected
hosts, installs only selected connections, and a selected host discovers and
calls a read tool through shared credentials. Declining host setup leaves
the server and SDK usable. (REQ-011, REQ-014, AC-005)

**RED.** Add `server_dev_selected_mcp_host_reads` in the confirmed CLI
target; it fails because the local command does not yet compose MCP setup.
Run `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_dev_selected_mcp_host_reads)'"`.

**GREEN.** Compose TASK-003 installation and proxy behavior, then rerun all
three exact scenario journeys.

**REFACTOR.** Keep host detection and config editing solely in TASK-003's
owner; local startup only selects when to offer it.

## Acceptance Criteria

A fresh installed Python or TypeScript `wyrd` command completes the local
journey without a checkout. Fresh CLI and Rust/Python/TypeScript SDK processes
can authenticate and complete representative write/read behavior. A selected
MCP host calls a read tool. Retry, cancellation, failure, and partial-progress
recovery meet the spec without secret exposure or silent upgrades.

## Expected Write Set and Consumer Closure

Likely: `crates/wyrd/wyrd-cli`, the existing protected client config and
credential owners in `crates/shared/wyrd-client`, existing server operator
surfaces only when their composition requires it, CLI/server journeys,
Python/TypeScript package journeys, Rust SDK journey, and local-development
docs. Paths are guidance, not an implementation allowlist.

## Verification and Evidence

Run the three exact scenario commands and the exact Rust SDK command above.
Add a focused `cli_bootstrap_saved_config` case to the existing Python
`test_local_development.py` target, run `mise run py:setup`, then run
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- bash -lc "cd sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration tests/integration/test_local_development.py -k cli_bootstrap_saved_config"'`.
Add an equivalently named case to the existing TypeScript
`local-development.test.ts` target, run `mise run ts:build` and
`mise run ts:build:testing`, then run
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && mise exec -- bash -lc "cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/local-development.test.ts -t cli_bootstrap_saved_config"'`.
Run `mise run test:server:startup` for the lifecycle seam,
`mise run check:deps` for client/server boundaries, `mise run docs:check`,
`mise run fmt`, `mise run lints`, and `git diff --check`. If Python files
change, run `mise run py:format`, `mise run py:lints`, and
`mise run py:typecheck` when public typing changes. If TypeScript files
change, run `mise run ts:typecheck`.
The fresh-client and MCP journeys are behavioral RED/GREEN proof; docs and
package metadata receive static checks rather than artificial RED.

## Docsite Rebuild Update

After implementation and verification pass, update the developer docsite
rebuild so developers can use what this task delivered. The rebuild lives in
the `wyrd-doc-site` worktree under the `developer-docsite-rebuild` change
packet. Its spec sets the page map, and its tasks set the page rules. Use
`$human-tech-docs`, keep pages `draft: true` with an accurate `status`, and
describe only behavior this task delivered and verified. Keep the development
setup minimal: put production detail in Operate, not Get started. Run every
documented command against the delivered build, then run the docsite's
`docs:check:commands`, `docs:linkcheck`, `docs:build`, and `docs:a11y`.
Record the pages changed and the checks run in Implementation Evidence.

Pages:

- Get started → *Set up Wyrd*: rewrite the development setup around
  `wyrd server dev --tenant <slug>`. It replaces the manual build, migrate,
  serve, and setup steps. The developer still supplies PostgreSQL and
  `WYRD_DATABASE_URL`.
- Get started → *Quickstart*: make its prerequisites point at the new setup
  and the saved credential and endpoint.
- Operate → *Troubleshoot a deployment*: local startup failures and safe
  reruns that do not reissue credentials.

## Material Stop Conditions

Stop for spec revision if automatic first-use setup cannot persist a usable
credential without an unapproved server credential-disclosure contract, or if
the local command must add a new durable owner beyond those approved.

## Authority Links

`changes/active/local-server-bootstrap/spec.md` revision 2;
`AGENTS.md` §§2, 8, 9, 11; `architecture/wyrd-design.md`;
`architecture/references/languages/testing-workflows.md`.

## Implementation Evidence — IMPLEMENTED

Authority checked: approved specification revision 7; this task's mapped
bootstrap requirements are unchanged from revision 4. Existing owners reused:
`ServerInstaller` selects the retained release; `wyrd-server migrate/setup`
perform durable operations; `CredentialsFile` protects both existing local
files; `GlobalConfig` composes their updates; the existing MCP installer and
proxy remain the only host editor and credential/transport path. The new CLI
owner retains only its installed binary, local inputs, and foreground child.

Initial RED: `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli
-E 'test(=server_dev_first_run_is_client_ready)'` selected one test and failed
with Clap `InvalidSubcommand("dev")`. This initial check covers command
selection; the real-process journey is under `pg_tests` so the fast lane can
skip PostgreSQL. Its corrected exact selector is
`test(=pg_tests::server_dev_first_run_is_client_ready)`.

Compile diagnoses:

- Symptom: new local error omitted from the exhaustive embedded-CLI error
  adapter. Evidence: `WYRD_LOG=info mise exec -- cargo check --locked -p
  wyrd-cli` reported E0004 at `error.rs`'s `From<WyrdCliError>` match. Cause:
  the new variant lacked its shared error projection. Fix site: that existing
  adapter, used by all embedded CLI projections; added the unavailable-client
  projection without changing other cases.
- Symptom: E0599 for `ListCardsRequest::default`. Evidence: tracing-enabled
  `cargo check` and the focused journey build. Cause: the existing request
  deliberately has no `Default` implementation. Fix site: local startup's
  bounded read, using the existing typed request fields with `limit: Some(1)`.
- Symptom: the CLI journey target failed to compile at
  `pg_tests::server_dev_first_run_is_client_ready`. Evidence: the tracing-enabled
  exact nextest command reported E0599 for `card.metadata()`.
  Independent read-only diagnosis: `Cards::get` returns the existing `Card`
  whose `metadata` is a public field (`envelope.rs:137`); only this new
  assertion used method syntax. This is a test syntax defect, not a production
  contract defect. Fix site: that field access; all assertions remain intact.
  Every CLI integration selector is affected because the target compiles as
  one binary. The diagnostician made no edits.

Implementation verification passes. Independent task review remains required;
no task-completion or review approval is claimed.

- SDK fixture diagnosis: initial and `WYRD_LOG=info MISE_VERBOSE=1` rerun
  reached local readiness, then nested mise refused the repository config as
  untrusted. Independent read-only diagnosis found the fixture's temporary
  `HOME` hid mise's trusted-config state. Fix site: remove that HOME override;
  `WYRD_CONFIG_HOME` and the temporary working directory already isolate Wyrd.
  Affected callers: all three fresh SDK-runtime fixture invocations. No SDK
  assertion or production behavior changes for this failure.

- Lint diagnosis: `mise run lints` and its `WYRD_LOG=info` rerun report
  missing panic documentation for the package-version invariant,
  `collapsible_if` in the saved-key identity check, and `single_match_else`
  in shutdown. Independent read-only diagnosis confirms local documentation
  and syntax fixes in `DevArgs::run` and `DevServer::{run,stop}`; dispatch is
  their caller. Preserve key validation, reaping errors, timeout, kill, and
  drain-deadline refusal unchanged. No allow attributes or gate changes.

- Fixture lint diagnosis: the focused Ruff check and `WYRD_LOG=info` rerun
  report I001 (the pathlib import interrupts the plain import group) and B904
  (cleanup raises a RuntimeError inside TimeoutExpired without chaining).
  Independent read-only diagnosis identifies only import ordering and explicit
  `from err` as fixes. All three SDK fixture callers retain their assertions,
  termination actions, and deadlines.

- Python type diagnosis: `mise run py:typecheck` and its tracing rerun
  report too-many-positional-arguments for pytest.skip. Independent read-only
  diagnosis traces ty 0.0.39's interpretation of pytest 8.4.2's exception
  decorator; the runtime signature accepts the reason. The existing smoke
  caller documents this limitation. Fix site: raise the same public
  `pytest.skip.Exception` in the existing missing-fixture branch, preserving
  its condition and message. No type-ignore, dependency, or assertion change.

- Container startup diagnosis: the native `mise run test:server:startup`
  and `WYRD_LOG=info` rerun build an image but fail `exec format error` before
  server code executes. Independent read-only diagnosis confirms the shared
  startup script builds/copies a host-native executable: Mach-O arm64 cannot
  run in Ubuntu. Buildx's missing plugin warning is unrelated; fallback image
  builds succeed. Execute the unchanged development and production scripts
  in a Rust 1.97.1 bookworm arm64 Linux runner, with host networking and an
  isolated debug output overlay so the Mac client artifacts remain usable.
  Linux evidence covers this lifecycle seam; the native macOS image-build
  assumption remains outside the local CLI implementation.

- Installed Node shutdown diagnosis: the packed-package SDK write/read passes,
  but SIGINT leaves its CLI spinning past the unchanged 50-second fixture
  deadline. The WYRD_LOG rerun reproduces it. Independent read-only live stack
  sampling shows `signal_hook_registry::handler -> Prev::execute ->
  node::SignalExit -> node::ResetStdio`; its server child has already exited.
  Node's default signal exit re-raises the signal Tokio chains. Fix site:
  the installed Node entrypoint registers Node SIGINT/SIGTERM listeners before
  calling the existing Rust dispatcher, so Tokio retains graceful shutdown.
  Affected callers: the npm CLI's Rust commands using interruption; native Rust
  and Python entrypoints are unaffected. No deadline or assertion was changed.

- The first Node fix rerun still packaged the old executable: independent
  read-only diagnosis confirms the package bin is `dist/cli.js`, while plain
  `tsc` uses `noEmit: true`. The compiled file lacked the new listeners.
  Rebuild with the existing `tsconfig.build.json` emission; no second code fix
  is supported by that stale-artifact result. Subsequent package checks use
  the normal build dependency before packing.

Installed-package checks now pass outside the checkout working directory:

- Production Python wheel installed in `/tmp/wyrd-task004-production-python`:
  `WYRD_TEST_INSTALLED_CLI=/tmp/wyrd-task004-production-python/bin/wyrd
  scripts/postgres/with-test-postgres.sh -- mise exec -- python3
  scripts/server/with-local-dev.py mise exec -- bash -lc 'cd
  sdks/wyrd-sdk-python && uv run python -m pytest -q -m integration
  tests/integration/test_local_development.py -k cli_bootstrap_saved_config'`
  — one selected test passed; startup uses the production wheel's installed
  command and a copied installed server, not a build fallback.
- Packed npm SDK plus platform addon: `scripts/postgres/with-test-postgres.sh
  -- mise exec -- node scripts/check_ts_package.mjs python3
  scripts/server/with-local-dev.py mise exec -- bash -lc 'cd
  sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run
  tests/integration/local-development.test.ts -t cli_bootstrap_saved_config'`
  — one selected test passed; installed CLI SIGINT shutdown also settled.
  The package checker remains the packaging owner; its optional child command
  supplies the installed executable to this journey.

Docsite updates in the separate `wyrd-doc-site` worktree:
`docs/src/content/docs/get-started/setup.md` and
`docs/src/content/docs/operate/troubleshoot-a-deployment.md` remain
`draft: true`, `status: partial`. Quickstart does not exist yet; its owning
`TASK-002-first-verified-result.md` now requires the delivered setup link and
saved-client prerequisite. No empty or published page was invented.
`docs:check:commands`, `docs:linkcheck`, `docs:build`, and `docs:a11y` pass;
that accessibility lane renders zero published pages because these are drafts.
The current Wyrd `docs:check` passes with 64 rendered pages. Reader review and
humanizer checks cover the changed setup/recovery prose. Existing docsite
changes belonging to other tasks were preserved.

- Connected-caller signal audit: independent read-only diagnosis confirms
  unconditional Node listeners swallow interruption in MCP proxy, ordinary
  HTTP commands, and installation, whose Rust paths have no signal listener.
  The actual Clap root has only a subcommand, and ServerCommand has literal
  Install/Dev without aliases or global arguments. Limit the Node interop
  listeners to the `server dev` prefix; all dispatch remains Rust-owned.
  Other installed commands retain Node's normal signal behavior.

- First-install cancellation audit: independent read-only diagnosis confirms
  local setup previously awaited download before polling its interruption
  future. Node delegates signals for this command, and the installer has a
  connection timeout but no body timeout, so an interrupted stalled download
  could wait indefinitely. Race installation against one pinned interruption
  future and reuse it during serving. Cancellation drops existing temporary
  staging before a child exists; once serving starts, existing stop/reap still
  runs. Explicit installation and installer APIs remain unchanged. All three
  CLI projections consume this shared local-command fix.

- Interactive host interruption diagnosis: independent read-only source
  diagnosis confirms Tokio's Unix signal hook uses `SA_RESTART`, and std
  `read_line` retries Interrupted. The existing synchronous host prompt holds
  the local command inside a future poll, preventing its outer interruption
  branch from running. Tokio stdin and spawn_blocking also leave an
  uncancellable reader that can hang runtime shutdown. Fix belongs in the
  existing MCP prompt owner, preserving shared menu/answer parsing and host
  editing while local startup awaits cancellable terminal input. Standalone
  MCP installation keeps its existing synchronous prompt.

The cancellable prompt uses the existing workspace Rustix package with its
terminal feature and Tokio AsyncFd, confined to the Unix CLI boundary. The
independent macOS PTY experiment found `/dev/tty` fails kqueue registration;
opening stdin's actual tty device succeeds. No inherited descriptor flags are
changed. The existing host menu and exact selection parser are shared by both
read paths, and cancellation drops the local terminal descriptor immediately.

Existing server startup scripts pass unchanged under pinned Rust 1.97.1,
protoc 33.2, and nextest 0.9.143 in the isolated Linux arm64 runner:
`bash scripts/server/test-startup-dev.sh` and
`bash scripts/server/test-startup-prod.sh`. Image:
`sha256:3464c63015302424a1e73bb3294ecdb18bd72cb10fafeca5c33889496611974f`,
source label `cb5656b2b913dbd88a8f6736bf4a0baaf998960c+dirty`.
Production includes exact migration refusal/retry and startup-image
write/verification tests. This is the task's existing container lifecycle seam;
the host-native macOS image-build assumption is unchanged.

Additional exact checks pass:

- `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli
  -E 'test(=server_dev_selects_installed_local_startup)'`.
- `mise exec -- cargo nextest run --locked -p wyrd-cli --lib
  -E 'test(=mcp::hosts::tests::prompt_selection_maps_answers_onto_detected_hosts)'`.
- `mise exec -- cargo nextest run --locked -p wyrd-client --lib
  -E 'test(=global_config::tests::local_setup_preserves_content_and_existing_key)'`.
- Real controlling-terminal fixture:
  `WYRD_LOCAL_DEV_TERMINAL=interrupt scripts/postgres/with-test-postgres.sh
  -- mise exec -- python3 scripts/server/with-local-dev.py` — interrupting
  the unanswered host prompt returns 130 and releases the HTTP listener.
- The same fixture in `decline` mode runs the focused Python SDK case:
  host setup is declined, a fresh client still writes/reads, root disclosure
  appears only on the separate controlling terminal, and no tenant key appears
  there. The final focused lane includes interrupt and installed Node decline.

The fixture gives its CLI an isolated process session; deadline failure kills
only that owned group before reaping. The unchanged 50-second settlement
budget remains an assertion, and successful interruption must return 130.
Get-started docsite setup now links Operate for detailed administrative
recovery, keeping the initial development setup small.

Final error-path inspection: propagating a terminal-write failure immediately
would discard the just-issued tenant key before protected persistence. Treat
an undeliverable root credential like the existing no-terminal case: print
secret-free recover-root guidance and continue saving the tenant key. This
changes no server issuance or disclosure contract and affects the shared local
command in all three projections. A failed terminal write itself is not forced
end-to-end; successful terminal disclosure and no-terminal recovery are covered.

Retry persistence diagnosis: the independent connected-writer audit found
that retry passed no expected key into the locked save, and saving an equal
key cleared its token cache. Pass the validated saved key through the existing
save transaction, reject conflicting or unexpectedly appeared defaults under
its existing lock, and write credentials only when installing a new default.
Callers are local startup and the existing GlobalConfig test. Existing login
and token-cache writers preserve the default key; no additional storage owner
or lock is needed. The deterministic test retains comments and cache bytes
and refuses a default appearing after an empty-key preflight.

Python shutdown diagnosis: the final focused lane's Python SDK write/read
passed, but the stronger process settlement assertion failed. A tracing-enabled
reproduction captured the installed command's return code -2. Independent
read-only diagnosis found Tokio chains CPython's SIGINT handler; Rust returns
130 after cleanup, then CPython processes its pending KeyboardInterrupt. Fix
the existing PyO3 console-script projection: check pending Python signals when
the dispatcher returned 130, consume only KeyboardInterrupt, and propagate
other exceptions. No Python handler mutation or fixture relaxation is needed.
The console entrypoint is shared by installed Python commands; ordinary
non-interrupted results are unchanged. The real installed-runtime fixture
retains its exact 130 assertion.


### Acceptance closure

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Installed CLI bootstraps an external database and retained server | `wyrd-cli/src/server/dev.rs` composes existing installed-server lifecycle | Three exact `pg_tests::server_dev_*` cases in `test:local-dev:journey`; production Python wheel and packed Node command start outside checkout | PASS |
| Fresh clients authenticate and write/read from protected saved files | Existing `GlobalConfig`/`CredentialsFile` owners; fresh SDK fixture | Exact `cli_bootstrap_saved_config` Rust, Python, and TypeScript cases in the focused lane | PASS |
| Selected MCP host calls a read tool | Existing installer, host editor, and proxy | `pg_tests::server_dev_selected_mcp_host_reads` | PASS |
| Retry, cancellation, failure, partial progress preserve state and secrets | Existing directory lock; credential-first save; owned server stop/reap; cancellable terminal prompt | `pg_tests::server_dev_repeat_and_partial_failure_preserve_state`, cached-credential unit case, real terminal interrupt/decline, installed Python/Node exit-130 assertions | PASS |
| Current and draft documentation describe the delivered journey | CLI reference, local-development guide, draft Setup/Operate, future Quickstart task prerequisite | Current `docs:check` (64 pages), separate docsite command/link/build/accessibility lanes and reader review | PASS |

Final focused lane: `WYRD_LOG=info mise run test:local-dev:journey` passes
all three CLI cases, the exact Rust SDK case, terminal interrupt, installed
Python SDK case, and packed Node terminal-decline SDK case. Log:
`/tmp/wyrd-task004-complete-journey-green.log`.
The production Python wheel was rebuilt with `mise exec -- uv run maturin
build --locked --out <repository>/target/task004-python-wheel`, installed
with `uv pip install --reinstall` in the isolated environment, and reran the
exact installed-command fixture above successfully. It imports outside the
checkout and exposes no `wyrd.testing` surface.

Final checks pass: `mise run fmt`, `mise run lints`, `mise run check:deps`,
`mise run codegen:check`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run py:test:unit` (593 passed),
`mise run ts:format:check`, `mise run ts:lints`, `mise run ts:typecheck`,
the fixture's explicit Ruff check/format check, and `git diff --check` in
both worktrees. PyO3 audit: existing native entrypoint, root/submodule
registration, package exports, both `__all__` lists, and generated signatures
remain wired; generation and runtime tests pass.

Non-goals remain excluded: no managed Postgres/storage lifecycle, CLI-owned
DDL or key issuance, normal-server auto-migration, implicit server upgrade,
new durable owner, compatibility alias, or docsite publication. Final tracked
and untracked audit confines the primary diff to this task's implementation,
fixtures, package projections, verification lane, documentation, and evidence.
The three owned docsite changes preserve that worktree's unrelated edits.

Material limits: live official-release download is not exercised; installed
package fixtures use a retained compatible server. Stalled-download
cancellation is covered.
Existing container startup scripts pass on the isolated Linux arm64 runner;
the native macOS lane cannot execute its Mac binary inside Ubuntu. Draft
pages are not rendered by the separate docsite accessibility lane. A failed
controlling-terminal write is checked by source inspection, not forced in the
journey. The user subsequently requested commits and a reviewer handoff.
Review range: `cb5656b2b913dbd88a8f6736bf4a0baaf998960c` through the
implementation commit containing this evidence. The three owned docsite files
are committed separately as `aecdaa05229d3571dedd362e8344fc28da5808a5`,
with base `2044e161e1a2020f5bb87cc6a36ee893ab311340`.
No review approval, merge, push, or deployment is claimed.
Handoff: independent `$wyrd-task-review`.

Documented recovery checks also pass against managed Postgres:

- `WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec --
  bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run
  --locked -p wyrd-cli --test cli
  -E 'test(=operator_journey::operator_administers_a_deployment_through_the_cli)'"`.
- `WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec --
  bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run
  --locked -p wyrd-server --test integration
  -E 'test(=platform_admin_e2e::an_operator_recovers_from_losing_every_platform_credential)'"`.

The corrected executable SDK scenario commands are composed by
`test:local-dev:journey`: the bootstrap fixture supplies the saved-file marker
and runs the exact selected test in a fresh runtime. Running the original
standalone SDK commands without that fixture would skip this scenario.


### User-requested structure correction

The standalone `scripts/server/with-local-dev.py` test runner is removed.
Its process/terminal checks now live in the existing Python local-development
integration module, selected directly by pytest from the existing focused
mise lane. The wrapper still owns PostgreSQL, and fresh Rust/Python/TypeScript
SDK processes still consume only the CLI's saved configuration. No new script,
package, dependency, or runtime owner is added.

The oversized `DevServer::run` body is split into startup, memory detection,
migration, provisioning, and foreground waiting. The existing concrete server
handle retains its process dependencies. `ProvisionedTenant` owns only parsed
identity/credential data and pure tenant-key validation; persistence continues
through existing GlobalConfig/CredentialsFile owners. Saved-client verification
and terminal disclosure are invocation-scoped helpers. DevArgs preflight and
compatible-server construction are separate from signal/lifetime coordination.
Error text, interruption polling, retry behavior, and budgets are preserved.

Verification for this correction passes; original committed evidence above
describes the initial candidate. The direct pytest entrypoint replaces all
historical invocations of the removed standalone script.

| Correction | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Remove standalone fixture script | Existing local-development integration module owns setup, terminal interaction, SDK subprocesses, and cleanup | Direct pytest Rust/Python/TypeScript/interrupt cases in the focused journey | PASS |
| Split oversized startup coordinator | Server lifecycle methods, pure ProvisionedTenant parsing/validation, existing persistence owners, fresh-client verification helper | Unchanged three CLI journey cases plus all SDK, terminal, and package cases | PASS |

`WYRD_LOG=info mise run test:local-dev:journey` passes all seven selected
outer cases: three CLI scenarios and four direct pytest scenarios. The nested
Rust, Python, and TypeScript SDK cases also pass. The installed production
Python wheel is rebuilt and its direct pytest Python scenario passes with
`WYRD_TEST_INSTALLED_CLI=/tmp/wyrd-task004-production-python/bin/wyrd`.
`mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`,
`mise run py:typecheck`, `mise run py:test:unit` (593 passed), and final
`git diff --check` pass. No new contract, dependency, or generated surface
is introduced; prior unchanged contract and documentation evidence remains.

Exact scenario commands run inside the mise toolchain/environment:


```bash
scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state) | test(=pg_tests::server_dev_selected_mcp_host_reads)'
scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py::test_installed_cli_bootstrap[rust]
scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]
scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]
scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration tests/integration/test_local_development.py::test_installed_cli_bootstrap[typescript]
```

The final tracked/untracked audit contains only the requested production
refactor, test-runner removal, existing test module and lane updates, the Rust
test documentation reference, and this evidence. No unrelated edits are
changed. Commit this correction separately from the initial implementation;
review the full range starting at `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`.
