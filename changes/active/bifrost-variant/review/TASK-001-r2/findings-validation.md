# TASK-001 r2 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior verdict and ledger: `changes/active/bifrost-variant/review/TASK-001-r1/{verdict.md,findings-validation.md}`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`
- Candidate stability: confirmed before and after validation; `HEAD` remained the candidate commit.
- CodeGraph: unavailable because this repository has no `.codegraph/` directory. Validation used the cumulative Git diff, repository search, direct source/caller tracing, and the pinned dependency checkout.

## Validation coverage

This pass read every required discovery report and the focused follow-up in
`TASK-001-r2`, then independently checked the cited source, full owning bodies,
callers, sibling consumers, tests, applicable authorities, original task,
revision-10 specification, prior findings, and remediation. It inspected the
complete cumulative base-to-candidate change inventory and the relevant pinned
`iceberg-compaction` source at
`fb3a594b0a93d9f99b62a77084e94be96fb7fba7`.

The binding human decisions control this ledger:

- Iceberg v3 lineage uses only field-ID projection, per-batch
  presence/type/null validation, and unchanged copy. Duplicate-ID scans and
  lineage-metrics gates are drift even when placed only in required proof code.
- `serde_json/arbitrary_precision` is rejected. The approved input correction
  is `RawValue` plus local lexical `i128` classification in
  `EncodedVariant::from_json_text`.
- A mechanism, check, file, setting, or option absent both established
  standards and comparable widely used projects is `DRIFT` and is never a
  remediation requirement.

## Proposed-finding disposition

| Proposed source ID | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `INV-R2-001` | REVISED | `FIND-TASK-001-3` | Production now follows the approved Iceberg mechanism, but V10 and V13 still make the rejected duplicate-ID assertion a passing condition and V13 additionally requires optional metrics. The prior ID is preserved and narrowed to proof-code drift. |
| `VARIANT-ARROW-R2-001`, `SDK-001` | REVISED | `FIND-TASK-001-11` | The duplicate reports identify one shared decoder defect. Exact JSON text output is possible through the installed Variant writer, but the locked Rust `serde_json::Value` terminal cannot represent an accepted Decimal16 integer beyond `u64` with the rejected feature disabled. A public-result decision is required. |
| `SYS-TASK-001-1` | REVISED | `FIND-TASK-001-12` | The late path is reachable and loses stable identity. The current closed terminal protocol has no Variant code or structured catalog-detail field, so closing it without overloading diagnostic prose requires an approved terminal-contract decision. |

The behavior, standards, maintainer, Iceberg-durability, Oracle/DataFusion,
security/tenancy, and remaining domain claims proposed no additional findings.
Their empty unions were checked against source and remain empty. The follow-up
resolved the lineage disagreement; no reviewer disagreement remains unresolved.

## Final validated ledger

### FIND-TASK-001-3 — Required lineage proofs retain rejected duplicate-ID and metrics checks

- **Source proposal:** `INV-R2-001`, corroborated by `followup-review.md`
- **Disposition:** REVISED
- **Classification:** DRIFT
- **Obligation:** The binding human decision permits only standard Iceberg v3
  field-ID projection, per-batch presence/type/null validation, and unchanged
  copy. It rejects duplicate-ID scans and optional-metrics gates, including
  checks that make required verification fail.
- **Location:**
  `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1158-1262`;
  pinned `iceberg-compaction/core/src/compaction/mod.rs:3289-3350,3401-3411`.
- **Producer-to-consumer trace:** The pinned production reader resolves the two
  reserved field IDs; `validate_row_lineage` checks both columns for presence,
  `Int64` type, and nulls per output batch; the writer receives that batch
  unchanged. Wyrd publication no longer inspects optional lineage metrics.
  V10 nevertheless stores the entire live result in a map keyed by `_row_id`
  and refuses a repeated ID. V13 repeats that assertion and also requires
  `value_counts` and `null_value_counts` entries for both hidden fields. Those
  assertions are reached by the task's required V10/V13 commands and can fail
  an otherwise standards-valid rewrite.
