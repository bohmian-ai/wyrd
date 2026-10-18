---
id: TASK-002
kind: implementation
status: proposed
spec: SPEC-agent-readiness
spec_revision: 2
requirements: [REQ-006, REQ-010, INV-001, INV-002, INV-003, INV-006, AC-007, AC-010, AC-011]
depends_on: [TASK-001]
---

# Make local tooling machine-readable without changing server lifetime

## Outcome and Value

An unattended caller can parse setup and final Card-lifecycle results/errors
without scraping prose. A person or harness keeps foreground `server dev`
alive and receives one JSON readiness result when ready. The repository's
platform rule agrees with its three supported distributions.

Execute with `$wyrd-implement`. TASK-001 must land first because this task
changes the same CLI dispatch/error/test owners and must exercise declare,
get and pull under their final names. TASK-003 documents the resulting
contract; no UI write set or `wyrd-ui` work is required here.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Result/error projection | CLI `card.rs::OutputFormat`, `lib.rs::run_cli_code`, `error.rs::CliBoundaryError::print` and command dispatch | Card lifecycle JSON/error helpers; native Python/TS CLI projections | Setup lacks format; error prints JSON plus prose; Clap bypasses structured failures | Apply the approved result/error-stream contract through existing CLI boundary | No new public error catalog or transport; use derive-backed metadata |
| Foreground readiness | `server/dev.rs::{DevArgs,DevServer}` existing install/migrate/setup/save/readiness/wait path | `pg_tests::server_dev_first_run_is_client_ready` and partial-failure journey | No machine readiness result while serving | Render one JSON readiness result at the existing proven-ready boundary and retain foreground wait/cleanup | No alternate startup owner, detached mode or stop command |
| Installer and host report | `ServerCommand::dispatch`, `ServerInstaller`; MCP `McpInstallReport`, `install`, proxy | Mock signed-release installer; selected-host install and real proxy journeys | Prose-only setup summaries | Project existing typed results as JSON, retaining noninteractive/refusal and credential behavior | Existing installer, host editing and shared client already own complete workflows |
| Supported target authority | `AGENTS.md` §2; `ServerInstaller` target selection; TS package native targets | Installer target/refusal source and package target inventory | Authority promises Intel macOS although current distributions have three targets | Correct normative target set and prove unchanged portable workflow | Static authority reconciliation; no platform expansion or new process mechanism |

Own common CLI output, argument failures, secret-safe diagnostic routing and
the necessary public CLI projections. The exact required command set is
REQ-006: server install/dev, mcp install, declare/check, get/pull, with existing
adjacent Card JSON retained. Report readiness before foreground termination;
a later server failure remains a structured failure with nonzero exit.
Do not change provisioning, persistence, authentication, server durability or
credential resolution to make formatting easier.

Update `AGENTS.md`'s platform rule and any other current normative claim of
four supported targets, preserving every portability prohibition. This is
the spec-authorized target correction, not permission to edit the approved
spec or the local-dev change packet. TASK-003 owns user-facing lifetime docs.

No detached mode, new server stop/lifecycle API, process-manager state,
server-aware check, production supervisor, credential in argv/host config,
MCP declaration/upload/check, sibling repo edits or UI work. Preserve other
contributors' changes; do not revert or reformat unrelated files.

## Approach

1. Add focused failures for final-command JSON, argument errors and setup
   reports using the current CLI boundary and actual installed process.
2. Extend existing output/error projection and diagnostic routing consistently,
   preserving public Wyrd metadata and native CLI embedding exit behavior.
3. Emit readiness from the current foreground ready boundary; expose installer
   and selected-host reports without altering setup/host workflows.
4. Reconcile the three-target authority and affected setup/platform assertions.
5. Prove focused runtime/contract behavior, regenerate touched outputs and
   record the diff/acceptance evidence.

## Ordered Implementation Scenarios

### Scenario 1 — Final commands have parseable success and failure streams

**Behavior.** JSON successes have one typed stdout result. Operational,
authentication/authorization, local validation and usage failures have one
stderr error document with stable code, detail, remediation and nonzero exit,
without prose/progress corruption. No configured credential is printed
(REQ-006, AC-007).

