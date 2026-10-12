# TASK-001 r6 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Original base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- R5 record commit: `86e199d2b174117130d7245bf9f397d20ebc91e2`
- R5 implementation commit: `5230a70aba6b334a8204144a3826149f66c3dfc3`
- Latest correction commit: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- R5 remediation: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

The complete cumulative base-to-candidate range, every earlier TASK-001 review
and remediation artifact, the R5 implementation range, current source and
callers, pinned Variant dependencies, and recorded verification were reviewed.
The candidate commit and tree remained unchanged throughout review; the new r6
review directory is outside that immutable tree.

## Independent review results

| Review | Result | Material outcome |
|---|---|---|
| Behavior | FAIL | Deep valid JSON can be classified by serde before Wyrd, and combined oversized JSON can return numeric/depth before size. |
| Invariants | FAIL | Raw non-finite floating values can cross ACK outside the revision-13 numeric domain. |
| Repository standards | FAIL | The implemented R5 remediation remains marked `ready`. |
| Maintainer | FAIL | Active Bifrost authority and immediate validator rustdoc omit revision-12/13 rules. |
| System resilience | PASS | Ordinary failure isolation and recovery paths pass; the validator separately retained the reachable raw amplification gap. |
| Reuse | PASS | No parallel owner or avoidable duplicate mechanism was confirmed. |
| Variant / Arrow | FAIL | Shared ranges remain amplifying, raw renderers bypass bounded scanning, duplicate names pass, numeric families are incomplete, and serde recursion outruns Wyrd. |
| Scribe ingest / durability | PASS | The common validation seam is before fingerprint, WAL, and ACK; retained defects are predicates/proof inside that seam. |
| Persisted schema | FAIL | Runtime schema/validator source passes, but required gateway boundary and published-null proof is absent. |
| Oracle / DataFusion | FAIL | Duplicate raw object names can be acknowledged and collapse differently in lookup and JSON rendering. |
| Security / tenancy | FAIL | Same duplicate-name value-integrity defect; authorization and tenant isolation otherwise pass. |
| SDK / CLI / MCP | PASS | Accepted ordinary values and late errors remain consistent; raw renderer safety is retained under the Variant owner. |
| Iceberg / Parquet durability | PASS | V3, lineage, GC, Bloom, and ordinary persisted-schema behavior pass. |
| Mandatory root-cause follow-up | RESOLVED | Current proposals were traced across all prior findings and consolidated into five correction groups. |
| Structured Ponytail validation | FIX_REQUIRED | Eight stable findings retained; no specification decision is missing. |

All required reports are present in this directory. `findings-validation.md`
is the independently validated finding ledger; report agreement alone was not
used as proof.

## Reconciled acceptance matrix

| Obligation | Evidence and result |
|---|---|
| Iceberg v3, lineage-preserving Forge rewrites/GC, final fork pins, and Bloom sizing | **PASS.** Cumulative source, pinned-fork evidence, and the fresh durability review agree. |
| Canonical Variant extension, schema/fingerprint ownership, SQL registration, and ordinary rendering | **PASS**, subject to raw canonicality and bounded-renderer findings below. |
| Exact numeric meaning through every accepted terminal | **FAIL — `FIND-TASK-001-24`.** Raw Decimal4/8 and non-finite Float/Double are still admitted. |
| Unique stored object keys and no silent value loss | **FAIL — `FIND-TASK-001-26`.** Equal resolved names pass when metadata is unsorted. |
| Bounded raw Variant validation and rendering | **FAIL — `FIND-TASK-001-16`.** The node-count fence does not prevent shared-range amplification and renderers bypass the scanner. |
| JSON validity, numeric-before-depth, and stack-safe depth classification | **FAIL — `FIND-TASK-001-18`.** Recursive serde work can decide first. |
| Encoded-size precedence before numeric/depth | **FAIL — `FIND-TASK-001-25`.** JSON checks recorded numeric/depth before an already oversized built encoding. |
| Nullable built-in Structs accept only complete-present/null-absent values | **PASS in source; FAIL in required proof — `FIND-TASK-001-14`.** Gateway boundary and published-null evidence is missing. |
| Required `requested_model` and optional `resolved_model` retain distinct schemas | **PASS.** Separate declarations and source validators are correct. |
| Built-in refusal occurs before WAL/ACK and service remains usable | **PASS for exercised paths**, with omitted inputs and gateway proof tracked by findings 14, 16, 18, 24, 25, and 26. |
| Interactive/distributed late catalog identity and no partial collected results | **PASS.** The shared `QueryCatalogError` path remains intact. |
| Rust, Python, TypeScript, HTTP/gRPC, MCP, and CLI projection | **PASS for admitted canonical values**; finding 16 owns unsafe raw rendering before terminal projection. |
| Security, tenancy, and sensitive-column boundaries | **PASS**, aside from finding 26's authorized-value integrity ambiguity. |
| Active task packet and architecture truthfully describe the candidate | **FAIL — `FIND-TASK-001-4`, `FIND-TASK-001-21`.** Authority/rustdoc omit later rules and R5 is still `ready`. |
| Scope and reuse constraints | **PASS.** No unrelated dependency, public API, migration, shredding, or parallel implementation owner was accepted. |

