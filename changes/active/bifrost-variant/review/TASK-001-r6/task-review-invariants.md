# TASK-001 R6 invariant review

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation evidence: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

The candidate and tree matched the immutable subject before investigation and
immediately before this report was written.

## Navigation and invariant trace

CodeGraph was used first. The relevant cumulative paths were then expanded from
source:

| Value or state | Producer / boundary | Shared owner and siblings | Sink / failure transition |
|---|---|---|---|
| JSON open value | `EncodedVariant::{from_json,from_json_text}` and `VariantColumnBuilder` | `append_raw`, `scan_numbers`, `raw_number_variant`, Oracle `parse_json`, row preparation, OTLP/gateway/audit/verification producers | Canonical Arrow Variant, Scribe admission, WAL/storage, Arrow and JSON/native query terminals |
| Already encoded Arrow Variant | Public/raw IPC writer into a built-in Variant column | `validate_declared_variants` -> `validate_variant_values` -> `EncodedVariant::from_bytes` -> `scan_encoded` / `Variant::try_new` | `decode_rows` runs the table validator before fingerprinting, stamping, WAL dispatch, or ACK; accepted bytes later reach `VariantArray`, `variant_bytes_to_json`, and `VariantToJson` |
| Nullable built-in Struct | Verification, metrics, and gateway producers | `DomainTable::WHOLE_STRUCTS`, `validate_predeclared`, `refuse_partial_structs`; metrics adds the same check after canonical ledger validation | Scribe refuses parent/child validity disagreement before durable work; complete-present and null-absent rows continue unchanged |
| Gateway model identity | Gateway capture producer | Separate `requested_model` and `resolved_model` declarations; fixed IPC required-child validation plus `CallsTable::WHOLE_STRUCTS` for the optional model | Required model children cannot be null; optional model is wholly present or wholly absent before storage |
| Stored/query Variant | Scribe/Parquet/Iceberg and Oracle providers | `mask_placeholders`, `variant_get`, `to_json`, Rust/Python/TypeScript/MCP/HTTP renderers | Arrow terminals retain the extension; JSON/native terminals require every accepted cell to render as a JSON-domain value |

The admission lifecycle is correctly placed: both decoded IPC and native
preprocessing converge on `scribe::execution_lanes::decode_rows`; its built-in
contract check runs before correlation stamping and before the ingress path can
prepare a WAL append. The remaining defect is inside the shared raw-value
producer, not in a downstream consumer.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-003: one canonical Variant extension, schema/wire/fingerprint/storage round trip | `wyrd-queue/src/variant.rs`; shared schema conversions; table declarations; Iceberg fork pins | Original V1, V12 and R5 focused queue/table evidence | PASS |
| REQ-004 / revision 13: accepted values retain type and meaning in every terminal; canonical Arrow uses the JSON exact-number domain | JSON conversion selects exact i64/u64/double representations; raw scan enforces Decimal16 bounds, but omits non-finite `Float` and `Double` | R5 tests cover integer and Decimal16 boundaries, not raw NaN or infinity | **FAIL — INV-R6-001** |
| Locked write order: size, encoding validity, numeric range, then depth | `VariantViolations::finish`; iterative `scan_encoded`; over-depth `scan_numbers` | R5 direct tests and real-server raw/JSON journeys cover malformed/numeric/depth compounds | PASS for covered classes; INV-R6-001 is an omitted numeric class rather than an ordering defect |
| REQ-006–REQ-011 / AC-001 / AC-003: built-in schemas and producers use typed Struct/Variant values | Table declarations and projection owners for signals, verification, gateway, audit, and agent traces | Original V1–V8, V14–V15 plus R5 verification/metrics journeys | PASS |
| Revision-12 nullable Struct contract: present values have every child; absent values have no child | `WHOLE_STRUCTS`, `refuse_partial_structs`, Results/Calls declarations, metric validator | R5 `partial_nullable_structs_are_refused`, `partial_metric_buckets_are_refused`, verification and metrics journeys | PASS |
| Gateway required and optional `ModelRef` shapes remain distinct | `tables/gateway/calls.rs`: required non-null children vs optional nullable children; optional model in `WHOLE_STRUCTS` | R5 gateway schema and unresolved-capture tests | PASS |
| REQ-017 / REQ-019: every Oracle session has the same semantic SQL surface and catalog error transport | `OracleVariantSql`; `QueryCatalogError`; local/distributed codec paths | Original V9 and R5 Oracle tests | PASS |
| REQ-019: invalid write values are refused before ACK and never stored partially | Scribe validates built-ins in `decode_rows` before fingerprint/stamp/WAL; known Variant violations retain catalog identity | R5 journey proves malformed, depth, Decimal16, shared-offset amplification, no-row, and recovery cases | **FAIL for raw non-finite floats — INV-R6-001** |
| INV-002: no silent narrowing or retyping | Exact integer handling and Decimal16 refusal prevent the previously found rounding paths | R5 direct and cross-boundary evidence | **FAIL for admitted raw NaN/infinity, which cannot reach JSON/native terminals as the written numeric value — INV-R6-001** |
| INV-004 / AC-008: sensitive Variant and Struct leaves remain permission-gated before IO | Existing Oracle projection authorization and table sensitivity metadata remain shared | Original V9 and cross-surface query journeys | PASS |
| INV-006: tenant isolation and distributed execution remain unchanged | No R5 remediation moved tenant derivation, physical table identity, or peer tripwires | Original distributed/tenant evidence; source trace found no bypass | PASS |
| INV-007: depth/size/hostile encoded work is bounded | JSON recursion stops building at 64; raw scan is explicit-stack and node-count bounded; shared-offset amplification is refused | R5 20,000-level and shared-object unit/server cases | PASS |
| REQ-001/REQ-002/INV-003: v3 creation, rewrite lineage, GC, and logical invisibility remain intact | Catalog/Forge/Iceberg cumulative diff and pinned fork owners | Original V10, V12, V13 | PASS |
| REQ-005 / AC-009: Bloom sizing uses row-group geometry | Shared writer-properties calculation | Original V11 | PASS |
| Non-goals: no shredding, second Variant model/reader, compatibility layer, new public arbitrary-precision number type, migration, or unrelated dependency | Cumulative diff and R5 remediation preserve existing owners; `c8da11067` workflow drift was removed | R5 path audit, skills sync, docs/codegen/diff checks | PASS |
| Required verification and truthful active packet | TASK-001 is in review at revision 13; R4 historical conflict is marked superseded; R5 records exact focused and broader commands | Recorded R5 command table is green after its diagnosed lint-only test split | PASS with proof limit below |

