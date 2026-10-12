# TASK-001 r5 Structured Ponytail Validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Latest remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Specification reviewed with the immutable candidate: revision 12; approved
  post-validation decision: revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate commit and tree were resolved directly before validation and
matched the supplied immutable subject. The untracked r5 review directory is
outside that tree. CodeGraph was used first for the current Variant, table
validator, gateway, and result paths. Validation then inspected the complete
cumulative and latest-remediation diffs, the approved specification and task,
the applicable repository and Bifrost authorities, all r1-r4 verdicts,
validated ledgers and remediation tasks, every r5 discovery report,
`followup-review.md`, `root-cause.md`, current callers and tests, and the pinned
Arrow 59.3 Variant implementation and JSON renderer.

All required r5 reports are present: behavior, invariants, standards,
maintainer, system resilience, reuse, the six routed domain reviews, focused
follow-up, and root-cause analysis. No report is missing or blocked.

## Proposed-finding disposition

| Discovery proposal | Disposition | Stable finding | Validation result |
|---|---|---|---|
| `BVR-R5-BEH-001`, `INV-R5-001`, JSON half of `VARIANT-ARROW-R5-001` | **CONFIRMED / CONSOLIDATED** | `FIND-TASK-001-18` | `append_raw` skips the first over-depth subtree, so the r4 numeric-before-depth correction is incomplete below depth 64. |
| `BVR-R5-BEH-002`, raw-byte half of `VARIANT-ARROW-R5-001` | **REVISED / CONSOLIDATED** | `FIND-TASK-001-16` | Depth still outranks malformed encoding below the cap. The correction must validate iteratively through the whole bounded encoding; merely checking safely reached nodes is not enough. |
| `INV-R5-002`, `OTLP-TABLES-002`, `FOLLOWUP-R5-001` | **REVISED / CONSOLIDATED** | `FIND-TASK-001-14` | Revision-12 physical nullability opened one shared semantic gap: Results, Metrics, and Calls accept nullable-child states their domain values cannot represent. |
| `OTLP-TABLES-001`, `PERSIST-R5-001`, `MNT-R5-001` | **CONFIRMED / CONSOLIDATED** | `FIND-TASK-001-22` | The `resolved_model` storage repair also weakens required `requested_model`; this is separate from value validation. |
| `INV-R5-003`, `STD-R5-002`, `MNT-R5-002` | **REVISED / CONSOLIDATED** | `FIND-TASK-001-21` | The prior stale-evidence finding remains incompletely closed across the active task and implemented R4 packet. |
| `STD-R5-001` | **CONFIRMED** | `FIND-TASK-001-23` | The later `wyrd-implement` policy edits have no TASK-001 authority; the user exception covered only the mirrored task-review edits. |
| `REUSE-R5-001` | **REJECTED** | — | No duplicate owner exists in the immutable base or candidate; an unintegrated TASK-002 commit is not a reusable candidate owner. |
| `REUSE-R5-002` | **REJECTED** | — | The candidate contains one Scribe implementation, copied exactly under explicit human direction for stack convergence. It is neither duplicate nor uncalled infrastructure in the reviewed subject. |
| `FOLLOWUP-R5-002` | **CONFIRMED; DECISION COMPLETED BY REVISION 13** | `FIND-TASK-001-24` | Raw Decimal16 values outside the exact terminal domain must be refused before ACK with the existing numeric-range error. |

`VARIANT-ARROW-R5-001` is correctly split: JSON-token conversion and raw
Variant-byte validation have different inputs, owners inside `EncodedVariant`,
failure mechanisms, and closure proofs. They are not one corrective change.

`FIND-TASK-001-13` remains omitted. It was the approved rejected workflow-scope
exception and is not revived by this round. Existing identities 14, 16, 18,
and 21 are preserved because the current defects are incomplete closure of
those findings. New retained defects begin at the next unused ID, 22.

## Root-cause validation

The r5 root-cause ledger is correct with two refinements reflected in the final
ledger:

