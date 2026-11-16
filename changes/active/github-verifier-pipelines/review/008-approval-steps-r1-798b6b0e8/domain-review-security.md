# TASK-008 sensitive-domain review: security

Result: **FAIL**.

Subject: first complete review of `cd0b8e18b...798b6b0e83595356c0d2328fa541119267e84fb2` in the repository named in `scope.md`. No prior acceptance evidence carried forward. Scope is authentication, approve permission and subject authority, tenant separation, attribution/replay identity, stale work, and durable decision provenance. Production source was not changed; no Cargo command was run by this reviewer.

## Authority and source coverage

Read the review skill, shared navigation/selection record, root contributor instructions, packet `spec.md` REQ-010/011 and INV-001/003/006, `contracts.md` Human approval, original TASK-008 including its obligations, and `design/approved-deferred-inputs.md` Approval and deadlines. Governing security sections inspected: accepted execution authority and deferred publication/approval (security posture around lines 145–178), token permission snapshot (250–271), delegation and federation (445–470), and typed Verifier scopes. The first two are approved task-specific amendments; delegation and token authority are established owners that this task must preserve.

CodeGraph was used before reading code. Expanded the shared map through `Caller`, `Caller::from_authenticated`, `Caller::attribution`, `delegated_attribution`, permission checking, token delegation, retained execution inspection, Scribe staging, and the query reader.

| Boundary | Producer → consumer inspected | Assessment |
|---|---|---|
| Normal authentication, exact approve scope | HTTP `Caller` extractor and MCP caller → shared `VerificationControl::decide_approval`; runtime `Action::Approve`, permission parse/validation/coverage, builtin roles | No login-age or credential-type shortcut. Approve remains distinct from read/run; exact UID required. |
| Subject authority and audit | `retained` → exact Verifier identity → `authorize_subject` in `service.rs:1017–1054` | Checks normal effective permissions plus signed Card scope and stages allowed/denied audit. No new permission owner. |
| Tenant isolation | caller tenant → queue `TenantConn`, state recovery, `ApprovalDecisions::decided/accept`, SYSTEM query context | Tenant remains server-derived and carried through queue and Bifrost paths. No body-supplied tenant or identity override. |
| Decision attribution and replay | verified delegated chain → `Caller::attribution` → approval receipt construction and replay → Scribe row/inspection | **FAIL**, SEC-008-1 below. |
| Pending identity and stale work | accepted state approvals → occurrence lookup → queue open check → fenced decision → runner decision map → existing continuation | Decisions name an accepted occurrence, never request-authored work. Old executions have distinct IDs; terminal queue rows refuse fresh decisions. Existing generation/supersession fences remain the execution-release boundary. |
| Durable provenance and forgery | `ApprovalDecisions` → `ScribeWrite::Fenced` with caller attribution → reserved approvals table; public Gate | Internal-only destination added to Gate refusal list. Receipt is built server-side; schema accepts no actor/time/authority. |
| Authority after decision | runner `decisions` and `reader` → engine decision outputs; existing `run.authority` | Approver contributes a receipt, not permissions; continuation retains execution authority. |

## Verification evidence inspected

`pg_approvals.rs:214–389` exercises real HTTP decision requests: viewer fails 403; unrelated Verifier approve scope fails 403 (`249–255`); foreign-tenant admin fails 404 (`258–279`); unknown approval fails 404 (`281–290`); a direct editor receives its own principal ID (`292–301`); exact retry returns the original receipt (`303–307`); different decision, reason and direct principal fail 409 (`308–322`); inspection retains identity/time (`325–332`); scoped approver rejection yields a failed verdict, and audit assertions check allow/deny outcomes (`334–389`). These assertions establish the direct identity case, not delegated actor identity.

`pg_approvals.rs:402–457` checks old-head/new-head approval IDs differ, cancelled/timed-out occurrences show expired with null actor, and fresh decisions refuse 409. `a_decision_racing_supersession_releases_nothing` retains an approved receipt but asserts cancelled rather than passed after supersession. This is consistent with INV-006's existing old-frozen-revision interval; acceptance receipt is not authority to release a superseded execution.

