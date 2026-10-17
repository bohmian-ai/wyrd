---
id: TASK-001
kind: implementation
status: proposed
spec: SPEC-local-dev-setup
spec_revision: 1
requirements: [REQ-001, REQ-002, REQ-003, REQ-004, REQ-005, REQ-006, REQ-007, REQ-008, REQ-009, INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009, AC-010]
depends_on: []
---

# Make local development one repeatable command

## Outcome and Value

A developer or agent starts, configures and resumes local Wyrd with
`wyrd server dev`. The same input-resolution and execution path handles
interactive prompts and complete unattended inputs. Gateway setup is optional;
when selected, local environment references configure credentials and
deployments through one atomic operation. The existing two-step gateway path
and server-side credential sources remain supported.

This one task delivers the entire accepted workflow and its consumer/test
closure. It extends the existing owners, rather than introducing a second
bootstrap engine. Execute with `$wyrd-implement`; independent
`$wyrd-task-review` is required afterward. This task is separate from
verifier-building-blocks and has no semantic dependency on TASK-006.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Installed startup | `wyrd-cli/src/server/dev.rs::{DevArgs,DevServer}` and `server/install.rs::ServerInstaller` | `tests/pg_tests.rs` first-use, restart, interruption and sealing-key cases; Python installed-CLI journeys | Tenant required at parse time; database env-only; no guided gateway stage | Extend current startup/input collection and supervised stages | None: current workflow already owns download/migrate/setup/readiness |
| Saved settings and credentials | `wyrd-client/src/global_config.rs::{GlobalConfig,save_local}`; `CredentialsFile` locking/replacement; `Environment` | `save_local_tenant`, `verify_saved_client`, existing local-dev journeys | Only client endpoints/key persist; server inputs and absolute data roots do not | Add development settings through the same protected persistence owners | No parallel credential store/profile framework |
| Tenant provisioning/recovery | `wyrd-server/src/main.rs::{setup,root_credential}`; existing `Platform` administration | CLI first-use and partial-failure tests; `save_local_tenant` refusal paths | One-time key loss currently sends users to separate recovery instructions | Preflight/save/resume locally; guide explicitly authorized existing recovery when required | No new issuance, recovery or durable receipt service |
| Local gateway authoring | `wyrd-cli/src/gateway.rs::{DeploymentFile,put_deployments}`; `ProviderCredentialWriteSource` / `ProviderDeployment` | `gateway_server_journey.rs` single/array and redacted lifecycle cases; `examples/gateway-setup/gateway.json` | Documents cannot resolve local env credentials; array writes commit separately | Add composite document support to existing command and share it with server-dev selection | New local-env source is authoring syntax only; reuse existing typed contracts |
| Atomic credential/deployment apply | `GatewayAdministration::{put_credential,put_deployment,seal,validate_source}`; existing tenant SQL writes | Server `components/gateway/pg_administration_tests.rs`; shared `CredentialWriter`; HTTP route registrations | Existing public methods each own a separate transaction | Extend same service with one tenant-transaction operation and shared validation/sealing behavior | One required atomic operation, no new state/store/encryption owner |
| MCP / secret-mutation boundary | `wyrd-server/src/mcp/gateway.rs::{gated,TOOLS}`; `wyrd-client/src/gateway_credential.rs::CredentialWriter` | MCP `gateway::pg_tests`; SDK deployment readers/invokers | New operation absent from runtime catalog and restricted transport | Project same typed operation through scoped MCP and existing mutation handle | No SDK managed-secret API or duplicate transport |
| Example onboarding | Main setup/gateway/support-desk guides; standalone Python README and `docker/{keys,setup}.sh`; sibling TS/Rust setup | Python installed-CLI gateway journey; standalone apps' existing tests | Examples duplicate bootstrap or require manual key publication | Replace Wyrd setup recipes with shared command and env-reference documents | No app framework changes; PostgreSQL-only recipe may remain |

Owner paths are guidance, not a private symbol/file allowlist. Revalidate the
complete workflow and consumers before adding helpers. Server encryption,
validation, authorization, transaction and credential-reference locking remain
in the existing owners. Do not implement atomic apply by looping over public
methods that each commit independently.

