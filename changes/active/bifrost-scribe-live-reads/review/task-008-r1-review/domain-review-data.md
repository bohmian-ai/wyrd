# Persistent analytical data and durability review

Result: **PASS**. No material finding in this domain.

## Immutable subject and scope

Candidate `23eafa368bca19208faf8311eb7b5421e3660b38`, R1 parent
`6e7add054e33701ca5ecb52a5c859948b15161a3`, cumulative TASK-007 base
`a7582db587c6170a290760f1741673125612b797`. Reviewed TASK-007, TASK-008,
their remediations and prior verdict hypotheses against approved spec revision
20, particularly REQ-014/015, AC-016/017 and INV-001/002/004/006.
Unrelated TASK-006 benchmark, process harness and lifecycle changes are excluded.
The supplied decisions stand: FIND-007-3 is unchanged, Postgres tenant columns
are outside the schema deletion, and the refusal code is
`WYRD_VALA_500_QUERY_TENANT_INVARIANT`.

This is an independent static discovery report. No source changes, tests,
Postgres wrappers, full mise lanes or commits were made. HEAD remained the
specified candidate when checked. No CodeGraph index exists.

Authority consulted: AGENTS.md, agent rules, specification and original tasks;
Bifrost identity, durability, assembly/publication and recovery authority;
Wyrd design's Bifrost ownership and doctrine's runtime/client boundary;
reference router and the complete Iceberg, analytical operations/reliability,
OLAP-serving and DataFusion references. The Iceberg reference's generic older
row-tenancy wording yields to revision 20 and current Bifrost authority's
explicit per-file contract.

## Coverage and source evidence

Paths below are relative to `crates/vala/vala-bifrost-redux/src/` unless stated.

