# Bifrost redux integration topology

This directory is the **tier-2** test surface defined in `AGENTS.md` §11: each
test exercises one subsystem against its real dependency, without the full
client → server → client journey.

## Which tier am I writing?

| | Here (`vala-bifrost-redux/tests`) | `wyrd-testing/tests` |
|---|---|---|
| Tier | 2 — integration | 1 — user journey |
| Links | this crate, directly | the SDK, over the wire |
| Server | none | `WyrdTestServer`, booted |
| Proves | plan shape, cut selection, dispatcher accounting, WAL/seal persistence, catalog writes | what a real caller observes end to end |
| Lane | `mise run test:bifrost` | `mise run test:bifrost:journey` |

The split is forced, not stylistic. `vala-bifrost-redux` is a dependency of
`wyrd-server`, so a test here cannot boot a server without a dependency cycle;
and most of what these tests assert has no wire representation for a tier-1
journey to observe. Neither tier can absorb the other.

**If your test needs a booted server or the real SDK, it belongs in
`wyrd-testing/tests`.** Read that directory's `README.md` before adding it.

Tier 2 never substitutes for a missing tier-1 journey. A user-facing capability
still ships a journey even when its internals are covered here.

## One target, one compile

`integration` is this crate's only `[[test]]` target. Every source in this
directory is a `#[path]` module of `tests/integration.rs`, so each file is
compiled once and each test runs once.

`autotests = false` in `Cargo.toml` is what makes that hold. Without it Cargo
would also promote every `tests/*.rs` file to a target of its own, and each
module would compile and execute **twice** — once inside `integration`, once
standalone. That is not hypothetical: `oracle_core`, `pg_file_list_tenant_table`,
and `forge_incremental_compaction` were declared that way and ran 109 tests
twice per lane invocation until the duplicate targets were removed.

So: **do not add a `[[test]]` entry for a file `integration.rs` already
includes.** Add a new target only for a genuinely separate binary that shares no
sources with `integration`, and give it a `//!` doc naming its tier, setup, and
owning lane.

## Adding a test

1. Add it to an existing module here, or add a new source file plus one
   `#[path]` line in `tests/integration.rs`.
2. Do not touch `Cargo.toml`.
3. Do not touch `mise.toml` — `test:bifrost` runs the binary whole.
4. Add `#[ignore]` only for a test that must stay out of the fast lane. Note
   that `test:bifrost` passes `--include-ignored`, so an ignored test still
   runs there; `#[ignore]` excludes it from the family lanes, not from Bifrost's
   own lane.

## Running

```bash
mise run test:bifrost                  # lib + integration + doc tests, whole

# One test while iterating:
mise exec -- cargo test --locked -p vala-bifrost-redux \
  --features test-support,bench-support --test integration \
  -- oracle_core::participant_cut_is_selected_sorted_and_read_once \
  --exact --nocapture --test-threads=1
```

Postgres-backed modules need the canonical local database; `mise run
test:bifrost` wraps the run in `scripts/postgres/with-test-postgres.sh` for you.

## File size

Keep a module under roughly 40 KB. Splitting is cheap by construction — a new
file plus one `#[path]` line, with no `Cargo.toml` or `mise.toml` change — so
there is no reason for a module to grow past the point where a reader can find
things in it.
