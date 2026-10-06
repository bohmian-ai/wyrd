# TASK-001 r5 focused follow-up review

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Latest remediation range: `bb6ae8070e276c20011e675ba1a804f0356e51ea..0e37748f3a27d3bcec4713e6210e97328e045886`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The commit and tree matched the supplied immutable subject before this report
was written. CodeGraph was used first, followed by the complete cumulative and
latest-remediation diffs, the original task/specification, all prior verdicts,
validated ledgers and remediation tasks, every r5 discovery report, current
owners/callers/tests, and the pinned Variant dependency source.

## Conflict resolution

### 1. Numeric range below the depth boundary

**Resolved: the finding is source-confirmed and is incomplete closure of
`FIND-TASK-001-18`.**

The locked write order in `spec.md:219-225` places numeric range before depth.
`EncodedVariant::from_json_text` is the shared producer for JSON-row admission
and Oracle `parse_json`, but `append_raw` records depth and returns without
inspecting the first over-depth subtree (`variant.rs:628-634`). It therefore
cannot discover an out-of-range integer inside that subtree. The r4 correction
continued only through siblings (`variant.rs:608-612`), although its authority
required numeric range to win independently of traversal order.

`BVR-R5-BEH-001`, `INV-R5-001`, and the JSON half of
`VARIANT-ARROW-R5-001` are the same proposed finding. The correction belongs
at `EncodedVariant::from_json_text`/`append_raw`: retain the first depth error,
but inspect the skipped raw subtree for higher-priority numeric tokens without
building a Variant subtree or adding a second Variant model. The work remains
bounded by the admitted JSON/request size.

### 2. Malformed raw bytes combined with excessive depth

**Resolved: the finding is source-confirmed, but its authority and correction
must be stated more narrowly than the discovery reports do.**

`architecture/bifrost-design.md:158-164` explicitly requires each raw stored
Variant to be checked in size, encoding, then depth order. The current
`EncodedVariant::from_bytes` returns the result of `check_depth` before calling
`Variant::try_new` (`variant.rs:184-196`), and `check_depth` returns immediately
at depth 65 (`variant.rs:766-790`). A malformed descendant below that boundary
is therefore reported as `TooDeep`, not `InvalidJson`.

`FIND-TASK-001-16` correctly prohibited calling upstream recursive full
validation before a depth guard because a compact hostile value can exhaust
the process stack. Its remediation text also said to stop at depth 65 and call
`Variant::try_new` only after the depth walk passes. That lower-authority
implementation prescription does not erase the active architecture's
encoding-before-depth contract. The valid combined outcome is one bounded,
non-recursive shallow-access walk that establishes encoding validity while
retaining the first depth violation; upstream recursive `try_new` remains
appropriate only after the value is known to be within the recursion bound.
The correction must not restore the pre-r4 stack-exhaustion path or introduce a
second byte-format parser.

`BVR-R5-BEH-002` and the raw-byte half of `VARIANT-ARROW-R5-001` are one
proposed finding sharing the `EncodedVariant::from_bytes` root with prior
`FIND-TASK-001-16`.

### 3. Nullable persisted children versus complete present values

**Resolved: one shared semantic-validation root exists, and it covers three
built-ins.**

Revision 12 deliberately makes children nullable so child nulls survive
Parquet when a parent Struct is absent. Physical nullability does not make a
present domain value partial:

- `drift_report` and `eval_summary` must populate every child when present
  (`spec.md:228-258`), but `ResultsTable` retains the generic
  `validate_predeclared` path.
- `positive_buckets` and `negative_buckets` are complete OTLP `Buckets` values
  when present, but `validate_metric_points` checks kind-specific top-level
  occupancy and never their child validity (`metrics/projection.rs:277-335`).
- `resolved_model` is `Option<ModelRef>`; when present, `ModelRef` requires
  both `provider` and `model` (`wyrd-spec/src/gateway/mod.rs:96-111`), but
  `CallsTable` also retains the generic validator.

