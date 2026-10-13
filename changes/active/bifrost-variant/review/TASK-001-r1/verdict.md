# TASK-001 Review Verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Candidate tree: `09c1495eb7d9e4033237e01bc7826b9cda7ac54e`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Review directory: `changes/active/bifrost-variant/review/TASK-001-r1/`

The candidate remained the checked-out `HEAD` throughout discovery, follow-up,
and independent validation. Review artifacts were written outside the candidate
commit and do not alter the reviewed source.

## Independent review results

| Review | Result | Proposed findings |
|---|---|---|
| Behavior | FAIL | `BVR-BEH-001`, `BVR-BEH-002`, `BVR-BEH-003` |
| Invariants | FAIL | `INV-REV-001` |
| Repository standards | FAIL | `REPO-001`, `REPO-002` |
| Maintainer | FAIL | `MR-001`, `MR-002`, `MR-003` |
| System resilience | PASS | None |
| Variant/Arrow domain | FAIL | `VARIANT-ARROW-001`, `VARIANT-ARROW-002` |
| Oracle/DataFusion domain | PASS | None |
| Iceberg durability domain | FAIL | `ICE-DUR-001` |
| Security/tenancy domain | PASS | None |
| SDK parity domain | PASS | None |

All required reports are present. No required reviewer or report was
unavailable.

## Follow-up decision

A focused follow-up was required because discovery reports materially
disagreed about two reachable paths:

1. whether distributed Variant error reconstruction was a necessary transport
   adaptation or a removable prose protocol; and
2. whether Forge's lineage metric gate and rewrite-wide row-ID uniqueness scan
   were required proof or nonstandard duplicate mechanisms.

`followup-review.md` resolved both conflicts from the actual dependency paths.
The existing string carrier can transport the existing serialized
`BifrostError` without parsing `Display`, and Iceberg lineage remains correct
through field-ID projection, validated copy, and before/after equality proof
without optional-metric or global uniqueness gates. No further follow-up was
needed.

## Reconciled acceptance matrix

| Obligation | Reconciled implementation and verification evidence | Result |
|---|---|---|
| Variant limits, numeric meaning, stable failures, and fingerprint | Core limits, extension type, fingerprint tag, and ordinary values are implemented and focused contract evidence passes. Large integral JSON tokens can still pass through `f64`, and not every built-in repeats Variant validation at the server trust boundary. | FAIL — `FIND-TASK-001-1`, `FIND-TASK-001-2` |
| Exact built-in Variant/Struct schemas and producer/consumer closure | Runtime schemas, producers, terminals, generated contracts, and cross-language journeys are present. The architecture authority and existing Bifrost schema guide still publish the replaced layouts. | FAIL — `FIND-TASK-001-4` |
| One Variant SQL registry in every Oracle session, Struct/Variant semantic separation, distributed error identity, and sensitivity before IO | Session registration, SQL semantics, full-root/residual behavior, and authorization evidence pass. Remote stable errors are recovered through a duplicate human-prose grammar. | FAIL — `FIND-TASK-001-5` |
| Iceberg v3 creation, hidden lineage preservation, safe repeated rewrites, v3 GC, and unchanged five-field handoff | v3 creation/refusal, first-row identity, repeated rewrites, no-commit failure, handoff, recovery, and GC evidence pass. Publication additionally requires optional metrics and the fork performs a nonstandard rewrite-wide uniqueness scan. | FAIL — `FIND-TASK-001-3` |
| Row-group Bloom capacity and existing FPP/folding behavior | The resolved behavior and focused test pass. Wyrd duplicates parquet-rs's native NDV derivation with a local constant and setter. | FAIL — `FIND-TASK-001-8` |
| Rust/Python/TypeScript/MCP result parity and public declarations | Native Variant decoding and generated declarations pass their checks. The TypeScript `QueryResult` JSDoc was detached from its public class. | FAIL — `FIND-TASK-001-7` |
| Minimal invariant-bearing API surface | Validated `EncodedVariant` constructors exist, but a public size-only constructor and dead emptiness branch expose an invalid state with no caller. | FAIL — `FIND-TASK-001-6` |
| Repository Rust documentation and module import rules | Formatting, lints, and boundary checks pass, but the cumulative diff still violates mandatory rustdoc and module-scope import/bare-signature rules. | FAIL — `FIND-TASK-001-9`, `FIND-TASK-001-10` |
| Non-goals and regression boundaries | No shredding policy, user-model inference, second reader/model, DataFusion repin, compatibility alias, migration, signing path, or unrelated public option was introduced. Security, tenancy, audit hashes, sensitivity, fixed trace IDs, and the Forge handoff otherwise remain preserved. | PASS |

