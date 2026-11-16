# Remediation scope and reviewer selection

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Task: `changes/active/github-verifier-pipelines/tasks/005-pull-request-extraction.md`.
Authority: approved revision 8 of `changes/active/github-verifier-pipelines/spec.md`.
Base: `b262696afcd731d94433dfd1d8d0a9b3ab0d64f4`.
Prior candidate: `1f798d132c03cbc2474e1bad4bc36763e8047848`.
Candidate: `2660af6de2ec7b625ab4564e9519526e240cb020`.
Review range: `git diff 1f798d132 2660af6de`; cumulative base-to-candidate range remains available. Ancestry is preserved.

## Scope and navigation map

Check closure of FIND-005-1 and TASK-005-R1, all remediation changes, and affected helper bodies/callers. Production, test assertions, dependencies, architecture, original task and spec are unchanged. Commit f0f7cf790 adds the Codex r1 reports; subsequent commits add six Rust documentation lines and append/qualify the remediation evidence within that recorded r1 task. Those historical artifacts are inputs, not edits performed by this review. Non-Codex `005-pull-request-extraction-r1/` and `-r2/` are excluded as evidence.

- Server integration `pg_code_review_cache.rs`: `HeldBatch.lock` retains the row-lock transaction; `write` retains the pending second write. Follow complete `start`/`release` bodies and group-commit/rollback test consumers.
- SQL integration `pg_verifier_executions.rs`: `source_extracted` reads the tenant-visible execution fact with `fetch_one(...).expect(...)`; consumers assert completion/replay and stale/rollback/cancel behavior.
- Root AGENTS, SQL nested AGENTS, agent_docs/index.md, rust.md and maintainer-style.md govern the documentation and existing helper shape. changes/spec-driven-development.md permits static proof for documentation-only remediation. TESTING.md distinguishes scoped checks from integrated journey gates.
- CodeGraph was queried first for both helper names/files but returned unrelated indexed symbols; inspect the specific uncovered helpers and callers directly. Do not infer missing source from that search result.

## Contracts, evidence and role applicability

Prior evidence means only sibling `005-pull-request-extraction-codex-r1/` reports. Its findings-validation.md reconciles X1–X13 and validates the six selected discovery reports; verdict.md and verification.md record the accepted source/assertion evidence and 198 focused behavioral tests. Carry-forward is conditional on independent confirmation that the current diff invalidates none of it.

| Contract; owners/producers → consumers | Coverage and selection |
|---|---|
| Mandatory changed-item rustdoc; HeldBatch fields and source_extracted → proof maintainers | Select one fresh ponytail-rev for remediation source/closure validation, accurate field ownership and panic contract, evidence and acceptance reconciliation. |
| Authored action/queue eligibility/source pins/output/bindings/HTTP-MCP/docs; X1–X3, X5–X7, X9, X11–X13 | Carry forward r1 task-review-behavior.md, standards-review.md, maintainer-review.md and findings-validation.md. No runtime, API, generated output, owner or dependency changes; omit new behavior/repo/maintainer specialists. |
| Completion fact/baseline/replay/recovery and metadata commit/rollback; X4, X8, X10 | Carry forward r1 task-review-invariants.md and system-review.md with actual source/assertion evidence, plus domain-review-data-security.md. Only test-helper comments change; omit new invariant/system specialists. |
| Tenant authority, credentials, RLS and persistent integrity; admitted source → evidence/history | Carry forward r1 domain-review-data-security.md, standards-review.md and findings-validation.md. No trust boundary, SQL, transaction or data shape changes; omit new domain specialist. |
| Independent validation, omissions and evidence sufficiency | Fresh ponytail-rev must independently verify this applicability record, actual diff, prior evidence, closure and affected paths. Exact report: findings-validation.md in this r2 directory. No intended verdict supplied. |

No discovery or follow-up pass is initially selected: there is one bounded prior finding and no material new implementation or unresolved conflict. Any validator-originated blocker must receive separate focused source validation before retention. Required fresh validator/report unavailability or candidate mutation blocks review. The orchestrator routes evidence and writes the verdict; it does not validate its own findings.

## Verification plan and limits

Fresh `mise run fmt:check`, `git diff --check 1f798d132 2660af6de`, and final candidate/worktree identity checks. Inspect comments against complete unchanged helper bodies and confirm all remaining range changes are Codex review reports/remediation evidence. No fresh runtime suite, generator, full lint or benchmark is justified by these comments; retain r1 execution evidence and its explicit limits. No new/moved production waits or hot-path change is present. This is the first remediation review after the initial Codex review, so the two-unsuccessful-remediation convergence threshold has not been reached.