`permission.rs:1483–1513` proves approve parsing, unrelated UID refusal, read/run noncoverage, and scoped wire decoding. Builtin-role tests assert workload lacks approve. Gate's existing all-principals reserved-table test adds `bifrost.verification.approvals` to its destination list (`gate/mod.rs:2778` onward).

No separate concurrent grant-revocation test was found. This is not a proposal to invent per-request database grant refresh: security posture explicitly says role revocation reaches the next token, the permissions claim is the only tenant authority, and verification resolves no roles. The task's current-authority language must be interpreted under that stronger normal-auth owner. Execution repository-access loss remains governed by its existing continuation boundary. No independent executable result is claimed here; orchestrator owns sequential verification.

## Proposed material finding

### SEC-008-1 — Delegated approvals record the subject as actor and let another actor replay the decision

- Classification: **INCORRECT**.
- Obligation: REQ-010 and contracts Human approval require a server-derived actor and same-actor/decision/reason replay, with other actors conflicting. Wyrd's existing identity owner distinguishes the verified effective subject from its current delegated actor: security posture Delegation and federation; `wyrd-spec/src/auth/attribution.rs:5–19`; `Caller::attribution` in `caller_extractor.rs:93–103`.
- Changed locations: `components/verification/service/approvals.rs:138` (receipt creation) and `:201` (`replay` actor comparison, also called after a concurrent custody race at `:162`).
- Producer and failing path: normal RFC 8693 exchange preserves subject S as `caller.principal.id` and records the current authorized Service actor A in the verified chain. `Caller::from_authenticated` preserves both (`caller_extractor.rs:55–63`); `Caller::attribution()` already derives `(S,A)`. With the normal required intersected read/approve permissions and subject Card access, a delegated caller reaches the approval service. The service nevertheless writes `actor_id = S`, while the same staged row's managed attribution records actual actor A (`approvals.rs:145–148`). A second appropriately authorized actor B acting for S sends the same decision/reason. `replay` compares only S, so B receives 200 as though it were the deciding actor. Even the direct subject S can replay A's decision. Both are different-actor replays under the existing attribution contract.
- Reachability/materiality: delegation is an explicitly supported ordinary credential flow, not an attacker-forged token or test-only caller. A queued manual approval against a Service subject covered by the actors' rooted Card scope suffices; the task marks all approval Verifiers queued-only, including manual invocations. Custom approve grants are supported and no credential-type prohibition is authorized. Durable receipt and inspection falsely identify who acted and the explicit conflict boundary collapses across actors sharing a subject. Authorization itself is still narrowed normally; this finding does not claim a permission escalation.
- Existing owner and correction: use the actor identity already resolved by `Caller::attribution().act_principal_id` consistently when constructing the receipt and comparing replay identity. Keep permission decisions on `caller.principal` and preserve the existing staged subject/actor pair, Bifrost fence, and continuation. Trace both normal and race-replay callers through the same comparison. No new login flow, token profile, storage column, or attribution abstraction is needed.
- Correction burden: two local uses of the wrong identity plus focused proof; changing delegation or rejecting delegated credentials would be larger and conflict with the normal-auth contract. A consumer-only inspection fix cannot repair the stored receipt or replay comparison.
- Focused closure proof: authorize a delegated approval using the existing token-exchange path; assert receipt and inspection actor equal the verified current actor, same actor retry returns the exact receipt, and another authorized actor with the same subject (and the direct subject) cannot replay it. Keep existing direct-principal success/replay and exact-scope refusal assertions. The existing `WyrdTestServer` delegation helpers and approval journey are sufficient.

## Limits

Reviewed source and assertions; did not run tests, benchmark, or alter production code. Sensitive-domain coverage is complete for this assigned scope; other specialist reports own lifecycle/persistence and broad repository gates. No optional hardening or additional authentication requirements are proposed.
