# TASK-001 verdict

**FIX_REQUIRED** — 15 independently validated findings.

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/code-review-workflow`.
Base: `f4e79ea00523af5cd9212fdacde64f16c97ad554` (`f36150646^`).
Candidate: `365bfc94bfef1a153f213462f7ef104afa823741`.
First review: complete base...candidate range, 15 implementation commits. Approved spec `changes/active/verifier-review-pipelines/spec.md`, revision 16; original task `tasks/TASK-001-durable-agent-work.md`, with frozen supplements and hashes in [subject.md](subject.md). No prior findings, closure evidence or remediation rounds; convergence assessment is inapplicable.

The live checkout was integrating main under separate human approval. On detecting its movement, validation paused; the human explicitly directed continuation of the original review. Validation resumed in `/tmp/wyrd-task-001-review-365bfc94b`, detached at the identical original candidate, with clean tracked source and matching frozen authority hashes. No merged/rebased source was substituted. This verdict covers the original range only; no integration acceptance is claimed.

## Independent reviewers and coverage

All required selected reports exist. Every reviewer was fresh and filled one role; the orchestrator routed claims and wrote artifacts, and did not validate its own findings.

| Role | Reason / coverage | Report and result |
|---|---|---|
| behavior-rev | Full task acceptance, callers/results, regressions and non-goals | [task-review-behavior.md](task-review-behavior.md): FAIL proposals |
| invariant-rev | Checkpoint/attempt/deadline/publication state flow | [task-review-invariants.md](task-review-invariants.md): FAIL proposals |
| repo-rev | All changed tiers/languages, nested authority, mandatory rustdoc | [standards-review.md](standards-review.md): FAIL proposals |
| maintainer-rev | Workflow ownership/reuse, API/type discoverability and changed layout | [maintainer-review.md](maintainer-review.md): FAIL proposals |
| system-rev | Process crash/restart, dependency failure and shared capability recovery | [system-review.md](system-review.md): FAIL proposals |
| domain-security | Exact-Verifier RBAC, principal revocation and publication/File trust | [domain-review-security.md](domain-review-security.md): FAIL proposals |
| domain-data | Persistence/RLS/transactions, page indexes and event integrity | [domain-review-data.md](domain-review-data.md): FAIL proposals |
| domain-concurrency | Lease/cancel/admission/replay races and writer exclusion | [domain-review-concurrency.md](domain-review-concurrency.md): FAIL proposals |
| followup-rev | Resolve final-durability approved amendment versus stale owner paragraph | [followup-review.md](followup-review.md): RESOLVED |
| ponytail-rev | Fresh source/materiality/correction validation of every proposal and acceptance coverage | [findings-validation.md](findings-validation.md): FIX_REQUIRED, finalized ledger |

Shared navigation/selection: [navigation.md](navigation.md). Proposal comparison: [claim-comparison.md](claim-comparison.md). Agreement was not proof; the final validator independently traced every retained cause. The focused follow-up was needed for a material authority conflict, not a unique-finding vote. It proved final-result durability and the missing owner amendment are already approved Task 001 scope. No additional follow-up was needed; the validator discovered no new blocker requiring separate validation.

## Reconciled acceptance matrix


| Required outcome / constraint | Reconciled result and source/proof assessment |
| --- | --- |
| Durable admission/idempotent keyed replay | PASS for existing admitted-input/row ordering: service → Scribe ACK → SubmissionSink commit → AdmissionReply; recorded Rust response-loss journey supports it. Revoked authority admission has finding 8. |
| Separate committed action/requirement restoration, ambiguous attempt counting | PASS for existing plain-work checkpoint restore/counting, engine/SQL/Rust journey evidence; not a pass for remaining deadline or expiry, findings 7/9. |
| Durable final completion/readback | FAIL, finding 10. Explicit approved owner amendment resolves the architecture conflict; flush-before-restart proof cannot cover crash-before-ACK. |
| Immutable file/JSON/null and complete directory snapshot | PASS for all-or-nothing accepted content/strict tree grammar/writer exclusion on normal execution; findings 1/2/13 limit replay, budgets and execution strategy, not content immutability already proven. |
| Publication uniqueness/fresh attempts | PASS for tuple uniqueness/fresh attempt separation; original/concurrent same-call receipt FAIL, finding 1. |
| Typed retained mounts, honest collection statuses and producer-only source lineage | PASS for inspected ordinary mounts/provenance, existing mount/engine proofs. Pending visibility FAIL, finding 3; accepted consumer size FAIL, finding 2. Task-002 expansion/speculation remains excluded. |
| Cancel preserves accepted work and terminal results | PASS for visible pending/running rows, repeated terminal cancel, token replacement and worker-format refusal; findings 8/9 narrow the authority windows not covered by those tests. |
| Required exact Agent/Prompt availability and verifier-run internal loading | FAIL, finding 5; injected-engine/card-read-granted scenario does not prove the required resolver path. |
| Canonical event table/Variant/history-only semantics and ordinary replay | PASS for registration, event intent transaction and same-configuration replay; frame-configuration recovery FAIL, finding 12. Preserve execution_outputs semantics. |
| Dedicated metadata-only exact-Verifier read/cancel, tenant and manifest-selected downloads | PASS for shared service/HTTP/Rust/MCP source and recorded journeys, including local verifier-read-only bytes. No Cards/Bifrost fallback or arbitrary path introduced. |
| Stable bounded indexed pages | FAIL findings 4/11. Existing in-order page proof establishes only sequential geometry. |
| Thin HTTP/Rust/Python/TS/CLI/MCP projections | PASS for owner split and method presence. Python expiry parity FAIL finding 14; required Python/TS content/live-cancel proof FAIL finding 6. Task 3 creates content via tools and does not defer Task-001 read-binding proof. |
| Required changed-item rustdoc | FAIL finding 15. Optional prose preferences remain excluded. |
| Existing plain Verifiers/direct synchronous verify; no extra Card kind, queue, graph store, ingest path or tool exposure | PASS on selected cumulative coverage and inspected affected callers; corrections must retain these non-goals. |
| Supported-platform parity | No platform-specific implementation expansion identified. Actual all-target CI evidence absent; record as verification limit for change/CI, not an invented implementation finding or an executed PASS. |

All selected reports exist and cover the assigned task/boundaries. The independent pass filled the specific routed uncertainties and did not restart comprehensive discovery. No new blocker was discovered by this validator requiring another fresh discovery agent. No prior findings/remediation rounds exist; convergence assessment is inapplicable.

## Validated findings and remediation

Only the independently confirmed/revised ledger is actionable; discovery duplicates create no extra work. Full source-local dispositions, exact evidence, scenarios, consequences, materiality, correction burden and focused closure proof are preserved in [findings-validation.md](findings-validation.md) and reproduced in the self-contained remediation task.

| ID | Required correction |
|---|---|
| FIND-TASK-001-1 | Recover persisted publication receipt before scratch/upload; correct concurrent identical replay |
| FIND-TASK-001-2 | Durable cumulative retention accounting and effective consumer-compatible ceilings |
| FIND-TASK-001-3 | Read accepted publications from their authoritative completed File metadata, removing Bifrost inventory rediscovery |
| FIND-TASK-001-4 | Commit-ordered stable page cuts for both paged tables |
| FIND-TASK-001-5 | Internal exact-capability authorization and unavailable-capability terminal semantics |
| FIND-TASK-001-6 | Required Python/TS successful content/download and unfinished-cancel journeys |
| FIND-TASK-001-7 | Conserve remaining deadline across claim/loading/dispatch |
| FIND-TASK-001-8 | Order all delayed durable admissions with principal revocation |
| FIND-TASK-001-9 | Refuse new work/checkpoint/publication writes under expired leases |
| FIND-TASK-001-10 | Durable selected final result before completed acknowledgement/commit |
| FIND-TASK-001-11 | Required tenant/execution/sequence access indexes |
| FIND-TASK-001-12 | Event replay physical identity independent of configurable frame grouping |
| FIND-TASK-001-13 | Snapshot IO through existing blocking strategy with full-lifetime writer exclusion |
| FIND-TASK-001-14 | Python download-plan required expiry field parity |
| FIND-TASK-001-15 | Bounded mandatory changed-item rustdoc corrections |

Remediation: [TASK-001-R1-durable-execution-guarantees.md](TASK-001-R1-durable-execution-guarantees.md). Route directly to `$wyrd-implement`; review did not implement, merge, push or deploy. The packet selects existing owners and groups shared causes; it preserves approved behavior and rejects extra queues/services, consumer guards and unrelated cleanup. No specification revision is required by the validated corrections.

## Human-approved publication correction update

On 2026-11-05 the human approved reusing the standardized File/Artifact completion metadata pattern instead of adding inventory-wait guards. [publication-readback-addendum.md](publication-readback-addendum.md) independently validates this bounded correction revision: completion already returns a verified object descriptor, but publication discards it and readers query Bifrost to rediscover its location. Preserve that descriptor privately with PostgreSQL publication acceptance and reuse shared storage reads for both hosted mounts and authorized inspection/download/content. Required File completion ACK, inventory/history recording, tenant/producer authorization, integrity and existing cleanup ownership remain intact; no general File GC or new catalog is assumed. Completed upload records are protected by the existing sweeper's state checks.

The remediation task and finding-3 summary above now select this correction, with related finding-1/2 replay/mountability proof updated. The original findings-validation.md remains historical evidence; its earlier wait/defer recommendation is superseded by this addendum. Finding IDs, original subject, diagnoses and FIX_REQUIRED verdict are unchanged. Closure now proves readback and downloads work after restart while inventory queries are unavailable, without new waiting/attempt consumption, and accepted bytes survive upload-session expiry/sweeper execution. This does not broaden the task to all generic File inventory readers or remove existing completion ACK guarantees.

## CLI naming concern, separate from findings

`wyrd execution` is broader than the verification namespace used by HTTP/MCP. A verification-qualified namespace such as `wyrd verification` would communicate scope more clearly. Existing command help identifies Verifier work, so the current name is not a material acceptance failure. No rename or compatibility alias is approved by this review, and the concern receives no FIND ID. A later explicit naming decision should update command tree/help/docs/journey together.

## Verification limits

Independent static source/diff review, with `git diff --check` passing on the original range and final detached candidate identity/clean tracked tree reconfirmed. Recorded implementation test/check results were inspected as recorded evidence; Cargo, Postgres, language runtime, EXPLAIN, fault injection and all-platform CI were not independently rerun. Mutating generators/build gates were not run against the immutable source. Source-proven interleavings and explicit required-proof gaps underpin the ledger; no measured latency or load claim is made. Main integration remains outside this verdict. No prior closure exists to carry forward.

Final verdict: **FIX_REQUIRED**. Finding IDs: FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3, FIND-TASK-001-4, FIND-TASK-001-5, FIND-TASK-001-6, FIND-TASK-001-7, FIND-TASK-001-8, FIND-TASK-001-9, FIND-TASK-001-10, FIND-TASK-001-11, FIND-TASK-001-12, FIND-TASK-001-13, FIND-TASK-001-14, FIND-TASK-001-15.
