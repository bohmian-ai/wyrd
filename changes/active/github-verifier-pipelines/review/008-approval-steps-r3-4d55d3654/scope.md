# TASK-008 round 3 scope

Original base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`. Immutable cumulative candidate: `4d55d3654ab7361b97aeadbb72b8813707515d49`. Prior candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`. Review `git diff cac4a4ce3 4d55d3654`, with the complete original range available. Review reports are the only write set. A later report-only commit is explicitly requested and does not change the reviewed source.

Original task: `changes/active/github-verifier-pipelines/tasks/008-approval-steps.md`; authority: approved spec and contracts in that packet, applicable architecture, AGENTS and topic guides. Prior acceptance, reports, verdicts and remediation live in sibling `008-approval-steps-r1-798b6b0e8` and `008-approval-steps-r2-cac4a4ce3` directories. Carry forward source-backed unchanged acceptance and closed FIND-008-1/2/5/6/7. Check remaining FIND-008-3/4/8 and affected paths. Current maintainer decision governs: caller Card scope is attribution only, never authorization; verification requires RBAC plus audit; decisions require only verifier:approve on the exact Verifier; approved actors are users and service accounts. This decision is settled, not a review question.

## Navigation and specialist applicability

| Contract/invariant | Producers, owners, consumers | Selected coverage |
| --- | --- | --- |
| Fresh decision original ceiling and cold resolution | VerificationControl::decide → VerifierCardCache::resolve(Db::Conn) → Target::authored_timeout → SQL open_for_decision; replay precedes new resolution | remediation discovery and data domain |
| Actor attribution, exact replay, users/service accounts | authenticated caller → inspection/decide → ApprovalDecisions custody → receipt and inspection; pg_approvals delegation journey | remediation discovery and security domain |
| RBAC-only verification and approve-only decisions | VerificationControl::authorize_verifier; retained(Action); execute/enqueue/evidence/inspection/decision, HTTP/MCP and SDK docs, builtin roles | security domain plus repository standards |
| Mandatory rules, projections and changed proof | Rust service/routes/tests/error/client, MCP tests/guide, Python stub, TS JSDoc, docs and authority amendments | repository standards and remediation discovery |
| Existing durable custody, checkpoint, queue handoff, occurrence identity and supersession | unchanged engine, state, Scribe, runner, SQL and thin SDK projections | carry r1/r2 accepted source-backed rows unless changed dependency invalidates them |

Fresh discovery reports: `task-review-behavior.md` (ponytail remediation), `domain-review-data.md` (persistent lifecycle), `domain-review-security.md` (RBAC/actor), `standards-review.md` (cross-tier rule/projection coverage). Fresh independent validation follows all selected reports and writes `findings-validation.md`. No agent validates its own new finding. Follow-up only for unresolved material conflict or validator-discovered blocker.

Separate invariant/system specialists omitted: data specialist covers changed decision admission, clock and pool ownership; r1/r2 cover unchanged durable ACK, queue/restart and live state. No changed runtime scheduler, deployment, or recovery machinery. Separate maintainer omitted: existing owners are extended/simplified; discovery and standards check reuse, changed-item rustdoc and layout, with r1 maintainer evidence for unchanged machinery. No new abstraction or dependency. Validator independently checks these selections. Cargo execution belongs solely to orchestrator; reviewers must not run Cargo.

Task 009 wording/mock extension is checked for bounded authority clarification; it adds no task-008 runtime machinery. Broad integrated SDK suites and clean L1 performance qualification remain change-review work per r1/r2. No automatic scope expansion after repeated remediation; convergence assessment is required if this round fails.
