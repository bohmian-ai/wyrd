# TASK-004 remediation re-review

**FIX_REQUIRED.** Independently retained findings: **FIND-TASK-004-1, FIND-TASK-004-14, FIND-TASK-004-16 and FIND-TASK-004-17**. Route the decision-complete [TASK-004-R4](TASK-004-R4-finish-admission-recovery.md) directly to `$wyrd-implement`. No specification revision is required.

Seven of the nine R3 finding causes are closed: **8–13 and 15**. FIND-1 is partly corrected and FIND-14 has a bounded documentation remainder. New FIND-16 concerns actual nulls in required scalar fields at the changed conformance boundary; FIND-17 concerns documentation displaced by the recovery insertion. Prior findings **2–7 remain closed**.

## Immutable subject and scope

| Identity | Commit / reference |
| --- | --- |
| Repository | `/home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-004` |
| Original cumulative base | `3e7c59db7a0f5e9506273eab826875cc8b59f226` |
| Prior reviewed candidate / R3 remediation base | `f75ada758827266ac0b25a05d6a7b530f86d845a` |
| Current immutable candidate | `5956466a1299a99521ed9264b2a711d3f1957e3e` |
| Remediation subject | `git diff f75ada758 5956466a1` |
| Cumulative acceptance context | Original base to current candidate, original task, prior verdicts/ledgers and R1/R2/R3 |

The candidate includes evidence commit `55cab2a19` and a subsequent merge whose delta is confined to review-workflow instructions/configuration. Other inherited merges and generated declaration seams were checked as integration context; unrelated task acceptance was not added to this round. Ancestry is retained, so carried-forward evidence needs no patch-correspondence reconstruction.

Approved [specification revision 13](../../spec.md), [storage supplement](../../supplements/table-storage-and-correlation.md), [original task](../../tasks/TASK-004-shared-table-storage.md), owning architecture and root/nested contributor rules govern acceptance. Neither approved specification nor supplement changed in this round. The one architecture edit aligns the file-target sentence with the existing creation-resolved-default obligation. Prior [verdict](../TASK-004-f75ada758/verdict.md), [validation](../TASK-004-f75ada758/findings-validation.md) and [R3](../TASK-004-f75ada758/TASK-004-R3-complete-version-boundaries.md) remain the closure context; R1/R2 remain available at their recorded paths and supersession boundaries.

Review scope was nine R3 closures, the latest diff and affected callers/consumers, not a repeated comprehensive audit. The required-scalar gap was examined because schema conformance is materially changed and required null validation is an explicit preserved obligation. It predates fingerprint removal and is not labeled a new regression or used to reopen the fixed stale-fingerprint cause. New buffer/group and documentation failures arise from the recovery changes themselves.

Uncommitted edits to tasks 002/003/005/006/007/008 were excluded and preserved. Review writes are confined to reports in this directory. The reviewed HEAD remained unchanged. No implementation, source mutation, staging, commit, merge, push or deployment was performed.

## Review selection and reports

| Selected role | Scope / reason | Result / report |
| --- | --- | --- |
| Fresh remediation Ponytail discovery | All nine closures, changed owners/callers/tests, Rust/Python/TS standards and changed-item documentation | FAIL — [task-review-behavior.md](task-review-behavior.md) |
| System/recovery specialist | Material no-claim retry, grouped fence/completion, cancellation and source-memory ownership changes | FAIL — [system-review.md](system-review.md) |
| Persistent-data specialist | Material promotion evidence, Forge selected output/lineage/cleanup and native fork publication changes | PASS — [domain-review-persistent-data.md](domain-review-persistent-data.md) |
| Focused documentation follow-up | Resolve explicit test/Panics rule versus R3's no-imagined-panics wording; displaced helper documentation | RESOLVED — [followup-review.md](followup-review.md) |
| Fresh independent Ponytail validation | Source, materiality, correction burden, all closures and proposed findings, including the empty persistent-data proposal ledger | FIX_REQUIRED — [findings-validation.md](findings-validation.md) |

