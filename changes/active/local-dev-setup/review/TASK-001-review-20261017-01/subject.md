# Immutable review subject

Scope clarification from the user after initial review: review the full
implemented changes in this worktree. Sibling repositories are outside this
review; their commit state is not a blocker or a prerequisite to remediation.
The original external-scope observations below are retained as history and
superseded by this direction. The main base/candidate and all source findings
are unchanged. See `verdict.md` for the final `FIX_REQUIRED` result and R1 task.

- Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
- Base: `bc3bf25e3f78b0c429110350a3de4d58290e632f` (plan commit immediately before first task implementation).
- Candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.
- Read the cumulative range with `git diff bc3bf25e3f78b0c429110350a3de4d58290e632f...5cfb0a3ac61434256fa97a86f3506b124158780c`.
- Approved authority: `changes/active/local-dev-setup/spec.md`, revision 1.
- Original task: `changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
- No prior review or remediation artifacts exist for this task.
- Main tree was clean at start. No `.codegraph/` exists: use normal repository navigation.
- External examples: `/home/thorrester/Documents/GitHub/wyrd-example-{py,ts,rs}` contain uncommitted changes, including unrelated changes. Task evidence explicitly identifies these as uncommitted. No immutable base/candidate pair covering those task changes is supplied; do not silently treat external working trees as an immutable reviewed candidate.

## Shared navigation map

This map starts navigation, not coverage or conclusions. Expand and verify it.

| Changed owner | Callers/consumers and relevant proof |
|---|---|
| CLI `server/dev.rs`: `DevArgs::resolve`, `DevInputs`, `DevServer::{run,save_settings,save_tenant,recover,ready,stop}`, `offer_gateway`, `saved_client`; `terminal.rs`; `mcp/hosts.rs` | `DevArgs::run`, installer and server binary subprocesses, protected config, CLI `tests/pg_tests.rs`, Python installed CLI journeys |
| Shared client `global_config.rs`: `DevSection`, `DevDatabaseFile`, `local_databases`, `save_local_dev`; config re-export | `Environment`, protected file/credential owners, existing `save_local`, saved client discovery, unit proof in same module |
| CLI `gateway.rs`: `DeploymentFile`, `AuthoringApply`, `resolve`, `apply`, `put_deployments` | standalone deployment command and dev startup; existing single/array and source semantics; `gateway_server_journey.rs` |
| Pure contracts `wyrd-spec/src/gateway/deployment.rs`: collection request/response | restricted `CredentialWriter::apply_deployments`, HTTP collection route, MCP apply tool, schema/codegen, first-class SDK readers |
| Server `components/gateway/service.rs`: `GatewayAdministration::apply_deployments`, shared `write_credential`/`write_deployment` | individual put/revoke/delete, tenant SQL locking/reference rules, sealing and audit owners; `pg_administration_tests.rs` |
| Server gateway routes and `mcp/gateway.rs` | mounted HTTP, domain/scope gating, `wyrd-mcp/tests/bifrost/mcp/gateway.rs`, `pg_openapi_contract.rs` |
| Setup/gateway/troubleshooting docs, architecture operations/security, `examples/gateway-setup` | shipped document and installed CLI consumers; Python `test_local_development.py`; external app documents and onboarding |
| `mise.toml`, `scripts/check_ts_package.mjs` | local-dev lane, package inventory; existing package README added before base |

## Rules and verification

Read `AGENTS.md`, `architecture/agent-rules.md`, `architecture/references/README.md`, `architecture/references/languages/spec-driven-development.md`, maintainer-style, testing-workflows, and relevant design/doctrine/security/deployment authority. Follow `.agents/skills/wyrd-task-review/SKILL.md` for your role. Do not use implementation summaries as acceptance proof. The task's Implementation Evidence is available verification claims (P1–P12 plus focused checks), without captured command logs; independently inspect proof bodies.

Source-only review by default. Do not launch Cargo, build, lint, codegen, or integration commands: shared target directory commands must be coordinated sequentially. If an exact focused check is necessary, message the orchestrator. Do not edit implementation or other reports. Write only your assigned report. Report substantive findings with local IDs, authority, location, reachable consequence, and smallest testable correction. No optional improvements. Preserve the source and immutable candidate. Reports must explicitly document verification limits.
