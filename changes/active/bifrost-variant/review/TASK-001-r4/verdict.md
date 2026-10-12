# TASK-001 Review Verdict — Round 4

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Last product commit: `cb8efab2f`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Review directory: `changes/active/bifrost-variant/review/TASK-001-r4/`

The checked-out branch advanced after review began, but only outside the pinned
candidate. Every reviewer resolved and rechecked the explicit candidate object
and tree; the later branch tip was not treated as reviewed source.

## Independent review results

| Review | Result | Proposed findings |
|---|---|---|
| Behavior | FAIL | `BVR-R4-BEH-001` |
| Invariants | FAIL | `INV-R4-001`, `INV-R4-002` |
| Repository standards | FAIL | `STD-R4-001` |
| Maintainer | FAIL | `MNT-001` |
| System resilience | PASS | None |
| Reuse | FAIL | `REUSE-R4-001` |
| Variant/Arrow domain | FAIL | `VARIANT-ARROW-R4-001`–`003` |
| Oracle/DataFusion domain | PASS | None |
| Iceberg durability domain | PASS | None |
| Security/tenancy domain | FAIL | one proposal, rejected by validation |
| SDK/transport parity domain | FAIL | `SDK-PARITY-R4-001` |

All required discovery reports are present. The repeat-review root-cause
follow-up completed with `RESOLVED`, and the structured Ponytail validation is
present in `findings-validation.md`.

## Follow-up and root-cause decision

The mandatory follow-up resolved every discovery conflict against the pinned
source. After the change owner's approved scope exception, independent
validation retained one current shared-root group:

1. The three factual symptoms in `MNT-001` are one stale-evidence finding caused
   by not refreshing the original task record after the approved revision,
   compaction repin, and placeholder implementation changed.

The behavior and standards proposals about the mirrored task-review skills are
rejected because the change owner explicitly allows those edits in this
candidate.

The remaining current findings have distinct producers and correction sites.
The historical `FIND-TASK-001-5` and `FIND-TASK-001-12` shared distributed
error-envelope root is closed by the candidate's general `QueryCatalogError`
path.

Validation rejected the security proposal. The approved requirement is refusal
before provider/source IO; the candidate authorizes payload columns before
`TableProvider::scan`, physical source construction, follower execution, and
row IO. Requiring zero catalog materialization or route discovery would broaden
TASK-001 and require a second planning shape.

## Reconciled acceptance matrix

| Obligation | Reconciled evidence | Result |
|---|---|---|
| Iceberg v3 creation, standard hidden-lineage preservation, repeated rewrites, v3 GC, unchanged handoff, and native Bloom sizing | Candidate source and independently rerun pinned-fork/Bloom tests support the final pins `iceberg-rust@e999331f...` and `iceberg-compaction@2b65fa18...`. | PASS |
| Variant type, fingerprint, exact i64/u64 meaning, built-in persisted layouts, producers, native rendering, and late catalog-error identity | Shared owners and focused tests establish the primary contract, but raw depth validation, exact extension metadata, and compound numeric/depth precedence remain wrong. | FAIL — `FIND-TASK-001-16`, `-17`, `-18` |
| Built-in trust-boundary refusal before ACK/WAL with locked first-error order | Recursive validation exists for every built-in, but the predeclared path traverses Variant values before completing earlier schema checks. | FAIL — `FIND-TASK-001-15` |
| Nullable verification Struct meaning with exact DataFusion `get_field` | Present summaries are correct; absent summaries retain valid placeholder children that `get_field` can expose as fabricated values. | FAIL — `FIND-TASK-001-14` |
| Oracle registration, Struct/Variant semantic separation, sensitivity before provider/source IO, interactive/distributed terminals, and SDK partial-result discard | Source tracing and focused Oracle/transport tests pass. The broader security proposal was rejected as outside the approved boundary. | PASS |
| Rust, Python, TypeScript, MCP, HTTP, gRPC, and CLI JSON projection | Implementations share the Variant renderer and non-CLI journeys pass; the changed compiled CLI surface lacks its required Variant journey. | FAIL — `FIND-TASK-001-19` |
| One owner per conversion, validation, rendering, and error mechanism | Round-4 consolidation removed the material parallel paths, but public test-only `EncodedVariant::to_json` remains beside the production byte renderer. | FAIL — `FIND-TASK-001-20` |
| Active task evidence accurately describes the immutable candidate | The task record names revision 10, the obsolete compaction pin, and a nonexistent placeholder helper. | FAIL — `FIND-TASK-001-21` |
| Non-goals and ownership boundaries | No shredding/TASK-003 work, user-model inference, second reader/model, DataFusion repin, migration, compatibility alias, signing, or new configuration entered the product implementation. | PASS |

