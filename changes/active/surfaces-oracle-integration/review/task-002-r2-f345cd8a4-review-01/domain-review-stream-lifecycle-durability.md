# Domain Review: Stream Lifecycle, Durability, and Resource Settlement

## Immutable subject

- Repository subject: `/tmp/wyrd-task-002-r2-review-f345cd8a4`
- Cumulative base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/TASK-002-R1-close-review-findings.md`
- Current remediation: `changes/active/surfaces-oracle-integration/review/task-002-r1-4d9d74b34-review-01/TASK-002-R2-close-r1-review-findings.md`
- Candidate identity was rechecked before this report was written and remained `f345cd8a4fdb5566ae6828ffb4f29d64f7f17599`.

## Reviewed boundary

This review traced the cumulative client write and query lifecycle through
`wyrd_client::Bifrost`, `WriterPool`, `SealedBatchSender`, `Producer`, the
length-delimited query decoder, `QueryResultStream` settlement, the HTTP body
drop boundary, and the Python, TypeScript, CLI, and test consumers. It covered
bounded Arrow ownership, direct and buffered write admission, concurrent
shutdown, producer drain, query terminal uniqueness, broken-stream settlement,
foreign-runtime close paths, and the focused lifecycle evidence retained from
the two prior review cycles.

The R2-only commits do not modify the production query state machine,
`WriterPool`, `SealedBatchSender`, or foreign-runtime stream owners. The only
post-R1 queue change is a test timing correction in
`wyrd-queue/src/producer.rs`; it does not alter production settlement behavior.
The findings below are cumulative candidate gaps, as required by remediation
review, rather than regressions introduced by the R2 error/docs work.

## Authority and source coverage

| Authority | Obligation reviewed | Source and caller coverage | Result |
|---|---|---|---|
| `AGENTS.md` §§ 2, 3, 5, 6, 9, 11, 12 | One SDK-facing owner, bounded concurrency, explicit durable lifecycle, and direct journey proof | `bifrost/{facade,handle,query,blocking,mod}.rs`, queue producer/sender, SDK/CLI consumers and tests | FAIL |
| `architecture/agent-rules.md` | Struct-owned workflows, bounded async behavior, exact focused tests, and no weakened gate | `Bifrost`, `WriterPool`, `Producer`, `SealedBatchSender`, `QueryResultStream` | FAIL |
| `architecture/bifrost-design.md` | Bounded ownership; shutdown closes admission and drains admitted work; cancellation and ownership release settle exactly once | Direct and buffered write paths, stream settlement, HTTP body-drop propagation | FAIL |
| `architecture/references/domain/arrow-analytical-interop.md` | `RecordBatch` is the bounded unit; cancellation releases owners exactly once; malformed completion cannot become success | direct batch sender, raw decoder, terminal promotion, query settlement | FAIL |
| `architecture/references/domain/analytical-operations-reliability.md` | Admission is bounded; shutdown closes admission before draining admitted work; success cannot hide unsettled ownership | writer budget, producer registry, direct send, shutdown, metrics | FAIL |
| `architecture/references/domain/olap-serving.md` | Stream bounded batches with one explicit terminal and never expose failed/truncated output as success | HTTP adapter, raw/client terminal validation and stream tests | PASS |
| `architecture/references/languages/rust-core.md` | Concrete lifecycle owners, narrow async IO, explicit cancellation behavior, and focused proof | all materially changed Rust owners and their callers | FAIL |
| `architecture/references/languages/testing-workflows.md` | Focused deterministic regression proof plus public journeys | prior focused tests and current task evidence | FAIL |
| Spec `REQ-019`, `REQ-020`, `REQ-021`, `REQ-056`, `REQ-060`, `INV-005`, `INV-024`, `AC-004`, `AC-021`; TASK-002 | One facade preserving direct/buffered Arrow ownership, drain, shutdown durability, terminal validation, and bounded settlement | complete client owner and public runtime paths above | FAIL |

## Prior-finding closure

| Finding | Source evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-002-10` | `WriterPool::producer_for` and `WriterPool::shutdown` serialize buffered-producer admission and the shutdown snapshot under the producer-map mutex at `crates/shared/wyrd-client/src/bifrost/handle.rs:266-287,319-351`. | Recorded focused PASS: `bifrost::handle::tests::shutdown_and_producer_admission_share_the_pool_lock`. | PASS |
| `FIND-TASK-002-11` | Every success, degraded, or failure terminal is provisional until `finish_at_clean_eof` proves EOF at `crates/shared/wyrd-client/src/bifrost/query.rs:713-830`. | Recorded focused PASS: `bifrost::query::tests::failed_terminal_requires_clean_eof`. | PASS |
| `FIND-TASK-002-12` | `RawQueryStream::pending` retains encoded wire frames and decodes only the frame popped for delivery at `crates/shared/wyrd-client/src/bifrost/query.rs:400-555`. | Recorded focused PASS: `bifrost::query::tests::coalesced_chunk_decodes_one_batch_per_delivery`. | PASS |

The private query mechanic remains behind the public `Bifrost` facade, and the
HTTP body adapter still drops Oracle guards without a buffering task. No prior
lifecycle finding was reopened.

## Material proposed findings

### STREAM-R2-001 — INCORRECT: shutdown does not coordinate admitted direct Arrow writes

