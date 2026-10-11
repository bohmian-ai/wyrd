# TASK-004 independent behavior review

Overall result: **FAIL**. One bounded test integration regression is proposed.

## Immutable subject and coverage

- Base: `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`.
- Candidate: `741e42ed97ac3ff9638217bbcd4f0c0583e6d464`.
- Approved authority: `changes/active/local-server-bootstrap/spec.md`, revision 7; original task `changes/active/local-server-bootstrap/tasks/TASK-004-local-dev-journey.md`, including its final corrected commands and cleanup obligation.
- Separate docs range: `2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5`.
- Read repository rules, applicable doctrine/client and runtime-identity design, spec-driven development, maintainer style, reference router, and testing authorities. No CodeGraph directory exists.

Inspected the complete cumulative changed-file range: CLI dispatch/dev lifecycle/error/installer visibility; shared configuration and credential owners; MCP prompt/installer composition; Python and Node CLI adapters; CLI and three SDK test consumers; focused and aggregate mise lanes; npm package checker; current documentation; task evidence. Inspected the separate draft Setup, Troubleshooting, and future Quickstart prerequisite changes through Git without checkout. Source remained immutable; only this report was written.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-001: installed Python and Node commands project one Rust CLI | `server/mod.rs` dispatches DevArgs; existing Python PyO3 entrypoint and Node `runWyrdCli` remain delegates | Installed-wheel evidence; packed Node command fixture and package checker | PASS |
| REQ-004: retain installed compatible server; install only when absent; refuse incompatible version before migration | `DevArgs::run` selects `installer.installed()` first; `DevServer::new` applies existing `compatible` before `run` | CLI repeat case retains current symlink; existing release/compatibility tests plus source ordering | PASS |
| REQ-005 / INV-001: migration, ordinary serve, readiness, server-owned setup and foreground lifetime | `DevServer::{start,migrate,ready,provision,wait,stop}` invoke installed binary and normal serving path; `main.rs::setup` owns tenant issuance | CLI first-run journey uses `UnmigratedDatabase`, not pre-provisioned tenant; startup script evidence | PASS |
| REQ-007: external database and existing local defaults, including macOS memory | `local_bind`, inherited child environments, `detect_memory`; storage remains normal server configuration | Real local starts, missing database refusal, current docs; no service-manager addition | PASS |
| REQ-008 / AC-004: server-issued tenant key protected and distinct from root | `ProvisionedTenant` parses setup output; `GlobalConfig::save_local` uses `CredentialsFile`; `disclose_root` opens invoking tty | Real first-use checks credential mode 0600; PTY checks root only on tty and no tenant key disclosure | PASS |
| REQ-009: save effective loopback endpoint in existing directory and preserve unrelated content | Shared `Environment::config_dir`; locked editable TOML documents; credential-first endpoint-second save | Real CLI journey and `local_setup_preserves_content_and_existing_key` assertions | PASS |
| REQ-010: safe retries, retained key/release/cache and recoverable partial progress | Server setup checks active tenant first; expected saved key is passed to locked save; equal credential does not rewrite credential document | Retry and readonly-persistence failures; static source matches recovery descriptions; cached-byte unit case | PASS |
| REQ-010: announce readiness only after fresh saved-file client works | `verify_saved_client` uses fixed environment containing config directory, obtains access token and performs bounded Cards read before message | Fresh CLI and SDK write/read scenarios; explicit source ordering | PASS |
| REQ-011 / AC-005: interactive detection, explicit script selection, harmless decline | `mcp::offer_local` uses HostInstaller detection, shared selection grammar, then existing InstallArgs | Real PTY interrupt and decline cases; selected-host CLI test | PASS |
| REQ-014: selected host discovers and invokes authenticated read with no host secret | `offer_local` supplies endpoint to existing installer/proxy, no new MCP auth path | `pg_tests::server_dev_selected_mcp_host_reads` checks saved Codex config, tools and `bifrost.list_tables` call | PASS |
| REQ-015 / AC-007: clear redacted startup failure and recovery | Static stage/recovery strings in DevArgs/DevServer; existing installer errors and structured ServerDev variant | Missing URL, restricted migration, production-readiness and partial persistence cases; source inspection of failure branches | PASS |
| INV-003: preserve production hardening | Child construction overrides listener/memory only; no auth or production-gate bypass | Production startup missing signing key refuses readiness in CLI failure case | PASS |
| INV-004: protected atomic writes; no secret argv/log/host entry | DSNs inherited through env, setup key captured rather than forwarded; existing directory lock/atomic replace | CLI and terminal output assertions; host config assertion; failure guidance is static | PASS |
| AC-001, AGENTS §11–12: journeys also coexist in their existing owning integration lane | Newly parametrized Python bootstrap driver uses a new temp cwd/config per case but the same inherited DB for every case | Tracing-enabled two-case reproduction below fails; `py:test:integration:inner` selects all four cases together | FAIL |
| AC-008 / task docs obligation: current and draft docs match delivered journey and safe recovery | Current CLI/local-development pages; separate draft Setup/Operate pages; absent Quickstart documented in its existing owner task | Recorded current/docsite checks; source confirms drafts stay `status: partial`; Quickstart spec excludes empty pages | PASS |
| Cleanup: focused lifecycle functions; remove standalone script and retain installed-runtime proof | DevServer retains process state; invocation helpers retain no long-lived dependencies; fixture moved to existing integration module | Final focused lane evidence remains credible individually; integration-lane regression is separately identified | PASS |
| Non-goals: no managed Postgres/storage, no client DDL/key issuance, no normal-boot migration, no automatic upgrade or new auth contract | Complete production diff remains CLI orchestration plus shared file-owner extensions and runtime signal adapters | Static cumulative inspection | PASS |

