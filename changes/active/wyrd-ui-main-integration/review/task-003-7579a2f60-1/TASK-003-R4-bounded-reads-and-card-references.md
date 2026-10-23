---
id: TASK-003-R4
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 13
parent_task: TASK-003
remediates: [FIND-003-11, FIND-003-12]
---

# Restore bounded-read admission and canonical copied Card references

Route directly to `$wyrd-implement`. Fix the two validated foundation gaps without changing the approved behavior. A Node bounded read must refuse requested budgets outside the same existing domain as MCP; Copy ref must produce the canonical flat reference including its kind.

## Subject and authority

- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
- Reviewed candidate: `7579a2f603c023daae0d15b9d5cc3013eec6f00d`; cumulative base `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, parent of `ff4145ac6`.
- Diagnosis and independent validation: [findings-validation.md](findings-validation.md); acceptance disposition: [verdict.md](verdict.md).
- REQ-030, TASK-003 scenario 2 and copied-identity acceptance, INV-001; canonical reference authority `architecture/wyrd-design.md:2159` and `crates/wyrd-spec/src/reference.rs`.

Follow root/TS SDK AGENTS, applicable architecture and `wyrd-ui` in the UI tree. TASK-027 and unrelated PR #205 content remain outside this remediation. Prior FIND-003-1..10 stay closed and must not regress.

## Diagnosis: requested query budgets

**FIND-003-11.** The public WireRequest accepts `max_rows` and `max_bytes`. `packages/ui/src/lib/node.ts` forwards them to SDK `BifrostQueryStream.collect`, which marks its stream done and delegates to native collection. `sdks/wyrd-sdk-ts/native/src/lib.rs:1377–1392` takes the stream and gives those values directly to `JsonResultBuilder`. The builder in `crates/shared/wyrd-queue/src/json_result.rs:69–79` stores arbitrary supplied budgets. It correctly accounts actual output against them but does not admit their range.

MCP's existing `QueryArguments::validate` in server `query/collect.rs:171–182` refuses zero, more than 10,000 rows, or more than 16,777,216 bytes before execution. Thus a tiny valid authorized query with `max_rows:10001` or `max_bytes:16777217` can succeed on Node and fail on MCP. A sufficiently large query can also retain more than the required collected-result cap on Node. Larger general streaming limits cannot fix a local JSON collection option that is never sent to the query service. The supplied green live test checks actual output exceeding smaller valid budgets, rather than inadmissible requested budgets.

Use the existing shared bounded-result owner beside `JsonResultBuilder` and its constants for the common admitted-budget rule; consume it from native collection and existing MCP argument admission. Keep one policy, including the current defaults (1000 rows/4 MiB) and allowed ranges (1..10,000 rows; 1..16 MiB). Refuse invalid input rather than clamping. Native/SDK refusal uses existing `WyrdError::Validation`, `NativeQueryStep::failure` and projected SDK error metadata; preserve MCP's existing `InvalidQueryArguments` details/reason and JSON-RPC invalid-params mapping. Protocol envelopes need not be identical.

The shared pure owner must not acquire streams or own SQL, permissions or MCP types. Native collection still owns release on every outcome. In particular, input refusal must release its acquired/retained stream even though the TypeScript wrapper already marks itself done. Preserve the current take/drop and notification-based cancellation guarantees; a guard that leaves native stream state retained is insufficient. Keep actual output overflow as `QueryResultTooLarge`, and preserve terminal/EOF validation, degraded evidence, positional encoding, cancellation and server settlement. General SDK streaming remains unchanged. No component-, BFF- or host-specific guards are needed.

## Diagnosis: copied Card identity

**FIND-003-12.** `packages/ui/src/lib/format.ts:51–53` accepts only name/version and emits `name@version`. CardHeader retains kind in its exact reference, but Copy ref at `CardHeader.svelte:103` writes the reduced string unchanged. Header metadata and CardRefChip text/title share the formatter. Model/ranker@1.4.2 and Data/ranker@1.4.2 therefore copy the same text, which cannot round-trip through the existing canonical CardRef parser. Structured wire reads remain correct. The current component assertions accept reduced text and do not cover successful canonical copying.

Correct the existing shared formatter to project the generated flat CardRef fields as `Kind/name@version[#uid]`. All existing consumers reuse that producer. Preserve a supplied optional UID, omitting null/absent UID; do not require the header to synthesize a UID or attach one to every reference. Keep Copy uid separate. Version navigation must not carry a previous version's UID into a new exact-version reference. No lookup, parser, alias, compatibility spelling, route, new reference type, schema or catalog change is required.

## Acceptance and focused proof

| Finding | Required observation |
|---|---|
| FIND-003-11 | The existing real Node/MCP read scenario refuses row/byte options zero and maximum+1, with no usable partial rows and each protocol's existing error projection. Exact maximum and omitted defaults still succeed for a tiny query. Valid smaller-budget overflow cases remain. Direct SDK collection refusal releases native stream state; a subsequent native-backed collection is refused, and ordinary later querying remains usable. |
| FIND-003-12 | The existing rendering scenario successfully copies canonical references for two kinds sharing name/version; chip text/title retain the kind. Formatter checks cover supplied and null/absent UID. Copy-failure feedback, Copy uid and exact version actions remain; a changed version never inherits a stale UID. |

Use the current tests and repository-managed prerequisites. Extend the existing scenarios, not a new harness. A tiny query proves requested-budget admission without generating load. Invalid-argument cases must assert each protocol's refusal separately rather than forcing identical envelopes through the current `both()` success/parity helper. Preserve prior cancellation and result-accounting proofs. Keep any Rust admission test inline in its existing owner and record its actual exact selector if added.

Rebuild the changed native owner, then run these narrow commands sequentially:

```sh
mise run ts:build
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^(TASK-003: Validated portable rendering|Node query acquisition cancellation)$'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- cargo clippy --locked -p wyrd-queue -p wyrd-sdk-ts -p wyrd-server --lib -- -D warnings
mise run fmt:check
mise exec -- git diff --check
```

Record meaningful RED/GREEN for the new admission/copy assertions, and the actual selected/passing test counts. Retain rustdoc for changed Rust items. Native build owns generated declaration updates; preserve generated parity if signatures/docs change. The correction does not require changing catalog, dependency, deployment or packaging contracts. Do not add broad journeys or aggregate gates here; combined proof belongs to change review.

Implementation write scope follows existing owners: shared JSON result admission, server argument admission, native SDK collection, shared UI formatter and directly affected tests/declarations. Private helper names, control flow and fixture structure remain implementation-owned. No new public behavior decision, query transport, setting, lifecycle owner, retry, identity model or unrelated cleanup is authorized by this task.

## Implementation evidence

Status: `IMPLEMENTED` at `18e62740f` (base `2b2ed0d9a`).

Reuse map: the existing shared owner `wyrd_queue::json_result` (constants + `JsonResultBuilder`) gains `admit_ceilings`/`RefusedCeiling`, the single range rule. MCP `QueryArguments::validate` now delegates to it (same reasons/details, same JSON-RPC invalid-params mapping). Native `collect` calls it after `take()`, so a refusal returns `NativeQueryStep::failure(WyrdError::Validation)` and drops the taken stream; the slot stays empty, so a later native collect is refused ("query stream is closed"). The existing `fmtCardRef` emits `Kind/name@version[#uid]`; CardHeader and CardRefChip reuse it. The chip's separate kind label was removed because the canonical text now carries the kind.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-003-11: Node and MCP refuse rows/bytes of 0 and max+1 with no rows, each in its own envelope | `crates/shared/wyrd-queue/src/json_result.rs::admit_ceilings`; `wyrd-server/src/query/collect.rs::QueryArguments::validate`; `sdks/wyrd-sdk-ts/native/src/lib.rs::collect` | `TASK-003: Identical authorized reads`: four invalid cases, Node `WYRD_SPEC_400_VALIDATION` with the reason, MCP invalid-params problem with the reason, `toEqual` with no `value`. RED with the native change reverted: `max_rows:0` returned `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE` (1 failed). GREEN: 1 passed | PASS |
| Exact maxima and omitted defaults succeed; smaller-budget overflow stays 413 | same | same scenario: `max_rows:10000,max_bytes:16777216` passes through `both()`; defaults and the existing 2-row/64-byte 413 cases are unchanged | PASS |
| Direct SDK refusal releases the native stream; ordinary querying continues | native `collect` refuses after `take()` | same scenario: `collect({maxRows:10001})` rejects with validation, then a second `collect()` rejects with `query stream is closed`, then a new stream returns `row_count: 3` | PASS |
| MCP admission is unchanged | `validate` delegates | `mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector` (zero/above-ceiling cases) and `query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes`: 2 passed | PASS |
| FIND-003-12: Copy ref copies canonical references for two kinds; chip text/title keep the kind | `packages/ui/src/lib/format.ts::fmtCardRef`; `CardRefChip.svelte` | `TASK-003: Validated portable rendering`: Copy ref writes `Model/ranker@1.4.2` and `Data/ranker@1.4.2`; chip text and title are `Model/…` and `Data/…`; the formatter covers a present, null and absent uid. RED with the formatter reverted: header text `ranker@1.4.2` failed (1 failed). GREEN: passed | PASS |
| Copy failure, Copy uid and version actions remain; a changed version never inherits a stale uid | CardHeader is unchanged (its `ref` is built from kind/name/selected version) | Validated portable rendering keeps Copy uid and Copy failed. In `TASK-003: Dependent loads and safe actions` the host ref carries `uid`; the emitted `card.version` ref equals `{...card, version:'1.4.1'}` and has no `uid` | PASS |

Commands, run in sequence after `mise run ts:build`, `mise run build:server` and `mise run ts:build:testing`:

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^(TASK-003: Validated portable rendering|TASK-003: Dependent loads and safe actions|Node query acquisition cancellation)$'`: 3 passed, 7 skipped
- `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'`: 1 passed, 1 skipped
- `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes) | test(=mcp::bifrost::tests::query_schema_is_closed_bounded_and_has_no_path_selector)'`: 2 passed
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`: 0 errors, 0 warnings, for both the package and the app
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test`: 8 files, 40 passed
- `mise exec -- cargo clippy --locked -p wyrd-queue -p wyrd-sdk-ts -p wyrd-server --lib -- -D warnings`: clean
- `mise run fmt:check`: clean. `mise exec -- git diff --check`: clean

The generated `sdks/wyrd-sdk-ts/wyrd/index.d.ts` and `index.d.cts` were regenerated by `ts:build` for the updated `collect` rustdoc only; the signature is unchanged. The SDK `collect` JSDoc now states the admitted range. Non-goals stayed excluded: there is no new error code, transport, setting, lifecycle owner, parser, route or schema/catalog change, and `WyrdView.svelte` and TASK-027 scenarios were not touched. `ts:build:testing` emitted only the existing macOS linker `__eh_frame` notice.