Scope includes CLI projections, shared protected config, pure gateway DTOs,
server HTTP/MCP/service/SQL seams, schema sources, documentation and necessary
example changes. First-class SDKs consume the configured gateway, but do not
gain secret-mutation methods. Do not remove `environment`, `external_secret`
or `managed_secret` sources, or change existing single/array command semantics.

No PostgreSQL installer/role creation, UI wizard, daemon, new login framework,
new encryption system, arbitrary provider discovery, live provider calls,
implicit key rotation, data reset or automatic application Card deployment.
Preserve unrelated configuration, working data and concurrent repository edits.

## Approach

1. Establish failing input-resolution and installed-CLI journeys, including
   gateway skip, before extending `DevArgs` / the saved configuration owner.
2. Add composite local authoring and the one typed server apply operation,
   reusing credential sealing, validation, tenant SQL and audit.
3. Connect the existing standalone gateway command and optional guided startup
   to that operation; add its scoped MCP projection and generated contracts.
4. Complete saved restart, readiness, interrupted-stage and authorized-recovery
   behavior without changing normal issuance or production server semantics.
5. Update server-backed examples, standalone app onboarding, architecture and
   docs; prove fresh saved-config consumers and retained setup methods.
6. Run focused verification, inspect the complete diff and append evidence.

## Ordered Implementation Scenarios

P1–P12 in Verification are exact focused commands. New selectors are marked
planned; their current package/target/parent module was inspected. Confirm
collection once authored and record an equivalent exact command if local test
placement changes. A zero-test selection is never proof.

### Scenario 1 — Fresh guided and unattended setup produce usable configuration

**Behavior.** Missing database/tenant inputs are collected interactively;
explicit/env/saved precedence is consistent and invalid input is refused.
Unattended runs never prompt and list all missing requirements before effects.
Unset storage is explained and persisted as absolute paths. Download,
migration, tenant setup, saved credentials and fresh-client readiness use the
existing workflow (REQ-001–003/007, INV-001–004/006, AC-001/002).

**RED.** Add the terminal/input cases in P1 and focused resolution/refusal
cases beside `DevArgs`. Current parsing requires `--tenant`, missing database
input errors, and no server settings are saved. Extend P2's first-use proof
to assert safe saved settings and no key/DSN disclosure, retaining its real
server and fresh-client checks. The RED must expose missing behavior rather
than unavailable release downloads or database setup.

**GREEN.** Resolve inputs once, prompt only when appropriate, validate before
side effects, persist through existing protected stores, and pass effective
settings to all owned children. Retain server-owned migration/provisioning and
bounded readiness. P1/P2 pass; no second local-server implementation is added.

**REFACTOR.** Consolidate repeated state/dependencies into the current owning
structs where they retain real lifecycle/configuration. Reuse existing prompt,
installer, config and secure-file utilities; do not create a wizard framework.

### Scenario 2 — A composite gateway document is atomic and preserves existing methods

**Behavior.** A secret-free document resolves selected `local_env` names in
the CLI and sends one request. The server seals and commits every credential
and deployment together; invalid/foreign/revoked references, missing sealing
configuration, SQL failure, permission denial and oversized input leave no
partial new configuration. Concurrent revoke/replacement cannot produce a
committed invalid reference. Existing commands and credential sources retain
their behavior (REQ-005/006, INV-001–003/005, AC-003–005/010).

**RED.** Add P3's compiled-CLI composite-document journey and P4's server
atomicity/security/concurrency cases. The composite input and collection
operation currently do not exist. Include duplicate names, missing/empty env,
provider mismatch, unused credential entries, invalid later deployment,
rollback after an earlier would-be write, unchanged unrelated resources,
and redacted errors. P5/P6 remain existing regressions, not manufactured RED.

**GREEN.** Extend the pure contracts and current administration service with
the selected collection POST and request/response. Resolve local-only sources
before transport; execute shared validation/sealing and SQL under one caller-
owned tenant transaction. Use current locks/reference integrity and ordinary
error mapping. Existing single/array methods keep their established semantics.
P3/P4 pass and P5/P6 remain green.

**REFACTOR.** Share current transaction-composable behavior rather than copying
the credential/deployment workflows or nesting separately committed methods.
No parallel secret store, compensating-write rollback, deployment receipt,
custom encryption or source reinterpretation.

### Scenario 3 — Optional discovery applies only the approved providers

