# TASK-001 r4 Structured Ponytail Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate commit and tree were resolved directly before and after validation.
The checked-out branch is later only in the two mirrored `wyrd-implement` skill
files; all candidate-source claims below use the explicit candidate object. The
untracked r4 review directory is outside the immutable subject.

CodeGraph was used first for the Variant admission, Oracle planning, provider,
and result-consumer paths. Validation then read the complete cumulative diff,
all required r4 reports, all prior findings ledgers/verdicts/remediation tasks,
the applicable repository and Bifrost authorities, the exact candidate source,
and the pinned dependency source needed to resolve the proposals.

## Proposed-finding disposition

| Discovery source | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `BVR-R4-BEH-001`, `STD-R4-001` | **REJECTED BY APPROVED SCOPE EXCEPTION** | — | The change owner explicitly allows the mirrored task-review skill edits in this candidate. |
| `INV-R4-001` | **REVISED** | `FIND-TASK-001-14` | The null-Struct child fabrication is reachable, but the smallest correction is at the verification-result producer, not a new shared read-normalization layer. |
| `INV-R4-002` | **CONFIRMED** | `FIND-TASK-001-15` | Predeclared built-ins traverse Variant values before their complete schema phase, violating locked competing-error precedence. |
| `VARIANT-ARROW-R4-001` | **REVISED** | `FIND-TASK-001-16` | The pre-cap recursion defect is real. The correction must preflight depth through the installed Variant representation's shallow accessors, not parse a second Variant model. |
| `VARIANT-ARROW-R4-002` | **CONFIRMED** | `FIND-TASK-001-17` | Correct extension name plus foreign extension metadata is accepted and normalized away. |
| `VARIANT-ARROW-R4-003` | **REVISED** | `FIND-TASK-001-18` | Numeric-versus-depth precedence is source-proven. The discovery claim about oversize-plus-depth is not retained without a reachable caller whose request-envelope check permits that compound case. |
| unlabeled security proposal / `SEC-R4-001` | **REJECTED** | — | The proposal broadens “before provider IO” into “before all catalog/provider construction and peer discovery.” The candidate authorizes before `TableProvider::scan`, physical source construction, follower execution, and row IO; provider construction is schema-only over an already materialized `Table`. |
| `SDK-PARITY-R4-001` | **CONFIRMED** | `FIND-TASK-001-19` | The changed compiled CLI JSONL surface has no Variant user journey. |
| `REUSE-R4-001` | **CONFIRMED** | `FIND-TASK-001-20` | `EncodedVariant::to_json` is a public one-line test-only wrapper beside the production byte renderer. |
| `MNT-001` | **REVISED** | `FIND-TASK-001-21` | One stale task-evidence artifact causes all three factual errors. Independent r4 fork proof limits runtime uncertainty, but it does not make the candidate's false “final candidate” record acceptable. |

No current proposal is a recurrence of `FIND-TASK-001-1` through
`FIND-TASK-001-12`. The current admission findings are follow-on defects in
ordering, extension identity, and bounded raw-byte traversal after the earlier
missing-boundary finding was closed. They therefore receive new IDs beginning
after 12.

## Root-cause validation

The decisions in `root-cause.md` are validated with one retained correction and
one approved scope exception:

1. The change owner explicitly permits the mirrored workflow-policy edits, so
   `BVR-R4-BEH-001` and `STD-R4-001` require no correction.
2. The three `MNT-001` symptoms share one artifact-maintenance root: the tracked
   original task evidence was not refreshed after the approved revision, final
   compaction repin, and placeholder implementation changed. They are not three
   findings.

The remaining retained findings have distinct producers and correction sites.
In particular, `FIND-TASK-001-14` is produced by verification-result Struct
construction; `FIND-TASK-001-15` by predeclared schema/value sequencing;
`FIND-TASK-001-16` by raw encoded-byte validation order; `FIND-TASK-001-17` by
the extension predicate; and `FIND-TASK-001-18` by JSON conversion failure
selection. Proximity in `variant.rs` or relation to prior
`FIND-TASK-001-2` is not a shared root.

