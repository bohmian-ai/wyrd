---
id: TASK-001-R2
kind: remediation
parent_task: TASK-001
spec: SPEC-bifrost-variant
spec_revision: 11
remediates: [FIND-TASK-001-3, FIND-TASK-001-11, FIND-TASK-001-12]
---

## Outcome

TASK-001 satisfies spec revision 11. Evidence and full traces for each finding
are in `findings-validation.md` in this directory.

## FIND-TASK-001-3 — lineage proofs use only standard Iceberg v3 checks

Delete the duplicate-`_row_id` assertions from V10/V13 and the optional-metrics
assertions from V13 (`crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs`
and the pinned `iceberg-compaction` tests named in the finding). Prove lineage by
comparing each row's hidden lineage before and after rewrite, keyed by the
fixtures' existing logical identities. Production stays field-ID projection,
per-batch presence/type/null validation, and unchanged copy. Add no replacement
mechanism. A fork change is pushed fast-forward to the existing
`wyrd/bifrost-variant` branch and repinned; no path patch remains.

## FIND-TASK-001-11 — refuse integers no 64-bit type holds

Revision 11 REQ-004: an integer in the i64 range is an integer; above i64 but
within u64 it is a scale-zero Variant decimal; any other integer is refused with
`WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`. Change the existing classification
in `EncodedVariant::from_json_text` (keep the RawValue lexical path; do not
enable `arbitrary_precision`). Delete the now-unreachable wide-decimal branch.
Prove at the shared encoder and Oracle `parse_json`: `i64::MAX`, `i64::MAX + 1`,
`u64::MAX`, `u64::MAX + 1` (refused), `i64::MIN`, `i64::MIN - 1` (refused). Prove
`u64::MAX` reads back exactly through Rust `sql_as`, Python, TypeScript, and MCP
JSON in the existing built-in Variant journeys (V5–V8).

## FIND-TASK-001-12 — late query failures keep their catalog error

Revision 11 REQ-019: the late query terminal carries the existing catalog error
(code and details) for every failure, in interactive and distributed execution
over HTTP and gRPC, and every SDK raises it unchanged and discards partial
rows. Replace the closed terminal code list plus free text with the existing
serialized catalog error (`wyrd-spec` error owner, `wyrd.v1.proto` terminal,
`oracle/query_stream.rs` terminal mapping, `oracle/mod.rs` distributed path,
`wyrd-client` `bifrost/query.rs` reconstruction). Reuse the pre-stream mapping;
no prose parsing, side channel, or Variant-only branch. A failure with no
catalog identity stays `WYRD_VALA_500_QUERY_EXECUTION_FAILED`. Regenerate
contracts.

Prove with an existing real-server query journey that emits at least one valid
batch before an invalid `parse_json` row: interactive and distributed both
return `WYRD_VALA_400_VARIANT_INVALID_JSON` with the same details as the
pre-stream case, through Rust, Python, and TypeScript; one unrelated late
failure stays generic; no partial rows are returned.

## Verification

Run TASK-001 Verification 1–17 on the final HEAD, the new focused tests and
journeys above, plus `mise run fmt`, `mise run lints`, `mise run py:format`,
`mise run py:lints`, `mise run ts:typecheck`, `mise run codegen:check`, and
`git diff --check`. Append an evidence table to this file.