**Behavior.** Fresh interactive startup can decline gateway setup or approve
a subset of detected OpenAI/Anthropic/Gemini/Jev variables and names. No-key
startup works. Noninteractive omission and `--skip-gateway` skip it; explicit
`--gateway-file` uses the same apply path and conflicts are refused. The
summary distinguishes configured/skipped and does not claim upstream key
validity (REQ-004/005/007, INV-002/005, AC-001–003).

**RED.** Add the selection/skip cases to P1 and P7's installed-CLI gateway
journey. Use local provider fixtures and distinct secret sentinels; currently
startup has no gateway offer or apply path. Prove unapproved provider values
are not submitted, keys never appear in output/files, and Gemini precedence
is deterministic. A supplied file requires no approval prompt in unattended mode.

**GREEN.** Offer known providers, show only variable/provider/deployment names,
and apply the approved document once through Scenario 2's owner. Keep optional
MCP-host selection independent. A declined gateway still reaches readiness;
a requested failing apply reports its stage and settles the owned process,
preserving completed tenant setup. Rerun P1/P3/P7.

**REFACTOR.** Keep one document resolution and apply path for interactive,
file-driven and standalone gateway commands. Reuse typed adapters and auth
metadata; do not scatter guessed provider behavior through startup code.

### Scenario 4 — Restart and partial failure preserve identity and owned resources

**Behavior.** Bare restart from another directory with no repeated exports
uses saved database/listeners/tenant/data paths/key and sealing key, preserves
data and provider decryption, and repeats no completed setup offers. Unsafe
files, conflicts, migration/readiness failure, interruption, lost one-time key
and missing recovery authority produce honest actionable failures. No
automatic key reissue or cross-tenant adoption (REQ-002/003/008, INV-002–004/006,
AC-006/007).

**RED.** Add P8's working-directory/restart case and extend P9's partial-stage
coverage. Remove inherited database/storage inputs for the restarted process;
the current implementation cannot restore them and uses relative defaults.
Observe retained key identity, data and provider resolution. Force failure
before/after protected persistence and preserve the existing no-reissue
assertions; recovery tests supply existing authority explicitly.

**GREEN.** Save and reload effective configuration through existing owners,
preserve stable data/sealing paths, and complete partial local persistence
where the issued credential is available. Where it is lost, expose the
existing explicitly authorized recovery flow, never infer authority from
loopback or a slug. Interrupt/failed-stage cleanup settles owned children.
Rerun P2/P7/P8/P9.

**REFACTOR.** Reuse existing shutdown and stage diagnostics. Add no durable
bootstrap state machine, time-based retry workaround, credential journal or
filesystem/Postgres atomicity promise. Do not copy `/proc` or OS-specific
process assumptions from existing fixtures into changed runtime/test code.

### Scenario 5 — Agents and fresh language consumers use the same setup

**Behavior.** The new collection operation has served OpenAPI and a scoped
MCP projection using the same typed owner, authorization and redaction.
Fresh SDK processes use saved settings and invoke/read configured deployments;
secret mutation remains confined to CLI/scoped MCP (REQ-006/007/009,
INV-001–003/005/006, AC-004/008/010).

**RED.** Extend P10's real MCP gateway journey to discover/apply/read the
composite operation, refuse missing scope/domain authority and another tenant,
and return no secrets. It currently lacks the tool. Extend P11's served
OpenAPI assertion for the collection request, response and refusals.
Keep the existing installed CLI-to-language consumer assertions in P1/P7;
the new setup path must actually feed those consumers.

**GREEN.** Publish the derived contracts, register the route and scoped tool,
and use the restricted shared mutation handle. SDKs read/invoke through their
existing facade and transports. P7/P10/P11 pass with saved-file-only client
configuration and local upstream fixtures.

**REFACTOR.** Remove duplicated transport or validation if exposed by the
journeys. Generate consumed contracts from source and keep Python-authored
stubs synchronized only where affected. Do not add SDK secret mutation merely
to make a test convenient.

### Scenario 6 — Examples teach and exercise the installed setup workflow

**Behavior.** Server-backed main examples and all three standalone apps use
the shared command, optional gateway and secret-free local-env document;
application registration/start remain explicit. Existing two-step setup is
still documented, and offline examples remain offline (REQ-009, INV-005,
AC-005/008–010).

