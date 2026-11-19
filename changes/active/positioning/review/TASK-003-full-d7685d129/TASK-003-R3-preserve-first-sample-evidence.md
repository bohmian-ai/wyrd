---
id: TASK-003-R3
kind: remediation
status: proposed
spec: SPEC-positioning
spec_revision: 7
requirements: [REQ-008, REQ-010, REQ-012, REQ-013, REQ-017, INV-001, INV-003, INV-005, AC-007, AC-009, AC-011, AC-012, AC-016]
depends_on: [TASK-003-R2]
parent_task: TASK-003
remediates: [FIND-TASK-003-2, FIND-TASK-003-5, FIND-TASK-003-6]
---

# Preserve first-sample evidence and describe actual retention

Route directly to `$wyrd-implement`. A TypeScript application's first sample
has the same caller-owned enqueue span and linked ingest evidence as later
samples. Published instructions distinguish a returned verdict from retained
evidence without promising unconditional storage or a 50 ms deadline. Batch
the remaining cancellation documentation with these corrections.

## Authority and immutable context

- Approved specification: `changes/active/positioning/spec.md`, revision 7,
  especially REQ-017, AC-016 and evidence/publication invariants.
- Approved telemetry specification:
  `changes/active/service-identity-and-telemetry/spec.md`, revision 17.
- Architecture owners: `architecture/wyrd-design.md` trace context/state
  lifecycle; `architecture/bifrost-design.md` outbox/durability;
  `architecture/operations/runbooks.md` Scribe loss handling.
- Original task:
  `changes/active/positioning/tasks/TASK-003-runnable-verification-tutorials.md`,
  content pin `6846032793910170e76c0cc0ec9de2f50cb6ad6f`.
- Prior remediation/evidence:
  `review/TASK-003-first-634e19cbc/TASK-003-R1-complete-schedule-reader-setup.md`
  and `review/TASK-003-R1-expanded-480251a70/TASK-003-R2-complete-sdk-tutorial-proof.md`
  under the positioning packet. Preserve both and their verdicts/ledgers.
- Original cumulative base: `5775c3795ebd6557d080b34d55dd2a67b8dbad63`.
  Full reviewed range: `634e19cbc517293ed2458f30ce73401714f3447d` through
  `d7685d129ffee870ab1c497cdcb1206a89bc80c4`.
  Prior substantive candidate: `480251a70f799b8581ca5a4be77983d6f345d5e3`.
- This review's independently validated diagnosis, source traces, closure and
  convergence assessment: sibling `findings-validation.md` and `verdict.md`.

Current revision 7 governs older task metadata. In particular, do not add the
superseded Rust configure/state.flush calls to the refund example: its
telemetry handle's shutdown is the approved export barrier.

## Diagnosis and selected correction

### FIND-TASK-003-5: the first TypeScript sample loses its enqueue producer

At `sdks/wyrd-sdk-ts/native/src/cards.rs:1074–1081`, submit runs synchronous
staging inside `trace::traced`. For a cold Window sample,
`wyrd-client/src/observe/mod.rs:437–443` returns deferred evidence without
queueing. That scope ends. The spawned completion carries only caller trace
context (`native/src/trace.rs:78–86`), starts the writer and finally enqueues
the sample at `observe/mod.rs:477–480`.

The shared `transport::trace::producer` no longer has the Node producer opener
in scope, so it returns the caller context instead. No Node `bifrost.enqueue`
PRODUCER span exists, and the ingest consumer cannot link to it. Samples
staged concurrently before startup finishes share this path. Their rows can
land with correct Run/subject/tenant identity; the defect is missing required
causal evidence, not proved row loss. Warm samples still enqueue in the Node
scope. Python keeps its traced scope around blocking completion, and Run
evidence uses the existing queue path.

The architecture requires queued writes to create PRODUCER spans in the
caller's tracer. REQ-017 makes cold sample submission an ordinary supported
path. The existing action-timeline case explicitly starts Bifrost, while the
new first-use test asserts rows only. Neither establishes this causal evidence
for the new path.

Preserve a Node-created producer context at the existing synchronous
staging/trace handoff for deferred Window evidence and apply it to eventual
sample admission. Reuse the existing Node trace source and shared immutable
context plumbing. Keep caller context for startup/network requests and the
producer context for the queued row. No Node/V8 callback may run on a Tokio
worker. Preserve synchronous warm admission, Run evidence/grouping, native
errors, row correlation, all-or-none enqueue and the existing one-writer gate.

