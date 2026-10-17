---
id: TASK-001-R1
kind: remediation
status: proposed
spec: SPEC-local-dev-setup
spec_revision: 1
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8]
depends_on: []
---

# Correct local setup prompting, identity, restart and failure proof

## Authority and execution scope

Execute directly with `$wyrd-implement` under approved
`changes/active/local-dev-setup/spec.md` revision 1 and original
`changes/active/local-dev-setup/tasks/TASK-001-guided-local-development.md`.
The complete independently validated diagnosis is in
`changes/active/local-dev-setup/review/TASK-001-review-20261017-01/findings-validation.md`;
the reconciled review is `verdict.md` in that directory.

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/local-dev-setup`.
Original base: `bc3bf25e3f78b0c429110350a3de4d58290e632f`.
Reviewed candidate: `5cfb0a3ac61434256fa97a86f3506b124158780c`.
These eight findings are the first remediation round. User direction scopes
this review and remediation to implemented changes in this worktree; sibling
example repositories are outside this review and require no commits or edits
to proceed. No specification revision is needed for these corrections.

The outcome is the existing one-command workflow with reliable unattended
behavior, preserved deployment and sealing identity, truthful failure guidance,
and focused proof of the diagnosed gaps. This task does not authorize commits,
branch changes, merge, push or deployment.

## Diagnosis and selected correction

### FIND-TASK-001-1 — unattended startup still asks the host question

REQ-001/AC-002 prohibit prompts with `--non-interactive`. Resolution computes
the correct policy, but `crates/wyrd/wyrd-cli/src/server/dev.rs:527` calls
`mcp::offer_local` for a fresh deployment without applying that policy. The
existing owner at `src/mcp/mod.rs:224-245` sees terminal input and detected
hosts and awaits a selection. A complete unattended invocation on a tty can
therefore stall after provisioning. Existing piped-input tests miss it.

Apply the already-resolved interactive policy at the dev orchestration
boundary that owns the flag. Implicit host offers require that policy;
explicit `--host` selections must continue installing without questions,
including on restarts. Reuse the current host offer/installer. Preserve
standalone `mcp install` behavior; no second prompt or host-install owner.

### FIND-TASK-001-2 — saved identity conflicts are detected after issuance

REQ-002/008 and INV-004 require unrelated-deployment refusal and safe retention
before one-time credential issuance. At `dev.rs:295-302`, saved-key checks
depend on a saved dev slug and selected-origin login. They miss an unrelated
remote client with no dev section and a saved local key aimed at another
database with the same slug. At `:507-511`, startup saves new DSNs/dev selection
then provisions. A fresh database returns a new key, which correctly passes
its own tenant-prefix check. `GlobalConfig::save_local` at
`crates/shared/wyrd-client/src/global_config.rs:232-249` then refuses replacing
the retained old key. The new key is dropped, the new tenant remains active,
and bare restart targets the new database with the old key. Without a saved
key, an unrelated selected endpoint can instead be silently replaced.

Complete identity preflight at the existing resolution/startup owners. Refuse
a positively unrelated saved client selection before startup or local writes,
with existing `WYRD_CONFIG_HOME` separation guidance. For a saved local key
whose selected database identity needs server evidence, authenticate through
the existing shared client against the selected ready owned server, bound to
the expected saved tenant, before replacing local dev/DSN selection or creating
a tenant. An unrecognized key is not authority to provision a replacement.

Reuse current client authentication, protected settings and server setup;
retain `save_local`'s final conflict protection for races. Preserve first-use
protected persistence before issuance, valid same-deployment listener/DSN or
password overrides, missing endpoints after partial key persistence, and
existing explicitly authorized recovery for missing/revoked local keys.
Do not equate raw URL/DSN strings with deployment identity, reject every
override, infer authority from a slug, or automatically rotate a key. Correct
the producer boundary once rather than adding per-consumer guards.

### FIND-TASK-001-3 — supplied sealing identity disappears on restart

REQ-003/008 and AC-006 require bare restart without exports and continued
provider decryption. `dev.rs:941-949` returns no local key selection when
`WYRD_SEALING_KEY_FILE` or `WYRD_SEALING_KEY_BASE64` is supplied; children
inherit that key, but saved dev inputs retain neither the effective identity
nor its bytes. Removing exports selects/generates a different default key.
Boot may only log the rewrap failure and continue; subsequent managed-secret
resolution fails. Generated-default-key restart proof does not cover this.

Retain the validated effective supplied key through the existing protected
local sealing-key file owner and restore exactly that identity without exports.
Use existing secret-file/config validation before retaining it. Keep key bytes
out of general configuration, argv, diagnostics and generated documents.
Preserve explicit override precedence, conflict handling and existing rewrap
semantics. Invalid or failed selections must not overwrite the retained valid
identity. Do not add a key store, encryption mechanism or implicit rotation.

### FIND-TASK-001-4 — uncertain gateway errors falsely promise rollback

REQ-006/007 and INV-004 explicitly distinguish transaction refusal from a lost
response after commit. `dev.rs:1059-1063` prints “nothing was applied” for every
error from the existing shared HTTP apply. The server commits before returning
its response (`components/gateway/service.rs:386`); response loss/decoding or
commit-acknowledgement failure can leave a committed mutation. Setup and
troubleshooting guidance and the collection route's 503 description repeat
the categorical guarantee. Existing refusal tests cannot establish it.

Correct generic failure guidance at the existing reporting/documentation
boundary: startup did not complete, tenant setup remains, gateway changes may
have committed, and inspection or reapplying the same named document is safe.
Retain definite precommit validation/refusal guarantees only where justified.
Preserve structured errors, owned-child settlement, transaction atomicity and
named upserts. No receipt, automatic compensation, retry framework, new status
API or error hierarchy is needed.

Affected descriptions: `docs/src/content/docs/get-started/setup.md` gateway
failure row, `docs/src/content/docs/operate/troubleshoot-a-deployment.md`
gateway row, and `crates/wyrd/wyrd-server/src/components/gateway/routes.rs`
collection-operation availability description; align operation rustdoc too.

### FIND-TASK-001-5 — initial concealed input precedes signal handling

REQ-001/003 and AC-007 require usable concealed input and controlled startup
interruption. `DevArgs::run` at `dev.rs:131-135` awaits resolution before
polling the existing interruption owner. At the first missing-DSN question,
`terminal.rs:90-97` disables echo. Default INT/TERM process termination bypasses
`EchoGuard::drop` (`:28-32`), so the invoking terminal can remain without echo.
Later cancellable prompts do not prove this earlier path.

Extend the existing cancellation boundary over input resolution, observing
interruption before concealed input can disable echo and dropping pending
input on interruption. Reuse `interruption`, `Terminal` and `EchoGuard`;
preserve unattended missing-input refusal and later child cleanup. No separate
signal subsystem or terminal framework.

### FIND-TASK-001-6 — required SQL-aborted rollback proof is absent

AC-004/task Scenario 2 require SQL-failure rollback after an earlier mutation.
`components/gateway/pg_administration_tests.rs:1495-1706` forces application
validation refusals, not a database-aborted composite transaction. Its
name-only baseline also cannot establish that an existing rotated value was
restored. CLI/MCP missing-reference cases have the same limit. This finding
does not claim the current transaction implementation is broken.

Extend the existing Postgres apply proof using its real referential authority:
first rotate an existing managed credential, then attempt a provider change
on another already-referenced credential. Ensure the document passes shape
validation so the later update reaches the existing tenant/name/provider FK
with `ON UPDATE RESTRICT`. Require the existing SQL conflict and prove the
earlier envelope, metadata and usable original secret remain unchanged, along
with the referenced pair and unrelated resources. Reuse current service,
stored-credential and resolution fixtures; no failure-injection framework or
production redesign.

### FIND-TASK-001-7 — mandatory test rustdoc is incomplete

AGENTS §16 includes changed private test items and requires panic contracts.
The following validated items lack required documentation:

- `crates/wyrd/wyrd-cli/tests/gateway_server_journey.rs`: `gateway` at 14,
  `gateway_env` at 20, `deployment_document_applies_local_environment` at 463,
  and its `VARIABLE`/`SECRET` constants at 464-465.
- `crates/wyrd/wyrd-server/src/components/gateway/pg_administration_tests.rs`:
  `apply_document` at 1481 and `SUBMITTED` at 1496.
- `crates/wyrd/wyrd-mcp/tests/bifrost/mcp/gateway.rs`: `APPLIED` at 97.

Add concrete panic conditions to the four affected functions and meaningful
test-role docs to the four constants. Batch this with substantive fixes;
preserve behavior and assertions, and do not document unrelated untouched code.

### FIND-TASK-001-8 — mandatory restart proof depends on Linux procfs

INV-006/AC-006 and the task's portable-proof constraint apply to the new P8
journey. At `crates/wyrd/wyrd-cli/tests/pg_tests.rs:701` it calls `LocalDev::new`,
which selects all three listeners using `free_bind` at 310-330. That shared
helper unconditionally reads `/proc/sys/net/ipv4/ip_local_port_range` and
panics on macOS before testing the CLI. The helper predates this change, but
the newly required proof reaches it. No production OS regression was found.

Remove procfs dependence at the existing shared listener-selection fixture.
Use ordinary standard-library loopback binding/address discovery, already
used by the Python installed fixture. Preserve distinct selected listeners,
saved addresses across restart, child cleanup and every behavior assertion.
Retire obsolete range-specific imports/docs. The fixture already releases
sockets before startup; do not promise permanent reservation or add OS
branches, target exclusions, timing retries, production listener inheritance,
dependencies or a parallel harness.

## Constraints and preserved behavior

Keep all durable setup, tenant authority, sealing, transaction, audit and SQL
behavior with their current owners. Preserve gateway optionality, two-step and
single/array documents, server environment/external/managed sources, redaction,
SDK read/invoke behavior and absence of SDK secret mutation. Preserve unrelated
configuration, data and concurrent edits. No data reset, implicit rotation,
new recovery protocol, auth bypass, new persistent store, cross-platform
exception, test suppression or weakened check. Sibling example edits and
unrelated repository cleanup are outside this remediation.

## Acceptance and focused proof

| Finding | Required observable closure |
|---|---|
| 1 | Complete unattended setup on a tty with a detected host reaches ready without a question or implicit host-file change; explicit hosts still install. |
| 2 | Remote saved selection with/without a key and no dev section refuses before mutation; selecting database B after setup A neither changes A's files nor issues B a tenant key; A resumes. Valid same-deployment overrides, partial saves and authorized recovery remain usable. |
| 3 | Both supported supplied sealing sources survive a no-export cross-directory restart; an existing managed deployment invokes the local provider without reapply/rotation; tenant key and secret boundaries remain. |
| 4 | After a real committed apply loses its response, setup fails and settles its child, reports uncertainty, retains readable committed resources and supports safe reapply. Definite refusals still prove rollback. |
| 5 | INT and TERM at the initial hidden DSN question give controlled exit and restored PTY echo with no secret disclosure or install/migrate/provision effects. |
| 6 | A real later SQL FK rejection rolls back an earlier existing-secret rotation, preserving envelope/metadata/original usable value and unrelated resources. |
| 7 | Every identified changed item meets the mandatory rustdoc/panic contract without changed test behavior. |
| 8 | The new restart journey and shared fixture contain no procfs dependency and exercise the same assertions on supported macOS/Linux; unavailable platform execution is recorded honestly. |

Extend existing focused proof rather than adding a harness. All commands below
are required future execution by the implementer, not results of this review.
For new individually selected tests, record and run their actual exact selectors;
zero selected tests is not proof. Run Cargo-backed commands sequentially.

Build the touched installed binaries before installed-CLI proof:

```sh
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests
```

Use these existing exact proof entry points for the extended cases:

```sh
# Real terminal: unattended host policy and interruption at concealed input.
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup

