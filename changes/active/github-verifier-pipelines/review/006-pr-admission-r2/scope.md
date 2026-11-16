# Task 006 remediation review scope

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004
Original base: 2165fe606efe498db4e0455866d29236e85d889c
Prior candidate: efda8cb2c59c8206b52dbe1d92fcae1ba34be430
Candidate: fb8da5ad7e87f784cde39513cb8605dba8bbf1a2
Review range: `git diff efda8cb2c fb8da5ad7`; cumulative original base to candidate. Preserve ancestry and immutable source. Reports only in this directory.

Approved spec: revision 5, `git show 0f08e6c9f:changes/active/github-verifier-pipelines/spec.md`; current draft revision 6 does not replace Task 006 basis. Original task: `changes/active/github-verifier-pipelines/tasks/006-pr-admission-and-supersession.md`. Prior verdict, validated ledger, specialist reports and remediation task are in sibling `../006-pr-admission-r1/`. Review `006-R1-close-admission-lifecycle-and-authority.md` and its obligations/evidence. Do not overwrite prior artifacts.

## Scope and navigation map

Check closure FIND-006-1 through FIND-006-7, R1-1 through R1-7 and R1-P, every changed code/test/doc path and affected shared consumers/writers. Carry forward original A2/A3/A4/A6/A11/A15/A16/A17 and structural exclusions from prior verdict and source-backed specialist reports unless this delta affects them. Recheck affected portions, not the whole repository.

| Boundary | Producers, owner and consumers | Prior evidence / closure scope |
|---|---|---|
| Observation ordering and zero-target supersession | GithubConnections::pull_request -> VerifierExecutionQueue lock/observe/deactivate/reserve -> activation and settlement | Prior behavior/invariant/data reports; check changed PR lock span, empty policy and held-read assertions. |
| Revocation, tenancy, binding | webhook routing -> installation_access/hold_binding -> slot writers -> activation | Prior security/data reports; check all access writers, lock order, replaced bindings and inverse ordering. |
| Inspection permission | VerificationControl::retained -> status/invocations/publications; queue or InputResolver origin | Prior security report; check shared gate, retained no-queue/missing evidence, ordinary reads and cancellation. |
| Provider failures and process lifecycle | GithubApp PR GET -> webhook ACK/scanner; run_delivery_recovery -> supervisor/shared drain | Prior system report; check 403 and cancellation, held-response proof. |
| Standards and docs | server/SQL/mock Rust, Python/TS wrappers, MCP test, guide/generated llms/architecture | Prior standards/maintainer reports; check changed-item rustdoc, declaration parity, docs accuracy. |

CodeGraph initial navigation located queue reserve, GithubSlotBinding and callers. Starting symbols: GithubConnections::installation_token, installation_access, hold_binding, pull_request, run_delivery_recovery; VerificationControl::retained; tests pg_github_pull_requests and pg_verifier_executions. Expand this map independently against source.

## Authority and topology

Read root and applicable nested AGENTS (docs, Python SDK, TS SDK, SQL), changes/spec-driven-development.md, agent_docs/index.md and relevant rust/errors/maintainer-style/agent-surfaces guides. Architecture authority: README map, wyrd-design GitHub admission/execution sections, security posture connection/tenant/execution-read sections, operations reliability/recovery, Bifrost audit/drain where relevant. TESTING.md and mise tasks own checks.

Select one fresh remediation ponytail discovery reviewer, report task-review-behavior.md, then a separate fresh ponytail validator, report findings-validation.md. Prior behavior/invariant/repo/maintainer/system/security/data coverage exists. No separate specialist initially selected: the delta implements their validated, decision-complete corrections through existing PR/slot locks, queue deactivation, permission audit and worker cancellation owners. No new schema, public API, security model or lifecycle owner is introduced. Closure reviewer explicitly covers affected sensitive data/security and runtime paths and all changed languages/docs. Add specialists for a material new boundary, missing coverage or unresolved domain question. Validator independently audits selection/omissions. UI/model/OIDC profiles remain inapplicable. First formal remediation review; convergence threshold not reached.

Discovery receives immutable range, applicable authorities, prior findings and evidence, not completion summary or current reviewer conclusions. Validator receives all selected reports without intended verdict. Follow-up only for material conflict, unresolved path or untraced cause; new validator-originated blocker needs separate source validation.
