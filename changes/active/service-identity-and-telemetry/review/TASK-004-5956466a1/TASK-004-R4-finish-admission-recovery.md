---
id: TASK-004-R4
kind: remediation
status: ready
spec: SPEC-service-identity-and-telemetry
spec_revision: 13
parent_task: TASK-004
remediates: [FIND-TASK-004-1, FIND-TASK-004-14, FIND-TASK-004-16, FIND-TASK-004-17]
requirements: [REQ-035, REQ-037, REQ-038, AC-013, AC-018, AC-023]
depends_on: []
---

# Finish admission recovery at the existing owners

Implement through `$wyrd-implement`. Finish the existing accepted-retry flow, preserve tenant-group rollback and account for retained input buffers. Enforce the selected table's required scalar fields through existing Arrow conformance. Batch three small documentation corrections with this behavior work. No specification revision is needed.

The concrete retry case is an OTLP export accepted under an hourly layout, followed by an authorized additional partition dimension: the identical export must still be acknowledged under its original definition even if the new layout would reject it. A native request deliberately changing its version claim remains a conflict.

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004`.
- Original cumulative base: `3e7c59db7a0f5e9506273eab826875cc8b59f226`.
- Prior candidate / R3 base: `f75ada758827266ac0b25a05d6a7b530f86d845a`.
- Reviewed candidate requiring this remediation: `5956466a1299a99521ed9264b2a711d3f1957e3e`.
- Approved [specification revision 13](../../spec.md), [storage supplement](../../supplements/table-storage-and-correlation.md), [original task](../../tasks/TASK-004-shared-table-storage.md), prior [R3](../TASK-004-f75ada758/TASK-004-R3-complete-version-boundaries.md), current [verdict](verdict.md) and [validated ledger](findings-validation.md).
- Governing owners: `architecture/bifrost-design.md` table versions, declared-schema conformance, resource ownership, append/fencing and recovery; root/nested `AGENTS.md`, applicable `agent_docs`, `TESTING.md` and spec-driven execution rules. Architecture and approved specification override private task mechanics.

Findings 2–13 and 15 are closed for their recorded causes. In particular, keep the stale SQL fingerprint removed, exact promotion/output versions, persisted target defaults, SDK integer projections, Python binding fix and immutable native no-op correction. Do not reopen or redesign those owners without evidence of a material impact from this write set. Original R1 copied-recipe mechanics remain superseded by revision 13.

## FIND-TASK-004-1: finish the retained-source and provisional-group workflow

### Diagnosis

Warm no-claim retries now use the mirror's original version, and cold Bloom-only retries succeed. Three connected failures remain in the introduced recovery flow:

1. **Current preparation can prevent original-version recovery.** `scribe/ingress.rs:562–593` reaches recovery only after a successful `prepare_and_dispatch_under`; admission/decode/preparation failures propagate at `:625–671`. Gate sends canonical input without a version (`gate/mod.rs:951–974`). Accept 33 spans in one hour with distinct `scope_name` values under the original hourly-only layout, then commit a supported additional identity(`scope_name`) dimension and restart. Current planning sees 33 tuples and refuses at `scribe/seal_key.rs:358–385` before consulting the original fence. The accepted export therefore receives a false refusal. The green Bloom-only retry journey cannot expose this because Blooms do not change partition planning.
2. **Fresh peers commit before an unresolved retry is proved.** `vala-sql/src/queries/scribe_batch_commits.rs:519–534` returns `AcceptedUnder` for any different positive version, including different content. `scribe/shards.rs:3834–3850` commits the tenant transaction, then the group inserts/acknowledges fresh peers before ingress performs retained equality or refuses an explicit changed claim. Supported example: native A accepted at version 1; Bloom edit to 2; cold retry using A's ID, explicit version 2 and changed payload, grouped with fresh B. A is rightly refused but B is already durable and visible, contrary to the specified contradiction rollback. This uses the native caller's own ID; no collision in canonical content-derived IDs is assumed. Batched uncertain-commit resolution uses the same classifier and must preserve the same group decision.
3. **Retained input outlives its memory charge.** `unclaimed_retry` (`ingress.rs:936–957`) shares canonical buffers with `owner: None` through the durable wait. For a normal two-hour OTLP export, partition materialization allocates distinct slice arrays (`seal_key.rs:250–278`). Gate has already transferred its sole decode owner; `prepared_slice_bytes` (`preprocess.rs:829–843`) counts only prepared slices/WAL and `charge_prepared` (`ingress.rs:778–790`) resizes that lease. The retry shadow still retains the original source, whose bytes are no longer covered. This extends buffer lifetime; it does not add a full-payload copy. Under delayed shard/WAL/SQL completion, ordinary admitted multipart exports exceed their recorded memory charge.

These are meaningful acknowledgement, group-acceptance and governor-ownership failures. Existing rows are not alleged lost, and no measured throughput regression is claimed. The first failure is incomplete R3 work; the latter two were introduced by its recovery implementation.

### Selected correction

Complete the existing ingress, bounded CPU, shard-completion and tenant-fence owners as one workflow. Keep the existing warm mirror selection. For a qualifying current-contract preparation refusal on a no-claim request, reuse exceptional accepted-ID acquisition to obtain original version/facts and compare the retained input under that definition on the existing bounded CPU owner. An unknown ID keeps the original refusal. Authentication, permission, request-size and resource refusals must not become recovery bypasses.

At a cold fence version mismatch, the existing shard group owner must retain the unresolved group and its completion ownership, roll back that tenant's still-uncommitted fresh fences, and complete strict retained-definition comparison outside the SQL transaction before resuming fencing. Preserve group order. A genuine contradiction or an explicit changed native claim commits no fresh peers from that tenant group. Reuse the grouped INSERT/conflict acquisition and batched uncertain-result reconciliation; CPU comparison inside an open SQL transaction and validation after peer ACK are not acceptable substitutes. Keep WAL durability, canonical attempt coordinates, repeated-identity guard, FIFO scheduling, RLS and poisoning semantics.

Keep the retained recovery source with charged ownership for its actual lifetime. Use the existing `ScribeMemoryLease` checked resize/split/category-transfer and canonical decode-owner mechanisms to preserve source ownership separately when preparation allocates new slice buffers. Reserve distinct source/prepared allocations before shrinking or transferring either charge. Shared allocations need neither copying nor duplicate charging. Release charges with their real source/prepared owners on success, refusal, retry and cancellation, preserving shard-owned admitted-work completion. Do not add another governor, cache, ledger or retention service.

Deleting the source prevents required cold equality; a preliminary lookup on every fresh request violates the throughput constraint. Guarding a downstream ACK cannot undo committed peers. These existing owners already supply the required source accounting, group transaction and exceptional lookup mechanisms; no new product or concurrency contract is required.

### Direct proof

- Extend the real canonical OTLP retry fixture with the one-hour/33-scope grouping edit. Prove warm and cold exact retries, including after publication and WAL retirement, return original acceptance with exact original rows/time/origin and no duplicates. For a new canonical export whose changed content makes current partition planning invalid, preserve that current-contract refusal when no original accepted ID exists. Do not require changed canonical content to keep the old deterministic ID. Exercise changed payload under a deliberately reused ID and strict version-claim conflicts through the native fixture below. Use the supported catalog edit seam; no public PATCH is required.
- Extend grouped fence/rollback proof with a different version **and** changed digest, plus fresh peers. Drive the actual shard/group consumer as well as SQL classification: no fresh fence, visible rows or ACK may precede the contradiction decision. Preserve explicit native changed-claim refusal, successful same-version duplicates, valid no-claim recovery, per-batch order and uncertain-result behavior.
- Hold completion for a normal canonical export spanning two hourly partitions in the existing test seam. Assert distinct retained source and prepared allocations remain charged while waiting, and charges transfer/release exactly once through success, refusal, cancellation and recovery. A shared-buffer clone alone is not evidence of copying.
- Retain exact fresh-operation counts: one multi-row INSERT per normal tenant group, no preliminary accepted-ID read, unchanged normal transaction count, batched conflict/reconciliation only when needed. These counts are correctness evidence, not benchmark results.

## FIND-TASK-004-16: enforce required scalar values using selected-schema conformance

### Diagnosis

The selected native definition now correctly replaces the stale creation SQL fingerprint. However `ConformPlan::build` (`scribe/execution_lanes.rs:694–734`) uses `tables::same_iceberg_type`, which intentionally ignores top-level requiredness, and retains the sent field. `apply` (`:752–775`) validates against that sent field; field-ID stamping also preserves its nullable flag. Variant validation and time/partition checks do not validate an unrelated scalar's registered requiredness.

A native caller can describe a table with required, nonpartition `value: Int64`, then send valid IPC whose `value` field is nullable and contains an actual null. The stream satisfies its own schema and reaches durable acceptance despite violating the table's schema. The existing `native_decoder_refuses_nonnullable_nulls` test checks an IPC stream lying about its own requiredness, so it does not cover this case.

This is a missed original validation obligation at the materially changed conformance authority, not a regression caused by removing the fingerprint. Narrow review expansion is justified by the explicit requirement to preserve nullability validation. The stale-fingerprint cause of FIND-8 remains closed. A nullable source flag with non-null values is not itself a material failure.

### Selected correction and proof

At the existing `ConformPlan` construction/application boundary, carry the selected registered top-level requiredness into the conformed field. Let existing `RecordBatch::try_new` validation reject actual nulls. Preserve source-schema identity for retry conflict evidence, existing casts, field metadata, nested type semantics and field-ID stamping. Leave `same_iceberg_type` unchanged: its sibling catalog callers intentionally compare types independently from requiredness. No new validator, row scan, hash pass, SQL call or dependency is needed.

Extend existing conformance proof for three cases: required registered scalar with nullable source containing null refuses; the same nullable source with non-null values succeeds and conforms to requiredness; an optional registered scalar containing null succeeds. Extend the existing real native mixed-schema journey to require typed pre-ACK refusal and no durable/visible residue, while valid current and retained-version writes still succeed. Do not reject previously supported non-null values merely because the sender's Arrow field is marked nullable.

## FIND-TASK-004-14 and FIND-TASK-004-17: batch the exact documentation remainder

- **FIND-14:** the parser's Errors contract and both tests' useful intent text are complete. The two tests at `wyrd-spec/src/vala/ids.rs:575–612` still lack `# Panics` for their actual failed round-trip/domain and invalid-input assertions/expectations. Root rules and `agent_docs/rust.md:235–240` explicitly include tests. Add one short section per named test describing those existing failed invariants; preserve all code/assertions. No production parser panic, new test or unrelated rustdoc sweep is required.
- **FIND-17:** inserted `Dispatched` at `scribe/ingress.rs:921–925` captured the existing conflict-helper explanation, leaving `table_version_conflict` at `:961` undocumented. Restore its existing description directly above the helper and retain the enum's outcome description. This helper constructs an error value; it does not itself need an Errors or Panics section.

