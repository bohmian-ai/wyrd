# TASK-001 r7 findings validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The commit and tree matched at the start of validation. This pass changed no
reviewed source and ran no Cargo, database, or language-runtime lane.

## Method and coverage

I read the complete r7 discovery set, follow-up, and root-cause ledger; every
prior r1-r6 verdict, validated ledger, root-cause record, and remediation task;
the approved specification and original task; the applicable repository and
Bifrost authorities; the cumulative base-to-candidate change; current source,
callers, and tests; and the pinned Variant and Iceberg-compaction dependency
source. CodeGraph was used before direct source inspection.

Agreement was not treated as proof. Each proposed failure was retraced through
its producer, shared owner, sibling consumers, and user-visible or required
proof boundary. The Ponytail ladder rejects a second syntax authority, timeout guards,
test harnesses, public validated-borrow types, and a raw-scanner rewrite where
the existing owner, dependency, test, or documentation correction is enough.

## Proposal disposition

| Discovery proposal | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `BVR-R7-BEH-001`, `INV-R7-001`, `ORACLE-R7-001`, `SYS-R7-001`, `VARIANT-R7-002` | **CONFIRMED / CONSOLIDATED** | `FIND-TASK-001-18` | One incomplete closure of the existing JSON-depth root: the shared owner repeatedly parses nested suffixes below the depth boundary. |
| `VARIANT-R7-001` | **CONFIRMED** | `FIND-TASK-001-25` | The existing actual-built-size root remains incomplete because a refused number contributes a null and, in objects, a field name. |
| `VARIANT-R7-003` | **REJECTED as a production defect** | — | `O(fields log fields)` sorting is bounded by the 8 MiB value and is the simplest safe disjoint-range check; revision 13 does not require a linear algorithm. |
| `STD-R7-001` | **REVISED** | `FIND-TASK-001-4` | The authority-synchronization root remains incomplete: production stays, but authority, owner rustdoc, and R6 evidence must stop claiming linear work. |
| `REUSE-R7-001` | **CONFIRMED** | `FIND-TASK-001-27` | `variant_bytes_to_json` performs the same full upstream validation immediately after the shared gate already performed it. |
| `BVR-R7-BEH-002` | **CONFIRMED as a proof gap** | `FIND-TASK-001-24` | The existing raw numeric-domain root lacks user-boundary proof for record-local OTLP non-finite refusal. |
| `SDK-R7-001` | **REJECTED as a specification conflict** | — | Revision 13's narrower canonical-Arrow numeric-domain rule, the approved R6 packet, and active Bifrost authority require non-finite refusal; its real missing proof is retained under finding 24. |
| `PERSIST-R7-001` | **CONFIRMED** | `FIND-TASK-001-28` | Existing-table reconciliation compares storage data types but discards the Variant field identity at every nesting level. |
| `PERSIST-R7-002` | **CONFIRMED as proof-only** | `FIND-TASK-001-3` | The existing lineage root remains incompletely proven because the mandated missing-lineage/no-commit phase is absent. |
| `STD-R7-002` | **CONFIRMED AS CLEANUP** | — | Named new or modified Rust tests in the R6 implementation record lack mandatory exact focused-command evidence. |
| `STD-R7-003` | **CONFIRMED AS CLEANUP** | — | The cumulative immutable range fails `git diff --check` on one extra terminal blank line. |

No maintainer, Scribe durability, security/tenancy, or additional SDK terminal
finding survived validation.

## Validated root-cause decisions

1. **JSON traversal:** finding 18 is one shared root in
   `EncodedVariant::from_json_text`; write preparation and Oracle are sibling
   consumers and must receive no local guard.
2. **JSON actual-built size:** finding 25 is adjacent but independent: it is
   the builder's representation of an already refused member, not the
   traversal's work bound.
3. **Raw validation and authority:** finding 4 is authority drift only, while
   finding 27 is a separate same-path duplicate after successful validation.
   Neither justifies replacing the simple offset sort with interval machinery.
4. **Physical Variant identity:** finding 28 is independent of prior incoming
   schema finding 17; it lives in existing-table reconciliation after Iceberg
   conversion.
5. **Proof:** findings 24 and 3 are two reopened findings in one proof group,
   but use different existing journeys and prove distinct state transitions.