The repository already has the single Scribe seam for this work:
`DomainTable::CANONICAL_VALIDATOR` (`tables/mod.rs:151-180,431-436`). Metrics
already installs its table-specific validator. The shared root is that the
revision-12 physical-schema remediation did not preserve the domain
all-or-none invariant at those table-owned validators. The correction is to
enforce complete-present/null-absent state at that existing seam for Results,
Metrics, and Calls, reusing `SchemaParse` and preserving schema/Variant failure
ordering. This consolidates `INV-R5-002` and `OTLP-TABLES-002` and adds the
previously unreviewed reachable `resolved_model` path.

### 4. `requested_model` was weakened while repairing `resolved_model`

**Resolved: the finding is confirmed and independent of the semantic-validator
root above.**

`CallsTable::model_ref_type` now declares nullable children and is used by both
the non-null `requested_model` and nullable `resolved_model`
(`calls.rs:33-45,111-112`). The typed wire contract requires a complete
`requested_model: ModelRef` and an optional but complete
`resolved_model: Option<ModelRef>`. The human-directed r4 follow-on names the
leak and fix for `resolved_model`; it does not authorize weakening the required
request. The smallest correction is distinct child nullability declarations
inside `CallsTable`, with the existing schema test pinning both nested shapes.
Table semantic validation remains separately required for a present
`resolved_model` because its children must stay physically nullable.

`OTLP-TABLES-001`, `PERSIST-R5-001`, and `MNT-R5-001` are duplicates of this
one proposal.

### 5. Scribe masked-null code copied from TASK-002

**Resolved: reject `REUSE-R5-002`.**

The candidate/base range contains one Scribe implementation, not two. A commit
on an unintegrated sibling task is not an existing owner in the immutable base
or candidate. More importantly, the human-directed r4 remediation explicitly
requires adopting TASK-002 commit `535367c94` verbatim so the eventual stack
has one implementation. Revision 12 means TASK-001's current built-in
producers no longer need required children masked by a null parent, but that
does not turn the explicitly approved shared implementation into an
unauthorized duplicate. No current runtime defect follows from retaining it.

### 6. `RawValue` row preparation versus TASK-002

**Resolved: reject `REUSE-R5-001`.**

No semantically equivalent row-preparation owner exists in the immutable base
or candidate. The base `BatchBuilder` is the owner being extended; it parsed
each row to `serde_json::Value`, which loses the original numeric token needed
to enforce the approved JSON integer contract. The candidate keeps raw fields
in that same owner and routes Variant values to the already shared
`EncodedVariant::from_json_text`. An unintegrated TASK-002 commit cannot be the
reuse target for TASK-001 review. The r4 remediation also expressly requires a
pre-ACK JSON-row proof of numeric-before-depth; a raw encoded-Variant fixture
cannot represent the rejected JSON token through the same public conversion
path. TASK-002 may later replace this partial implementation when integrated,
but that future merge concern is not a duplicate in this subject.

### 7. Workflow-policy and active-packet drift

**Resolved.**

- `STD-R5-001` is confirmed. Commit `c8da11067` changes both mirrored
  `wyrd-implement` skills with repository-wide mandatory policy and evidence
  requirements. The r4 approved exception covered the mirrored
  `wyrd-task-review` changes only; its validator explicitly excluded the later
  `wyrd-implement` edits from the pinned candidate. Remove only the
  `wyrd-implement` changes from this task range.
- `STD-R5-002`, `INV-R5-003`, and `MNT-R5-002` share one artifact-lifecycle
  root: active task/remediation records accumulated later implementation
  evidence without advancing their operative status/instructions and cited
  owners. TASK-001 remains `proposed`, still says to implement revision 10,
  and names deleted `remote_variant_error`; TASK-001-R4 remains `ready` under
  revision 11 and initially directs non-null children before a later revision-12
  resolution. Correct active lifecycle/instructions and the stale symbol, and
  clearly mark the revision-11 R4 diagnosis as superseded history rather than
  rewriting away what that review originally decided.

