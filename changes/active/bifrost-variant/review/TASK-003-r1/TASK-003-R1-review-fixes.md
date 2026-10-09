# TASK-003-R1: review fixes

Implement with `$wyrd-implement`.

## Inputs

- Approved spec: `changes/active/bifrost-variant/spec.md` (working tree,
  intentionally uncommitted) and
  `changes/active/bifrost-variant/spec-revision-26-datafusion-variant.md`.
- Original task (governing plan):
  `changes/active/bifrost-variant/tasks/TASK-003-native-datafusion-read-path-plan.md`.
- Reviewed candidate: Wyrd `861a0342f` (base `b145767dd`). Fork pins:
  DataFusion `a4ff14550`, arrow-rs `0d0eb24`, datafusion-distributed
  `0b81b12`.
- Validated ledger: `findings-validation.md` in this directory. It is part of
  this task: each FIND entry's location, evidence and closure command are
  binding.

## Constraints and non-goals

- Do not reintroduce deleted Wyrd analytical scan, assignment, pruning,
  cache or Variant SQL code.
- Do not add DataFusion or `datafusion-distributed` logic to Wyrd. A gap in
  either belongs in the fork, which follows the fork rules in the plan.
- No `#[allow]`, no weakened assertions, no new dependencies.
- Never commit `spec.md` or `TASK-004-forge-parallelism.md`.
- SPEC-1 (`parquet-variant*` in `vala-bifrost-redux`) is out of scope here
  and waits on a human spec decision.
- Preserve:
  - the native distribution rule;
  - the leader-settle ordering (the fold is taken on success and dropped
    before release otherwise);
  - live route pruning;
  - the footer-proof reader on every scan;
  - all journeys currently green.

## Diagnoses and corrections

### 1. FIND-003-1: fork revision not in the peer contract

- **Obligation:** revision 26 and plan:165 require that a peer on a
  different DataFusion fork revision is refused before any plan is decoded.
- **Current behavior:** `ORACLE_VARIANT_SQL_VERSION` was deleted from
  `stage_body_digest` (`oracle/peer.rs:312`) and `physical_plan_fingerprint`
  (`oracle/codec.rs:27`), and nothing replaced it.
- **Consequence:** in a mixed rolling deploy, builds decode each other's
  plans silently.
- **Correction:**
  - One codec-owned constant holds the pinned DataFusion revision, and one
    private helper hashes it into both digests.
  - Do not bump the protocol versions.
  - Tests prove that a revision change alters the digest, that follower
    preflight refuses a mismatch, and that the constant equals the root
    `Cargo.toml` pin (read with `include_str!`).

### 2. FIND-003-12: no pre-execution tenant-prefix refusal

- **Obligation:** `spec.md:338-340` requires the worker rewrite handler to
  refuse any decoded file outside the authenticated tenant's warehouse
  prefix before execution.
- **Current behavior:** `TenantWarehouseReads::bind` (`oracle/exec.rs:897-990`)
  checks only the scan type and the store URL, and its test expects a
  foreign tenant to bind.
- **Correction:**
  - `bind` checks every `PartitionedFile` location against the tenant
    prefix, through one helper beside `catalog/tenant_table.rs:176-181`.
  - It reuses the footer-proof error.
  - The test expects a refusal; add the non-Parquet and non-local-URL
    refusal cases.

### 3. FIND-003-11: Analytical scan statistics double-count and mix units

- **Current behavior:**
  - `visit` and the distributed fold walk each network boundary's stage
    twice: once through the explicit `Stage::Local` branch, once through
    upstream `children()`.
  - `distinct_files` counts files per task while `files_pruned` counts
    ranges.
  - A head-stage scan's bytes are taken twice.
- **Correction:**
  - Delete the boundary branch in both walks.
  - Count ranges on both sides of the subtraction.
  - For a `DistributedExec` root, take bytes and pruning only from the fold.
  - Add a unit test asserting equal, once-per-range counts for a plain scan
    and a GROUP BY over two files.

### 4. FIND-003-13: planning IO escapes the query deadline

- **Current behavior:** `plan_physical` (`oracle/mod.rs:2859-2902`) reads
  manifests and footers with no deadline.
- **Correction:**
  - Wrap it in `timeout_at(deadline)` with the existing `mod.rs:3702`
    pattern, mapping the timeout to `QueryTimeout`.
  - Add an `arm_stall()` journey with a short deadline.
  - Planning before admission stays, as plan:575 approves it.

### 5. FIND-003-14: recovery re-uploads published keys (REQ-034)

- **Current behavior:** `converge` (`parquet/object_uploader.rs:219`) PUTs
  unconditionally.
