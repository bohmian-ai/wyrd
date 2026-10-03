# Diagnosis: `verification_runtime::two_bindings_share_one_client_observation`

## Symptom

`wyrd-testing --test server -P journey`,
`verification_runtime::two_bindings_share_one_client_observation` fails with

```
the drifting and steady bindings must reach their own verdicts over the one
shared observation: [Some("inconclusive"), Some("inconclusive")]
```

It failed 6 of 6 times run alone on this host (with tracing:
`WYRD_LOG=info,vala_bifrost_redux=debug,wyrd_server=debug`) and also failed
inside `mise run test:bifrost:journey:server`. Whether it fails depends on how
far the Colima VM clock has drifted from the host clock at the time.

## Evidence

The trace shows both runs read a sealed cut that contains the observation file
(`Oracle pinned one sealed cut tables=1 hot_files=1`). Each run issues one
Custom aggregate.

Temporary instrumentation, since reverted, logged the engine's SQL and the
folded row:

```
SELECT COUNT(*) AS n, COUNT(num_value) AS numeric_n, AVG(num_value) AS mean
  FROM vala.drift.observations WHERE card_uid = '01a10392-73f8-…' AND series = 'score'
   AND wyrd_event_time >= '2026-10-01T00:00:00.000000Z'
   AND wyrd_event_time <  '2026-10-03T21:01:32.516389Z'      row=Some(None)  (n = 0)
```

The stored observation, read through Oracle in the same run:

```
series=score  wyrd_event_time=1791061292559824 (21:01:32.559824)
              wyrd_ingested_at=1791061292559824  created_at=…292305316 (client)
```

Without the event-time bound, the card_uid filter counts 2 rows and the lower
bound counts 2 rows. Only the upper bound excludes the row: the event time is
later than the window end, even though the test wrote the observation
*before* it made the binding due.

Clock probe inside the test (Rust `Utc::now()` around a Postgres
`SELECT clock_timestamp()`):

```
host_before=21:03:11.323066  db=21:03:11.242688  host_after=21:03:11.324235
```

The Postgres clock (Colima VM) runs about 80 ms behind the host clock.

## Cause

The window end and the observation's event time come from two different
clocks, and the test assumes they agree:

- The observation carries no `wyrd_event_time`. The in-process Scribe stamps
  its admission receipt from the host process clock
  (`crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:394`,
  `execution_lanes.rs:694`).
- `VerificationFixture::make_binding_due`
  (`crates/wyrd/wyrd-testing/src/verification.rs:497-500`) set
  `next_run_at = statement_timestamp()`, which is the Postgres clock. The
  scheduler turns that cursor into the exclusive window end
  (`crates/wyrd/wyrd-sql/src/queries/verification.rs:256-261`:
  `DriftWindow { start: previous_before(due), end: due }`).
- The engine filters `wyrd_event_time < end`
  (`crates/wyrd/wyrd-server/src/verification/drift.rs:155`).

