# R3 trace evidence domain review

**Result: PASS within the assigned trace boundary.** FIND-TASK-003-5 is closed by source and focused proof inspection. No material finding is proposed.

## Subject, authority and limits

Reviewed HEAD `908a169460550bba10eecfd2758fa9deddd9d4cc` against R3 base
`7bb44f2dff1994499e575c27f0be5568dcc2d167`. Inspected the complete R3 diff,
with targeted source tracing of the producer handoff and
`d7685d129..7bb44f2df` lifecycle dependency seam. HEAD remained the candidate;
the implementation paths inspected matched its Git contents. Dirty planning,
mockups and unrelated packets were excluded. Original-task and R3-task Git
blob hashes match the common scope's `6846032793910170e76c0cc0ec9de2f50cb6ad6f`
and `a0e6da2f93e223a7276607dd0e8985b4291c6b76` pins.

Authority: approved positioning revision 7 REQ-017/AC-016 and evidence
invariants; approved service-identity-and-telemetry revision 17; original
TASK-003 and R3 remediation; `architecture/wyrd-design.md:1035–1043` and
`:1071–1077` for caller-owned producers and linked consumers; root and SDK
directory rules, task-review skill, repository workflow, Rust and maintainer
guides. In particular, the architecture requires a queued write's producer
and its server consumer link; it does not state that every attempted producer
must correspond to an accepted or durable record.

Carried forward the prior full review's source-backed native judgment,
Trace-slot authoring, result-reader, tenant/attribution, provider and lifecycle
coverage from `../TASK-003-full-d7685d129/domain-review-evidence.md`,
`findings-validation.md`, `task-review-invariants.md` and `system-review.md`.
The changed handoff does not enter those durable/server owners. This report
does not repeat the tutorial review or evaluate FIND-2/FIND-6 wording.

## Producer-to-consumer trace

`NativeRun::submit` (`native/src/cards.rs:1066–1088`) still validates inputs
and groups the view before calling `observe.stage` synchronously inside
`trace::traced`. The existing JS-thread callback
(`wyrd/src/index.ts:2247–2275`, `native/src/trace.rs:58–95`) opens and ends a
Node PRODUCER span under the active Node span, returning only parsed owned
`TraceContext`. No V8 callback or handle crosses into the spawned future.

For a cold Window, `observe/mod.rs:437–460` checks lifecycle and validates
features before opening that producer. `Evidence::Window` stores the features
and immutable optional context. `finish` (`:484–495`) awaits the existing
writer owner, then scopes the stored context through the existing
`with_producer_span` mechanism only around `drift_value`. Its callback returns
the captured context; it does not open a second span. Even stored `None` is
returned as such rather than falling back to a later, unrelated ambient span.

`drift_value` (`:231–249`) still constructs one record and passes its tall rows
and original Run correlation through `Bifrost::insert_observation` to
`bifrost/handle.rs:205–219::insert_rows`. The shared producer selector obtains
the saved context there. `wyrd-queue/src/producer.rs:515–585` performs the same
preflight and all-or-none admission, charges the trace bytes and shares the
context across the record's rows and eventual frame. No new row path, tracer,
queue, identity resolver or writer exists.

Startup runs before the replay scope, under `trace::spawn`'s independent caller
context (`native/src/trace.rs:78–86`). Thus startup/network requests continue
the caller span rather than becoming children of an ended enqueue span. Warm
submission still calls `drift_value` synchronously in the original Node scope;
Run evidence still follows the unchanged file pin/queued execution branch.

Python's `PyObserveHandle::submit` (`src/observe/mod.rs:440–460`) keeps the whole
shared submit inside `trace::traced` and blocking caller-context scope. Its
producer now opens before cold startup and the same stored context reaches
admission; Python lookup and span creation remain on its existing bridge.
Rust submit captures at synchronous staging in the poll's current tracing
context, then reuses that context across the await. Neither sibling acquires
a new runtime or provider.

## Scoped acceptance matrix

| Obligation | Source evidence | Proof inspected | Result |
|---|---|---|---|
| Cold and warm Window samples each retain a caller-owned producer and linked ingest consumer | Staging/context replay above; warm path unchanged | TS action-timeline now omits explicit startup, names `sample.cold` and `sample.warm`, requires exactly one producer under each parent, and requires a matching `bifrost.gate.write` CONSUMER link for every producer | PASS |
| Samples remain distinct; concurrent first samples retain row admission | One `drift_value` per staged Window; same correlation and atomic queue owner | Timeline requires two distinct `record_id` values for the model Run. Existing observation-surface first-use test concurrently submits two samples and requires two distinct records plus exact written subject | PASS |
| Startup uses caller context; foreign callbacks stay off Tokio workers | `trace::spawn` retains CALLER; replay scope begins after startup and contains only owned context | Full native trace/stage/spawn and shared finish bodies inspected; timeline also checks connected request ancestry | PASS |
| Warm, Run, Python and Rust paths retain their existing owners | Warm early return and Run branch unchanged; Python bridge encloses shared call | Source inspection plus supplied Python timeline and shared-client regression evidence | PASS |
| Errors/cancellation never fabricate row acceptance | Validation precedes producer capture; startup error propagates before `drift_value`; queue admission remains fallible | Existing typed outcome path and claim/drop owner inspected; no new error handling or acknowledgement branch | PASS |
| No new server hot-path wait | New operations are local span capture and owned context replay | Prior system wait inventory carried forward; actual R3 source adds no awaited IO or changes to verification/gateway/Oracle/Scribe owners | PASS |

The R3 task records an expected RED for missing `sample.cold` producer, then
GREEN for the exact timeline selection, observation-surface (7), Python
timeline (1), shared client library (290), and relevant checks. These are
supplied execution claims, not fresh runs by this reviewer. The assertion
changes directly exercise the diagnosed gap: unrelated table-write producers
cannot satisfy the per-sample parent checks. The separate concurrent test is
adequate retained-row regression coverage; another cross-language concurrency
trace matrix is not required by this correction.

## Failed-start attempted span assessment

A valid cold sample can open its producer and then fail startup or have its
Rust future dropped before admission. That producer can lack a stored row or
consumer. This is reachable, but no explicit supported trace/row atomicity
contract is violated. The typed failure remains visible, no `Recorded` result
is returned on startup failure, and no row is fabricated. Node's spawned
future continues independently of merely discarding its promise, as before.

The existing warm `insert_rows` also obtains its producer context before
`enqueue_rows` performs preflight/admission; queue refusal can already leave
an attempted producer without a row. Producer spans are therefore causal
operation evidence, not durable row receipts. Requiring the cold path to erase
an exported span or delay Node span creation onto a Tokio worker would impose
new semantics or break the bridge. No such correction is justified.

Failure/drop still releases an unpublished `StartClaim` through its existing
Drop implementation, while shutdown remains terminal. The prerequisite removal
of NOT_STARTED maps the existing optional/current lookup to the same owner;
`drift_value` is called only after a running check or successful startup, and
concurrent shutdown still returns CLOSED rather than publishing a replacement.
These seams do not invalidate prior lifecycle acceptance.

No material proposal remains. No lifecycle redesign, additional guard, new
tracer or public contract is needed for FIND-TASK-003-5.

Skipped: fresh tests, Cargo, benchmarks and whole-task rediscovery; the
orchestrator owns checks. Risk retained: a failed or abandoned cold submission
may leave an attempted producer span without a row; it is not a durability ack.
