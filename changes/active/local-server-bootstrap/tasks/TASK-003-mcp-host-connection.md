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