These corrections close explicit repository gates and are batched here with behavior. Source inspection plus ordinary formatting/lints suffices; do not create a style-only round or a check for comments.

## Constraints and preserved behavior

No new SQL call or transaction on successful fresh ingest; no extra full-payload copy/hash pass, per-row version metadata, global lock or normal per-batch describe. Keep the one TableVersion/native-definition authority and all existing permission/tenant/footer/checksum boundaries. Preserve native original-version retries and explicit changed-claim conflicts, request-wide 32/33 fanout, replay suppression by immutable canonical fence coordinates, genuinely unfenced recovery, pins, staged restoration, promotion, exact physical grouping and uncertain-publication reconciliation.

No new public API, async setter, PATCH, cache broadcast, history service, copied recipe, retry ledger, migration/backfill, retention TTL, writer policy/default tuning or dependency change is required. Preserve unrelated dirty task files. Do not commit or push unless asked.

Do not run benchmarks now. Throughput remains unmeasured under revision 13; the existing matched-comparison regression rule remains unchanged. Correct accounting can expose legitimate pressure that previously went uncharged; do not conceal that by increasing budgets or weakening admission.

## Acceptance mapping and focused commands

| Finding | Required observed result |
| --- | --- |
| FIND-1 | Canonical original-version recovery survives current-preparation refusal; contradictory changed-version groups roll back fresh peers before visibility/ACK; retained-source and prepared allocations remain covered through completion/cancellation; normal SQL counts unchanged. |
| FIND-14 | Both exact version proof tests document their concrete failed-invariant panics; parser Errors and assertions preserved. |
| FIND-16 | Required scalar actual null refuses before ACK with no residue; nullable flags with valid non-null data and genuinely optional nulls preserve supported behavior. |
| FIND-17 | Conflict helper has its own attached rustdoc and inserted enum keeps its outcome documentation. |

