---
id: TASK-004-R1
kind: remediation
status: ready
spec: SPEC-local-server-bootstrap
spec_revision: 7
requirements: [REQ-005, REQ-007, REQ-008, REQ-009, REQ-010, REQ-011, INV-003, AC-001, AC-004, AC-005]
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-2, FIND-TASK-004-3, FIND-TASK-004-4]
---

# Local setup corrections

Execution skill: `$wyrd-implement`. This task packages independently validated findings; it does not request another design/planning cycle.

## Authority and immutable review inputs

- Approved spec: `changes/active/local-server-bootstrap/spec.md`, revision 7.
- Original task: `changes/active/local-server-bootstrap/tasks/TASK-004-local-dev-journey.md`, including final corrected commands and requested cleanup.
- Reviewed base: `cb5656b2b913dbd88a8f6736bf4a0baaf998960c`.
- Reviewed candidate: `741e42ed97ac3ff9638217bbcd4f0c0583e6d464`.
- Separate draft-docsite range: `2044e161e1a2020f5bb87cc6a36ee893ab311340..aecdaa05229d3571dedd362e8344fc28da5808a5`.
- Review verdict: `changes/active/local-server-bootstrap/review/TASK-004-741e42ed-20261011/verdict.md`.
- Validated ledger/recommendations: sibling `findings-validation.md`.

Apply AGENTS.md, architecture/agent-rules.md and applicable architecture/reference authority. This remains local development setup. Make the smallest changes at existing owners; do not add a production supervisor or security protocol.

User-directed correction: the recommendation and proof for FIND-TASK-004-1 below supersede the earlier lane-exclusion recommendation in the review reports. The diagnosed failure and finding ID are unchanged; this is a correction to the existing finding's remediation, not a new independently discovered finding.

## Diagnosis and selected correction

### FIND-TASK-004-1: shared test-lane state outlives fresh bootstrap directories

**Obligation/current failure.** Installed-runtime coverage must remain runnable after the cleanup. The new `installed_local_cli` fixture creates fresh temporary working/storage/config directories for every driver and deletes them afterward, but uses the unchanged inherited database URL and fixed tenant `local` (`sdks/wyrd-sdk-python/tests/integration/test_local_development.py:311–381`). Ordinary Python integration and Bifrost story selectors (`mise.toml:1204–1209`, `:607–612`) select all four drivers under one existing database lifecycle. The first leaves catalog/tenant state pointing at a removed warehouse/key store; the second correctly fails readiness. The tracing-enabled two-case reproduction returned one pass and one failure, with outside-active-warehouse diagnostics and exit 69. Individually wrapped dedicated cases passing does not prove these ordinary selectors work.

**Outcome/recommendation.** Give each installed startup test its own empty database through the existing `wyrd-dev-fixtures::pg::UnmigratedDatabase` owner, with the same lifetime as its temporary storage and credential directories. The wrapper continues to supply PostgreSQL for the overall run. The fixture supplies the isolated database URL only to its CLI and SDK child processes, retains the database across retries within that test, and releases it after those processes stop. Keep these tests in their normal integration selectors and retain the existing dedicated commands and SDK aggregate dependencies; do not add exclusions to make the failure disappear.

The Rust owner already creates and drops isolated empty databases and is used by the native CLI startup tests. It is not currently exposed to Python. Add only the necessary thin testing-only projection through the existing `wyrd-testing` and Python SDK testing boundaries; keep allocation and cleanup in the existing Rust owner and use the shared runtime. Include registration, public testing exports and source-generated typing as required. The production wheel must not acquire this test capability. Do not create databases through Python SQL, duplicate the database lifecycle, or use a pre-provisioned WyrdTestServer in place of the CLI being tested.

This fixes the producer lifetime mismatch. Each test still proves that the CLI performs migrations and issues credentials itself. Preserve server integrity checks, assertions, deadlines, fixed tenant names and ordinary test coverage. No database reset between consumers, credential seeding, standalone runner or new fixture owner is needed.

### FIND-TASK-004-2: the host prompt hides ordinary serving-child exit

**Obligation/current failure.** REQ-005 requires foreground exit reporting; REQ-010 requires truthful ready output. `DevServer::run` awaits `mcp::offer_local` before its child wait (`server/dev.rs:172–185`). The async prompt can remain unanswered indefinitely. If the retained server exits then, only CLI interruption/overall completion is observed; the parent stays in the prompt, and a later answer permits the final ready message before the existing exit error. Prompt Ctrl-C proof does not cover child loss.

**Outcome/recommendation.** Keep child-exit observation active during the optional host offer in the existing DevServer lifetime owner. On exit, cancel the pending offer, report the existing redacted server-exit failure, and settle/reap through the existing cleanup. Standalone MCP installation and shared prompt grammar remain unchanged. Python and Node must continue sharing the Rust behavior. No restart, another supervisor, polling infrastructure, SDK guard or persistent state is needed.