All selected reports are complete. Every discovery reviewer received the same [immutable subject/navigation map](subject.md), with its own assigned scope; the first round's team was not recreated. Established authorization/tenant boundaries did not receive a separate specialist, while changed capability/RLS preservation remained in the selected reviews. [Claim comparison](claim-comparison.md) groups shared causes and records the follow-up trigger. The follow-up resolved a concrete authority uncertainty; no extra discovery pass was used merely to seek agreement. The validator independently reconciled it against source and rules. The orchestrator packages only the validated ledger.

## Validated finding ledger

| Stable ID | Classification / status | Material diagnosis and minimum correction |
| --- | --- | --- |
| FIND-TASK-004-1 | INCORRECT / REVISED | Cold no-claim retries still fail before recovery when current partition planning refuses; unresolved changed-version groups commit fresh peers before equality; retained canonical source buffers outlive their charge. Complete exceptional lookup, provisional group rollback/comparison and charged source ownership at existing ingress/shard/lease owners. |
| FIND-TASK-004-14 | VIOLATION / REVISED | Two exact version proof tests still lack mandatory Panics sections for their concrete failed invariants. Add two short sections, preserving parser behavior/assertions. |
| FIND-TASK-004-16 | INCORRECT / REVISED | Native input can mark a registered required nonpartition scalar nullable and store an actual null. Carry registered requiredness into the existing conformed field and use Arrow's existing batch validation; preserve harmless nullable source flags with non-null values. |
| FIND-TASK-004-17 | VIOLATION / REVISED | The inserted recovery enum captured existing helper docs, leaving `table_version_conflict` undocumented. Restore attached helper documentation, keeping the enum's own outcome description. |

The independent [ledger](findings-validation.md) records discovery IDs, exact source, producer-to-consumer traces, obligations, consequences, correction burden and focused closure proof. [R4](TASK-004-R4-finish-admission-recovery.md) carries those diagnoses and selected existing-owner corrections forward without requiring an implementer to choose a new architecture. No rejected proposal is softened into optional work.

## Reconciled acceptance and prior closure

| Obligation | Result / source-backed acceptance |
| --- | --- |
| Warm original-version canonical retry; cold Bloom-only recovery | PASS for these subpaths: mirror selects accepting version; cold hint re-prepares and compares retained input. They do not prove the refusal paths below. |
| Cold canonical recovery after a current-contract preparation refusal | FAIL, FIND-1: supported 33-scope grouping edit rejects before accepted-ID acquisition. |
| Grouped fence order, contradiction rollback, unchanged normal SQL and uncertainty semantics | SQL operation-count structure remains intact; rollback FAILS for different-version unresolved conflicts, FIND-1. Same-version rollback proof does not cover the changed classifier. |
| Retained recovery input remains governed through preparation/completion | FAIL, FIND-1: multipart prepared allocations replace the source lease charge while retry shadow retains source buffers. Sharing buffers does not imply copying and does not eliminate retained ownership. |
| Native original-version retry, strict explicit changed claims and canonical-coordinate replay | PASS for existing accepted-version/refusal/replay semantics; strict refusal must occur before committing fresh peers. No requirement that a changed native claim succeed. |
| Selected native schema replaces stale creation SQL fingerprint | PASS; FIND-8 closed. Extended mixed-schema proof writes new names/types after edit and preserves old-version content conflicts. |
| Required nullability and structural/type/reserved/Variant checks | Names, casts, reserved and Variant validation retained; actual required scalar nulls FAIL, FIND-16. No broad nullable-flag refusal is required. |
| Version-bearing seals, WAL, staging and exact promotion evidence | PASS; new accepting version reaches durable promotion JSON and exact retained-definition/footer/tuple validation, FIND-9 closed. |
| Forge output version, decimal paths, retry assumptions, lineage and cleanup | PASS; selected policy feeds existing native writer via in-memory output view; publication carries selected version and revalidates retained assumptions through Bloom-only conflict; no additional output generation, FIND-10 closed. |
| Committed writer defaults do not follow later deployment changes | PASS; target is stored at creation and policy extraction uses it; A/B definition/creation proof, FIND-11 closed. |
| Exact Python int / TS bigint SDK descriptions and unchanged string wire | PASS, FIND-12 closed; direct conversion, authored types and language-owned boundary proofs include values above 2^53 and i64 maximum. |
| Python same-name binding replacement during describe | PASS, FIND-13 closed; complete selected-state equality under existing lock, IO outside lock/GIL, real threaded replacement/write/read proof. |
| Effective native no-op publication and immutable dependency pin | PASS, FIND-15 closed. Exact pinned fork refuses an ineffective version-intended edit before metadata publication; pointer/reference atomicity and no-version housekeeping unchanged. |
| Request-union 32/33 fanout; native spec-qualified grouping; independent SQL tuple validation; full-key capture | PASS; findings 2–5 carried forward with impacted seams checked. Compaction fork changes only dependency resolution, preserving native grouping. |
| Prior documentation and physical/Oracle policy-equivalence proof | Original findings 6–7 remain closed; new/existing-item documentation remainder is confined to FIND-14/17. Oracle observations are not throughput measurements. |
| Existing ownership, tenant/RLS, checksums, pins and retained definitions | PASS outside the precisely identified source-lease/group failures; preserve these explicit boundaries in R4. |
| Mandatory changed-item rustdoc | FAIL only for the named FIND-14/17 remainder; batch with behavior, no style-only round. |
| Throughput constraints and non-goals | No new successful-fresh-ingest SQL or payload copy/hash established. Buffer lifetime/accounting fails separately. Throughput remains UNMEASURED as allowed; no benchmark was requested or run. |

