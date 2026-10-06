# TASK-001 r6 focused follow-up review

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior rounds: every r1-r5 verdict, validated ledger, root-cause ledger where
  present, and remediation task
- Remediation commits inspected: the complete sequence after the first
  candidate, with particular attention to `86e199d2b`, `5230a70ab`, and
  `ef92074f0`

The candidate and tree matched before and after this pass. CodeGraph was used
first, then the current owners, callers, tests, pinned Variant dependency, and
all r6 discovery reports were inspected. No Cargo or mise command was run.

## Conflicts resolved

| Conflict or proposal | Source path inspected | Resolution |
|---|---|---|
| `BVR-R6-BEH-001` versus the r5 claim that JSON depth is bounded by Wyrd's walker | `EncodedVariant::from_json_text` parses the complete document as `&RawValue` before `append_raw`; serde_json 1.0.150 retains its default recursion limit. `from_json` also calls recursive `Value::to_string()` before that owner. | **Confirmed and merged with `VARIANT-ARROW-R6-004`.** Valid JSON beyond serde's unrelated ceiling becomes root `InvalidJson`, and a deeply constructed `Value` can recurse before Wyrd's depth decision. The correction belongs before recursive serde work in the existing JSON owner; disabling serde's guard alone is insufficient. |
| `BVR-R6-BEH-002` versus reports that the locked size-first order passes | `from_json_text` calls `found.finish()` before `builder.finish()` and `sized`; rejected numeric nodes become null and over-depth subtrees are not built. `from_bytes` checks size first. | **Confirmed, independent.** Raw bytes follow the locked order; JSON does not. Merely swapping the last two calls covers a large accepted sibling but cannot measure canonical encoded size contributed by a skipped rejected subtree. The JSON owner must determine canonical encoded size before selecting numeric/depth without admitting a partial value. |
| `INV-R6-001` versus `VARIANT-ARROW-R6-003` | `scan_encoded` has only a guarded `Decimal16` arm; `Decimal4`, `Decimal8`, `Float`, and `Double` fall through. `raw_number_variant` admits only finite doubles, while revision 13 gives raw input the same JSON numeric domain. | **`INV-R6-001` is a confirmed subset of revised `VARIANT-ARROW-R6-003`.** Non-finite Float/Double are unquestionably outside the JSON domain and fail every JSON/native renderer. Decimal4/8 are also non-canonical raw decimal encodings: JSON produces integer primitives, the one approved unsigned Decimal16 form, or finite Double, never Decimal4/8. One extension of the existing raw numeric match closes the group; no renderer guard belongs here. |
| `ODF-R6-001` and `SEC-R6-001` | For unsorted metadata, both Wyrd's `scan_encoded` and pinned `parquet-variant` compare `name < previous`, not `<=`. Oracle lookup and JSON map rendering collapse two admitted equal names differently. | **Same confirmed finding.** Require strictly increasing resolved object names for either metadata mode in `scan_encoded`. JSON/OTel last-occurrence normalization remains separate because it produces one unique stored key. |
| `VARIANT-ARROW-R6-001` versus system, reuse, SDK, Scribe, and Iceberg passes, and the r5 shared-object addendum | `ef92074f0` rejects only after visited nodes exceed `value.len()`. A shallow object may point many fields at one large child, remain below that count, and expand output quadratically. `variant_bytes_to_json`, `variant_cell_to_json`, `VariantJsonEncoderFactory`, and `mask_placeholders` invoke upstream recursive validation/rendering without `scan_encoded`. | **Confirmed; the crafted shared-byte fix is not sufficient.** It closes the demonstrated exponential-node admission fixture, not shared-range amplification generally, and it does not protect stored/query renderers. The root is the absence of one borrowing, bounded canonical validation gate for every raw-byte consumer. Correct the existing Variant owner so accepted object value regions cannot be repeatedly expanded and route raw renderers through the same validation before upstream recursion. This does not contradict Scribe/system pass conclusions about WAL ordering and the exact fixed fixture. |
| `PERSIST-R6-001` versus Scribe and durability passes | Source shows `CallsTable::WHOLE_STRUCTS` reaches the common validator and the producer emits correct null children. The r5 task nevertheless explicitly required a gateway server-boundary refusal and published-null proof; only direct validator and producer unit tests exist. | **Confirmed as a material proof-only gap, not a production defect.** Keep production unchanged. Close it at the existing server-owned gateway capture/peer boundary and published query journey. The Scribe pass correctly establishes reachability by source; it does not satisfy the task's explicitly required evidence. |
| `STD-R6-001` versus the r5 implementation record | TASK-001 is `review`; TASK-001-R5 still says `ready` while containing completed implementation evidence and final verification. | **Confirmed as a material active-packet defect only.** Change the existing R5 front matter to `review`; no runtime correction or new check is warranted. This is the still-open artifact-lifecycle root previously tracked as `FIND-TASK-001-21`. |
| `MNT-R6-001` versus standards/reuse passes | `architecture/bifrost-design.md` omits revision-13 raw numeric admission and the complete malformed/numeric/depth order, and omits the revision-12 physical-nullability/all-or-none rule for Results, metric buckets, and gateway `resolved_model`. `validate_declared_variants` rustdoc omits numeric-range errors. | **Confirmed as a material authority/documentation defect.** It is a recurrence of the historical `FIND-TASK-001-4` authority-sync root after later revisions added contracts. Update the existing authority and immediate owner rustdoc only. It is independent of `STD-R6-001`'s lifecycle state. |

## Materiality and root boundaries

- Runtime findings are material when a reachable accepted input can receive the
  wrong stable error, cross ACK outside the approved value domain, silently
  lose a field, or drive unbounded result processing. Green sibling-domain
  reports do not conflict where they reviewed storage ordering, transport, or
  ordinary accepted values rather than these omitted inputs.
- `PERSIST-R6-001` is material because the approved R5 remediation explicitly
  required that cross-boundary proof. It must remain labeled evidence-only;
  source currently supports the intended runtime behavior.
- `STD-R6-001` and `MNT-R6-001` are material repository artifacts under the
  normative task lifecycle and architecture-authority rules. Neither implies a
  new runtime bug.
- No current issue reopens Iceberg lineage, Bloom sizing, distributed catalog
  error identity, SDK collection, Scribe ACK/WAL ordering, tenant isolation, or
  authorization. Their pass reports are compatible with the retained roots.

## New proposed findings from this follow-up

None beyond the discovery union. This pass revises and deduplicates that union:

1. `BVR-R6-BEH-001` + `VARIANT-ARROW-R6-004` — one JSON recursion/depth root.
2. `BVR-R6-BEH-002` — independent JSON encoded-size precedence root.
3. `INV-R6-001` + `VARIANT-ARROW-R6-003` — one incomplete raw numeric-domain root.
4. `ODF-R6-001` + `SEC-R6-001` + `VARIANT-ARROW-R6-002` — one raw duplicate-name root.
5. `VARIANT-ARROW-R6-001` — incomplete shared-byte and renderer validation root.
6. `PERSIST-R6-001` — gateway evidence-only root.
7. `STD-R6-001` — active-packet lifecycle root.
8. `MNT-R6-001` — architecture/immediate-rustdoc synchronization root.

## Result

**RESOLVED**

Every r6 disagreement is narrowed to a concrete producer, consumer, correction
site, and materiality class. The complete cross-round mapping is in
`root-cause.md`.
