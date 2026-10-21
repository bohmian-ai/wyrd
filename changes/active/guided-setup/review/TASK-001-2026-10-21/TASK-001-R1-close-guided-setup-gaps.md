---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-guided-setup
spec_revision: 6
requirements: [REQ-002, REQ-021, REQ-022, REQ-023, REQ-027, REQ-037, REQ-038, INV-008, AC-003, AC-014, AC-015, AC-020]
depends_on: [TASK-001]
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5]
---

# Close guided setup and ASGI acceptance gaps

## Outcome and authority

Complete the existing guided setup and ASGI implementation: application teardown errors still attempt final evidence drain; Wyrd-created diagnostics do not disclose arbitrary exception bodies; Manual users can confirm persisted results and identify their Run; connected supported hosts receive their prompt command. Add the separately approved `wyrd init guided` mode to launch the selected coding agent after readiness and keep Wyrd serving afterward. Batch three required Rust documentation corrections.

Authority: [approved specification revision 6](../../spec.md), [original TASK-001](../../tasks/TASK-001-guided-setup-and-asgi.md), [revision-5 review verdict](verdict.md), and [independent finding evidence](findings-validation.md). Repository root is `/home/thorrester/Documents/GitHub/wyrd-worktrees/guided-setup`. Original review base: `ebd75dc224854413d17cb06b70da8181b78cde6b`; candidate: `32fbc264adccaf1160894769d5501fbe441259a5`. This remediation adds to that candidate; later review retains the original full cumulative range and applies the approved revision-6 scope delta. Original task/evidence and review reports remain the historical revision-5 subject, rather than being rewritten as if they required guided launch.

Route to `$wyrd-implement`. The maintainer approved the guided-mode scope revision on 2026-10-21. This packet combines the original five validated corrections with that explicit human-directed addition; it assigns no new FIND ID to the addition and authorizes no public surface beyond revision 6 or change to native state lifecycle behavior.

## Diagnosis and selected corrections

### FIND-TASK-001-1: final drain on an established lifespan's abnormal exit

At `sdks/wyrd-sdk-python/python/wyrd/integrations/asgi/_middleware.py:119–162`, startup succeeds, the app sends startup.complete, then app shutdown can queue evidence and raise before a terminal reply. The `replied` branch at 145–146 immediately reraises; only terminal-message paths call `_drain`. Public middleware and installed Uvicorn probes show startup/final-work events with no state shutdown call while server shutdown finishes. Existing C5 uses successful/explicit failure messages and misses this path. Run exit restores correlation; it cannot drain buffered rows.

Use the existing off-loop `_drain` and `WyrdState.shutdown` boundary for terminal cleanup after an established wrapped lifespan exits abnormally. The existing native state remains the sole provider flush/writer drain owner. Keep application teardown before drain, preserve primary exception/cancellation, and do not drain again when an explicit terminal-message path already attempted it. A secondary drain failure must remain safely observable without replacing the original application failure. An unanswered failed phase must not become success; avoid duplicate terminal replies when the app already sent one.

Cover a controlled cancellation while an established app is awaiting shutdown with the same cleanup attempt and original cancellation propagation. Do not add shielding beyond the host budget, retries, a supervisor, new timeout policy, or promises for repeated cancellation, cancellation during startup's worker thread, timeout or hard kill. Preserve standard unsupported-lifespan behavior; the rejected pre-first-receive proposal is not part of this task.

### FIND-TASK-001-2: safe Wyrd-created lifecycle diagnostics

`_middleware.py:25–29` formats arbitrary exception bodies; callers at 107, 115–116, 125 and 151 put the result into cleanup/drain logs and generated failure responses. An actual middleware probe disclosed synthetic DSN/token text and a newline. REQ-022 requires a scrubbed diagnostic; existing C5 checks only benign error text.

Correct the existing diagnostic projection once. Retain canonical Wyrd code/safe public fields; use static phase/classification information for unexpected exceptions instead of arbitrary `str(error)` or traceback content. Preserve the specific actionable missing-extra diagnostic already owned by `wyrd.otel._EXPORTER_EXTRA` and its import guards. Do not classify every ImportError as that known condition, introduce a blacklist/redaction framework or add an error catalog. Keep primary application exception propagation. Explicit application-owned startup.failed/shutdown.failed messages remain their original messages and failures; rewriting them was rejected as adjacent scope.