1. The JSON subtree case is incomplete closure of `FIND-TASK-001-18`, not a
   new numeric contract. The r4 implementation retained depth and continued
   only through siblings; it does not inspect descendants of the first skipped
   container.
2. The malformed/deep case is incomplete closure of `FIND-TASK-001-16`. Its
   safe preflight solved recursive stack exhaustion, but selecting depth before
   complete encoding validity contradicts the active size/encoding/depth order.
   The correction cannot call recursive `Variant::try_new` first and cannot
   stop validation at depth 65.
3. Verification summaries, exponential-histogram buckets, and
   `resolved_model` share one remediation-induced semantic root. Physical child
   nullability is necessary for correct Parquet reads, but the existing
   table-owned validator seam was not extended to enforce
   complete-present/null-absent values. Correct the shared invariant once and
   invoke it from the three existing table validators.
4. The `requested_model` declaration is independent of that semantic root.
   Even perfect row validation would leave the public/persisted schema and
   fingerprint advertising optional children for a required value.
5. The task status/revision text, deleted symbol, and contradictory R4 packet
   are one active-artifact lifecycle root and remain `FIND-TASK-001-21`.
6. The `wyrd-implement` edits are independent scope drift. Commits
   `a6f1326f6` and `a6429060f` changed only the mirrored `wyrd-task-review`
   skill under the explicit exception. Later commit `c8da11067` changes the
   repository-wide implementation protocol and was never covered by it.
7. Raw high-precision decimals share numeric-contract lineage with
   `FIND-TASK-001-11`, but are not incomplete implementation of revision 11's
   JSON-token decision. After validation, the user selected the smallest
   user-facing behavior in approved revision 13: raw Arrow uses the same exact
   numeric domain and refuses every non-canonical Decimal16 before ACK.

The historical `FIND-TASK-001-5`/`-12` general distributed-error root remains
closed by `QueryCatalogError`; the stale reference to the deleted
`remote_variant_error` is artifact drift, not a runtime recurrence. Other prior
root-cause decisions remain consistent with the current candidate.

## Final deduplicated ledger

### FIND-TASK-001-14 — Nullable built-in Structs admit semantically partial values

- **Discovery proposals:** `INV-R5-002`, `OTLP-TABLES-002`,
  `FOLLOWUP-R5-001`
- **Status:** REVISED / CONSOLIDATED
- **Classification:** INCORRECT / REGRESSION
- **Violated obligation:** Revision 12 makes children nullable only to preserve
  child nulls beneath an absent persisted Struct. It also requires every child
  of a present verification summary to be written and every child of an absent
  one to be null. OTLP `Buckets` and `Option<ModelRef>` have the same closed
  typed meaning. INV-002 and the pre-ACK built-in boundary prohibit storing a
  third, partial state.
