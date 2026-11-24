# Task-local input waits

Status: finalized — maintainer agreed on 2026-11-24; approved spec revision 9.

## Problem and desired outcome

TASK-006 remediation exposed a reporting gap: a deferred-input consumer is
already blocked in the engine while independent work runs, but inspection only
names missing inputs when the entire execution parks. An engineer or agent
cannot see which task needs evidence soon enough. The UI also infers task waits
from definition edges, which fails without definition access.

Maintainer direction (2026-11-24): follow established step-engine behavior;
do not invent a new execution model. A task reports its own input wait promptly,
independent tasks continue, and the run stays running while work progresses.
This proposal changes reporting, preserving the existing scheduler and parked-run
lifecycle. It is not implementation authorization.

## Agreed contract

Extend the existing execution status with optional `input_waits`, defaulting to
empty. Reuse the same type in recoverable execution state:

```json
{
  "status": "running",
  "input_waits": [
    {"task_id": "ci-review", "slots": ["ci"]},
    {"task_id": "green", "slots": ["ci"]}
  ]
}
```

`InputWait` contains `task_id: TaskId` and `slots: BTreeSet<SlotName>`.
Entries have nonempty slot sets, are unique and ordered by task ID, and contain
only missing declared deferred-slot names. Omit the field when empty. No values,
credentials, timestamps, new state enum, or invocation identities are needed.

- Report each directly consuming pending step or whole-Verifier requirement.
  Include direct consumers even when they also have unfinished dependencies.
  Attached requirements contribute their slots to their owning step, matching
  existing readiness gating. An indirect dependent is not an input consumer.
- Waiting happens before dispatch/fanout expansion. Do not manufacture an
  invocation, attempt, item or iteration for work that has not started.
- Missing ordinary inputs retain their existing behavior; this concerns only
  declared resumable deferred inputs. Unused deferred inputs create no wait.
- Existing pending approval occurrences remain separate and unchanged.
- The run stays `running` while independent work runs. Existing `waiting` status
  and its `waiting` slot summary apply when the execution parks. Approval-only
  parking can have an empty slot summary. Never derive run status from
  `input_waits`, and never manufacture a verdict from a wait.
- Terminal inspection returns no active input waits. Historical wait transitions
  need no new event stream or history API.

## Connected implementation flow

1. **Authority and wire owner.** Clarify task-local reporting in the deferred-input
   section of `architecture/wyrd-design.md` (currently lines 1722–1745), preserving
   its existing consumer/dependent/independent scheduling rules. Add `InputWait`
   and `input_waits` in `crates/wyrd-spec/src/verification/execution.rs` and
   `state.rs`, with existing derives, slot-set schema treatment and rustdoc.
2. **Readiness owner.** `Engine::drive` in
   `crates/vala/vala-verify/src/blocks/engine.rs` already calculates `waited(node)`.
   Reuse it to report a complete pending-consumer snapshot after the readiness
   pass, before waiting on running tasks or returning `EngineError::Waiting`.
   Extend the existing `Checkpoints` boundary with a synchronous fallible
   snapshot method; map failure to the existing checkpoint failure. Runs without
   checkpoints keep their current behavior. Do not add another readiness evaluator.
3. **Retention owner.** Implement the callback in server `RunState` through
   `change`: replace the list, skip unchanged snapshots, preserve lease fencing,
   and stage through the existing asynchronous Scribe writer. Add no IO wait or
   PostgreSQL operation to scheduler progress. The existing parked revision still
   needs its existing persistence acknowledgement before releasing the claim.
4. **Consumption and completion.** `Runner::deferred` reads accepted evidence
   before driving the engine. `RunState::accept_evidence` removes that slot from
   every wait and drops empty entries; the next readiness pass recomputes the
   complete snapshot. Clear active waits at terminal selection/failure, and
   filter them out at terminal inspection too, covering cancellation,
   supersession, deadline and access-loss paths. A stale writer changes nothing.
   Publication acceptance alone must not clear a running engine's waits: the
   engine holds a fixed evidence snapshot for that drive. Existing wake/recheck
   and resumption consume evidence under the same execution identity.
