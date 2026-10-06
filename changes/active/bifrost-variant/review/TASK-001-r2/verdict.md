# TASK-001 r2 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `99c5871ec5ca664b9f54baa379b437ee09d66e95`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Prior review: `changes/active/bifrost-variant/review/TASK-001-r1/`
- Remediation task: `changes/active/bifrost-variant/review/TASK-001-r1/TASK-001-R1-close-variant-contract-gaps.md`

The complete cumulative base-to-candidate diff was reviewed. The candidate
commit resolved to the same object before discovery, follow-up, validation,
and verdict preparation. The reviewed source was not modified.

## Binding decisions applied

- Iceberg v3 lineage relies only on field-ID projection, per-batch
  presence/type/null validation, and unchanged copy. Duplicate-ID scans and
  optional-metrics gates are drift, including when they are conditions of a
  required test.
- `serde_json/arbitrary_precision` is rejected because workspace feature
  unification breaks flatten/tagged deserialization. The approved input fix is
  local `RawValue` plus lexical `i128` classification in
  `EncodedVariant::from_json_text`.
- A mechanism, check, file, setting, or option absent both the established
  standard and comparable widely used projects is `DRIFT` and is not required
  as remediation.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| REQ-001, REQ-002, INV-003, INV-006, AC-002: v3-only storage, standard hidden-lineage preservation, repeated rewrite, and v3 GC | Production uses standard field-ID projection, per-batch presence/type/null validation, unchanged batch write, v3 publication, and v3 GC. V10 and V13 exercise repeated rewrites, but still impose rejected duplicate-ID checks and V13 also requires optional metric entries. | **FAIL — `FIND-TASK-001-3`** |
| REQ-003, REQ-004, REQ-019, INV-002: one Variant model, exact numeric meaning, limits, and stable input/query failures | Input classification correctly uses the approved local `RawValue` and `i128` path; focused queue and Oracle tests pass. Exact Decimal16 values beyond `u64` are later narrowed through `f64` by the shared non-Arrow result decoder. | **FAIL — `FIND-TASK-001-11`** |
| REQ-006–REQ-011, INV-001, INV-004, INV-005, AC-001, AC-003: built-in Variant/Struct layouts, producers, promotions, sensitivity, and typed querying | Table declarations, producer closure, recursive pre-ACK validation, OTLP projection, generated contracts, SDK journeys, and documentation agree. | PASS |
| REQ-017, REQ-019, INV-004, INV-006, AC-005, AC-008: one Oracle Variant SQL registry, Struct separation, authorization before IO, and stable errors | All production sessions use the shared owner; local and distributed first-failure paths preserve tagged errors. After a successful batch, the closed query-terminal contract collapses a Variant failure to generic execution failure. | **FAIL — `FIND-TASK-001-12`** |
| REQ-005, AC-009: Bloom capacity follows row-group geometry with unchanged FPP/folding | Wyrd's duplicate NDV constant/setters are removed; both recipes use parquet-rs native row-group default. Focused test passed. | PASS |
| Prior documentation/API/style remediation | Bifrost architecture/schema docs, structured remote error transport, `EncodedVariant` constructor surface, `QueryResult` JSDoc, Rust documentation, and import style close prior findings 4–10. | PASS |
| Security, tenancy, audit, and admission durability | Permission checks precede IO, tenant tripwires remain intact, audit hashes are preserved, and malformed built-in Variant batches are refused before WAL/ACK with typed errors. | PASS |
| Non-goals and drift boundary | No shredding policy, TASK-002 authoring, second Variant model/reader, DataFusion repin, migration, compatibility alias, new configuration, or replacement bespoke lineage mechanism entered production. | PASS except required proof drift in `FIND-TASK-001-3` |

## Independent review results

| Report | Result | Material proposal |
|---|---|---|
| `task-review-behavior.md` | PASS | None |
| `task-review-invariants.md` | FAIL | `INV-R2-001` |
| `standards-review.md` | PASS | None |
| `maintainer-review.md` | PASS | None |
| `system-review.md` | FAIL | `SYS-TASK-001-1` |
| `domain-review-variant-arrow.md` | FAIL | `VARIANT-ARROW-R2-001` |
| `domain-review-iceberg-durability.md` | PASS | None |
| `domain-review-oracle-datafusion.md` | PASS | None |
| `domain-review-security-tenancy.md` | PASS | None |
| `domain-review-sdk-parity.md` | FAIL | `SDK-001` |

