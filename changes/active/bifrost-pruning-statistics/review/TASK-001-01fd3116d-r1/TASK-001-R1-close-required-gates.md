---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-bifrost-pruning-statistics
spec_revision: 3
requirements: [AC-005, INV-004, INV-005]
depends_on: []
parent_task: TASK-001
remediates: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3]
---

# Close required dependency gates and bounded Wyrd standards gaps

## Authority and acceptance context

Implement under [approved specification revision 3](../../spec.md) and the
[original TASK-001](../../tasks/TASK-001-dependency-owned-pruning.md).
Retain the [review verdict](verdict.md), [validated ledger](findings-validation.md),
and their reports as acceptance context.

Reviewed Wyrd root:
`/home/thorrester/Documents/GitHub/wyrd-worktrees/bifrost-pruning-statistics`.
Original base: `d7de12a74c4bdbf93e727cc2383147e34298f689`.
Prior candidate: `01fd3116d2501d89317073a2abb547818a23b4a8`.

Reviewed DataFusion root:
`/home/thorrester/Documents/GitHub/datafusion-pruning-statistics`.
Cumulative dependency base: `19796a13ed46877cf998013886947b28a6f774b3`.
Prior candidate: `9c724e501b613ea1e3de6a7e8d9fb700eed65b25`.

The review established no material runtime correctness defect in the statistics
fix. Preserve the accepted behavior and proof; do not redesign it to address
these findings. Revalidate applicable AGENTS and normal owner instructions in
both repositories. Git scheduling, commits, pushes and publication remain
caller-owned; this remediation does not authorize them.

## Diagnosis and selected correction

### FIND-TASK-001-1: dependency delivery proof is incomplete

The delivered dependency is consumed by eleven patches in Wyrd `Cargo.toml`,
the regenerated lockfile and `ORACLE_DATAFUSION_REVISION`. Its own `AGENTS.md`
requires format, all-target/all-feature Clippy, contributor checks and extended
workspace tests. The original task explicitly preserves those requirements.
Its implementation evidence instead records common/pruning-only Clippy,
unresolved `schema_rewriter.rs` formatting and no full extended suite. Wyrd
checks and focused dependency tests do not satisfy independent fork authority.

This is an explicit mandatory-gate violation, not an allegation of a hidden
runtime bug. The smallest correction is to complete and record the fork's
existing required commands on its declared toolchain, repair actual failures at
their owners without weakening checks, and identify the final delivered source.
No new validator, test framework, benchmark service or Wyrd aggregate is needed.
Pre-existing failure origin does not waive a mandatory gate. Keep repair scope
to actual check failures; unrelated features are not authorized.

Read and satisfy the exact current fork `AGENTS.md` and referenced contributor
requirements. The reviewed minimum command set, executed in the fork through
Wyrd's required `mise` invocation discipline and the fork's pinned toolchain, is:

```bash
mise exec -- cargo fmt --all
mise exec -- cargo clippy --all-targets --all-features -- -D warnings
mise exec -- ./dev/rust_lint.sh
mise exec -- env RUST_BACKTRACE=1 cargo test --profile ci --exclude datafusion-examples --exclude datafusion-benchmarks --exclude datafusion-cli --workspace --lib --tests --bins --features avro,json,backtrace,extended_tests,recursive_protection,parquet_encryption
```

Confirm the actual Rust toolchain used; the review-time focused commands did
not establish pinned-toolchain proof. Equivalent existing command/script reuse
may avoid duplicate work only when its full coverage is recorded. These broad
fork checks are preserved mandatory delivery obligations, not new remediation
aggregates. No directly applicable local conversion benchmark was established;
retain the applicability assessment and bounded cost evidence rather than
inventing a benchmark harness or requiring unrelated benchmarks.

If necessary gate repairs change the fork revision, Wyrd's existing pin owner
must consume the final compatible immutable revision consistently in all eleven
patches, Cargo-generated lockfile and codec constant. Preserve source authority
and revision fingerprint semantics. Do not hand-edit Cargo.lock or ship local
path/reference overrides. If delivery needs an unpublished revision, finish local
verification and report that delivery dependency; do not claim final pin completion.

### FIND-TASK-001-2: changed items miss mandatory accurate rustdoc