6. **Evidence cleanup:** `STD-R7-002` and `STD-R7-003` are mandatory completion
   gates, not stable implementation findings; they need only exact command
   records and one whitespace deletion.

No correction belongs in Oracle timeout handling, SDK-specific validators,
Scribe durability, RBAC, tenancy, or a new parser.

The r7 `root-cause.md` source decisions are otherwise validated. Reopened or
incompletely closed roots retain their prior stable IDs; only the duplicate
raw validation and physical reconciliation roots receive new IDs 27 and 28.

## Final deduplicated ledger

### FIND-TASK-001-18 — Deep JSON classification performs quadratic synchronous work

- **Discovery IDs:** `BVR-R7-BEH-001`, `INV-R7-001`, `ORACLE-R7-001`,
  `SYS-R7-001`, `VARIANT-R7-002`
- **Status:** REOPENED / CONFIRMED / CONSOLIDATED
- **Classification:** INCORRECT / RESOURCE SAFETY
- **Violated obligation:** Revision 13 `INV-007` requires Variant conversion to
  be bounded by value depth/size; the R6 depth correction also requires hostile
  valid JSON to reach Wyrd's numeric/depth decision without dependency work
  outrunning it.
- **Exact location and evidence:**
  `crates/shared/wyrd-queue/src/variant.rs:218-227,731-807,819-864`.
  `append_raw` reparses each accepted container, which is capped at 64 levels,
  but after depth 65 `scan_numbers` deserializes every pending container's
  complete remaining `RawValue`; a one-element chain therefore scans suffixes
  of length `n`, `n-2`, and so on. The source itself documents
  “subtree size times depth.” Buffered writes reach this owner through
  `batch_builder`, while Oracle `parse_json` and `try_parse_json` call it
  synchronously from `oracle/variant_sql.rs:600-620`.
- **Observable consequence:** A compact valid tenant value can occupy an
  ingest or DataFusion executor thread for work quadratic in its nesting before
  returning the expected error; a query deadline cannot preempt that synchronous
  scan.
- **Decision-complete minimal correction:** Keep `EncodedVariant` and
  `serde_json` as the only syntax/semantic authorities, but replace nested
  suffix deserialization with one iterative structural traversal of the
  already syntax-validated text that consumes each token/span once, records
  exact numeric tokens and pointers, retains final-key-wins object
  normalization, and builds only accepted members. Preserve malformed ->
  numeric -> depth selection, size precedence, `try_parse_json`'s invalid-only
  null behavior, and the current public API. Add no downstream timeout,
  second semantic model, public parser, or configuration knob.
- **Focused closure proof:** Retain the 64/65/128/129/10,000 classification
  cases and add deterministic test-only token/byte visitation evidence showing
  proportional work for 1,000- and 10,000-level inputs, plus the existing
  Oracle exact-error case. Do not use wall-clock thresholds or add a new
  journey harness.

### FIND-TASK-001-25 — Rejected JSON numbers can manufacture a size error

- **Discovery ID:** `VARIANT-R7-001`
- **Status:** REOPENED / CONFIRMED
- **Classification:** INCORRECT
- **Violated obligation:** The revision-13 locked contract and active Bifrost
  authority say JSON size counts bytes built from accepted members and a
  refused number contributes none; numeric range must therefore win when the
  accepted portion remains within 8,388,608 bytes.
- **Exact location and evidence:**
  `crates/shared/wyrd-queue/src/variant.rs:800-805` records the numeric
  violation and appends `Variant::Null`; `from_json_text` then sizes that
  temporary encoding at lines 224-226 before returning the recorded numeric
  error. In an object the append also retains the rejected field's metadata.
  The current size-priority test uses an accepted sibling already over the
  ceiling, so it cannot detect this case.
- **Observable consequence:** An accepted string or object just below the
  limit plus one refused number can return
  `WYRD_VALA_413_VARIANT_TOO_LARGE` instead of
  `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE`.
- **Decision-complete minimal correction:** At the existing `append_raw`
  owner, record a refused number without appending a value or object member;
  do not change accepted JSON null handling, traversal order, error selection,
  or the builder abstraction.