## Validated finding ledger

| Finding | Status | Classification | Root correction |
|---|---|---|---|
| `FIND-TASK-001-4` | REOPENED / REVISED | MISSING / VIOLATION | Synchronize the existing Bifrost authority and validator rustdoc with revision-12/13 persisted rules. |
| `FIND-TASK-001-14` | REOPENED AS PROOF-ONLY | MISSING EVIDENCE | Add the required gateway peer refusal/continuation and published-null proof without changing production unless it falsifies the trace. |
| `FIND-TASK-001-16` | REOPENED / REVISED | INCORRECT / RESOURCE SAFETY | Reject overlapping/shared child regions and route every raw renderer through the same bounded borrowing validation. |
| `FIND-TASK-001-18` | REOPENED / CONSOLIDATED | INCORRECT | Prevent recursive serde work from outrunning Wyrd's depth/numeric decision for text and programmatic JSON. |
| `FIND-TASK-001-21` | REOPENED | VIOLATION | Move the implemented R5 remediation from `ready` to `review`. |
| `FIND-TASK-001-24` | REOPENED / CONSOLIDATED | INCORRECT | Reject raw Decimal4/8 and non-finite Float/Double through the existing numeric-domain selector. |
| `FIND-TASK-001-25` | NEW / CONFIRMED | INCORRECT | Check the actual built encoded size before selecting recorded numeric/depth failure. |
| `FIND-TASK-001-26` | NEW / CONSOLIDATED | INCORRECT / DATA INTEGRITY | Require strictly increasing resolved raw object names in both metadata modes. |

`FIND-TASK-001-13` remains omitted under the user's approved scope decision.
The complete evidence, reachability, consequences, and rejected alternatives
are in `findings-validation.md`.

## Validated root-cause groups

1. **JSON admission:** findings 18 and 25 share `EncodedVariant` ownership but
   are independent facts: recursive parsing bypasses Wyrd classification, and
   built encoded size is selected too late.
2. **Raw canonicality:** findings 24 and 26 are separate numeric and unique-key
   predicates in the existing raw scanner; both belong there rather than in
   downstream consumers.
3. **Raw resource safety:** finding 16 proves `ef92074f0` closes only the
   demonstrated exponential-node fixture; it does not establish non-overlap or
   protect renderers that bypass the scanner.
4. **Persisted gateway proof:** finding 14 is evidence-only; the common
   validator and producer are correct in source.
5. **Artifacts:** finding 21 is lifecycle state, while finding 4 is active
   authority and owner documentation; neither requires runtime machinery.

The cross-round mapping for every prior finding and remediation commit is in
`root-cause.md`.

## Prior-finding closure

- Findings 1-3, 5-13, 15, 17, 19, 20, 22, and 23 remain closed or rejected as
  recorded by r5.
- Findings 4, 14, 16, 18, 21, and 24 retain their IDs because this review
  proves incomplete closure at the same authority, evidence, raw-validation,
  JSON, lifecycle, and numeric roots.
- Findings 25 and 26 are the next new stable IDs.
- No current finding reopens Iceberg lineage, Bloom sizing, Forge behavior,
  distributed error transport, SDK collection, Scribe WAL/ACK ordering, RBAC,
  or tenancy.

## Verification evidence and limits

- The R5 record contains focused unit, Oracle, server, metrics, gateway,
  Rust/MCP/Python/TypeScript journey, format, lint, codegen, skill-sync, docs,
  and diff-check results; the final raw-alias commit reran its affected Variant,
  server, Oracle, lint, and diff checks.
- This independent review ran no Cargo, `mise`, codegen, database, cluster, or
  language-runtime lane so reviewers sharing the checkout would not overlap
  build work.
- Both cumulative and remediation-range `git diff --check` passed, and the
  candidate/tree identity was rechecked after validation.
- Existing evidence does not cover deep JSON before serde's guard, combined
  encoded-size failures, raw Decimal4/8 or non-finite floats, duplicate resolved
  names, shallow shared-range amplification through renderers, or the required
  gateway peer/publication proof.

## Verdict

**FIX_REQUIRED**

The eight retained findings are consolidated and decision-complete in
`TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`, routed directly to
`$wyrd-implement`.