# Identity preservation, partial saves, explicit recovery and shared fixture regressions.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'

# Supplied sealing identity and decryption; extend the existing retained-key proof.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_keeps_the_gateway_managed_secret_key)'

# SQL-aborted rollback after an earlier real credential rotation.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped)'

# Installed gateway: response-loss reporting, durable readback and safe reapply.
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup

# Served collection description remains aligned with the actual operation.
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::gateway_operations_publish_problem_json_refusals)'
```

Use existing language setup only when the focused consumer proof needs it.
For the touched source/docs write set, run `mise run fmt`, `mise run lints`,
`mise run docs:check`; if Python tests change, run `mise run py:format`,
`mise run py:lints`, `mise run py:typecheck` and `mise run py:test:unit`.
No generated schema shape, dependency or SQL capability boundary is prescribed;
run their existing narrow checks if the actual correction changes them.
No full journey suite, broad `gate` or external-example proof is required for
this remediation. Integrated broad journeys belong to change review.

Before handoff, run `git diff --check`, inspect the cumulative source and
record actual focused results, supported-platform execution limits and the
new candidate identity without modifying the approved spec. Re-review the
original base-to-candidate range, original task, this verdict and remediation
together; preserve all eight `FIND-*` IDs and prove each closure from source.

## Implementation evidence

Implemented with `$wyrd-implement` on `change/local-dev-setup`, range
`5cfb0a3ac..06ba0e96c` (10 commits). Platform executed: Linux x86_64 only.
macOS was not available to run; FIND-8 portability rests on the source: the
fixture now uses only `TcpListener::bind("127.0.0.1:0")`, and no new code
reads `/proc`, uses `cfg(target_os)`, or branches by OS.

| Finding | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 | `mcp::offer_local` takes the resolved `interactive` policy instead of probing stdin; `DevServer::run` passes `inputs.interactive`. Explicit `--host` still installs. | `test_installed_cli_guided_setup` (`unattended_terminal_offers_no_hosts`: tty plus `--non-interactive` reaches ready with no question and the host file byte-identical; restart with `--non-interactive --host codex` installs). Red before the fix: "completed local setup asked again". | PASS |
| 2 | `DevArgs::resolve` refuses a config directory with no `[dev]` tenant whose saved `[client] http_url` origin differs from the selected endpoint. `DevServer::run` authenticates any saved key with the existing `saved_client` (now addressed to the selected listeners), plus a bounded read, against the ready selected server before `save_settings`/`provision`. `save_local` keeps its final conflict guard. | `server_dev_repeat_and_partial_failure_preserve_state`: database B with the same slug is refused ("does not accept the saved key"), A's `credentials.toml`/`config.toml` are byte-identical, B has `platform.tenants` count 0, the server is settled and A resumes. A remote selection with and without a key is refused before anything runs, files byte-preserved. Partial-save and lost-key recovery assertions in the same test still pass. `server_dev_saved_settings_survive_working_directory_change`: a same-deployment `--bind` override keeps the key and resaves the endpoint. | PASS |
| 3 | `local_sealing_key` validates a supplied `WYRD_SEALING_KEY_FILE` (through the shared `wyrd_utils::secret::read_secret_file` owner-only rule) or `WYRD_SEALING_KEY_BASE64` (base64 of 32 bytes, as the server decodes it) and retains it as the existing owner-only `server/sealing.key` with no-clobber staging. A later different supplied key is refused and never overwrites the retained key. Every child receives the retained file and `WYRD_SEALING_KEY_BASE64` is removed. | `server_dev_keeps_the_gateway_managed_secret_key`, run for generated, file and inline sources: seal, stop, bare restart from another directory with no sealing variable, local-provider invocation succeeds (6 mock calls), `credentials.toml` unchanged, the different key is refused with the retained file byte-identical, and neither the sealing key nor the secret appears in output. | PASS |
| 4 | `configure_gateway` reports that setup did not complete, that a refusal applies nothing but a lost response may have committed, and that reapplying the same named document is safe. Aligned: `setup.md` and `troubleshoot-a-deployment.md` rows, the collection route's 503 description, and the operation and `configure_gateway` rustdoc. | `test_installed_cli_gateway_setup`: a forwarding proxy that drops only the composite apply's answer makes startup fail (not 0 or 130) with "may have committed" and never "nothing was applied", the listener is settled, the committed `lost-answer` deployment reads back on restart, and reapplying succeeds. Served-document check: `pg_openapi_contract::gateway_operations_publish_problem_json_refusals`. | PASS |
| 5 | `DevArgs::run` polls `interruption()` (biased) over `resolve`, so the signal listeners register before any question and dropping the pending `conceal` future runs `EchoGuard::drop`. | `test_installed_cli_guided_setup` (`interrupt_hidden_question` for SIGINT and SIGTERM): exit 130, slave termios `ECHO` restored, the typed text is never shown, and no config, credentials or sealing key exist afterwards. Red before the fix: the process died from the signal (`-2`). | PASS |
| 6 | Test only: after the valid commit, a document rotates `managed-key` and then moves the referenced `shared` credential to another provider, which trips the deployments' FK (`ON UPDATE RESTRICT`). | `pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped`: `GatewayResourceConflict`; the `managed-key` row (envelope and metadata) and `shared` row are byte-identical; deployments are unchanged; the original envelope still opens to the original value; the audit sequence gains one `allowed`. | PASS |
| 7 | Rustdoc and panic contracts for `gateway`, `gateway_env`, `deployment_document_applies_local_environment`, `apply_document`; role docs for `VARIABLE`, `SECRET`, `SUBMITTED`, `APPLIED` (and new `ROTATED`). | `mise run lints`, `mise run fmt`; no behavior change. | PASS |
| 8 | `free_bind` uses standard-library loopback `bind(:0)`; procfs read and the atomic imports removed. | P2, P8, P9 and the selected-host and managed-key selectors pass on Linux. macOS not executed (stated above). | PASS |

Commands run sequentially, all passing:

```sh
mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server -p wyrd-monty-worker --bins --tests
mise exec -- scripts/postgres/with-test-postgres.sh -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_guided_setup
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_saved_settings_survive_working_directory_change)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_keeps_the_gateway_managed_secret_key)'
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::pg_administration_tests::gateway_apply_is_atomic_and_tenant_scoped)'
mise exec -- scripts/postgres/with-test-postgres.sh -- node scripts/check_ts_package.mjs mise exec -- uv run --directory sdks/wyrd-sdk-python python -m pytest -q -m local_dev tests/integration/test_local_development.py::test_installed_cli_gateway_setup
mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-server --test integration -E 'test(=pg_openapi_contract::gateway_operations_publish_problem_json_refusals)'
mise run fmt; mise run lints; mise run docs:check; mise run check:deps
mise run py:format; mise run py:lints; mise run py:typecheck; mise run py:test:unit
git diff --check bc3bf25e..HEAD
```

### Diagnosis: guided setup summary assertion

- **Symptom:** `test_installed_cli_guided_setup` failed once at
  `assert b"configured deployments gemini-local" in shown`, although `shown`
  ended with the ready line.
- **Evidence:** `DevServer::run` prints the ready message in one multi-line
  `println!`. Rust's line-buffered stdout writes it one line at a time, and
  `converse` returned on the first chunk containing `Local Wyrd ready at `,
  before the `Gateway:` line had been read.
- **Cause:** the test's read loop stopped before the end of the message.
  The CLI had configured the deployment.
- **Fix site:** `converse` now waits for the message's final line,
  `Press Ctrl-C to stop.`. A fresh read-only diagnostician independently
  reached the same cause and fix. `converse` has one caller.
  `wait_for_local_cli` asserts nothing after the ready line and was left
  unchanged.

### Decisions and limits

- FIND-3: a supplied key that differs from an already-retained
  `sealing.key` is refused, not written over it. Overwriting would
  permanently destroy the key that existing managed secrets are sealed under.
  To adopt a different key, either unset the variables or deliberately remove
  the retained file.
- FIND-2: a refused database switch still runs idempotent migrations on the
  selected database before the key preflight, because the preflight needs
  that ready server. No tenant, key or local file is created or changed.
- The server logs a refused exchange's public key prefix (`wyrd_sk_<tenant>`).
  The new assertion checks that the secret itself is absent rather than the
  prefix.
- `wyrd-cli` gains a direct dependency on `wyrd-utils`. It was already
  present transitively through `wyrd-client`, and `check:deps` passes.
- Non-goals: no spec change, sibling example edits, new store, recovery
  protocol, OS branch or weakened check. No unrelated file changed.
