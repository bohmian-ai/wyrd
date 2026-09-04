# Bifrost user-journey tests

This directory is the **tier-1** surface defined in `AGENTS.md` §11: a real SDK
driving a real server along a complete user or agent path, client → server →
client, with no in-process engine fixtures.

Tier-2 integration tests — one subsystem against its real dependency, no server
— live in `crates/vala/vala-bifrost-redux/tests`. Read that directory's
`README.md` before deciding where a test belongs. The short version: **if it
needs a booted server or the SDK, it goes here.**

## Layout

Every test lives in `tests/bifrost/<capability>/`, where `main.rs` is the binary
root and each sibling is one of its modules:

```
tests/bifrost/oracle/
    main.rs              the [[test]] target — mod lines only
    support.rs           fixtures used by more than one module; no tests
    distributed.rs       one theme of the capability
    spill.rs
    ...
```

This is an ordinary directory module tree, so a module is declared with a plain
`mod distributed;` — no `#[path]` attributes.

It also means a source file **cannot** be auto-promoted to a test target of its
own. Cargo only auto-discovers `tests/*.rs` at the top level, and nothing lives
there. That matters: when sources sat flat in `tests/`, the only thing stopping
each one from becoming a second binary was `autotests = false` plus discipline —
and discipline failed. `vala-bifrost-redux` declared three files as `[[test]]`
targets *and* included them as modules, so 109 tests compiled and ran twice on
every lane invocation until it was found. The directory layout makes that
mistake unrepresentable.

## The binaries

| Binary | Modules | Proves | Setup |
|---|---|---|---|
| `forge` | `ownership`, `expiry`, `convergence`, `orphan_gc`, `dedicated_roles`, `live_rewrite`, `live_replacement`, `lease_theft`, `commit_windows`, `maintenance_interleaving` | compaction, maintenance, publication recovery, worker lifecycle, lease reclaim, commit-window reconciliation | Postgres |
| `scribe` | `write_read`, `telemetry`, `lifecycle`, `event_time_window`, `source_boundary`, `source_boundary_recovery` | write, ack, seal, WAL replay, exactly-once restart, admission window, OTLP source-boundary durability | Postgres + server |
| `oracle` | `published`, `distributed`, `convergence`, `spill`, `capacity`, `grpc_surface`, `observability`, `recovery`, `layout`, `peer` | query execution, distributed follower dispatch, peer security, tail fencing, cancellation, telemetry | Postgres + server |
| `otlp` | `logs_export`, `metrics_export`, `trace_export`, `trace_export_http`, `mixed_batch`, `negative` | OTLP logs, metrics, traces, mixed batch, negative protocol surface | Postgres + server |
| `server` | `grpc_mount`, `smoke`, `owner_inspection` | gRPC mount, boot, owner lifecycle and inspection | Postgres + server |

MCP journeys live in a sibling crate under the same rule:
`wyrd-mcp/tests/bifrost/mcp/` aggregates `layout` and `rbac`.

A `support` module holds only what more than one sibling uses. A helper with a
single consumer belongs in that consumer, so reading one journey never starts
with reading a shared fixture file.

## Registration

**A journey is registered by membership in a capability binary that a lane runs
whole.** It is *not* registered by `#[ignore]`.

`#[ignore]` excludes a test from the default family lane. `mod pg_tests` is
source organization only; mise does not inspect module names. The `otlp` binary
shows the two axes are independent — none of its tests are `#[ignore]`d, all are
inside `mod pg_tests`, and the whole binary runs in both its family and journey
lanes.

So:

- Put the test in the binary that owns its capability. It now runs in that
  binary's lane. **No `mise.toml` edit.**
- Add `#[ignore]` when it must stay out of the default family lane.

## Adding a journey

1. Pick the binary whose capability it proves.
2. Add it to the module that owns its theme, or add a new file plus one `mod`
   line in that binary's `main.rs`.
3. If it needs Postgres or a server, `#[ignore]` it (or place it in
   `mod pg_tests`).

Do **not** add a `[[test]]` entry for the new file. Add a new `[[test]]` target
only for a genuinely new capability, and give it a `//!` doc naming the
capability, its setup, its lane, and what it leaves to another binary.

Keep a module under roughly 40 KB. Splitting costs one file and one `mod` line.

## Lanes

| Lane | Runs |
|---|---|
| `mise run test:bifrost` | every Bifrost unit, integration, and Rust/Python/TypeScript journey lane |
| `mise run test:bifrost:journey` | every capability below, in sequence, under one database lifecycle |
| `mise run test:bifrost:journey:sdk` | `vala-sdk` `pg_bifrost_e2e`, whole |
| `mise run test:bifrost:journey:forge` | `forge`, whole |
| `mise run test:bifrost:journey:scribe` | `scribe`, whole |
| `mise run test:bifrost:journey:oracle` | `oracle`, whole |
| `mise run test:bifrost:journey:otlp` | `otlp`, whole |
| `mise run test:bifrost:journey:server` | `server`, whole |
| `mise run test:bifrost:journey:mcp` | `wyrd-mcp` `mcp`, whole |
| `mise run test:bifrost:journey:python` | Python Bifrost client and query journeys |
| `mise run test:bifrost:journey:typescript` | TypeScript Oracle query journey |
| `mise run test:wyrd` | everything not `#[ignore]`d and not in `mod pg_tests` |

Every lane selects a Cargo **target**, never a test name. Cargo resolves a
target against the manifest and fails loudly when it is missing; a positional
name filter is unverified and exits 0 after matching nothing.

## Running one test directly

```bash
mise exec -- cargo test --locked -p wyrd-testing --test oracle \
  -- distributed::pg_bifrost_oracle_distributed_journey --exact --ignored --nocapture --test-threads=1
```

Most need a live database. Wrap it the way the lanes do:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && <command>'
```

## What a test may not do

- **A test must assert.** A test that calls another test and prints a marker
  proves nothing, costs that test's full runtime a second time, and gives its
  name to behavior it does not check. Eleven such tests were deleted from the
  redux oracle group; do not reintroduce the pattern.
- **A test must not name a plan.** Task, case, and hypothesis identifiers are
  meaningless once the plan closes and force a reader to find a document that
  may be private. Describe the behavior instead.
