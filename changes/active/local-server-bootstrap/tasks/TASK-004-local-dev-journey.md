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
