# Testing and tooling domain review

## Subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Domain: test execution, journey coverage, gate composition, Python tooling,
  focused selectors, and the R11 evidence record.
- Result: **FAIL**

I reviewed the immutable commit objects. An unrelated uncommitted `mise.toml`
edit appeared in the shared worktree during review; it is not part of this
candidate and was neither used as evidence nor modified here.

## Authority and source coverage

| Boundary | Authority | Source/evidence inspected | Result |
|---|---|---|---|
| User-facing behavior needs real SDK-to-server journeys | `AGENTS.md` §11; `architecture/references/languages/testing-workflows.md` | Rust delegation journey in `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs`; Python and TypeScript Bifrost integration journeys | PASS |
| Tests must not pass after selecting or executing nothing | `AGENTS.md` §11; `architecture/agent-rules.md` | Commit `1fec93048`; server, SQL, CLI, identity, and Rust SDK test sources; family and Bifrost lane definitions in candidate `mise.toml` | PASS |
| Ignored journeys need an owning lane | `AGENTS.md` §11; testing workflow reference | Candidate `mise.toml`, `scripts/run-bifrost-tests.sh`, `scripts/run-bifrost-journeys.sh`, and all remaining Rust `#[ignore]` sites | PASS |
| Python production package excludes the test harness | `AGENTS.md` §2, §7; `mise run check:py-wheel-no-testing` contract | Candidate `mise.toml:875-879,978-989`; `sdks/wyrd-sdk-python/src/lib.rs:110-115` | FAIL (`TEST-1`) |
| Named proof is literal, replayable, positively selected, and tied to the final candidate | R11 task `Focused closure proof` and implementation-evidence contract | R11 task evidence at lines 338-365; candidate `check:docs` definition | FAIL (`TEST-2`) |

## Test Coverage Analysis

### Current Coverage

- `1fec93048` removes the `WYRD_{CLI,AUTH,AUTHZ_CHECK,REG}_E2E` early returns;
  the affected server/CLI tests now construct real `WyrdTestServer`/Postgres
  fixtures instead of returning success, and no matching environment-gated
  early return remains in the reviewed test sources.
- The 20 external-identity tests are visibly `#[ignore]`d and
  `test:identity:journey:inner` runs the whole `identity_e2e` target with
  `--run-ignored=all` after starting Keycloak and Dex; the candidate gate owns
  that lane.
- Outside identity, every remaining Rust `#[ignore]` is on a Bifrost target;
  the candidate's `test:bifrost` aggregate runs the ignored Redux and seven
  Rust capability targets with `--run-ignored=all`, plus the Python and
  TypeScript Bifrost journeys.
- The previously ignored CLI query/download/WyrdState tests and Rust SDK
  `cards_state` journey are now ordinary family-lane tests; `test:wyrd` and
  `test:shared` both own a repository-managed Postgres lifecycle.
- `query::service_b_acts_for_service_a_with_only_a_table_authority`
  (`query.rs:1140-1475`) is a real bound-server journey: it proves directed
  A-subject/B-actor allowance, reverse denial, JWT `sub`/`act`/audience and
  narrowed permissions, delegated read, HTTP and native-ingest write refusal,
  direct-B write success, no refused effect, audience binding, and A/B audit
  attribution.
- Python
  `test_delegated_client_reads_as_a_and_cannot_write_with_b_authority`
  (`test_bifrost_e2e.py:998`) and the TypeScript journey
  (`bifrost-write.test.ts:72`) both use their public SDKs against a real test
  server and prove delegated read, delegated write denial before effect, and
  direct-B write success. Environment-only construction and input-conflict
  behavior are also exercised in those integration suites.
- The reported broad results (`test:wyrd` 2024, `test:sql` 247,
  `test:identity:journey` 20, `test:shared` 658, `test:bifrost` 9/9,
  Python integration 57, and TypeScript integration 18) are consistent with
  the candidate's lane composition. The user additionally reports one green
  full gate; no durable gate transcript was supplied, so this review relies on
  the recorded owning-lane results and source inspection rather than inventing
  one.

