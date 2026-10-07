# TASK-001 r6 cross-round root-cause ledger

## Method

Each current proposal and every prior stable finding was traced from producer
to consumer against the cumulative candidate. A shared root is recorded only
when the same owner/invariant and one correction boundary close the grouped
symptoms. Similar subject matter alone is not enough. `Proof only` means source
supports the intended behavior but the approved task's required evidence is
absent.

## Current r6 proposals

| Finding / proposal | Prior linkage and remediation | Decision | Source evidence | Root correction and symptoms closed |
|---|---|---|---|---|
| `BVR-R6-BEH-001` | Follow-on to depth work in `FIND-TASK-001-18`; `5230a70ab` | **Shared root with `VARIANT-ARROW-R6-004`; independent of prior numeric-selection correction.** | `from_json_text` performs full `serde_json::from_str::<&RawValue>` before Wyrd's walker; `from_json` recursively serializes first. serde's 128-level guard can return invalid before the 64-level rule runs. | At the existing JSON `EncodedVariant` owner, decide Wyrd validity/numeric/depth without recursive serde work outrunning the public bound. Closes direct JSON, row preparation, `parse_json`, `try_parse_json`, and programmatic `Value` symptoms. |
| `VARIANT-ARROW-R6-004` | Same | **Duplicate/shared root with `BVR-R6-BEH-001`.** | Same producer and consumers; adds the programmatic `Value::to_string` stack path. | Same correction. |
| `BVR-R6-BEH-002` | Related to error-precedence work `FIND-TASK-001-16/-18`; `5230a70ab` | **Independent follow-on.** | `found.finish()` precedes `builder.finish()`/`sized`; rejected numeric nodes are replaced and over-depth subtrees omitted, so JSON cannot consistently select canonical encoded size first. | At `from_json_text`, determine the canonical encoded-size fact before the numeric/depth decision. Closes oversize+numeric and oversize+depth differences between JSON and raw Arrow. |
| `INV-R6-001` | Incomplete closure of `FIND-TASK-001-24`; `5230a70ab` | **Shared root with revised `VARIANT-ARROW-R6-003`.** | `scan_encoded` accepts non-finite Float/Double; JSON rejects them and all native/JSON renderers fail later. | Extend the existing raw numeric-domain match and shared precedence accumulator. Refuse before ACK with numeric kind `double`. |
| `VARIANT-ARROW-R6-003` | `FIND-TASK-001-11/-24`; spec revision 13 in `86e199d2b`; raw scan in `5230a70ab` | **Shared numeric-domain root; broader than `INV-R6-001`.** | The only raw numeric arm is disallowed `Decimal16`; Decimal4/8 and non-finite Float/Double fall through. JSON's canonical domain produces integer primitives, one unsigned Decimal16 form, or finite Double. | Extend `scan_encoded` rather than adding consumer guards: reject Decimal4/8 and non-finite Float/Double, retain finite Float/Double and the approved Decimal16 range. Closes all raw numeric encodings that cannot preserve the JSON-domain meaning across terminals. |
| `ODF-R6-001` | Related to unique-key portion of REQ-004; no prior stable finding | **Shared root with `SEC-R6-001` and `VARIANT-ARROW-R6-002`.** | In the unsorted-metadata branch, Wyrd and upstream use `<` instead of `<=`; equal adjacent resolved names pass, Oracle lookup selects one, and JSON map rendering overwrites one. | Require strict increase in `scan_encoded` for either metadata mode. Closes admission, lookup, predicate, and rendering ambiguity while retaining JSON/OTel last-wins normalization before encoding. |
| `SEC-R6-001` | Same | **Duplicate of `ODF-R6-001`.** | Same bytes, trust boundary, and consumer consequence; security framing adds no separate authorization defect. | Same correction. |
| `VARIANT-ARROW-R6-002` | Same | **Duplicate of `ODF-R6-001`.** | Same owner and predicate. | Same correction. |
| `VARIANT-ARROW-R6-001` | Direct follow-on to the r5 shared-object addendum and `ef92074f0`; related to `FIND-TASK-001-16`'s bounded raw validation | **Shared root with the addendum's crafted-byte symptom; independent of duplicate-name and numeric predicates. The crafted fix is insufficient.** | `visited > value.len()` rejects the demonstrated exponential-node chain but accepts repeated large child ranges while visits remain below bytes. Raw renderers and `mask_placeholders` bypass `scan_encoded` and invoke recursive upstream validation/rendering. | In the existing borrowing Variant validator, make accepted object child regions non-amplifying and apply that bounded validation to every raw-byte/Arrow renderer before recursive upstream conversion. Closes admission quadratic expansion, corrupt stored-cell recursion/amplification, and all Rust/Python/TypeScript/MCP/CLI/HTTP renderer symptoms. |
| `PERSIST-R6-001` | Incomplete proof closure of `FIND-TASK-001-14`; R5 expressly required gateway proof in `TASK-001-R5` | **Shared historical nullable-Struct root; proof only, not a confirmed runtime defect.** | `CallsTable::WHOLE_STRUCTS` reaches the common Scribe validator and producer unit proof is correct, but no existing real-server gateway path proves partial `resolved_model` refusal/no-row/continuation or published null children. | Add only the required existing-boundary gateway capture/peer journey and published child query. Closes the R5 evidence obligation; production remains unchanged unless that proof falsifies the source trace. |
| `STD-R6-001` | `FIND-TASK-001-21`; artifact changes in `86e199d2b`/`5230a70ab` | **Shared active-packet lifecycle root with prior FIND-21; independent of runtime.** | R5 is fully implemented and under r6 review but its front matter remains `ready`; TASK-001 correctly says `review`. | Change R5 status to `review`. Closes duplicate-work routing and restores active-packet truth. |
| `MNT-R6-001` | Historical shared root with `FIND-TASK-001-4`; later revision-12/13 behavior in `7c2c60f3e`/`5230a70ab` was not propagated | **Shared authority-synchronization root; independent of `STD-R6-001`.** | Active Bifrost authority omits raw Decimal16/numeric precedence and whole-present/null-absent persisted Struct rules; immediate validator rustdoc omits numeric-range errors. | Update the existing authority paragraphs and `validate_declared_variants` rustdoc. Closes the two authority omissions and the owner-doc omission without a new artifact or checker. |