- **Focused closure proof:** Add one direct boundary case whose accepted
  members encode within the limit but whose current null/field placeholder
  crosses it, and assert the exact numeric-range violation and pointer; retain
  the existing case where accepted bytes alone exceed the ceiling and size
  correctly wins.

### FIND-TASK-001-4 — Active authority falsely promises linear raw validation

- **Discovery IDs:** revised portion of `STD-R7-001`; code portion of
  `VARIANT-R7-003` rejected
- **Status:** REOPENED / REVISED
- **Classification:** MISSING / AUTHORITY DRIFT
- **Violated obligation:** `AGENTS.md` makes `architecture/bifrost-design.md`
  the active Bifrost authority and requires implementation evidence and
  materially changed rustdoc to describe the behavior that actually ships.
- **Exact location and evidence:** `architecture/bifrost-design.md:169`,
  `crates/shared/wyrd-queue/src/variant.rs:941-944`, and
  `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md:307`
  claim linear validation. `object_field_slots` at
  `variant.rs:1050-1074` sorts field starts and binary-searches their
  successors, which is `O(fields log fields)`. It is still iterative,
  size-bounded, and rejects shared or overlapping regions.
- **Observable consequence:** The winning authority and completion evidence
  promise an algorithmic property the implementation does not provide, so a
  future maintainer cannot tell whether the simple safe implementation is
  compliant.
- **Decision-complete minimal correction:** Keep `object_field_slots`
  unchanged. Replace only the three linearity claims with the approved
  property actually needed by `INV-007`: iterative, bounded by the fixed
  encoded-size limit, and protected against shared/overlapping-region work
  amplification. Do not add a bitmap, interval tree, alternate scanner, or
  performance configuration merely to preserve accidental prose.
- **Focused closure proof:** Review the three amended statements against the
  implementation, retain the existing overlap/shared-region adversarial tests,
  and run `mise run docs:check` plus cumulative `git diff --check`.

### FIND-TASK-001-27 — Byte rendering fully validates the same Variant twice

- **Discovery ID:** `REUSE-R7-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / DUPLICATE MECHANISM
- **Violated obligation:** `AGENTS.md` section 15 and the reuse-review contract
  require one existing owner for semantically identical validation.
- **Exact location and evidence:** `EncodedVariant::validate` at
  `crates/shared/wyrd-queue/src/variant.rs:269-284` runs Wyrd's scan and then
  full upstream `Variant::try_new`; `variant_bytes_to_json` at lines 347-350
  immediately calls the same full upstream constructor again before rendering.
  Rust, Python, TypeScript, HTTP, MCP, and CLI byte-result adapters reach this
  renderer.
- **Observable consequence:** Every accepted byte-rendered Variant pays two
  identical recursive dependency validations, and the new shared owner is
  harder to reuse correctly because it discards the validated dependency
  value.
- **Decision-complete minimal correction:** Make the existing private borrowed
  gate return the validated dependency `Variant` it already constructs; byte
  rendering consumes that value, while `from_bytes` and non-rendering guards
  discard it. Parse `VariantMetadata` once and pass it through the existing
  Wyrd scan and upstream `try_new_with_metadata`. Preserve size-first behavior,
  panic containment, the depth short-circuit before upstream recursion, and
  the single public `EncodedVariant`; add no public wrapper or second renderer.
- **Focused closure proof:** Render one accepted nested object through
  `variant_bytes_to_json`, and retain the existing malformed, hostile-depth,
  overlap, duplicate-name, decimal, and non-finite refusal cases through the
  same gate.

### FIND-TASK-001-24 — OTLP non-finite refusal lacks user-boundary proof

- **Discovery IDs:** `BVR-R7-BEH-002`; proof-only portion of `SDK-R7-001`
- **Status:** REOPENED / CONFIRMED / REVISED TO PROOF-ONLY
- **Classification:** MISSING EVIDENCE
- **Violated obligation:** `AGENTS.md` section 11 requires user-journey proof
  for new user-observable negative behavior with cross-boundary state;
  `REQ-004`, `REQ-011`, `INV-002`, and `AC-001` require one invalid signal
  record to be refused without losing valid siblings.
- **Exact location and evidence:** The only new proof is
  `crates/vala/vala-bifrost-redux/src/tables/logs/mod.rs:170-203`, which calls
  the projector in process. The existing real journey
  `crates/wyrd/wyrd-testing/tests/bifrost/otlp/logs_export.rs:489-562`
  already proves the same partial-success/publish/query flow for an oversized
  body, but not for NaN or infinity.
- **Observable consequence:** Current evidence does not establish that the
  collector reports the numeric-range code/count, the invalid record remains
  absent durably, and its valid sibling survives acknowledgement, publication,
  and Oracle readback.
- **Decision-complete minimal correction:** Extend the existing log Variant
  OTLP journey, not production code or another harness, with one non-finite
  body and one valid sibling. Reuse the shared signal numeric gate; one log
  journey is sufficient because span/metric attributes use that same owner and
  its narrow projector behavior is already unit-covered.
- **Focused closure proof:** Assert one rejected log record and
  `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` in the real collector partial
  success, publish, and query the existing scope to prove exactly the finite
  sibling persisted. Retain the in-process projector test.

### FIND-TASK-001-28 — Physical schema reconciliation erases Variant identity

- **Discovery ID:** `PERSIST-R7-001`
- **Status:** CONFIRMED
- **Classification:** INCORRECT / DATA INTEGRITY
- **Violated obligation:** `REQ-003` requires one canonical Variant logical
  type to round-trip through Arrow and Iceberg; existing-table reconciliation
  must not equate that atomic type with an ordinary user Struct that merely has
  the same storage children.
- **Exact location and evidence:** `validate_physical_table` converts the
  current Iceberg schema and calls `schema_shape_matches` at
  `crates/vala/vala-bifrost-redux/src/catalog/bifrost_catalog.rs:1101-1107`.
  `field_shape_matches` at lines 1523-1528 compares only name, nullability, and
  `DataType`; recursive `arrow_type_shape_matches` at
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:833-878` likewise never
  checks field extension metadata. Therefore canonical Variant and the plain
  `Struct<metadata: Binary, value: Binary>` storage shape compare equal.
