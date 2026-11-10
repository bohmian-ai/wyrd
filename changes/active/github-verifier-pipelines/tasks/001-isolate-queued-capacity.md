---
id: TASK-001
status: proposed
approved_spec_revision: 3
requirements: [REQ-015, REQ-016]
acceptance: [AC-010]
invariants: [INV-001, INV-005, INV-007]
depends_on: []
---

# Isolate queued capacity

Carry **approved-runtime-and-github.md → Direct and queued capacity / Direct
execution eligibility** into the existing engine composition. Direct-eligible
verification must still execute while the queue fills its own execution,
workspace, Python and model-call allowances. This is reserved admission, with no
borrowing; it does not promise isolation of shared CPU, database or providers.

## Existing flow and selected change

Inputs are existing direct `verify` and queued `submit`/runner requests, their
accepted Caller/Verifier/evidence, and operator configuration. Today
`server/verification/mod.rs` composes one `HostedBlocks`; `hosted/blocks.rs`
constructs its Sandbox and Workspaces and builds the common engine, while
`runner.rs` owns queue execution slots. Splitting that semaphore alone leaves
workspace, Python and gateway model admission shared.

Extend that composition to select independent direct and queued resource
ownership at all four admissions, keeping `HostedBlocks::engine`, the hosted
block executors and the normal gateway/tenant accounting path. Route each call
through its accepted execution mode, including queued continuations. Expose the
isolated limits and original execution ceiling through server configuration's
existing typed/environment validation, not a new scheduler or process service.
Keep limits at their existing owners; model requests must use the selected lane
before entering the existing provider path.

Establish direct eligibility validation before any engine step. Tasks 005, 007
and 008 attach their new extraction/deferred/approval forms to this validation
when those contracts arrive; do not create those actions here or silently enqueue
a direct request. Ordinary direct graphs retain their current behavior.

Amend **wyrd-design.md → Verifier / Agents, tools, and workspaces** for isolated
admission and queued-only features, and relevant operator operations guidance
for limits/deadline ceilings. Preserve the one engine and existing authority,
evidence, billing, cancellation and platform behavior.

## Output and consumer

Direct and queued engine composition now select their own resource pools under
operator limits. The existing runner and tasks 005–008 consume queued resources;
the synchronous service consumes direct resources. No new user operation or
queue is introduced. Inspection/configuration documentation must explain what
is reserved and how intended human approval periods fit the original ceiling.

## Proof and delivery

**AC-010:** Against WyrdTestServer and repository Postgres, hold queued
execution, workspace, Python and model admission at configured capacity, then
run a direct-eligible SDK Verifier that uses each corresponding resource and
observe completion through the SDK. Exercise each admission distinctly so an
unused shared pool cannot hide a gap. Queued work must wait for its own capacity;
releasing it allows progress. Confirm existing tenant/subject accounting remains
on the ordinary gateway path. The later introducing tasks prove each queued-only
action refuses direct execution before its first step.

Owner/tier: resource and configuration branch tests in their owning Rust modules;
real resource composition/journey proof in `wyrd-testing`'s existing `server`
target (`verification_runtime` owner). Use `mise run test:bifrost:journey:server`
for the managed Postgres/server runner, and the affected `test:wyrd`/`test:skald`
owner slices for hosted and model admission. New test names/focused commands are
recorded after implementation, before RED; none are invented here.

Run `mise run fmt`, `mise run lints`, `mise run check:deps` and
`git diff --check`. No synthetic host load, separate engine, platform-specific
resource enforcement or completion-time SLA is part of this task.

## Obligations

| ID | Source (task line) | Observation | Planned assertion |
|---|---|---|---|
| O1 | AC-010 "hold queued execution … run a direct-eligible SDK Verifier" | With every queued execution slot held, a direct SDK `verify` completes | SDK journey: queued slot held by a held queued run; direct verify result `passed` |
| O2 | AC-010 "workspace" | With every queued workspace held, a direct SDK Verifier whose Agent uses a workspace completes | SDK journey: queued workspaces held; direct workspace Agent passes |
| O3 | AC-010 "Python" | With every queued Python admission held, a direct SDK `code` Verifier completes | SDK journey: queued sandbox held; direct code Verifier passes |
| O4 | AC-010 "model admission" | With every queued model-call admission held, a direct SDK Agent Verifier completes | SDK journey: queued model calls held; direct Agent passes |
| O5 | AC-010 "Queued work must wait for its own capacity; releasing it allows progress" (execution) | A queued run does not start while the queued slot is held; completes after release | SDK journey: second queued run not settled while held, completed after release |
| O6 | same (workspace) | A queued workspace run waits while queued workspaces are held; completes after release | SDK journey: deferral observed, then completed |
| O7 | same (Python) | A queued code run waits while queued Python is held; completes after release | SDK journey: deferral observed, then completed |
| O8 | same (model) | A queued Agent run waits for queued model admission; completes after release | SDK journey: wait observed, then completed |
| O9 | "no borrowing" (design/spec REQ-015) | Direct work never takes queued capacity: a full direct lane refuses direct work although queued capacity is free | SDK journey: direct workspace held → direct verify `WYRD_SERVER_503_SERVICE_UNAVAILABLE` |
| O10 | "Route each call through its accepted execution mode, including queued continuations" | Direct calls use direct resources; queued attempts, including re-claimed deferred continuations, use queued resources | O2–O8 assertions (deferred queued run resumes on queued lane after release) |
| O11 | "Expose the isolated limits and original execution ceiling through server configuration's existing typed/environment validation" | Every lane limit, direct/queued execution ceiling and the queued execution deadline parse from env; bad/inconsistent values refuse startup | `config::tests::verification_limits_env_applies_and_refuses` |
| O12 | "Keep limits at their existing owners" / defaults | Defaults keep the original combined Python/workspace capacity and the 64-slot five-minute queued ceiling | unit tests in `verification::hosted` and `config` |
| O13 | "Establish direct eligibility validation before any engine step" | A queued-only (Window) Verifier is refused by direct `verify` before any step runs, through one named check | existing SDK journey `verify_in_real_time` window refusal |
| O14 | "Confirm existing tenant/subject accounting remains on the ordinary gateway path" | A direct Agent's model call admitted by its lane still passes through the gateway and is attributed to the direct subject | SDK journey: gateway capture/accounting row carries subject |
| O15 | "Amend wyrd-design.md … and relevant operator operations guidance" | Design and operator docs describe isolated admission, queued-only features, limits and the deadline ceiling for approvals | `mise run docs:check` |
| O16 | "Run fmt, lints, check:deps, git diff --check" | All pass | command output |
| O17 | "Preserve the one engine" | Both modes build their engine through `HostedBlocks::engine` | code citation + O1–O8 |
