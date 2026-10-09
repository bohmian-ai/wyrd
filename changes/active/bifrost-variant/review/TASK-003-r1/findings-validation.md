# TASK-003 r1: findings validation (`ponytail-rev`)

The orchestrator saved this from the validator's returned text, because the
harness refused the subagent's own write. The content is unchanged.

- Subject: `b145767dd` → `861a0342f`.
- Lanes the validator ran:
  - `check:docs` and `check:examples`: exit 0.
  - `check:test-coverage`: exit 1. It is in `gate`.
  - `check:rustls-provider`: exit 1. It is in no gate.
  - `cargo hakari generate --diff`: non-empty.
- No source was modified.

## Discovery status

| ID | Status | Reason | FIND |
|---|---|---|---|
| BEH-1 | CONFIRMED | Both digests (`peer.rs:312` `stage_body_digest`, `codec.rs:27` `physical_plan_fingerprint`) lost `ORACLE_VARIANT_SQL_VERSION` and gained no fork revision; plan:165 requires one | 1 |
| BEH-2 | CONFIRMED | The `events` List fixture is never queried; no test uses `unnest`; the distributed nested read (`peer_network/analytical.rs:853`) asserts neither leaves nor bytes | 2 |
| BEH-3 | CONFIRMED | `prove_late_failures` now uses `if let Some`; the Rust late case was deleted; Python and TS accept 0 delivered rows | 3 |
| BEH-4 | REVISED | Only the wording of the mechanism is wrong; the fixture reshape is valid | 4 |
| BEH-5 | CONFIRMED | `wyrd.v1.bin` still holds removed messages; this diff's `Cargo.toml` causes the hakari diff | 5 |
| BEH-6 | REVISED | The violation is real, but production writer code uses `parquet-variant`; fixing it needs a spec decision | SPEC-1 |
| BEH-7 | REJECTED | Lanes are evidenced; only the drift covered by FIND-5 and FIND-6 remains | — |
| BEH-8 | CONFIRMED | A pruning metric is documented with no emitter; the `hot_parquet_scan` and `row_count` docs are false | 7 |
| BEH-9 | REJECTED | Nothing requires a journey test of a validated upstream default | — |
| INV-1 | CONFIRMED | `PublishedAssignment`/`assigned` has one call site, which passes `None` | 10 |
| INV-2 | CONFIRMED | The boundary's `Stage::Local` input and `children()` return the same plan, so it is walked twice; units are mixed | 11 |
| INV-3 | CONFIRMED | Duplicate of BEH-1 | 1 |
| INV-4 | CONFIRMED | `bind` never checks `PartitionedFile` locations against `tenants/{tenant}/` (`spec.md:338-340`) | 12 |
| INV-5 | CONFIRMED | Duplicate of BEH-5 | 5 |
| INV-6 | REVISED | Duplicate of BEH-6 | SPEC-1 |
| STD-1 | CONFIRMED | Proto drift (BEH-5 cluster) | 5 |
| STD-2 | CONFIRMED | Workspace-hack drift; `check:workspace-hack` is in `check`, which is in `gate` | 5 |
| STD-3 | CONFIRMED | "spec REQ-032"/"spec REQ-034" references will point nowhere once the packet is deleted | 15 |
| STD-4 | REVISED | Confirmed for the new items; the `signer.rs` part disappears with FIND-17 | 16 |
| STD-5 | CONFIRMED | Function-scoped `use` statements in non-test `bench_support.rs` | 16 |
| STD-6 | CONFIRMED | Ten new panicking helpers lack `# Panics` (AGENTS §16 hard blocker) | 18 |
| STD-7 | REJECTED | A shape preference on fold code that FIND-11 largely deletes | — |
| STD-8 | CONFIRMED | `check:test-coverage` is red and in `gate`; `gate` applies because the toolchain and root patch set changed | 6 |
| STD-9 | REJECTED | `check:rustls-provider` is in no gate, CI or verify lane; the dependency chain is untouched | — |
| STD-10 | CONFIRMED | The bench task claims the `dist` profile without passing `--profile dist` | 19 |
| MNT-1 | CONFIRMED | `state.rs:1903-2018` shutdown docs describe things that don't exist; `abort_selected_owners` is async with no await | 7 |
| MNT-2 | CONFIRMED | Stale or misplaced docs across `analytical.rs`, `follower.rs`, `exec.rs`, `session.rs`, `bench_support.rs` | 7 |
| MNT-3 | CONFIRMED | `reconciliation_limit_bytes` has no reader but can still refuse boot (`boot/mod.rs:1582-1588`) | 8 |
| MNT-4 | CONFIRMED | `from_object_store`, `PermissionDenied` and `Backend` are constructed only under `cfg(test)` | 9 |
| MNT-5 | REJECTED | No path can produce `Some(0)` | — |
| MNT-6 | CONFIRMED | Signer duplication; the correction is revised so it needs no `#[allow]` | 17 |
| MNT-7 | CONFIRMED | Duplicate of STD-10 | 19 |
| MNT-8 | CONFIRMED | Duplicate of STD-4/5 | 16 |
| MNT-9 | CONFIRMED | `// ponytail:` tag at `forge/compact.rs:182` | 15 |
| SYS-1 | CONFIRMED | Duplicate of BEH-1 | 1 |
| SYS-2 | REVISED | Planning IO runs outside `timeout_at`; the pre-admission part is rejected because plan:575 approves it | 13 |
| REU-1 | REJECTED | A deliberate per-query template clone with no reachable failure | — |
| REU-2 | REVISED | Reachable; the dependency it needs (`unshred_variant`) is already installed | 20 |
| REU-3 | REJECTED | Test-only | — |
| REU-4 | CONFIRMED | Duplicate of MNT-6 | 17 |
| REU-5 | REJECTED | Test-only | — |
| SEC-1 | CONFIRMED | Duplicate of BEH-1 | 1 |
| SEC-2 | CONFIRMED | The refusal branches of `bind` are untested | 12 |
| RES-1 | REVISED | Duplicate of SYS-2 | 13 |
| RES-2 | CONFIRMED | `bifrost-design.md:715-718` "no precharge" contradicts :1231-1233 and `with_metadata_cache` | 7 |
| STO-1 | CONFIRMED | `converge` always PUTs, so recovery re-uploads published keys | 14 |
| STO-2 | CONFIRMED | Duplicate of BEH-8 | 7 |
| FUP-1 | CONFIRMED | Duplicate of INV-4 | 12 |
| FUP-2 | CONFIRMED | Wrong Group A mechanism at 5 fixture sites and plan:618-627 | 4 |
| FUP-3/4/5 | CONFIRMED | Scan stats double count, mix units, and count the head-stage scan twice | 11 |

