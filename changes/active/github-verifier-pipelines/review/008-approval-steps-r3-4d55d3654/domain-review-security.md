# TASK-008 round 3 security review

Result: **PASS**. FIND-008-4 is **CLOSED**. No new material security finding is proposed.

## Immutable subject and scope

Candidate: `4d55d3654ab7361b97aeadbb72b8813707515d49`; prior candidate: `cac4a4ce3ba4b6c41d23e81e927f6afbe1de284a`; original cumulative base: `cd0b8e18bd5737267f13c8e72110a06d379d7a0b`. HEAD was checked and remained the candidate during this audit.

Reviewed the remediation delta and affected security paths using the shared `scope.md` map. Acceptance context includes the original task, approved packet spec REQ-010/011/015, contracts Human approval, r1/r2 security evidence and r2 remediation. Governing authority includes root AGENTS, `agent_docs/errors.md`, and `architecture/wyrd-security-posture.md` accepted execution authority, Permission model and Permission check. CodeGraph preceded targeted source inspection.

The maintainer's settled decision governs: caller Card scope is attribution only; RBAC plus audit authorize verification; approval requires only `verifier:approve` on the exact Verifier; users and service accounts are approved actors. This audit does not reopen that decision or prescribe a credential-type restriction.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

No optional hardening or unrelated recommendations proposed.

### Positive Controls

- Exact Verifier authorization remains distinct from coarse operation admission. `inspection.rs:458–502` obtains the tenant-scoped execution identity and calls the ordinary audited authorization owner with `PermissionScope::Verifier(verifier)` and the requested action. `require_verifier` is only coarse admission; possessing approve on a different Verifier cannot authorize the selected execution.
- Decisions pass `Action::Approve` to that existing reader (`service/approvals.rs:78–80`). The read-only repository permission branch is skipped only for non-read action (`inspection.rs:503–524`); inspection continues using `Action::Read`. This implements approve-only decisions without broadening ordinary inspection.
- Decision authorization occurs before occurrence lookup, accepted-decision replay or new acceptance. Both retained replay and concurrent-winner replay use the same actor/status/reason comparison (`service/approvals.rs:110–113,179–191,224–236`).
- Receipt actor and custody attribution derive from verified `Caller::attribution()` (`service/approvals.rs:162–177`). `Caller::from_authenticated` takes the verified delegation chain, not request arguments, and attribution selects the verified current actor (`caller_extractor.rs:55–64,93–104`). Authorization continues using the effective principal.
- Tenant is derived from the authenticated caller. Retained queue reads, accepted state and approval custody remain tenant-bound (`inspection.rs:461–478,534–536`; `service/approvals.rs:108–109,118–124`). A foreign tenant's administrator cannot select another tenant's execution.
- The existing Scribe custody owner retains first-wins fencing and acknowledgement before a positive receipt (`verification/approvals.rs:124–165`). A committed fence without a visible decision remains unavailable rather than absence (`:72–110`). Existing r1/r2 evidence covers unchanged custody, error redaction and recovery behavior.
- Execute, enqueue and deferred evidence now use RBAC without the removed Card-scope authorization condition. Exact run permission is checked before execution reservation or submission (`service.rs:530–543,757–774`); each eligible evidence subscriber is checked before retention (`service/evidence.rs:235–276`). `authorize_verifier` retains normal PermissionCheck and nonblocking allow/deny audit staging (`service.rs:1008–1031`).
- Approval audit uses the existing `audit::authorize` owner, staging allowance or denial through the process Scribe outbox (`audit/mod.rs:109–133,146 onward`). Audit attribution includes verified subject, current actor and delegation history (`:35–75`). No alternate authorization owner was introduced.
- HTTP decodes typed path/body arguments and calls `VerificationControl::decide_approval` with authenticated Caller (`routes.rs:722–734`). MCP dispatch obtains its authenticated Caller and calls the same service (`mcp/mod.rs:455–466`; `mcp/verification.rs:491–503`). Tool visibility is coarse admission, not a substitute for exact service authorization.

## Acceptance and closure matrix

| Obligation / boundary | Source and observable proof inspected | Result |
| --- | --- | --- |
| FIND-008-4: ordinary delegated acceptance records the actual authenticated actor | `pg_approvals.rs:912–947` bootstraps a Service, obtains its ordinary rooted credential, delegates the user's token through the existing exchange owner, decides the real pending occurrence, and asserts receipt actor equals the Service's principal ID. Production stamps verified actor at `service/approvals.rs:165`. | PASS; CLOSED |
| Persisted inspection retains that actor | The connected test reads the execution again and asserts the inspected occurrence actor is the same Service (`pg_approvals.rs:963–968`). The assertion exercises accepted custody rather than a manually constructed expected receipt. | PASS |
| Identical actor retry replays; another actor sharing the subject and direct subject conflict | Connected retry asserts complete receipt equality (`pg_approvals.rs:948–952`). A second separately bootstrapped/exchanged Service acting for the same user, and the user directly, each receive typed 409 `WYRD_VERIFICATION_409_APPROVAL_NOT_PENDING` (`:954–961`). Shared production replay compares verified actor, decision and reason. | PASS |
| Only exact verifier:approve is required, audited | The second approver's seeded role contains only `Permission::verifier_approve(verifier)` (`pg_approvals.rs:375–385`), then successfully rejects the occurrence and produces an allowed audit (`:403–418`). No verifier:read or cards:read is required. Wrong-Verifier approve is refused (`:276–306`), viewer is refused (`:265–274`), and foreign tenant administrator receives 404 (`:308–330`). | PASS |
| RBAC-only direct and queued execution preserves denial before staging | Changed HTTP enqueue test accepts rooted, unrelated, delegated and user callers holding the selected run grant; a different-Verifier grant receives 403, denied audit, and no extra submission. Direct HTTP and MCP tests accept rooted/rootless permitted callers and refuse a viewer lacking run. Source retains exact checks before staging. | PASS |
| Deferred evidence uses exact subscribing Verifier run authority | Changed call removes only Card attribution authorization; existing per-subscriber permission check, target resolution, slot conformance and artifact admission remain (`service/evidence.rs:235–276`). | PASS |
| Decision adds no authority to resumed work | Decision receipt stores actor/status/reason; acceptance wakes the original execution. Submission still retains accepted effective authority (`service.rs:771–774`). Carry forward r1/r2 accepted authority, restart, supersession and execution-state evidence. | PASS |

The changed tests replace expectations for a security condition the maintainer explicitly removed. Their positive unrelated/rootless cases and negative no-run/wrong-Verifier cases align with the settled boundary; this is not an unexplained weakening of an authorized control.

## Coverage and limits

Reviewed production authorization, audit, identity, replay, tenant and HTTP/MCP dispatch paths and the connected proof assertions. No new dependency, cryptography, token profile, public identity input or alternate authentication mechanism appears in this delta. Unchanged fenced custody, redaction, supersession and restart security evidence carries forward from r1/r2; no changed dependency invalidates that evidence.

No Cargo commands, source edits or runtime exploitation were performed by this reviewer. The orchestrator owns current test execution. This PASS is the scoped security review result and does not claim broad SDK, outage or performance qualification.