- **Exact locations:**
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:272-291,348-395,431-436`;
  `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs:38-67`;
  `crates/vala/vala-bifrost-redux/src/tables/metrics/projection.rs:290-335`;
  `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:33-45,111-112`.
- **Evidence:** `validate_predeclared` establishes schema shape and validates
  nested Variant bytes, but returns success for a null nested child.
  `validate_metric_points` validates metric-kind occupancy and numeric
  alternatives but never compares `positive_buckets` or `negative_buckets`
  parent validity with `offset` and `bucket_counts`. `CallsTable` uses the
  generic validator. Consequently an authorized canonical Arrow batch may
  submit a present summary, bucket set, or resolved model with a missing child,
  or a null parent retaining visible child values, and reach Scribe/WAL. Current
  producers and journeys generate only the two valid states, so their green
  proof does not cover this public trust boundary.
- **Observable consequence:** Durable built-in rows can contain a report,
  bucket collection, or resolved model that no owning Rust/OTLP domain value
  represents. `get_field` can expose hidden values beneath a null parent or SQL
  null for a required member of a present value.
- **Decision-complete correction:** Keep the revision-12 nullable physical
  children. Add one private table-layer helper at the existing
  `DomainTable::CANONICAL_VALIDATOR` seam that, for a named Struct column,
  requires each child's row validity to equal the parent validity. Invoke it
  after existing schema and Variant validation from a `ResultsTable` validator,
  from `validate_metric_points` for both bucket columns, and from a
  `CallsTable` validator for `resolved_model`. Reuse `BifrostError::SchemaParse`
  with row/column detail. Do not normalize reads, repair rows, create a new
  validator framework, or add per-consumer guards.
- **Focused closure proof:** Add focused table-validator cases for present
  missing-child and absent retained-child rows. Extend the existing raw-Arrow
  real-server admission journey to exercise one invalid value for each of
  Results, Metrics, and Calls, assert `WYRD_VALA_400_SCHEMA_PARSE`, no ACK/no
  retained row, and a following valid write. Retain the hot/published
  absent/present journeys.

### FIND-TASK-001-16 — Raw depth preflight still selects depth before malformed encoding

- **Discovery proposals:** `BVR-R5-BEH-002`, raw half of
  `VARIANT-ARROW-R5-001`
- **Status:** REVISED / CONSOLIDATED
- **Classification:** INCORRECT
- **Violated obligation:** `architecture/bifrost-design.md:158-164` and the
  locked write order require raw stored Variant values to select size, encoding
  validity, then depth. `FIND-TASK-001-16` also requires hostile depth to remain
  bounded before any recursive dependency validation.
- **Exact location:**
  `crates/shared/wyrd-queue/src/variant.rs:184-196,766-790`, reached by
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:348-372` before Scribe
  dispatch/WAL/ACK.
- **Evidence:** `from_bytes` propagates `check_depth` before calling
  `Variant::try_new`; `check_depth` returns at the first depth-65 container.
  Malformed descendants or a malformed later branch are therefore never
  reached by the only full validation call and the public result is
  `VariantTooDeep`. The isolated r4 tests pair a valid deep value with a
  separate shallow malformed value and cannot prove their compound order.
- **Observable consequence:** Physical child order/depth can change a malformed
  raw Variant's stable code from `WYRD_VALA_400_VARIANT_INVALID_JSON` to
  `WYRD_VALA_400_VARIANT_TOO_DEEP`, despite both requests being safely refused.
- **Decision-complete correction:** Keep `EncodedVariant::from_bytes` as the
  sole owner and size first. Replace the recursive early-return depth walk with
  one explicit-stack traversal over the installed Variant representation's
  shallow accessors across the whole already-size-bounded value. Fully validate
  metadata and every shallow node/offset/name-order condition while recording
  the first depth violation; malformed evidence wins. Call upstream recursive
  `Variant::try_new` only when recorded depth is within 64. Contain any
  infallible-access panic at this owner as today. Do not parse Variant bytes a
  second time, restore recursive validation before the cap, add a model, or add
  a configurable limit.
- **Focused closure proof:** Add a direct raw-byte case with malformed content
  below a depth-65 wrapper and the sibling orders reversed, expecting
  `InvalidJson`; extend the existing raw-IPC journey with the same compact
  compound value, exact catalog problem, no ACK/no row, and a following valid
  write. Retain the 20,000-level valid hostile-depth availability proof.

### FIND-TASK-001-18 — JSON numeric precedence still stops at the depth boundary

- **Discovery proposals:** `BVR-R5-BEH-001`, `INV-R5-001`, JSON half of
  `VARIANT-ARROW-R5-001`
- **Status:** CONFIRMED / CONSOLIDATED
- **Classification:** INCORRECT
- **Violated obligation:** Spec revision 12's locked write order places numeric
  range before depth, independent of layout, through JSON-row admission and
  Oracle `parse_json`.
- **Exact location:**
  `crates/shared/wyrd-queue/src/variant.rs:154-164,619-684`; consumers at
  `crates/shared/wyrd-queue/src/batch_builder.rs:349-385` and
  `crates/vala/vala-bifrost-redux/src/oracle/variant_sql.rs:535-620`.