Use Red–Green–Refactor on the bounded scenarios above. Preserve trace-backed diagnoses for changed tests; do not increase timeouts, retry a red gate into silence, weaken assertions or add suppressions. Reuse existing fixtures and add a narrowly named sibling only where necessary. Record each new test's actual nonempty exact selector before claiming closure. The commands below are required focused owners to extend/run, not proof that current tests already exercise the missing cases; they were not run by this review.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::otlp_replay_across_a_definition_edit_is_acknowledged_once) | test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::mixed_runtime_specifications_preserve_rows_and_pins) | test(=write_read::accepted_batch_revision_survives_alteration_and_restart)"'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && cargo nextest run --locked -p vala-sql --test integration -E "test(/pg_scribe_batch_commits::pg_tests::/)"'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::remembered_conform_plan_matches_a_fresh_plan_and_is_reused) | test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type) | test(=scribe::preprocess::tests::native_decoder_refuses_nonnullable_nulls) | test(=scribe::ingress::tests::decode_to_memtable_transfers_one_charge)'

mise exec -- cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::ids::tests::table_version_round_trips_canonical_strings_beyond_two_to_the_53) | test(=vala::ids::tests::table_version_refuses_noncanonical_forms)'

mise run fmt
mise run lints
mise run check:deps
mise run check:tenant-isolation
git diff --check
```

Confirm current module paths/nonempty selection against source before executing; the SQL module selector covers only the existing grouped-fence proof file. Also run the exact new or extended held-completion/accounting, shard-group rollback and replay tests named during implementation, recording counts and commands. Keep Cargo-backed commands sequential. Only run additional language/codegen checks if the actual write set changes those contracts; no SDK/dependency fork work is prescribed here.

Do not require full Redux, all language journeys, `gate`, `verify:bifrost` or other broad aggregates solely for this remediation. Integrated verification runs at change review. Record supplied prior evidence separately from new results and leave unexplained initial R3 timeouts disclosed unless an actual diagnosed failure establishes a correction. The next review is limited to closure of these four IDs, the fix diff and affected paths, with this cumulative candidate and prior acceptance evidence retained.

## Implementation evidence

Candidate: uncommitted working tree on `task/service-identity-and-telemetry/TASK-004` over `5956466a1`. Prior R3 evidence stays in the R3 packet; every result below is new.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-1(a): an exact no-claim retry the current layout refuses is acknowledged under the version that accepted it; an unknown ID keeps the current refusal | `scribe/ingress.rs` `prepare_and_dispatch` / `prepare_and_dispatch_under` (`Dispatched::Refused`, `is_contract_refusal`, `settle_verification`, exceptional `accepted_version`); `vala-sql` `scribe_batch_commits::accepted_version` | `scribe::accepted_retry::pg_tests::a_retry_the_current_layout_refuses_is_acknowledged_under_its_own` (warm and cold-after-publish retries under identity(`scope`) with 33 scopes; changed batch keeps `InvalidFrame`; no new file rows). RED: recovery disabled → `cold retry: InvalidFrame` at `accepted_retry.rs:102`. OTLP: `negative::pg_tests::otlp_replay_across_a_partition_edit_is_acknowledged_once` (exact original rows/time/origin; a changed export is refused `WYRD_VALA_400_OTLP_REQUEST_MALFORMED`) | PASS |
| FIND-1(b): an unproved version mismatch rolls back the tenant's fresh fences and holds the group until ingress proves or refuses it outside the transaction; group order preserved | `scribe/shards.rs` `commit_batch_control_fences` (no COMMIT when any result is `AcceptedUnder`), `append_and_sync_batch_commits` retry loop, `verify_accepted_retry`; `scribe/preprocess.rs` `AcceptedUnder` verdict on `DurableCompletion` | `scribe::shards::tests::an_unproved_version_mismatch_holds_fresh_peers_until_its_verdict` (no fresh fence or peer ACK before the verdict; `false` → peers fail, 0 fences, 0 visible rows; `true` → peers commit in order with `first_commit`). RED: early return removed → `no fresh fence is durable before the verdict` (left 2, right 0). SQL: `pg_scribe_batch_commits::pg_tests::a_changed_batch_under_another_version_reports_it_before_any_commit` | PASS |
| FIND-1(c): retained source allocations not shared by prepared slices stay charged while the completion is held, and release exactly once on success, refusal, cancellation, failed recovery and recovery | `scribe/ingress.rs` `RetainedSource`, `charge_prepared` (checked resize, `split`, `transfer_category(Decode)`); `scribe/memory.rs` `unshared_arrow_bytes`, `unshared_ipc_bytes` | `scribe::accepted_retry::pg_tests::a_held_unclaimed_write_keeps_its_retained_input_charged_once` (two-day batch held behind an open fence key; Decode = baseline + IPC frame while held; baseline after each outcome; cold recovery across a partition edit leaves no charge). RED 1: retained bytes zeroed → left 0, right 776. RED 2: lease leaked in `into_frame` → `success releases the charge`, left 776, right 0. Unit: `scribe::memory::tests::a_retained_source_is_charged_only_beyond_its_shared_slices` | PASS |
| FIND-1: fresh-operation counts unchanged | Exceptional read only after a contract refusal on a no-claim frame; group retry loop only on `AcceptedUnder` | `pg_scribe_batch_commits::pg_tests::a_fresh_group_inserts_without_a_conflict_read` (one INSERT, no read) and `one_statement_records_and_classifies_a_tenant_group`; the new SQL test counts 1 INSERT + 1 SELECT only on mismatch | PASS |
| FIND-14: both version tests document their panics | `wyrd-spec/src/vala/ids.rs` `# Panics` on both tests | `vala::ids::tests::table_version_round_trips_canonical_strings_beyond_two_to_the_53`, `vala::ids::tests::table_version_refuses_noncanonical_forms` | PASS |
| FIND-16: required registered scalar with a null refuses before ACK with no residue; nullable flag with non-null data and optional nulls succeed | `scribe/execution_lanes.rs` `ConformPlan` carries registered requiredness; `same_iceberg_type` unchanged | `scribe::execution_lanes::tests::registered_requiredness_decides_scalar_nulls`; journey `write_read::mixed_runtime_specifications_preserve_rows_and_pins` (typed `WYRD_VALA_400_OTLP_REQUEST_MALFORMED`, no residue) | PASS |
| FIND-17: conflict helper has its own rustdoc; `Dispatched` keeps its outcome doc | `scribe/ingress.rs` `table_version_conflict`, `Dispatched` | Source inspection; `mise run lints` | PASS |

