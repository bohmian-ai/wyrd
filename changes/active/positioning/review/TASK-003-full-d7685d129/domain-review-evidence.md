# TASK-003 evidence and write-integrity review

**Result: FAIL.** Two material findings are proposed: the new TypeScript lazy
sample path omits the required producer span, and the revised retention prose
promises stronger timing and loss guarantees than the existing outbox provides.

## Subject and scope

Read-only source: `/tmp/wyrd-task003-review-d7685d129`, HEAD
`d7685d129ffee870ab1c497cdcb1206a89bc80c4`; reviewed range
`634e19cbc517293ed2458f30ce73401714f3447d...d7685d129ffee870ab1c497cdcb1206a89bc80c4`.
The checkout was clean and its HEAD unchanged at the final source check.
Primary-worktree implementations and tests were excluded. This report is the
only file written by this reviewer. No tests, Cargo commands, benchmark,
source changes, or delegated reviews were run.

Authority: approved positioning revision 7, particularly REQ-017, AC-016 and
INV-001; approved service-identity-and-telemetry revision 17; original
TASK-003 scenarios 2–4 and their truthful evidence/retention requirements;
`architecture/wyrd-design.md` Trace context section (1035–1043) and verification
pipeline (1826–1845); `architecture/bifrost-design.md` outbox ownership and
shutdown (23–27), outbox delivery (1391–1422); and
`architecture/wyrd-security-posture.md` tenant stamping (72–80) and existing
outbox failure semantics (798–839). Read the task-review skill, common scope,
original task, prior substantive verdict, Python directory rules, and maintainer
guide. No `.codegraph` exists, so navigation used source searches.

Prior acceptance is carried forward for unchanged native trace judgment,
result/execution readers, tenant refusal and schedule semantics, after checking
the changed producers and relevant unchanged owners below. The superseded
Rust configure/flush demand is not reapplied: revision 7 explicitly accepts
the telemetry handle shutdown used by the Rust example.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence inspected | Result |
|---|---|---|---|
| First sample/write starts the existing writer; each submitted record is admitted once to its intended table | `state.rs::bifrost_writer` rechecks under the start gate; `observe/mod.rs::record_or_stage` either records and returns None or stages without enqueuing; `finish` writes staged Window features once; `RunBifrost::write_value` passes the resolved destination directly | TS `observation-surface.test.ts`, first-use concurrent submits and record count; burst exact-once test; source inspection, execution owned by root | PASS for evidence/data flow; phase/cancel interleavings belong to invariant/system reviews |
| Run/subject/tenant attribution remains attached to every row | `Run::correlation` carries the current view UID and Run ID; Window `drift_value` and caller `write_value` share `Bifrost::insert_observation`; writer `insert_rows` forwards both; state startup uses the same state client; server `gate/attribution.rs::frame_dimensions` reads descriptors with `auth.tenant` | TS separate-table/view assertion and first-use `subject_card_uid` assertion; prior tenant evidence plus Python investigation foreign-tenant refusal | PASS |
| A queued Bifrost write records a producer span in the caller's own tracer, including the newly supported first sample | Warm path invokes the foreign-runtime callback synchronously; TS staged Window path leaves that scope before async admission | `action-timeline.test.ts` asserts producer spans and ingest links but explicitly calls `startBifrost`; new first-use test asserts rows only | FAIL: EV-FULL-1 |
| Python first sample retains the existing span bridge | Python `PyObserveHandle::submit` wraps the complete `trace::block_on(observe.submit(...))` in `trace::traced`; `with_producer_span` remains scoped while the future is driven and the callback reacquires Python for its own span | Python action-timeline source plus unchanged callback/block_on bodies; no fresh run | PASS |
| Trace model field and mapping lower to canonical trace slots; other fields and bare ID values remain intact | Rust `InputSlot::json` consumes the marker; `fields` resolves field schemas through the existing lowerer; Python Trace schema hook and TS Trace metadata feed it; `verify/mod.rs` converts trace strings to typed TraceId declarations and validates 32 hex digits | Rust `trace_field_declares_a_trace_slot`; Python `test_authoring.py` model/mapping/value test; TS `authoring.test.ts` Trace property/mapping/value test | PASS |
| Actual complete/skipped work and missing trace produce the native distinctions | Refund programs instrument actual lookup and draft branches; complete/skipped replies stay equal; missing-trace case omits the trace slot rather than manufacturing an empty trace | Python actual-program test asserts exact complete and skipped span lists and captured output; Rust/TS actual-program journeys inspect outputs; Python native whole-trace/late/missing tests remain | PASS |
| Investigation reads exact versions and retained original evidence, handles delayed visibility and native unknown/foreign errors without rejudging | `investigate.{py,rs,ts}` use public Bifrost SQL then Execution details with bounded visibility polling; unchanged server result builder carries exact verifier/subject/run/owner and inputs | Python actual-program investigation asserts stored original reply and same 404 code for unknown and foreign execution; Rust/TS actual-program output assertions; prior accepted source-backed coverage | PASS |
| Published storage claims accurately separate a returned verdict from eventual retention and all supported loss paths | Changed conceptual/FAQ prose gives 50 ms / clean-shutdown / abrupt-stop guarantees stronger than unchanged Scribe outbox | Existing outbox shutdown-deadline test and failure implementations directly contradict the new text | FAIL: EV-FULL-2 |
| Ordinary registered-table namespace, grants, tenant and native error contracts remain | `write_value` still rejects non-dataset names before admission, uses existing authorized resolver; no auth/SQL/storage implementation changed in the range; TS NativeSubmission still projects WyrdError via `NativeWyrdError::from_wyrd`, Python retains WyrdPyResult | TS refused-emit codes; actual investigation foreign 404; source dependency check | PASS within existing boundary, without a new standalone security/storage audit |

