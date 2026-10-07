# TASK-001 R7 domain review: Scribe ingest and admission

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation reviewed: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

## Reviewed boundary

This review traced built-in Arrow and canonical OTLP input from the Scribe
transport frame through material planning, bounded decode, the table-owned
validator, preparation, shard dispatch, WAL/fence durability, acknowledgement,
and restart ownership. It specifically covered Variant extension and value
validation, whole-or-absent Struct validation, gateway capture peer input,
OTLP record-level partial success, and whether a refusal can mutate durable
batch state.

| Boundary | Authority and source evidence | Result |
|---|---|---|
| Validation before durable mutation | `architecture/bifrost-design.md:180-182,308-350`; `scribe/ingress.rs:534-624`; `scribe/preprocess.rs:564-630` | PASS — decode, table validation, full native-stream materialization, and prepared-memory charging all finish before `try_send`; WAL and the fence are owned only by the shard after dispatch. |
| One built-in validation seam | `scribe/execution_lanes.rs:533-634`; `tables/mod.rs:218-335` | PASS — every resolved built-in calls its registry-owned `canonical_validator`; pre-declared tables share `validate_predeclared`, while canonical signals reuse `validate_canonical_user_batch` and their table-specific rules. |
| Schema/extension before value walk | `tables/mod.rs:218-294`; `tables/signal.rs:894-948` | PASS — undeclared and incompatible schemas are decided before Variant bytes are visited; all Variant-bearing declarations are checked before the row-major, logical-field-order value walk. |
| Raw Variant error contract | `tables/mod.rs:218-254,361-438`; `wyrd-queue/src/variant.rs` shared `EncodedVariant::from_bytes`/`validate` owner | PASS — raw IPC reaches the shared size, canonical-encoding, numeric-domain, and depth validator and preserves its catalogued error through `ScribeError::ContractViolation`. |
| Raw IPC atomic refusal | `scribe/preprocess.rs:205-299,335-406,573-630`; `scribe/ingress.rs:534-624` | PASS — every planned record batch is decoded, stamped, validated, and retained locally before the append is sent to a shard; a later source failure drops earlier prepared slices without WAL, fence, mailbox, or ACK mutation. |
| Whole-or-absent Structs | `tables/mod.rs:274-335`; `tables/verification/results.rs:71-83`; `tables/gateway/calls.rs:79-87`; `tables/metrics/projection.rs:286-335` | PASS — verification and gateway declarations use `WHOLE_STRUCTS`, and metrics' canonical validator applies the same shared refusal to both bucket Structs. |
| Gateway peer trust boundary | `wyrd-testing/tests/gateway/peer.rs:122-290`; common raw-IPC path above | PASS — a partial `resolved_model` crosses the real mTLS capture peer, returns the exact `SchemaParse`, leaves no queryable row, and does not prevent the next valid capture. |
| OTLP finite-number behavior | `tables/signal.rs:95-145,373-462`; signal projections; `tables/logs/mod.rs:178-204` | PASS — Variant construction validates before a row is appended; record-owned invalid content rejects only that record, while invalid shared resource/scope content correctly rejects each dependent record. |
| Stable batch identity and recovery | `scribe/preprocess.rs:408-535,564-650`; `scribe/shards.rs` durable completion path; `scribe/wal.rs` prepared append and replay owners | PASS — validation changes neither batch identity nor replay state; only a fully prepared closed slice set reaches the shard, and ACK remains downstream of WAL sync and the durable batch fence. |

## Review Findings

### Critical

None.

### Important

None.

### Suggestions

None.

## Open Questions

None.

## Verification Notes

- Reviewed the complete base-to-candidate Scribe/table diff and the R6 implementation evidence, not only the final commits.
- Source tracing confirms that raw IPC, canonical Arrow, the capture peer, and canonical OTLP all converge on the same table-owned validation before dispatch.
- The recorded green evidence includes 847 `vala-bifrost-redux` library tests, the raw-IPC Variant admission journey, the gateway capture peer journey, and the focused OTLP non-finite-number test.
- I did not rerun those environment-backed lanes in this read-only domain pass; their recorded commands and exit-zero results are credible and directly exercise the changed boundary.
- `git diff --check` for the reviewed Scribe, table, and journey surfaces is clean.

## Overall result

**PASS** — no material Scribe ingest, admission, durability, recovery, gateway-peer, whole-Struct, or OTLP partial-success finding remains.

## Identity confirmation

At report completion, `HEAD` remained
`6147cc617d81f2c03464043be698ab2565e9d745` with tree
`7b7bb069ecbe8600e09cc8b6ea4e938709614718`; the reviewed candidate did not
change during this pass.