- **Observable consequence:** Reconciliation can accept a stale or externally
  altered physical table whose catalog contract says Variant while its Iceberg
  schema is an ordinary Struct, leaving readers and writers with different
  logical types.
- **Decision-complete minimal correction:** Extend the existing field-level
  recursive physical comparator to require canonical Variant parity via
  `wyrd_queue::variant::is_variant` before applying the existing approved
  binary/string/list/timezone normalizations. Preserve field-ID exclusion,
  nested name matching, and all current aliases; add no second schema
  comparator or migration path.
- **Focused closure proof:** Add focused top-level and nested comparisons that
  reject Variant-versus-storage-Struct in both directions, retain the accepted
  timezone/width normalizations, and retain the pinned Iceberg Variant
  round-trip as positive proof.

### FIND-TASK-001-3 — Required failed-lineage/no-commit transition is unproven

- **Discovery ID:** `PERSIST-R7-002`
- **Status:** REOPENED / CONFIRMED AS PROOF-ONLY
- **Classification:** MISSING EVIDENCE
- **Violated obligation:** Original TASK-001 lines 126-145 expressly require
  `v3_row_lineage_survives_repeated_rewrite` to inject missing lineage and
  prove the failed rewrite commits nothing while preserving the existing
  handoff and recovery identity.
- **Exact location and evidence:**
  `crates/vala/vala-bifrost-redux/tests/integration/forge/managed_rewrite.rs:1450-1562`
  proves successful rewrites and GC only. The pinned fork's
  `row_lineage_is_complete` calls the predicate directly and cannot prove that
  validation remains before output handoff or commit. Production currently
  validates each batch before writing it, so no production defect is asserted.
- **Observable consequence:** The evidence can stay green if a later change
  moves lineage validation after an output write or permits a commit-capable
  handoff after a later-batch failure.
- **Decision-complete minimal correction:** Extend the existing managed
  rewrite/fork test seam to deliver at least one valid batch followed by a
  missing or null lineage batch, then exercise the existing managed-attempt
  observer and publication boundary. Add no public injection hook, alternate
  handoff, or production guard unless the test falsifies current source.
- **Focused closure proof:** Assert rewrite failure, unchanged snapshot and
  logical rows, no successful or commit-capable handoff, and complete
  accounting of any opened output as reclaimable possible output; retain the
  existing two-rewrite and GC success assertions.