**RED.** P1 is a planned focused test in the inspected CLI integration owner;
drive the compiled binary for malformed/missing arguments, offline check
failure and real denied/read operations, plus a successful final lifecycle
command. Baseline Clap/error printing breaks the complete-document assertions.
Keep P2's prior lifecycle/refusal assertions while extending its output checks.

**GREEN.** Extend the existing formatter/parser boundary and required command
formats; preserve originating catalog metadata, typed local diagnostics and
exit codes. Rerun P1/P2 and TASK-001's affected local-check assertions.

**REFACTOR.** Consolidate repeated projection at the existing boundary; do not
invent command-specific problem shapes or suppress diagnostics/errors.

### Scenario 2 — A foreground server reports readiness before it exits

**Behavior.** `server dev --non-interactive --format json` emits one readiness
result after actual authenticated readiness and remains in the foreground.
Missing inputs fail before side effects, startup-stage errors are structured,
and normal interruption/failed startup preserve existing cleanup/saved-state
behavior (REQ-006, INV-006, AC-007/010).

**RED.** Extend P3 with JSON readiness consumption while the process remains
alive, a fresh saved-config client and structured missing-input/stage errors.
Retain P4's recovery/credential assertions. Baseline format parsing/output
fails; unavailable release infrastructure is not a valid RED.

**GREEN.** Render at `DevServer`'s existing ready boundary and keep its wait,
interrupt and save/recovery logic. No process detachment or persistent process
identity is introduced. P3/P4 pass with the final Card names from TASK-001.

**REFACTOR.** Reuse current readiness and cleanup; do not introduce a second
bootstrap state machine or expand local-dev lifetime semantics.

### Scenario 3 — Installer and selected MCP hosts return safe typed reports

**Behavior.** Server install and MCP install support JSON; unattended host
selection is explicit, repeated install converges, unrelated host files stay
unchanged, and the installed proxy authenticates using saved state. Human
output remains usable (REQ-006, AC-007).

**RED.** Extend P5/P6 with JSON success and failure assertions, no-prompt
missing-host refusal and secret sentinels. The baseline lacks format support.
Keep P7 as real connection regression rather than manufacture a failing proxy.

**GREEN.** Render the existing installer/install reports without changing
signature/digest verification, compatibility, host selection or shared auth.
Pass P5/P6 and retain P7 and earlier scenarios.

**REFACTOR.** Reuse the report and host/proxy owners; remove duplicated prose
paths where the common projection already carries the same information.

## Acceptance Criteria

| Criterion | Spec obligations | Proof |
|---|---|---|
| Required final commands consistently project success, usage and domain errors in JSON | REQ-006, AC-007 | P1/P2, package projections and secret sentinel assertions |
| One actual ready result precedes foreground wait; refusal/recovery behavior remains | REQ-006, INV-003/006 | P3/P4 and fresh-client readback |
| Installation/host reports are typed, explicit, repeatable and credential-safe | REQ-006, AC-007 | P5-P7 |
| Normative target set is Linux x86_64, Linux arm64, macOS arm64, with portability bans intact | REQ-010, AC-010 | Static authority/package/installer review and focused runtime evidence on the supported hosts |
| No new lifetime, auth or error ownership boundary | INV-001/002/003/006, AC-011 contribution | Diff, contract generation and focused regression |

## Expected Write Set and Consumer Closure

- CLI parser/dispatcher/result/error boundary, Card output integration,
  server dev/install reports and MCP install/proxy consumers where necessary.
- Native CLI embedding/testing projections and generated errors only where
  their exposed contract is affected; no independent SDK transport changes.
- CLI unit/integration/installed-process tests and mock release/host fixtures;
  target authority in `AGENTS.md` and current normative platform descriptions.
- Existing mise test references only when necessary for touched selectors.
  Final onboarding prose, guide start and examples remain TASK-003's scope.

Paths guide the consumer closure, not private helper/test fixture structure.
No UI or other change packet/review edits are needed.

## Verification and Evidence

