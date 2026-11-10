# R5 maintainer review

**FAIL.** The terminal workflow consolidation is materially implemented at the
existing runtime owner. One explicit documentation gate and the previously
required hosted-dependent proof remain open in this scope.

## Subject and method

Reviewed HEAD `0d317ecdf2d28d2e77b6f394ebe0f443d256d580` plus the pinned working
sources described in `subject.md`; R3 `d89356985` supplies cumulative context.
Both `subject-sha256.txt` and `packet-sha256.txt` passed `sha256sum -c` during
review. No source, test, task, authority, staging or Git identity was changed.
No Cargo command was run. This report is independent of the other current
discovery reports and implementation conversation.

CodeGraph was consulted first for runner terminal owners/call paths. Actual
working source and HEAD/R3 diffs were then inspected. Authorities read include
root and SQL AGENTS, `agent_docs/rust.md`, `maintainer-style.md`, the task-review
skill, original TASK-001 Revision 1, R4 and R5, the approved terminal supplement
revision 3, the verification recovery/terminal sections of `wyrd-design.md` and
`bifrost-design.md`, the operations recovery section, and the revocation section
of the security posture. R3 verdict and validation supply unchanged acceptance
context; rejected R4 completion claims are not acceptance evidence.

## Changed-surface coverage

| Surface | Ownership, callers and rules inspected | Assessment |
|---|---|---|
| `runner.rs`: `execute`, `record`, `conclude`, `result`, `deliver`, `attempt`, `deliver_owed`, `settle`, `settle_delivery` | Attempt success/error/deadline/final retry and accepted-decision recovery; refused targets; pending delivery; ACK and queue transaction ownership | One decision/result operation is shared; no second failure-recording workflow survives. |
| `Frozen`, `Work`, `Offer`, `Custody`, claim structs and `custody` | Both claim shapes into existing `ResultRun`/`StateRow`; exact Card reconciliation and SYSTEM lookup; lifecycle and owner construction | These are narrow value/context types, with retained clients and lifecycle on `VerifierRunner`. No new service, writer or general framework. |
| `state.rs`: `Decision`, select/fail/decide, StateScribe write/durable deadlines | Canonical mutual exclusion; accepted state precedence; shared pending future and ticket coverage; terminal ACK allowance | Existing RunState remains the sole live/canonical decision owner. Changed timeout plumbing preserves short locks and existing shared writer. |
| `inspection.rs::cancel` and its removed shortcut | Authorized control transaction → queue cancel → commit; subsequent runner delivery | Request-side `RunState::deliver`/retirement path is removed. Immediate cancellation retains its control owner. |
| SQL terminal/delivery operations and row decoders | `TerminalDelivery`; attempt lease and delivery due-time fences; `PendingDelivery`, shared frozen-input/Trigger decoding; tenant capability and transaction composition | Existing queue owns narrow coordination. No duplicate SQL parser path: both claimed row forms reuse the extracted existing decoding rules. Documentation omission below. |
| Runtime P1/P2/P5 helpers and proofs | Commit-log trigger, canonical failure assertion, stale attempt, selected result ACK rejection/retry, existing fixture and fault seams | Commit logging observes the transaction rather than eventual repair; no new test harness. New helpers/tests have intent and panic docs. |
| Durable-work P4/T3 and group Support reuse | Public cancel/revoke/enqueue, actual group refusal, source outage and SYSTEM absence, failed retirement, retained identity; group fixture visibility | Reuses the existing group fixture. Retained reader/permission negatives remain present; no second group fixture owner. |
| SQL delivery-fence proof and other changed terminate/cancel consumers | New latest-claim test plus signature changes in existing SQL/server/Bifrost tests | New test directly exercises acknowledged versus owed settlement, stale computation, stale delivery, retry and retirement. |
| R4 T5, charge/preparation, proto and previous rustdoc gaps | `Checkpoint::charge/new/weight/prepare`, outbox weight/admission/release, T5 assertions, `execution_state[_of]`, `Leased::writer`, `IngestCaptureRequest` | Variable payload charge and stable stored weight are restored; old three rustdoc omissions close; tag 10 and name `state` are reserved. Additional new-item documentation gate below. |
| R4 T6 hosted-dependent proof | Full current test at `pg_durable_verifier_work.rs:3012`, its `judged`/`settlements` helpers and original/R4 requirement | Still a state-adapter proof, not the required hosted dependent observation. Finding below. |
| Migration/rollback | HEAD migration diff empty; R3 cumulative migration exception and recorded rollback inventory | Applied terminal-delivery migration is preserved; no new schema family or checksum rewrite. Temporal rollback execution is recorded evidence, not reconstructible from a final diff alone. |

`conclude` is a free function, but owns no retained dependencies: the caller
owns the RunState, reserved slot and lease lifetimes for the call. This fits the
explicit caller-owned orchestration exception in `agent_docs/rust.md`; forcing
another struct here is unnecessary. `record` adds recovery/identity preparation
for work that cannot compute and delegates the full terminal operation; it does
not duplicate it. `settle` dispatches the two existing fence kinds, with one
tenant transaction at each queue boundary. Merely counting these private stages
as separate workflows would misidentify the rejected R4 defect.

The result route reuses `ResultPayloadBuilder::build_as` (`results.rs:318`),
the selected result ID, existing ordered output/summary rows, and the existing
Scribe result ACK (`scribe_outbox.rs:1878–1907`). The common route therefore does
more than share `RunState::fail`: selected pending delivery now reaches result
ACK before retirement. Repeated delivery can stage another batch with the same
logical result ID; generic same-sealed-batch replay dedup remains its existing
owner. No new exactly-once batch contract is inferred here.

## Findings

