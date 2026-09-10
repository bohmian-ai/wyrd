# Testing Wyrd

How the test suite is organized, which lane to run, and where a new test goes.
The rules live in `AGENTS.md` §11; this file is the map.

## The three tiers

Ranked. A higher tier proves the product works; a lower tier proves a part
works. **A lower tier never substitutes for a missing higher one.**

| Tier | What it drives | Where it lives | Needs |
|---|---|---|---|
| 1 — user journey | the real SDK against a real server, client → server → client | `crates/wyrd/wyrd-testing/tests/`, `crates/wyrd/wyrd-mcp/tests/`, `vala-sdk`'s `pg_bifrost_e2e` | Postgres + booted server |
| 2 — integration | one subsystem against its real dependency, no server | `crates/vala/vala-bifrost-redux/tests/`, `crates/wyrd/wyrd-sql/tests/`, … | usually Postgres |
| 3 — unit | one function or type, in isolation | `src/**/mod tests` | nothing |

Every new user- or agent-facing capability ships a tier-1 journey. Pushing a
user-observable behavior — especially a negative flow — down to a unit test
*only* is a coverage gap, not a substitute.

### Why Bifrost has two test crates

`vala-bifrost-redux` is a dependency of `wyrd-server`, which is a dependency of
`wyrd-testing`. A test inside redux therefore **cannot** boot a server without a
dependency cycle. And much of what redux asserts — DataFusion physical-plan
shape, participant cut selection, dispatcher attempt accounting — has no wire
representation for a journey to observe through a server API. Neither tier can
absorb the other, so both exist.

**Deciding where a test goes:** does it need a booted server or the SDK? Yes →
`wyrd-testing`. No → `vala-bifrost-redux`.

## Layout

Both trees use the same shape: a capability directory whose `main.rs` is the
`[[test]]` target and whose siblings are ordinary directory modules.

```
crates/wyrd/wyrd-testing/tests/bifrost/oracle/
    main.rs        the target — mod lines only
    support.rs     fixtures used by 2+ siblings; no tests
    distributed.rs one theme
    spill.rs
```

Three rules follow from it:

1. **No `#[path]` attributes.** A subdirectory resolves modules natively.
2. **One compile per source.** Cargo only auto-discovers `tests/*.rs` at the top
   level, and nothing lives there, so a file cannot become a second target.
   This is enforced by structure, not by discipline — which matters, because
   discipline previously failed: `vala-bifrost-redux` declared three files as
   `[[test]]` targets *and* included them as modules, and 109 tests compiled and
   ran twice on every lane invocation until it was caught.
3. **Adding a test needs no `Cargo.toml` or `mise.toml` edit.** Lanes select
   targets, so membership in a binary is the registration.

Keep a module under roughly 40 KB. Splitting costs one file and one `mod` line.

## Which lane do I run?

Run the narrowest complete capability lane covering what you changed. `mise run
gate` is the nightly, release, and conservative fallback aggregate.

### Bifrost

```bash
mise run verify:bifrost                  # complete Bifrost checks + all test tiers
mise run test:bifrost                    # all Bifrost tests and language surfaces
mise run test:bifrost:integration:redux  # tier 2: the whole redux crate
mise run test:bifrost:journey            # tier 1: every capability, one DB lifecycle
mise run test:bifrost:journey:oracle     # tier 1: one capability
#                     :sdk :forge :scribe :server :mcp :python :typescript
```

### Everything else

```bash
mise run test:wyrd | test:skald | test:vala | test:shared   # family lanes
mise run test:sql                        # SQL-backed integration
mise run test:tonic                      # wyrd-tonic at its required feature union
mise run test:storage:matrix             # object-store emulators
mise run py:test:unit                    # Python
mise run ts:test:unit                    # TypeScript
mise run test:rust                       # full Rust aggregate
```

`test:rust` is the broad Rust aggregate. Capability work uses its capability
lane instead; Bifrost-only pull requests run `verify:bifrost`.

### Checks and codegen

```bash
mise run fmt lints                       # always
mise run codegen:check                   # contracts, schemas, stubs
mise run check:client-tier | check:pyo3-scope | check:unwrap-audit
```

## Family lanes vs. gated journeys

Two independent axes decide where a test runs. They are often confused.

- **Which binary or capability-owned module** it is in decides **which lane**
  runs it. This is the only thing that registers a test.
- **`#[ignore]`** keeps a test out of its default family lane. `mod pg_tests`
  is source organization only; mise does not interpret module names.

Family lanes run every non-ignored default-feature test in their crates and
provision Postgres where any owned crate needs it. Feature-specific and external
service journeys keep explicit lanes.

Bifrost's own lanes pass `--include-ignored`, so an ignored test still runs
there. Marking a test `#[ignore]` excludes it from the default family lane, not
from Bifrost.

## When a test fails

Every lane runs with `RUST_BACKTRACE=1` and `--no-fail-fast`, and the journey
lanes run all seven capabilities before reporting. You should get the complete
set of failures from one invocation — if you find yourself rerunning a lane to
discover the next failure, something has regressed in the harness.

On a panic inside a journey, the harness also prints what the installed
production telemetry saw: every span the tracer marked failed with its scrubbed
attributes and trace id, the span names that finished, and the non-zero metric
series. This is why an assertion on a number is diagnosable — the span tree that
explains it prints alongside it.

Two things the lanes deliberately do **not** do:

- **Run in parallel.** `--test-threads=1` is forced. The telemetry recorder and
  subscriber are process-global singletons and the capabilities share one
  Postgres lifecycle; more threads produce failures that are artifacts of that
  sharing. Run a lane in the background rather than widening it.
- **Keep the database.** The Postgres fixture is a temp instance torn down at
  the end of the lane, so there is no post-mortem SQL. Anything you need after
  the fact has to be asserted or printed during the run.

## Writing a test

Follow `AGENTS.md` §16 for style, and these two rules specifically:

- **A test must assert.** A test that calls another test and prints a marker
  proves nothing, silently pays that test's full runtime a second time, and
  lends its name to behavior it never checks. Eleven such tests existed in the
  redux oracle group and were deleted.
- **A test must not name a plan.** Task, case, and hypothesis identifiers
  (`T13`, `P27`, `D85`, `H1`) are meaningless once the plan closes and send a
  reader hunting for a document that may be private. Describe the behavior.

Python tests use top-level `def test_*` only — never `class TestFoo:`.

## Where to look next

- `crates/wyrd/wyrd-testing/tests/README.md` — tier-1 binaries, their modules,
  and how to add a journey.
- `crates/vala/vala-bifrost-redux/tests/README.md` — the tier-2 rule and its
  subsystem groups.
- `AGENTS.md` §11 — the normative taxonomy and verification scope.
- `mise.toml` — every lane, with a description on each.