- **Evidence:** On entering the first container at depth 65, `append_raw`
  stores `TooDeep` and immediately returns without deserializing that raw
  subtree. Sixty-five nested arrays around `18446744073709551616` therefore
  return depth, while the same token in an in-limit sibling returns numeric
  range. The value is valid JSON and far below size ceilings. The r4 tests swap
  sibling keys only; neither places the numeric token below the skipped
  boundary.
- **Observable consequence:** Equivalent compound-invalid JSON receives a
  different stable public problem based on where the number sits relative to
  depth 64.
- **Decision-complete correction:** At the existing raw-token walker, retain
  the first depth violation but continue a non-building numeric-token scan of
  the skipped `RawValue` subtree, reusing `raw_number_variant` and the existing
  path owner. The admitted JSON/request-size bound is the ceiling. Do not build
  the rejected Variant subtree, add a parser/model, or add a setting.
- **Focused closure proof:** Prove 65 nested arrays around an out-of-range
  integer through direct conversion, Oracle `parse_json`, and the existing
  pre-ACK JSON-row journey. Assert the numeric-range code/path, no retained
  row, subsequent server availability, and unchanged isolated-depth behavior.

### FIND-TASK-001-21 — Active task and R4 remediation records still contradict the candidate

- **Discovery proposals:** `INV-R5-003`, `STD-R5-002`, `MNT-R5-002`
- **Status:** REVISED / CONSOLIDATED
- **Classification:** VIOLATION
- **Violated obligation:** The active packet must identify one approved
  authority, current lifecycle state, operative instruction, and factual
  implementation owner so review and remediation remain reproducible.
- **Exact locations:**
  `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md:4,50-53,246-255`;
  `changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md:4-7,15-18,49-55,254-313`.
- **Evidence:** TASK-001 remains `proposed`, its operative approach says to
  implement revision 10, and its final Oracle evidence cites deleted
  `remote_variant_error`; the current general owner is `QueryCatalogError` in
  `oracle/mod.rs`. R4 remains `ready` and bound to revision 11; its initial
  correction mandates non-null children, while its later human resolution and
  evidence bind revision 12 and nullable children. These are operative
  contradictions, not the clearly dated cold-rehearsal history.
- **Observable consequence:** A maintainer following the active packet is told
  to restore superseded storage or navigate a nonexistent error owner, and
  cannot tell which already-executed task is awaiting review.
- **Decision-complete correction:** Update TASK-001 to `status: review`, bind
  its operative approach to revision 13, and replace the deleted symbol claim with
  the general `QueryCatalogError` structured local/remote catalog-error owner.
  Mark R4 `status: complete`, bind its front matter/authority and operative
  outcome/constraints to revision 12, and label the revision-11 non-null-child
  diagnosis as superseded historical evidence without erasing it. Add no new
  evidence artifact or checker.
- **Focused closure proof:** Static-check TASK-001 against revision 13 and R4's
  superseded/operative sections against revision 12, resolve every cited source symbol in the
  candidate, and run `git diff --check`.

### FIND-TASK-001-22 — `requested_model` inherited `resolved_model`'s nullable children

- **Discovery proposals:** `OTLP-TABLES-001`, `PERSIST-R5-001`, `MNT-R5-001`
- **Status:** CONFIRMED / CONSOLIDATED
- **Classification:** REGRESSION
- **Violated obligation:** `GatewayCallPayloadV1` requires
  `requested_model: ModelRef` and allows `resolved_model: Option<ModelRef>`;
  each present `ModelRef` requires both `provider` and `model`. The human R4
  follow-on authorizes nullable persisted children only beneath the optional
  resolved model and requires unrelated schemas to remain unchanged.
- **Exact location:**
  `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:33-45,111-112`;
  typed contract at `crates/wyrd-spec/src/gateway/record.rs:542-545` and
  `crates/wyrd-spec/src/gateway/mod.rs:96-111`.
- **Evidence:** One `model_ref_type()` returns nullable children and is reused
  by both fields. The table test asserts only the parent nullability. The
  unresolved-capture test proves the optional producer but cannot detect the
  required request schema weakening.