The broken assumption is that the database's `statement_timestamp()`, taken
after the write is acknowledged, is later than the host-clock receipt instant
Scribe stamped on that write. When the Postgres clock lags the host by more
than the gap between the write and `make_binding_due`, the window ends before
the observation. Both Custom aggregates then see `n = 0`, fold to
`Drift(None)`, and publish `inconclusive`. The same assumption appears in prose
at `verification_runtime.rs` (`MONTHLY` doc: "an observation acknowledged just
before a binding is made due always falls inside the window").

The earlier suspects did not cause this:

- Oracle event-time pruning: the file was pinned and the predicate is correct.
- Live-route selection: the trace shows the sealed cut read.
- The Drift fold: it correctly folds `n = 0` to unscorable.

## Fix site

The shared owner is `VerificationFixture::make_binding_due`
(`crates/wyrd/wyrd-testing/src/verification.rs`). Every harness routes
through it. It now sets
`next_run_at = GREATEST(statement_timestamp(), $2)`, where `$2` is the
fixture process's `Utc::now()`:

- Postgres still decides dueness. The scheduler's
  `next_run_at <= statement_timestamp()` claim fires once the database clock
  reaches the cursor, and every caller already polls for the run.
- The window end is never earlier than any receipt an in-process Scribe
  stamped before the call.

Production code is unchanged. Real schedule windows end on cron boundaries,
not on "now". Stamping receipts from Postgres would put a database round trip
on the ingest hot path and would contradict AGENTS.md §15, which says event
times are producer-owned.

Callers checked at this owner:

- Same cross-clock assumption, fixed by this change:
  - `sdks/wyrd-sdk-rust/tests/drift_verification.rs:1347` and `:2139`
  - `sdks/wyrd-sdk-python/tests/integration/test_drift_journey.py:183`, via
    `sdks/wyrd-sdk-python/src/testing.rs:571`
  - `sdks/wyrd-sdk-ts/native-testing/src/lib.rs:176`, used by the TS Drift
    journey
- Scripted engines with no windowed evidence read, so dueness timing is the
  only effect and they already poll:
  - `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:90`
  - `crates/wyrd/wyrd-server/tests/pg_verification_runtime.rs:174`
  - `crates/wyrd/wyrd-server/tests/pg_operator_delivery.rs:244` and `:302`

## Independent diagnostician report

A fresh read-only agent received only the failing command, the traces
(uninstrumented and instrumented), and the instrumentation diff. Its report,
condensed:

> **Cause.** A cross-clock comparison between the Colima VM's Postgres clock
> and the host clock, with Postgres about 80 ms behind. `wyrd_event_time` is
> Scribe's host-clock receipt stamp (`execution_lanes.rs:694-695`, `:827`;
> diag4 `wyrd_event_time = wyrd_ingested_at = 21:03:11.302554`). The window end
> is Postgres `statement_timestamp()` from `make_binding_due`
> (`verification.rs:497-500`), carried into `DriftWindow { end: due }`
> (`wyrd-sql/.../verification.rs:256-261`); diag4 window ends `11.248769` and
> `11.250332`. The binding was made due about 30 ms after the write on the
> host clock, yet its window ends about 54 ms before the stored event time.
> `wyrd_event_time < end` (`drift.rs:155`) drops the row, `n = 0`, and both
> runs are inconclusive. The broken assumption: Postgres
> `statement_timestamp()` at make-due is later than Scribe's host-clock
> receipt of an earlier write.
>
> **Fix site.** `VerificationFixture::make_binding_due`, e.g.
> `SET next_run_at = GREATEST(statement_timestamp(), $2)` with the host
> `Utc::now()`. Postgres keeps owning dueness, and the window end never
> precedes an in-process Scribe receipt. Not Scribe or the scheduler.
>
> **Affected callers.** The Rust SDK `drift_verification.rs:1347`, `:2139`;
> Python `test_drift_journey.py:183` (via `testing.rs:571`); TS
> `drift-verification.test.ts:252` (via `native-testing/src/lib.rs:176`). Not
> affected (scripted engines): `verification_runtime.rs:90`,
> `pg_verification_runtime.rs:174`, `pg_operator_delivery.rs:244`, `:302`.

The report agrees with the diagnosis above on cause, fix site, and callers.

## Verification evidence

- Focused command run 10 times after the fix: 10 of 10 passed
  (`1 passed, 31 skipped` each).
  ```
  scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_LOG=info,vala_bifrost_redux=debug,wyrd_server=debug mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E "test(=verification_runtime::two_bindings_share_one_client_observation)"'
  ```
- `mise run test:bifrost:journey:server`: 32 tests run, 32 passed.
- `mise run fmt`: pass.
- `mise run lints`: pass.
- `git diff --check`: clean.
- Not run here: the Rust SDK, Python, and TS Drift journeys. They share the
  fixed fixture.
