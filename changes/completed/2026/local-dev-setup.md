# One local development setup

- Change: `local-dev-setup`
- Specification: `SPEC-local-dev-setup`, approved revision 1
- Completed: 2026-10-19
- Reviewed target: `a7feddd16` (TASK-001 with its R1 and R2 remediations,
  task re-review PASS), approved by the human owner in place of a final
  change review
- Delivery reference: not supplied

A developer who has a Wyrd client installed and PostgreSQL available runs
`wyrd server dev` and gets a working local server plus saved client
configuration. An agent gets the same result by supplying inputs without
prompts. Running the bare command again, from any directory, resumes the same
deployment. Gateway setup is optional. When it is chosen, provider keys come
from local environment variables named in a secret-free document and are
applied in one atomic server operation. The existing two-step credential and
deployment commands keep working.

## Shipped behavior

- `wyrd server dev` no longer requires `--tenant`. Each input resolves from
  the argument, then canonical environment, then saved development settings,
  then a concealed or plain prompt on a usable terminal. New flags:
  `--non-interactive`, `--bind`, `--grpc-bind`, `--storage-url`,
  `--gateway-file`, `--skip-gateway`. Storage URLs that carry credentials are
  refused as arguments. Unattended runs never prompt. They list every missing
  input together and exit before download, migration or provisioning.
- Database URLs persist through the protected `CredentialsFile`. Tenant,
  listeners, storage selection and absolute storage/Bifrost roots persist in
  a `[dev]` section of global configuration. Unset storage gets a warning and
  a stable user-config-relative location. The client keeps reading its
  endpoints from `[client]` and its tenant key from the existing credential
  store.
- Startup reuses `ServerInstaller`, ordinary `migrate` and server-owned
  `setup`. Before any tenant or gateway call it waits for readiness and
  proves a fresh client built only from saved files. A normal restart reuses
  the exact tenant key and sealing key and issues no new credential.
- On a fresh interactive run, gateway discovery offers OpenAI, Anthropic,
  Gemini (`GOOGLE_API_KEY`, else `GEMINI_API_KEY`) and TypeSafe Jev. It shows
  only provider, variable name and proposed deployment name. A resumed
  deployment does not repeat the gateway or MCP-host offers.
- `wyrd gateway deployment put --file` and `server dev --gateway-file` also
  accept a composite `{credentials, deployments}` document. A credential
  source may be `{"local_env": "<VAR>"}`. The CLI resolves it in memory into
  a write-only `managed_secret` and sends one request.
- `POST /v1/admin/gateway/provider-deployments` and the scoped MCP write tool
  `gateway.apply_provider_deployments` validate, seal and commit every named
  credential and deployment in one tenant transaction. They return redacted
  views in request order. The shared `CredentialWriter` gained the operation.
  First-class SDK `Gateway` handles did not.
- Errors name the failed stage. A gateway apply whose answer was lost reports
  that it may have committed and that reapplying the same named document is
  safe.

## Invariants and constraints

- The CLI collects input and supervises child processes. The server owns
  provisioning, sealing, authorization, SQL and atomic apply. Contracts in
  `wyrd-spec` stay IO-free.
- Secrets never appear in argv, general config, output, logs, responses or
  expanded files. `local_env` references are never persisted or resolved
  server-side. Existing `environment` and `external_secret` sources keep
  their operator semantics.
- A saved key is used only after preflight proves it against the selected
  server: the exact explicit key, no login or ambient substitution. A key with
  no saved `[dev]` selection, or a different-origin selection, is refused
  before any write or issuance. Loopback or a slug grants no tenant
  authority.
- A retained sealing key is never overwritten. A different supplied key is
  refused.
- Data is never moved, reset or merged implicitly. Lost-key recovery needs
  existing platform authority and explicit intent.
- Interactive input uses Unix terminal facilities common to Linux and macOS.
  Test fixtures use standard loopback binding, not procfs.

## Decisions and deviations

- No spec revisions: revision 1 governed the whole change.
- One task delivered the full scope. Two task reviews returned FIX_REQUIRED.
  R1 fixed unattended host prompting, identity preflight ordering, sealing-key
  retention, lost-answer guidance, signal handling around concealed input,
  SQL-abort rollback proof, rustdoc, and a procfs-dependent fixture. R2 made
  the exact-key preflight complete and derived the "already configured" offer
  policy from the retained key plus saved endpoint rather than listener
  equality. The third review passed with no open findings.
- A refused database switch still runs idempotent migrations on the selected
  database, because the key preflight needs a ready server. No tenant, key or
  file is created.
- If the key was saved but the endpoint was not, the partial resume stays
  "unconfigured" and the setup offers run again.
