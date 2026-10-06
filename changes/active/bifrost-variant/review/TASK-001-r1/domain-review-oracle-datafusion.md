# Oracle/DataFusion domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Domain: Oracle session registration, DataFusion SQL semantics, distributed plan compatibility, sensitivity ordering, and query-error identity

The candidate remained at the stated commit throughout this review.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| SQL contract | Spec REQ-017/REQ-019, INV-004/INV-006, AC-001/003/005/008; `oracle/variant_sql.rs` in full | PASS |
| Registration ownership | Spec “Oracle registration and distributed wire”; `oracle/mod.rs:3095-3106`, `resources.rs:4235-4255`, `oracle/analytical.rs:488-543,7543-7583,7622-7693`, `oracle/admission.rs:1502-1522`, `oracle/follower.rs:1222-1325`, `oracle/analytical_scan.rs:116-165` | PASS |
| Physical-plan compatibility | `oracle/codec.rs:23-38`, `oracle/peer.rs:303-322`, follower preflight at `oracle/follower.rs:1363-1518` | PASS |
| Semantic execution | `variant_get`, `variant_as_text`, `parse_json`, `try_parse_json`, `to_json`, canonical storage, dynamic-path evaluation, and placeholder masking in `oracle/variant_sql.rs:137-603` | PASS |
| Struct separation | Planner type check and fallback at `oracle/variant_sql.rs:151-183`; plan-shape assertions at `oracle/variant_sql.rs:786-797` | PASS |
| Sensitive-column ordering | Existing logical-plan authorization in `oracle/mod.rs`; journey refusal and no-follower-lease assertion at `wyrd-testing/tests/bifrost/oracle/published.rs:1236-1269` | PASS |
| Distributed error identity | `oracle/mod.rs:4250-4349`, public reconstruction in `wyrd-client/src/error.rs:182-210`, analytical invalid-JSON journey at `published.rs:1209-1234` | PASS |
| End-to-end session evidence | `published::variant_sql_registry_covers_every_session` at `published.rs:1083-1274` and task evidence | PASS |

The review also traced the pinned `datafusion-distributed` worker error path. That dependency propagates execution failures as DataFusion error text, so the candidate's bounded reconstruction at the Oracle terminal is attached to an existing transport constraint rather than a second query protocol. It adds no alternate reader, planner, registry, option, setting, or compatibility path. For comparison, gRPC documents both its ordinary status-plus-message model and its richer details model ([gRPC error handling](https://grpc.io/docs/guides/error/)); Google AIP-193 recommends machine-readable detail metadata when the transport supports it ([AIP-193](https://google.aip.dev/193)). Wyrd already uses structured problem details at its public boundary; the internal reconstruction is confined to the dependency-owned string hop and is immediately projected back into the existing `BifrostError` catalog. I found no mechanism that is absent both established repository/dependency behavior and comparable standard practice, so there is no standing-direction DRIFT finding.

## Findings

None.

## Domain assessment

- `OracleVariantSql` is the single dependency-owning registration owner. Leader planning, admitted Interactive execution, follower decode/execution, analytical planning/leader execution, and distributed worker construction all receive the same UDF registry before their relevant plan/decode/execute boundary.
- Literal Variant chains lower to one `variant_get` call; `->>` wraps its result in `variant_as_text`. Struct operands are left to DataFusion and remain `get_field`.
- Literal paths use Arrow-rs `variant_get`; dynamic paths evaluate each row from the full root. Results are unshredded back to the declared Variant extension storage, preserving downstream chaining and Arrow terminals.
- Null Struct parents are masked before every Variant decode, preventing empty child placeholders from entering the upstream decoder while preserving SQL null.
- `parse_json` returns catalogued errors for invalid or unstorable values; `try_parse_json` maps invalid JSON only to null and retains the other fixed Variant limits.
- The Variant SQL version participates in both follower physical-plan fingerprints and analytical stage-body digests. Existing preflight checks compare those values before decode, provider construction, or source IO.
- Sensitive Variant expressions retain their logical root dependency, so existing authorization rejects them before remote graph leases or provider IO.
- Interactive and Analytical invalid-JSON failures retain the same stable catalog identity through Oracle and the public client projection.

## Verification

Executed against the immutable candidate:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract) | test(=oracle::tests::remote_variant_errors_keep_their_catalog_identity)'
```

Result: 2 passed, 0 failed.

The task records a passing Postgres-backed `published::variant_sql_registry_covers_every_session` journey. Its source exercises the Interactive and Analytical paths, follower execution, strict/lenient parsing, semantic Variant and exact Struct access, stable error code, and sensitivity refusal before follower leases advance.

## Verification limits

- I did not rerun the Postgres-backed Oracle journey; I inspected its full source and used the immutable task's recorded passing evidence.
- TASK-003 owns shredded-leaf projection, residual fallback over shredded files, and nested-leaf IO pruning. TASK-001 correctly retains whole-root semantics and does not claim TASK-003's physical behavior.

## Overall result

**PASS**
