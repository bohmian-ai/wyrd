# TASK-002: segmented IPC and WAL boundary review

Subject: `5a057524f07d23a515d505caa4955539926d7fee` →
`c40e7245c3ada4a35177e3a8c2d37c42d058c619`; HEAD matched the candidate
at entry and after inspection. Approved contract: spec revision 3, particularly
REQ-001, REQ-004 and REQ-011; scenario 2 of the original task. The maintainer
explicitly approved the Arrow fork and excluded deferred benchmarks as review
blockers. This report makes no throughput claim.

## Boundary and authority

Reviewed preparation → owned chunks → WAL framing and IO → replay consumers,
including lease lifetime and existing acceptance arbitration. Authority read:
root AGENTS.md supplied with the request, `TESTING.md`, maintainer style,
`architecture/bifrost-design.md` durability, append/staging and recovery/cleanup
sections, approved spec and task execution references. No nested AGENTS.md was
found under `crates/vala` or `crates/shared`. Source navigation used CodeGraph
before focused reads where its excerpts omitted the needed method bodies.

## Source and proof assessment

- `preprocess.rs:801` prepares each slice through the existing Arrow encoder,
  passes owned ordered buffers into `PreparedWalAppend::chunked`, and computes
  logical identity through the unchanged `logical_data_identity`. Managed-field
  exclusion, schema fingerprint, partition and tenant identity remain unchanged.
  `prepared_slice_bytes` at line 742 charges buffer capacity; ingress
  `charge_prepared` at line 896 settles that amount before WAL dispatch. The
  scenario-2 parity assertion at `preprocess.rs:1496` compares joined test output
  with an independent `StreamWriter` at production compression settings for a
  sliced batch containing nested lists, dictionaries and nulls, for two tenants.
- `wal.rs:618` checks each chunk length and cumulative stream length against
  `u32`; framed payload conversion also checks `u32` in
  `append_prepared_borrowed` at line 813. The existing framing fields, version,
  partition and table-version encoding are preserved. SHA-256 and CRC32C advance
  over every ordered payload part, including all chunks. Production never joins
  the payload: the flattening in `PreparedWalAppend::record` is under `cfg(test)`.
- `wal.rs:765` writes at most 64 borrowed pieces per window, passing each window
  to the existing `write_all_vectored`. It preserves short-write resumption,
  interruption retry, empty pieces, `WriteZero` and unchanged error propagation.
  `pieces_beyond_one_window_write_in_order` asserts exact bytes across more than
  two windows for short-write limits 1, 2, 7 and unlimited; existing scripted tests
  assert interruption, zero progress, empty pieces and non-retryable errors.
- `a_many_chunk_slice_round_trips_byte_for_byte` at `wal.rs:4462` writes 200
  chunks through the actual file writer, syncs, reads and validates the record,
  then compares its payload with the equivalent contiguous record. This connects
  segmented output to the unchanged WAL reader/CRC decoder, rather than only
  testing an in-memory concatenation.
- `WalWriter::append_prepared` at `wal.rs:2623` holds the prepared owner until IO
  returns. Any window failure reaches unchanged
  `rollback_prepared_append` at line 2272, truncating and syncing to the prior
  logical boundary; rollback failure faults the WAL. A later append remains
  usable after successful rollback, as asserted by
  `partial_write_rolls_back_and_allows_later_append`. The representation changes
  neither grouping nor the subsequent sync/acceptance order.
- `shards.rs:4733` moves the request leases into `WriteIngressSlices`.
  `execute_wal_io` at `execution_lanes.rs:2477` keeps them through all writes and
  returns them with successful results; on error they drop after the operation
  settles. `ingress_slice_io_retains_leases_until_the_write_settles` checks the
  actual many-chunk IO operation retains its 4096-byte charge until the returned
  owners drop, and releases it on a device-full error. No asynchronous reference
  escapes borrowed chunk storage.