**RED.** Extend P7/P12 to run the shipped example gateway documents through
the installed command and saved-config consumers. Current documents require
prior credential files/puts. The docs-only rewrite has no manufactured RED:
inspect the README/script path and prove it no longer duplicates Wyrd
migration, key generation or tenant bootstrap.

**GREEN.** Update documents, scripts and examples to use the new command;
remove obsolete Wyrd Docker bootstrap recipes while retaining a PostgreSQL-
only option if useful. Keep current honest SDK acquisition instructions—do
not invent published packages/releases. Run P7/P12 and the touched examples'
existing smoke checks against built SDKs/local providers.

**REFACTOR.** Delete duplicated example setup instead of wrapping it in more
scripts. Keep the apps standalone; no app imports another example's Cards,
temporary checkout symlink or secret-bearing generated file.

## Acceptance Criteria

| Criterion | Obligations | Owner and proof |
|---|---|---|
| Guided/complete unattended setup converges; missing/invalid inputs refuse safely | REQ-001–003/007, AC-001/002 | DevArgs/DevServer/config; P1/P2 and focused resolution proof |
| Gateway is optional; discovery sends only approved providers with redacted output | REQ-004/005/007, AC-003 | CLI selection/authoring; P1/P3/P7 |
| Composite apply is atomic, tenant-scoped, reference-safe and server-sealed | REQ-005/006, AC-004 | GatewayAdministration/SQL; P3/P4/P10 |
| Two-step/single/array setup and all existing sources retain behavior | REQ-005/006, INV-005, AC-005 | Existing command and source owners; P5/P6/P7 |
| Restart and failures retain identity/data and expose authorized recovery without reissue | REQ-002/003/008, AC-006/007 | DevServer/secure files/platform owners; P8/P9 and retained sealing assertions in P7 |
| Fresh SDK/MCP consumers work; schemas/exports preserve secret boundaries | REQ-006/007/009, AC-008/010 | Client/SDK consumption, HTTP/MCP; P1/P7/P10/P11 and codegen/dependency checks |
| Main and standalone examples use shared installed setup and accurate prerequisites | REQ-009, AC-009 | Example docs/scripts; P7/P12, example smoke and docs checks |

INV-001–006 apply to every row. Record their source/evidence preservation,
including no plaintext leaks, no production-security relaxation and no
unrelated resource deletion. All spec REQ/INV/AC IDs map to this one task.

## Expected Write Set and Consumer Closure

- `wyrd-cli` server dev/installer composition, gateway document/dispatch and
  existing CLI runtime projections; protected global config/credential owners
  in `wyrd-client` and necessary typed settings/error projections.
- Pure gateway request/response and authoring validation sources in
  `wyrd-spec`; restricted shared `CredentialWriter` transport. No new
  first-class SDK secret-mutation exports.
- Server gateway administration/routes/MCP registration, existing tenant SQL
  query composition and tests. Reuse current tables; a new store or migration
  is not prescribed or needed merely for collection apply.
- CLI/terminal and installed-server journeys; server Postgres, OpenAPI and
  runtime MCP proof; generated outputs from their sources.
- Main setup/quickstart/gateway/support-desk and other server-backed examples,
  current architecture/security/operations guidance and relevant mise lanes.
- Sibling `../wyrd-example-py`, `../wyrd-example-ts`, `../wyrd-example-rs`
  READMEs, gateway documents, bootstrap scripts and required smoke tests.
  Each repository retains its own reviewable diff and verification evidence.

Paths guide the consumer closure; private structure remains implementation-
owned. Do not alter verifier-building-blocks packets/reviews or unrelated
working-tree changes. No branch switch, commit, merge or release is implied.

## Verification and Evidence

The following source-backed targets were inspected. P1/P3/P4/P8/P12 are
planned additions; others extend existing tests while preserving their prior
assertions. Tests requiring Python/Node lifetimes stay in those runtimes.
Before installed CLI proof, run `mise exec -- cargo build --locked -p
wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests`.
Run `mise run py:setup`, `mise run ts:build` and
`mise run ts:build:testing` when their installed-language proof requires it.
Use repository Postgres wrappers; no synthetic host load or provider secrets.