5. **Public readers.** Project the retained list in `VerificationControl::get_execution`
   and its `Retained` inspection owner, alongside existing waiting and approvals.
   Initialize empty in direct and SQL response constructors. Keep current parent
   authorization, tenant isolation and retained reads after queue cleanup.
   HTTP, SDKs, MCP `verification.get_execution`, and CLI `execution status` use
   this same projection; add no endpoint or surface-specific computation.
6. **Language contracts.** Regenerate TypeScript wire types through
   `mise run codegen:regen`; the TS status wrapper already preserves other fields.
   Extend Python's authored `verify.ExecutionState` TypedDict with `InputWait` and
   optional `input_waits`; use existing Python/Rust status transport. Audit exports
   and constructors so all first-class clients expose the same response.
7. **UI consumer.** `loadExecution` already supplies the status response.
   Use reported task IDs and slots in `executionStates` and
   `VerificationExecution.svelte`; remove edge-based wait inference, including
   `awaitedBy`. Show “waiting for input ci” on the consuming task while peers run.
   Use the reported slots for missing-input presentation too. Definitions supply
   labels/layout only. With no definition, include waiting task IDs in the existing
   recorded-work view and label them `Task <id>`; the wire does not need a task-kind
   discriminator. Keep whole-Verifier requirements visible as waiting when their
   definition is available. Attached requirements remain under their owning step.
   Absent reporting from an older retained state means no recorded task wait;
   retain any existing parked slot summary without inventing task attribution.

These are extensions of existing owners. A new task-status service, generic
wait framework, dependency analyzer, polling operation, or scheduling mode has
no role in this change.

## Persistence and deployment

Bump `EXECUTION_STATE_FORMAT` from 5 to 6 and keep formats 3–5 readable with an
empty default. Resumed scheduling fills the list. Do not backfill historical
waits or alter evidence/approval identities. There is no SQL migration and no
committed JSON Schema: schemas are runtime-derived; regenerate consumed contract
artifacts through the existing generator.

Use a coordinated upgrade: stop old workers before starting format-6
writers, then resume from existing retained state. Old binaries reject format 6
and cannot safely reclaim it. A normal mixed-version rolling deployment is not
supported by this proposal. Reader-first rollout would require a separate
reader-only release; do not add a writer switch or dual-write machinery here.

## Evidence and rationale

Verified by source inspection on 2026-11-24; no runtime tests were run:

- [Architecture](../../../../architecture/wyrd-design.md): deferred-input section
  already says consumers and dependents wait while independent steps run.
- [Engine::drive](../../../../crates/vala/vala-verify/src/blocks/engine.rs):
  `waited` gates each pending node before dispatch; the union is currently exposed
  only when all running tasks drain. `Step::input_slots` includes attached checks
  in `card/verifier/validate.rs`; IDs are unique across steps and requirements.
- [RunState](../../../../crates/wyrd/wyrd-server/src/verification/state.rs):
  existing writer, `change`, `accept_evidence`, `wait` and `decide` own retention.
- [Runner](../../../../crates/wyrd/wyrd-server/src/verification/runner.rs):
  `deferred` accepts supplied evidence before engine execution; it does not refresh
  a running drive. Publication durability and runner consumption are distinct.
- [Inspection](../../../../crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs)
  owns lifecycle projection and only exposes the parked slot summary for `waiting`.
- [UI](../../../../crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/execution.ts)
  infers task waits from graph edges. Its component's `awaitedBy` does the same.
- [Remediation](../review/TASK-006-bfbdf016a-01/TASK-006-R1-approval-recovery-and-boundaries.md)
  records the actual running-peer journey and its unresolved simultaneous-state gap.
  Its recorded passes are prior evidence, not tests rerun for this proposal.