## Ledger

Paths are relative to `crates/vala/vala-bifrost-redux/src` unless stated
otherwise.

**FIND-003-1 — MISSING: the peer contract does not bind the fork revision**
- Sources: BEH-1, SYS-1, SEC-1, INV-3.
- Obligation: plan:165, "the peer fingerprint carries the fork revision".
- Location: `oracle/peer.rs:312` and `oracle/codec.rs:27`. No
  fork-revision constant exists anywhere in `crates/`, and the protocol
  versions are unchanged.
- Consequence: in a rolling deploy, a worker built on another fork revision
  accepts the plans with no typed refusal.
- Correction:
  - Add one `const ORACLE_DATAFUSION_REVISION` (`a4ff1455…`) in `codec.rs`.
  - Hash it into both digests through one private helper that takes the
    revision as a parameter.
  - Do not bump the protocol versions.
  - Add tests: a different revision gives a different digest, follower
    preflight refuses the mismatch, and the constant matches the root
    `Cargo.toml` pin (`include_str!`).
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::codec::tests::fingerprint_binds_datafusion_revision) | test(=oracle::codec::tests::datafusion_revision_matches_workspace_pin)'`

**FIND-003-2 — MISSING: AC-007 has no List element, `unnest` or distributed nested-leaf proof**
- Sources: BEH-2.
- Location: `oracle/nested_pushdown.rs:196-345`;
  `crates/wyrd/wyrd-testing/tests/bifrost/peer_network/analytical.rs:853`.