### Diagnosis recorded during implementation

- **Symptom:** the new `a_retained_source_is_charged_only_beyond_its_shared_slices` expected 0 for a zero-copy slice and got 272.
- **Evidence:** `memory.rs` `retained_array_bytes` starts every array at `size_of::<ArrayData>()` before charging buffers.
- **Cause:** a fully aliased source still holds its own two array descriptors (2 × 136 bytes), and they are real retained memory. The test expectation was wrong; the measure was not.
- **Fix site:** the new test only. It now expects `num_columns × size_of::<ArrayData>()`. No other callers are affected.

### Commands

All Cargo commands ran sequentially. The PG commands ran through `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && …'`.

```bash
cargo nextest run --locked -p wyrd-testing --test otlp -P journey --run-ignored=all -E "test(=negative::pg_tests::otlp_replay_across_a_definition_edit_is_acknowledged_once) | test(=negative::pg_tests::mixed_otlp_requests_commit_only_complete_siblings_and_exact_partial_success) | test(=negative::pg_tests::otlp_replay_across_a_partition_edit_is_acknowledged_once)"   # 3 passed
cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E "test(=write_read::mixed_runtime_specifications_preserve_rows_and_pins) | test(=write_read::accepted_batch_revision_survives_alteration_and_restart)"   # 2 passed
cargo nextest run --locked -p vala-sql --test integration -E "test(/pg_scribe_batch_commits::pg_tests::/)"   # 5 passed
cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=scribe::execution_lanes::tests::remembered_conform_plan_matches_a_fresh_plan_and_is_reused) | test(=scribe::execution_lanes::tests::same_iceberg_type_spellings_cast_to_the_registered_type) | test(=scribe::preprocess::tests::native_decoder_refuses_nonnullable_nulls) | test(=scribe::ingress::tests::decode_to_memtable_transfers_one_charge) | test(=scribe::execution_lanes::tests::registered_requiredness_decides_scalar_nulls)'   # 5 passed
cargo nextest run --locked -p wyrd-spec --lib -E 'test(=vala::ids::tests::table_version_round_trips_canonical_strings_beyond_two_to_the_53) | test(=vala::ids::tests::table_version_refuses_noncanonical_forms)'   # 2 passed
cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^scribe::memory::tests::/)'   # 5 passed
cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -E "test(/scribe::accepted_retry::/)"   # 2 passed
cargo nextest run --locked -p vala-bifrost-redux --features test-support --lib -E "test(/^scribe::/)"   # 375 passed (PG)
cargo nextest run --locked -p vala-bifrost-redux --features test-support --test integration -E "test(/^scribe::/)"   # 2 passed (PG)
mise run fmt; mise run lints; mise run check:deps; mise run check:tenant-isolation; git diff --check   # all pass
```