The historical decision grouping `FIND-TASK-001-5` with
`FIND-TASK-001-12` under the general distributed catalog envelope is also
source-valid and closed in the candidate. The other prior-finding closure
decisions in `root-cause.md` match the candidate source and prior remediation
records.

## Final deduplicated ledger

### FIND-TASK-001-14 — Null verification Structs expose fabricated child values

- **Discovery ID:** `INV-R4-001`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** REQ-009, INV-002, and TASK-001 Scenario 1 require
  `drift_report` and `eval_summary` to preserve their nullable persisted meaning
  while Struct access remains DataFusion `get_field`.
- **Exact location:**
  `crates/wyrd/wyrd-server/src/verification/results.rs:700-742`; missing proof
  in `crates/wyrd/wyrd-testing/tests/bifrost/server/verification_runtime.rs:1075-1120`.
- **Evidence:** An absent `drift_report` sets a null parent over valid children
  `""`, encoded JSON null, and `""`; absent `eval_summary` sets a null parent
  over numeric zero children. DataFusion 55 `get_field` returns the selected
  child array without intersecting it with parent validity. Candidate
  `mask_placeholders` only recognizes empty-byte Variant placeholders, so it
  cannot mask these valid JSON-null bytes or primitive placeholders. Existing
  tests filter the null parent or select only a present summary.
- **Observable consequence:** A row with no report can yield empty strings,
  JSON text `null`, or numeric zero from child expressions instead of SQL null,
  fabricating a report that was never present.
- **Decision-complete correction:** Correct the producer, not every query
  consumer. When either parent Struct is absent, emit null child slots beneath
  the null parent using Arrow's native masked-null Struct semantics
  (`StructArray::try_new` permits child nulls covered by the parent null). Use
  the existing `VariantColumnBuilder` null path for `features` and nullable
  primitive arrays for the other children. Preserve the public non-null child
  declarations, present-row values, DataFusion `get_field`, and the existing
  shared Variant placeholder guard for other storage inputs. Add no Struct
  normalization layer or custom SQL operator.
- **Focused closure proof:** Extend the existing verification real-server
  journey to project every `drift_report[...]` and `eval_summary[...]` child
  from absent and present parents before and after publication. Assert SQL null
  for every absent child, including `to_json(drift_report['features'])`, and
  unchanged values for present summaries.

### FIND-TASK-001-15 — Predeclared built-ins return later Variant failures before earlier schema failures

- **Discovery ID:** `INV-R4-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** Spec revision 11 lines 221-226 and TASK-001 Scenario
  1 lock the order undeclared field, unsupported/wire type, Variant bytes,
  validity, numeric range, then depth.
- **Exact location:** `crates/vala/vala-bifrost-redux/src/tables/mod.rs:217-269`;
  `crates/vala/vala-bifrost-redux/src/scribe/execution_lanes.rs:533-568,601-634`.
- **Evidence:** `validate_predeclared` immediately calls
  `validate_declared_variants`. That function checks only declared fields that
  hold Variant and then traverses their values. Complete undeclared, missing,
  order, and non-Variant wire identity remain for the later fingerprint. A
  batch containing an earlier schema defect and malformed/deep Variant bytes
  therefore returns the later Variant error. The canonical-signal sibling
  already performs full schema phases before its Variant walk.
- **Observable consequence:** The same competing invalid input returns a
  different stable catalog error depending on which built-in table receives it,
  and the predeclared path violates the public first-failure contract.
- **Decision-complete correction:** Extend the existing table-owned
  predeclared validator so it establishes the complete declared user schema in
  logical-field order and returns the existing undeclared/unsupported errors
  before calling the shared recursive Variant-value validator. Keep the later
  physical fingerprint as the physical identity fence; do not add a second
  validator type, per-table switch, or client-side guard.
- **Focused closure proof:** In the existing raw-IPC server journey, send one
  predeclared built-in batch combining an undeclared field or non-Variant type
  mismatch with each of malformed, over-depth, and oversized Variant bytes.
  Assert the earlier catalog code, no ACK, and no durable row; retain isolated
  Variant cases to prove their codes still surface when schema is valid.

### FIND-TASK-001-16 — Raw encoded Variant depth is bounded only after recursive dependency validation

- **Discovery ID:** `VARIANT-ARROW-R4-001`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** The fixed depth-64 processing bound in spec revision
  11 and INV-007 must be enforced before untrusted raw Arrow can recurse without
  that bound.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:163-179`, reached
  from `tables/mod.rs:304-328` and Scribe `decode_rows` before ACK/WAL.