- Correction:
  - Add two `HotTable` tests asserting the decoded leaves for
    `events[1]['attributes'] ->> 'k'` and
    `unnest(events)['attributes'] ->> 'k'`.
  - Extend the :853 journey to assert that
    `oracle_query_bytes_scanned_total` for the nested leaf is less than for
    `SELECT events`.
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::nested_pushdown::tests::.*(list_element|unnest)/)'` and `mise run test:bifrost:journey:oracle`.

**FIND-003-3 — MISSING: the REQ-019 ¶2 late terminal is no longer proven**
- Sources: BEH-3.
- Location: `crates/wyrd/wyrd-testing/tests/bifrost/oracle/published.rs:932`;
  the Rust late case deleted at `pg_bifrost_e2e.rs` ~2858; the weakened
  Python and TS assertions.
- Cause: the 16384-row Inexact count triggers a round-robin repartition, so
  the failing row can land in the first batch.
- Correction:
  - Make the late fixture one ordered, single-partition read, so the failure
    lands after the first 8192 rows.
  - Restore the mandatory terminal with `delivered == 8192` on Rust (Oracle
    and SDK), Python and TS.
- Closure: `mise run test:bifrost:journey:oracle`, `:sdk`, `:python` and
  `:typescript`.

**FIND-003-4 — INCORRECT (docs): wrong Group A mechanism**
- Sources: BEH-4, FUP-2.
- Location:
  - `analytical_lifecycle.rs:105-109`
  - `analytical_activation.rs:744-746`
  - `capacity.rs:758-762, 820-824, 1815-1819`
  - plan:618-627
- Correction: reword each site to "one object, one file group, a single
  partition, so no stage boundary".
- Closure: `git grep -n "one task per published object" -- crates changes`
  returns nothing.

**FIND-003-5 — REGRESSION: stale `wyrd.v1.bin` and workspace-hack drift**
- Sources: BEH-5, STD-1, STD-2, INV-5.
- Correction: run the `WYRD_PROTO_SNAPSHOT=1` rebuild, then
  `mise exec -- cargo hakari generate && mise exec -- cargo hakari manage-deps`.
- Closure: `mise run check:proto-drift && mise run check:workspace-hack`.

**FIND-003-6 — VIOLATION: `check:test-coverage` is red in `gate`**
- Sources: STD-8.
- Correction: add `wyrd-types` to `FAMILY_SHARED` in
  `scripts/test-families.sh`.
- Closure: `mise run check:test-coverage`.

**FIND-003-7 — INCORRECT/DRIFT: touched docs contradict behavior**
- Sources: BEH-8, STO-2, MNT-1, MNT-2, RES-2, plus the plan status table.
- Correction:
  - Delete `bifrost_oracle_file_pruning_total` from
    `architecture/bifrost-design.md:1221`.
  - Rewrite `architecture/bifrost-design.md:715-718` to state the
    composition-time footer-cache charge.
  - Fix the `hot_parquet_scan` (`exec.rs:1007`), `row_count`
    (`exec.rs:1243`) and `scan()` (~1547) docs.
  - In `state.rs:1903-2018`, remove the false shutdown and abort claims,
    make `abort_selected_owners` a plain `fn`, and fix the :1817 doc.
  - Fix the MNT-2 sites:
    - `analytical.rs:7144`
    - `follower.rs:56, 768-774, 810/856`
    - `exec.rs:1-5, 384, 1715`
    - `session.rs:1`
    - `bench_support.rs:224, 293, 443`
    - the bench main
    - the `exec.rs:3466` test name
  - Update the plan status table at lines 18-25.
- Closure: `git grep -n bifrost_oracle_file_pruning_total -- architecture crates`
  returns nothing, then `mise run docs:check` and `mise run lints`.