- `wyrd-cli` takes a direct dependency on `wyrd-utils`, which it already had
  transitively, so it can reuse the owner-only secret-file reader.
- The standalone `wyrd-example-py`, `-ts` and `-rs` repositories were updated
  outside this repository and left uncommitted. Review scope excluded them by
  owner direction. The in-tree consumer test skips when they are absent.
- Evidence limits: focused runs were Linux x86_64 only, with no macOS or arm64
  execution. Reviews were source audits. Broad integrated change-review
  journeys were not run because the owner approved in place of that review.

## Acceptance closure

| Criterion | Evidence | Result |
|---|---|---|
| AC-001 guided setup | Installed PTY journey: prompts, storage warning, decline gateway, fresh-client ready; SIGINT/SIGTERM at concealed input restores echo and persists nothing | PASS |
| AC-002 unattended | Missing-input, argument and conflict refusals; tty with `--non-interactive` asks nothing | PASS |
| AC-003 discovery and documents | Guided subset with Gemini precedence; composite CLI journey resolves `local_env` with no secret files | PASS |
| AC-004 atomic apply | Server Postgres proof including SQL-aborted rollback after a rotation, concurrency and tenant scope; MCP journey; served OpenAPI | PASS |
| AC-005 retained methods | Existing credential/deployment lifecycle, single/array documents, operator and sealed sources | PASS |
| AC-006 restart | Cross-directory restart with no exports keeps key, data, endpoints and provider decryption; listener-only restart asks nothing | PASS |
| AC-007 failure and recovery | Partial-failure, identity-refusal, lost-answer and interruption journeys; files byte-preserved, children settled | PASS |
| AC-008 consumers | Fresh Rust/Python/TypeScript and MCP consumers on saved config; no SDK secret mutation | PASS |
| AC-009 examples and docs | In-tree gateway example and docs; standalone repos only within the scope limit above | PASS within scope |
| AC-010 checks | fmt, lints, deps, tenant isolation, codegen, docs, examples, Python lint/type | PASS |

## Open at completion

- REQ-009 asked for a design update. Operations and security guidance were
  updated, but `architecture/wyrd-design.md` does not describe `wyrd server
  dev` or the composite `local_env` gateway apply. The owner closed the change
  without a final change review, so this gap was not routed to remediation.

## Links

- [Deployment operations](../../../architecture/operations/deployment-and-release.md),
  [security posture](../../../architecture/wyrd-security-posture.md),
  [Wyrd design](../../../architecture/wyrd-design.md)
- [Setup](../../../docs/src/content/docs/get-started/setup.md),
  [quickstart](../../../docs/src/content/docs/get-started/quickstart.md),
  [LLM gateway](../../../docs/src/content/docs/connect/set-up-the-llm-gateway.md),
  [troubleshooting](../../../docs/src/content/docs/operate/troubleshoot-a-deployment.md),
  [gateway example](../../../examples/gateway-setup/README.md)
- CLI: [`server/dev.rs`](../../../crates/wyrd/wyrd-cli/src/server/dev.rs),
  [`terminal.rs`](../../../crates/wyrd/wyrd-cli/src/terminal.rs),
  [`gateway.rs`](../../../crates/wyrd/wyrd-cli/src/gateway.rs)
- Client: [`global_config.rs`](../../../crates/shared/wyrd-client/src/global_config.rs),
  [`gateway_credential.rs`](../../../crates/shared/wyrd-client/src/gateway_credential.rs)
- Contract: [`gateway/deployment.rs`](../../../crates/wyrd-spec/src/gateway/deployment.rs)
- Server: [`gateway/service.rs`](../../../crates/wyrd/wyrd-server/src/components/gateway/service.rs),
  [`gateway/routes.rs`](../../../crates/wyrd/wyrd-server/src/components/gateway/routes.rs),
  [`mcp/gateway.rs`](../../../crates/wyrd/wyrd-server/src/mcp/gateway.rs)
- Tests: [CLI dev journeys](../../../crates/wyrd/wyrd-cli/tests/pg_tests.rs),
  [CLI gateway journey](../../../crates/wyrd/wyrd-cli/tests/gateway_server_journey.rs),
  [apply atomicity](../../../crates/wyrd/wyrd-server/src/components/gateway/pg_administration_tests.rs),
  [MCP gateway](../../../crates/wyrd/wyrd-mcp/tests/bifrost/mcp/gateway.rs),
  [served OpenAPI](../../../crates/wyrd/wyrd-server/tests/integration/pg_openapi_contract.rs),
  [installed-CLI Python journeys](../../../sdks/wyrd-sdk-python/tests/integration/test_local_development.py)
