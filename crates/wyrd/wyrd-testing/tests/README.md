# Bifrost integration test topology

These tests are grouped by the **capability they prove**, not by the file they
happen to live in. Every test source is a `#[path]` module of exactly one
capability binary, and every lane runs a binary **whole**.

## The binaries

| Binary | Aggregates | Proves | Setup |
|---|---|---|---|
| `forge` | `forge_journeys.rs`, `forge_compaction_interleaving.rs`, `forge_maintenance_interleaving.rs` | compaction, maintenance, publication recovery, worker lifecycle, lease reclaim, concurrent compaction ordering | Postgres |
| `scribe` | `public_write_journeys.rs`, `pg_event_time_window_journey.rs`, `scribe_source_boundary_journey.rs`, `scribe_source_boundary_recovery.rs` | write, ack, seal, WAL replay, exactly-once restart, admission window, OTLP source-boundary durability | Postgres + server |
| `oracle` | `oracle_edge_journeys.rs`, `oracle_peer.rs` | query execution, distributed follower dispatch, peer security, tail fencing, cancellation, telemetry | Postgres + server |
| `otlp` | the six `otlp_*` export/journey files, plus `otlp_support.rs` (helpers, no tests) | OTLP logs, metrics, traces, mixed batch, negative protocol surface | Postgres + server |
| `cluster` | `pg_bifrost_cluster_load.rs`, `pg_bifrost_materializer.rs` | multi-pod topology, tenant fairness, cluster shutdown | Postgres + server |
| `server` | `pg_grpc_mount.rs`, `test_server_smoke.rs`, `bifrost_owner_inspection.rs` | gRPC mount, boot, owner lifecycle and inspection | Postgres + server |
| `interleavings` | `bifrost_interleavings.rs`, `interleaving_smoke.rs` | deterministic scheduler permutation checks | none |

MCP journeys live in a sibling crate under the same rule:
`wyrd-mcp/tests/mcp.rs` aggregates `bifrost_rbac.rs`.

## The registration rule

**A journey is registered by membership in a capability binary that a lane runs
whole.** It is *not* registered by `#[ignore]`.

`#[ignore]` and `mod pg_tests` do something different: they exclude a test from
the **fast** lane (`test:wyrd` and `test:e2e`, which run
`cargo test -p wyrd-testing … --skip pg_tests`). The `otlp` binary shows the two
axes are independent — 25 tests, none `#[ignore]`d, all inside `mod pg_tests`,
and the whole binary runs in the journey lane.

So:

- Put the test in the binary that owns its capability. It now runs in that
  binary's lane. **No `mise.toml` edit.**
- Add `#[ignore]` (or put it in `mod pg_tests`) when it needs the serialized
  Postgres-backed lane and must stay out of the fast lane.

## Adding a journey

1. Pick the binary whose capability it proves, from the table above.
2. Add the test to an existing module there, or add a new source file plus one
   `#[path]` line in that binary's root.
3. If it needs Postgres or a server, `#[ignore]` it (or place it in
   `mod pg_tests`) so the fast lane stays green without a database.

Do **not** add a `[[test]]` entry for the new source file. `autotests = false`
is set in both `wyrd-testing/Cargo.toml` and `wyrd-mcp/Cargo.toml` precisely so
a `#[path]`-included file does not also become a target of its own — declaring
one compiles that source into two binaries.

Add a new `[[test]]` target only when you are introducing a genuinely new
capability, and give it a `//!` doc naming the capability, its setup, its lane,
and what it deliberately leaves to another binary.

## Which lane runs what

| Lane | Runs |
|---|---|
| `mise run test:bifrost:journey` | `vala-sdk` `pg_bifrost_e2e`, then `forge`, `scribe`, `oracle`, `otlp`, `server`, `interleavings`, and `wyrd-mcp` `mcp` — each whole, `--include-ignored --test-threads=1` |
| `mise run test:bifrost:cluster` | `cluster`, whole |
| `mise run test:bifrost:oracle-distributed-parity` | `oracle`, whole, alongside the redux distributed-parity targets |
| `mise run test:wyrd` / `test:e2e` | everything not `#[ignore]`d and not in `mod pg_tests` — in practice `interleavings` plus the unignored tests in `forge`, `scribe`, and `server` |

## Running one test directly

Lanes select targets; you can still select a name while iterating:

```bash
mise exec -- cargo test --locked -p wyrd-testing --test oracle \
  -- oracle_edge_journeys::pg_bifrost_oracle_published_journey --exact --ignored --nocapture --test-threads=1
```

Most of these need a live database. Wrap them the way the lanes do:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc \
  'mise run db:migrate:inner && cargo test --locked -p wyrd-testing --test forge -- --include-ignored --test-threads=1'
```

## Why lanes never name a test

`cargo test <name>` exits **0** when it matches nothing — it prints
`running 0 tests` and reports success. The positional filter is an unverified
substring handed to libtest. Cargo resolves `-p`, `--test <binary>`, and
`--features` against the manifest instead, and fails loudly (`error: no test
target named ...`, exit 101) when one is missing.

A per-test filter in a lane is therefore a claim about coverage that nothing
verifies, and this suite had live examples of it: two filters had been reporting
green for a test name that existed nowhere under `crates/`. Lanes select
targets. Test names belong in a task's proof table, where a reviewer reads them
against the diff.

When a lane genuinely needs less than a whole binary, add a narrower `[[test]]`
target — a manifest fact Cargo checks — rather than a name filter.