- **Evidence:** `EncodedVariant::from_bytes` checks size, then calls
  `Variant::try_new`, and only afterward calls `check_depth`. In
  `parquet-variant` 59.3, `try_new_with_metadata` calls recursive
  `with_full_validation`; list/object validation recursively constructs every
  child. The candidate's `nested_lists` fixture shows one valid nested-list
  level costs only ten bytes, so hostile depth can be enormous below the
  8,388,608-byte ceiling. The path is reachable from raw built-in Arrow IPC.
- **Observable consequence:** A request can exhaust a worker stack or terminate
  the shared process before Wyrd produces `WYRD_VALA_400_VARIANT_TOO_DEEP`.
- **Decision-complete correction:** At `EncodedVariant::from_bytes`, retain the
  size check first, then perform an iterative depth preflight over the installed
  `parquet_variant::Variant` shallow representation/accessors, stopping at
  depth 65. Contain the dependency's documented panic-on-malformed shallow
  access with the standard-library unwind boundary already used in this
  repository and map it to `InvalidJson`. Only after the preflight proves depth
  at most 64 call `Variant::try_new` for the dependency's full validity check.
  This keeps upstream as the encoding/validity authority and introduces neither
  a byte-format parser nor a parallel Variant model.
- **Focused closure proof:** Add one raw-IPC case with a compact, valid,
  extremely deep list that returns the exact depth-65 refusal, then submit a
  valid batch to the same real server to prove process availability. Retain the
  malformed-byte and ordinary depth-65 cases.

### FIND-TASK-001-17 — Foreign Variant extension metadata is accepted and erased

- **Discovery ID:** `VARIANT-ARROW-R4-002`
- **Status:** CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** The Bifrost Variant wire contract requires the exact
  canonical `arrow.parquet.variant` extension, including its empty extension
  metadata, at every nesting level; mismatches are `UnsupportedType`.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:418-432`;
  `crates/shared/wyrd-queue/src/schema.rs:248-277`;
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:217-245`.
- **Evidence:** `is_variant` checks only the extension name. `field_to_spec`
  consequently classifies a correct-name/foreign-metadata field as Variant and
  removes both extension keys; `spec_to_field` reconstructs canonical empty
  metadata before admission comparison. The pinned `VariantType` declares
  empty metadata but its deserializer intentionally ignores the supplied value.
- **Observable consequence:** The trust boundary accepts a non-canonical wire
  type and destroys the evidence needed to diagnose the mismatch.
- **Decision-complete correction:** Make the shared `is_variant` owner require
  both `VariantType::NAME` and `extension_type_metadata() == Some("")`.
  Continue using that one predicate from Arrow-to-wire conversion and recursive
  admission; add no second extension validator.
- **Focused closure proof:** Extend the existing raw Arrow admission journey
  with correct-name/foreign-metadata fields at one top-level built-in Variant
  and one nested Variant. Assert `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE`
  before ACK and no durable row.

### FIND-TASK-001-18 — JSON numeric/depth compound failures depend on traversal order

