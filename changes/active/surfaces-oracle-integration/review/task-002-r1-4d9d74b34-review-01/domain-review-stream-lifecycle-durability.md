# Domain Review: Stream Lifecycle, Concurrency, Settlement, and Durability

## Immutable subject

- Repository: `/tmp/wyrd-task-002-r1-review-4d9d74b34`
- Base: `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`
- Candidate: `4d9d74b34803b3f27d55f740a5b40ffbe968b306`
- Approved specification: `changes/active/surfaces-oracle-integration/spec.md`, revision 7
- Original task: `changes/active/surfaces-oracle-integration/tasks/TASK-002-converge-client-and-sdks.md`
- Prior review and remediation: `changes/active/surfaces-oracle-integration/review/task-002-f66a33769-review-01/`
- Candidate identity was rechecked after inspection and remained `4d9d74b34803b3f27d55f740a5b40ffbe968b306`; the immutable worktree was clean.

## Material findings

No material findings.

## Reviewed boundary

This review traced the cumulative client-side Bifrost write and query lifecycle from public Rust/Python/TypeScript callers through `wyrd_client::Bifrost`, `WriterPool`, `Producer`, `SealedBatchSender`, `QueryClient`, `RawQueryStream`, `QueryResultStream`, and the HTTP framing decoder. It covered producer admission versus shutdown, producer-local enqueue/drain ordering, direct Arrow batch settlement, bounded decoded Arrow ownership under coalesced transport chunks, schema/batch/terminal ordering, failed-terminal clean-EOF proof, broken-stream settlement, and public lifecycle routing.

The adjacent direct Arrow path at `crates/shared/wyrd-client/src/bifrost/handle.rs:182-211` does not create the prior insert/shutdown defect: it is an awaited caller-owned operation that returns only after its own durable acknowledgement and is explicitly outside the buffered producer drain. Shutdown closes later direct writes through the same `closed` state; it does not claim to cancel an independently awaited request that already began. No additional coordination owner is required by the approved contract.

## Authority and source coverage

| Authority | Obligation reviewed | Source/caller coverage | Result |
|---|---|---|---|
| `AGENTS.md` §§4-6, 11-12 | Cohesive owner methods, bounded concurrency, explicit async IO, focused verification, no weakened gates | `bifrost/{facade,handle,query,blocking,mod}.rs`; `wyrd-queue/{producer,sealed_sender,sink}.rs`; focused tests | PASS |
| `architecture/agent-rules.md` | Struct-centered ownership, bounded async behavior, exact test selection | `Bifrost`, `WriterPool`, `Producer`, `QueryResultStream` and their tests | PASS |
| `architecture/bifrost-design.md` | One public Bifrost facade, bounded stream/buffer ownership, explicit terminal, structured cancellation and exactly-once release | facade routing, encoded pending frames, Arrow decoder, stream settlement, producer drain | PASS |
| `architecture/references/domain/arrow-analytical-interop.md` | `RecordBatch` is the bounded ownership unit; EOF, transport loss, cancellation, and success remain distinct | `RawQueryStream::next_decoded_frame`, `decode_frame`, `QueryIpcDecoder`, coalesced-chunk proof | PASS |
| `architecture/references/domain/analytical-operations-reliability.md` | Reject new work before closed ownership, preserve acknowledged authority, drain admitted buffered work, retain explicit settlement | `WriterPool::{producer_for,insert,shutdown}`, `Producer::{enqueue,shutdown}`, direct batch sender | PASS |
| `architecture/references/languages/rust-core.md` | Concrete owners, narrow async IO, explicit ownership and error paths | all materially changed Rust lifecycle items and callers | PASS |
| `architecture/references/languages/testing-workflows.md` | Focused deterministic regression proof plus retained public journeys | three remediation tests and recorded Rust/Python/TypeScript/CLI journeys | PASS |
| Spec REQ-019, REQ-020, REQ-021, REQ-056, REQ-060, INV-005, INV-024, AC-004, AC-021; TASK-002/TASK-002-R1 | One facade; preserve bounded Arrow ownership, sink settlement, backpressure, drain, terminal uniqueness, and lifecycle durability | public SDK/CLI/test callers and complete lifecycle owners above | PASS |

## Prior-finding closure

| Finding | Source evidence | Verification evidence | Result |
|---|---|---|---|
| `FIND-TASK-002-10` | `WriterPool::producer_for` acquires the producer-map mutex before reading `closed` at `handle.rs:319-351`; `WriterPool::shutdown` sets `closed` and snapshots under that same mutex at `handle.rs:266-287`. Any returned producer is in the snapshot, while `Producer::enqueue` and `Producer::shutdown` serialize the remaining enqueue/drain race through producer state and channel closure at `wyrd-queue/src/producer.rs:650-724,808-919`. | Recorded focused PASS: `bifrost::handle::tests::shutdown_and_producer_admission_share_the_pool_lock`; the test fails the former pre-lock close transition and proves admitted data drains with zero retained producer/dynamic ownership. | PASS |
| `FIND-TASK-002-11` | Every terminal now reaches `QueryResultStream::finish_at_clean_eof` at `query.rs:713-830`. The helper retains and settles a failed terminal only after clean EOF; any following frame, body error, or deadline expiry marks the stream broken. A cancelled EOF check is safely resumed from the provisional raw terminal at `query.rs:714-721`. | Recorded focused PASS: `bifrost::query::tests::failed_terminal_requires_clean_eof`, covering duplicate terminal, trailing batch, and trailing schema; the existing clean failed-terminal test remains present at `query.rs:2618-2640`. | PASS |
| `FIND-TASK-002-12` | `RawQueryStream::pending` holds `WireQueryStreamFrame`, not decoded batches, at `query.rs:399-417`; `next_decoded_frame` pops one frame before `decode_frame` advances the stateful Arrow decoder at `query.rs:459-555`. Coalescing therefore cannot materialize more than the one batch being returned. | Recorded focused PASS: `bifrost::query::tests::coalesced_chunk_decodes_one_batch_per_delivery`, which supplies schema, sixteen batches, and terminal in one body item and checks delivery order, decoder progress, row count, and terminal retention. | PASS |

Adjacent public authority is also closed: `bifrost/mod.rs:41-59` keeps the query module private and exports neither `QueryClient` nor `RawQueryStream`; async facade methods are at `facade.rs:480-591`, blocking counterparts at `blocking.rs:191-249`, and CLI, Python, TypeScript, and Wyrd test callers route through those `Bifrost` methods. The two compile-fail doctests plus `bifrost::sdk::bifrost_owns_the_complete_query_lifecycle` are the recorded focused proof.

## Verification limits

- The remediation record reports all three focused lifecycle tests passing, along with `fmt`, workspace lints, Rust/Python/TypeScript/CLI journeys, and the original TASK-002 preservation lanes.
- An independent focused rerun was attempted through the repository-pinned `mise` toolchain but did not begin because another process held Cargo's shared build-directory lock; it was cancelled without executing tests. This review therefore relies on source validation and the task-recorded focused results.
- Per TASK-002/TASK-002-R1, the Bifrost aggregate lane was intentionally not run. No missing aggregate substitutes for a required focused proof in this reviewed domain.

## Overall result

**PASS**

The cumulative candidate closes `FIND-TASK-002-10`, `FIND-TASK-002-11`, and `FIND-TASK-002-12` without weakening adjacent write durability, backpressure, query settlement, or public facade ownership. The validated finding ledger for this domain is empty.