## EV-FULL-1 — First TypeScript window samples skip the caller's producer span

Classification: **REGRESSION / INCORRECT**. Location:
`sdks/wyrd-sdk-ts/native/src/cards.rs:1074–1081`, coupled to
`crates/shared/wyrd-client/src/observe/mod.rs:437–443,477–479`.

The owning architecture states that a queued Bifrost write records a
`bifrost.enqueue` PRODUCER span in the caller's own tracer and the ingest
consumer links that producer (`wyrd-design.md:1041–1042`). REQ-017 adds first-use
automatic startup; INV-001 preserves existing telemetry/data contracts.

Reachable path: configure the state normally, enter an active Node span, and
call the first `run.observe.submit` with Window samples without startBifrost.
NativeRun runs `observe.stage` inside `trace::traced` on the JS thread. The
writer is not running, so stage validates and returns Window features without
calling `drift_value`. The producer callback's synchronous scope then ends.
`trace::spawn` captures only the caller context and asynchronously calls
enqueue_staged → finish → bifrost_writer → drift_value →
insert_observation → writer.insert_rows. At that last step,
`transport/trace.rs::producer` finds no PRODUCER callback and returns CALLER
directly. It never invokes Node's span opener. Every concurrently staged
first sample follows the same path.

The row still lands with the correct run/subject/trace and the consumer may
link the caller span, so this is not a claimed tenant or row-loss defect.
The required exported enqueue operation and producer-specific causal link
are absent for a normal newly supported operation. This affects user-visible
evidence, rather than an optional naming preference. The next warm submit
uses the JS callback and has a different evidence shape. Python does not
have this split: its callback scope encloses the whole block_on call.

Sibling producers checked: warm TS `stage`, Python submit, Rust submit,
caller-table writes, `Bifrost::insert_observation` and
`BifrostWriter::insert_rows`; the foreign span opener lives in TS
`native/src/trace.rs` and the shared selection owner is
`transport/trace.rs::producer`. The existing TS timeline test starts the
writer explicitly, so its >=2 producer assertion cannot detect this case.
The new observation first-use test only counts retained data.