## Proposed findings

### INV-R6-001 — Raw non-finite floating values cross the admission witness and fail at query terminals

- **Classification:** INCORRECT
- **Relationship to prior findings:** incomplete closure of
  `FIND-TASK-001-24`'s root invariant that a successful
  `EncodedVariant::from_bytes` proves the value belongs to revision 13's exact
  cross-language numeric domain. It is independent of the Decimal16
  representation case but uses the same producer and correction boundary.
- **Violated obligation:** spec revision 13 requires canonical Arrow Variant
  input to use the same exact numeric domain as JSON and requires every
  acknowledged value to retain its numeric meaning through Rust, Python,
  TypeScript, HTTP, MCP, and CLI terminals. `EncodedVariant` itself promises
  that successful construction means the value renders exactly everywhere.
  REQ-019 requires an unstorable numeric value to fail before ACK.
- **Exact locations:**
  `crates/shared/wyrd-queue/src/variant.rs:154-158,210-246,874-958`;
  admission consumers at
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:197-253,377-415` and
  `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:533-577,601-634`;
  failing renderer at `crates/shared/wyrd-queue/src/variant.rs:287-301`.
- **Evidence:** `scan_encoded` records out-of-domain values only for
  `Variant::Decimal16`. It accepts `Variant::Float(f32::NAN/INFINITY)` and
  `Variant::Double(f64::NAN/INFINITY)`, and upstream `Variant::try_new`
  validates those as structurally valid Variant primitives. A raw Arrow writer
  can therefore put either primitive in a correctly declared built-in Variant
  column; `validate_variant_values` calls `EncodedVariant::from_bytes`, receives
  success, and Scribe proceeds past its only pre-ACK value gate. The installed
  `parquet-variant-json` renderer uses
  `serde_json::Number::from_f64` for Float and Double and returns an error for
  every non-finite value; the local `variant_bytes_to_json` maps that late
  failure to `InvalidJson`. Thus the invariant claimed by `EncodedVariant` is
  false even though all callers and storage sinks trust it.
- **Sibling consumers checked:** Arrow query terminals can return the encoded
  primitive unchanged, while Rust typed rows, Python/TypeScript native rows,
  HTTP/MCP/CLI JSON, Oracle `to_json`, and the shared Arrow JSON encoder all
  depend on JSON rendering. This is not a dormant helper or a single-terminal
  presentation issue.
- **Observable consequence:** an authorized raw Arrow write can be ACKed and
  durably stored, then fail native/JSON reads of that row rather than returning
  the acknowledged numeric value. Different terminals therefore disagree on
  whether the stored cell is readable, and the failure occurs after durable
  admission instead of at the trust boundary.
- **Required testable correction:** extend the existing `scan_encoded` numeric
  match in `EncodedVariant::from_bytes` to record non-finite Float and Double
  primitives with the existing `VariantNumericOutOfRange` mechanism before
  `VariantViolations::finish`. Reuse the JSON path's existing floating numeric
  kind (`"double"`) so raw Arrow and JSON expose one public number domain and
  one stable error; add no renderer guard, second validator, new error, or
  normalization path. Add one focused raw-byte unit case for NaN and infinity
  (including one combined with over-depth input to retain numeric-over-depth
  precedence), and extend the existing raw-IPC pre-ACK journey to assert the
  exact numeric-range details, no ACK/no retained row, and a following valid
  write.

## Verification limits

Per coordinator direction, this review did not start Cargo, mise, codegen, or
database jobs in the shared checkout. It relied on the recorded R5 command
table and independently inspected the cumulative candidate, relevant tests,
and installed Variant renderer source. The recorded proof has no raw NaN or
infinity case, so it cannot close INV-R6-001.

## Overall result

**FAIL** — `INV-R6-001` leaves the shared `EncodedVariant` admission invariant
false for a reachable canonical raw-Arrow numeric value.
