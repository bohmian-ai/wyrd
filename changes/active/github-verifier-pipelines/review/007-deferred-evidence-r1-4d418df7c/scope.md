# Task 007 review scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-007`.
Base: `16ea9694c34a5ec1867306bd5e34df62f0888611`.
Candidate: `4d418df7c23ee03a976f89349b74debdddee72c6`.
First acceptance review of the complete base-to-candidate range; no carried-forward acceptance or prior-finding closure. Read `git diff 16ea9694c...4d418df7c`; the user also requests `git diff 16ea9694c HEAD` (merge base is checked). Source stays immutable; reports only are writable here.

## Authority and approved decisions

Original task: `changes/active/github-verifier-pipelines/tasks/007-deferred-evidence-and-resumption.md`. Approved spec: `changes/active/github-verifier-pipelines/spec.md` revision 7. Supplements are under `changes/active/github-verifier-pipelines/design/`: `approved-deferred-inputs.md`, `approved-runtime-and-github.md`, plus `contracts.md` at the packet root. Task header revision 5 does not override its explicit amendments or the user's approval.

The maintainer approved attempt refund on park, one CLI `--file` flag with its documented JSON/media limitation, and AC-007 as ordinary restart/retry/refusal outcomes without test-only crash hooks. These are settled, not findings. Task evidence claims are inputs to inspect, not independently established test runs. Root/nested AGENTS, spec-driven-development, applicable agent_docs, architecture Verifier/Bifrost/security sections, TESTING.md and operations recovery own the relevant boundaries. Shared memory search supplies preferences only, not new acceptance criteria.

## Starting navigation map

| Contract/invariant | Owners, producers, consumers and tests | Required review |
|---|---|---|
| Typed deferred slots, server-derived queue-only, ordinary direct baseline | wyrd-spec card/verifier and verification execution/state; server cards service; vala-verify blocks engine; registration and engine tests | behavior, repo |
| Publisher source and typed all-surface projections | shared wyrd-client verify/evidence; wyrd-loader repository source inference; CLI evidence; Python verify/evidence; TS native verification and wrapper; MCP verification; gen_schemas/gen_ts_types; SDK/CLI/MCP journeys, docs and example | behavior, repo, maintainer |
| Authorized matching, no metadata authority, selected bindings and exact pins | server components/verification/service/evidence; github service; verification/evidence; accepted PR/binding dependency owners; wrong-tenant/repository/type/refusal journeys | security domain |
| Canonical durable acceptance, conflict/replay, no historical subscription leakage | verification/evidence, InputResolver; ScribeOutbox; Bifrost evidence table and Gate; committed batch fences; publish and SQL coordination | persistent-data/concurrency domain, invariant |
| Sole state writer, independent branches, accepted progress and budget | vala-verify engine; server RunState, runner deferred/park, engines; existing Completions, claims, leases and resources | invariant, maintainer |
| ACK before retiring delivery, release compute, same-ID resume, original deadline; cancellation/revocation/supersession | runner, RunState, ScribeOutbox; verifier_executions/pull_requests SQL and existing queue/SubmissionSink; pg_deferred_evidence, pg_verification_runtime, pg_github_pull_requests | system, invariant, data domain |
| Cross-tier mandatory rules and generated/public parity | full 92-file diff including tests, fixtures, architecture, task 008 adjacency, docs diagrams, mise/codegen | repo |

All listed specialist roles are selected: lifecycle/shared-state complexity requires invariant; broad Rust/Python/TS/SQL/docs surface requires repo; new DeferredEvidence and Evidence handles plus changed state/loader ownership require maintainer; recovery/park/runtime consequences require system; publisher permission/tenancy requires security; batch acceptance/SQL readiness races require a coherent persistent-data and concurrency specialist. No additional ML/LLM, UI, or provider-protocol specialist: no model protocol or UI implementation changes; existing provider access is inspected by security. The domains overlap at custody but have distinct scopes. Each reviewer must expand this map from actual source and callers and compare new workflows to existing owners.

## Reports and verification

Fresh discovery reports: `task-review-behavior.md`, `task-review-invariants.md`, `standards-review.md`, `maintainer-review.md`, `system-review.md`, `domain-review-security.md`, `domain-review-data.md`. A fresh validator produces `findings-validation.md` after all discovery. Follow-up only for a material unresolved path or conflicting claim. Orchestrator consolidates validated evidence into `verdict.md` and remediation if required.

Reviewers may run read-only navigation and nonmutating checks; do not run Cargo concurrently, edit source, commit, push, or configure Git. Coordinate any requested runtime check with the orchestrator. Verification limitations and the distinction between implementation-recorded and review-observed results must be explicit. No new dependency or test harness for review.
