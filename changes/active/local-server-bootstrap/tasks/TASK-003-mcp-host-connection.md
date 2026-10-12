---
id: TASK-003
spec: SPEC-local-server-bootstrap@5
depends_on: []
maps: [REQ-001, REQ-011, REQ-012, REQ-013, REQ-014, REQ-015, INV-001, INV-003, INV-004, AC-005, AC-006, AC-008]
---

## Outcome and Value

A developer selects detected MCP hosts once and can use Wyrd tools against
either a local or explicitly selected deployed server, with credentials
resolved by the existing client each time the host connects.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
| --- | --- | --- | --- | --- | --- |
| MCP auth and transport | `wyrd_mcp::client::WyrdMcpHttpClient`, `wyrd_client::WyrdClient` | MCP connectivity journeys; CLI `client::from_global`; saved-login and API-key resolution | No host-launchable stdio connection to authenticated `/mcp` | Extend the CLI using the existing shared client/MCP transport | A local host proxy is required because host config cannot reproduce Wyrd's renewable credential chain |
| CLI host setup | `wyrd_cli::Cli/Command`, `WyrdCliError` | Installed Python/TypeScript CLI entries and tests | No host detection, selection, or scoped config edit | Add `wyrd mcp install` and `wyrd mcp proxy` under the existing CLI | No separate package CLI |
| Endpoint/credential selection | `ClientConfig::from_global_with_overrides`, `SavedLogins` | `wyrd auth login --server --tenant`, MCP transport tests | External host must retain its URL while the global endpoint stays unchanged | Use explicit endpoint override and existing credential chain | No profile or MCP secret store |

The supported variants are Codex CLI/IDE, Claude Code, Copilot CLI,
Copilot in VS Code, Cursor, Pi, and Hermes Agent. Treat Copilot CLI and VS
Code as separate configurations.
Preserve unrelated host settings. Do not create a second MCP tool catalog,
credential source, token cache, or auth header implementation.

## Approach

1. Prove the missing authenticated host-to-Wyrd path through a real server.
2. Add the host-launchable CLI proxy using the existing client transport and
   the server's `/mcp` surface.
3. Detect supported hosts, present a multi-select in interactive use, and
   accept explicit host selection for scripts and agents.
4. Write only selected host connections, retain an explicit external URL, and
   validate repeat/conflict/failure behavior.

## Ordered Implementation Scenarios

### Scenario 1 — Authenticated proxy

**Behavior.** A host process launched through `wyrd mcp proxy` discovers
tools and calls a read tool using the normal saved login or API-key resolution;
expired credentials refresh normally, insufficient scope is refused by the
server, and secrets are absent from host config. (REQ-013, REQ-014, INV-001)

**RED.** Add `mcp_proxy_discovers_and_reads_with_shared_auth` in the existing
`wyrd-cli` integration target; it fails because no proxy command exists. Run
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_proxy_discovers_and_reads_with_shared_auth)'"`.

**GREEN.** Extend the CLI using the existing Wyrd client and MCP transport;
rerun the exact journey.

**REFACTOR.** Keep protocol forwarding separate from Wyrd credential
decisions, which remain in the shared client.

### Scenario 2 — Selective host installation

**Behavior.** Detection offers the seven host variants; only user-selected
hosts change. Non-interactive use requires named selections. Repetition
does not duplicate Wyrd entries, unrelated entries survive, and conflicts
or unwritable configs report per-host failure. (REQ-011, REQ-012, INV-004)

**RED.** Add `mcp_install_changes_only_selected_hosts` in the confirmed
`wyrd-cli` target; it fails because no installation command exists. Run
`mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_install_changes_only_selected_hosts)'`.

**GREEN.** Extend CLI host setup and rerun the scenario tests, including
host-format contract cases for every supported variant.

**REFACTOR.** Share only behavior the host formats actually have in common;
keep distinct host-native configuration semantics visible.

### Scenario 3 — Deployed server stays explicit

**Behavior.** `wyrd mcp install --server <url>` retains that URL in the
selected host connection; the proxy authenticates to it through existing
credential resolution and never starts a local server or rewrites global
`config.toml`. (REQ-013, AC-006)

**RED.** Add `mcp_external_server_preserves_global_endpoint` to the
existing CLI target and run
`scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_external_server_preserves_global_endpoint)'"`.
It fails when the external path or explicit endpoint retention is absent.

**GREEN.** Complete endpoint selection and rerun all three exact tests.

**REFACTOR.** Reuse client endpoint precedence rather than adding another
host-specific resolver.

## Acceptance Criteria

Selected real hosts can discover and call a read tool. Host config contains
no credential. Existing Wyrd and unrelated host entries are handled without
data loss. External server use needs no local server and changes no global
default. Stable, redacted failures cover auth, permission, network, and
host-config problems.

## Expected Write Set and Consumer Closure

Likely: `crates/wyrd/wyrd-cli`, the existing `wyrd-mcp` client boundary
only where needed, CLI and MCP journey tests, installed package CLI smoke
tests, and agent-facing docs. Host config locations/formats must be validated
against each host's current official contract before implementation; paths
here do not prescribe private module or fixture structure.

## Verification and Evidence

Run the three exact scenario commands, `mise run test:wyrd`,
`mise run py:test:cli:unit`, `mise run ts:test:unit`,
`mise run docs:check`, `mise run fmt`, `mise run lints`, and
`git diff --check`. MCP runtime discovery/call proof is required; static
host-file assertions alone are insufficient.

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

- Connect → *Connect an agent through MCP*: `wyrd mcp install`, choosing
  detected hosts, connecting to a local server or an explicitly selected
  deployed server, and how credentials resolve when the host connects.