| Boundary / obligation | Source trace and evidence | Assessment |
|---|---|---|
| Durable identity remains tenant/table/batch-bound without a tenant row | `scribe/execution_lanes.rs` removes the server-added row array and row-field recipe; `contracts.rs` updates the projected source fingerprint consistently. `scribe/shards.rs::prepare_replay_state` still resolves the replay seal key to its binding and recreates immutable ownership under that tenant/table, with the recorded WAL segments. Its cumulative change is only a test request-field deletion. | PASS: no ACK, durable batch fence, WAL ownership or replay identity was moved into the footer. |
| Staged files bind the authenticated seal key to the footer | `scribe/member_stager.rs:160–237` derives the assembly key from the frozen seal key, supplies its tenant and table binding to `encode_batch`, and retains file fsync, preflight, digest and directory fsync. `scribe/parquet_writer.rs:384–407` rejects disagreement among binding, explicit tenant and frozen seal key before materialization. `RollingArtifactWriter::seal_open_artifact` writes `BifrostFooterIdentity` and `inspect_sealed_artifact` verifies its schema, object and tenant before returning publication evidence. | PASS: stamping is at the existing writer owner; removal of the row validation does not remove binding validation. |
| Claim merge and publication preserve tenant and deterministic identity | `scribe/claim_assembly.rs::gather` revalidates member records and assembly keys and records claim membership durably. `ScribeStagingRuntime::assemble` (`staging_runtime.rs:711–723`) obtains the footer tenant from the same context binding that derives the object base. Recovery context resolves the recovered key's tenant and verifies the registered recipe reproduces that key. `ClaimAssembler::encode` preserves merge order and checks the output row sum against promised member rows. | PASS: no independent footer authority, changed claim membership or mixed-tenant merge was introduced. |
| Hot publication precedes staged cleanup | Existing `scribe/claim_publication.rs::publish` builds inserts from the claim and binding, validates uploaded promotion evidence, commits/reconciles the identical full set, then advances published authority and retires members. `await_lease_drain` waits on the exact key/member before deletion. These durable mechanics are unchanged by TASK-007/008. | PASS: query reader replacement does not alter publication ordering, uncertain acceptance handling or persistent settlement. |
| Staged reader lifetime survives publication | `scribe/hot_source.rs::staged_sources` selects only the requested tenant/table/partition and increments held leases under the registry lock. `LiveTailBatches::into_parts` transfers the lease with the unserved paths. `oracle/follower.rs::live_leaf` moves it to `HotParquetExec`; `oracle/exec.rs:2663–2668,2734–2737,2890–2900` retains the lease on the plan and each lazy partition stream. Dropping all owners invokes `StagedSourceLease::drop`. | PASS: opening a run lazily cannot race deletion of that run while its read remains live. Errors during resolver construction drop the local lease normally. |
| Published, hot and staged reads refuse unproven footer tenancy before decode | `parquet/footer.rs::verify_footer_tenant` requires one nonempty matching value through `footer_value`; ambiguity is refused. `oracle/exec.rs::PublishedFooterLoader::load` validates before returning metadata to the published reader. `tenant_proven_reader_metadata` applies the same proof to hot/staged reader metadata; `hot_stream` invokes it before row-group selection, projection and row decoding. Staged metadata keys use signed tenant/table and writer epoch. | PASS: query-local refusal preserves stored authority and requires no compatibility decoder. Each actual reader opening performs the proof; no coordinated once-per-filename cache is required. |
| Forge promotion and rewrite preserve the footer contract | Promotion remains unchanged-object append through `forge/scribe_promotion.rs` and `parquet/promoted_object.rs`, preserving the Scribe-created bytes and existing checksum/physical metrics proof. Both `forge/managed/executor.rs` configuration paths pass `self.binding.tenant` through `ForgeTablePolicy::to_core_config` to `bifrost_rewrite_writer_properties`; the existing writer properties stamp the tenant on every rewrite output. | PASS: no new catalog commit, fence, output generation or recovery mechanism. |
| Schema and field identities remain coherent after deletion | `tables/managed_columns.rs::CANONICAL_ENVELOPE_FIELDS` retains IDs 1000–1006 and explicitly retires 1008 without renumbering survivors. Dynamic and canonical schema builders and affected table definitions remove the row column. No Postgres column or RLS removal is present in the scoped change. | PASS: the only persisted-format change is the approved column removal/footer addition, with existing local data recreation and strict missing-footer refusal. |
| R1 correction has no persistent/runtime regression | `git diff HEAD~1` changes module imports and equivalent bare declaration names in writer properties, Forge policy, claim assembly and reader metadata; it adds the schema-test panic contract and exact-proof evidence. Constructor values, Arrow/Parquet types, producer identity, SQL/publication operations, ownership and runtime control flow are unchanged. | PASS. |

## Proof assessment and limits

The integrated-tree evidence supplies complete package, target, feature/profile,
exact selector and environment recipes. Its preserved `task-008-review/final-named.log`
records one selected passing test for each named recipe. Domain-relevant checks
include complete claim-footer preservation, generation sort/tenant/artifact
identity, missing/foreign footer refusal, staged pruning, immediate lease drop,
publication overlap, Forge unchanged promotion, Forge rewrite/recovery and the
Scribe write/flush/read journey. The selected tests' source supports the claimed
boundary: assembled output footers are examined, staged lease tests inspect
actual registry ownership, and journeys read through the resulting output.

These historical execution results were inspected, not independently rerun by
this reviewer. No claim is made that the full Bifrost lane, benchmark, all
recovery scenarios or cloud storage were executed in this confirmation review.
The orchestrator owns current permitted focused proof. The import-only source
correction needs no additional persistent-state test harness.

FIND-007-4's data-path declaration sites now use the existing owning module import
blocks; FIND-007-6's changed schema test documents its actual ordering and
nullability assertions; FIND-007-5's named writer, lease, Forge and write/read
proofs now have exact current recipes and corresponding recorded results.
Repository-wide closure remains the standards/behavior reviewers' remit.

## Proposed findings

None. The cumulative candidate satisfies the reviewed persistent analytical data
and durability obligations within the stated static/evidence limits.