### FIND-TASK-001-3: publish and test Manual completion

`docs/src/content/docs/get-started/setup.md:137–139` sends Manual users only to Quickstart. The complete Quickstart procedure prints synchronous verdicts but never retrieves persisted evidence or identifies its Run. Meanwhile `test_local_development.py:1229–1300` reads SQL from the agent prompt and silently supplies that operation through `stored_verdicts`. Thus green C4 proves APIs while leaving the promised manual operation unpublished. Generated llms content repeats the omission.

Complete the existing published manual path with one language-neutral readback after the existing three language examples: existing Card lookup/result query, matching passed/failed results under one Run, and Run identification, with publication-delay/failure guidance. Reuse current MCP read/query surfaces and the setup prompt's SQL semantics. Make it directly reachable from Manual, and update C4 to perform/check the published completion operation rather than an operation documented only in a test helper. Follow docs source-example rules and regenerate projections. Add no setup executor, SDK loader, new CLI contract or unrelated tutorial/deployment rewrite.

### FIND-TASK-001-4: complete existing host launch branches

`crates/wyrd/wyrd-cli/src/mcp/hosts.rs:93–101` returns no command for five existing host variants. Connected outcomes reach `McpInstallReport::print_handoff` (`mcp/mod.rs:239–260`) and receive paste fallback despite supported prompt syntax. Current tests cover Codex/Claude and incorrectly pin Cursor as unsupported. REQ-002 requires commands for every connected host that accepts an initial CLI prompt.