`scan_sizing.rs:589` says zero batches keep nothing without evaluating the
predicate, but guard removal now delegates the aligned zero-input case to
DataFusion's predicate. The result is correct; the method description is stale.
Four new items in `published.rs:1773–1777`—`SPANS`, `OWNER`, `NEIGHBOUR`, and
`HOUR_NANOS`—lack rustdoc. Root rules explicitly make changed-item rustdoc a
merge gate, including private/test items.

Update the existing method's empty-input description and add short meaningful
rustdoc for the existing constants' fixture roles. Convert the existing OWNER
rationale into rustdoc where useful. Preserve values, executable behavior,
conservative fallback and fixture construction. A few documentation lines close
the gap; no fixture rewrite, new helper or runtime test is needed.

### FIND-TASK-001-3: new imports violate the module placement constraint

New concrete imports are inside these nongeneric function bodies:

- W1 in `scan_sizing.rs:855–857`.
- W2 in `oracle/live.rs:1611–1614`.
- J1 helper in `published.rs:1771`.

They do not meet either exception in `agent_docs/rust.md`. Existing module/test
import groups already own these names. Move only newly introduced imports there,
merging existing groups and duplicates. Keep unit-test imports in their current
`cfg(test)` module. No new module, alias, wrapper, production dependency or lint
allowance is needed. Do not sweep neighboring old violations. This is a bounded
explicit-rule correction with no runtime change.

## Constraints and non-goals

- Preserve exact/unknown/inexact/typed-null behavior, container order, diagnostics
  scrubbing and quiet empty selections. No new Wyrd empty guard or generic helper.
- Preserve SDK rows, null/duplicate behavior, source cuts/captures, tenant footer
  proof, resource accounting and the existing dispatch/failure boundaries.
- No public/persisted contract, schema, feature, dependency, UI or statistics
  collection change. No new benchmark/logging framework or unrelated fork upgrade.
- Do not suppress checks, ignore tests, widen boundary globs or dismiss actual
  failures as pre-existing. Diagnose failures before changing tests or assertions.
- If an actual repair requires material behavior outside the approved conversion
  contract, preserve its reproducer and return for specification authority instead
  of silently widening this task. Routine formatting/import/doc repair is local.

## Acceptance and focused closure proof

| Finding | Required observation | Proof |
|---|---|---|
| FIND-TASK-001-1 | Existing required fork delivery checks pass on final source and actual pinned toolchain; final Wyrd pin/lock/fingerprint agree if changed | Record exact commands, results/counts, toolchain and dependency identity; static pin/lock/fingerprint audit; focused affected regression proof for any executable repair |
| FIND-TASK-001-2 | Empty-input documentation is accurate; all four fixture constants have useful rustdoc | Static changed-item inspection and formatting/diff checks; executable evidence carries forward for doc-only changes |
| FIND-TASK-001-3 | Only new local imports move to their existing owning module groups; compilation/lint remains clean | Static import inspection and narrow library-test/Oracle-target compile or lint proof; no runtime change required |

The Wyrd documentation/import-only write set can use these focused checks:

```bash
mise exec -- cargo fmt -p vala-bifrost-redux -p wyrd-testing -- --check
mise exec -- cargo check --locked -p vala-bifrost-redux --tests
mise exec -- cargo check --locked -p wyrd-testing --test oracle
mise exec -- git diff --check
```

Reuse existing flags/features appropriate to each target. If checking imports
with lint instead of compile, use the existing owner's narrow lint coverage;
do not invent a new gate. Compile-only coverage is sufficient for pure import
movement. Documentation-only edits need no RED or runtime rerun.

