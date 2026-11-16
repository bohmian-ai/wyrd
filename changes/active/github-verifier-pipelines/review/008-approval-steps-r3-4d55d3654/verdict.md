# TASK-008 round 3 verdict

**PASS**

## Immutable subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/github-verifier-pipelines-task-004`.

- Original task: `changes/active/github-verifier-pipelines/tasks/008-approval-steps.md`.
- Original base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`.
- Prior candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`.
- Immutable cumulative candidate: `4d55d3654ab7361b97aeadbb72b8813707515d49`.
- Remediation diff: `git diff cac4a4ce3 4d55d3654`; original cumulative range remains acceptance context.
- Authority: approved packet spec revision 8, Human approval contract, original task and applicable repository/architecture rules, with the current explicit maintainer decision.

This is the second remediation review. It checks remaining FIND-008-3/4/8, the actual remediation diff and affected callers. Source-backed r1/r2 acceptance remains for unchanged behavior; scope.md and findings-validation.md record why the current changes do not invalidate it. Prior verdicts, findings and remediations remain intact in sibling r1/r2 directories.

The maintainer decision is settled: a caller's Card scope attributes only, never authorizes; RBAC plus audit authorizes verification; approval needs only verifier:approve on the exact Verifier; users and service accounts are approved actors. Superseded Card-scope expectations do not reappear as review requirements. Human-directed authority and test corrections receive no FIND IDs. The small task-009 mock-extension clarification adds no unrelated runtime scope.

## Independent coverage and reconciliation

| Role | Report | Result |
| --- | --- | --- |
| Fresh remediation ponytail discovery | task-review-behavior.md | PASS |
| Persistent-data/lifecycle specialist | domain-review-data.md | PASS |
| Security/RBAC/actor specialist | domain-review-security.md | PASS |
| Cross-tier repository standards specialist | standards-review.md | PASS |
| Fresh independent structured ponytail validator | findings-validation.md | PASS recommendation; validated empty open ledger |

scope.md maps each changed contract to its owners, producers, consumers and selected specialists. The validator independently accepted that applicability and coverage. Separate invariant/system review is unnecessary this round: the data specialist covers changed admission/deadline/resource behavior and prior invariant/system reports cover unchanged engine/state, custody, queue and recovery. No substantial new abstraction or ownership change requires a fresh maintainer specialist; discovery/standards check changed shape and mandatory docs, with prior maintainer evidence carried forward. No required report is missing.

Discovery reports were complete before fresh validation began. The security specialist's report was transcribed by the orchestrator because that role is read-only; its independent analysis is preserved. claim-comparison.md records no conflicting material claims or untraced common cause. No followup-rev was needed, and validation introduced no new blocker requiring separate source validation.

## Reconciled acceptance and closure

| Obligation / finding | Accepted implementation and proof | Result |
| --- | --- | --- |
| FIND-008-3, original ceiling and target refusal | Fresh admission preserves the resolver's inner refusal as typed target-not-found; only a resolved target supplies optional authored timeout. Existing PostgreSQL predicate retains admission-relative min(authored,runtime). Accepted identical replay precedes new eligibility. Real target deletion and authored-expiry journeys passed. | CLOSED |
| FIND-008-4, verified actor and exact retry | Receipt and replay use verified acting principal. Ordinary Service bootstrap/exchange/delegation decides a real occurrence; persisted inspection names that actor; complete retry replays; another actor sharing the subject and direct subject receive typed 409. Journey passed. | CLOSED |
| FIND-008-8, cold-cache resource ownership | Existing resolver uses Db::Conn of the decision's held tenant transaction. Real restarted-cache journey holds all pool slots except one and accepts the decision without a second acquisition. Journey passed. | CLOSED |
| FIND-008-1/2/5/6/7 | Prior independent closure of reached pending visibility, withdrawn history/refusal, MCP Panics docs, approved/rejected owner wording and safe unavailable diagnostics remains valid. Current withdrawal, unacknowledged-ACK/retry, restart and supersession tests passed. | CLOSED, carried forward |
| REQ-010a/b, stable occurrence and park | Closed action/schema, registration-derived queue_only, stable execution/step/item/iteration identity, no pending SQL or approval deadline, independent progress and sole-state park handoff retain prior source-backed evidence. Current changes do not replace those owners. | PASS |
| REQ-010c, exact permission, tenant, audit, custody and replay | retained(Action::Approve) audits exact Verifier before occurrence/replay, without read/cards prerequisites. Approve-only positive and wrong-Verifier/viewer/foreign-tenant negatives remain. Canonical fenced decision ACK still precedes receipt. | PASS |
| Shared verification authorization | Existing authorizer enforces exact RBAC and stages allow/deny audit for execute, enqueue and deferred evidence. Changed HTTP/MCP positive rooted/rootless/unrelated-context and negative no-run/wrong-Verifier assertions preserve the settled boundary. | PASS |
| REQ-011 and INV-001–007 | Same ExecutionId, original accepted authority/budgets/deadline, rejection as data, no stale release, first-wins custody, restart and bounded ACK-to-queue repair retain prior source evidence and current focused proof. No new lifecycle owner or SQL decision payload store. | PASS |
| REQ-016, surfaces, docs and mandatory gates | Prior typed SDK/CLI/MCP inspection/decision coverage carries forward. Current projections change permission descriptions, not signatures/exports; standards review covers changed Rust docs, errors, route/MCP and authored/generated text. | PASS |
| Non-goals and smallest complete correction | Existing target resolver, caller-owned transaction, verified attribution and ordinary identity test owners suffice. No new deadline, pool policy, dependency, login/proof/token restriction, UI or duplicate workflow. | PASS |

findings-validation.md contains independently checked source locations, assertions, producer-to-consumer traces, coverage and correction-burden assessment. Validated open finding ledger: **empty**. No new IDs or remediation task. The unchanged publication wording noted in standards review establishes no contrary Card-scope authorization contract under the settled maintainer decision; it creates no remediation work.

This successful second remediation review converges at the existing owners: the earlier incomplete target-refusal branch, missing connected actor proof and fix-induced nested acquisition are now closed. There is no second unsuccessful remediation verdict requiring another correction scope.

## Verification and limits

verification.md records observed exact commands and nextest run IDs: 13/13 focused server tests, 1/1 changed MCP journey, both SQL migration setup tests in each wrapper, fmt:check and diff checks passed. Cargo commands were sequential. Source remained at the immutable candidate throughout review; only this new report directory was written. The requested report-only commit follows this verdict and changes no reviewed source. No merge, push or deployment is performed.

Broader SDK/CLI suites, full lints/codegen/docs regeneration, aggregate gates and clean L1 performance qualification were not rerun. They remain integrated change-review qualification under the prior bounded-review agreement; this PASS does not claim those results.

**PASS** — no open findings; no remediation task.