**FIND-003-8 — DRIFT: orphaned reconciliation budget still refuses boot**
- Sources: MNT-3.
- Location: `oracle/mod.rs:239`; `boot/mod.rs:1582-1588` and :1630.
- Correction: delete the field, its computation, the zero-budget refusal and
  the argument.
- Closure: `git grep -n reconciliation_limit_bytes -- crates` returns
  nothing, then
  `mise exec -- cargo check --locked -p vala-bifrost-redux -p wyrd-server --all-features`.

**FIND-003-9 — DRIFT: test-only error constructor and variants in production code**
- Sources: MNT-4.
- Location: `storage/error.rs:42`.
- Correction: delete `from_object_store`, `PermissionDenied` and `Backend`;
  the test asserts the variant production actually returns.
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/storage::/)'`.

**FIND-003-10 — DRIFT: dead `PublishedAssignment`**
- Sources: INV-1.
- Location: `oracle/exec.rs:1111/1150`; the single caller at :1503 passes
  `None`.
- Correction: delete the type, the parameter and the `Some` branch.
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::exec::tests::/)'`.

**FIND-003-11 — REGRESSION: scan stats double-count and mix units**
- Sources: INV-2, FUP-3/4/5.
- Location: `oracle/exec.rs:262-394, 425-485, 1596`. At
  `datafusion-distributed` `0b81b12`, `children()` of the network
  shuffle/coalesce/broadcast nodes returns the same `local_plan()` that the
  boundary branch also walks.
- Correction:
  - Delete the boundary branch in both `visit` and the fold.
  - Count `PartitionedFile` ranges on both sides, replacing
    `distinct_files`.
  - For a `DistributedExec` root, take bytes and pruning only from the fold.
  - Add a unit test on the `physical_root_alone_selects_query_class`
    scaffold over two files: a plain scan and a GROUP BY report equal
    counts, with each range counted once.
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/oracle::exec::tests::distributed_scan_stats_count_each_range_once/)'`.

**FIND-003-12: spec wording (revised by user decision, 2026-10-09)**
- Sources: INV-4, FUP-1, SEC-2.
- Original claim: `TenantWarehouseReads::bind` (`oracle/exec.rs:897-990`)
  has no pre-execution path-prefix refusal, which the spec text
  (`spec.md:338-340`) asked for.
- Decision: no code change.
  - Every worker scan already goes through `TenantParquetReaderFactory`,
    which refuses a file whose footer is missing or names another tenant
    before any row is decoded.
  - `decoded_stage_scans_read_only_as_the_bound_tenant` and
    `hot_parquet_refuses_foreign_or_missing_footer_tenant_before_any_row`
    prove this.
  - The footer proof checks the file's contents, which is stronger than
    checking its path.
- Correction: revision 26 now names the footer proof as the tenant
  boundary and drops the prefix wording.
- Closure: those two tests pass unchanged, and the wording is present in
  `spec-revision-26-datafusion-variant.md`.

**FIND-003-13 — REGRESSION: planning footer IO escapes the deadline**
- Sources: SYS-2, RES-1.
- Location: `oracle/mod.rs:2859-2902`, against `bifrost-design.md:643` and
  :804.
- Correction:
  - Wrap `plan_physical` in `timeout_at`, reusing the `mod.rs:3702`
    pattern, and map the timeout to `QueryTimeout`.
  - Add a stall journey using `arm_stall()` with a short deadline.
- Closure: `mise run test:bifrost:journey:oracle`.

**FIND-003-14 — REGRESSION: recovery re-uploads published keys (REQ-034)**
- Sources: STO-1.
- Location: `parquet/object_uploader.rs:219` `converge`, reached from
  `persistence.rs:1610`.
- Correction:
  - Call `verify_remote` first. On `Ok`, return with no PUT; on `NotFound`
    or `ContentContradiction`, fall through to the existing put-and-verify
    loop.
  - Add a test with a PUT-counting store expecting 0, 1 and 1 PUTs for a
    matching, absent and contradicting key.
- Closure: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/parquet::object_uploader::tests::converge_/)'`.