Only if dependency executable code or the final consumed revision changes,
rerun the focused D1–D4/W1–W3 and codec proof affected by that change, using the
original task's exact commands. The compatibility test's focused command is:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(=oracle::codec::tests::plan_fingerprint_binds_the_pinned_datafusion_revision)'
```

No full Wyrd journey suite, `gate`, or broad Wyrd aggregate is required by this
remediation. Integrated capability and cross-surface closeout remain at change
review. Do not repeat already-passing broad checks solely for doc/import edits.
Run Cargo work sequentially per target directory.

Record closure evidence per stable finding ID and final revisions. Preserve
original task and reviewed candidate references. Route implementation to
`$wyrd-implement`; subsequent task review checks this delta and affected paths
while carrying forward accepted unaffected runtime evidence.

## Implementation evidence

Final revisions:
- Wyrd: `01fd3116d` → `e8a53ae44` (docs/imports) → `9eae0c7fe` (pin).
- DataFusion: `9c724e501` → `b669163b78a7692a9a381d4d13dceeae632c5c34`. This was a fast-forward push to `origin/wyrd-production`.

Toolchain: rustc/clippy/rustfmt `1.98.1`, matching the fork's `rust-toolchain.toml`. The Wyrd `mise` environment exports `RUSTUP_TOOLCHAIN=1.99.0`, so every fork command ran as `mise exec -- env RUSTUP_TOOLCHAIN=1.98.1 CMAKE=<mise cmake 3.26.4> PROTOC=<mise protoc 33.2> …`. The fork directory has no mise version for the cmake/protoc shims, so they were pointed at the installed binaries; no mise config was changed.

### FIND-TASK-001-1: fork delivery gates

Each failure was diagnosed and repaired at its owner, without weakening any check.

| Gate | First failure | Cause and repair | Final |
|---|---|---|---|
| `cargo fmt --all` | `schema_rewriter.rs:1024` | rustfmt line-width drift from `1eab5493e`; `9c5c18bff` formats it | PASS |
| `cargo clippy --all-targets --all-features -- -D warnings` | none | — | PASS |
| `dev/rust_lint.sh` (all 14 steps), run through `uv run` as the script requires; it installed its missing tools itself | taplo: `Cargo.toml` | `functions-variant` key out of order; `c65de7b5f` sorts it | PASS |
| | `cargo audit`: 16 advisories (RUSTSEC-2026-0338…0410) | locked `ctutils`, `quinn-proto`, `rustls`, `rustls-webpki`, `webpki-root-certs`, `zstd`, `zstd-safe`, `zstd-sys` had published advisories; `77f6fa9dc` makes a lockfile-only semver-compatible update. The three remaining unmaintained/unsound/yanked warnings are the script's allowed warnings | PASS |
| | `cargo machete`: `datafusion-functions-variant` | no source references `datafusion-expr-common` or the dev `tokio`; `1396cc096` drops both | PASS |
| extended `cargo test --profile ci … --features avro,json,backtrace,extended_tests,recursive_protection,parquet_encryption` | `core_integration` `sort_mem_validation` ×4 | See the diagnosis below; `f18af5b12` | PASS |
| | `parquet_integration` `invalid_stored_variants_are_errors` | See the diagnosis below; `b669163b7` | PASS |

On `b669163b7`: fmt=0, clippy=0, rust_lint=0, extended test=0. The extended run printed 70 `test result` lines with 12,029 passed, 0 failed and 8 ignored; all 8 ignores are pre-existing upstream ignores.

**Diagnosis: memory-limit validation.**
- **Symptom:** every failing run printed `Memory before: 0.0 B`, then failed with "Memory usage exceeded the theoretical limit", for example 298.0 MB against 228.9 MB.
- **Evidence:** in `utils.rs`, `measure_max_rss`'s monitor first ran `System::new_all()`, a full host scan that took about 370–380 ms on this 919-process host. That is longer than the 14 ms baseline query plus the 200 ms grace sleep, so the monitor was dropped before its first sample. strace showed no read of the test's own `/proc/<pid>/statm` during the baseline.
- **Cause:** the baseline peak was never sampled, so the whole process RSS was compared against a delta limit.
- **Fix site:** the private `measure_max_rss`, which every `memory_limit_validation` caller (sort and SMJ) goes through. The monitor now uses `System::new()`, since the per-process refresh loads the pid by itself, and the unused up-front full scan is gone. The limits and assertions are unchanged.
- **Diagnostician report:** a fresh read-only diagnostician gave the same cause and fix site. Result: 19 passed, plus 1 upstream ignore; `sort_no_mem_limit` now measures 99.7→168.9 MB, a 69.2 MB delta.

**Diagnosis: invalid stored Variants.**
- **Symptom:** `SELECT variant_get(v, 'k')` on an unshredded malformed value (a root list with a truncated element) returned no error.
- **Evidence:** the arrow-rs pin `ca5feff` that came in with base `19796a13e` ("Validate a Variant only along the path variant_get reads") changes `shredded_get_path` to use `try_value_unvalidated`. In `try_get_path`, `(List, Field)` → `Ok(None)`. A probe confirmed that `try_new` gives Err and `try_get_path(["k"])` gives `Ok(None)`.
- **Cause:** the test was last changed in `ab03f6835`, before the pin, and still expected full-row validation from a lookup that never reaches the corrupt bytes. Upstream deliberately made `variant_get` cost the path's depth, not the size of the row.
- **Fix site:** the test's lookup path. It is now `variant_get(v, 0)`, which reaches the corrupt bytes in all three cases (bad metadata, truncated element, excess depth). It keeps the test's "error, never a panic" property without restoring the full-row cost. The crate doc in `functions-variant/src/lib.rs` now states that only bytes a function reads are an error.
- **Diagnostician report:** a fresh read-only diagnostician gave the same cause and recommended the test-side fix. The cumulative diff does not cause the failure.
- **Result:** 12 of 12 `parquet::variant` tests pass with `--features …,variant`.

Benchmarks: the earlier applicability assessment stands. No local benchmark exercises `PrunableStatistics` conversion, and none was invented.

Wyrd pin owner: all 11 `[patch]` revisions, the Cargo-regenerated `Cargo.lock` (35 `b669163b` sources, 0 references to the old rev, and one dropped `datafusion-expr-common` edge from `functions-variant`), and `ORACLE_DATAFUSION_REVISION` all agree on `b669163b78a7692a9a381d4d13dceeae632c5c34`. No path or local override was shipped.

Because the consumed revision changed, the affected proof was rerun on the new pin:
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --no-tests=fail -E 'test(/^oracle::live::tests::/) | test(/^oracle::memtable_source::tests::/) | test(/^scan_sizing::tests::/) | test(=oracle::exec::tests::hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row) | test(=oracle::codec::tests::plan_fingerprint_binds_the_pinned_datafusion_revision)'` passed 20 of 20. This covers W1–W3, the owner set, the footer refusal test and the codec pin test.
- J1 `published::observe_predicates_preserve_rows_across_live_and_persisted_sources` and J2 `published::time_column_range_prunes_published_files` both pass, using the original task's `with-test-postgres.sh` journey command.
- D1–D4 live in the fork's `common` and `pruning` crates and are covered by the passing extended workspace run.

