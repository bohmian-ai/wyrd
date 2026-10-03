# Intermittent `verification_runtime::two_bindings_share_one_client_observation`

- **Symptom:** both Drift verdicts come back `inconclusive` because the run's
  window counted zero rows. Before the fix the test failed 6 of 6 times when
  run alone.
- **Evidence:** in a failing run's SQL the window ended at `21:01:32.516`,
  while the stored `wyrd_event_time` was `21:01:32.559`. A probe in the test
  read the database clock about 80 ms behind the host (Postgres runs in a
  Colima VM).
- **Cause:** the observation's `wyrd_event_time` is the only timestamp here
  that PostgreSQL does not own. The client owns it, and when the client omits
  it, Scribe stamps its receipt instant from the host clock
  (`scribe/execution_lanes.rs` `current_receipt_micros`). The window end is
  PostgreSQL's `statement_timestamp()`, set when the test made the binding
  due. The test wrote the observation and then expected its host-clock
  receipt instant to fall before PostgreSQL's "now". When the database clock
  lags the host, it falls after. Production does not assume this; an event
  after a window's end lands in the next window.
- **Rejected fix:** `0ad7e348d` wrote the host clock into `next_run_at`. That
  is a PostgreSQL coordination timestamp, which AGENTS.md §15 says PostgreSQL
  owns. It was reverted in `896a2366b`.
- **Fix site:** the test's own observation. Its client stamps an explicit
  `wyrd_event_time` one minute before its own clock (`EVENT_TIME_LEAD` in
  `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs`).
  `make_binding_due` stays plain `statement_timestamp()`.
- **Proof:** the focused command passed 5 of 5 runs, and `mise run lints`
  passed.
