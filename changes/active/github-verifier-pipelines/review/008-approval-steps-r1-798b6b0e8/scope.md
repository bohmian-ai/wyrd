# TASK-008 review subject and coverage

First acceptance review. Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.
Immutable range: `cd0b8e18b...798b6b0e83595356c0d2328fa541119267e84fb2`; HEAD must remain the candidate. Initial working tree clean.
Task: `changes/active/github-verifier-pipelines/tasks/008-approval-steps.md`.
Approved authority: packet `spec.md` revision 8, `contracts.md` Human approval, relevant design supplements referenced by the task, root/nested AGENTS, and owning architecture sections. Task frontmatter revision 5 predates approved amendments; audit against current approved authority. Architecture amendments are explicitly required by the task/spec; assess their consistency rather than treating candidate amendments as self-authorization.
No prior TASK-008 evidence carried forward. Task evidence is a claim to verify, not independent proof. Review complete range, including changed tests and architecture. Production source stays immutable; reports only in this directory.

## Shared navigation map

| Contract/invariant | Owners and flow | Selected coverage |
|---|---|---|
| Approval authoring, queued-only eligibility, occurrence and requirement semantics | wyrd-spec card/verifier + verification contracts → cards/service registration → vala-verify blocks engine/scope → server verification/state | behavior-rev comprehensive acceptance; invariant-rev shared lifecycle; maintainer-rev owner reuse |
| Exact accepted occurrence, decision replay and ACK, tenant and permission boundary | verification/service/inspection + service/approvals → ApprovalDecisions → Scribe outbox/Bifrost approvals; wyrd-runtime permissions and roles; verifier_executions/approvals SQL | domain-rev-security; domain-rev-data; invariant-rev; system-rev |
| Same-execution continuation, original expiration, rejection/expiry, authority loss, supersession and restart | engine waiting → server runner/state/inputs/approvals → existing queue park/recheck/wake + canonical evidence/state owners | invariant-rev; system-rev; domain-rev-data; behavior-rev |
| Public inspection/decide/errors/scopes/docs and parity | wyrd-client verify/execution → Rust SDK; Python wrapper/stub; TS native/wrapper/generated declarations; CLI execution; server routes/MCP; docs/fixtures | behavior-rev; repo-rev all tiers; maintainer-rev |
| Verification and resource/performance regression | pg_approvals, engine tests, declaration/OpenAPI tests, SDK deferred-evidence journeys, CLI github_journey, MCP github; task Evidence commands | all reviewers evaluate proof in scope; orchestrator records executable checks |

Required specialist selection: invariant-rev (complex parked/shared state), repo-rev (77 files across contracts/runtime/server/SQL/three SDKs/docs), maintainer-rev (new ApprovalDecisions workflow and engine changes), system-rev (ACK/queue/restart failure propagation), domain-rev-security (approve RBAC/subject trust/tenant separation), domain-rev-data (Bifrost acceptance fences + SQL concurrency/deadline coordination). None omitted. Separate concurrency agent omitted because data reviewer owns the coherent decision custody/queue concurrency domain, with invariant reviewer covering engine transitions.

Reviewers read `git diff cd0b8e18b...798b6b0e8` directly, verify the map, expand affected callers/consumers, follow CodeGraph before code reads, and independently inspect applicable authority. Do not use task Evidence summary as proof. Each selected role writes its assigned report here. A fresh independent ponytail-rev follows completed discovery and any needed follow-up; it validates applicability, coverage, every proposal and correction. Orchestrator does not self-validate findings.

Shared memory surfaced maintainer preferences about memory-only hot paths and full clean L1 benchmark evidence when a hot path changes. Determine applicability from actual source; no synthetic host load. New/moved IO waits require sanctioned owner-boundary analysis. No benchmark result is claimed at scope establishment.