Use the existing terminal/process fixture for an unanswered offer followed by termination of only the serving child. Assert failure without terminal input, no final ready announcement, preserved files and secret-free recovery. Preserve the existing Ctrl-C and decline behavior. Any retry in this scenario must reuse its database/config/warehouse rather than manufacture another first-use deployment.

### FIND-TASK-004-3: local deployment connection is established only for HTTP

**Obligation/current failure.** REQ-007 requires safe local inputs; fresh SDK usability and INV-003 apply to public gRPC too. CLI preflight/child construction validates/overrides HTTP only (`server/dev.rs:105–126,151–161,483–514`). An inherited `WYRD_GRPC_BIND=0.0.0.0:50051` therefore reaches the ordinary plaintext public server listener. Its default is already safe loopback; this is a configured-input failure.

`GlobalConfig::save_local` saves only HTTP (`global_config.rs:104–142`). Shared `ClientConfig::from_environment` prioritizes an explicit saved `grpc_url` over new HTTP derivation (`config.rs:159–170`). A stale hosted endpoint therefore remains active for fresh Bifrost clients, which attach the local bearer and frame data (`bifrost/grpc.rs:98–105,262–281`). With no explicit saved URL, HTTP derivation instead fixes port 50051, which also mismatches a custom loopback public bind. Current SDK cases only write/read Cards through HTTP, so they do not prove saved gRPC usability.

**Outcome/recommendation.** Select the actual public gRPC bind in the existing CLI local-input boundary. Preserve a valid nonzero loopback `WYRD_GRPC_BIND`; otherwise use the ordinary loopback default. Reject invalid/non-loopback overrides before provisioning. Supply that selected value to owned child commands so a server-config file cannot bypass local selection. Save the corresponding actual client gRPC destination alongside HTTP through the existing locked GlobalConfig/CredentialsFile writer.

`client.grpc_url` is related deployment connection state and should be updated for the selected deployment; all unrelated settings, comments, saved logins, keys and cache behavior remain preserved. Use existing socket/URL and transport configuration mechanisms, without another config loader, file format, transport or language-specific fix. Normal server boot and remote deployment behavior remain unchanged. Do not add TLS, endpoint attestation or inherited listener contracts.

Extend the existing native first-use case with a stale saved gRPC URL, custom loopback gRPC port, and an actual fresh saved-client Bifrost gRPC write/read. Assert alignment and preserved unrelated config, without sending the local bearer/data to the stale destination. Extend the existing failure/retry case for non-loopback refusal before issuance and secret-free guidance. Confirm the selected CLI value governs a non-loopback server-file setting. Retain ordinary default success and the existing preservation unit test.

### FIND-TASK-004-4: required helper rustdoc contracts are absent

**Obligation/current failure.** AGENTS §16 requires error and panic sections on private helpers and tests. `mcp/hosts.rs:734` (`show_hosts`) and `:749` (`show_question`) propagate writes/flush without `# Errors`. `server/dev.rs:330` (`DevServer::ready`) expects the retained child without `# Panics`. `wyrd-cli/tests/pg_tests.rs:235` (`LocalDev::environment`) expects a UTF-8 fixture path without `# Panics`. Independent validation read the complete bodies and their callers.

**Outcome/recommendation.** Document these existing write/flush failures and lifecycle/path panic invariants. Preserve runtime bodies and signatures for this correction. Batch it with the behavior fixes; source inspection plus formatting closes the documentation gap without new tests or guards.

## User-directed addition: prompt only during first local setup

This is an explicitly requested behavior change, separate from independently validated FIND IDs. The user approved interactive first setup and requested that subsequent `wyrd server dev` attempts recognize completed configuration and proceed without another coding-tool configuration question. No new migration or configuration command is requested.

**Current behavior.** `DevServer::run` invokes `mcp::offer_local` on every successful startup. That offer detects installed coding tools, not whether local setup was already completed, so an interactive restart asks again even when saved configuration is usable.

**Required behavior and selected correction.** Use the existing saved local endpoint and credential state to recognize completed setup for the requested deployment before this invocation writes configuration. On first successful interactive setup, offer detected coding tools as today. On later starts with completed configuration, skip the automatic offer and proceed to readiness and foreground serving without user input. Declining coding-tool setup on the first run must also suppress the offer on later starts; installing an MCP connection is optional, not the definition of completed local setup.

Keep the decision in the existing CLI startup owner and reuse protected configuration/credential readers and validation. Do not add a setup marker, preference file, flag or separate persistence owner. Recognize prior configuration rather than the files just written during first setup. Unsafe, missing or unusable credentials continue through existing validation/recovery, never silently count as a completed deployment. If interruption happens at the first prompt after usable configuration was saved, a subsequent start recognizes that saved state and does not ask again. Preserve first-use child-exit supervision from FIND-TASK-004-2.

Explicit `--host` selections remain honored without a question. `wyrd mcp install` remains the explicit way to add or change coding-tool connections later, with its existing interactive behavior. Keep existing migration, provisioning, installer and shutdown semantics; this addition changes only automatic first-use prompting. Do not add `wyrd configure` or `wyrd server migrate`.

