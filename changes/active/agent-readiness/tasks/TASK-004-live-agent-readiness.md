---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-agent-readiness
spec_revision: 2
requirements: [REQ-011, INV-001, INV-002, INV-003, INV-004, INV-006, INV-007, AC-011, AC-012]
depends_on: [TASK-003]
---

# Deliver the opt-in live coding-agent readiness lane

## Outcome and Value

The owner can occasionally run `mise run test:agent-readiness:smoke:live`
to learn whether Claude Code and Codex can independently build and execute a
simple application integrated with Wyrd using this commit's docs and MCP.
The harness owns a real foreground development server and locally served docs;
authorized Wyrd evidence decides success, with per-agent failure/transcript
details that show where onboarding broke down.

Execute with `$wyrd-implement`. This final task depends on TASK-003, and
transitively TASK-002 and TASK-001: final lifecycle APIs, parseable readiness,
host connection and the complete local docs/guide must exist first. It delivers
REQ-011/AC-012 and contributes live-lane exclusion proof to AC-011. It does not
replace any deterministic test or create a new product evaluation platform.

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Opt-in execution boundary | `mise.toml` tasks `test:gateway:smoke:live`, `test:operators:smoke:live`; gateway smoke credential fixtures | Existing explicit live tasks outside gate; `gate` dependency/shard inventory | No coding-agent readiness task | One tracked approved live task using untracked `mise.local.toml` credentials/local settings | One required orchestration entry point; no CI scheduler or new evaluation service |
| Local server lifecycle | Production `DevServer` setup/readiness/wait and shared protected config; test `local_deployment/installed_local_cli` and Rust `DevServerFixture` | Installed-CLI first-run/saved-config/host journeys | Existing fixtures do not run both real coding agents against a connected server | Harness owns the actual foreground CLI/server, waits for JSON readiness and settles it | Reuse existing setup, Postgres and current-build preparation; no new bootstrap engine or server stop mode |
| Local docs build | `docs:generate/build/preview`, `docs/package.json`, static adapter deployment base | `docs:check`; TASK-003 full-reference/content checks | No live-agent run bound to locally served docs identity | Build and serve docs from this candidate, wait for their availability and provide that origin | Existing docs toolchain serves the complete workflow; no second docs server product |
| Agent connection/isolation | CLI MCP `install`, proxy and shared saved credential resolution | Existing CLI selected-host install/proxy journeys in `tests/cli.rs` and isolated host configs | No two-agent noninteractive application exercise | Install Wyrd MCP into fresh isolated host/app contexts and invoke real Claude Code/Codex headlessly | Minimal child-process coordination is required; no agent framework, custom MCP transport or host-wide config mutation |
| Independent evidence observer | `Cards::get/list`, `Bifrost` SQL, existing verification execution status/results and input provenance | Three support-desk stories resume executions and join evidence; MCP execution/read journeys | No per-live-agent verdict based on actual server state | Use authorized public reads for exact declared graphs, Card/Run-correlated rows and sync/queued judgments | Small test observer is needed; no new durable Run registry, judge Card kind or metrics database |

Reuse complete workflows, not just leaf calls. The current installed-CLI
fixtures already prepare current binaries under an isolated version root and
save client settings. They contain Unix-specific terminal/process helpers;
do not copy those mechanisms into new portable runtime/harness behavior.
Choose the smallest existing portable facilities that satisfy the approved
three-target and foreground-owner requirements. Private runner language,
file layout, subprocess test seams and artifact organization are implementation
choices, not a new product boundary.

The harness supplies the same bounded app-building objective, local docs
origin and running MCP connection to each agent, not a Wyrd integration recipe,
prewritten YAML/app, internal plan/skill context or human clarification. Normal
public SDK types/help remain usable. Isolate host configuration and application
directories from previous runs, the owner's normal setup and each other.
Credentials/local settings are supplied through untracked `mise.local.toml`
and existing protected auth mechanisms, never tracked values or argv secrets.

Success requires both independent applications' exact Service/components and
Verifier bindings, observed Runs evidenced by correlated rows, telemetry,
custom rows and real retained synchronous and queued asynchronous judgments.
Use execution/input provenance to distinguish modes; two synchronous results
or a queued ID are not sync-plus-async success. Passed/failed application
verdicts are judgments; missing/inconclusive evidence, incomplete retention,
terminal execution errors and agent self-report are not success. Per-agent
attribution must prevent another run/agent's evidence from satisfying a check.

No Wyrd core/API/runtime changes, new lifecycle command, provider smoke-suite
redesign, production service manager, universal agent benchmark, live CI/gate
dependency, sibling repo changes or UI work. Preserve others' edits. Changes
needed in predecessor outcomes go through their existing task/spec authority;
do not turn this task into review remediation.

## Approach

