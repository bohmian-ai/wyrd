# Clean-candidate verification

Source: detached `/tmp/wyrd-task003-review-d7685d129` at
`d7685d129ffee870ab1c497cdcb1206a89bc80c4`. Tracked and untracked Git status
remained clean after checks. Primary worktree's additional lifecycle/error/
generated/test changes are excluded from these runs.

| Fresh check | Result |
|---|---|
| `git diff --check 634e19cbc...d7685d129` | PASS |
| `mise run fmt:check` | PASS |
| Exact five Rust client tests below | PASS, 5 selected / 5 passed, 285 skipped |
| `mise run docs:linkcheck` | PASS |
| `mise run docs:check:commands` | PASS |
| `mise run docs:build` | PASS, static build/Pagefind, 42 pages indexed |
| `mise run docs:a11y` | PASS, 42 pages, light/dark AA contrast |
| `mise run docs:svelte:check` | PASS, zero errors/warnings |
| `mise run docs:test` | PASS, 13 files / 102 tests |

Exact Rust command (shared artifact target only; all source resolves from the
clean detached checkout):

```bash
mise exec -- env CARGO_TARGET_DIR=/Users/stevenforrester/Documents/GitHub/wyrd/target cargo nextest run --locked -p wyrd-client --lib -E 'test(=observe::tests::concurrent_first_writes_describe_each_table_once) | test(=observe::tests::shutdown_during_start_stays_closed) | test(=observe::tests::failed_startup_may_be_retried) | test(=observe::tests::flush_drains_only_a_started_writer) | test(=authoring::tests::trace_field_declares_a_trace_slot)'
```

The concurrency-named Rust test covers destination/producer pooling after
explicit startup. It is not claimed as a test of the new lazy-start gate or
TypeScript cold producer-span path. Reports assess the actual scope of each
existing assertion and supplied integration proof. Cargo test compiled
against the detached source, not dirty primary implementation. The docs test
task also used that artifact target via `CARGO_TARGET_DIR` and its existing
contract-generator/runner; no generator mutation of source was performed.

Dependency setup: an initial attempt to reuse the primary docs node_modules
through a symlink was refused by pnpm's dependency-location verification before
the build. The link was removed, then `mise exec -- pnpm install --offline
--frozen-lockfile` installed the locked dependencies locally in the detached
docs directory (161 reused, zero downloaded). The subsequent normal owning
docs build passed. No dependency or lockfile/source change was made and no
verification gate was disabled.

Broader Rust/Python/TS suites, nine example journeys, affected SDK integrations,
lint/typecheck and generated-sync results are supplied by the user and prior
task/remediation evidence, not wholesale fresh executions here. No full gate,
full journey aggregate, full docs:check generation aggregate, live provider or
benchmark was run. Selected reviewers independently traced defined server
hot-path owners and found no added/moved external wait in them; cold SDK
startup waits and their approval are recorded in the specialist reports. No
engine performance/benchmark VALID claim is made.