- **Observable consequence:** The registered Arrow/Iceberg schema and
  fingerprint advertise a partial requested model that the durable wire value
  cannot represent, and required Parquet leaves were changed for a field with
  no absent-parent problem.
- **Decision-complete correction:** Delete the misleading shared helper and
  declare the two small Struct types explicitly in `CallsTable::arrow_fields`:
  non-null children for `requested_model`, nullable children for
  `resolved_model`. Keep the shared semantic validator from finding 14 for a
  present/absent resolved model. Add no wrapper type, trait, compatibility
  schema, or migration.
- **Focused closure proof:** Extend the existing gateway table contract test to
  assert both nested child layouts and retain
  `unresolved_call_nulls_resolved_model_children`; the finding-14 raw admission
  proof rejects a partial present resolved model.

### FIND-TASK-001-23 — TASK-001 includes unapproved global `wyrd-implement` policy

- **Discovery proposal:** `STD-R5-001`
- **Status:** CONFIRMED
- **Classification:** VIOLATION / DRIFT
- **Violated obligation:** TASK-001 is scoped to Bifrost Variant storage/query
  behavior and proof. Its R4 non-goals exclude unrelated workflow-policy
  refactors. The explicit user exception covered finding 13 and mirrored
  `wyrd-task-review` changes only.
- **Exact locations:** `.agents/skills/wyrd-implement/SKILL.md:38-48,102-111`
  and generated mirror `.claude/skills/wyrd-implement/SKILL.md` at the same
  sections; introduced by `c8da11067`.
- **Evidence:** The edits make repository/dependency searches mandatory for
  every future implementation, add a blocking relationship to `reuse-rev`, and
  require a new evidence table for every new item. No TASK-001 behavior,
  acceptance criterion, or approved exception requires those global workflow
  semantics. The earlier allowed commits `a6f1326f6` and `a6429060f` touch only
  `wyrd-task-review`.
- **Observable consequence:** Merging a Bifrost storage task silently changes
  the cost and protocol of every future Wyrd implementation.
- **Decision-complete correction:** Remove only commit `c8da11067`'s changes
  from the two mirrored `wyrd-implement` files. Retain the explicitly allowed
  `wyrd-task-review` changes and land any desired implementation-policy change
  under separately approved workflow scope.
- **Focused closure proof:** Base-to-candidate changed-path/diff audit plus
  `mise run check:skills-sync`.

### FIND-TASK-001-24 — Raw non-canonical decimals are admitted despite the exact terminal contract

- **Discovery proposal:** `FOLLOWUP-R5-002`
- **Status:** CONFIRMED / DECISION COMPLETE UNDER SPEC REVISION 13
- **Classification:** INCORRECT
- **Violated obligation:** Revision 13 gives canonical Arrow Variant input the
  same exact numeric domain as JSON: integer primitives fit `i64`; Decimal16 is
  accepted only at scale zero for `i64::MAX + 1..=u64::MAX`; every other
  Decimal16 is refused before ACK with numeric-range and
  `numeric_kind: "decimal"`.
- **Exact locations:** admission at
  `crates/shared/wyrd-queue/src/variant.rs:184-196` and
  `crates/vala/vala-bifrost-redux/src/tables/mod.rs:348-372`; public raw write at
  `crates/shared/wyrd-client/src/bifrost/facade.rs:477-509`; native/JSON
  rendering at `crates/shared/wyrd-queue/src/variant.rs:246-251,280-337` and
  `crates/shared/wyrd-client/src/bifrost/facade.rs:895-925`; dependency fallback
  at `parquet-variant-json-59.3.0/src/to_json.rs:285-304`.