### 8. Unreviewed raw-Arrow numeric path

**Resolved: new material finding; specification decision required.**

`EncodedVariant::from_bytes` checks only size, depth, and structural Variant
validity. A raw Arrow Variant can therefore carry a valid scale-zero
`Decimal16` above `u64::MAX`, or another high-precision decimal, through
`validate_declared_variants` and Scribe admission. The shared JSON/native
renderer then calls upstream `VariantToJson::to_json_value`
(`variant.rs:246-251`). In `parquet-variant-json` 59.3, a `Decimal16` that
cannot narrow to `i64` or `u64` falls back to `f64`; fractional decimals may
also fall back to `f64`. The written Variant can therefore read back with a
different value, violating REQ-004's same-type/value rule.

This is reachable through the supported canonical Arrow extension path and is
shared lineage with `FIND-TASK-001-11`: revision 11 narrowed JSON-token input
to the exact `i64`/`u64` domain, but the raw encoded-Variant trust boundary was
left structurally valid rather than terminal-representable. The approved spec
simultaneously lists decimal as a Variant value and fixes typed Rust results to
`serde_json::Value`, which cannot represent arbitrary exact decimals without
the rejected `arbitrary_precision` feature. Existing authority does not decide
whether raw decimals outside the exact terminal domain must be refused, or
which new exact native representation they use. This proposal therefore
requires `SPEC_REVISION_REQUIRED`, not an invented admission rule.

## Proposed finding disposition

| Discovery proposal | Resolution | Root / correction boundary |
|---|---|---|
| `BVR-R5-BEH-001`, `INV-R5-001`, JSON half of `VARIANT-ARROW-R5-001` | Confirmed, consolidate | `EncodedVariant::from_json_text` / `append_raw`; complete `FIND-TASK-001-18` below the depth boundary. |
| `BVR-R5-BEH-002`, raw half of `VARIANT-ARROW-R5-001` | Confirmed, consolidate | `EncodedVariant::from_bytes`; preserve bounded safety while honoring encoding-before-depth. |
| `INV-R5-002`, `OTLP-TABLES-002`, new gateway present-value path | Revised into one finding | Existing `DomainTable::CANONICAL_VALIDATOR` seam for Results, Metrics, and Calls. |
| `OTLP-TABLES-001`, `PERSIST-R5-001`, `MNT-R5-001` | Confirmed, consolidate | `CallsTable` nested declarations; required request and optional result need different child nullability. |
| `INV-R5-003`, `STD-R5-002`, `MNT-R5-002` | Revised into one finding | Active task/remediation lifecycle and factual evidence; preserve historical diagnosis as superseded. |
| `STD-R5-001` | Confirmed | Remove the two unapproved `wyrd-implement` skill changes; retain allowed task-review edits. |
| `REUSE-R5-001` | Rejected | No duplicate exists in base/candidate; current `BatchBuilder` is the extended owner. |
| `REUSE-R5-002` | Rejected | Explicit human direction approved the single candidate implementation; sibling commit is not a second candidate owner. |
| Raw-Arrow high-precision numeric admission | New, confirmed reachability; spec revision needed | `EncodedVariant::from_bytes` to Scribe to `VariantToJson::to_json_value`; authority must choose accepted domain or exact terminal representation. |

## Verification limits

This was a source/root-cause pass. I did not rerun the expensive PostgreSQL,
object-store, cross-language, or fork suites. The r5 discovery reports record
focused green tests, but none covers numeric defects below the first over-depth
container, malformed bytes below it, partial present nullable Structs, nested
gateway child nullability parity, or raw high-precision Decimal admission and
terminal rendering.

## Follow-up result

**RESOLVED** — all assigned conflicts and the newly requested raw-numeric path
were traced to current source. The raw-decimal path is resolved as a real
authority gap requiring specification revision, not as an implementation
choice for this reviewer.