- Name the MCP failure modes a developer hits (no detected host, unreachable
  server, missing credential) on that page, not in Operate.

## Material Stop Conditions

Stop for spec revision if a supported host cannot launch an authenticated
connection without embedding a secret, or if a host requires a public
credential or permission contract different from the approved shared path.

## Authority Links

`changes/active/local-server-bootstrap/spec.md` revision 5;
`AGENTS.md` §§2, 9, 11; `architecture/references/languages/agent-harness.md`;
`architecture/wyrd-security-posture.md`.

## Implementation Evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
| --- | --- | --- | --- |
| Selected hosts discover and call a read tool over shared auth (REQ-013, REQ-014, INV-001) | `crates/wyrd/wyrd-cli/src/mcp/proxy.rs` bridges stdio to `/mcp` through `client::from_global` | `mcp_proxy_discovers_and_reads_with_shared_auth` (exact `--test cli` command) | PASS |
| Seven host variants, selection-only writes, repeat/conflict/unwritable per host (REQ-011, REQ-012, INV-004) | `crates/wyrd/wyrd-cli/src/mcp/{mod,hosts}.rs` (`HostInstaller`, JSON/TOML/YAML edits) | `mcp_install_changes_only_selected_hosts`; `mcp::hosts` unit tests (4); Python `test_mcp_host_connection.py` (8); TS `mcp-host-connection.test.ts` (3) | PASS |
| External server retained; no local server; global `config.toml` untouched (REQ-013, AC-006) | `--server` validated with `HttpConfig::validate`, stored in host args only | `mcp_external_server_preserves_global_endpoint`; Python `test_external_server_preserves_global_endpoint` | PASS |
| Host config contains no credential | Entry holds only binary path, `mcp proxy [--server]`, `env.WYRD_CONFIG_HOME` | Journey, Python, and TS assertions that the API key is absent from every host file | PASS |
| Stable, redacted failures (auth, permission, network, host config) | `WyrdCliError::{McpHostSelection, McpHostInstall, McpProxy}`; in-process codes `WYRD_SPEC_400_VALIDATION`, `WYRD_CLIENT_400_CONFIG_INVALID` | Journey failure cases; parametrized Python/TS invalid cases; `docs:check` (MCP host codes table) | PASS |
| Language tests use typed in-process commands, not argv | `wyrd_cli::commands::mcp_install` projected via `wyrd.testing.cli.mcp_install` and `@wyrd/testing` `cli.mcpInstall`; TASK-002 argv `server install` tests removed (covered by Rust `server_install.rs`) | `py:test:cli:unit` (6), `ts:test:unit`, `py:typecheck`, `ts:typecheck`, `codegen:check` | PASS |
| Docs and docsite | `reference/cli.svx` `wyrd mcp`; generated `api/errors.md`, `llms*.txt`; docsite `connect/connect-an-agent-through-mcp.md` (`wyrd-doc-site` commit `70a559944`) | `docs:check`; docsite `docs:check:commands`, `docs:linkcheck`, `docs:build`, `docs:a11y`; reader test | PASS |
| Format, lints, owner lane | — | `fmt`, `lints`, `py:lints`, `ts:lints`, `ts:format`, `mise run test:wyrd`, `git diff --check` | PASS |

**Scope change.** Spec revision 5 (owner-approved) adds Cursor, Pi, and Hermes
Agent. Host contracts were checked against current docs: pi.dev/docs/latest/mcp
(`~/.pi/agent/mcp.json`, `PI_CODING_AGENT_DIR`), cursor.com/docs/context/mcp
(`~/.cursor/mcp.json`), and the Hermes Agent MCP docs (`~/.hermes/config.yaml`
`mcp_servers`, `HERMES_HOME`).

**Hermes YAML.** No comment-preserving YAML dependency is installed and
`serde_yaml` is lossy, so the entry is written as one JSON flow line (valid
YAML) by a guarded line edit, then re-parsed and compared; any mismatch,
including inline `mcp_servers: {}`, reports `unreadable` and leaves the file
unchanged.

**Reuse map.** Auth, endpoint precedence, and transport reuse
`ClientConfig`/`WyrdClient`/`client::from_global`; no second credential
source, token cache, or tool catalog was added.

**Diagnosis — `pg_verification_runtime::unscored_drift_publishes_only_the_summary`
failed once in `test:wyrd` (`left: []`, `right: ["vala.verification.results"]`).**

- Symptom: `harness.writes()` returned no batch after `wait_run` saw the run
  `completed`.
- Evidence: `VerifierRunner::settle` committed the fenced `complete`
  (`verification/runner.rs`, `conn.commit()`), and only afterwards did
  `attempt` call `outbox.stage`; `Outbox::stage` is the only place `pending`
  rises, and `PublicationFault::sent` returns as soon as `settle()` sees
  `pending == 0`.
- Cause: the completion became visible in PostgreSQL before its result was
  pending on the outbox, contradicting the runner's module contract ("the run
  completes once it is staged"); a reader polling between commit and stage saw
  `completed` with nothing to settle.
- Fix site: `VerifierRunner::settle` now stages the result inside the
  lease-fenced transaction, after `complete` applies and before commit. A
  stale lease stages nothing; a commit failure retried under the same lease
  does not stage twice; a lost lease after a failed commit can leave one
  extra result for a re-executed run (owner-accepted trade-off). Other callers
  exposed to the same window (`pg_verification_runtime` completed-run, lost-ack,
  multi-replica, and reclaim tests) are fixed at the same owner.
- Diagnostician: independent read-only report confirmed the cause and fix
  site. Verified by all 32 `pg_verification_runtime` tests and `test:wyrd`.

**Non-goals.** No profile system, MCP secret store, separate package CLI,
second tool catalog, or global endpoint rewrite was added.