- **Reachability proof:** An authorized Rust caller can create
  `VariantDecimal16::try_new(18_446_744_073_709_551_617_i128, 0)`, append it
  with the installed `VariantBuilder`, pass the finished bytes through public
  `EncodedVariant::from_bytes`, append that value with public
  `VariantColumnBuilder`, and send the resulting canonical extension batch via
  public `Bifrost::write_batch`. Scribe's built-in validator reaches
  `from_bytes`, which checks size, depth, and structural validity only; the
  decimal is valid and is admitted before WAL/ACK. Query Arrow terminals retain
  the encoded extension, but every shared JSON/native terminal delegates to
  `VariantToJson::to_json_value`. For Decimal16 outside both i64 and u64, that
  implementation casts to `f64`; the example above rounds to
  `18_446_744_073_709_551_616`, and high-precision scaled decimals have the same
  terminal loss. This path is public and reachable, not dormant test support.
- **Observable consequence:** A successful canonical Arrow write can later
  return a different numeric type/value through Rust typed rows, HTTP/MCP/CLI
  JSON, and other projections using the shared renderer, violating REQ-004.
- **Decision-complete correction:** Extend the same explicit-stack raw Variant
  validation required by finding 16 to enforce revision 13's numeric domain.
  Retain valid integer primitives and only scale-zero Decimal16 coefficients in
  `i64::MAX + 1..=u64::MAX`; return the existing
  `VariantNumericOutOfRange` with the field, row, path, and
  `numeric_kind: "decimal"` for every other Decimal16. Do not add an
  arbitrary-precision type, renderer, dependency, wire shape, or error code.
- **Focused closure proof:** Add direct and public canonical-Arrow pre-ACK
  cases for a scale-zero Decimal16 above `u64::MAX`, a fractional Decimal16,
  and a valid `u64::MAX` Decimal16. Assert exact numeric-range details and no
  retained row for the first two, exact round-trip for the last, and continued
  server availability.

## Rejected reuse proposals

### `REUSE-R5-001` — raw-row preparation duplicates TASK-002

**REJECTED.** The immutable base and candidate contain one `BatchBuilder` owner.
Its raw-field extension is necessary because the prior `serde_json::Value`
path destroys the exact rejected integer token used by the approved JSON-row
admission proof. Commit `0fcb8a7ac` is on an unintegrated sibling task and is
not callable code in this subject. A raw-IPC fixture would test already-encoded
Variant bytes, not the public JSON conversion path governed by finding 18.
Later TASK-002 integration may replace the smaller implementation, but future
merge work is not a duplicate in this candidate.

### `REUSE-R5-002` — Scribe masked-null handling duplicates TASK-002

**REJECTED.** The candidate has one implementation in Scribe. The human R4
resolution explicitly directs adoption of TASK-002 commit `535367c94` verbatim
so the eventual stack converges on one owner. The code still protects the
general Arrow IPC/WAL boundary; lack of a current required-child producer in
the revision-12 built-ins does not make explicitly approved shared boundary
handling dead or duplicated. Deleting it would diverge the instructed stack
and is not a TASK-001 simplification.

## Verification assessment

- Candidate/tree identity: **PASS** before writing this ledger.
- Required r5 report inventory: **COMPLETE**.
- `git diff --check` for the cumulative and latest-remediation ranges was
  reported green by independent reviewers; this validator rechecks it at
  handoff.
- Existing focused unit and journey evidence proves the isolated and healthy
  cases it names. It does not exercise a numeric token below depth 64,
  malformed encoding below that boundary, partial nullable-child Structs,
  requested-model nested nullability, or raw high-precision Decimal16 terminal
  exactness.
- The expensive PostgreSQL, object-store, cross-language, and pinned-fork
  suites were not rerun by this validator. Their existing green evidence does
  not close the source-proven gaps above.

## Post-validation human decision

The user rejected a no-packet `SPEC_REVISION_REQUIRED` outcome and directed the
review to choose from intended user behavior. Approved spec revision 13 selects
refusal: canonical Arrow uses the same exact i64/u64/double terminal domain as
JSON, and non-canonical Decimal16 values fail before ACK. No arbitrary-precision
public number surface is added.

## Recommendation to orchestrator

**FIX_REQUIRED.** The implementation ledger retains
`FIND-TASK-001-14`, `-16`, `-18`, `-21`, `-22`, `-23`, and `-24`; every
correction is decision-complete under approved revision 13 and reuses an
existing owner or installed mechanism.