- **Correction:**
  - Call `verify_remote` first and return on a match.
  - On not-found or a content contradiction, fall through to the existing
    put-and-verify loop.
  - A PUT-counting store test expects 0, 1 and 1 PUTs.

### 6. FIND-003-2 and FIND-003-3: missing AC-007 proof and weakened REQ-019 ¶2 proof

- **FIND-003-2 (AC-007):**
  - Add List-element and `unnest` decoded-leaf unit tests over the existing
    `events` fixture.
  - Add a bytes comparison to the distributed nested journey.
- **FIND-003-3 (REQ-019 ¶2):**
  - Root cause: the 16384-row Inexact count adds a round-robin repartition,
    so the failing row can arrive first.
  - Make the late fixture an ordered single-partition read.
  - Restore the mandatory late terminal (`delivered == 8192`) in the Rust
    Oracle and SDK, Python, and TypeScript journeys, including the deleted
    Rust SDK case.

### 7. FIND-003-5 and FIND-003-6: red generated artifacts and checks

- Regenerate `wyrd.v1.bin` (`WYRD_PROTO_SNAPSHOT=1`).
- Run `cargo hakari generate && cargo hakari manage-deps`.
- Add `wyrd-types` to `FAMILY_SHARED` in `scripts/test-families.sh`.

### 8. FIND-003-8, FIND-003-9, FIND-003-10, FIND-003-20: deletions

Delete each of these:

- `reconciliation_limit_bytes`, with its boot computation and refusal;
- `BifrostStorageError::from_object_store`, `PermissionDenied` and
  `Backend`, with the test changed to assert the variant production returns;
- `PublishedAssignment` and its `assigned` parameter and branch;
- `ShreddedResiduals` in `wyrd-queue/src/variant.rs`, replaced by
  `parquet_variant_compute::unshred_variant` followed by the existing
  `validate`. A depth bound, if needed first, belongs in the arrow-rs fork.

### 9. FIND-003-4, FIND-003-7, FIND-003-15 to FIND-003-19: documentation and style

- **FIND-003-4:** reword the Group A mechanism at every listed site to
  "one object, one file group, a single partition, so no stage boundary".
- **FIND-003-7:**
  - Fix every stale rustdoc and design-doc site listed.
  - Make `abort_selected_owners` synchronous.
  - Update the plan status table.
- **FIND-003-15:** remove the `REQ-0xx` and `ponytail:` references from code
  and manifests.
- **FIND-003-16:** hoist the imports and use bare type names.
- **FIND-003-17:** keep one `BackendSigner` method per operation, returning
  `impl Future`, with no `#[allow]`.
- **FIND-003-18:** add `# Panics` to the ten helpers and rustdoc to
  `HOUR_MICROS`.
- **FIND-003-19:** add `--profile dist` to `bench:bifrost:reads`.

## Acceptance criteria

| Finding | Accepted when |
|---|---|
| FIND-003-1 | Both codec tests pass, and a follower refuses a digest under a different revision |
| FIND-003-2 | The List-element and `unnest` leaf tests pass; the journey shows the nested-leaf bytes below the whole-column bytes |
| FIND-003-3 | The late terminal is mandatory, with `delivered == 8192`, on all four surfaces |
| FIND-003-4 | `git grep -n "one task per published object" -- crates changes` returns nothing |
| FIND-003-5 | `mise run check:proto-drift && mise run check:workspace-hack` pass |
| FIND-003-6 | `mise run check:test-coverage` passes |
| FIND-003-7 | The metric grep returns nothing; `docs:check` and `lints` pass |
| FIND-003-8, 9, 10 | The greps return nothing and the crates check with `--all-features` |
| FIND-003-11 | `distributed_scan_stats_count_each_range_once` passes |
| FIND-003-12 | The refusal tests pass |
| FIND-003-13 | The stall journey refuses with `QueryTimeout` |
| FIND-003-14 | The `converge_*` PUT-count tests pass |
| FIND-003-15 to 18 | Their greps and `lints` pass |
| FIND-003-19 | `mise tasks info bench:bifrost:reads` shows `--profile dist` |
| FIND-003-20 | `variant::tests::shredded_residuals_pass_the_read_gate` passes with `ShreddedResiduals` deleted |

## Proof

- Run each closure command from `findings-validation.md` with the exact
  `mise exec -- cargo nextest` expression.
- Then run:
  - `mise run test:bifrost`
  - `mise run fmt`
  - `mise run lints`
  - `mise run codegen:check`
  - `mise run check:proto-drift`
  - `mise run check:workspace-hack`
  - `mise run check:test-coverage`
  - `git diff --check`
- `mise run gate` applies (toolchain and root patch set changed). Run it last
  instead of re-running its component lanes.