The reviewer's “every engine” claim is broader than necessary. The established
convention is task-local visibility independent of aggregate lifecycle; engines
use different wire representations and aggregate states.
[Buildkite input steps](https://buildkite.com/docs/pipelines/configure/step-types/input-step)
explicitly allow unrelated steps to run while input is outstanding.
[Airflow deferral](https://airflow.apache.org/docs/apache-airflow/stable/authoring-and-scheduling/deferring.html)
records suspended work at the task. Wyrd should follow that behavior without
copying another engine's enum or replacing its existing run-parking semantics.

## Observable proof and implementation order

The maintainer explicitly waived TDD ordering for this task on 2026-11-24.
Passing behavioral proof remains required; do not treat a compiling type or a
scripted engine outcome as proof of concurrent real-engine behavior.

1. Engine unit test in `blocks/engine/tests.rs`: exact direct consumers and missing
   slots, attached requirement ownership, no indirect/unused-slot entries, no
   pre-dispatch invocation/fanout identity. Check unchanged snapshot behavior at
   RunState and stale-writer refusal through existing writer tests.
2. State unit tests: format-6 round trip, formats 3–5 default empty, future formats
   remain refused. No historical reconstruction.
3. Real-server deferred-evidence journey in server
   `tests/integration/pg_deferred_evidence.rs`: hold an actual independent provider
   response; observe `running`, empty parked summary, exact step and standalone
   requirement waits and no blocked-step invocation. Publish while held; waits
   remain until runner consumption. Release, resume, and observe waits clear,
   the consumer executes and the same execution completes. Use a controlled gate,
   not sleep duration or synthetic load, to establish overlap.
4. Extend existing publication/restart/supersession/deadline/access-loss/cancel
   coverage where relevant: consume multiple slots independently, retain waits
   across recovery, never reopen terminal work, terminal inspection has no waits,
   and retained status remains readable after queue cleanup. The existing
   publication-while-running test uses an EngineScript; keep it for wake behavior,
   but use the real-engine journey above for task-wait semantics.
5. Extend existing Rust/Python/TS deferred-evidence journeys and MCP verification
   inspection coverage to read the new field through their public clients,
   including a negative scoped read. Reuse current fixtures/runners; no parallel
   test framework. CLI JSON serialization uses the existing typed status result.
6. Extend the existing TASK-006 real-server UI journey and
   `fixtures/cards/approvals/beside-review.yaml` with a `ci-review` step consuming
   `ci`. Before releasing the held notes reviewer, assert all three together:
   waiting input consumer, pending approval, and running peer, with no verdict.
   Repeat rendering without definition access. After parking and evidence
   consumption, show the appropriate recorded transition without UI inference.
   Small UI tests cover task-ID mapping, terminal clearing and missing definitions.

Planning should select exact focused commands for existing tests and establish
new selectors during implementation, using `mise` and `TESTING.md`. Required
qualification includes affected crate format/lints, focused engine/state/server
and client journeys, `ui:check`, and `codegen:check`. Use established SDK checks
for authored/generated contracts. No benchmark, broad aggregate, auth redesign,
or unrelated remediation is introduced by this proposal.

## Decisions, conflicts and handoff

The maintainer approved this proposal and requested the spec revision and one
complete implementation task on 2026-11-24. Approved spec revision 9 records
REQ-022/AC-015; the owning architecture deferred-input section is amended.
[TASK-006A](../tasks/TASK-006A-task-local-input-waits.md) carries the connected
implementation and proof. Its explicit no-TDD exception does not waive tests.

Revision 9 supersedes revision 8's backend exclusion and TASK-006-R1's preserved
scope only for this reporting work and the remaining simultaneous-state gap.
Preserve completed idempotent rejection and Origin work and recorded evidence.
No material decisions remain open. Private names/layout remain implementation-owned;
this planning turn does not implement, deploy, commit or launch task review.
