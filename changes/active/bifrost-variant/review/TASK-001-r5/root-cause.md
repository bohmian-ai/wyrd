# TASK-001 r5 root-cause ledger

## Current-round proposals

| Finding | Related prior finding/remediation | Decision | Source evidence | Root correction site and symptoms closed |
|---|---|---|---|---|
| `BVR-R5-BEH-001` | `FIND-TASK-001-18`; `7c2c60f3e` | **Shared root.** | `append_raw` returns at the first depth-65 container and does not inspect its raw descendants (`variant.rs:628-634`). | Complete numeric-before-depth selection at `EncodedVariant::from_json_text`/`append_raw`; closes BVR-001, INV-001, and the JSON half of VARIANT-001 while preserving isolated depth errors. |
| `INV-R5-001` | `FIND-TASK-001-18`; `7c2c60f3e` | **Shared root with BVR-R5-BEH-001.** | Same producer and reachable JSON-row/Oracle consumers. | Same correction as BVR-R5-BEH-001. |
| `VARIANT-ARROW-R5-001` | `FIND-TASK-001-16`, `-18`; `7c2c60f3e` | **Two roots, not one finding.** | JSON tokens stop in `append_raw`; encoded bytes stop in `check_depth` before `Variant::try_new`. The producers and required corrections differ. | Split into the JSON correction above and the raw-byte correction below. |
| `BVR-R5-BEH-002` | `FIND-TASK-001-16`; `7c2c60f3e` | **Shared root.** | `from_bytes` propagates `check_depth` before full validity, and the walk returns at depth 65 (`variant.rs:184-196,766-790`), contrary to `bifrost-design.md:158-164`. | At `EncodedVariant::from_bytes`, use bounded non-recursive shallow validation that retains depth but selects malformed encoding first; closes BVR-002 and the raw half of VARIANT-001 without restoring recursive stack risk. |
| `INV-R5-002` | `FIND-TASK-001-14`; `7c2c60f3e`, `0e37748f3` | **Shared root with OTLP-TABLES-002 and the new gateway path; remediation-induced follow-on to FIND-14.** | Revision-12 nullable leaves preserve absent-parent nulls, but `ResultsTable` kept the generic validator, so a present summary may omit children. | Use the existing `DomainTable::CANONICAL_VALIDATOR` seam to enforce complete-present/null-absent state for Results, Metrics, and Calls. |
| `OTLP-TABLES-002` | `FIND-TASK-001-14`; `0e37748f3` | **Shared root with INV-R5-002.** | `validate_metric_points` checks metric-kind occupancy but not bucket child validity (`metrics/projection.rs:277-335`). | Same table-owned semantic-validation correction; extend Metrics' existing validator. |
| `FOLLOWUP-R5-001` — partial present `resolved_model` | `FIND-TASK-001-14`; `0e37748f3` | **Shared root with INV-R5-002.** | `resolved_model: Option<ModelRef>` has nullable physical children, while `CallsTable` retains generic validation. | Same semantic-validation seam; a present resolved model requires both children and an absent parent requires null children. |
| `OTLP-TABLES-001` | Human-directed r4 follow-on; `0e37748f3` | **Shared root with PERSIST-R5-001/MNT-R5-001; independent of semantic-validation omission.** | One `model_ref_type()` with nullable children serves required `requested_model` and optional `resolved_model` (`calls.rs:33-45,111-112`). | Split the two nested declarations in `CallsTable`; pin both layouts. This closes the schema weakening only; resolved-model semantic validation remains above. |
| `PERSIST-R5-001` | Same | **Duplicate of OTLP-TABLES-001.** | Same persisted declaration and typed wire contract. | Same `CallsTable` correction. |
| `MNT-R5-001` | Same | **Duplicate of OTLP-TABLES-001.** | Same helper hides the required/optional contract distinction. | Same `CallsTable` correction. |
| `INV-R5-003` | `FIND-TASK-001-21`; `7c2c60f3e` | **Shared artifact-lifecycle root with STD-R5-002/MNT-R5-002.** | TASK-001 evidence still names deleted `remote_variant_error`; current owner is `QueryCatalogError`. | Advance active task/remediation status/instructions and correct factual owner citations while retaining superseded history. |
| `STD-R5-002` | `FIND-TASK-001-21`; spec revision 12 + `7c2c60f3e`/`0e37748f3` | **Shared artifact-lifecycle root.** | Original task remains `proposed` and its operative approach still says revision 10 despite revision-12 implementation/evidence. | Same active-packet correction. |
| `MNT-R5-002` | `FIND-TASK-001-21`; r4 human revision-12 resolution | **Shared artifact-lifecycle root.** | R4 remediation remains `ready`/revision 11 and initially mandates non-null children, followed later by the opposite approved resolution. | Mark operative state and supersession clearly; do not erase the historical revision-11 diagnosis. |
| `STD-R5-001` | Commit `c8da11067`; r4 exception covered only task-review commits `a6f1326f6`/`a6429060f` | **Independent scope drift.** | Base-to-candidate diff changes global `wyrd-implement` policy and evidence requirements; no TASK-001 authority or approved exception covers it. | Remove only mirrored `wyrd-implement` changes from this task range; retain approved task-review edits. |
| `REUSE-R5-001` | `FIND-TASK-001-18`; `7c2c60f3e`; unintegrated TASK-002 commit `0fcb8a7ac` | **Rejected, no duplicate root in subject.** | Base `BatchBuilder` is the owner being extended. No equivalent base/candidate raw-field Variant path exists; sibling task code is not in the immutable subject. | No correction. Later integration may choose TASK-002's fuller owner. |
| `REUSE-R5-002` | Human-directed r4 resolution; `7c2c60f3e`; unintegrated TASK-002 commit `535367c94` | **Rejected by explicit scope and subject identity.** | Candidate has one implementation, copied exactly as human-directed; the sibling commit is not a second implementation in the candidate. | No correction. |
| `FOLLOWUP-R5-002` — raw high-precision decimals lose exactness | `FIND-TASK-001-11`; `e450046c6`, `8a3e63168`; admission work `347747ee3`, `4fa671a65`, `7c2c60f3e` | **Shared numeric-domain root with FIND-11; specification gap.** | `from_bytes` accepts any structurally valid Decimal16; Scribe calls it before ACK. `variant_bytes_to_json` delegates to upstream `to_json_value`, whose Decimal16 fallback narrows outside i64/u64 to f64. | **SPEC_REVISION_REQUIRED:** choose whether raw decimals outside the exact terminal domain are refused at `EncodedVariant::from_bytes` or receive a new exact native representation. No implementation correction is authorized yet. |

