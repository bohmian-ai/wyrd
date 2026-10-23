---
id: TASK-003-R5
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 13
parent_task: TASK-003
remediates: [FIND-003-11]
---

# Refuse original invalid query budgets before native numeric conversion

Route directly to `$wyrd-implement`. Finish the existing requested-budget admission correction once at native SDK collection. A forbidden JavaScript budget must be refused rather than transformed into an allowed integer.

## Subject and authority

- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13.
- Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`.
- Immutable reviewed candidate: `39c15176955edca785cc578168c67e1efcb80ea5`.
- Cumulative base: `c62ebce8c4de0d43a2b9d50d360de628c49fd533`, parent of `ff4145ac6`.
- Prior remediation: `review/task-003-7579a2f60-1/TASK-003-R4-bounded-reads-and-card-references.md`, commits `18e62740f` and `791122315`.
- Independent diagnosis, source trace and selected correction: [findings-validation.md](findings-validation.md). Acceptance disposition: [verdict.md](verdict.md).

REQ-030 and original scenario 2 require equivalent bounded reads and existing collection ceilings on SDK/MCP. R4 requires refusal rather than clamping. TS SDK AGENTS requires validation at the native boundary. Follow root/nested rules and preserve earlier finding closures; PR #205 and TASK-027 remain outside this task.

## Diagnosis — FIND-003-11 remains incompletely corrected

The public `WireRequest` and SDK collection options are JavaScript `number` values. `packages/ui/src/lib/node.ts:30–40` forwards them into `sdks/wyrd-sdk-ts/wyrd/src/index.ts:545–547`, which marks its wrapper done and calls native collection unchanged. Native `collect` at `sdks/wyrd-sdk-ts/native/src/lib.rs:1382–1403` accepts both options as `Option<u32>`, then calls the shared `admit_ceilings` rule.

The locked napi 3.12.3 conversion uses `napi_get_value_uint32` before entering that method. It truncates fractions and wraps large Numbers. For example, original `max_rows:4294977296` becomes 10000, `max_bytes:4299161600` becomes 4194304, and fractional 10000.5 rows or 4194304.5 bytes become those same valid integers. A tiny authorized query can then return success. MCP receives original JSON numbers: fractional budgets or overflowing row values fail integer deserialization, while the large byte value fails shared range admission. The public refusal and parity contract is therefore still false.

R4 correctly enforces effective integer caps and releases streams for ordinary invalid u32 values. This residual finding concerns original-value admission; it does not establish allocation above the effective caps or a tenant escape. Current green tests cover 0/maximum+1/default/exact-max values already representable as u32 and cannot detect information lost during foreign conversion. The counterexamples are independently source-traced, not newly executed live reproductions.

## Selected correction and preserved behavior

Preserve the incoming JavaScript Number at the existing native collection boundary until finite, integral and losslessly representable validation has completed. Then narrow and reuse `wyrd_queue::json_result::admit_ceilings` and the existing JSON builder. Ordinary installed N-API Number extraction and standard numeric checks suffice; keep the public JavaScript `number` API. Do not introduce a numeric abstraction or dependency, or duplicate ceiling policy in hosts.

Native collection owns this correction because every direct SDK and Node-executor caller reaches it, and it owns the current information-losing conversion. Wrapper-, component- or BFF-only guards would leave that owner reachable with invalid input. MCP already has the correct integer contract and shared range rule; no MCP or queue/server behavior change is needed.

Refusal must use the existing `WyrdError::Validation` → `NativeQueryStep::failure` → SDK error projection. Preserve native stream release on every refusal using its existing take/drop ownership: the wrapper is already done, so a refused Number must not strand a retained native stream or rely on garbage collection. Keep close notification, cancellation, clean EOF/terminal validation, server settlement and ordinary streaming intact.

Keep omitted defaults (1000 rows/4 MiB), admitted ranges (1..10,000 rows; 1..16 MiB), valid smaller budgets and exact maxima, actual-output `QueryResultTooLarge`, exact JSON accounting, positional rows/nulls/duplicate columns, degraded evidence, and protocol-specific error envelopes. No clamping, query rewriting, transport, setting, retry, lifecycle owner, durable decision or new error family is authorized. FIND-003-12 and prior chart/action/cancellation corrections remain closed. Private signature/control flow and fixture structure remain implementation-owned.

Expected scope is native SDK collection, directly affected generated declarations through their generator, and existing native-backed integration assertions. Regenerate declarations through `ts:build`; never hand-edit them. Do not make unrelated source/test changes or re-own TASK-027.

## Acceptance and focused proof

Extend the existing `TASK-003: Identical authorized reads` tiny-query scenario. Cross the actual rebuilt native boundary; a Rust-only admission test cannot prove JavaScript extraction fidelity.

| Obligation | Required observation |
|---|---|
| FIND-003-11 original row/byte value admission | Wrapping and fractional originals for both fields, including the examples above, are refused independently on Node and MCP with no usable rows. Node retains stable validation metadata; MCP keeps its existing invalid-input envelope. Do not require identical wording between integer-deserialization and range failures. |
| Refusal releases ownership | After a direct native-backed SDK refusal, another collect on the same stream remains closed; a fresh ordinary query succeeds. |
| Adjacent behavior remains | Existing zero/max+1, omitted defaults/exact maxima, smaller-valid-budget output overflow and cancellation assertions still pass. |

Record meaningful RED/GREEN for the added cases, actual selected counts and prerequisites. Use the current real-server setup and test file; no new harness, synthetic load, whole journey suite or broad aggregate is required. Rebuild the native owner before testing. Run the narrow commands sequentially:

```sh
mise run ts:build
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-003: Identical authorized reads$"'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Node query acquisition cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- cargo clippy --locked -p wyrd-sdk-ts --lib -- -D warnings
mise run ts:napi:check
mise run fmt:check
mise exec -- git diff --check
```

No queue/server correction is selected. If source evidence makes changing that shared owner necessary, preserve its exact regression with `mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=query::collect::tests::query_result_is_positional_and_counts_exact_structured_json_bytes)'`. Do not expand into unrelated query or numeric-boundary redesign. A new material contract/security/ownership decision returns to specification authority rather than being chosen here.
