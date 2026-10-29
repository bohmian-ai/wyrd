---
id: TASK-CHANGE-R2
kind: remediation
status: ready
spec: SPEC-benchmark-experiments
spec_revision: 19
requirements: [INV-002]
remediates: [FIND-CHANGE-002, FIND-CHANGE-003, FIND-CHANGE-005, FIND-CHANGE-007]
depends_on: []
---

# Close inherited Scribe delivery gates

Owning approved [benchmark revision 19](../../../benchmark-experiments/spec.md) and [TASK-006-R2 delivery](../../../benchmark-experiments/tasks/TASK-006-R2-revision-19-supplement.md). This is a repository correctness/docs follow-up to the inherited foundation, including the separately delivered Scribe changes; it is not execution of open measurement/optimization work. The cache owner and stale cluster assertion already exist unchanged at the integrated base; do not attribute FIND-CHANGE-002 to the new storage work or to this merge’s executable patch. Storage revision 2 does not own these inherited production changes.

Reviewed combined base: `3899b0a35ec2eede7bf294db0cf66b27defa1498`; immutable target: `462ebee97c81772103054f7d090053ac7054bafa`.
Review/findings/remediation only: the implementor owns all source changes.
Every change below closes a mandatory repository gate. A red test alone is
not evidence of a supported-use production defect. No benchmark measurement,
scaling optimization or excluded benchmark remediation is authorized here.
Route this bounded task directly to `$wyrd-implement`; retain exact durable,
tenancy, permission and assertion contracts. See [change review](change-review.md)
for the complete acceptance matrix and fresh verification evidence.

## Correct the exact restart cache accounting assertion

FIND-CHANGE-002, INCORRECT / mandatory red gate: the integrated `gate` run
fails `bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot`
in `crates/wyrd/wyrd-testing/src/bifrost/cluster.rs:3013`. The assertion
expects governed memory to equal only the metadata-cache budget. Trace shows
the same retained 3-CPU / 3-GiB managed plan before and after restart, no
replayed streams, and zero Scribe bytes. The observed 322122546 bytes are
exactly the sum of two 161061273-byte cache reservations.

Architecture `bifrost-design.md:1032` requires both footer and range caches'
whole budgets to remain charged. `BifrostStorage::with_oracle_caches`
(`storage/mod.rs:163–171`) implements that rule, and the resolved policy
defaults each to managed/20. The test's footer-only expectation/comment is
wrong, not evidence of a resource leak. Preserve the exact clean-node
accounting assertion but compare it with the sum of the same storage owner's
`metadata_cache_bytes()` and `data_cache_bytes()`. Update its comment and
changed-item documentation to describe both retained reservations. Keep every
plan, WAL-root, zero-Scribe, pool-usability and no-active-query assertion. Do
not add tolerance, reset counters, disable a cache, or change production
resource composition.

## Document the new delivered Scribe items

FIND-CHANGE-003 (benchmark-owned subset), VIOLATION. The full integrated range also introduces three test/helper items through
the separately delivered benchmark-rev19 merge. Preserve that provenance and
batch their concrete panic-contract omissions with this same finding:

| New item | Missing required contract |
|---|---|
| `vala-bifrost-redux/src/scribe/admission.rs:309`, `unstaged_backlog_at_its_bound_refuses_as_busy` | `# Panics` for failed reservation and boundary/refusal assertions. |
| `vala-bifrost-redux/src/scribe/replay.rs:1409`, `test_shard` | `# Panics` for its named invariant that the test routing lane fits the WAL header width. |
| `vala-bifrost-redux/src/scribe/wal.rs:4542`, `retired_segment_is_recycled_and_its_stale_tail_ends_replay` | `# Panics` for fixture IO and recycling/replay/fence expectation failures. |

Add concise source-faithful panic contracts only. No benchmark execution or
measurement, new behavior, fixture change, spelling-only item cleanup, or
untouched documentation debt is included.

## Match publication-budget proof to its configured owner