| Proof | Exact focused command |
|---|---|
| P1 — planned guided terminal and unattended input journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup` |
| P2 — existing first-use readiness/client journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'` |
| P3 — planned composite CLI journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all --test-threads 1 -E 'test(=gateway_server_journey::deployment_document_applies_local_environment)'` |
| P4 — planned server atomicity/security/concurrency proof | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped)'` |
| P5a — retained credential/deployment CLI lifecycle | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all --test-threads 1 -E 'test(=gateway_server_journey::gateway_commands_manage_redacted_tenant_configuration)'` |
| P5b — retained single/array document semantics | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -P journey --run-ignored=all --test-threads 1 -E 'test(=gateway_server_journey::deployment_file_accepts_one_or_many)'` |
| P6a — retained server-side operator sources | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_credential_sources_resolve_from_operator_config)'` |
| P6b — retained sealing and identity-bound envelopes | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_managed_secrets_persist_sealed_and_bound_envelopes)'` |
| P7 — existing installed setup, examples and fresh language gateway callers | `mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup` |
| P8 — planned saved-settings/cross-directory restart | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'` |
| P9 — existing interruption/partial-stage/no-reissue regression | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'` |
| P10 — existing MCP gateway journey, extended for composite apply | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(=gateway::pg_tests::agent_administers_redacted_gateway_configuration)'` |
| P11 — existing served gateway contract, extended for collection POST | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::gateway_operations_publish_problem_json_refusals)'` |
| P12 — planned standalone example setup/document smoke | `mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_standalone_examples_use_saved_local_setup` |

P12 must exercise shipped example documents/commands and a real saved-config
consumer, rather than merely assert README strings. Existing standalone app
tests support regression but do not substitute for installed-command setup.
Use each sibling's current runner through `mise exec --` and the managed
Postgres environment for touched app smoke; record exact named-test selectors
when selected. Preserve honest SDK acquisition and independently record all
three sibling diffs; unavailable external artifacts are explicit evidence limits.

Additional focused checks: `mise run fmt`, `mise run lints`,
`mise run check:deps`, `mise run check:tenant-isolation`,
`mise run codegen:check`, `mise run docs:check`, `mise run check:examples`.
Generate source-driven artifacts with `mise run codegen:regen`; do not hand-edit
snapshots. P11 proves the actual served OpenAPI; P10 proves the MCP catalog.
If Python/TS sources change, run their formatting/lints/type checks and required
owning-runtime unit lane; package `.pyi` is authored. No UI changes are needed.

Docs, examples and generated updates use static/consumer/regression proof;
they need no manufactured RED. New behavioral branches use ordered TDD above.
Record additional exact commands for any specifically named test added during
execution, including injected failure and concurrency cases. Confirm that a
permission-denial test reaches the real public boundary, not a harness parser.

Full user-journey suites and broad integrated aggregates run once at change
review, not as task verification. Even with one task, focused execution proof
does not replace final integrated review. Diagnose unexpected failures with
tracing and follow `$wyrd-implement`'s fresh-diagnostician rules when triggered.

Before handoff, run `git diff --check`, inspect tracked/untracked changes in
each touched repository and append the spec acceptance matrix, non-goal
closure, actual command results, diagnosis/evidence limits and immutable
candidate identities. Planning has run no implementation tests.

## Material Stop Conditions

- Correctness needs new credential issuance/recovery semantics, storing an
  additional unrestricted platform credential automatically, a role change,
  cross-tenant authority, or implicit key rotation.
- Atomic gateway apply cannot preserve current reference/revocation/assignment
  rules without a new durable store or changed concurrency contract.
- A requirement would expose secrets in argv/general config or SDK mutation,
  weaken production hardening/readiness, or silently move/reset existing data.
- Installed release/package availability prevents required real consumer
  proof; report the exact missing artifact and finish unaffected work rather
  than claiming publication or replacing the installed path with a source-only
  application bootstrap.

Route genuine material changes to `$wyrd-spec`; ordinary helper placement,
fixture organization, safe prompting and equivalent focused commands remain
implementation-owned. The existing credential sources and optional-gateway
decision are settled; no extra naming/approval rounds are required.

## Authority Links

- [Approved spec revision 1](../spec.md).
- [AGENTS.md](../../../../AGENTS.md) and
  [agent rules](../../../../architecture/agent-rules.md).
