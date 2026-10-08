# Testing Wyrd

How the test suite is organized, which lane to run, and where a new test goes.
The rules live in `AGENTS.md` §11; this file is the map.

## The three tiers

Ranked. A higher tier proves the product works; a lower tier proves a part
works. **A lower tier never substitutes for a missing higher one.**

| Tier | What it drives | Where it lives | Needs |
|---|---|---|---|
| 1 — user journey | the real SDK against a real server, client → server → client | `crates/wyrd/wyrd-testing/tests/`, `crates/wyrd/wyrd-mcp/tests/`, `wyrd-client`'s `pg_bifrost_e2e` | Postgres + booted server |
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
`[[test]]` target and whose siblings are ordinary directory modules. Every other
crate has exactly one integration target, `tests/integration/main.rs`, with one
module per surface; lanes select a surface with `-E 'test(/^<module>::/)'`.
Each target statically links the crate's whole dependency cone, so one target
per crate keeps link time, linker memory, and `target/` size proportional to
crates rather than to test files.

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
When using `gate` for final verification, do not also run its component lanes
as a checklist. Use focused tests while iterating; add a separate final lane
only if `mise.toml` shows that required proof is outside `gate`.

### Bifrost

```bash
mise run verify:bifrost                  # complete Bifrost checks + all test tiers
mise run test:bifrost                    # all Bifrost tests and language surfaces
mise run test:bifrost:integration:redux  # tier 2: the whole redux crate
mise run test:bifrost:journey            # tier 1: every capability, one DB lifecycle
mise run test:bifrost:journey:oracle     # tier 1: one capability
mise run test:bifrost:journey:scribe:production-geometry  # scheduled 512 MiB object qualification
#                     :sdk :forge :scribe :oracle :otlp :server :mcp :python :typescript
```

### SDKs

```bash
mise run verify:python-sdk               # checks + unit, TensorFlow, harness, every integration journey, identity
mise run verify:rust-sdk                 # checks + wyrd-sdk-rust unit and every story journey, identity
mise run verify:typescript-sdk           # checks + napi/package/types, unit, every integration journey, identity
```

Each also runs `test:identity:journey` against Keycloak and Dex, so Docker must
be available. One run owns those containers at a time, so the three tasks share
that one dependency and `gate`, which runs all three, runs it once.

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

### What CI selects

`.github/scripts/select-ci.py` maps each changed file to its workspace package
and expands it to that package's consumers via `cargo metadata`. A pull request
runs `check`, the boundary checks, and the owning lanes of the affected
packages. Those lanes cover the family lanes, filtered by `WYRD_TEST_PACKAGES`,
along with codegen, the SDK platform jobs, and the Python, TypeScript,
Bifrost, gateway, identity, and storage journeys that consume them. The selector writes
every reason to the job summary. Any global, mixed, or unknown change, or a
package it cannot place, takes `gate`. Main pushes build only the affected
release packages; releases build every package.
`mise run check:ci-selection` pins every route.

### Checks and codegen

For scoped verification (`gate` already includes its checks):