Smallest correction: preserve the existing Node producer-span bridge across
the staged Window path. Capture a producer context through the existing
JS-thread callback at the synchronous boundary and carry that context to
the actual admission, without invoking Node from a Tokio thread, adding a
second tracing/provider owner, or changing Run/subject correlation. Keep
warm synchronous admission and one producer per observation. A focused
TypeScript native-runtime test should submit a first sample and a warm
sample inside a real active span, then inspect their producer spans and
ingest links as well as their distinct retained rows; include two concurrent
first samples. Reuse the existing action-timeline/observation harness. This
is a bounded bridge correction, not a reason for new lifecycle machinery.

## EV-FULL-2 — Retention prose promises an unconditional 50 ms write and loss-free graceful shutdown

Classification: **INCORRECT**. Locations:
`docs/src/content/docs/understand/how-verification-works.md:190` and
`docs/src/content/docs/understand/technical-questions.md:53`, also projected
in `docs/public/llms-full.txt`.

Original TASK-003 scenarios 1 and 4 require truthful distinction between the
returned verdict and retained visibility; the approved positioning task does
not authorize changing persistence guarantees. The existing outbox authority
explicitly retains retry, terminal-rejection, unreachable and expired
shutdown-deadline outcomes (`wyrd-security-posture.md:832–839`;
`bifrost-design.md:1420–1422`).

The changed text says records batch for at most 50 ms and then write to the
WAL, that a clean shutdown writes everything still queued, and that only an
abrupt stop can lose a record. Actual direct verification reserves staging
slots before running and `components/verification/service.rs:920–935` stages
its inputs and Result on the ordinary Scribe outbox under the caller tenant.
The following ordinary failure path disproves the new guarantee: return a
direct verdict, make Scribe temporarily unavailable while the result remains
in memory, and initiate graceful shutdown. `Outbox::shutdown` abandons and
counts items still unwritten at the deadline (`outbox.rs:625–674`); its existing
`shutdown_counts_items_unwritten_at_the_deadline_as_lost` test asserts exactly
that behavior. This needs no abrupt process crash. Separately,
`ScribeSink::tally` consumes terminally rejected frames as loss
(`scribe_outbox.rs:1776–1779`), and `write` consumes unreachable-Scribe writes
as counted loss (`1890–1893`). These apply to the same ScribeWrite::Result
owner, not a hypothetical unrelated audit outbox.

50 ms is the lane's configured batching linger, not a durability or total
queue-time bound. `OutboxWriter::is_ready` also waits for the lane's in-flight
slice and retry backoff (`outbox.rs:1136–1144`), `dispatch` waits for available
concurrency, and Scribe submission can retry; work behind an unavailable
Scribe remains unacknowledged beyond that linger. Thus even a running process
can exceed the implied write bound before visibility. The WAL survival
boundary itself is not challenged.

Consequence and materiality: the reader is being taught when original
verification evidence is recoverable. The new claims can cause an operator
to regard graceful shutdown or a 50 ms delay as sufficient proof of
retention and explain an absent audit trail incorrectly. This is central to
the task's evidence tutorial, not speculative hardening of the server.

Smallest correction: change only the two prose owners to describe the 50 ms
batching linger without guaranteeing total write time; retain the real
acknowledgement/visibility distinction and deadline/failure loss caveat.
Regenerate the existing derived llms content. Do not add a new persistence
guarantee, retry layer, or shutdown mechanism to make the wording true.
Source comparison against the existing shutdown-loss test and Scribe failure
branches plus the narrow docs/generated checks is sufficient closure proof;
no new engine test or server implementation is needed.

## Verification limits

This is independent source review, not fresh execution evidence. Root owns
the supplied/fresh checks and any required benchmark assessment. Startup
network/schema waits occur on first writer startup; the inspected warm
sample path retains synchronous bounded admission and adds no engine wait.
No server auth, SQL, persistence implementation or migrations changed in
the reviewed range, so no additional sensitive-boundary specialist is
required by these findings. Existing native judgment/retention tests were
read as evidence and not rerun.

Skipped: test/benchmark execution and unrelated lifecycle interleavings.
Risk: ordinary TypeScript first samples lack required producer evidence, and
the published retention text overstates the existing durability boundary.
