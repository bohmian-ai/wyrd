# Maintainer review — R4 working source

Result: **FAIL**.

Subject: HEAD `0d317ecdf2d28d2e77b6f394ebe0f443d256d580`, prior candidate `d89356985566ab3492699682942438c25c8322fd`, plus the pinned dirty runner and runtime test. `sha256sum -c review/task-001-r4-0d317ecdf-working/subject-sha256.txt` passed for all ten entries (command run from repository root with the full packet path). No source edits or Cargo execution.

## Scope and governing rules

This is the independently assigned maintainer review of terminal-decision, delivery and acknowledgement ownership, including the attempted dirty correction. It is not a new whole-task discovery pass. Authority: supplied AGENTS.md; SQL AGENTS.md; `agent_docs/maintainer-style.md` (one owner for retained state; helpers isolate actual rules); `changes/spec-driven-development.md`; approved supplement revision 3, especially its terminal-settlement section; R4 remediation; `architecture/wyrd-design.md:1857–1888`. The amendment requires every required canonical record to be acknowledged before retiring delivery and directs extension of existing owners. It preserves immediate control cancellation and direct success's fast return.

CodeGraph was used before source searching, including runner, state and result-builder explorations. Source and Git delta were inspected independently; no other discovery reports were read.

| Changed surface | Owner, callers and proof inspected | Assessment |
|---|---|---|
| `Decision`, `RunState::select/fail/decide` | state memory authority; queued execute/retained, direct service, pending delivery; `a_recorded_failure_is_never_superseded` | One memory lock selects either disposition; existing owner extension is appropriate. No separate decision framework is needed. |
| `execute`, `refused`, `decided_transition`, `result`, `deliver` | queued attempt, lease confirmation, result builder/outbox acknowledgement, recovered decisions | Normal selected-outcome route retains existing builder and result ACK owner. |
| `RunState::deliver`, `registered` | cancellation request and runner pending-delivery callers; exact queue metadata and accepted state | Accepted state is reused, but selected-outcome handling skips the existing result delivery owner; M-R4-01. |
| `retained`, dirty tuple/`recorded`, `attempt`, `settle` | retry exhaustion, terminal failure, recovery, release; SQL terminate/retry/delivered | ACK provenance reaches settlement but is disabled; M-R4-02. |
| `deliver_owed`, `deliver_cancellation` | existing queue claims and rescheduling; control cancellation and runner delivery | A request-side best effort and worker retry are legitimate distinct callers. They must agree on what counts as complete delivery. Do not replace them with a service/projector. |
| Queue terminal coordination | terminate, cancel, principal cancel, claim-deliveries, delivered, redeliver; `PendingDelivery` | Existing queue extension fits approved custody. This review inspects their terminal consumers and transaction composition; persistent-data correctness is the separate data specialist's scope. |
| Relevant tests | failure exclusivity unit test; `retryable_engine_failures_exhaust_to_errored`; `terminal_failure_reclaims_without_reentering_the_engine`; result replay assertions and dirty test delta | Source checks support dispositions/replay, but no execution was performed here. Atomic ACK-aware retirement assertion exposes M-R4-02; selected-success cancellation interruption remains the necessary focused gap for M-R4-01. |

## M-R4-01 — pending delivery treats a selected checkpoint as completion of all required records

Classification: **INCORRECT / duplicate workflow ownership**.

Changed locations: `state.rs:597–633`, especially `606–607`; consumers `runner.rs:543–562` and `service/inspection.rs:728–751`. Existing owner: `runner.rs:1255–1278`, `1306–1344`, `1347–1410`, backed by `ResultPayloadBuilder::build_as` in `results.rs:318–343`.

A selected checkpoint and an acknowledged result are different stages. The normal runner explicitly builds the selected output/summary with the stable result ID, sends through the existing outbox and waits for result ACK. The new pending-delivery route instead returns any `Decision` from `RunState::accepted` immediately; both callers treat any successful return as permission to clear `delivery_due_at`. Thus two paths now define completion of the same selected decision differently.