The correction belongs at the producer handoff; guards at the server reader
cannot recreate a missing caller span. Moving initialization into synchronous
hot code, eagerly starting all submissions, adding another tracer/writer or
transporting a JavaScript callback across threads would be larger or unsafe.
No lifecycle or provider redesign is warranted.

### FIND-TASK-003-6: retention prose exceeds the actual outbox guarantees

Changed paragraphs at `understand/how-verification-works.md:190`,
`understand/technical-questions.md:53` and
`build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md:40`
under `docs/src/content/docs/` promise only abrupt-stop loss, complete clean
shutdown retention, a write within 50 ms, or treat absence as merely not-yet
written. `docs/public/llms-full.txt` repeats these claims.

The verification service reserves result/input outbox slots before execution
(`components/verification/service.rs:601–605`) and stages without waiting for
storage (`:921–935`). These reservations avoid ordinary unreserved shedding;
they do not guarantee delivery. Server shutdown passes a finite budget to
the outbox (`app/server.rs:828–838`), whose deadline abandons and counts
unwritten rows (`wyrd-runtime/src/outbox.rs:610–675`). Terminal rejection and
unreachable destinations also consume rows as observed loss. A graceful stop
during a Scribe outage can therefore lose an already returned verdict's
record without a crash. The existing shutdown-loss test proves that boundary.

The 50 ms constant is batching linger. Dispatch also depends on same-lane
in-flight work, concurrency and retry backoff; it is no storage/visibility
deadline. Abrupt process death cannot reliably count its lost memory. A
reader relying on these claims could mistake absent proof for guaranteed
future arrival or assume a graceful rollout retained everything.

Maintainer clarification after review: keep the user-facing correction simple.
Preserve the correct WAL durability statement. Use this boundary at the concept
owner and link to it from the other pages:

> Wyrd returns the verdict, then writes its record in the background. Once
> committed to Bifrost's write-ahead log, the record is durable; a server stop
> or write failure before that can lose it.

Remove the fixed 50 ms write promise and unconditional clean-shutdown/crash-loss
counting claims. Do not replace them with queue mechanics, failure-mode lists,
retry/backoff, reservation or shutdown-budget explanations in these user flows;
existing architecture/operations reference owns those details. The investigation
step needs only to say the record may not yet be available and the program
retries for up to a minute, without promising arrival. Regenerate the derived
full text. No runtime, durability, queue, timeout or metric change is authorized.

### FIND-TASK-003-2: three startup/drain items omit cancellation contracts

The previous seven named documentation fixes are closed. The full requested
SDK review found cancellation/progress/retry documentation missing at new
`state.rs:787–801::start_gated`, new
`observe/lifecycle.rs:142–160::flush`, and changed public
`state.rs:1087–1100::flush` in `crates/shared/wyrd-client/src/`.

Root AGENTS makes changed-item rustdoc a merge gate; the Rust guide expressly
includes private helpers and async/durable cancellation, partial progress and
retry. An Errors-only crossreference does not document these semantics.
Abandoned startup releases its unpublished claim unless shutdown has closed
it. Flush delegates to a blocking drain: abandoning the async waiter does not
roll back or necessarily stop that drain, so acknowledgement/progress can
continue.

Add concise documentation at those existing items, using the existing public
starter cancellation contract and actual continuing-drain behavior. Preserve
API, phase/gate, writer ownership and runtime behavior. Three local comments
close this gate; no cancellation framework, new test or state reset is needed.
Document any new/changed Rust items introduced by the producer correction as
required, without adding unrelated comments to untouched code.

## Preserved behavior and non-goals

Keep writerless flush success, closed-state refusal, first-use automatic
startup, explicit custom startup, default managed configure/opt-out, TS async
configuration and frozen failed installs. Preserve the single lifecycle,
shared client/cache/pool, actual-work/native-insufficiency judgments, exact
retained attribution, authorization and tenant separation.

FIND-1 and FIND-4 remain closed; FIND-3 is superseded by approved revision 7
and its replacement passes. Do not resurrect rejected provider-race/stub
layout proposals or redesign startup. No wire, schema, migration, auth,
dependency, new durable behavior or designated server hot-path wait is
authorized. No full benchmark is required for this bounded bridge/prose/docs
write set unless implementation actually crosses a designated engine boundary.

The primary checkout has concurrent uncommitted changes not included in
reviewed d7685d129. Preserve others' edits; do not revert or silently treat them
as accepted candidate fixes. Keep the next review subject unambiguous. Commit
only when asked.

## Acceptance and focused proof

