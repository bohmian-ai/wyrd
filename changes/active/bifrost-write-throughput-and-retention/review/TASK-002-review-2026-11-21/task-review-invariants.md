# TASK-002 invariant review

Subject: base `5a057524f07d23a515d505caa4955539926d7fee`, candidate
`c40e7245c3ada4a35177e3a8c2d37c42d058c619`; HEAD matched the candidate
through this review. Approved specification: `spec.md`, revision 3.

Boundary: encoder worker isolation, retained memory accounting, stream-state
isolation, shutdown/cancellation, and inspection ownership. This specialist did
not review the entire task or run Cargo concurrently with the orchestrator.
The maintainer's approval of the Arrow fork and explicit nonblocking benchmark
deferral apply. Performance remains unmeasured, without a performance finding.

Authority: repository AGENTS.md; TESTING.md; maintainer-style.md; specification
REQ-012 and task scenario 3; `architecture/bifrost-design.md` sections “Shards
and hierarchical ownership” and “Resource and failure invariants.” No nested
AGENTS.md was found below the changed Vala/shared ownership trees.

## Coverage and source trace

- `ScribeImpl::build` installs exactly one context slot per existing ingress
  worker (`scribe/mod.rs:1185`, `execution_lanes.rs:187–206`), without adding
  workers. The ingress pool start handler sets a thread-local worker marker;
  callers outside such a worker receive no slot. Production preprocessing
  captures the matching pool's contexts and submits encoding to that pool.
- `preprocess.rs:921–970` removes a context from its slot for one synchronous
  encode. The existing lease stays live locally while the slot is empty.
  Error propagation and unwinding drop that lease and context; an abandoned
  asynchronous waiter does not interrupt the synchronous worker closure.
  `execution_lanes.rs:483–510` catches worker panics and settles the job.
- `preprocess.rs:979–1013` measures completed context retention, rejects
  retention above the configured cap, resizes the existing root lease or
  reserves `Metadata`, and treats ordinary root refusal as “keep nothing.”
  Accounting errors propagate. Clear/restore races are correctly fenced by
  the closed flag and its second check under the slot lock.
- Reviewed the actual pinned dependency source at
  `/home/thorrester/.cargo/git/checkouts/arrow-rs-7086235adca86d7c/f349772/`.
  `arrow-ipc/src/writer.rs:1900–1907` starts fresh schema/dictionary/EOS state;
  `1951–1955` returns the context after releasing the metadata builder.
  `compression.rs:67–76` sums scratch capacity and native zstd context size;
  FlatBuffers 25.12.19 `FlatBufferBuilder::new()` uses zero initial capacity,
  so replacing the builder does release its unmeasured retained allocation.
  The zstd compressor is replaced if compression level changes.
- `preprocess::tests::lane_workers_reuse_charged_encoder_contexts` holds prior
  outputs while alternating schemas, compares bytes with independent writers,
  checks root/category/slot equality, and exercises encode failure, oversize,
  capacity refusal, clear, off-lane calls, and an unrelated Rayon pool.
  Dependency tests additionally decode independent streams and alternate
  dictionary contents/compression levels. Assertions support these claims.
- `inspection_snapshot` includes encoder slot charges in the ownership sum
  (`scribe/mod.rs:2458–2487`). A context temporarily taken out of a slot remains
  charged at the root. Inspection already explicitly permits independently
  sampled attribution under uninterrupted writes (`2441–2444`); authoritative
  totals stay root-owned. Exact post-ACK assertions now include measured
  encoder metadata while retaining zero `Prepared` assertions.

## Proposed finding INV-1 — abort shutdown leaves encoder retention open

Classification: **INCORRECT**. Requirement/authority: REQ-012's failure and
cancellation cleanup; task scenario 3's cleanup and preserved shutdown behavior;
architecture's structured cancellation and capability memory ownership.

Location: `crates/vala/vala-bifrost-redux/src/scribe/mod.rs:1453`,
`1488–1533`; related owner `scribe/preprocess.rs:1007–1031`.

Starting conditions and trigger: a normal append has warmed at least one
ingress worker's retained context. Call public `ScribeImpl::abort_shutdown`,
or cancel an in-progress graceful shutdown before its context-clear step.
The latter is an existing supported path: `ShutdownCancellationFinalizer::drop`
calls `abort_shutdown` (`mod.rs:374–379`), and the server's deadline-expiry
role abort also calls it (`wyrd-server/src/state.rs:823–828`). Retain a Scribe
or ingress-pool handle for terminal inspection, as existing shutdown tests do.

Expected: shutdown closes reusable context retention; idle contexts release
their memory, and a synchronous encode already in flight keeps its charge
until it settles, then cannot restore into the stopped lane.

Actual source-backed behavior: context clearing occurs only on the ordinary
graceful-shutdown continuation, at line 1453. The abort path reaches
`finalize_shutdown_owners` without clearing/closing contexts, then publishes
`SHUTDOWN_STOPPED`. The Scribe still owns `ingress_cpu`, and its cloneable pool
owns `Arc<EncoderContexts>`; closing the queue neither drops this field nor
closes the context set. Thus idle native compressor allocations and their
root `Metadata` leases remain until the last pool/context owner is dropped.
An already-running encode can also restore after abort because `closed`
remains false. A subsequent `shutdown` observes STOPPED and returns at
1393–1401, so it cannot reach the missing clear.

Consequence: a terminated Scribe retains unusable encoder memory against the
shared process root while callers still own handles; cancelled/aborted
shutdown does not provide the lifecycle cleanup added for graceful shutdown.
The retained amount is bounded, and this finding does not claim data loss,
unbounded growth, cross-tenant exposure, or an effect after process exit.

Proof limits: this is a deterministic ownership/control-flow finding, not a
new runtime reproduction. The existing cancellation test
`scribe::tests::wal_closeout::cancelled_shutdown_owner_is_recovered_by_abort_finalizer`
starts without an append and checks only Raw/Decode/Prepared/Queued ownership
through `assert_ingress_owners_settled` (`wal_closeout.rs:289–312`), so the
reported green suite cannot falsify retained `Metadata` after a warmed abort.

Smallest correction boundary: close/drop reusable encoder contexts through
the shared terminal shutdown owner, reusing `EncoderContexts::clear`; preserve
the existing in-flight lease lifetime and clear/restore race protection.
No new executor, asynchronous cleanup, or changes to durable recovery are needed.

Observable closure: warm a real Scribe ingress encoder, retain its handle,
and assert positive encoder retention before direct abort and cancelled
graceful shutdown. After each terminal path, assert zero idle encoder bytes
and no restored context once any previously running encode has settled.
Use the existing inline Scribe shutdown owner tests; a wire journey is not
needed for this internal resource-lifecycle assertion.

## Evidence limits

The task records 993/993 redux library tests and 75/75 Scribe/OTLP journeys
plus crate clippy, and the assertions above were inspected independently.
This specialist did not rerun those suites or dependency-owner tests; root
coordinates verification. No benchmarks were run. No other blocking issue
was found in the assigned boundary. INV-1 requires fresh independent validation.