### Gaps

#### `TEST-1` — the production-wheel boundary check now consumes a testing build

- **Violated rule:** the production Python wheel must not expose test-only
  harness behavior, and a named boundary check must test the property it
  advertises.
- **Location:** candidate `mise.toml:875-879` makes the sole `py:setup` run
  `maturin develop --features testing`; `mise.toml:978-989` then makes
  `check:py-wheel-no-testing` depend on that setup and fail when
  `import wyrd.testing` succeeds; the feature deliberately registers that
  module at `sdks/wyrd-sdk-python/src/lib.rs:110-115`.
- **Evidence:** the check's prerequisite installs the exact feature whose
  absence the check asserts. This is reachable by running the repository's
  named check; it is not a speculative production-wheel concern.
- **Consequence:** `mise run check:py-wheel-no-testing` is deterministically
  false against its own prerequisite and no longer provides credible proof
  that the default production build excludes `wyrd.testing`.
- **Required correction:** keep the single testing-enabled developer setup if
  desired, but have `check:py-wheel-no-testing` install/build the default
  feature extension before its import assertion (or equivalently restore the
  former separate default setup); add no new checker or harness.
- **Focused closure proof:** `mise run check:py-wheel-no-testing` passes on the
  default build, and `mise run py:test:testing` still passes with the testing
  build.

#### `TEST-2` — the R11 evidence is not the exact replayable record the task requires

- **Violated obligation:** R11 lines 333-336 require the final candidate and a
  literal repository-native command plus positive selected count for every
  named test, and line 330 requires strict rustdoc for every affected crate.
- **Location and evidence:** the evidence header names `395a0dee5` rather than
  final candidate `261168087`; R11-2 uses a `<name>` placeholder and omits the
  full command; R11-6 and R9-2 also record command fragments; the Python
  focused selection omits the named environment-only construction test; and
  R11-7 cites only `mise run check:docs`, whose candidate definition documents
  just `wyrd-spec`, `wyrd-auth-issue`, and `wyrd-auth-verify` despite claiming a
  ten-crate R9/R10 inventory.
- **Consequence:** the implementation has strong broad coverage, but a reviewer
  cannot replay all claimed focused results or establish the required strict
  rustdoc scope from the committed evidence.
- **Required correction:** amend only the evidence record to identify the final
  candidate, expand every placeholder/fragment into the literal command that
  ran with its positive count, include a positive focused selection for the
  Python environment-only path, and record the exact strict-rustdoc command(s)
  and package list covering every affected crate. No new test or permanent
  checker is warranted.
- **Focused closure proof:** review the amended table against the actual named
  selectors; rerun only any command for which a real prior result cannot be
  supplied.

### Recommended Verification

- `mise run check:py-wheel-no-testing` — proves the default Python extension
  excludes the harness after `TEST-1` is corrected.
- `mise run py:test:testing` — proves the same correction did not break the
  intentional test-only build.
- The literal focused commands recorded for R11-1 through R11-7 — establishes
  positive selection without relying on abbreviated evidence.
- Strict rustdoc with
  `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links"` for every
  crate in the R9/R10 documentation inventory — closes the currently unproved
  part of R11-7.
- `git diff --check c5c20754a167e8f4d74a555a720bd51df6179a6f <final-candidate>`
  — final whitespace check.

### Residual Risk

- No durable raw transcript for the reported full gate was committed. That is
  not itself a finding because the owning-lane evidence and source paths are
  sufficient once `TEST-2` is corrected.
- The test changes substantially increase default family-lane runtime, but
  they close real false-positive execution and no evidence shows an incorrect
  lane boundary; runtime optimization is out of scope.

## Overall result

**FAIL.** The required public journeys are real, correctly owned, and no longer
silently skip, but the Python production-boundary check contradicts its own
setup and the committed R11 evidence does not meet the task's exact replayable
proof contract.