## Validated finding ledger

The independent source of truth is `findings-validation.md`.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-14` | REVISED | INCORRECT | Produce null child slots beneath absent verification Struct parents so every child projection remains SQL null. |
| `FIND-TASK-001-15` | CONFIRMED | INCORRECT | Complete predeclared schema checks before traversing Variant values, preserving the locked error order. |
| `FIND-TASK-001-16` | REVISED | VIOLATION | Enforce depth iteratively before upstream recursive full validation, then retain upstream as the validity authority. |
| `FIND-TASK-001-17` | CONFIRMED | INCORRECT | Require the exact canonical Variant extension name and empty metadata at every nesting level. |
| `FIND-TASK-001-18` | REVISED | INCORRECT | Make numeric-range outrank depth independently of object traversal order. |
| `FIND-TASK-001-19` | CONFIRMED | MISSING | Prove the compiled CLI renders native Variant JSON, exact `u64::MAX`, and `3.0` in its existing server journey. |
| `FIND-TASK-001-20` | CONFIRMED | VIOLATION / DRIFT | Delete the dead public `EncodedVariant::to_json` wrapper and reuse the byte renderer in its tests. |
| `FIND-TASK-001-21` | REVISED | VIOLATION | Correct the existing task evidence to revision 11, the final compaction pin, and the actual placeholder owner. |

No retained correction requires a new product, public API, architecture,
security, compatibility, concurrency, resource-ownership, or persistent-data
decision.

## Prior-finding closure

| Prior finding | Closure |
|---|---|
| `FIND-TASK-001-1`, `-11` | Closed: raw-token classification and revision-11 i64/u64 boundary preserve every accepted integer exactly. |
| `FIND-TASK-001-2` | Closed for its original missing-boundary defect: every built-in now validates Variant data before ACK/WAL. Findings 15–17 are independent follow-on ordering, recursion, and identity defects. |
| `FIND-TASK-001-3` | Closed: production and proof use standard Iceberg metadata-field projection and the final pinned fork. |
| `FIND-TASK-001-4` | Closed: active architecture and schema documentation state revision-11 behavior. |
| `FIND-TASK-001-5`, `-12` | Closed together by the general structured distributed catalog-error envelope and shared reconstruction. |
| `FIND-TASK-001-6`–`-10` | Closed. Finding 20 is a distinct output-helper duplication, not a recurrence of the construction-invariant finding. |

## Verification evidence and limits

- Candidate object/tree identity and cumulative `git diff --check` were
  repeatedly verified.
- Fresh focused queue, table-contract, Oracle, tonic, client, Bloom, Iceberg,
  and compaction tests passed across discovery reviews.
- The final compaction revision `2b65fa189f2d05002acc6e59515a071a63777970`
  independently passed its lineage tests; the tracked task record remains
  inaccurate and is finding 21.
- Repository boundary, generated-artifact, Python typing, TypeScript N-API,
  unwrap, clippy-allow, object-store-pin, proto-drift, and skill-sync checks
  reported green.
- The full V1–V17 environment-backed journey set was not rerun by the final
  validator. Its recorded evidence is historical and does not cover the eight
  source-proven gaps above.

## Verdict

**FIX_REQUIRED**

Eight bounded findings remain. Remediation is specified in
`TASK-001-R4-close-final-variant-contract-gaps.md` and routes directly to
`$wyrd-implement`.