AC-002/003/006 and REQ-016 onward have independent task owners; this review checks only TASK-004's changed interactions and regression boundaries, not acceptance of later tasks.

## Existing-owner and caller audit

The new DevServer lifecycle earns ownership of its retained child, installation and endpoint. There is no existing installed foreground lifecycle owner that already composes this journey: ServerInstaller owns verified acquisition, while server main owns migration/setup/serving. DevServer delegates each durable operation rather than reimplementing it. ProvisionedTenant owns the setup-output boundary and pure tenant/key relation; no server credential issuance moved client-side.

GlobalConfig extends the existing CredentialsFile owner and directory lock rather than introducing another store, lock file or writer. Sibling writers are SavedLogins save/renew/remove and the shared auth API-key token cache; they use the same file/lock and preserve the default key. The new save compares the expected key again under this lock, preserves existing cache bytes on equal keys, and writes the credential before endpoint. ClientConfig's ordinary credential chain still owns authentication; the bootstrap verification excludes ambient credential/endpoint overrides.

MCP installation and proxy retain their host-format editors and shared auth chain. The second input mode is needed because the standalone synchronous reader cannot yield to the local process cancellation owner; menu and parsing are shared. The Unix AsyncFd input owns only a separately opened terminal descriptor. Node listeners are confined to the literal `server dev` dispatch prefix; other commands retain their existing signal behavior. Python consumes only a pending KeyboardInterrupt after Rust has returned interruption code 130.

The test-regression producer is different: existing `with-test-postgres.sh` gives isolation to its *invocation*, not to every pytest case. Existing `UnmigratedDatabase` gives Rust CLI cases isolated empty databases; ordinary WyrdTestServer journeys own their databases through PgFixture. The new Python driver consumes the wrapper's fixed database directly and assumes every driver case gets a separate wrapper. That assumption holds in the focused mise lane but not in the already-existing Python integration lane.

## Proposed finding

### BEH-001 — REGRESSION: parametrized bootstrap drivers reuse durable state across fresh deployments