```bash
mise run fmt lints                       # relevant Rust format and lint checks
mise run codegen:check                   # contracts, schemas, stubs
mise run check:deps                      # crate boundaries on the dependency graph
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

Tests are maintained as product code. A reader must be able to understand the
user behavior, setup, action, and expected outcome in one pass without tracing
through unrelated helpers or knowing the task that introduced the test.

Prefer the most ergonomic public surface that represents how a user actually
works. A test that passes only through awkward setup, private APIs, internal
state inspection, or a test-only workaround is evidence of a product or
harness gap, not a successful user journey. Fix that gap or move the assertion
to the internal tier that owns it.

Follow `AGENTS.md` §16 for style, and these rules specifically:

- **A test must assert.** A test that calls another test and prints a marker
  proves nothing, silently pays that test's full runtime a second time, and
  lends its name to behavior it never checks. Eleven such tests existed in the
  redux oracle group and were deleted.
- **A test must not name a plan.** Task, case, and hypothesis identifiers
  (`T13`, `P27`, `D85`, `H1`) are meaningless once the plan closes and send a
  reader hunting for a document that may be private. Describe the behavior.
- **A test must read as behavior.** Keep setup proportional, call the owning
  public API directly, and assert typed outcomes. A helper earns its place only
  when it removes genuine repetition without hiding the action or expectation.
- **A test must demonstrate the intended ergonomics.** Do not preserve an
  awkward public workflow merely because it can be made to pass. Client-facing
  tests are executable examples of the API Wyrd intends users to adopt.

Python tests use top-level `def test_*` only — never `class TestFoo:`.

### Client-facing tests (Rust, Python, TypeScript SDKs)

SDK tests are the product's public examples. Review every client-facing test
against this checklist:

- [ ] **One story per file, one outcome per test.** The test name states what
  the user gets (`agent_answer_passes_its_verifier`). A story uses the same
  file name, test names, and fixtures in all three SDKs.
- [ ] **Checked-in YAML only.** Cards come from `fixtures/cards/<story>/` at
  the repository root. Test code never builds or edits YAML, JSON, digests, or
  URLs.
- [ ] **Deployment-shaped server.** The session `WyrdTestServer` exports its
  address and key the way a deployment's environment does; SDK and CLI calls
  resolve them without arguments. Only a credentials test passes them.
- [ ] **A second principal is an explicit client.** A test acting as anyone
  but the session administrator builds a `WyrdClient` from that principal's
  key and passes it as the surface's one `client` argument; a state acting as
  a Service is created with that client (`WyrdState.from_path(path,
  client=...)`) and starts Bifrost with no identity argument. Server URLs,
  credentials, and gRPC URLs go only to the `WyrdClient` constructor, and no
  test switches principals by editing the environment.
- [ ] **Public surfaces only.** Public SDK modules, the in-process CLI
  functions, the narrowly scoped test controls (`wait_for_baseline` and
  `make_binding_due`), and the server's credential
  fixtures for principals that are not Card keys. A Service or Agent key
  comes from the CLI `issue_key` and holds `workload`; a Role beyond it is
  assigned through the public `Principals` surface and takes effect for a
  client built from the key after the assignment. No private or extension import,
  subprocess, raw HTTP, SQL against server tables, digest computation, YAML or
  JSON parsing of results, sleep, or polling loop.
- [ ] **Readable without repository archaeology.** The test body shows the
  user action and typed assertion directly. Support code uses domain names,
  stays close to the story, and does not force a reader through generic
  builders, nested wrappers, or implementation-detail fixtures to understand
  the behavior.
- [ ] **Setup is fixtures that return domain objects** (a `WyrdState`, a
  registered Card), not helper functions in the test file. The body acts on
  the SDK and asserts on typed results.
- [ ] **Errors assert one exact catalog code** on the raised `WyrdError`. No
  message matching, no "any of these codes".
- [ ] **Fixed, meaningful names.** No uuid or time suffixes; registering a
  fixture again is idempotent.
- [ ] **Value tables** use `parametrize` / `it.each` / a table loop with one
  assertion shape, never branching inside the loop.
- [ ] **Engine mathematics and internals stay in Rust tests**: PSI bins, SPC
  limits, judge scoring, cache and fence counters, audit staging.
- [ ] **Type-only checks are compile-time**: `expectTypeOf` in `*.test-d.ts`,
  `ty` fixtures outside pytest collection.
- [ ] **Written to be owned.** Fixtures, fixture YAML, `conftest`, and
  `tests/support` are minimal, realistic, typed, and named for the domain.
  Fixture YAML reads as the Card a user would author.

## Where to look next

- `crates/wyrd/wyrd-testing/tests/README.md` — tier-1 binaries, their modules,
  and how to add a journey.
- `crates/vala/vala-bifrost-redux/tests/README.md` — the tier-2 rule and its
  subsystem groups.
- `AGENTS.md` §11 — the normative taxonomy and verification scope.
- `mise.toml` — every lane, with a description on each.
