# Oracle/DataFusion domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Domain: Oracle session registration, DataFusion Variant semantics, distributed plan compatibility, sensitivity ordering, and local/remote stable-error transport

The candidate remained at the stated commit throughout this review.

## Authority and source coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Ownership and execution boundary | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/bifrost-design.md` Oracle and Variant SQL sections; `architecture/references/domain/{vala-architecture,datafusion,olap-serving}.md` | PASS |
| Approved behavior | Spec REQ-017/REQ-019, INV-002/INV-004/INV-006, AC-001/AC-003/AC-005/AC-008; original TASK-001; R1 findings 1 and 5 plus preserved Oracle behavior | PASS |
| One SQL registry owner | `oracle/variant_sql.rs:76-137`; leader planning in `oracle/mod.rs:3096-3108`; admitted and follower state through `resources.rs:4235-4260` and `oracle/follower.rs:198-216`; analytical worker/leader in `oracle/analytical.rs:525-544,7568-7585,7679-7694` | PASS |
| Codec and distributed compatibility | Version-bound follower fingerprint in `oracle/codec.rs:20-38`; version-bound analytical stage digest in `oracle/peer.rs:303-324`; authenticated follower preflight and decode in `oracle/follower.rs:1205-1285` | PASS |
| `->`, `->>`, Struct separation | Planner lowering and Struct fallback in `oracle/variant_sql.rs:140-185`; semantic UDF contract in `:189-285`; plan-shape assertions in `:811-899` | PASS |
| Full-root and residual semantics | Arrow-rs literal lookup, per-row dynamic full-root lookup, canonical unshredded return, and null-parent masking in `oracle/variant_sql.rs:249-407`; TASK-003 boundary retained for shredded physical projection | PASS |
| JSON functions and exact numerics | `parse_json`, `try_parse_json`, and `to_json` in `oracle/variant_sql.rs:483-688`; approved `RawValue` plus lexical `i128` classification in `wyrd-queue/src/variant.rs:142-164,580-695` | PASS |
| Sensitive data before IO | Optimized-plan gate in `oracle/mod.rs:3298-3330,3935-3994`; no-follower-lease journey assertion in `wyrd-testing/tests/bifrost/oracle/published.rs:1236-1269` | PASS |
| Structured local/remote errors | `VariantQueryError` tagged serialization and bounded decoder in `oracle/variant_sql.rs:648-700`; source-chain mapping and generic fallback in `oracle/mod.rs:4248-4300`; exact public reconstruction in `wyrd-client/src/error.rs:183-210` | PASS |
| Production-path proof | `published::variant_sql_registry_covers_every_session` in `wyrd-testing/tests/bifrost/oracle/published.rs:1092-1273`, plus the immutable task's recorded passing journey evidence | PASS |

## Prior-finding closure

- `FIND-TASK-001-1` is closed. `EncodedVariant::from_json_text` validates with the already-supported `serde_json::RawValue`, recursively retains raw number tokens, parses integral tokens as `i128`, narrows to Variant integer or scale-zero Decimal16, and refuses the first unsupported integer. Oracle `parse_json` uses that owner. The rejected workspace-wide `arbitrary_precision` feature was not introduced.
- `FIND-TASK-001-5` is closed. Local execution retains the typed `BifrostError` source. The dependency-owned distributed string carrier receives only the existing tagged serde representation; Oracle accepts only the four Variant catalog variants and leaves malformed, unrelated, and non-Variant catalog strings as `QueryExecutionFailed`. Stable identity no longer depends on human `Display` prose.

## Domain assessment

- Every production planning, admission, follower, analytical-leader, analytical-planning, and worker state reaches the same dependency-owning `OracleVariantSql` installer before its plan boundary. Physical-only derived source resolution copies the task state's scalar registry and does not create a second SQL-planning surface.
- Literal Variant chains become one Arrow-rs-backed `variant_get`; `->>` wraps that result with `variant_as_text`. Non-Variant operands remain with DataFusion, preserving exact Struct `get_field` behavior.
- Dynamic paths evaluate row by row from the full logical root. Every Variant result is unshredded into the canonical extension storage, so chaining, Arrow terminals, and residual values keep their logical meaning.
- `try_parse_json` suppresses only invalid JSON. Numeric range, depth, and size violations remain stable errors, as required.
- Authorization examines the optimized logical scan's projected columns and filters before physical planning, graph leases, provider construction, or row IO; Variant expressions retain the sensitive root dependency.
- The function-set version is carried in the existing plan/stage integrity digests. This is ordinary versioned codec compatibility, not an added option, setting, check subsystem, or alternate protocol.
- The structured error payload reuses the existing catalog serde form and the dependency's existing error carrier. This is the minimum common structured-envelope mechanism; no parallel public error model or prose grammar remains.
- No mechanism, check, file, setting, or option in this domain is unsupported by both the established repository/DataFusion pattern and comparable distributed-query practice. No standing-direction `DRIFT` finding applies.

## Findings

None.

## Verification

Executed against the immutable candidate:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract) | test(=oracle::variant_sql::tests::parse_json_keeps_exact_integers_and_refuses_the_rest) | test(=oracle::tests::variant_errors_keep_their_catalog_identity_locally_and_remotely)'
```

Result: 3 passed, 0 failed.

The remediation evidence also records the Postgres-backed `published::variant_sql_registry_covers_every_session` journey as passing. Its source drives Interactive and Analytical execution through two followers, exact integer JSON, semantic Variant and Struct access, strict/lenient parsing, stable remote error identity, and a sensitive Variant refusal before follower leases advance.

## Verification limits

- I did not rerun the serialized Postgres-backed four-pod journey; I inspected its complete source and relied on the candidate's recorded passing result.
- TASK-003 owns shredded physical-leaf projection, per-file residual fallback, and nested-leaf pruning. This review verified TASK-001's full-root semantic and canonical residual behavior without requiring TASK-003 work.
- I did not rerun the broad repository gate; the focused checks and recorded journey cover this assigned domain.

## Overall result

**PASS**