### FIND-TASK-001-2: rustdoc

- `scan_sizing.rs` `BatchPruning::keep`: the doc now says that zero batches with zero aligned bounds reach the predicate like any other input and keep nothing.
- `published.rs` `prove_observe_predicates`: `SPANS`, `OWNER`, `NEIGHBOUR` and `HOUR_NANOS` each have rustdoc describing their fixture role. The `OWNER` rationale comment is now rustdoc. Values are unchanged.

### FIND-TASK-001-3: imports

- W1's three imports moved to `scan_sizing::tests`.
- W2's four imports were merged into `oracle::live::tests`: `TimeUnit` joins the existing `arrow::datatypes` group, and `FIXTURE_TIME_COLUMN` is imported through `crate::oracle::exec::tests`.
- J1's `canonical_signals` import moved into `published.rs`'s module import group.
- Older function-scoped imports nearby were left alone.

| Finding | Implementation | Verification | Result |
|---|---|---|---|
| FIND-TASK-001-1 | fork `9c5c18bff..b669163b7`; Wyrd `9eae0c7fe` | fork fmt/clippy/rust_lint/extended suite on 1.98.1; pin/lock/codec audit; 20 focused tests + J1 + J2 | PASS |
| FIND-TASK-001-2 | Wyrd `e8a53ae44` | static inspection; `cargo fmt -p vala-bifrost-redux -p wyrd-testing -- --check` | PASS |
| FIND-TASK-001-3 | Wyrd `e8a53ae44` | `cargo check --locked -p vala-bifrost-redux --tests`; `cargo check --locked -p wyrd-testing --test oracle`; `mise run lints` | PASS |

Wyrd checks on `9eae0c7fe` pass: `mise run fmt`, `lints`, `check:deps`, `check:tenant-isolation`, `check:storage:drift`, and `git diff --check`.

Non-goals held:
- No change to statistics behavior, public or persisted contracts, schemas, features or UI.
- No new Wyrd dependency, guard, helper, benchmark, logging framework, allowance or ignore.
- The fork's lockfile security update and its unused-dependency removal are gate repairs required by the fork's `rust_lint.sh`; neither touches Wyrd's dependency set beyond the dropped lock edge.

Limits:
- The two fork test repairs touch upstream-owned test code, `memory_limit_validation/utils.rs` and `tests/parquet/variant.rs`.
- The advisory database is time-dependent: newly published advisories can turn `cargo audit` red again without any code change.