### MNT-R5-001 — VIOLATION: changed fallible SQL owner lacks required error documentation

**Location:** `crates/wyrd/wyrd-sql/src/queries/verifier_executions.rs:3397`,
`VerifierExecutionQueue::settle_delivery`.

**Rule:** root AGENTS requires rustdoc on every new/changed Rust item and
`# Errors` on fallible items; `agent_docs/rust.md` explicitly includes private
helpers. Mandatory changed-item documentation is not waived by the review's
materiality threshold. This continues the FIND-TASK-001-23 gate rather than
introducing a style-only review scope.

**Evidence and consequence:** the new helper is the shared mutation owner for
`delivered` and `redeliver`. Its only documentation is “Apply
`SETTLE_DELIVERY_SQL` to `pending`'s claim.” Its full body awaits SQL with `?`
and returns `Result<Settlement, SqlxError>`, but has no `# Errors` section.
The caller docs cover their public errors; they do not document the new private
item as explicitly required. The same changed-surface pass finds that new local
`Counted` implementations `state.rs:410` (`Write::write`) and `:414`
(`Write::flush`) have no item rustdoc; their always-successful behavior is easy
to document without inventing error cases.

**Smallest correction:** add local rustdoc describing the delivery helper's
database error and caller-owned transaction/fence role, and document the two
new counting-sink methods' saturating count/no-buffer behavior and infallible
implementation. Keep the existing owners, behavior and types. No new checker,
runtime test or refactor is needed. Verify the bounded changed-item source and
applicable existing format/lint gates; batch with behavioral/proof correction.

### MNT-R5-002 — MISSING: T6 still does not exercise a hosted dependent consuming accepted predecessor results

**Location:**
`crates/wyrd/wyrd-server/tests/integration/pg_durable_verifier_work.rs:3012–3107`,
`concurrent_settlements_release_their_hosted_dependent`.

**Obligation:** original TASK-001 Revision 1's A/B→C observation and R4 scenario
6 explicitly require hosted predecessors settling concurrently while ACK is
held and the real dependent consuming both only after accepted coverage. R5's
acceptance table and verification instructions retain that obligation. R3's
verdict/validation explicitly carried this missing observation forward.

**Evidence:** the test begins `left`/`right` directly on RunState, fabricates
their requirement settlements with `judged`, and joins direct
`writer.settle(...)` calls behind `HeldRevisions`. After release it checks the
accepted state, then explicitly calls `writer.begin(&combine, 1)`. No hosted
engine executes, no dependency resolver releases C, and C never consumes either
value. Its restart assertion checks restored adapter entries only. A hosted
dependency-release bug could therefore pass this test unchanged.

**Materiality:** this is a named, previously open acceptance proof for the
concurrency refactor, not a speculative runtime defect or demand for broader
testing. The recorded GREEN for T6 proves the assertions it contains, but
cannot close the actual required observation. The state-level assertions remain
useful and need not be discarded.

**Smallest correction:** extend an existing hosted/checkpoint integration test
using the existing engine and held-ACK fixture so real A/B settlements are
recorded while ACK is held, C's execution consumes both accepted results, and
restart restores the values without repeating settled calls. No production
change is warranted unless that focused observation fails; no new harness or
performance project is required. Record and run the exact focused selector.

## Carry-forward and bounded acceptance

| Obligation | Source/evidence retained or checked | Result in this scope |
|---|---|---|
| Shared terminal ownership and existing-owner reuse | Runner call paths above; P1/P2/P4/P5 assertions and appended R5 exact RED/GREEN evidence | PASS ownership |
| Removed SQL execution-head, generic batch fences, ordinary publication/File and security owners, provider/platform boundaries | R3 verdict/validation retained rows; this delta adds no corresponding store/provider/platform workflow | Carried PASS within original recorded limits |
| Short locks, one pending revision, shared waiters and exact ticket coverage | Current `RunState::durable:892–947`, StateScribe write, R3 validation and unchanged outbox lane/ACK owner | Carried PASS; changed deadlines inspected |
| Original FIND23 three items | `Checkpoint::prepare:475`, `execution_state:2318`, `Leased::writer:164` now have actual Errors/Panics docs | Old locations CLOSED; new-item gate remains MNT-R5-001 |
| FIND22 complete variable payload charge and stable release | `Checkpoint::charge:406` counts full values without retaining serialized bytes; stored weight is unchanged by OnceLock preparation; T5 asserts variable growth, byte refusal and released charge | PASS scoped source/assertions; R5 records fresh GREEN |
| FIND26 protobuf retirement | `wyrd.v1.proto:143–144` reserves all old slots plus 10/`state`; unchanged HEAD bytes after rebuild; R5 records codegen GREEN | PASS |
| FIND16 metadata | Frozen delivery identity preserves group/owner; T3 retains authorized read and negative caller assertions | PASS scoped source/assertions; R5 records fresh GREEN |
| FIND19 capacity proof | `execute`/`record` reservation refusal → Defer; exhausted attempt maps Defer to LeaseLost; P3 exercises source outage only | Capacity observation explicitly absent; lifecycle specialist must reconcile required proof, no capacity PASS claimed here |
| Original hosted-dependent observation | Full T6 body described above | FAIL MNT-R5-002 |

No unrelated ownership redesign or optional maintainer refactor is proposed.
Convergence: R5 removes the rejected dual-path orchestration; the remaining
items here are one mandatory documentation completion and an incomplete
original proof, not a newly expanded runtime contract.

Skipped Cargo execution and broad rediscovery, as assigned. Recorded test/gate
results are evidence supplied with the pinned packet, not independently rerun
by this reviewer; runtime lifecycle/data conclusions belong to their selected
specialists.