Reachable scenario: a queued run ACKs its selected state; result delivery has not completed; cancellation/principal revocation atomically changes its still-running queue row to cancelled and leaves pending delivery. The old computation lease is fenced. The request callback or a restarted runner reads `Decision::Selected` and immediately retires delivery. Neither route stages the missing result. A process death before result enqueue, or a dropped/rejected result frame, leaves selected success without its required output/summary and with no remaining queue responsibility to repair it. This does not depend on an exotic supplied state or on widening the accepted old-worker overlap.

Concrete maintenance cost: a maintainer changing terminal result requirements must discover and update a second purported finalization path inside the state loader, while its `Result<Decision, StateError>` return does not promise or enforce result-record completion. The existing correct result delivery owner is bypassed. This violates the explicit ACK-before-retirement rule and can lose recoverable result records, so this is not a naming or layout preference.

Smallest testable correction: keep `RunState` responsible for selecting/loading its canonical decision and reuse the existing result builder, stable selected identity, outbox and required-result ACK behavior when pending delivery encounters a selected outcome. Retire pending delivery only after that route completes all required records; preserve cancellation's immediate computation stop, record-only recovery and direct fast return. Carry any exact metadata required by the existing builder through existing queue/identity custody rather than introducing a new result payload mirror or alternate builder. Do not prescribe an extra helper, service or generic finalizer merely to share that behavior.

Focused proof: interrupt after selected-state ACK but before required result ACK, cancel/revoke, restart without original caller, and verify same result identity and required canonical rows before delivery retirement, with zero engine entries.

## M-R4-02 — the dirty ACK correction remains disabled, preserving a second delivery pass for every acknowledged failure

Classification: **DRIFT / incomplete correction**.

Changed locations: `runner.rs:414`, `479`, `1183–1211`, `1255–1278`, `1568–1601`, especially `settle:967–969`. Existing mechanisms to reuse: `RunState::fail` returns only after `durable`; recovered `Decision::Failed` comes from accepted state; SQL `queue.delivered` already composes on caller-owned `TenantConn` and is idempotent.

Dirty source propagates `recorded` from both accepted-failure recovery and `retained` through `attempt` to `settle`. Nevertheless `if false && recorded && ...` prevents it from ever retiring the acknowledged failure's delivery in the same settlement transaction. `terminate` and exhausted `retry` mark delivery due unconditionally. Each ordinary engine-terminal failure/final retry therefore persists another delivery item even though its canonical failure has already been ACKed.

Concrete cost: already completed failure recording now requires another due-tenant walk, claim transaction, SYSTEM lookup, accepted-revision discovery/state read and retirement transaction. If read authority/source availability changes after the original ACK, that unnecessary durable item retries despite there being no missing record. The extra work applies to the normal failure path, not only a recovery corner. The dirty test `retryable_engine_failures_exhaust_to_errored` now asserts the opposite: the first transaction exposing errored must owe no delivery, while current SQL necessarily commits it due and relies on a race with another worker to clear it. Keeping a complete disabled provenance path also misleads maintainers: rustdoc says settlement uses it, executable control flow does not.

Smallest testable correction: use the existing acknowledged-failure provenance to retire only the successfully fenced matching terminal settlement in that same transaction, preserving due delivery when recording was unacknowledged. Preserve cancellation's competing pending work and stale-lease behavior. Do not add a second consumer guard or another Bifrost reread to rediscover ACKs already known at the producer. The current positional `(Transition, Option<StagedResult>, bool)` permits contradictory combinations, but that alone is not a separate blocker: the required outcome is to couple ACK evidence to the exact decision being settled. A local representation change is warranted only if needed to preserve that relationship through transition rewrites; no particular new enum/helper is required by this review.

Focused proof: observe the terminal settlement atomically, before a delivery worker can repair it; acknowledged engine failure/final-attempt recovery must commit no owed delivery, failed state ACK must retain it, and a stale computation settlement must not retire another cancellation delivery.

## Limits

No style-only findings or optional refactors are proposed. The failure-memory `Decision` enum is earned; it does not itself constitute a second durable owner. No implementation, Cargo, broad gate or runtime performance measurement was performed. Source pins were unchanged when checked; execution proof remains the orchestrator's evidence responsibility.
