# Handoff: OTLP logs ingest throughput

## Goal

Raise acknowledged OTLP logs ingest throughput, measured the way OpenObserve
does: uncompressed input MiB/s per vCPU and MiB per CPU-second, on the
`otlp-logs-c64` step of `mise run bench:l1:bifrost:writes`. The user wants
fixes, not A/B experiments or more timers.

## Diagnosis (read-only Codex diagnostician, full text in `codex-throughput.md`)

- With 64 closed-loop clients, throughput is 64 divided by the request time:
  64 / ~0.067 s ≈ 955 req/s, against 935 measured. Raising req/s means cutting
  each request's time, not adding cores; the server uses ~8 of 32.
- Mean time per request is 67.8 ms: 0.91 ms `build`, 0.025 ms `token`,
  66.9 ms gRPC `export`. The server's `bifrost.otlp.logs` timer covers
  30.6 ms of it. That leaves **~36 ms per request inside the gRPC call but
  outside the server**: channel dispatch, protobuf encoding, transport and
  first-frame arrival, then response delivery.
- On the server, Scribe averages 25.7 ms: shard wait ~9.75 ms, WAL append
  ~2.15 ms, sync ~6.73 ms, visibility ~4.41 ms.
- In the current run, Variant conversion is ~2 µs per group. The earlier
  13 ms per group came from a different learned-layout state, so widening
  `CONVERSION_WINDOW` is **not** justified by this run.
- **Earlier Run A (64 driver connections) was invalid.** The patch passed one
  endpoint 64 times to `Channel::balance_list`. Tonic keys balanced endpoints
  by URI (`tonic-0.14.9/src/transport/channel/mod.rs:134`), so the duplicates
  replaced each other and Run A still ran on one connection. Earlier
  conclusions that "the single connection is not the cap" are void.
- Hyper's h2 client polls request bodies from its single connection-dispatch
  task (`hyper-1.10.1/src/proto/h2/client.rs:539`). With all 64 senders on one
  connection, that task serializes ~600 MB/s of encoding and framing. This is
  the concrete serialization point behind the ~36 ms.

## The fix to implement

Give every closed-loop sender its own HTTP/2 connection, as 64 real OTLP
exporters would have, at `Otlp::connect`
(`crates/wyrd/wyrd-testing/src/bin/capacity/query/run.rs` ~207) and the
channel setup it is cloned from (`query/mod.rs` ~1637).

- Either open one `Channel` per sender and await each `connect()` before the
  window opens (the simplest form), or use `Channel::balance_channel` with
  distinct integer keys.
- Connections must be established before measurement; don't use a lazy
  balancer.
- Preserve: fresh row identities per request, the partial-success and
  rejection checks in `Sender::send`, late-completion accounting, busy-retry
  behavior, and the step's 64-client meaning.
- Callers affected: the spans, logs and UI senders in `run.rs`.
- No server change is needed for this. Server admission, tenancy, error
  semantics and the chain WAL sync → durable tenant fence → visibility →
  ACK are untouched.

Verify by test first:
- **Test:** a `run.rs` unit test that the sender set holds N distinct
  connections.
- **Checks:** `mise run fmt && mise run lints`;
  `mise exec -- cargo nextest run --locked -p wyrd-testing --bin capacity`;
  `mise run test:bifrost:journey:otlp`.
- **Benchmark:** one `mise run bench:l1:bifrost:writes -- --steps otlp-logs-c64 --allow-dirty`.
  Uncommitted edits need `--allow-dirty`; without it the run is refused after
  a ~9 minute build.
- **Compare against:** the current tree (935 req/s, 17.87 MiB/s/vCPU,
  69.54 MiB/cpu-s, `export` mean ~66 ms).

If the result moves the cap to the server, the next target is Scribe's
shard wait and sync. Report to the user before redesigning.

## Committed before this handoff (`0047757df`, `3d3371c5f`)

1. `crates/wyrd/wyrd-tonic/src/server/mod.rs`: gRPC server HTTP/2 receive
   windows `CONNECTION_WINDOW_BYTES = 16 MiB`, `STREAM_WINDOW_BYTES = 4 MiB`
   (hyper default 1 MiB each). User approved. Measured gain was +16% once and
   +3% on rerun, so it isn't proven repeatable.
2. `crates/wyrd/wyrd-testing/src/bin/capacity/query/{run.rs,isolated.rs,mod.rs}`:
   driver timer `client.otlp.export` (`build` / `token` / `export`), reported
   as `diagnostic.driver.client.otlp.export.*`. Test:
   `query::isolated::tests::otlp_client_phases_read_back_from_the_driver_recorder`.

Verified: lints clean; `wyrd-testing --bin capacity` 90/90; `wyrd-testing
--lib` under `scripts/postgres/with-test-postgres.sh --` 73/73;
`wyrd-tonic --lib` 38/38; `test:bifrost:journey:otlp` 21/21.

## Measured runs (`otlp-logs-c64`, 3 repetitions, 0 errors)

| Run | req/s | MiB/s/vCPU | MiB/cpu-s | client p50 | server p50 | cores |
|---|---|---|---|---|---|---|
| Baseline, 1 shard, 1 connection | 909 | 17.38 | 75.59 | 61.9 ms | 31.6 ms | 7.36 |
| 4 Scribe shards | 907 | 17.34 | 56.98 | — | — | 9.74 |
| "64 connections" (**invalid: still 1 connection**) | 932 | 17.82 | 78.84 | 60.9 ms | 29.7 ms | — |
| 16/4 MiB server windows, one-off | 1053 | 20.13 | 78.02 | 57.5 ms | 17.7 ms | 8.26 |
| Current tree | 935 | 17.87 | 69.54 | 62.5 ms | 24.5 ms | 8.22 |

## Evidence files

Scratchpad
`/tmp/claude-1000/-home-thorrester-Documents-GitHub-wyrd-worktrees-github-verifier-pipelines-task-004/3b40eaa1-3e89-46cc-9840-492e3a545056/scratchpad/`
(temporary; a reboot can clear it):

- `codex-throughput.md`: this diagnosis
- `bench-phases.txt`: the current-tree report
- `prof-driver-c64.txt`, `prof-c64.txt`: CPU profiles
- `ab-patch.py`: the invalid Run A patch

## Standing rules from the user

- Every command goes through `mise`. Benchmarks run only via `mise run bench:*`,
  one at a time.
- No more A/B experiments; implement fixes.
- Don't propose changing the WAL compression algorithm; it was tried with no
  effect.
- No commits unless asked. No attribution, links or session trailers.
- Replies under about 150 words. Report before a redesign; never chain more
  than one redesign without the user.
- Use a read-only Codex diagnostician for timing or concurrency causes
  (`codex exec -m gpt-6.1-sol -c model_reasoning_effort=high -s read-only`),
  with a brief that gives no hypothesis.

Related, separate: `changes/active/openobserve-logs-l2-benchmark/design/notes.md`
(L2 benchmark design; D1–D5 await the user).