1. Add the minimal deterministic preflight/process/observer failure proof at
   the chosen test owner, reusing current fixtures without external credentials.
2. Add the tracked live task with explicit prerequisite/settings documentation,
   and prove it is unreachable from every automatic gate/CI path.
3. Own current-build foreground Wyrd and docs generation/build/serve; wait for
   real readiness and initialize isolated connected host/application contexts.
4. Invoke both real agents non-interactively with the bounded objective, retain
   redacted transcripts and evaluate independent server evidence under deadlines.
5. Report both outcomes and failure points, settle owned children, and record
   the owner's actual opt-in live result plus focused deterministic proof.

## Ordered Implementation Scenarios

Use TDD for the coordinator and observer's behavior. Tests need not invoke
paid agents to force a process failure or an incomplete evidence case. Keep
such tests narrow; they supplement, never replace, the actual live task's
real-server/two-real-agent journey. Record exact focused commands for the
chosen self-check tests after confirming their collection; no private fixture
layout or extra permanent runner interface is prescribed here.

### Scenario 1 — Both agents receive a ready server and this commit's docs

**Behavior.** Preflight missing binaries/auth/settings fails explicitly.
The harness owns foreground Wyrd, waits for actual JSON readiness and serves
docs built from the candidate before launching agents. Each starts in a fresh
isolated app/host context with MCP installed through the real CLI, and all
onboarding links used resolve to this local build (REQ-011, AC-012).

**RED.** Add focused coordinator assertions for unavailable prerequisites,
startup/readiness or docs failure, and isolation/launch ordering. Before the
new task exists, no supported two-agent entry point can meet them. Use process
test doubles only for failure/ordering checks; real CLI/MCP/server behavior
must still be exercised by P1. Do not treat provider/network unavailability as
an expected behavioral RED.

**GREEN.** Extend existing setup/build/host facilities with the minimum test
coordinator. Supply only local docs/MCP entry points and ordinary environment
configuration. Preserve foreground ownership and pass the new checks.

**REFACTOR.** Delete duplicated bootstrap/host/documentation logic; use current
owners. Do not add service-manager state, custom transports or an agent framework.

### Scenario 2 — Only independent Wyrd evidence can produce a successful outcome

**Behavior.** The observer authorizes and reads exact new app graphs, Run rows,
telemetry/custom data and retained sync/async judgments. It rejects stale or
other-agent results, empty/degraded evidence, an execution ID without a
result, inconclusive/error states and fabricated agent reports (REQ-011,
INV-003/004, AC-012).

**RED.** Add narrow observer cases for each missing milestone and wrongly
attributed/mode result, plus passed and failed application judgments. Use
real public server reads for the positive/negative evidence seam; pure
classification cases may be isolated. Assert no new Run table is consulted.
The baseline has no live-agent evidence decision; P1 cannot be implemented
as a check of subprocess exit status or transcript text.

**GREEN.** Query current authorized Card/verification/Bifrost surfaces with
bounded projection/time range and polling. Confirm both agents independently
meet all required evidence before overall success; retain the reason and
identifiers for every non-success. Pass earlier coordinator and observer cases.

**REFACTOR.** Reuse existing SDK query/status mapping; keep only the test's
milestone decision logic. Add no durable evaluator, alternate query transport
or product correctness score.

### Scenario 3 — Failure is inspectable and cleanup preserves the owner's setup

**Behavior.** Each agent has an outcome, transcript location, failure point,
evidence IDs and redacted diagnostics. Timeout, child failure, unavailable
external service and cancellation never become skipped successes; one
failure still allows reporting both outcomes. Owned agent/docs/Wyrd processes
settle and the owner's normal config/data remain intact (REQ-011, AC-012).

**RED.** Add deterministic child-failure/deadline/cancellation and secret
sentinel checks at the coordinator boundary. Assert retained diagnostics,
no accidental live selection by gate/CI, and unchanged unrelated config/data.
These checks fail until the coordinator owns/report-cleans its children.

**GREEN.** Complete bounded execution/polling, reporting and structured cleanup
with existing portable subprocess facilities. Run P1 explicitly when the
owner's credentials and settings are supplied, recording both real outcomes.
Missing authority leaves AC-012 unverified/blocked, never a claimed pass.

**REFACTOR.** Reuse cleanup and safe reporting already present in the chosen
test owner; avoid a general job manager, transcript system or retry framework.

## Acceptance Criteria

| Criterion | Spec obligations | Proof |
|---|---|---|
| Tracked opt-in task uses untracked local settings, excluded from all automatic execution | REQ-011, INV-007, AC-011 | Task/dependency/CI source audit and deterministic selection checks |
| Harness owns ready foreground Wyrd and commit-local docs before both isolated connected agents | REQ-011, INV-001/003/006, AC-012 | Coordinator tests and actual P1 process/host/docs evidence |
| Server evidence proves both applications' full graph/Observe/sync/async workflow | REQ-011, INV-004, AC-012 | Observer negative/positive cases and real P1 authorized readback |
| Missing authority/evidence never passes; outcomes are inspectable and cleanup is safe | REQ-011, INV-003/006/007, AC-012 | Failure/cancellation/secret checks, transcripts and real per-agent outcome report |