## Prior stable findings

| Prior finding | Remediation / linkage | Root-cause decision and current closure |
|---|---|---|
| `FIND-TASK-001-1` | `45b61e80c`, then revision-11 range `e450046c6` | **Independent JSON-token precision root; closed.** Raw lexical integer classification removed the `f64` path. Current JSON recursion and raw numeric-family proposals have different producers. |
| `FIND-TASK-001-2` | `347747ee3`, `87b736e2e`, consolidation `dab737003` | **Missing universal built-in trust-boundary root; closed.** Every built-in now reaches the pre-ACK validator. Current raw canonicality findings are predicates inside that boundary, not a bypass of it. |
| `FIND-TASK-001-3` | `fb2415572`, `83954156c`, fork repins through `9a65d5469` | **Nonstandard Iceberg lineage-mechanism root; closed.** No current proposal reopens lineage or Bloom behavior. |
| `FIND-TASK-001-4` | `9e39241ac`, final revision-11 correction `79f60eec3` | **Authority-sync root; historically closed for revision 11, reopened by `MNT-R6-001` for later revision-12/13 contracts.** The correction site remains the existing active authority, plus the immediate owner rustdoc now cited. |
| `FIND-TASK-001-5` | `0181ff52a`, superseded by general envelope `929ce7771` | **Shared historical root with FIND-12; closed.** One general `QueryCatalogError` envelope replaced Variant-only prose recovery. |
| `FIND-TASK-001-6` | `963782b7d` | **Invalid `EncodedVariant` construction surface; closed.** Current findings concern incomplete validation predicates of the retained constructors, not the removed size-only public constructor. |
| `FIND-TASK-001-7` | `963782b7d` | **Detached TypeScript documentation root; closed.** |
| `FIND-TASK-001-8` | `86f570328` | **Duplicated Parquet Bloom NDV root; closed.** |
| `FIND-TASK-001-9` | `0d562a891`, `111e7bdce` | **Cumulative Rust documentation root; closed for those items.** `MNT-R6-001` is later semantic drift in one owner/authority, not missing documentation across the original diff. |
| `FIND-TASK-001-10` | `1612506e2`, final closure `b0acb1901` | **Import/declaration-style root; closed.** |
| `FIND-TASK-001-11` | revision 11 `93979daa1`, `e450046c6`, journeys `8a3e63168` | **Accepted numeric-domain/result-representation root; closed for JSON i64/u64.** It is historical lineage for FIND-24/current raw numeric proposals, but required a distinct raw-Arrow predicate. |
| `FIND-TASK-001-12` | terminal carrier `70142ba09` et al.; general peer envelope `929ce7771` | **Shared root with FIND-5; closed.** Current Oracle report passes error transport aside from the unrelated duplicate-key value defect. |
| `FIND-TASK-001-13` | Explicitly rejected/excepted scope proposal in r4/r5 records | **No stable implementation root.** Remains omitted; current review adds no workflow-scope finding beyond active packet status. |
| `FIND-TASK-001-14` | nullable-child repair `7c2c60f3e`; semantic validator `5230a70ab` | **Nullable persisted-child semantics root. Runtime correction is source-closed; gateway proof remains open as `PERSIST-R6-001`.** Results and metrics have boundary/publication proof; gateway has unit/source proof only. |
| `FIND-TASK-001-15` | `7c2c60f3e` | **Admission-phase ordering root; closed.** Complete schema checks precede Variant traversal. Current size ordering is inside JSON value conversion and independent. |
| `FIND-TASK-001-16` | `7c2c60f3e`, complete raw scan `5230a70ab`, shared-object addendum `ef92074f0` | **Raw stack-safety/complete-malformed-selection root partly closed.** Hostile depth and the crafted exponential-node fixture are closed; `VARIANT-ARROW-R6-001` shows the addendum did not establish a general non-amplifying invariant or protect renderers. |
| `FIND-TASK-001-17` | `7c2c60f3e` | **Exact extension identity root; closed.** |
| `FIND-TASK-001-18` | sibling-order repair `7c2c60f3e`; below-depth scan `5230a70ab` | **JSON numeric-before-depth root; closed for inputs reaching the walker.** `BVR-R6-BEH-001` is an upstream serde-recursion bypass and `BVR-R6-BEH-002` is the separately omitted size class. |
| `FIND-TASK-001-19` | r4 compiled CLI journey | **Missing CLI projection proof; closed.** Current renderer-safety proposal is a malformed/corrupt-cell resource invariant, not absence of accepted-value CLI coverage. |
| `FIND-TASK-001-20` | r4 deletion of `EncodedVariant::to_json` | **Duplicate rendering wrapper root; closed.** Routing renderers through one bounded validator does not require restoring that public wrapper. |
| `FIND-TASK-001-21` | task corrections in `86e199d2b`/`5230a70ab` | **Active-artifact lifecycle/factuality root not fully closed.** Facts and parent task status were corrected; `STD-R6-001` identifies the remaining R5 `ready` state. Independent from authority prose under MNT-R6-001. |
| `FIND-TASK-001-22` | separate gateway declarations `5230a70ab` | **Required/optional gateway schema-conflation root; closed.** PERSIST-R6-001 asks for evidence of the optional field's validator/publication path, not another declaration change. |
| `FIND-TASK-001-23` | unauthorized implementation-skill edits removed by `5230a70ab` | **Workflow-scope drift root; closed.** Approved task-review skill changes remain out of scope for this follow-up. |
| `FIND-TASK-001-24` | spec revision 13 `86e199d2b`; Decimal16 scan `5230a70ab` | **Raw numeric-domain root incompletely closed.** Approved Decimal16 is enforced, but `INV-R6-001`/`VARIANT-ARROW-R6-003` identify Decimal4/8 and non-finite Float/Double omitted from the same producer invariant. |

## Current correction groups

1. **JSON boundary:** recursion/depth classification and canonical encoded-size
   priority are two independent defects in `EncodedVariant::from_json_text`.
   They share an owner but need distinct facts; neither should be hidden inside
   consumer guards.
2. **Raw canonicality:** one `scan_encoded` correction covers the missing raw
   numeric families; another strict-name predicate covers duplicate object
   keys. They are separate invariants despite sharing the walker.
3. **Raw resource safety:** `ef92074f0` is an incomplete symptom fix. One
   borrowing bounded validator must reject amplifying object ranges and guard
   every raw renderer before upstream recursion.
4. **Persisted gateway proof:** production source currently passes; add only
   the explicitly required existing-boundary refusal/continuation/publication
   proof.
5. **Artifacts:** correct R5 lifecycle state and synchronize the active
   Bifrost authority/immediate rustdoc. These are independent, material
   artifact roots and require no runtime mechanism.

No current correction belongs in Iceberg, Forge, Bloom configuration, Oracle
error transport, SDK collectors, Scribe WAL/ACK ownership, RBAC, or tenancy.
