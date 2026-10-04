# REQ-187 evidence: wyrd-client stamps `wyrd_event_time` at emit

Contract: `changes/active/verified-change-contract/spec.md` revision 63, REQ-187.

## Implementation

- One stamp place: `WriterPool::insert_rows` in
  `crates/shared/wyrd-client/src/bifrost/handle.rs` reads
  `chrono::Utc::now().timestamp_micros()` once per emit. `WriterPool::insert`
  now delegates to `insert_rows`, so every SDK JSON row (Rust, Python, TS)
  goes through it.
- The value moves through `Producer::enqueue`/`enqueue_rows` into
  `queue::Row::event_time_micros`, and `build_frame` hands it to the new
  `BatchBuilder::append_stamped_json_row`. Batching, linger, retry, and flush
  move the stored value but never re-read the clock.
- `BatchBuilder` adds a non-null `wyrd_event_time: Timestamp(us, "UTC")` column
  when its rows carry event times. A caller's payload `wyrd_event_time`
  (non-null RFC 3339) wins over the stamp. A null value counts as absent. Other
  `wyrd_*` keys are still refused.
- Scribe is unchanged. It already keeps a writer-supplied `wyrd_event_time`
  as-is and checks the acceptance window. No configuration was added.

## Acceptance

| Criterion | Implementation | Verification | Result |
|---|---|---|---|
| drift/eval/record rows carry `wyrd_event_time` | `WriterPool::insert_rows` stamp | unit `observe::tests::emits_stamp_their_own_event_time_and_keep_a_callers`; journey `observations_store_the_client_emit_time` | PASS |
| Stamped from the client clock at emit; stored value lies between client readings taken before and after the emit (no sleeps) | stamp before `enqueue_rows` | journey shifts Scribe's receipt clock +24h, then asserts all 4 stored values are in [before, after] | PASS |
| Batching/linger/retry/flush do not change it | stamp stored on `Row`, carried into the frame | unit test uses 60s linger; stamps are bounded by emit-time readings, not flush time | PASS |
| Caller-supplied value kept | `BatchBuilder::append_row` reads payload key | unit tests (queue + client); journey stores a value 1h before `before` unchanged | PASS |
| Rust, Python, TS inherit one stamp | single funnel in wyrd-client | Python journey lane 46 passed; TS journey lane 26 passed | PASS |
| Scribe unchanged, no config | no server diff | diff audit | PASS |

## Diagnosis: two eval journeys made stale by REQ-187

`mise run test:bifrost:journey:server` first run: 2 failures in
`crates/wyrd/wyrd-testing/tests/bifrost/server/eval_verification.rs`.

1. `continuous_eval_runs_the_terminal_matrix`: "created_at … shares the managed day".
   - Evidence: the stored event time (…710389) was 85 µs after created_at (…710304).
     The client stamp now wins over the +1 day receipt shift.
   - Cause: the test assumed Scribe's shifted receipt clock set event time.
2. `continuous_eval_reads_ordered_bounded_trace_evidence`: "expected timed_out … status completed".
   - Evidence: the span read window starts at `utc_day(event_time - 1 day)`
     (`crates/wyrd/wyrd-server/src/verification/eval.rs` around lines 637-650).
     A record stamped now reads today's spans instead of the window three days ahead.
   - Cause: same old receipt-stamp assumption.
   - Fix site: these tests only, confirmed by an independent read-only
     diagnostician. Test 1 now asserts the event time lies within the client
     emit window. Test 2 inserts its "future" record as a raw frame stamped
     `now + 3 days`.
   - Other tests are unaffected: sealed replay uses raw unstamped frames, and
     the verification_runtime straddle case uses gateway capture.

After the fix the server lane passed 32/32.

## Commands

| Command | Result |
|---|---|
| `mise exec -- cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::emits_stamp_their_own_event_time_and_keep_a_callers)'` | 1 passed |
| `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=batch_builder::batch_builder_tests::stamped_rows_carry_event_time_and_keep_a_payload_value)'` | 1 passed |
| `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-sdk-rust --test observe_run -P journey --run-ignored=all -E 'test(=observations_store_the_client_emit_time)'` | 1 passed (after the lint refactor) |
| `mise run test:shared` | 711 passed |
| `mise run test:bifrost:journey:observe` | 5/5 passed |
| `mise run test:bifrost:journey:drift` | 4/4 passed |
| `mise run test:bifrost:journey:server` (eval + verification_runtime) | 32/32 passed |
| `mise run test:bifrost:journey:python` | 46 passed |
| `mise run test:bifrost:journey:typescript` | 8 files, 26 tests passed |
| `mise run fmt`, `mise run lints`, `git diff --check` | clean |

`codegen:check` was not run. No generated schema, stub, or public wire
contract changed. Only Rust signatures inside `wyrd-queue` changed, and the
Python and TS lanes compiled against them.

## Notes and risks

- Plain `Bifrost::insert` JSON rows are now client-stamped too, because they
  share the funnel. This keeps every SDK frame for a table uniformly timed.
  Direct `BatchBuilder::append_json_row` users stay unstamped.
- On the first `record` for a table, the table is described before the stamp
  is taken. So on first use only, the stamp includes the describe latency.
- A frame that mixes stamped and unstamped rows is refused with `SchemaParse`.
  SDK paths always stamp, so this is only reachable by direct builder misuse.
- The journey is in `sdks/wyrd-sdk-rust/tests/observe_run.rs`, which owns the
  observe journey lane.
- REQ-186 files (`verifier_runs.rs`, `wyrd-testing/src/verification.rs`) were
  not touched. No Drift window-timing failures were seen.