- **Discovery ID:** `VARIANT-ARROW-R4-003`
- **Status:** REVISED
- **Classification:** INCORRECT
- **Violated obligation:** Spec revision 11 lines 221-226 place numeric range
  before depth for one field's Variant conversion; object key order cannot
  select the public code.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:139-160,580-689`.
- **Evidence:** `append_raw` returns immediately from `enter_container` or
  `raw_number_variant`, and object children are visited in `BTreeMap` order.
  An object containing one out-of-range integer and one depth-65 branch returns
  whichever violation's key sorts first. Existing tests cover each error only
  in isolation. The discovery's size-plus-depth example is not retained: for a
  write the request-envelope check precedes Variant conversion, and no concrete
  reachable caller permitting that compound payload was established.
- **Observable consequence:** Semantically equivalent invalid JSON objects can
  return different stable codes/details solely because their keys sort
  differently.
- **Decision-complete correction:** Keep `EncodedVariant::from_json_text` and
  its existing raw-token walker as the sole conversion owner. Make that walker
  retain a depth violation while continuing the bounded traversal needed to
  find a higher-priority numeric violation, then select numeric before depth
  independent of object order. Do not add another JSON parser, Variant type, or
  configurable limit.
- **Focused closure proof:** Add direct-conversion and Oracle `parse_json`
  compound cases with the numeric and depth branches under keys that sort in
  both orders. Assert identical numeric-range code and details; retain isolated
  depth behavior and one pre-ACK compound write case within the request bound.

### FIND-TASK-001-19 — The compiled CLI's changed Variant JSON surface has no journey proof

- **Discovery ID:** `SDK-PARITY-R4-001`
- **Status:** CONFIRMED
- **Classification:** MISSING
- **Violated obligation:** REQ-018 requires every JSON-rendering row surface to
  render Variant as its JSON value. `architecture/agent-rules.md` requires a
  user journey for every changed user-facing capability.
- **Exact location:** `crates/wyrd/wyrd-cli/src/query/mod.rs:148-170`; coverage
  gap in `crates/wyrd/wyrd-cli/tests/query_server_journey.rs:63-122`.
- **Evidence:** The candidate replaces `LineDelimitedWriter` with a writer using
  `VariantJsonEncoderFactory` and adds the production dependency. The compiled
  CLI journey still selects only primitive `id` and `value` columns. SDK and
  MCP journeys execute different adapters and cannot prove CLI metadata
  propagation, binary wiring, or raw JSONL spelling.
- **Observable consequence:** A CLI-only regression can emit the storage Struct
  or collapse `3.0` to `3` while all current TASK-001 journeys remain green.
- **Decision-complete correction:** Extend the existing compiled-binary
  `query_server_journey` and its current server/credential helpers. Query a
  Variant object containing a double `3.0` and `u64::MAX`; assert native object
  output, exact integer digits, and raw `3.0` spelling. Add no new harness or
  CLI-specific encoder.
- **Focused closure proof:** Run the exact new selector through the existing
  repository-managed `test:cli:journey` environment.

### FIND-TASK-001-20 — `EncodedVariant::to_json` is a dead public wrapper beside the real renderer

- **Discovery ID:** `REUSE-R4-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / DRIFT
- **Violated obligation:** `AGENTS.md` §15 requires reuse of the existing owner
  and no unearned helper; the task-review reuse rule makes a confirmed duplicate
  blocking.
- **Exact location:** `crates/shared/wyrd-queue/src/variant.rs:219-249`, with
  only test callers at lines 791 and 861.
- **Evidence:** `EncodedVariant::to_json` is exactly one delegation to
  `variant_bytes_to_json`. Production Python, TypeScript, signal, and row
  rendering consumers already use the byte renderer because they own separate
  metadata/value buffers. Repository-wide caller tracing found no production
  use of the method.
- **Observable consequence:** The public API presents two owners for identical
  rendering behavior and creates another surface to document and preserve.
- **Decision-complete correction:** Delete `EncodedVariant::to_json` and call
  `variant_bytes_to_json(encoded.metadata(), encoded.value())` from the two
  unit tests. Add no replacement method or trait.