**FIND-003-15 — VIOLATION: plan and agent references in permanent code**
- Sources: STD-3, MNT-9.
- Location: `Cargo.toml:230`, `parquet/object_uploader.rs:157`,
  `forge/compact.rs:182`.
- Closure: `git grep -nE 'REQ-0[0-9]{2}|ponytail:' -- Cargo.toml crates/`
  returns nothing.

**FIND-003-16 — VIOLATION: function-scoped imports and fully qualified names in new items**
- Sources: STD-4, STD-5, MNT-8.
- Location: `bench_support.rs:293-462`, `bindings.rs:242`,
  `resources.rs:4074`, `mod.rs:1143`, forge support.
- Correction: hoist the imports to the top of each module (cfg-gated where
  needed) and use bare names.
- Closure: `rg -n '^\s{5,}use ' crates/vala/vala-bifrost-redux/src/bench_support.rs`
  returns nothing, then `mise run lints`.

**FIND-003-17 — VIOLATION: each `BackendSigner` operation is defined twice**
- Sources: MNT-6, REU-4.
- Location: `crates/wyrd/wyrd-storage/src/signer.rs`.
- Constraint: the pedantic lint set includes `unused_async`, and the lane at
  `mise.toml:54` runs without `--all-features`.
- Correction:
  - Write one
    `fn op(&self, ..) -> impl Future<Output = Result<..>> + '_ { async move { match self {..} } }`
    per operation and delete the non-cloud copies.
  - This needs no `#[allow]`.
- Closure: `mise exec -- cargo clippy --locked -p wyrd-storage --all-targets -- -D warnings`,
  then the same command with `--all-features`.

**FIND-003-18 — VIOLATION: ten new panicking helpers lack `# Panics`**
- Sources: STD-6.
- Location:
  - `forge/compact.rs:463`
  - `forge/orphan_gc.rs:2294`
  - `oracle/exec.rs:2951, 2982, 3091`
  - `parquet/object_uploader.rs:407`
  - `wyrd-spec/src/vala/assignment_authority.rs:276, 289`
  - `benches/bifrost_reads/main.rs:38, 65`
- Correction: add the `# Panics` sections, and a rustdoc for `HOUR_MICROS`
  at `oracle/live.rs:976`.
- Closure: `mise run lints` and a re-read of the items.

**FIND-003-19 — INCORRECT: the bench task claims the `dist` profile**
- Sources: STD-10, MNT-7.
- Location: `mise.toml:554-556`.
- Correction: add `--profile dist` to `run`.
- Closure: `mise tasks info bench:bifrost:reads`.

**FIND-003-20 — DRIFT: Wyrd re-implements the Variant shredded-layout walk**
- Sources: REU-2.
- Location: `crates/shared/wyrd-queue/src/variant.rs:504-640`. It runs on
  every result cell, through `variant_cell_to_json` at :383 and the JSON
  encoder at :434.
- Correction:
  - In `mask_placeholders`, call
    `parquet_variant_compute::unshred_variant` (already a dependency), then
    run the existing `validate`.
  - Delete `ShreddedResiduals`.
  - If a depth bound has to run first, put it in the arrow-rs fork.
- Closure: `mise exec -- cargo nextest run --locked -p wyrd-queue --lib -E 'test(=variant::tests::shredded_residuals_pass_the_read_gate)'`.

## SPEC_REVISION_REQUIRED

**SPEC-1 (BEH-6, INV-6): `parquet-variant*` dependency in `vala-bifrost-redux`**
- Revision 26 says "no `parquet-variant*` dependency" in this crate, and
  also says "Wyrd keeps only the write side (REQ-020)".
- The writer uses the dependency in production code (`tables/fields.rs:18`,
  `tables/mod.rs:12`, `tables/signal.rs:23`).
- Options:
  - (a) Scope the ban to read-side use only.
  - (b) Move the write-side Variant typing to another owner.
- Recommendation: (a).

STO-1 under REQ-034 is not a spec revision. The verify-first correction in
FIND-003-14 satisfies both clauses without conditional upload.
