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

## Implementation Evidence — 2026-10-06

All commands below ran on the final candidate. Each used
`CARGO_TARGET_DIR=/home/thorrester/Documents/GitHub/wyrd-bifrost-variant/target`
and exited 0. The commands for V1–V17 are exactly those in TASK-001
Verification.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-3: lineage tests contain no duplicate-`_row_id` or metrics assertions, and they compare each row's hidden lineage keyed by logical identity | `vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs`: `LiveRow {key, row_id, last_updated_sequence_number}`, and every known tuple stays live. The fork `iceberg-compaction` commit `b68d9a9ff6c23bdc2d2e8c9d704b0aefe3c564d9` was pushed fast-forward to `wyrd/bifrost-variant` and repinned in `Cargo.toml`/`Cargo.lock`. No path patch exists. `iceberg-rust` stays at `e999331f280b698bcd026550812b5047e8789df6`. Production paths are unchanged. | V10 `forge::managed_rewrite::v3_row_lineage_survives_repeated_rewrite`; V13 `compaction::tests::rewrite_preserves_v3_row_lineage`; V12 `arrow::schema::tests::variant_round_trips_unshredded`. Tested revisions equal the pins. | PASS |
| FIND-11: an i64 is an integer; a value above i64 but within u64 is a scale-0 Decimal16; anything else is refused with `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` | `wyrd-queue/src/variant.rs`: `EncodedVariant::from_json_text` classifies from the RawValue token. The wide-decimal branch is deleted, `arbitrary_precision` stays off, and `docs/src/content/docs/bifrost/schema.svx` is updated. | `wyrd-queue` `variant::tests::json_text_classifies_integers_from_their_tokens`; `vala-bifrost-redux` `oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest`. Both cover `i64::MAX`, `i64::MAX + 1`, `u64::MAX`, `u64::MAX + 1`, `i64::MIN` and `i64::MIN - 1`. | PASS |
| FIND-11: `u64::MAX` reads back exactly through Rust `sql_as`, Python, TypeScript and MCP JSON | The V5–V8 built-in Variant journeys write and assert `18446744073709551615`. | V5 Rust, V6 Python, V7 TypeScript, V8 MCP `builtin_variant_and_struct_payloads_are_queryable` | PASS |
| FIND-12: the late terminal carries the full catalog problem for every failure, over HTTP and gRPC, in interactive and distributed execution | `wyrd-spec/src/vala/api.rs`: `QueryTerminalFrame.error: Option<Box<WyrdProblem>>`. The closed terminal code enum, free text and detail cap are deleted. `wyrd.v1.proto` reserves 2/6 and the `freshness`/`error` names, and adds `bytes error_problem_json = 9`, with `wyrd.v1.bin` regenerated. `wyrd-tonic/src/query_conversion.rs` round-trips it. `oracle/query_stream.rs` maps the late arm through the same `map_datafusion_error` as the pre-stream path. `oracle/mod.rs` `failed_terminal` covers distributed execution. A failure with no catalog identity stays `WYRD_VALA_500_QUERY_EXECUTION_FAILED`. | `oracle::query_stream::tests::late_catalog_error_keeps_its_identity` (local and peer-forwarded); `oracle::tests::late_failure_terminal_is_closed_and_non_success`; `query_conversion::tests::failed_terminal_problem_round_trips`; `wyrd-spec` `vala::api` tests; `wyrd-server` query, gRPC and MCP lib tests; V16 `codegen:check` | PASS |
| FIND-12: SDKs raise the problem unchanged and discard partial rows, with no prose parsing, side channel or Variant-only branch | `wyrd-client/src/error.rs` `from_problem` reuses `code_to_wyrd_error`. `bifrost/query.rs` projects `FailedTerminal` through it. `wyrd-server` `query::service::terminal_error` (scheduled, MCP) uses the same function. | `bifrost::query::tests::failed_terminal_problem_rebuilds_its_catalog_error`; TS unit `tests/unit/bifrost-query.test.ts` | PASS |
| FIND-12: real-server journeys deliver a valid batch before the invalid `parse_json` row; the details match the pre-stream case; one unrelated late failure stays generic; no partial rows are returned | Rust V5 (`pg_bifrost_e2e.rs`, Interactive, 10000 rows: 8192 valid rows, then `{bad`). V9 (`published.rs` `prove_late_failures`): Interactive with 10000 rows in one object, and Analytical with two objects of 40960 rows, failing only in the last sorted batch. Each case checks the terminal path, `row_count > 0`, and the problem against the pre-stream problem. Python V6 and TypeScript V7: 8192 streamed rows, then `sql()` raises the matching code, status, detail and details. The late CAST failure is generic in every journey. | V5, V6, V7, V9 | PASS |
| Repository lanes | Clippy `large_enum_variant` on `QueryStreamFrame` is fixed by boxing the terminal problem field. The serde, schema and proto wire shapes are unchanged. | `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, `mise run ts:typecheck`, V16 `mise run codegen:check`, V17 `git diff --check`; V1–V4, V11, V14, V15 | PASS |

Stated limit (lead decision): the Python and TypeScript journeys prove late
failures only on Interactive. Both test harnesses start a single Oracle pod, so
neither can run an Analytical (distributed) query. Distributed execution is
entirely server-side and is proven on a real multi-pod cluster by Rust V9.
Python and TypeScript share the same `wyrd-client` problem reconstruction that
Rust V5 and V9 exercise, so the projection is the same.

Diagnosis — V10 lineage row changed identity:
- **Symptom:** after a rewrite, the row with key `2026-10-04T12:00:00.001Z`
  appeared to move from lineage (2,1) to (28,10).
- **Evidence:** Forge deletes the compacted `vala.file_list` rows
  (`crates/vala/vala-sql/src/queries/forge_tasks.rs:1336`). The fixture
  `seal_more` derives its file number from `file_rows().len()`.
- **Cause:** fixture values recur after cleanup, so the logical value alone
  does not identify a row.
- **Fix site:** the test identifies each row by the whole
  (key, `_row_id`, sequence) tuple and asserts that every known tuple stays
  live. Production is unchanged.

Non-goals remained excluded:
- There is no duplicate-ID scan or metrics gate anywhere.
- `arbitrary_precision` is off.
- There is no prose parsing, side channel or Variant-only branch.
- There is no compatibility alias for the removed terminal fields.
- There is no path patch.