- Replay consumes the same persisted stream bytes and slice identities.
  `replay.rs:1073` reconstructs terminal commit coordinates and slice digests;
  the unchanged `shards.rs:1928` resolver asks durable acceptance authority and
  suppresses both unfenced and noncanonical attempts before restored rows become
  live. Errors propagate rather than becoming an absent receipt. Chunk ownership
  does not change the fence, frontier or independently staged recovery owners.
  Existing real-server assertions in `scribe/ambiguous_retry.rs:83` check the
  same batch identity survives ambiguous delivery and yields exactly one row;
  `scribe/source_boundary_recovery.rs:34` checks exact visible rows and durable
  object identity across failed and ambiguously committed publication;
  `scribe/write_read.rs:1154` checks acknowledged rows, pressure, publication,
  restart and deduplicated resend using actual client/server dependencies.

No added or relocated PostgreSQL, object-store, Vault or Scribe wait was found
in this boundary: the existing WAL append, sync, acceptance and replay operations
retain their owners and ordering. Additional vectored syscalls remain within the
existing serial append operation and do not add a durability cycle.

## Evidence and result

No blocking implementation defect or boundary-specific evidence gap found.
REQ-001/004 preservation and REQ-011 are supported by the connected source paths,
actual assertions, and candidate-bound execution recorded in the task: 993 redux
library tests and 75 Scribe/OTLP journeys pass. The earlier integration run is
recorded at the scenario-1 checkpoint; the final library and journey runs exercise
the final representation. Scenario 2 records behavioral RED failures for unsupported
many-chunk writing and joined encoder output, followed by GREEN; no contrary
test-first history was found within this assigned scope.

I reran `git diff --check 5a057524f...c40e7245c` successfully. I did not rerun
Cargo tests, to keep Cargo serial with the orchestrator's required checks; runtime
results above are recorded implementation evidence, not fresh reviewer execution.
Encoder context lifecycle/accounting is assigned to the invariant reviewer;
overall acceptance, mandatory gates and deferred performance qualification belong
to the reconciled verdict.

## Focused follow-up: shipped-feature lint failure

**Proposed ID DOMAIN-001 — VIOLATION: the required shipped-server lint gate
fails on the retained contiguous constructor.**

- Authority: AGENTS.md requires clean format/lints and forbids suppressing a red
  gate. The original task's final verification explicitly requires
  `mise run lints`. `mise.toml:55` defines that gate as both all-feature workspace
  Clippy and Clippy of the shipped server alone without test/benchmark features.
- Exact location: `scribe/wal.rs:511`, `PreparedWalAppend::new`.
- Starting state/trigger: build the candidate's shipped feature graph through
  the second command in `mise run lints`,
  `cargo clippy --locked -p wyrd-server --bin wyrd-server -- -D warnings`.
- Expected: both configured lint commands pass. Actual: the orchestrator's
  execution fails with `PreparedWalAppend::new` unused / `dead_code` denied.
  This reviewer did not repeat the Cargo invocation; this follow-up independently
  establishes its source cause.
- Cause and evidence: the base's production `preprocess::prepare_slice` called
  `PreparedWalAppend::new`; the candidate switches that caller to `chunked` at
  `preprocess.rs:817` while leaving `new` compiled unconditionally. Every remaining
  `new` call is in unit tests, test-support methods (`wal.rs:2548,2846`), or
  `bench_support.rs:152`, whose module is `cfg(feature = "bench-support")` in
  `lib.rs:10`. `wal.rs:2794` is a `cfg(test)` helper; replay callers are inside
  its test module. The all-feature workspace lint therefore retains callers and
  masks the shipped feature graph's dead code.
- Consequence: the mandatory lint task exits unsuccessfully for the supported
  production build configuration. No data corruption is alleged.
- Smallest correction boundary: make the old contiguous convenience constructor
  available only to its actual test/test-support/bench-support consumers, or
  remove it and use the existing chunked constructor at those consumers. Preserve
  the production segmented flow and all framing/parity fixtures. Do not add a
  lint allowance or enable test features on the shipped-server gate.
- Observable closure: the unmodified `mise run lints` passes both commands;
  existing many-chunk parity and encoded-length tests still compile and pass,
  along with any affected benchmark-support compilation. No new runtime test or
  production behavior is needed for this compile-configuration correction.

The earlier no-defect conclusion applies to functional WAL semantics; this
follow-up adds a concrete mandatory gate violation for independent validation.