P1 is a planned addition under the inspected `card_lifecycle` module of the
existing `wyrd-cli` target `cli`. Its package/target/parent are confirmed; the
implementer owns fixture organization and confirms collection after authoring.
Other selectors below exist now; adjust and record exact collected equivalents
if TASK-001 renamed one. Never count an empty selection as passing proof.

| Proof | Exact focused command |
|---|---|
| P1 planned JSON success/failure stream journey | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable)'` |
| P2 lifecycle regression with final verbs | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::card_lifecycle_cli_journey)'` |
| P3 installed foreground readiness | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'` |
| P4 retained startup failure/recovery | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'` |
| P5 signed installer selection/failure | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_selects_latest_stable) or test(=server_install::server_install_preserves_previous_on_failure)'` |
| P6 selected-host installation | `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_install_changes_only_selected_hosts)'` |
| P7 saved-client MCP proxy read | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'` |

Before installed-server proof, use the current lane's prerequisite build:
`mise exec -- cargo build --locked -p wyrd-cli -p wyrd-server --bins --tests`.
The installed lifecycle fixture points an isolated version root at current
build binaries; P5 separately proves signed release selection/verification.
Do not replace the real CLI path with in-process calls or depend on published releases.

Run `mise run fmt` and `mise run lints`. When public error/schema sources
change, regenerate and run `mise run codegen:check`; run the owning Python/TS
format/lint/type checks only for actual binding changes. Platform wording is a
static obligation, not a reason to manufacture RED or add a new platform gate.
Record which supported hosts actually executed focused proof; do not present
cross-compilation or source review as execution on another host.

No full journey suite or broad aggregate in task verification; integrated
change review owns that proof once. Append acceptance/non-goal evidence,
actual command results and any corrected selectors. Diagnose unexpected
failures with tracing under `$wyrd-implement`. Finish with `git diff --check`
and tracked/untracked diff review; do not stage, commit or revert others.

## Material Stop Conditions

Stop for a required change to public error semantics beyond REQ-006,
credential/persistence/readiness guarantees, detached or production process
management, platform-specific prerequisites or additional target scope.
No such decision is currently open. Private output composition, safe log
routing, fixture organization and collected test names are implementation
choices. Report missing required external artifact/host evidence honestly;
do not weaken the supported-platform or installed-process proof.

## Authority Links

- [Approved spec revision 2](../spec.md) and [TASK-001](TASK-001-declare-get-pull.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Router](../../../../architecture/references/README.md), [spec workflow](../../../../architecture/references/languages/spec-driven-development.md),
  [execution](../../../../architecture/references/languages/implementation-execution.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Agent harness](../../../../architecture/references/languages/agent-harness.md), [errors](../../../../architecture/references/languages/errors.md),
  [security](../../../../architecture/wyrd-security-posture.md), [deployment operations](../../../../architecture/operations/deployment-and-release.md).

## Implementation Evidence

Executed on Linux x86_64 only. Linux arm64 and macOS arm64 were not executed
here; their agreement rests on source review: the installer's `host_target`
lists exactly the three targets, and the release matrix runs macos-15 (arm64)
and ubuntu.

### Reuse and gap record

| Owner reused | Gap closed | Extension |
|---|---|---|
| `lib.rs::run_cli_code`, `CliBoundaryError::print` | Error printed JSON plus prose; Clap bypassed the catalog | `json_requested` scans raw argv once; JSON mode prints one stderr document; Clap failures become `WYRD_CLI_400_USAGE` (exit 64) |
| `WyrdCliError` derive catalog | Local errors carried no details | `WyrdCliError::details`: loader diagnostics, field, startup stage plus log, MCP host outcomes, usage kind/argument |
| `card.rs::OutputFormat`, `print_json` | Setup commands lacked `--format`; a failed check wrote `ok:false` to stdout | Reused on `server install`, `server dev`, `mcp install`; JSON check failures leave stdout empty |
| `DevServer` ready boundary | No machine readiness | One single-line `DevReady` after readiness and the fresh-client proof; the existing foreground wait, interrupt, and stop are unchanged |
| `DevServer` output | Progress and the child's output shared the machine streams | `Diagnostics` appends to `<server root>/logs/server-dev.log` in JSON mode; stage errors and readiness name it |
| `McpInstallReport` | Prose only | JSON report; failures carry it as `details.hosts`; JSON never prompts |
| `main.rs` tracing | Logs on stderr | Default filter is `off` in JSON mode unless `RUST_LOG` is set |

