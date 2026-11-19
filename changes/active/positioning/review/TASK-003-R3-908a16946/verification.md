# R3 verification

Immutable candidate: `908a169460550bba10eecfd2758fa9deddd9d4cc`.
R3 base: `7bb44f2dff1994499e575c27f0be5568dcc2d167`.
Implementation files remained clean; unrelated dirty planning files and prior
untracked reviews are excluded.

Fresh orchestrator checks:

| Command / selection | Result |
|---|---|
| `git diff --check 7bb44f2df 908a16946` | PASS |
| `mise run fmt:check` | PASS |
| `mise run docs:linkcheck` | PASS |
| `mise run docs:check:commands` | PASS |
| Exact real-server/Postgres TypeScript timeline command below | PASS: exactly 1 selected/passed, 3 skipped; migration idempotence setup also passed |
| `mise run docs:test` | PASS: 13 files, 102 tests |

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cd sdks/wyrd-sdk-ts/wyrd && pnpm exec vitest run tests/integration/action-timeline.test.ts -t '^service action timeline is one connected tree$'"
```

Vitest reported the selected scenario passing in 5.023 seconds (one test file,
one passing test, 7.12 seconds total). The assertions identify each cold/warm
parent's one producer, every producer's linked ingest consumer, and exactly two
distinct sample record IDs. This is execution proof, not only source spelling
or module resolution.

The appended R3 evidence additionally supplies observation-surface7, Python
timeline1, Rust client290, lints, TS format/type/declaration checks, Rust docs
with warnings denied, docs generation/tests/build/a11y and whitespace results.
Those are supplied runs; this record does not claim wholesale independent
reruns. Broad journeys/gate and engine benchmarks are outside the unchanged
owners/this remediation write set. Any source changes discovered during review
must be recorded and may invalidate the subject.
