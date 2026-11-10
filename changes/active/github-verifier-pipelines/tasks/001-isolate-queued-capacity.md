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