### Acceptance

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Required final commands project success, usage and domain errors in JSON | `lib.rs::{json_requested,run_cli_code}`, `error.rs::{WyrdCliError::{usage,details},CliBoundaryError::print}`, `card.rs::dispatch_check` | P1 (usage, offline check, no credentials, 403, declare/get/pull, token sentinel), P2, lib `tests::json_mode_is_read_from_raw_arguments` | PASS |
| One ready result precedes the foreground wait; refusal and recovery retained | `server/dev.rs::{DevReady,Diagnostics,DevServer::run,DevArgs::run}` | P3 (missing input with no side effect, occupied listener `stage`+`log`, single readiness line, served fresh-client writes/reads, stdout/stderr hold only that line, log content without secrets), P4 | PASS |
| Installer and host reports are typed, explicit, repeatable, credential-safe | `server/mod.rs::InstallArgs`, `install.rs::InstalledServer: Serialize`, `mcp/mod.rs::{InstallArgs::run,McpInstallReport::{failed,print_text,into_result},offer_local}` | P5 (result shape; real binary JSON 503 refusal through an unreachable proxy), P6 (JSON conflict details, no-prompt refusal, repeat `unchanged`, sentinel), P7 | PASS |
| Normative targets are Linux x86_64, Linux arm64 and macOS arm64, with portability bans intact | `AGENTS.md` §2; installer `host_target` unchanged (already three) | Static review: `git grep` finds no other current four-target claim | PASS (Linux x86_64 executed only) |
| No new lifetime, auth or error ownership boundary | Same catalog derive; no detach/stop API; credential resolution untouched | `mise run codegen:check`, `mise run check:deps`, diff review | PASS |

### Commands

- P1: `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=card_lifecycle::pg_tests::json_results_and_errors_are_machine_readable)'` — 1 passed
- P2: `… -E 'test(=card_lifecycle::pg_tests::card_lifecycle_cli_journey)'` — 1 passed
- P3: `… -E 'test(=pg_tests::server_dev_first_run_is_client_ready)'` — 1 passed
- P4: `… -E 'test(=pg_tests::server_dev_repeat_and_partial_failure_preserve_state)'` — 1 passed
- P5: `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=server_install::server_install_selects_latest_stable) or test(=server_install::server_install_preserves_previous_on_failure)'` — 2 passed
- P6: `mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=mcp_install_changes_only_selected_hosts)'` — 1 passed
- P7: `… -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'` — 1 passed
- Retained TASK-001 checks rewritten to read diagnostics from the error document; all passed:
  - `card_lifecycle::invalid_local_reference_returns_card_load_error_without_credentials`
  - `declare_check_validates_disk_references_without_contacting_a_configured_server`
  - `declare_check_rejects_verifier_service_component_with_stable_diagnostic`
  - `declare_check_rejects_unbound_verifier_peer_with_stable_diagnostic`
  - `pg_tests::cli_enforces_the_verifier_card_contract`
- Supporting lanes:
  - `mise exec -- cargo nextest run --locked -p wyrd-cli --lib` — 71 passed
  - `mise run fmt` and `mise run lints` — clean
  - `mise run docs:generate` (adds `WYRD_CLI_400_USAGE` to `errors.md`) — done
  - `mise run codegen:check`, `mise run docs:check` and `mise run check:deps` — passed
  - `git diff --check` — clean
- No Python or TypeScript binding changed. Both embed `run_cli_code`, so they inherit the contract; no SDK test depends on stream shape.

### Non-goals

- Excluded:
  - detached mode or a stop/lifecycle API;
  - process-manager state;
  - a server-aware check;
  - credentials in argv or host config;
  - MCP declaration, upload or check;
  - UI;
  - sibling repositories;
  - edits to other change packets.
- Text mode keeps its existing JSON-line-plus-prose stderr.
- Limit: an interrupt (exit 130) emits no error document; it is caller-initiated termination, recorded in the diagnostic log.