- **Observable consequence:** The required proof can reject a standards-valid
  implementation for an invariant or optional metadata that the approved
  production mechanism neither needs nor promises. Production closure alone
  therefore does not close prior `FIND-TASK-001-3`.
- **Decision-complete correction:** Delete the duplicate-ID assertions from V10
  and V13 and the optional lineage-metrics assertions from V13. Reuse each
  fixture's existing stable logical row identity to compare its two hidden
  lineage values before and after both rewrites. Preserve the production
  field-ID projection, per-batch validator, unchanged write, repeated-rewrite
  coverage, GC coverage, and absent-metrics success case. Add no replacement
  scan, metric requirement, mechanism, setting, option, or repository check.
- **Focused closure proof:** Run the existing exact V10 and V13 commands plus
  the existing pinned per-batch missing/type/null validator test. The
  standards-valid absent-metrics case must pass and every logical fixture row
  must retain both lineage values across repeated rewrites.

### FIND-TASK-001-11 — Accepted Decimal16 integers are narrowed at native and JSON result terminals

- **Source proposals:** `VARIANT-ARROW-R2-001`, `SDK-001`
- **Disposition:** REVISED
- **Classification:** INCORRECT
- **Obligation:** REQ-004 and INV-002 require a written Variant value to read
  with the same type and value and prohibit integer conversion through floating
  point. REQ-018 requires typed and JSON-rendering terminals to expose the
  native Variant value consistently.
- **Location:** `crates/shared/wyrd-queue/src/variant.rs:222-333`;
  `crates/shared/wyrd-client/src/bifrost/facade.rs:893-927`;
  `sdks/wyrd-sdk-python/src/bifrost/mod.rs:307-327`;
  `sdks/wyrd-sdk-ts/native/src/lib.rs:423-445`;
  `crates/wyrd/wyrd-server/src/mcp/bifrost.rs:633-675`; installed
  `parquet-variant-json-59.3.0/src/to_json.rs:236-305`.
- **Producer-to-consumer trace:** The approved `RawValue` correction stores an
  integral token such as `18446744073709551617` exactly as scale-zero
  Decimal16. `variant_bytes_to_json` then calls the installed
  `VariantToJson::to_json_value`; for Decimal16 outside both `i64` and `u64`,
  that method executes `Value::from(integer as f64)`. Rust `sql_as`, Python,
  TypeScript, MCP, CLI, and Arrow-JSON rendering all reach this shared path.
  Raw Arrow results remain exact. Existing journeys stop at values still
  representable by `i64` or `u64`, so they do not exercise the fallback.
- **Observable consequence:** A value accepted and stored exactly returns from
  every non-Arrow terminal as a different numeric type and, for ordinary
  20-to-38-digit cases, a different value.
- **Decision boundary:** The installed direct Variant JSON writer can emit the
  exact digits for JSON text, and Python/TypeScript can represent large integral
  values as `int`/`bigint`. However revision 10 locks Rust `sql_as` to
  `serde_json::Value`, whose ordinary `Number` cannot represent an integer
  beyond `u64`; the binding decision forbids the only workspace feature that
  changes that representation. A string would retype the value, a Wyrd number
  wrapper would add a second model/public type, and rejecting the value would
  contradict the approved Decimal16 input contract. Choosing among those
  public behaviors is not a bounded remediation decision.
- **Required specification decision:** Revise the public native-result contract
  to select one exact representation for accepted Decimal16 values beyond the
  64-bit ranges, and reconcile Rust `sql_as`, Python, TypeScript, MCP/HTTP JSON,
  and the no-second-Variant-model constraint. Preserve the approved local
  `RawValue` plus `i128` input classification and keep
  `serde_json/arbitrary_precision` rejected. After approval, implement the
  selected representation at the shared decoder/writer owner rather than with
  per-consumer guards.
- **Focused closure proof after revision:** Query the same scale-zero value just
  above `u64::MAX`, a value whose `f64` rendering changes digits, and the
  positive and negative supported Decimal16 boundaries. Assert exact
  type/value in every approved native terminal, exact digits in MCP/HTTP JSON,
  and the unchanged Arrow extension in raw terminals.

### FIND-TASK-001-12 — Late Variant query failures collapse to generic execution failure