The first `mise run lints` failed with `clippy::cast_possible_wrap` in the new `write_read.rs` `null_value` helper (`row as i64`). The helper now counts in `i64` (`(0_i64..).take(n)`). Lints were rerun and pass, and the two Scribe journey tests were rerun and pass.

### Non-goals and limits

- **Non-goals held:**
  - no new public API, PATCH, async setter, cache, ledger, governor, retention service, migration or dependency;
  - `same_iceberg_type` is unchanged;
  - no benchmark, `gate`, `verify:bifrost` or full journey suite was run;
  - the unrelated dirty TASK-002/003/005/006/007/008 files are untouched;
  - nothing is committed.
- **Throughput is unmeasured** under revision 13. A successful fresh ingest adds no SQL call, transaction, payload copy or hash pass. The exceptional `accepted_version` read and the group retry loop run only after a contract refusal or an `AcceptedUnder` result. Correct retained-source accounting can expose memory pressure that was previously uncharged; budgets were not raised.
- **Cold OTLP restart after a built-in partition edit is not journey-proven.** Built-in table provisioning (`validate_physical_table`) refuses a non-canonical partition spec at startup, and tenant partition configuration of system tables belongs to TASK-005. The OTLP journey proves the warm partition-edit replay. The cold path is proven at the Scribe integration tier on a registered table (`a_retry_the_current_layout_refuses_is_acknowledged_under_its_own`).
- **Failed or busy verification fails closed.** If ingress cannot prove a held retry, every fresh peer in that tenant group fails with Internal `scribe batch commit identity mismatch` instead of committing.
- **The held-completion test waits on `pg_locks WHERE NOT granted`.** That count is cluster-wide. It matches the existing crash test's polling pattern.
- **`unshared_ipc_bytes` is all-or-nothing.** A retained IPC frame is either fully shared or fully charged. A slice alias costs only its array descriptors.
- **Charge-release timing.** The retained source's charge is released just before the recovery attempt takes its own charge, so the two are never held at once.