**Violated obligation:** AGENTS §11–12 requires runnable owning integration coverage and a green relevant lane; TASK-004 requires fresh first-use journeys, source-backed reuse of existing test owners, and preserved regression boundaries.

**Exact locations:** `sdks/wyrd-sdk-python/tests/integration/test_local_development.py:311` creates a new working/config directory per invocation; line 334 directly reuses the inherited platform database URL; lines 350 and 381 use fixed tenant `local` across all four parametrized cases. `mise.toml:1204–1209` runs all integration tests in one wrapper invocation. These changed driver cases are selected by that existing lane.

**Producer-to-consumer evidence:** First case creates Wyrd's database state and Iceberg catalog locations using temporary warehouse A, provisions `local`, and stores its key in temporary config A. Cleanup removes A but leaves the wrapper database alive. Second case creates temporary warehouse/config B, reuses exactly that database and launches the CLI for `local`. The normal server reads retained Iceberg locations for warehouse A and refuses them as outside B; the driver therefore never reaches its host prompt/readiness assertion. Even if storage were merely reused, the same tenant would already be active and its saved key would remain in removed config A, so a fresh config B cannot prove first-use provisioning. These are one producer defect, not separate fixes at readiness, token lookup or assertions.

**Observed consequence:** The existing Python integration/SDK verification gate fails after the first bootstrap case, despite all individually isolated focused cases passing. The task's claimed behavior-preserving fixture move is incomplete at its owning lane boundary.

**Traced reproduction, run against this immutable candidate:**

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]'
```

Result: **1 passed, 1 failed in 20.68 seconds**, exit 1. The first SDK roundtrip passes. The second native CLI exits 69; server diagnostics say `built-in tables could not be ensured`, `DataInvalid`, and `the location is outside the active warehouse`, naming the first and second distinct temporary storage roots. The CLI projects `WYRD_CLI_503_SERVER_DEV` at readiness. The controlling-terminal assertion subsequently fails because no prompt was reached; cleanup reports actual exit 69 instead of expected 130. This is an environmental-lifetime mismatch in the driver, not a server integrity refusal to weaken.

**Smallest testable correction boundary:** Give each independent driver invocation an isolated empty external database lifetime aligned with its temporary warehouse/config, through the existing repository-managed database lifecycle. Reuse the existing Postgres wrapper or existing unmigrated fixture authority; do not perform migration or tenant issuance in the fixture, create a second durable fixture owner, weaken the outside-warehouse validation, reuse a prior credential to impersonate first use, remove driver cases from owning coverage, or change deadlines/assertions. Keep all four runtime/signal scenarios and the packed/production installed-command consumers intact.

**Focused closure proof:** Run the exact two-case command above under one owning lane, then run all four driver cases together using the same managed outer environment. Both sequential launches must actually provision fresh deployments and retain their write/read or interruption assertions. Run the narrow Python integration lane that originally selected these tests; a full change-wide suite is not needed to locate or close this fixture defect.

## Failure-diagnosis assessment and limits

Recorded fixes correspond to their stated source boundaries: missing exhaustive error arm and request construction are compile failures; field access/import/chaining/type-skip edits preserve test assertions; HOME/mise trust handling confines HOME mutation to the CLI terminal process; stale Node build emission diagnosis matches the package executable; scoped Node listeners, early installation select, AsyncFd prompt, expected-key transaction and Python pending-signal handling match reachable interruption/persistence paths. No timeout, skip, gate exception or assertion weakening was found in those fixes. The current defect arose when the standalone runner was moved into a parametrized integration test; individually wrapped final commands do not prove aggregate test coexistence.

No prior independent FIND ledger exists. Live official-release transfer, forced terminal-write failure and the full aggregate were not rerun here. Existing supplied verification is assessed as evidence, not replaced by a claim that every lane was independently executed. Draft docsite accessibility runs do not render these draft pages; static source and reported command/build checks cover their present delivery boundary. The concrete two-case regression is independently reproduced with tracing and is sufficient to prevent acceptance until corrected.
