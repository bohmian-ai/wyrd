# Task 007 focused remediation verdict: FIX_REQUIRED

All five production corrections are source-correct. FIND-007-4 and FIND-007-5 close. FIND-007-1, FIND-007-2 and FIND-007-3 remain open only for incomplete regression proof; their former runtime defects are not reported as persisting. New FIND-007-6 is an independently confirmed mandatory test-rule violation.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Original base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Prior candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Reviewed HEAD: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.
Incremental range: `git diff 4d418df7c 74fc2190e`; prior candidate is an ancestor. Original cumulative context remains available.

Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Approved specification: packet `spec.md` revision 7, with the original task's explicit approved amendments. Prior acceptance context is sibling `007-deferred-evidence-r1-4d418df7c/`, including verdict, validation, specialist reports and `007-R1-close-publication-and-proof-gaps.md` with its Evidence section.

This round checks only prior-finding closure, the incremental changes and affected callers/consumers/tests. Prior source-backed acceptance carries forward where unchanged; no full discovery restart. The five product files change Artifact admission, attribution, receipt reconciliation, test routing/fixtures, SQL decoding and two rustdoc links. Added prior-review reports are acceptance context. No unrelated product change was retained.

Approved attempt refund on park, single CLI `--file` and outcome-based ordinary restart AC-007 remain settled. New test-only production hooks remain forbidden. Neither missing Docker access nor those decisions is a product finding.

## Independent selection, comparison and coverage

[scope.md](scope.md) records the shared navigation map, affected contracts, owners and specialist selection/omission rationale. [findings-validation.md](findings-validation.md) independently validates that selection and coverage, full affected publication/admission/attribution/custody paths, all closure claims and correction burden.

| Role | Report / disposition |
|---|---|
| Fresh Ponytail remediation reviewer and validator | [findings-validation.md](findings-validation.md): FIX_REQUIRED; source-correct implementation, three unfinished proof rows and one mandatory test violation |
| Fresh focused follow-up | [followup-review.md](followup-review.md): RESOLVED / CONFIRMED FOLLOW-007-R2-1; independently validates validator-discovered stable-code assertion concern |
| Prior behavior, invariant, repo, maintainer, system, security, data coverage | Carried forward from R1 matrices with explicit source/assertion coverage except the qualified closure rows; not inferred solely from unchanged files |

No new discovery team was required: admission and attribution reuse existing owners, receipt reconciliation preserves fencing/protocols, SQL changes only decoding, and runtime lifecycle code is unchanged. The fresh validator traced these changed boundaries and confirmed no separate unresolved sensitive-domain question. Broad cross-tier, ownership and recovery reports remain applicable to unaffected behavior.

One follow-up was necessary because the validator discovered a new mandatory-rule concern and could not validate its own new finding. Its independent source report confirms it. No conflicting claim, untraced shared cause or missing specialist requires another pass. The orchestrator retained only independently validated findings, with no intended verdict supplied to reviewers.

## Reconciled acceptance and prior-finding closure

Detailed current source/assertion citations and producer-to-consumer traces are in the validation report.

