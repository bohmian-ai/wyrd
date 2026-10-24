# TASK-004 cumulative re-review verdict

**FIX_REQUIRED.** Nine independently validated findings remain: **FIND-TASK-004-1** and **FIND-TASK-004-8 through FIND-TASK-004-15**. Prior findings **2–7 are closed** for their original scope. No specification revision is required. Route [TASK-004-R3](TASK-004-R3-complete-version-boundaries.md) to `$wyrd-implement`.

## Immutable subject and authority

| Subject | Identity |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004` |
| Original cumulative base | `3e7c59db7a0f5e9506273eab826875cc8b59f226` |
| Prior reviewed candidate / remediation base | `c4dd61029a54c44017c63bd4ff469472c406bc13` |
| Current candidate | `f75ada758827266ac0b25a05d6a7b530f86d845a` |
| Complete subject | `git diff 3e7c59db7...f75ada758` |
| Remediation locator | `git diff c4dd61029...f75ada758` |

Approved [specification revision 13](../../spec.md) and its [storage supplement](../../supplements/table-storage-and-correlation.md) supersede the old composite/accepted-recipe mechanics. The [original task](../../tasks/TASK-004-shared-table-storage.md), prior [verdict](../TASK-004-c4dd61029/verdict.md), [validated ledger](../TASK-004-c4dd61029/findings-validation.md), [R1](../TASK-004-c4dd61029/TASK-004-R1-retained-storage-authority.md) and replacement [R2](../TASK-004-c4dd61029/TASK-004-R2-single-table-version.md) remain the cumulative acceptance and closure inputs. Root/nested rules and governing architecture retain stronger authority.

The explicitly supplied PR 206 merge is authorized integration context. Its schema/client/generated-declaration/dependency seams were considered without treating the merge as unauthorized drift or auditing unrelated task completion. Uncommitted benchmark and other-task authoring was excluded and left untouched. HEAD remained the candidate; review writes are reports only. No implementation, commit, merge, push or deployment was performed.

## Independent reports

| Role | Result | Report |
| --- | --- | --- |
| Behavior | FAIL | [task-review-behavior.md](task-review-behavior.md) |
| Invariants | FAIL | [task-review-invariants.md](task-review-invariants.md) |
| Repository standards | FAIL | [standards-review.md](standards-review.md) |
| Maintainer | FAIL | [maintainer-review.md](maintainer-review.md) |
| System resilience | FAIL | [system-review.md](system-review.md) |
| Persistent data | FAIL | [domain-review-persistent-data.md](domain-review-persistent-data.md) |
| Tenancy/security | PASS | [domain-review-tenancy-security.md](domain-review-tenancy-security.md) |
| Concurrency | FAIL | [domain-review-concurrency.md](domain-review-concurrency.md) |
| Focused native-publication follow-up | RESOLVED | [followup-review.md](followup-review.md) |
| Fresh independent validation | FIX_REQUIRED | [findings-validation.md](findings-validation.md) |

All required reports are complete. [Claim comparison](claim-comparison.md) records root-cause grouping and conflicting closure/matrix claims. The focused follow-up was necessary to distinguish R2's explicit native no-op obligation from unsupported raw property mutations and future TASK-005 orchestration. Validation independently checked every claim, including unique ones, against complete correction-boundary bodies, callers and pinned fork source; the orchestrator did not self-validate findings.

## Final finding ledger

Exact source, producer/consumer evidence, selected corrections, write sets and focused proof are preserved in [findings-validation.md](findings-validation.md) and carried into the self-contained R3 task.

| Stable ID | Disposition / classification | Required correction |
| --- | --- | --- |
| FIND-TASK-004-1 | REVISED / INCORRECT | Recover the original accepted definition for canonical retries with no explicit version claim, using the existing mirror/fence and exceptional failure path without new successful-ingest SQL. |
| FIND-TASK-004-8 | CONFIRMED / INCORRECT | Stop checking the selected native schema against the stale creation SQL fingerprint; retain structural and accepted-content validation. |
| FIND-TASK-004-9 | CONFIRMED / MISSING | Preserve the accepted version in durable promotion evidence and validate against that exact retained definition. |
| FIND-TASK-004-10 | CONFIRMED / MISSING | Carry the selected Forge output version through writer identity/path, publication revalidation and committed lineage. |
| FIND-TASK-004-11 | CONFIRMED / INCORRECT | Persist the resolved default file target in the created definition so deployment changes cannot reinterpret an existing version. |
| FIND-TASK-004-12 | CONFIRMED / INCORRECT | Project canonical wire strings to Python `int` and TypeScript `bigint` at existing SDK description boundaries. |
| FIND-TASK-004-13 | CONFIRMED / REGRESSION | Prevent an in-flight Python describe from overwriting a newer same-name selected declaration. |
| FIND-TASK-004-14 | CONFIRMED / VIOLATION | Document the new TableVersion parser error contract and two undocumented proof tests in the same remediation. |
| FIND-TASK-004-15 | REVISED / INCORRECT | Refuse an ineffective version-intended Bloom edit before native publication, consuming no version/reference. |

## Prior closure and reconciled acceptance

| Obligation | Reconciled result / evidence |
| --- | --- |
| Shared layout/type/default matrix, zoned time axis, complete typed tuples and canonical wire | PASS: existing resolver/transform/codec owners, focused matrix/create/refusal proof. |
| Native version representation, coherent historical schema/layout/Blooms and reference retention | PASS for Rust/wire, retained resolution and conservative metadata protection; SDK values and writer default completeness fail separately below. |
| Atomic native pointer/reference publication, CAS and uncertain commit handling | PASS for effective versioned edits; ineffective identical Bloom edit fails FIND-15. |
| Fresh ingest adds no SQL; grouped insertion/reconciliation preserves rollback/order/RLS/poisoning | PASS structurally from SQL owner and supplied operation-count/negative proof; no throughput qualification inferred. |
| Original-version native retry and canonical-coordinate WAL suppression | PASS for the explicit original-version native path and replay; no-claim canonical retry fails FIND-1. Changing a native version claim is not automatically the same retry. |
| Fresh admission under evolved selected schema | FAIL: creation SQL fingerprint contradicts the selected native definition, FIND-8. Historical field-ID reads still pass. |
| Version-bearing seals/WAL/assembly/staged restoration | PASS: retained native lookup removes prior current-Bloom reconstruction. |
| Request-wide 32/33 union before WAL | PASS; FIND-2 CLOSED, including multi-RecordBatch and repeated tuples. |
| Partition-scoped native compaction includes spec identity | PASS; FIND-3 CLOSED in actual pinned core. |
| Complete durable SQL/JSON tuple comparison before promotion | PASS; FIND-4 CLOSED at worker revalidation. Missing definition version is separate FIND-9. |
| Full-key live capture across restored ordinal reuse | PASS; FIND-5 CLOSED by full `SealKey` membership and focused proof. |
| Required prior rustdoc correction | PASS; FIND-6 CLOSED for named original omissions. New version parser/tests fail FIND-14. |
| Physical sort/Bloom output, exact Oracle equivalence and geometry/pruning/planning observations | PASS; FIND-7 CLOSED by new policy journey and supplied actual observations, without claiming measured throughput. |
| Exact accepting/output definition identity through promotion and Forge | FAIL: dropped promotion version and absent selected rewrite version/path/lineage, FIND-9/10. |
| Complete definition fixes mutable writer defaults | FAIL: omitted target falls back to deployment config, FIND-11. |
| Public SDK exact integer version values | FAIL: runtime/type projections return strings, FIND-12; generated wire strings remain correct. |
| Existing client binding/record/drain lifecycle | Record/drain and bound producer version behavior pass; Python stale-result overwrite fails FIND-13. Automatic synchronous resolution of unresolved declarations is not required. |
| Tenant/permission/SQL capability/footer/fence/pin/role ownership | PASS in reviewed source; preserve these boundaries in every correction. |
| Explicit non-goals and throughput deferral | PASS: no required new engine, copied recipe, cache broadcast, public PATCH, release migration, history rewrite, TTL, tuning or benchmark execution. Throughput remains UNMEASURED as permitted. |

Broad discovery PASS statements about SDK integer projection, promotion carriers or warm-before-preparation recovery are superseded by the validator's actual-source evidence. Native explicit-original-version success is not proof of canonical retry recovery. Physical/query evidence is accepted independently of absent throughput benchmarks.

## Rejected scope and correction proposals

No remediation is prescribed for automatic network resolution through synchronous versionless rebinding, a native spent ID with a deliberately different version claim, generic missing-bump property guards without a supported producer, table recreation without a producer, TASK-005 cache propagation, speculative retention TTLs, broad style preferences or forced benchmarks. No new async public API or deferred row queue is needed. No rejected claim is softened into optional advice. R2's one-version architecture is retained; copied R1 recipe transport is not reintroduced.

## Verification limits

R2 records native fork, library, SQL, Scribe/Forge/Oracle, Rust SDK/observe, Python/TypeScript and static checks at their stated commits. These supplied runs were assessed with the actual assertions and diagnoses. The caller reported `verify:bifrost` in progress on the merged branch; no completed final result was available to this review, so none is claimed. A green run cannot close missing source behavior or tests expecting an incorrect public type.

This review independently checked the cumulative diff's whitespace and immutable commit, but ran no Cargo build/test, database mutation, generator, fault experiment or benchmark. Pinned fork source was inspected at its exact revisions. Findings are source-validated reachable failures or explicit required seam/documentation failures, not measured regressions. R3 uses focused proof; broad integrated suites remain change-review work. No additional spec decision is required by the selected corrections.

The independent validator completed the bounded final-packet comprehension pass and found all nine diagnoses, existing-owner corrections, preserved invariants, proof mappings and exclusions consistent with its ledger. No explanation gap or new approval decision remained. Implementation, focused verification and the corrected immutable fork pin are outstanding; preparing a local fork patch alone cannot close FIND-15.