Extend the existing host match using its existing literal single-argument quoting and documented prefixes: Copilot `copilot -i`, Pi `pi`, VS Code `code chat`, Cursor `agent`, Hermes `hermes chat -q`. Keep existing Codex/Claude commands. The validator checked all seven host/config consumers and current official references; links and compatibility limits are in [FIND-TASK-001-4](findings-validation.md#find-task-001-4--revised--missing-host-launch-match-omits-supported-initial-prompt-commands). REQ-002 does not require an interactive-only session, so the Hermes query mode qualifies. Cursor CLI shares the already configured editor MCP settings; no separate host/config is needed.

For ordinary init preserve output-only handoff, current-directory operation and foreground supervision. Keep host launch syntax at the existing host owner; use executable plus argument values for the approved guided invocation rather than executing its printed shell string. Printed commands and guided launch must project the same host syntax and literal prompt. Do not introduce version probes, new host kinds, configuration or a capability registry. Executable availability checking is required only for the separately approved guided selection. Retain ordinary-init fallback for genuinely unsupported launch commands; do not designate a supported variant unsupported to exercise it.

### FIND-TASK-001-5: batch required Rust item documentation

The explicit changed-item rustdoc gate remains unmet at three inspected sites:

- `crates/wyrd-spec/src/envelope.rs:333`, changed `Spec::json_schema`: document its union/schema-consumer contract on the method; impl-level prose does not document the item.
- `crates/wyrd-spec/src/mcp.rs:81`, new tests module: add accurate module rustdoc.
- `crates/wyrd/wyrd-cli/tests/cli.rs:111`, new `walk`: document directory/entry/file-read panic conditions under `# Panics`, and correct the same item's full joined-path description.

Keep all behavior unchanged for this finding. No new runtime test, check, abstraction or source-wide documentation sweep is warranted. Source inspection closes the documentation property; batch with the behavioral fixes.

## Human-directed addition: `wyrd init guided`

This section implements REQ-037–038/AC-020, not a newly discovered defect. The existing `InitArgs::run` owns bootstrap resolution and signals; `DevServer::run` owns startup/readiness, saved identity, gateway, host setup and foreground child supervision. `McpHost` owns host-specific launch syntax, `HostInstaller` owns repeatable connection configuration, and `wyrd_spec::mcp::setup_prompt` owns release-labelled text. Extend that complete workflow, not a second bootstrap, supervisor, shell launcher or prompt copy. `HostInstaller::detected` currently detects configuration locations, which alone cannot prove an executable is available. Existing launch output is a shell-display string and cannot safely be repurposed as an executable command.

The approved command launches exactly one user-selected installed agent after Wyrd and that host connection are ready. Reuse the existing `--host` selector; reject multiple explicit hosts, otherwise let the user choose one usable agent. Do not infer selection from the first detected configuration or silently substitute another agent. Require interactive terminal IO and reject JSON/prompt-only conflicts before bootstrap side effects. Other inputs, saved credentials, gateway and local data remain under the existing owner. Launch with the current project directory and literal shared prompt argument, using the host's ordinary permissions and approvals.

Agent completion/nonzero exit or failed launch must be visible while the ready server remains serving until interrupted; exit alone is not evidence of successful verification. Keep supervising server/UI while terminal IO belongs to the agent. On interruption or unexpected server/UI exit, stop and reap the agent child Wyrd started before existing bounded server/UI settlement, leaving a usable terminal. Do not kill pre-existing editor processes or claim ownership of detached host sessions. This adds no daemon, agent runtime, auth path or automatic approval.

Update CLI help, the Setup agent-assisted tab and generated docs to teach `wyrd init guided`; retain ordinary-init printed handoff, manual path and offline `--print-prompt`. Update the local-CLI operations authority with this explicit opt-in child ownership while preserving the doctrine that Wyrd is not an application/agent runtime. TASK-002's identity work remains unchanged.

### Scenario 6 — Explicit agent selection and launch

**Behavior.** Guided mode launches exactly the chosen installed host after server/host readiness, with the exact release prompt, project directory and usable terminal IO. Missing/unsupported host, failed connection, multiple selections and nonterminal/JSON/prompt-only invocation refuse explicitly without another agent being launched. Ordinary init still launches none.

**RED.** Extend the existing C2 installed-CLI journey to exercise guided selection/launch with controlled host executables and terminal IO. Confirm the missing guided command/launch fails the expected behavioral assertion, not an unavailable paid host or environment prerequisite. Run C1 for retained parser/offline behavior; if a new named test is needed, record and confirm its exact selector before RED.

**GREEN.** Extend existing bootstrap/host owners and their consumers to perform the approved invocation, with existing MCP authentication and one prompt source. Update help and Setup examples. Rerun the host literal proof, C1 and C2.

**REFACTOR.** Keep one host invocation representation for safe argument launch and shell-display projection; no shell evaluation, duplicate host registry or second bootstrap workflow.

### Scenario 7 — Agent and server lifecycle

**Behavior.** Controlled host completion/nonzero exit or failed spawn is reported and leaves the ready server available. Interruption and unexpected server/UI exit stop/reap Wyrd's launched agent child and settle existing server/UI children, retaining terminal usability and saved setup. Agent exit does not claim verification success.

**RED.** Extend C2 with controlled host lifetimes, real readiness checks and terminal/process observations for each outcome. Missing child cleanup, loss of foreground server or unusable terminal must fail assertions. Never use a paid live-agent session or model output as proof.

**GREEN.** Extend the existing signal/child supervision boundary with the one launched agent while preserving existing bounded server/UI shutdown. Rerun scenarios 6 and 7 and the retained C2 bootstrap assertions.

**REFACTOR.** Retain existing foreground ownership and shutdown budgets; add no daemon, detached supervisor, approval bypass or new agent engine.

## Preserved behavior and scope

Keep existing bootstrap settings, credentials, gateway, ordinary-init JSON stdout and child supervision; one prompt source/release projection; application-owned telemetry provider and explicit failure messages; Run/alias/request context; worker-local resources and native bounded lifecycle. Guided mode alone adds the approved selected-agent child. Never add another writer/provider/transport/registry or HTTP instrumentation. No auth/deployment identity or TASK-002 edits. No OpenAPI oneOf refactor, automatic framework discovery or hard-kill delivery guarantee. Fix private mechanics within these boundaries; do not prescribe new helpers or fixture structure.

## Ordered scenarios and acceptance

Run one behavioral scenario RED → minimum GREEN → refactor against previously green proof; record exact selected tests, expected missing-behavior failure and final results. Rustdoc uses static inspection, not manufactured RED.

| Scenario / finding | Required observation |
| --- | --- |
| 1. Abnormal established lifespan / FIND-TASK-001-1 | Generic app sends startup.complete, consumes shutdown, queues final evidence and raises before reply. Primary error remains observable, final off-loop drain is attempted, successful state closure retains queryable evidence; loop remains responsive. Secondary drain refusal does not replace primary error. Controlled established-lifespan cancellation attempts cleanup then propagates. Normal and explicit terminal-message paths drain once after app teardown. |
| 2. Safe diagnostics / FIND-TASK-001-2 | Synthetic DSN/token/newline bodies in startup and cleanup/drain faults do not enter Wyrd-created messages/logs. Wyrd code and actual missing-extra remedy remain useful. Explicit app failure text remains unchanged and failed. |
| 3. Manual completion / FIND-TASK-001-3 | Each existing language path executes the published completion operation against the real server, retrieving matching stored passed/failed results and their Run ID. A reader can locate and execute it from Manual. Generated docs reflect the instructions. |
| 4. Host launch / FIND-TASK-001-4 | Every existing supported host has its documented prefix with exactly preserved prompt argument bytes, including quotes/newlines/dollars/backticks. Ordinary-init connected-host journey asserts the resulting handoff without starting agents. |
| 5. Rustdoc / FIND-TASK-001-5 | All three exact changed items have accurate item docs, including helper panic conditions. No runtime/schema changes introduced for documentation. |
| 6. Explicit guided selection / human-directed REQ-037 | Controlled host executable observes exact prompt/argv/current directory and terminal IO only after real readiness and connection. Refusals launch no substitute; ordinary init launches none. Help, agent-assisted docs and generated output describe the command. |
| 7. Guided lifecycle / human-directed REQ-038 | Controlled agent exits leave server reachable and report status; failed launch is visible; interrupt/server failure settles Wyrd's launched child and existing server/UI without killing unrelated processes or leaving terminal unusable. |

## Focused proof and execution

Reuse the original environment preparation and repository-managed Postgres described in TASK-001. Every specifically named test below must actually be selected; any additional separately named fault test needs its exact selector recorded in completion evidence. Extend the existing lifespan driver minimally so raw task failure is observable rather than hanging while waiting only for a message. Do not introduce a second harness.

Host literal argument proof:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --lib -E 'test(=mcp::hosts::tests::launch_preserves_the_setup_prompt_literally)'
```

Connected handoff proof (C2):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-cli --test cli --run-ignored=all -E "test(=pg_tests::guided_init_lifecycle_and_prompt)"'
```

Parser/offline regression (C1), directly affected by the new command shape:

```bash
mise exec -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=guided_init_print_prompt_is_offline)'
```

Published manual operation, all existing language parameters (C4):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_local_development.py::test_guided_manual_setup'
```

Lifespan negative paths and directly affected provider-ownership regression (C5/C7):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && uv run --directory sdks/wyrd-sdk-python python -m pytest -q tests/integration/test_asgi_integration.py::test_asgi_lifespan tests/integration/test_asgi_integration.py::test_asgi_telemetry_ownership'
```

Edited-owner static checks, run Cargo sequentially:

```bash
mise run py:lints
mise run py:typecheck
mise run fmt:check
mise exec -- cargo clippy --locked -p wyrd-spec -p wyrd-cli --all-targets --all-features -- -D warnings
mise run docs:generate
mise run docs:check:commands
mise run docs:linkcheck
mise exec -- git diff --check
```

Inspect generated equality and the published completion steps; use existing content checks for edited docs if necessary. No wire-generation input needs changing; run codegen only if implementation actually changes one. Do not require whole docs/all-SDK/bootstrap aggregates or all C1–C8 journeys in remediation; combined suites run once at change review. Record actual platforms, diagnostics and verification limits without claiming unrun proof.

## Packet checks

A fresh read-only reader reviewed revision 6 and this task together and found
no blocking comprehension gap. It received no implementation source or prior
review conclusions. This confirms document clarity, not implementation proof.

The bounded [guided-mode handoff review](guided-revision-readiness.md) returned
Ready with architecture, executability and independent cold rehearsal passing;
it included separate simplicity and system passes. It did not rerun the original
findings audit or runtime journeys. Terminal and child cleanup remain obligations
to prove during implementation.