| Obligation | Reconciled evidence | Result |
|---|---|---|
| REQ-008 typed slots, derived queue-only, direct baseline, independent branches | R1 behavior/invariant/standards acceptance; implementation and tests unchanged | PASS carried forward |
| REQ-009 matching/source/pins, no publication-started execution or historical replay, JSON/File baseline | R1 behavior/security/data evidence and unchanged shared facades/custody | PASS carried forward |
| FIND-007-1 Artifact metadata/authority before custody | Publisher invokes existing TenantEvidence admission for each kept subscriber; new refusal/valid-acceptance assertions exist, but no denied publisher publication or successful consumer in that story | Implementation PASS; proof FAIL |
| FIND-007-2 canonical subject/actor | Caller::attribution now propagates unchanged through DeferredEvidence/Scribe; no delegated accepted-row assertion reaches publication | Implementation PASS; proof FAIL |
| FIND-007-3 canonical losing-offer receipt | Raced acceptance merged into held and fresh cleared; no executable reconciliation proof with losing offered time | Implementation PASS; proof FAIL |
| FIND-007-4 normal lifecycle routing; REQ-011 / AC-007 | Four ignores removed; default family nextest selects tests; unchanged assertions cover restart/same ID, running consumer, free compute before ACK and original deadline | CLOSED; fresh execution unavailable |
| FIND-007-5 mandatory SQL mapping | Named try_get for subscriber/wake IDs, named ORDER BY; TenantConn, SQL meaning and transaction owner preserved | CLOSED |
| INV-004 canonical unavailability | Prior accepted source trace and publication refusal/retry assertions; unchanged recovery/read paths | PASS carried forward |
| INV-005/006 exact authority, cancellation/revocation/supersession, original deadline, no terminal reopening | Prior invariant/system/security evidence; affected consumers retain checks | PASS carried forward |
| REQ-016 / AC-011 all shipped surfaces, source resolution, schemas/docs/example | Prior cross-tier evidence; shared service callers unchanged | PASS carried forward, subject to publication proof rows |
| Changed rustdoc / test rules | Substantive docs retained and EvidenceTable links fixed; new test checks wording instead of stable codes | FAIL FIND-007-6 |
| Ownership and non-goals | Existing TenantEvidence/Caller/custody/RunState/queue retained; no new hook, broker, payload mirror/table, permission or public protocol | PASS |

## Validated material findings

| ID / status / classification | Exact boundary and remaining consequence |
|---|---|
| FIND-007-1 / REVISED / MISSING | `pg_deferred_evidence.rs:777–857` ends at acceptance. Required publication-level Artifact-read denial and successful consumption are unproved. Production admission defect is fixed. |
| FIND-007-2 / REVISED / MISSING | `service/evidence.rs:138` now uses correct attribution, but no connected test reads a delegated publication's canonical evidence subject/actor. Required provenance regression proof remains absent. |
| FIND-007-3 / REVISED / MISSING | `service/evidence.rs:146–185` is source-correct, but sequential replay cannot detect regression of losing-offer reconciliation. Add deterministic proof of production-used receipt logic plus existing custody/replay integration; no hooks or timing-dependent interleaving. |
| FIND-007-6 / CONFIRMED / VIOLATION | New Artifact refusal matrix `pg_deferred_evidence.rs:813–842` checks message substrings rather than exact stable error codes, contrary to `agent_docs/rust.md:257`. Local test-only code assertions close it. |

The validation ledger records source IDs, obligations, materiality, full tracing, correction burden, selected owner and closure proof. FIND-007-6 maps to independently confirmed FOLLOW-007-R2-1. FIND-007-4/5 need no further correction. No new production failure, speculative hardening or optional improvement is retained. The remaining work is one focused proof package, not runtime redesign. This is the first unsuccessful remediation review; the skill's two-round convergence-assessment threshold does not yet apply.

## Verification limits and handoff

[verification.md](verification.md) records fresh formatting, range whitespace and full `mise run lints` PASS, including workspace all-features/all-targets and shipped-server Clippy. The actual managed-Postgres focused command failed at Docker provisioning before migrations or test selection: zero tests ran. R1 remediation's reported nine passing tests remain implementation-recorded evidence. Source-only attribution/race proof is not relabeled runtime PASS. Docker denial itself is not a finding; the missing assertions are established from candidate source.

HEAD was rechecked unchanged; tracked source/index remain clean. Only the new review directory was written. No commit, push, Git configuration, source patch, test hook or generated artifact was created. Broad suites, code generation and benchmarks were skipped under the focused remediation scope.

**Verdict: FIX_REQUIRED.** Route [007-R2-finish-publication-regression-proof.md](007-R2-finish-publication-regression-proof.md) directly to `$wyrd-implement`. The next review retains the source-correct fixes and closed findings and checks only the focused proof/test corrections and affected paths.