## Convergence assessment

This is the second unsuccessful remediation review. Seven R3 causes now close on source and supplied proof; earlier findings 2–7 remain closed. The original retry cause is incompletely addressed where current preparation fails. The same recovery change introduced group-decision ordering and retained-memory accounting regressions. Required scalar nulls are a missed original obligation within the edited conformance authority; this is the only narrow behavioral scope expansion. Documentation consists of the original two-item gate remainder and one insertion-caused omission.

The causes are consolidated into existing recovery/group/source ownership and conformance boundaries, with the small prose corrections batched. Another broad audit, new public API, copied recipe, cache/ledger, generic hardening or benchmark demand is not justified. The next review must check these four IDs and the fix's affected paths, retaining accepted promotion, Forge, defaults, SDK and native-publication evidence. Existing authority already fixes all required outcomes, so remediation is not blocked on a spec revision or new approval decision.

## Verification limits

The R3 evidence at `55cab2a19` supplies focused Postgres journeys, Redux library 855/855, Forge integrations 47/47, language integrations and static checks at their recorded commits. Assertions, modified test diagnoses, exact fork pins and relevant source were inspected. Green Bloom-only or same-version tests cannot close different preparation/group failures. The supplied initial two aggregate timeouts have no established cause; later passes are useful evidence but do not diagnose them. No new finding is inferred merely from unnamed unreproduced timeouts, and no test timeout/skip change is prescribed.

Reviewers inspected exact iceberg-rust `850d308a8c50cdd7890ecfdf5bfd9825efebb241` and compaction `c213af00eac40377d800e24f3ed68945034e7de4` source read-only. Manifest/lock agree. This review independently checked immutable HEAD and `git diff --check`; it ran no Cargo build/test, database mutation, generator, runtime fault experiment or benchmark. Findings are independently source-validated supported failures or explicit mandatory gates. Focused runtime closure remains implementation work, and no throughput equivalence is claimed.

The independent validator also checked the completed verdict and R4 against its ledger. One proof sentence was narrowed to distinguish changed canonical content, which derives a new ID, from deliberate native reused-ID contradictions. With that correction applied, it found the packet faithful and ready for a fresh implementer, with no further scope, explanation or decision gap.