## Mandatory completion cleanup

The following confirmed repository-rule failures block completion but are not
separate stable implementation findings.

### Exact focused-command evidence

- **Discovery ID:** `STD-R7-002`
- **Status:** CONFIRMED AS CLEANUP
- **Violated obligation:** `AGENTS.md` section 11 and the spec-driven
  development reference require every specifically named Rust test in a task
  or implementation record to be run and recorded with an explicit package,
  target, and exact `test(=...)` selector.
- **Exact location and evidence:**
  `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md:305-338`
  names six new Variant tests but records only the whole `wyrd-queue` library
  lane, names the modified maximal-log test without a focused command, and
  uses regex `test(/unresolved_call_nulls_resolved_model_children/)` for the
  gateway unit.
- **Observable consequence:** The implementation record cannot prove that the
  exact named tests were selected rather than merely compiled, skipped, or
  matched incidentally by a broad expression.
- **Decision-complete minimal correction:** Run every named new or modified
  Rust test through `mise exec -- cargo nextest run --locked` with its explicit
  package/target/features and exact `test(=fully::qualified::name)` expression,
  using the repository Postgres wrapper only where the test needs it; append
  those commands, counts, and exits to the existing R6 evidence. Retain the
  already-green whole-library and journey lanes; add no script or checker.
- **Focused closure proof:** Exact commands are required for
  `raw_shared_field_values_are_refused`,
  `renderers_refuse_hostile_stored_variants`,
  `json_depth_is_decided_by_wyrd_at_any_depth`,
  `raw_numbers_outside_the_json_domain_are_refused`,
  `json_size_outranks_numeric_and_depth`,
  `raw_repeated_field_names_are_refused`,
  `maximal_log_projection_preserves_body_context_and_presence`, and
  `unresolved_call_nulls_resolved_model_children`; record one selected/passed
  test for each exact selector.

### Cumulative whitespace gate

- **Discovery ID:** `STD-R7-003`
- **Status:** CONFIRMED AS CLEANUP
- **Violated obligation:** `AGENTS.md` completion requires the applicable
  repository gates to pass, and a red gate blocks completion regardless of
  where the defect entered the cumulative candidate.
- **Exact location and evidence:**
  `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..6147cc617d81f2c03464043be698ab2565e9d745`
  reports
  `changes/active/bifrost-variant/review/TASK-001-r6/standards-review.md:88: new blank line at EOF`.
  The R6 implementation record checked only `b4ea01848..HEAD`, which excluded
  that earlier artifact.
- **Observable consequence:** The immutable cumulative task candidate does not
  pass its required whitespace check despite the recorded green narrower
  range.
- **Decision-complete minimal correction:** Delete only the extra terminal
  blank line; change no report content and add no formatter or check.
- **Focused closure proof:** Run the exact cumulative `git diff --check`
  command above and record exit zero.

## Rejected or narrowed claims

- The raw object slot algorithm is not required to become linear. Its sort is
  the smallest clear way to support unordered offsets and reject overlap under
  the fixed 8 MiB bound; only the false linearity claim is actionable.
- Non-finite Float/Double refusal is authorized by revision 13's specific
  canonical-Arrow numeric-domain rule and the approved R6 decision. The broad
  “IEEE meaning” sentence governs accepted floating values; it does not reopen
  the rejected raw forms.
- No Oracle timeout/cancellation guard is an acceptable fix for finding 18;
  it would leave buffered writes vulnerable and duplicate the shared owner's
  responsibility.
- No production lineage change is authorized by finding 3 unless the required
  negative proof fails against current source.

## Recommendation

**FIX_REQUIRED**

Seven stable findings remain across six remediation root groups: JSON
traversal, JSON actual-built size, raw validation and authority, physical
Variant reconciliation, proof, and mandatory evidence cleanup. The proof
group contains reopened findings 24 and 3; the cleanup group contains the
exact-command and cumulative-whitespace gates without
minting separate stable IDs. No product, public API,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision is missing, so no specification revision is required.

## Identity confirmation

At completion, `HEAD^{commit}` remained
`6147cc617d81f2c03464043be698ab2565e9d745` and `HEAD^{tree}` remained
`7b7bb069ecbe8600e09cc8b6ea4e938709614718`.
