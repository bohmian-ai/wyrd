# TASK-008 and TASK-007-R1 cumulative verdict

**FIX_REQUIRED** — bounded repository compliance and evidence corrections remain. No executable acceptance defect or excess tenant machinery was established.

## Immutable subject and authority

- Candidate: `6e7add054e33701ca5ecb52a5c859948b15161a3`, detached in this review worktree.
- Immediate base: `f7bebf704d6f3b1dd20d041e70c6ca512c0da307`; `git diff HEAD~1` is TASK-008 + TASK-007-R1 + specification corrections.
- Original TASK-007 cumulative base: `a7582db587c6170a290760f1741673125612b797`. Original TASK-007-owned changes were assessed cumulatively, including its prior verdict and R1. Unrelated TASK-006 benchmark/process harness and lifecycle work remains excluded as established by the prior review.
- Approved authority: [spec revision 20](../../spec.md), REQ-014/015, AC-016/017 and applicable invariants; [TASK-008](../../tasks/TASK-008-tenant-proven-per-file.md), [original TASK-007](../../tasks/TASK-007-one-parquet-scan-for-live-reads.md), [TASK-007-R1](../task-007-review/TASK-007-R1-close-live-scan-gaps.md), [prior verdict](../task-007-review/verdict.md), repository rules and routed authorities documented in reports.

Current user decisions govern: FIND-007-3 is accepted unchanged, Postgres tenant columns are excluded, and the approved error is `WYRD_VALA_500_QUERY_TENANT_INVARIANT`. No disposed concern is re-raised. No CodeGraph index exists. HEAD and tracked source remained unchanged; only review/build/state artifacts were written. No commits.

## Independent review results

Each role used a separate fresh agent. All required reports are present; discovery reviewers were not given one another's conclusions.

| Role | Report | Result |
|---|---|---|
| Behavior | [task-review-behavior.md](task-review-behavior.md) | FAIL, evidence proposal |
| Invariants | [task-review-invariants.md](task-review-invariants.md) | FAIL, declaration proposal |
| Repository standards | [standards-review.md](standards-review.md) | FAIL, declarations and documentation |
| Maintainer | [maintainer-review.md](maintainer-review.md) | FAIL, declaration proposal |
| System resilience | [system-review.md](system-review.md) | PASS |
| Tenancy/security | [domain-review-tenancy.md](domain-review-tenancy.md) | PASS |
| Persistent analytical data/durability | [domain-review-data.md](domain-review-data.md) | PASS |
| Concurrency/native stream lifecycle | [domain-review-concurrency.md](domain-review-concurrency.md) | PASS |
| Focused per-open proof follow-up | [followup-review.md](followup-review.md) | RESOLVED, no finding |
| Fresh structured Ponytail validation | [findings-validation.md](findings-validation.md) | FIX_REQUIRED |

[Claim comparison](claim-comparison.md) groups proposals by governing obligation rather than source line. One fresh follow-up resolved the partitioned-file ambiguity: every reader opening performs one shared footer comparison before decode. A stricter coordinated once-per-logical-filename cache is not required. No other source or authority conflict remains unresolved.

## Reconciled acceptance matrix

