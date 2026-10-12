# Iceberg v3 durability domain review

## Subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation inputs: `TASK-001-R1-close-variant-contract-gaps.md` and
  `TASK-001-R2-exact-integers-and-late-errors.md`
- Reviewed boundary: Iceberg v3 table creation and validation, append row-ID
  allocation, hidden-lineage projection and rewrite, Forge publication and
  recovery, manifest rewrite, snapshot expiry, and garbage collection.

`HEAD` remained the candidate throughout this review. The binding human
decision controls the apparent older wording in the specification and task:
lineage relies on standard Iceberg v3 handling. A rewrite-wide duplicate-ID
scan and optional `DataFile` metrics gate are prohibited in production and in
required proof code.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Standard row-lineage contract | Apache Iceberg format specification, “Row Lineage,” “First Row ID Inheritance,” and “Snapshot Row IDs”; spec REQ-001/REQ-002/AC-002; binding human decision | **PASS.** The standard requires v3 snapshot/manifest inheritance for newly assigned IDs and exact copying of non-null `_row_id` and unchanged-row `_last_updated_sequence_number` during rewrites. It does not require a duplicate-ID scan or optional metrics map as a publication gate. |
| Fresh tables and reconciliation | `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1010-1102`; registration callers for built-ins and user tables | **PASS.** The shared physical-table owner creates `FormatVersion::V3` and refuses an existing table of any other format. There is no migration switch, compatibility path, or separate table-kind behavior. |
| Append row-ID allocation | `crates/vala/vala-bifrost-redux/src/forge/scribe_promotion.rs:1012-1057`; pinned `iceberg-rust` snapshot, manifest-list, and row-lineage reader paths | **PASS.** Promotion uses the native Iceberg fast-append transaction. The pinned library owns `next-row-id`, snapshot `first-row-id`, manifest/file inheritance, and retry reassignment. Wyrd adds no allocator, repair table, check, setting, or option. |
| Rewrite projection and unchanged copy | pinned `iceberg-compaction` `datafusion_processor.rs:867-894`, `iceberg_file_task_scan.rs:648-685,759-772`, and `executor/datafusion/mod.rs:260-328,404-438`; pinned `iceberg-rust` row-lineage reader | **PASS.** V3 planning projects the two reserved field IDs outside the logical schema, resolves inherited values through Iceberg's reader, rejects missing/mistyped/null internal batches, and hands the same batch to the Iceberg writer. The implementation contains no row-ID collection/sort or lineage-metrics prerequisite. |
| Non-committing core and Forge publication | `crates/vala/vala-bifrost-redux/src/forge/managed/{executor,handoff}.rs`; `src/forge/publication.rs`; `src/forge/worker.rs`; pinned core writer drain and output ledger | **PASS.** The managed core has no catalog authority and returns the established five-field handoff only after successful execution. A validation or writer failure returns no successful handoff; produced objects remain tracked for ordinary orphan recovery. Forge publication validates base, selection, file identity, partition, and nonempty content without inspecting optional lineage metrics. |
| Repeated rewrites | `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1112-1562`; pinned `compaction::tests::rewrite_preserves_v3_row_lineage` | **PASS.** The journey follows user and built-in rows through two physical rewrites and compares each observed logical value plus both hidden lineage values. The fork test covers inherited append lineage mixed with physically copied rewrite lineage. Neither proof rejects repeated IDs or requires metrics maps. |
| V3 manifest maintenance and GC | `crates/vala/vala-bifrost-redux/src/forge/gc.rs:223-341,344-599`; `src/forge/worker.rs` expiry/cleanup paths; pinned `iceberg-rust` `rewrite_manifests` and snapshot-expiry implementations; `managed_rewrite.rs:1330-1562`; `production_routes.rs` | **PASS.** Forge no longer skips v3 manifest rewrite. Iceberg preserves existing `first_row_id` values while assigning space only where the v3 rules require it. The production journey covers manifest merge, expiry, surviving lineage, and a fresh append after maintenance; existing cleanup roots, leases, durable handoffs, and orphan recovery remain intact. |
| Dependency identity | `Cargo.toml:229-247`, `Cargo.lock`, and both checked-out dependency sources | **PASS.** All Iceberg crates resolve to `e999331f280b698bcd026550812b5047e8789df6`; compaction resolves to `b68d9a9ff6c23bdc2d2e8c9d704b0aefe3c564d9`. No path patch or moving branch is present. |

The resulting mechanism matches the established Iceberg v3 contract and uses
Iceberg's native transaction, metadata inheritance, reader, writer, manifest
rewrite, and snapshot-expiry behavior. I found no Wyrd-specific lineage gate,
option, file, or duplicate standard mechanism that qualifies as `DRIFT` under
the standing direction.

## Verification assessment

Independently run on the pinned compaction checkout:

```text
mise exec -- cargo nextest run --locked \
  --manifest-path /home/thorrester/.cargo/git/checkouts/iceberg-compaction-3b4eb9269abd88f3/b68d9a9/Cargo.toml \
  -p iceberg-compaction-core --lib \
  -E 'test(=executor::datafusion::tests::row_lineage_is_complete) | test(=compaction::tests::rewrite_preserves_v3_row_lineage)'
```

Result: **PASS**, 2 tests run, 151 skipped. The tested checkout resolved exactly
to the candidate pin. `git diff --check
80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..555308ba14058ddc56102d2f925298ef43858175`
also passed.

The candidate's immutable implementation evidence records PASS for the
repository-managed PostgreSQL journey
`forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`, including
v3 creation, two rewrites, manifest rewrite, snapshot expiry, and post-GC append
allocation. I did not rerun that shared-environment journey in this parallel
domain pass. This is a verification limit, not a finding: the source path and
recorded final-candidate result were available, and the independently rerun
pinned-core tests cover the exact lineage projection/copy owner.

## Findings

No material findings.

Prior `FIND-TASK-001-3` is closed in this domain. Candidate remediation removed
the duplicate-ID assertions and optional-metrics assertions from the required
proofs while retaining standard field-ID projection, presence/type/null
validation, unchanged copying, repeated-rewrite comparison, and v3 maintenance
coverage. No replacement mechanism was added.

## Overall result

**PASS**

The candidate satisfies TASK-001's Iceberg v3 persistent-data, Forge rewrite,
publication/recovery, and garbage-collection obligations under the binding
human decisions.