- [Design](../../../../architecture/wyrd-design.md),
  [doctrine](../../../../architecture/wyrd-doctrine.mdx),
  [security posture](../../../../architecture/wyrd-security-posture.md),
  [deployment operations](../../../../architecture/operations/deployment-and-release.md),
  [Bifrost design](../../../../architecture/bifrost-design.md).
- [Reference router](../../../../architecture/references/README.md),
  [spec workflow](../../../../architecture/references/languages/spec-driven-development.md),
  [implementation execution](../../../../architecture/references/languages/implementation-execution.md),
  [testing](../../../../architecture/references/languages/testing-workflows.md).

## Implementation Evidence

Commits: 2f3c4abef, bb9c7fe68, 7f7c99543, 88c8a07d1, 288bbd536, 6f2b215c6,
3c875c3d6, and the evidence commit. Build:
`mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Guided/unattended setup converges; missing/invalid inputs refuse safely | `wyrd-cli/src/server/dev.rs` `DevArgs::resolve`, `terminal.rs` | P1, P2, P8 (unattended missing-input, storage-userinfo, gateway-file/skip refusals) | PASS |
| Gateway optional; only approved providers sent; redacted output | `DevInputs::offer_gateway`/`configure_gateway`, `PROVIDERS` | P1 (gemini only), P3, P7 | PASS |
| Atomic tenant-scoped apply over HTTP/MCP; no partial commit | server gateway apply, `wyrd-mcp` gateway tool | P4, P10, P11 | PASS |
| Retained credential/deployment and server-source semantics | unchanged owners | P5a, P5b, P6a, P6b | PASS |
| Saved `[dev]` settings, protected DB URLs, absolute data paths, cwd-independent restart | `wyrd-client/src/global_config.rs` `save_local_dev`/`local_databases`; `DevServer::save_settings` | P2, P8, `global_config` unit test | PASS |
| Preflight save before key issue; no reissue; explicit recovery | `DevServer::run`/`save_tenant`/`recover` | P9 | PASS |
| Main example/docs use shared setup and secret-free `local_env` document | `examples/gateway-setup/*`, `docs/...`, architecture notes | P7, `check:examples`, `docs:check` | PASS |
| Standalone sibling apps use shared setup | sibling `gateway.json` (`local_env` `OPENAI_API_KEY`), PostgreSQL-only Compose, READMEs; `test_standalone_examples_use_saved_local_setup` | P12 (with `WYRD_EXAMPLES_DIR=<parent of the siblings>`), sibling `uv run pytest` 6 passed, `npm test` 6 passed | PASS |

Checks: `mise run fmt`, `lints`, `check:deps`, `check:tenant-isolation`,
`codegen:check`, `docs:check`, `check:examples`, `py:lints`, `py:typecheck`,
`git diff --check` — all pass. `scripts/check_ts_package.mjs` now allows the
package `README.md` added by a9ed593cd (root cause of the P7 inventory failure).

Non-goals stayed excluded: no new credential sources, no platform-specific
mechanism, no generated-artifact hand edits, no secret in argv/config/output.

Sibling diffs (uncommitted; this task implies no sibling commit):
- `wyrd-example-py`: `docker-compose.yml` → PostgreSQL only, `docker/` removed,
  `gateway.json` → composite `local_env` document (all three were untracked
  in-progress files; originals equal `wyrd-example-ts` HEAD), README setup
  and "Run it with OpenAI" step 1 → `uv run wyrd server dev --gateway-file`,
  `test/conftest.py` reads `["deployments"]`. Other uncommitted edits in that
  tree predate this task and were left as found.
- `wyrd-example-ts`: same Compose/`docker/`/`gateway.json`/README changes with
  `npx wyrd`; `test/support-desk.test.ts` reads `.deployments`. Its prior
  `README.md`/`scripts/evidence.ts` edits were left as found.
- `wyrd-example-rs`: new `gateway.json`; README quickstart uses
  `--skip-gateway` and live setup uses `--gateway-file` in place of the
  `managed_secret` pipe. No code changed, so its ignored cargo test was not rerun.

Limits:
- P12 skips when the siblings are not checked out (CI); locally it ran against
  `/home/thorrester/Documents/GitHub/wyrd-example-*`.
- Interactive prompting is Unix-only (`cfg(unix)`); other hosts get the
  unattended refusal listing missing inputs.