## Prior stable findings and remediation roots

| Prior finding | Remediation commits | Root-cause decision and current closure |
|---|---|---|
| `FIND-TASK-001-1` | `45b61e80c`, later range decision `e450046c6` | **Numeric input-domain lineage.** The initial parser converted large integer tokens through `serde_json::Value`; raw-token classification fixed that producer. Closed for JSON parsing, but related to FIND-11/FOLLOWUP-R5-002 through the accepted numeric domain. |
| `FIND-TASK-001-2` | `347747ee3`, `4fa671a65` | **Independent missing trust boundary.** Built-ins originally omitted recursive Variant checks before ACK. Closed. Later FIND-15/16/17 concern ordering, recursion safety, and identity inside the newly added boundary, not the same missing-boundary root. |
| `FIND-TASK-001-3` | `fb2415572`, `83954156c`, `bfd4ad34d`, `c2436fb08`, final pin `eccf249ad`/`9a65d5469` | **Lineage proof drift.** Optional metrics and rewrite-wide duplicate scans duplicated standard Iceberg evidence. Closed by deletion while retaining field-ID projection and per-batch validation. Independent of r5. |
| `FIND-TASK-001-4` | `9e39241ac`, `111e7bdce`, final authority correction `79f60eec3` | **Stale authority/documentation.** Closed for Bifrost behavior. It is distinct from the active task/remediation lifecycle root now reported by STD-R5-002/MNT-R5-002/INV-R5-003. |
| `FIND-TASK-001-5` | `0181ff52a`, superseded by general carrier `929ce7771` | **Shared distributed-error root with FIND-12.** Variant-specific prose reconstruction was the symptom; the general `QueryCatalogError` envelope closes both. Closed. |
| `FIND-TASK-001-6` | `963782b7d` | **Invalid construction surface.** Public size-only construction and dead emptiness exposed states the type promised to exclude. Closed; independent of current findings. |
| `FIND-TASK-001-7` | `963782b7d` | **Declaration placement regression.** Existing TypeScript JSDoc detached from `QueryResult`. Closed; independent. |
| `FIND-TASK-001-8` | `86f570328` | **Duplicate Bloom ownership.** Local NDV configuration duplicated parquet-rs folding. Closed by deletion; independent. |
| `FIND-TASK-001-9` | `0d562a891` | **Repository documentation-rule violation.** Missing rustdoc across changed items. Closed; independent. |
| `FIND-TASK-001-10` | `1612506e2`, `b0acb1901` | **Repository import/declaration-rule violation.** Closed after the second cumulative pass; independent. |
| `FIND-TASK-001-11` | Spec revision 11 `93979daa1`, `e450046c6`, journeys `8a3e63168` | **Accepted numeric domain versus terminal representability.** JSON inputs were narrowed to exact i64/u64 and that path closed. FOLLOWUP-R5-002 proves the same root remains reachable through raw Arrow Decimal values. |
| `FIND-TASK-001-12` | `70142ba09`, `ade4a37c2`, `7dd34bdcf`, `c5f090464`, generalized by `929ce7771`/`eaa8a5dbe` | **Shared distributed-error root with FIND-5.** Closed by one structured carrier for every catalog error. The stale `remote_variant_error` mention is artifact drift, not runtime recurrence. |
| `FIND-TASK-001-13` | — | No stable finding with this ID was issued. |
| `FIND-TASK-001-14` | `7c2c60f3e`, follow-on `0e37748f3`, spec revision 12 | **Nullable-parent physical/semantic split.** The original absent-parent fabricated-child symptom is closed by nullable persisted leaves and null-producing owners. INV-R5-002, OTLP-TABLES-002, and FOLLOWUP-R5-001 are remediation-induced siblings: the physical fix admits partial present states unless table validators preserve domain completeness. |
| `FIND-TASK-001-15` | `7c2c60f3e` | **Schema/value phase ordering.** Closed by completing predeclared shape checks before Variant traversal. Independent of current per-value precedence findings. |
| `FIND-TASK-001-16` | `7c2c60f3e` | **Raw validation safety/order.** The stack-exhaustion symptom is closed by preflighting depth before recursive upstream validation. BVR-R5-BEH-002 is the same root's remaining compound-error edge and must preserve that safety property. |
| `FIND-TASK-001-17` | `7c2c60f3e` | **Extension identity predicate.** Closed by requiring canonical name plus empty metadata at every nesting level. Independent. |
| `FIND-TASK-001-18` | `7c2c60f3e` | **JSON failure-selection root.** Sibling key-order dependence is closed; BVR-R5-BEH-001/INV-R5-001 prove the same producer still skips higher-priority numeric tokens below the depth boundary. Not fully closed. |
| `FIND-TASK-001-19` | `7c2c60f3e` | **Missing compiled CLI proof.** Closed by extending the existing journey with native object, exact u64, and `3.0` assertions. Independent. |
| `FIND-TASK-001-20` | `7c2c60f3e` | **Dead rendering wrapper.** Closed by deleting `EncodedVariant::to_json` and using the byte renderer directly. Independent. |
| `FIND-TASK-001-21` | `7c2c60f3e`, later revision-12 evidence `0e37748f3` | **Active artifact lifecycle/evidence root.** The named revision/pin/placeholder symptoms were partly corrected, but stale lifecycle state, operative revision text, deleted `remote_variant_error`, and contradictory R4 instructions remain. Shared current root with STD-R5-002/MNT-R5-002/INV-R5-003. |

## Root-cause result

**RESOLVED.** Current proposals reduce to six implementation/root groups, two
rejected reuse claims, and one new specification-level numeric-domain gap:

1. JSON numeric-before-depth below the boundary;
2. bounded raw encoding-before-depth;
3. table-owned completeness for nullable persisted Struct children;
4. separate required/optional gateway model declarations;
5. active-packet lifecycle/evidence correction;
6. removal of unapproved `wyrd-implement` policy edits; and
7. specification revision for raw high-precision Decimal acceptance versus
   exact terminal representation.