- **Focused closure proof:** Run the existing `wyrd-queue` Variant unit tests;
  no new test is needed for deletion.

### FIND-TASK-001-21 — The tracked final evidence record does not describe the candidate

- **Discovery ID:** `MNT-001`
- **Status:** REVISED
- **Classification:** VIOLATION
- **Violated obligation:** The active change packet must preserve directly
  reviewable task evidence for the immutable candidate; recorded completion
  evidence must identify the authority, implementation owner, and tested
  dependency revision it claims.
- **Exact location:**
  `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md:238,248-256,276-287`.
- **Evidence:** The task front matter binds revision 11 while its authority link
  says revision 10; its “final candidate” table records compaction SHA
  `94db7b94...` while candidate `Cargo.toml:247` and `Cargo.lock` pin
  `2b65fa189f2d05002acc6e59515a071a63777970`; and its diagnosis names a
  nonexistent `wyrd_queue::variant::is_placeholder` instead of the inline
  logic in `mask_placeholders`. The r4 durability reviewer independently ran
  the required fork tests at `2b65fa18...`, so this is not an unresolved
  lineage-runtime finding. It is still a false candidate evidence record, not
  merely an absent optional note.
- **Observable consequence:** A maintainer cannot reproduce the task's claimed
  final V13 proof or navigate the shipped placeholder correction from the
  immutable packet used by final review and completion.
- **Decision-complete correction:** Update this existing task record once:
  revision 11 in the authority link, actual `mask_placeholders` ownership/logic,
  and the tested/pinned compaction SHA `2b65fa18...`. Attribute the final-pin
  fork proof accurately; do not create a second evidence file or imply the
  broader Postgres V10 journey was rerun at the final pin if it was not.
- **Focused closure proof:** Compare the recorded SHAs to `Cargo.toml` and
  `Cargo.lock`, verify the named symbol exists, and run the exact pinned V13
  command (already independently green in r4) plus `git diff --check`.

## Rejected proposal

### `SEC-R4-001` — payload authorization after prohibited provider/peer IO

**REJECTED.** The task requires a sensitive expression to be refused before
provider IO. Candidate `plan_physical` obtains and optimizes the logical plan,
calls `authorize_payload_columns`, and only then calls
`SessionState::create_physical_plan`, which invokes `TableProvider::scan`.
`OracleTableProvider::scan` documents and implements lazy source access; actual
Iceberg, hot-Parquet, live-fragment, follower, and row IO occurs only from the
physical plan after authorization. Its earlier `try_new` calls pinned
`IcebergStaticTableProvider::try_new_from_table`, whose pinned implementation
only converts the already-loaded table schema and performs no object or row IO.

Cut materialization and Scribe route discovery do occur earlier, but neither
the approved spec's payload rule nor TASK-001's explicit “before provider IO”
criterion requires the payload-column decision before all catalog work or
participant discovery. The object-scope permission is separately checked
before metadata/source IO. Requiring a schema-only planning architecture and
zero route-list calls would broaden the accepted task and duplicate planning
machinery without closing an approved behavior gap. Existing source therefore
satisfies the scoped obligation; no remediation is retained.

## Verification assessment

The candidate source and all required reports were available, so validation is
not blocked. Independent r4 checks credibly establish the final Iceberg and
compaction pins, focused Variant/Oracle behavior, generated boundaries, and the
candidate tree identity. The expensive full journey set was not rerun by this
validator. That limit does not erase the source-proven findings above, and the
independent final-pin fork proof does not cure the inaccurate tracked evidence
record in `FIND-TASK-001-21`.

## Recommendation to orchestrator

**FIX_REQUIRED** — retain `FIND-TASK-001-14` through
`FIND-TASK-001-21`. No retained correction requires a new product, public API,
architecture, compatibility, concurrency, resource-ownership, or persistent
data decision. The approved spec and existing repository, standard-library,
Arrow/DataFusion, and installed Variant mechanisms decide all corrections.
