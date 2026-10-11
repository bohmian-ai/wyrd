# Focused follow-up: Iceberg v3 lineage proof boundary

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Pinned compaction fork: `/home/thorrester/Documents/GitHub/iceberg-compaction` at `fb3a594b0a93d9f99b62a77084e94be96fb7fba7`
- Conflict investigated: whether V10 and V13 contain prohibited duplicate-ID or optional-metrics checks, despite the production rewrite path using only the approved standard Iceberg v3 handling.

Both repositories matched the identities above during inspection. CodeGraph was unavailable because the Wyrd repository has no `.codegraph/` directory.

## Paths inspected

- `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs`
- `crates/vala/vala-bifrost-redux/tests/integration/forge/support.rs`
- `crates/vala/vala-bifrost-redux/src/forge/publication.rs`
- `crates/vala/vala-bifrost-redux/src/forge/managed/handoff.rs`
- `crates/vala/vala-bifrost-redux/src/forge/worker.rs`
- `/home/thorrester/Documents/GitHub/iceberg-compaction/core/src/executor/datafusion/iceberg_file_task_scan.rs`
- `/home/thorrester/Documents/GitHub/iceberg-compaction/core/src/executor/datafusion/mod.rs`
- `/home/thorrester/Documents/GitHub/iceberg-compaction/core/src/compaction/mod.rs`

## Evidence resolving the conflict

### Production conforms to the approved standard-only mechanism

The pinned fork resolves `_row_id` and `_last_updated_sequence_number` by their reserved field IDs when constructing the file-task projection (`iceberg_file_task_scan.rs:160-214,759-773`). For every v3 output batch, the executor calls `validate_row_lineage` before writing (`executor/datafusion/mod.rs:251-305`). That validator checks only presence, `Int64` type, and null count for the two reserved columns (`:404-438`), after which the exact same `RecordBatch` is passed unchanged to `data_file_writer.write(batch)` (`:303-313`). `executor::datafusion::tests::row_lineage_is_complete` covers exactly the accepted missing/type/null boundary (`:533-564`); it contains no uniqueness scan or metrics assertion.

Wyrd publication likewise has no hidden-lineage metrics prerequisite. `RewriteCommitRequest::derive` validates the handoff's base, selected/live inputs, delete disposition, output content/partition/nonempty measurements, and path identity in `publication.rs:362-458`; neither it nor the managed handoff/worker path searches the two reserved field IDs in `value_counts` or `null_value_counts`. `support.rs` supplies the real promotion, Scribe, Forge, catalog, and object-store fixture used by V10, but defines no additional lineage uniqueness or optional-metrics check.

Accordingly, the durability review is correct only about the production mechanism: the removed rewrite-wide production collection/sort and publication metrics gate have not returned.

### V10 still performs a prohibited duplicate-ID check

The V10 journey's `LineageTable::scan` reads every live row and accumulates the whole result in a `BTreeMap` keyed by `_row_id` (`managed_rewrite.rs:1158-1229`). Each insertion asserts that no prior row used the same ID (`:1220-1226`), and `observe` explicitly relies on “the scan's own uniqueness check” (`:1232-1262`). This is not merely a before/after outcome comparison: it imposes a global uniqueness assertion over the complete live table, so a duplicate makes the required journey fail independently of field-ID projection, per-batch presence/type/null validation, or unchanged copying.

The journey can compare the approved preservation outcome without that extra invariant because every fixture row already has a stable logical key (`LineageTable::key_column` and `RowLineage::key`). Keying observations by that fixture identity and retaining `_row_id` plus `_last_updated_sequence_number` as the compared values proves unchanged lineage across rewrites without scanning for global row-ID uniqueness.

### V13 repeats the duplicate check and adds an optional-metrics gate

The pinned fork's test helper `row_lineage` also scans every live row into a `BTreeMap` keyed by `_row_id` and asserts that each insertion is new (`compaction/mod.rs:3289-3350`). `rewrite_preserves_v3_row_lineage` calls that helper before and after both rewrites (`:3385-3396,3419-3421`). The equality assertion is a valid preservation outcome, but the helper's map key and insertion assertion additionally make global uniqueness a condition of passing V13. The fixture's logical `id`/`name` values already provide the stable comparison identity, so that additional condition is not needed to prove unchanged lineage.

The same V13 test iterates every live data file and requires `null_value_counts[field_id] == 0` and `value_counts[field_id] == record_count` for both hidden fields (`:3401-3411`). Those `DataFile` maps are optional Iceberg statistics. This assertion is not a production publication gate, but it is a verification gate: a standards-valid rewrite with absent metric entries fails the task's required V13 command. The remediation task specifically required the pinned lineage proof to include a standards-valid absent-metric case, so these assertions contradict rather than supply that proof.

### Applicable human direction

The binding decision permits only standard Iceberg v3 field-ID projection, per-batch presence/type/null validation, and unchanged copy, and rejects any duplicate-ID scan or metrics gate. The standing direction expressly includes a nonstandard “check,” not only production code, as DRIFT. Therefore the distinction between a test assertion and a production refusal does not make these two required verification checks acceptable.

## Resolution

The reports do not conflict about production behavior; they conflict about whether prohibited mechanisms inside required tests count. They do under the binding direction.

- The production half of the durability/behavior claim is confirmed: no duplicate-ID scan or lineage-metrics gate remains in Wyrd publication or the fork executor.
- `INV-R2-001` is source-supported for V10 and V13: both contain a whole-table duplicate-ID assertion, and V13 also requires optional lineage metrics.
- The smallest correction is confined to proof code: remove those assertions and compare each fixture row's hidden lineage before/after by its existing stable logical identity. Preserve the production projection, per-batch validator, unchanged write, and all repeated-rewrite/GC coverage. Add no replacement mechanism, setting, option, or repository check.
- No new proposed finding arose from this follow-up; the evidence narrows the existing proposed finding to the required V10/V13 proof code rather than production Forge publication or executor behavior.

## Result

**RESOLVED**