- **Source proposal:** `SYS-TASK-001-1`
- **Disposition:** REVISED
- **Classification:** INCORRECT
- **Obligation:** REQ-019 requires invalid JSON, numeric overflow, excessive
  depth, and excessive encoded size to fail as query errors with their stable
  catalog identities. Prior `FIND-TASK-001-5` requires interactive and
  distributed Variant failures to retain identical code/details without
  parsing human display text.
- **Location:** `crates/vala/vala-bifrost-redux/src/oracle/query_stream.rs:450-505,1317-1339`;
  `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4127-4295`;
  `crates/wyrd-spec/src/vala/api.rs:817-921`;
  `crates/wyrd/wyrd-tonic/proto/wyrd.v1.proto:23-44`;
  `crates/shared/wyrd-client/src/bifrost/query.rs:185-219`.
- **Producer-to-consumer trace:** `parse_json` raises a typed/tagged
  `VariantQueryError`. Before stream establishment, `map_datafusion_error`
  recovers the exact `BifrostError`. After schema or batch emission,
  `QueryStreamEvent::Batch(Some(Err(error)))` instead calls
  `terminal_error_code`, which has no Variant branch. The closed terminal enum
  has no Variant identities, `failed_terminal_on_path` supplies no detail, and
  the client therefore reconstructs `QueryExecutionFailed`. A scan with a
  valid earlier batch and invalid later batch reaches this path in both local
  and distributed execution; failed-terminal semantics correctly discard the
  partial result but do not preserve its cause.
- **Observable consequence:** Row placement changes the public error from the
  exact corrective Variant code/details to
  `WYRD_VALA_500_QUERY_EXECUTION_FAILED`, even though the same SQL and invalid
  value caused both failures.
- **Decision boundary:** The public late-terminal wire contract is a closed enum
  plus optional scrubbed diagnostic text. It neither names the four Variant
  catalog identities nor carries their structured fields. Overloading the
  diagnostic string as a hidden machine protocol would make the declared code
  false and recreate the kind of string-carrier ambiguity prior remediation
  removed. Adding terminal enum identities or a structured catalog error is a
  public wire-contract change expressly outside the r1 remediation non-goals.
- **Required specification decision:** Extend or revise the late query-terminal
  contract so it can carry the already-approved Variant catalog identity and
  exact details after framing begins, for interactive and distributed streams,
  using the existing catalog/error owners rather than prose parsing or a side
  channel. The revision must state how raw HTTP/gRPC consumers and every SDK
  interpret that terminal.
- **Focused closure proof after revision:** Use an existing real-server query
  journey that emits at least one valid batch before each representative
  Variant failure and assert the same catalog code/details as the pre-stream
  path in interactive and distributed execution. Retain one unrelated late
  execution failure proving it remains generic and prove no partial rows are
  accepted.

## Rejected proposals

None. The reports proposing no findings were independently checked and remain
an explicitly validated empty set; they do not add optional advice to this
ledger.

## Prior-finding closure

| Stable finding | Validation result |
|---|---|
| `FIND-TASK-001-1` | Input-side classification is closed by the binding `RawValue` plus local lexical `i128` implementation. `FIND-TASK-001-11` is a distinct reachable output-contract gap exposed by that correct input fix. |
| `FIND-TASK-001-2` | Closed: built-in Variant identity and bytes are checked before ACK/WAL with typed refusal propagation. |
| `FIND-TASK-001-3` | Not closed: production is compliant, but required V10/V13 proof code retains the prohibited checks described above. |
| `FIND-TASK-001-4` through `FIND-TASK-001-10` | Closed by the documented, structured-error, invariant-surface, TypeScript-documentation, native-Bloom, rustdoc, and import remediations. |

## Validation result

**VALIDATED — 3 retained findings.** `FIND-TASK-001-3` is bounded remediation.
`FIND-TASK-001-11` and `FIND-TASK-001-12` require public contract decisions, so
the validated recommendation is **SPEC_REVISION_REQUIRED**, not an invented
remediation. Required reports, source, callers, authorities, and pinned
dependency evidence were available; no disagreement remains unresolved.