**Focused proof.** Extend the existing installed-command terminal fixture to prove first interactive setup offers detected coding tools, then stop and restart using the same database, storage and saved configuration. The restart must reach readiness without terminal input or another offer and preserve the key and connection files. Cover first-run decline and interruption after configuration save, plus explicit `--host` on a later start. Retain non-interactive startup and standalone `wyrd mcp install` behavior. Use the existing focused first-run/retry and Python `[interrupt]`/packed-TypeScript decline cases; record exact selectors for any additional test genuinely needed. Update the current CLI reference and local-development guide to distinguish first setup from everyday restart.

## Scope, preserved behavior and non-goals

Expected owners are the CLI dev lifecycle, shared protected global config, their existing tests, existing Python terminal fixture, the existing Rust database-fixture/testing boundaries needed for its thin Python projection, and current CLI/local-development documentation. Ordinary test selectors remain unchanged. Paths guide ownership, not private mechanics. Batch only documentation corrections needed for these changes.

Preserve server-owned migration/issuance, installed release verification/compatibility, no silent updates, active-tenant no-reissuance, protected whole-document preservation, cached tokens on same-key retry, terminal-only root disclosure, static secret-free recovery, existing installer/MCP editors, standalone MCP behavior, and installed Python/Node exit handling. Do not weaken a gate or hide required test coverage.

Explicit non-goals: hostile local co-user protection, a listener handoff protocol, automatic restarts, new database/service management, new transports/auth contracts, another fixture runner/owner, language-specific durable logic, unrelated refactoring or draft-docsite publication. The rejected SEC-001 proposal is not a remediation obligation.

## Acceptance criteria

| Finding | Required closure |
| --- | --- |
| FIND-TASK-004-1 | Each startup test owns a fresh unmigrated database for its entire deployment lifetime; retries inside that test retain the same database and files. The previously failing pair passes sequentially under one wrapper, and ordinary integration collection still includes the new tests. Existing tests and dedicated/aggregate coverage remain enabled; production packages do not expose the testing-only projection. |
| FIND-TASK-004-2 | An unanswered offer cannot hide child exit or produce final readiness; CLI settles with redacted failure, retains saved state, and existing interruption/decline behavior remains intact. |
| FIND-TASK-004-3 | Invalid/non-loopback public gRPC input is refused before provisioning; selected loopback bind governs child configuration and saved fresh-client destination; default/custom-port saved-client gRPC write/read works despite stale prior configuration; unrelated files/settings/key/cache survive. |
| FIND-TASK-004-4 | All four specified helper error/panic contracts are documented substantively and match their final bodies. |
| User-directed first-use prompting | First interactive setup offers detected coding tools; subsequent starts with completed saved configuration reach readiness without another question, including after decline or interruption at the first offer. Explicit `--host` and standalone `wyrd mcp install` remain usable. No new command, setup marker or preference file is added. |

## Focused proof

Rebuild the touched production artifacts through existing owners before process tests: `mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server`, and `mise run py:setup` for the embedded Python CLI used by the fixture. Use tracing when diagnosing failures. Extend current cases rather than replacing their assertions or creating a new harness.

```bash
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready) | test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=global_config::tests::local_setup_preserves_content_and_existing_key)'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]'
WYRD_LOG=info scripts/postgres/with-test-postgres.sh -- mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m integration 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[python]' 'tests/integration/test_local_development.py::test_installed_cli_bootstrap[interrupt]'
```

The last command is the exact two-case regression reproduction: both tests must pass within one wrapper invocation, with their original write/read and interruption assertions. Confirm ordinary integration collection still includes all four startup cases and existing local-development tests. Exercise the changed fixture's remaining Rust and packed-TypeScript driver cases through their existing exact dedicated commands. Also run one existing WyrdTestServer-backed local-development test alongside the regression pair in the same wrapped pytest run to verify the new database fixture does not alter the ordinary server fixture or its environment. Identify its exact node ID from collection and record the command and selected counts. Retain existing selectors and aggregate dependencies; no special exclusion or replacement lane is part of the correction.

Run `mise run fmt` and `mise run lints` for the Rust write set, plus `mise run py:format`, `mise run py:lints`, and `mise run py:typecheck` for the Python testing projection and fixture. Regenerate typing through its existing source/generator and run `mise run codegen:check`; never hand-edit generated stubs. Run `mise run check:deps` and `mise run check:py-wheel-no-testing` to preserve test-only dependency/package boundaries, `mise run docs:check` for the first-setup/restart documentation, and `git diff --check`. Do not require full user-journey suites, all Bifrost/SDK suites, or `mise run gate` for this remediation. Final integrated change review owns those broader journeys once.

Record exact commands, selected test counts, traces/diagnoses, closure against each FIND ID, and the separately identified user-directed first-use prompting behavior. A later `$wyrd-task-review` audits the original task, this verdict/ledger, this remediation, the explicit user instruction, and the complete cumulative fixed candidate.
