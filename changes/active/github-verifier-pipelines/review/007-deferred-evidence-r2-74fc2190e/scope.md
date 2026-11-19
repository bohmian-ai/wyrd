# Task 007 focused remediation review

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Original base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Prior candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
Candidate: `74fc2190e255a3e3cf7515091ff45ab86a288f6b`.
Review range: `git diff 4d418df7c 74fc2190e`; cumulative range available for context only.
Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`.
Approved authority: packet `spec.md` revision 7 and explicit task amendments. Prior reports and remediation task: sibling `007-deferred-evidence-r1-4d418df7c/`, including remediation Evidence.

Scope is FIND-007-1..5 closure, incremental diff and affected consumers. Carry forward prior verdict/validator matrices and seven specialist reports for unaffected behavior. No ancestry rewrite or implementation redesign is evident; immutable identities are checked again at completion. Prior report additions in this range are context, not a product change.

Approved attempt refund, single CLI `--file`, outcome-based AC-007 and no new test-only production hooks are fixed. Do not reopen them or require hook-based proof. Implementation-recorded PASS is distinct from source inspection and fresh review execution.

## Navigation and specialist applicability

| Contract / changed boundary | Source, producers, consumers and proof | Selection and carry-forward rationale |
|---|---|---|
| Artifact admission before immutable custody (FIND-007-1) | `components/verification/service/evidence.rs::subscribers` now invokes existing `TenantEvidence::admit`; hosted evidence resolves metadata/authorization; `pg_deferred_evidence::artifact_evidence_is_admitted_before_it_is_accepted`; existing `pg_hosted_artifacts::artifact_reads_refuse_before_use` | Fresh Ponytail validator traces publisher authorization, slot limits, refusal and consumer admission. Prior security/data reports establish the boundary; remediation reuses existing owner without new auth/persistence semantics. No separate domain discovery unless tracing exposes an unresolved material question. |
| Verified attribution (FIND-007-2) | `publish_evidence` now passes `Caller::attribution()` to `DeferredEvidence::accept` and `ScribeWrite::Evidence`; canonical evidence row attribution | Validator checks owner and propagation; prior security report covers verified Caller. One shared-owner call substitution does not warrant repeating security discovery. Assess claimed proof limits explicitly. |
| Canonical racing receipt (FIND-007-3) | Publication reconciliation extends `held` from canonical `accepted` and clears `fresh`; same custody/read/wake owners and HTTP/MCP consumers | Validator traces changed reconciliation and receipt construction with prior data/concurrency trace. No changed fencing, writer, transaction or concurrency protocol; no separate data/invariant specialist unless a material unresolved path emerges. |
| Normal lifecycle proof routing (FIND-007-4) | Four ignores removed from `pg_deferred_evidence`; `test:wyrd`, nextest profiles, existing Postgres lifecycle | Validator checks routing/assertions; prior system/invariant reports cover unchanged runtime park/restart/resource/deadline behavior. No runtime recovery code changed, so no fresh system reviewer. |
| SQL name mapping (FIND-007-5) | `VerifierExecutionQueue::evidence_subscribers` / `wake_evidence_consumers` use `Row::try_get` by name; publisher and runner consume identities | Validator checks named columns, unchanged TenantConn/transaction/query behavior against nested SQL AGENTS. Local decoding change does not require broad cross-tier standards discovery. |
| Rustdoc and test fixture reuse | EvidenceTable links corrected; existing hosted Artifact client becomes crate-visible; new Artifact test reuses declaration/client owners | Validator checks changed-item docs and fixture callers. Prior maintainer/standards coverage carries forward elsewhere. No substantial new abstraction or workflow; no maintainer specialist needed. |
| Unchanged public surfaces, lifecycle, source inference, schemas and non-goals | Prior behavior, repo, maintainer, invariant, system and domain matrices with source/assertion citations | Carry forward acceptance except rows qualified by FIND-007-1..5; no frontend, SDK or model-provider protocol changes in remediation. No full discovery restart. |

Select one fresh `ponytail-rev` as the focused remediation reviewer and independent validator, as prescribed by the remediation default. The orchestrator routes evidence and writes the verdict, and does not validate its own findings. Report: `findings-validation.md`. If the validator discovers a new blocker, obtain separate focused source validation before retaining it; add a required specialist if it identifies a genuine coverage gap. No follow-up selected initially because no conflicting current claims have yet been established.

CodeGraph was used first to map publication/subscribers, HTTP/MCP callers, TenantEvidence admission, Caller attribution and custody. Root/nested AGENTS, spec-driven development, maintainer style, applicable Rust/errors guides, architecture Verifier/Bifrost/security and TESTING/mise govern the bounded review. Shared memory was searched for task context; general preferences do not expand this explicitly focused review into broad benchmarking or full suites.

Reports only may change. No commits, pushes, Git configuration or test-only production hooks. Coordinate Cargo execution through the orchestrator. Fresh command results belong in `verification.md`; Docker restrictions must not be reported as product defects or connected PASS.

## Focused validation added during review

The validator proposed a new mandatory-rule concern in the new Artifact refusal test: it checks message substrings rather than stable Wyrd error codes, contrary to `agent_docs/rust.md` Tests. A fresh `followup-rev` was selected only to independently validate that proposal and its materiality/correction burden, with exact report path `followup-review.md`. The validator cannot validate its own new finding. This is a specific changed-test question, not reopened discovery or another vote on the five prior findings. Prior-finding proof closure remains with the validator. The orchestrator will retain the new proposal only if independently confirmed/revised and reconciled by the validator.