| Obligation | Implementation and proof assessment | Result |
|---|---|---|
| Delete tenant row column from Bifrost envelope, write recipes, schemas and scan closures | Spec/redux schema and table definitions, writer stamping deletion, OracleScanProjection; source/schema tests and journey closure assertions | PASS |
| Authenticated tenant footer on staged, assembled, published and Forge rewritten files | Existing footer identity, seal/binding checks, artifact writer and Forge writer properties; output-footer assertions and recorded journeys | PASS |
| Missing/foreign footer refuses before file rows; shared published/hot/staged scan; no compatibility fallback | Mandatory published loader and hot/staged reader metadata adapter use shared proof before decoding; cache hits checked; encrypted bypass refused; focused tests passed | PASS |
| Delete row filter/tripwire/codec; memory remains seal-bound | Deleted provider/tripwire machinery and dead collaborator plumbing; existing signed memory source retained | PASS |
| Update architecture and user docs | Bifrost architecture, routed references and seven Bifrost guides remove the row column; recorded docs gate | PASS within verification limits |
| Shared staged scan/pruning, no custom decoder/per-batch compilation | Existing HotParquetExec, metadata cache and signed conjunction; staged pruning/lease source proof | PASS |
| Session partition counts on empty/sparse/mixed memory and staged sources | Exactly N memory groups plus N staged partitions; new actual-source/exact-once test independently passed | PASS |
| Native Arrow without IPC/hash, counted/fingerprinted completion, remote unchanged | Existing Scribe producer tally and leader decoder reconcile native counts/fingerprint; both focused completion tests independently passed | PASS |
| Scribe-local authority, staged leases, write/publication/recovery behavior | Source/domain/system traces preserve owners and request-scoped failures; FIND-007-3 disposition respected | PASS |
| Mandatory top-level imports and bare declaration types | New tenant/reader/test writer declarations violate the same rule despite old sites being corrected | FAIL, FIND-007-4 |
| Reproducible named proof and truthful attribution | Old recipes improved; TASK-008 named writer/journey recipes and new R1 proof attribution remain incomplete | FAIL, FIND-007-5 |
| Changed-test panic contracts | Original two sites corrected; newly changed schema envelope proof omits its panic contract | FAIL, FIND-007-6 |
| Minimal authorized change, no unrelated architecture or tenant mechanism | One footer identity field plus shared per-open proof; native tally belongs to explicit R1, existing reader/owner mechanisms reused | PASS |

Historical test claims do not substitute for current source audit or independently executed checks. Runtime PASS rows describe the reconciled implementation and available proof, with the limits below.

## Validated ledger and prior closure

Only independently confirmed/revised claims enter this verdict. [findings-validation.md](findings-validation.md) supplies exact locations, source traces, claim dispositions and selected correction boundaries.

| Stable ID / status / class | Discovery IDs and exact location | Retained defect / selected correction |
|---|---|---|
| FIND-007-4 / CONFIRMED / VIOLATION | INV-008-1, RSTD-008-1, MAINT-008-1; exec.rs:1111,1137–1138,1593,3990; claim_assembly.rs:87; forge/managed/policy.rs:260; parquet/writer_properties.rs:121 | Changed declarations bypass module imports; import existing concrete types and use bare names in owning production/test modules |
| FIND-007-5 / REVISED / VIOLATION | BEH-008-1; TASK-008:63–72, R1:79–101, original TASK-007 exact-recipe section | Named packet is incompletely reproducible; complete exact recipes and truthful historical/current/deferred attribution, including retained individually named server checks |
| FIND-007-6 / CONFIRMED / VIOLATION | RSTD-008-2; schema/managed_columns.rs:48–79 | Changed schema proof lacks panic rustdoc; document actual assertions on the existing test and retain its body |

FIND-007-1 and FIND-007-2 are closed by source and focused proof. FIND-007-3 is accepted unchanged by maintainer decision. Original cited FIND-007-4 declaration sites and FIND-007-6's two tests were corrected, but new same-rule instances prevent cumulative closure. FIND-007-5's old recipes improved; the current packet has additional missing named proof. Stable IDs are retained rather than multiplying the same governing violations.

## Verification and delivery

[verification.md](verification.md) records exact current commands: **5/5 focused unit tests passed**, with nonzero exact selection, plus whitespace and immutable-source checks. No Postgres-wrapped test, full mise lane, benchmark, commit or implementation edit was performed. Candidate evidence claims broader unit, journey, format/lint/codegen/docs passes; those results were statically assessed and not independently reproduced. Integrated journey/benchmark qualification remains caller-owned under the execution restriction.

One self-contained remediation task is ready for `$wyrd-implement`: [TASK-008-R1-close-review-gaps.md](TASK-008-R1-close-review-gaps.md). It needs only imports, test documentation and truthful evidence corrections; no production behavior, architecture or persistent-data decision is required.