- **Violated obligation:** Spec `REQ-019`, `REQ-060`, `AC-004`, and `AC-021`
  require direct Arrow writes and flush/shutdown durability to preserve Oracle's
  bounded ownership, sink settlement, cleanup ownership, and drain behavior.
  The Bifrost reliability authority requires shutdown to close admission before
  draining admitted work.
- **Location:** `crates/shared/wyrd-client/src/bifrost/handle.rs:199-211` and
  `266-287`; public caller at
  `crates/shared/wyrd-client/src/bifrost/facade.rs:331-339`;
  `crates/shared/wyrd-queue/src/sealed_sender.rs:75-114`.
- **Evidence:** `WriterPool::write_batch` performs an atomic `closed` read and
  then independently encodes, reserves live-batch/byte ownership, and awaits
  the sink. `WriterPool::shutdown` sets `closed`, snapshots only buffered
  producers, and returns after draining that snapshot. No owner serializes the
  direct-send admission transition with shutdown or tracks an already admitted
  direct sender. A direct write can therefore read `closed == false`, pause,
  let an empty-pool shutdown return success, then reserve bytes and send after
  the terminal shutdown result.
- **Observable consequence:** `Bifrost::shutdown` can return `Ok(())` while an
  admitted direct Arrow batch still owns bytes/live-batch capacity and has not
  reached its durable acknowledgement. That batch may acknowledge only after
  shutdown returned, so the client reports a terminal drain while admitted
  work remains unsettled. None of the located direct-write tests races the send
  against shutdown.
- **Required testable correction:** Coordinate direct-send admission and
  in-flight ownership in the existing `WriterPool` lifecycle so shutdown first
  refuses new direct sends and then waits for every already admitted direct
  send to settle before success. Preserve the existing direct one-batch/one-ID
  sender, byte budget, retry behavior, and buffered producer drain. Add one
  deterministic stalled-sink test proving either a racing direct send is
  refused or shutdown waits for its acknowledgement, and that successful
  shutdown leaves zero live batches and dynamic bytes.

### STREAM-R2-002 — INCORRECT: a malformed healthy drain makes settlement re-enterable

- **Violated obligation:** `architecture/bifrost-design.md` requires
  cancellation and ownership release exactly once. `QueryResultStream::settle`
  itself promises that a second call is a no-op and cancellation is issued at
  most once; spec `REQ-056` and TASK-002 require bounded lifecycle settlement
  for dropped or broken streams.
- **Location:** `crates/shared/wyrd-client/src/bifrost/query.rs:787-795` and
  `900-976`; incomplete-exit proof at `query.rs:1394-1480`.
- **Evidence:** `settle` first replaces `Healthy` with `Settled`, sends one
  cancellation, and drains the body. If that drain discovers a malformed or
  post-terminal frame, `next_batch -> mark_broken` writes `Broken` back into
  the stream. `settle` then completes its status proof but never restores
  `Settled`. Calling the public method again swaps out `Broken`, sends another
  cancellation, and repeats the status loop. The existing test proves two
  calls after a *clean* healthy drain and one call after an already-broken
  stream; it does not cover the reachable `Healthy -> drain failure -> Broken`
  transition inside settlement.
- **Observable consequence:** repeated `settle()` calls on that public Rust
  path issue more than one cancellation request and can repeat bounded polling
  to the deadline, contradicting its at-most-once contract. Server cancellation
  is idempotent, so this does not establish double release, but it does repeat
  lifecycle work that the client explicitly promises to settle once.
- **Required testable correction:** Keep settlement resumable if the settlement
  future itself is cancelled, but make a completed drain-failure/status path
  terminal in the existing `QueryResultStream` owner. Add one deterministic
  test with a healthy stream whose drain encounters a malformed or trailing
  frame, call `settle()` twice, and assert exactly one cancellation and one
  completed settlement cycle.

## Security and performance boundary

No separate security or tenant-isolation defect was found in this domain. Both
findings are bounded lifecycle/concurrency defects. `STREAM-R2-001` can retain
admitted memory and work beyond successful shutdown; `STREAM-R2-002` repeats
bounded network lifecycle work but does not create unbounded polling because
the server-pinned deadline remains enforced.

## Verification limits

- The remediation record reports the three prior lifecycle focused tests,
  shared-client tests, Rust/Python/TypeScript journeys, format, lint, codegen,
  docs, and boundary checks passing.
- This review attempted the three prior exact focused lifecycle tests through
  `mise exec -- cargo nextest run`. Compilation did not complete because the
  isolated worktree's Cargo target fingerprint paths disappeared during the
  build (`No such file or directory`); no selected test executed. The prior
  closure results therefore rely on source validation and the recorded task
  evidence.
- No existing focused or journey test located in the cumulative candidate
  exercises direct `write_batch` concurrently with `shutdown` or invokes
  `settle()` twice after settlement itself discovers a broken healthy body.
- Per TASK-002, the Bifrost aggregate was intentionally not run.

## Overall result

**FAIL**

The candidate preserves the three previously reviewed lifecycle fixes, but the
cumulative task still allows successful shutdown ahead of an admitted direct
Arrow write and allows completed malformed-drain settlement to issue lifecycle
cancellation more than once.