Every required discovery report was present. The structured Ponytail validator
independently checked the reports that proposed no findings and retained no
additional issues from them.

## Follow-up decision

A focused follow-up was required because the invariant reviewer reported
nonstandard lineage assertions in V10/V13 while the behavior and Iceberg
reviewers reported the production mechanism compliant. `followup-review.md`
resolved the conflict:

- production Forge and the pinned executor use only the approved standard
  lineage mechanism;
- V10 and V13 nevertheless scan for global duplicate row IDs; and
- V13 additionally requires optional lineage metrics.

The conflict was resolved in favor of a narrowed proof-code finding. No new
finding arose in follow-up.

## Validated finding ledger

The complete validated ledger and decision boundaries are in
`findings-validation.md`.

| Finding | Status | Classification | Required outcome |
|---|---|---|---|
| `FIND-TASK-001-3` | REVISED | DRIFT | Delete duplicate-ID assertions from V10/V13 and optional-metrics assertions from V13. Compare hidden lineage by the fixtures' existing logical identities. Preserve standard production validation/copy and add no replacement mechanism. |
| `FIND-TASK-001-11` | REVISED | INCORRECT | Revise the public native-result contract to choose an exact representation for accepted Decimal16 values beyond 64-bit ranges across Rust, Python, TypeScript, MCP/HTTP, while retaining the approved input classification and rejected `arbitrary_precision` feature. |
| `FIND-TASK-001-12` | REVISED | INCORRECT | Revise the public late query-terminal contract so already-approved Variant catalog identities and details survive after framing begins for HTTP/gRPC and every SDK, without prose parsing or a side channel. |

`FIND-TASK-001-3` is bounded implementation remediation, but findings 11 and
12 require new public contract decisions. They cannot be packaged into an r2
implementation remediation under revision 10.

## Prior-finding closure

| Prior finding | Result |
|---|---|
| `FIND-TASK-001-1` | Closed for input classification by approved `RawValue` plus local lexical `i128`; new `FIND-TASK-001-11` is the distinct output-contract gap exposed by accepting that value class. |
| `FIND-TASK-001-2` | Closed by recursive built-in Variant validation before ACK/WAL with typed error propagation. |
| `FIND-TASK-001-3` | Not closed; narrowed to prohibited assertions in required V10/V13 proof code. |
| `FIND-TASK-001-4`–`FIND-TASK-001-10` | Closed. |

## Verification evidence and limits

This review directly observed:

- `git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..99c5871ec5ca664b9f54baa379b437ee09d66e95` — PASS;
- the two focused `wyrd-queue` exact-number/Variant tests — PASS;
- the focused Oracle exact-integer test — PASS;
- the focused Bloom row-group-capacity test — PASS;
- `mise run codegen:check` — PASS;
- Oracle/DataFusion focused semantics and error tests — 3 PASS;
- security/tenancy focused tests — 10 PASS; and
- pinned compaction per-batch lineage and repeated-rewrite tests — 2 PASS.

The candidate's immutable implementation record reports the complete original
V1–V17 task-local suite and the remediation format, lint, language, docs, and
boundary lanes as passing. Parallel reviewers did not rerun every
Postgres-backed multi-language journey. That is not the cause of this verdict:
the three retained findings are source-confirmed reachable gaps or prohibited
proof conditions, not missing verification evidence.

CodeGraph was unavailable because the repository has no `.codegraph/`
directory. Reviewers used the cumulative Git diff, repository search, direct
source/caller tracing, and the pinned dependency checkout.

## Verdict

**SPEC_REVISION_REQUIRED**

Revision 10 does not select a viable exact non-Arrow representation for
Decimal16 integers beyond `u64`, and its public late-stream error requirement
cannot be implemented through the current closed terminal contract without a
wire decision. Those decisions are expensive-to-reverse public behavior and
must be approved before implementation remediation is written. No remediation
task is emitted from this review.