## Expected Write Set and Consumer Closure

- `mise.toml`'s new explicit `test:agent-readiness:smoke:live` task and minimal
  repository test/script implementation using existing setup/SDK/docs tooling.
- Necessary test-only support and credential-free observer/coordinator checks,
  plus concise instructions for untracked `mise.local.toml` settings and
  artifact locations. Reuse existing ignored local configuration; never edit
  or commit the owner's real settings/credentials.
- Minimal docs/host URL wiring necessary to exercise the built candidate,
  preserving TASK-003's canonical generated content and public contracts.
  No new production contract, Wyrd UI edit or sibling repository write.

The coordinator's private placement is implementation-owned. Do not force a
new framework, fixture hierarchy or package dependency merely to host it.

## Verification and Evidence

| Proof | Exact command / evidence |
|---|---|
| P1 task's culminating two-agent real journey | `mise run test:agent-readiness:smoke:live` |
| P2 existing installed-CLI/host prerequisite regression if its support is changed | `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-cli --test cli -E 'test(=pg_tests::server_dev_selected_mcp_host_reads)'` |
| P3 built docs prerequisite | `mise run docs:build` and the existing `mise run docs:preview` foreground serve entry, with its readiness checked by the harness |

P1 is the task's exact approved public journey entry, not an inferred command
for an assumed new file. Its parent owns required builds/Postgres/settings
using the inspected live-lane precedent. P2's current package/target/selector
and P3's package scripts/tasks were inspected; use the final collected
equivalent if a predecessor moved a selector. Use existing CLI/native package
prerequisite builds; do not silently run a released binary against new docs
or give agents repository-internal integration instructions.

Record exact collected commands for every new named coordinator/observer
self-check. Run those first without live credentials, including observer
readback over a real server where cross-boundary state matters. Live P1 is
owner-invoked and may incur normal agent/provider usage; do not move it into
gate/CI or disguise missing credentials as a successful skipped run. Record
Wyrd commit/binary/docs identity, each agent version/config identity, actual
per-agent outcome, exact evidence IDs/observations and transcript location.
No agent self-report establishes acceptance. Keep secret material redacted.

Run applicable format/lint/type checks for the actual runner language and
`git diff --check`, plus source inspection of the task/CI dependency graph.
Do not run full deterministic journey suites or their broad aggregate as this
task's verification. Integrated change review runs the spec's final aggregate
once, after all tasks, and consumes owner-run P1 separately because live lanes
are deliberately excluded.

Coverage handoff for that review: TASK-001 owns AC-002/003/004/005; TASK-002
owns AC-007 machine/setup behavior and AC-010 platform authority/execution;
TASK-003 owns AC-001/008/009 plus AC-007 onboarding and AC-010 lifetime docs;
this task owns AC-012 and AC-011 live-lane exclusion. Each task contributes
its scoped AC-011 proof and mapped invariants. The final integrated AC-011
check is review-owned, not a fifth implementation task or a remediation plan.

Append the compact acceptance/evidence and non-goal matrix, actual commands,
material limits and final diff audit. No staging, commit or spec edit.

## Material Stop Conditions

Return to `$wyrd-spec` if success needs a different agent population,
prewritten Wyrd solution, public-site fallback, MCP mutation/check tools,
production/daemon lifetime, altered auth/tenancy/durability, a new evaluator
service or gate/CI live execution. Missing agent binaries, auth, supported
host or external provider availability are explicit proof blockers, not
permission to substitute fake agents or omit an outcome. Ordinary process
test seams, bounded waits, artifact paths and compatible installed CLI
invocation flags are implementation-owned; verify flags from installed help.

## Authority Links

- [Approved spec revision 2](../spec.md), [TASK-001](TASK-001-declare-get-pull.md),
  [TASK-002](TASK-002-machine-readable-local-tooling.md), [TASK-003](TASK-003-self-contained-agent-onboarding.md).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md),
  [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Router](../../../../architecture/references/README.md), [spec workflow](../../../../architecture/references/languages/spec-driven-development.md),
  [execution](../../../../architecture/references/languages/implementation-execution.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Agent harness](../../../../architecture/references/languages/agent-harness.md),
  [security](../../../../architecture/wyrd-security-posture.md), [Bifrost](../../../../architecture/bifrost-design.md),
  [telemetry](../../../../architecture/references/domain/telemetry-observations.md), [evaluation](../../../../architecture/references/domain/evaluation.md).