| Finding | Required closure |
|---|---|
| FIND-TASK-003-5 | A valid cold Window sample submitted from an active Node span creates its own Node enqueue PRODUCER and linked ingest CONSUMER; a later warm sample does too. Distinct sample records and existing concurrent-first-row behavior remain correct. |
| FIND-TASK-003-6 | Source paragraphs/generated consumers state the simple before/after WAL durability boundary and bounded inspection behavior, without fixed write deadlines, unconditional retention/future-arrival claims or internal queue explanations. |
| FIND-TASK-003-2 | The three existing operations document actual cancellation/progress/retry; previously fixed sites stay fixed. |

For FIND-5, extend the existing real-server TypeScript action-timeline journey
to include cold and warm samples. Establish RED for the missing cold producer,
then the minimum handoff fix, then GREEN. Identify each sample's producer and
consumer link: a loose global producer count satisfied by an unrelated table
write is insufficient. Assert both logical sample records remain distinct.
Reuse existing fixtures and trace capture; add no new harness or exhaustive
cross-language/concurrency matrix.

Exact focused existing selector:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/action-timeline.test.ts -t '^service action timeline is one connected tree$'"
```

Confirm a nonempty selection and retain the current concurrent-first-write
readback proof. Run only directly affected native/TS type/lint/format checks;
if shared trace plumbing changes, select its affected tests and record exact
commands. Do not require all SDK or full user-journey suites. No Python/Rust
tutorial or every-minute schedule rerun is needed for an unchanged path.

For FIND-6 use source/authority proof, not a new outage test: compare the
paragraphs to the cited reservation, retry, sink and deadline branches and
existing shutdown-loss evidence. Regenerate with `mise run docs:generate`,
then run `docs:test`, `docs:check:commands`, `docs:linkcheck`, `docs:build` and
`docs:a11y` for the affected publication. Inspect the three rustdoc additions
against their owners and run `mise run fmt:check`; comments need no new runtime
tests. Run `git diff --check`. Next review is closure/delta/affected paths with
accepted evidence carried forward, unless new material changes justify a
specifically recorded expansion.

## Implementation evidence

Commits `7894fd172`, `3ad95731b`, `908a16946` on `7bb44f2df` (the
`WYRD_SDK_400_BIFROST_NOT_STARTED` catalog deletion, committed separately
first so the R3 range is unambiguous).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-003-5: cold and warm samples each have their own Node PRODUCER linked to an ingest CONSUMER; records stay distinct | `wyrd-client/src/observe/mod.rs` `record_or_stage` opens the PRODUCER through `trace::producer` at staging (inside Node's `traced` scope on the JS thread) and stores it in `Evidence::Window`; `finish` replays it with the existing `trace::with_producer_span` after the writer starts. Startup requests keep the caller context. No new tracer, writer, or cross-thread JS callback. | RED then GREEN: TS `action-timeline.test.ts` `^service action timeline is one connected tree$` (1 selected; RED had 2 producers, none under `sample.cold`). It now asserts exactly one PRODUCER under each of `sample.cold`/`sample.warm`, a linked `bifrost.gate.write` consumer for every producer, and `COUNT(DISTINCT record_id) = 2` for the run. `observation-surface.test.ts` (7 passed, includes concurrent-first-write readback); Python `test_action_timeline.py` (1 passed); `wyrd-client` lib (290 passed); `lints`, `ts:lints`, `ts:format:check`, `ts:typecheck`, `ts:napi:check` | PASS |
| FIND-TASK-003-6: simple WAL durability boundary, bounded inspection, no fixed deadline or unconditional retention | `understand/how-verification-works.md`, `understand/technical-questions.md`, `build-and-verify/verify/run-a-verification-and-inspect-its-evidence.md`; `docs/public/llms-full.txt` regenerated | Source comparison against `service.rs` reservation/staging, `app/server.rs` shutdown budget and `outbox.rs` deadline abandonment: text now claims only durability after WAL commit and possible loss before it. `docs:generate`, `docs:test`, `docs:check:commands`, `docs:linkcheck`, `docs:build`, `docs:a11y` | PASS |
| FIND-TASK-003-2: cancellation of `start_gated`, `BifrostLifecycle::flush`, `WyrdState::flush` | `# Cancellation` sections at the three items, matching the `StartClaim` drop behavior and the `spawn_blocking` drain in `Bifrost::drain` | `fmt:check`; `cargo doc -p wyrd-client` with `-D warnings` (also fixed three broken links it reported: two private `SLOT_KEYWORD` links in the `Trace` docs, one to the feature-gated `with_benchmark_labels`) | PASS |

Limits: a cold sample whose writer startup then fails has already emitted its
PRODUCER span with no row; the span records the attempted enqueue. Non-goals
held: no wire, schema, migration, auth, dependency, runtime durability, or
server hot-path change; no lifecycle redesign.