FIND-CHANGE-005, INCORRECT / mandatory red gate: both
`crates/wyrd/wyrd-testing/tests/bifrost/scribe/lifecycle.rs`
`scribe_publishes_due_claims_concurrently_within_the_claim_budget` (:132)
and `concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once`
(:215) fail at their overlap ceiling with six publications against a
hardcoded four. Their shared `CLAIM_BUDGET` at :103 explicitly assumes
the fixed four WAL IO workers size publication.

The delivered benchmark merge changed boot's persistence worker count to
`scribe_config.persistence_cpu_threads`. `PersistenceRuntime::build_staging`
sets the shared assembler's claim budget to that worker count; `start`
passes the identical count to its one shared PersistenceWorker.
`StagingAssembler::check_claim_budget` still bounds globally outstanding
claims; driven-claim ownership still prevents duplicate publication across
flushes. The fault's object-write guard is entered once per `publish_claim`
interval, not once per output artifact/chunk. Config's public default derives
persistence threads from host parallelism, which resolves to six on this host;
the observed six is within the actual bound. Source therefore supports a
stale test expectation, not production over-admission. No independent fixed
four-claim contract exists in the architecture.

Derive these two tests' expected claim budget from the same existing public
`ScribeRuntimeConfig::default().persistence_cpu_threads` that their unmodified
builder uses. Offer twice that many tables so a full-budget publisher must
queue work. Use the same derived budget in hold/contention thresholds and the
upper-bound assertion; update the explanation of the owner. Preserve exact
per-table published row counts and returned values, both flush successes,
single claim ownership and the concurrency observation for a budget that can
run concurrent claims. A configured one-claim budget should require one
overlapping publication, not invent a second slot solely for this assertion.
Do not copy the host's observed six into a constant, increase a production
budget, weaken exact-once checks, change object-write counter scope, add a new
setting/test seam, or run a benchmark.

## Snapshot failed-replay evidence after closing the preceding writer

FIND-CHANGE-007, INCORRECT / mandatory red gate:
`crates/wyrd/wyrd-testing/tests/bifrost/server/owner_inspection.rs:365`,
`failed_wal_replay_is_role_local`, snapshots corrupted WAL bytes while the
old Scribe writer is still alive. `restart_bound` first gracefully shuts that
writer down; the delivered WAL recycling owner legitimately trims its stale
allocated tail in `WalWriter::close_all_streams`. The captured before/after
files have the same path and identical 3458-byte prefix, but length changes
from 53467 to 3458. This demonstrates the wrong observation boundary, not
failed replay rewriting acknowledged bytes.

Before corrupting the prologue or taking `before`, call the existing public
Scribe `shutdown` through `server.bifrost_scribe()` with the existing bounded
shutdown deadline. This closes the actual writer before the operator-evidence
snapshot. `Scribe::shutdown` is idempotent (STOPPED returns immediately), so
`restart_bound` may still own normal whole-server restart. Preserve the full
exact byte-for-byte comparison, corruption, role-local health, published
readability and refusal-without-ACK checks. No prefix-only comparison, new
fixture, WAL production change or weaker recovery boundary.

## Acceptance and proof

Inspect the three new-item panic docs against unchanged bodies. Preserve exact cache accounting, budget derivation and full WAL evidence equality. Run each exact nonzero selector:

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --lib -E 'test(=bifrost::cluster::tests::cluster_restart_rederives_same_plan_from_retained_snapshot)'"
```

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --test scribe -P journey --run-ignored=all -E 'test(=lifecycle::scribe_publishes_due_claims_concurrently_within_the_claim_budget) | test(=lifecycle::concurrent_flushes_share_the_claim_budget_and_publish_each_claim_once)'"
```

```sh
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_testing=debug,wyrd_server=debug,vala_bifrost_redux=debug cargo nextest run --locked -p wyrd-testing --test server -P journey --run-ignored=all -E 'test(=owner_inspection::failed_wal_replay_is_role_local)'"
```

After the exact selectors pass, run `mise run fmt:check`, `mise run lints`
and `git diff --check`. For Python/TypeScript test edits, run their applicable
format/lint tasks too. No repeated broad aggregate, complete language suite or
benchmark is required for these bounded corrections. Record the resulting
immutable candidate and exact diff for closure review.
