# TASK-008 round 2 scope

Immutable remediation range: `798b6b0e83595356c0d2328fa541119267e84fb2..cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`. Original cumulative base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`. HEAD must remain candidate. Original task: `../../tasks/008-approval-steps.md`; approved packet spec revision 8, contracts Human approval, task supplements, root AGENTS and applicable architecture remain authority. Prior directory: `../008-approval-steps-r1-798b6b0e8/`, including verdict, validation and remediation task.

Review closure FIND-008-1–7, actual remediation source/tests/docs diff and affected paths. Carry forward prior acceptance matrix and specialist evidence for unchanged wire types, SDK projections, registration queue_only, authorization/audit, canonical Scribe custody, stable occurrence identity, supersession and language surfaces. Reopen only changed dependencies or a demonstrated evidence gap. Reports added in the diff are historical evidence, not production machinery.

## Shared navigation map and coverage selection

| Contract / invariant | Producer, owner, consumers | Coverage |
|---|---|---|
| Reached occurrence while peer runs; stable accepted state and same-execution continuation | engine Done::Awaiting → Checkpoints::reached → RunState::record_approval/change → background writer; inspection and runner park/recheck | fresh remediation ponytail discovery; data/lifecycle domain specialist |
| Withdrawn speculative occurrence refuses fresh decision, retains history/replay | ExecutionState::speculation → Retained::withdrawn/approvals/undecided → decide; engine discard/restore | discovery; data/lifecycle specialist |
| Original authored/runtime deadline | exact Target::authored_timeout → runner and decide → SQL open_for_decision | data/lifecycle specialist; unchanged SQL clock/fence coverage carried from prior data report |
| Actor attribution and exact replay; public error redaction | authenticated Caller attribution → decide/replay → receipt; ApprovalDecisions::unavailable → HTTP/MCP | security specialist; prior RBAC/tenant coverage carried forward |
| Mandatory docs and accurate closed decision names | MCP Opened::start Panics; architecture Verifier sentence → existing enum/contracts | discovery |

Required discovery reports: `task-review-behavior.md` (fresh ponytail remediation reviewer), `domain-review-data.md` (coherent persistent-state/lifecycle and recovery boundary), `domain-review-security.md` (delegated actor identity and redaction). Independent fresh validator follows all discovery, writes `findings-validation.md`, validates specialist applicability and carried-forward coverage. No reviewer self-validates a new finding.

Separate invariant/system specialists omitted because the scoped data reviewer covers changed state, runner handoff and deadline recovery end to end; prior invariant/system reports cover unchanged scheduler/custody. Separate repo specialist omitted: no broad cross-tier change; discovery covers changed Rust docs/tests and architecture, security covers errors. Separate maintainer specialist omitted: extends existing checkpoint/state owners without substantial new abstraction; discovery evaluates reuse. No new UI, SDK, auth protocol, deployment or storage-schema contract. Follow-up only for material unresolved conflict/path. Orchestrator runs Cargo sequentially; agents must not run Cargo.

Round 1 explicitly reserves broad SDK suites and clean full L1 benchmark qualification to integrated change review; bounded remediation proof does not claim those results. Inventory new/moved waits in this round and preserve that limit.