## Validated finding ledger

The independently validated source of truth is `findings-validation.md`.

| Finding | Classification | Required outcome |
|---|---|---|
| `FIND-TASK-001-1` | INCORRECT | Preserve large integral JSON tokens exactly as supported integers/Decimal16 or return the stable numeric-range error. |
| `FIND-TASK-001-2` | INCORRECT | Validate extension identity and Variant bytes recursively for every TASK-001 built-in before admission, preserving typed errors. |
| `FIND-TASK-001-3` | DRIFT | Delete optional-metric lineage gating and the rewrite-wide row-ID collection/sort; retain standard projection/copy validation and equality proof. |
| `FIND-TASK-001-4` | MISSING | Update the Bifrost authority and existing schema guide to the shipped v3/Variant/query/built-in contract. |
| `FIND-TASK-001-5` | DRIFT | Carry the existing serialized `BifrostError` through the existing dependency string carrier and delete prose parsing. |
| `FIND-TASK-001-6` | DRIFT | Make the internal size-only Variant helper private and delete the unused emptiness branch. |
| `FIND-TASK-001-7` | REGRESSION | Reattach the existing JSDoc to the exported `QueryResult` class. |
| `FIND-TASK-001-8` | DRIFT | Delete Wyrd's Bloom NDV duplicate and rely on parquet-rs's native row-group default. |
| `FIND-TASK-001-9` | VIOLATION | Add required substantive rustdoc to every added or materially changed Rust item. |
| `FIND-TASK-001-10` | VIOLATION | Move ordinary imports to module scope and use imported bare names in signatures and bounds. |

No finding requires a new product, public API, architecture, security,
compatibility, cross-service, concurrency, resource-ownership, or
persistent-data decision. The bounded corrections therefore remain under the
approved specification.

## Verification evidence and limits

The task records all 17 task-local proofs as passing on the candidate.
Reviewers additionally ran focused Variant, Oracle, security, SDK, Forge, and
repository checks. The orchestrator independently passed:

- `git diff --check` for the cumulative candidate;
- `mise run codegen:check`;
- `tables::tests::variant_contract_and_builtin_schemas_are_stable`;
- `parquet::writer_properties::tests::bloom_capacity_uses_row_group_limit_for_scribe_and_forge`;
- the pinned iceberg-rust `variant_round_trips_unshredded` test at
  `e999331f280b698bcd026550812b5047e8789df6`; and
- the pinned compaction `rewrite_preserves_v3_row_lineage` test at
  `94db7b94f72c48c75c937d36a59e80c237e8ca72`.

Those green checks prove the paths they exercise but do not cover large
integral JSON tokens, malformed raw built-in Variant admission, prose-format
independence, standards-valid files without optional lineage metrics, stale
documentation, or the static repository-rule violations. No reviewer reported
missing source or an unavailable required dependency.

## Prior-finding closure

This is the first task review for this candidate. There are no prior stable
`FIND-*` identifiers to close.

## Verdict

**FIX_REQUIRED**

The independently validated ledger contains ten bounded implementation
findings. Remediation is specified in
`TASK-001-R1-close-variant-contract-gaps.md` and routes directly to
`$wyrd-implement`.
