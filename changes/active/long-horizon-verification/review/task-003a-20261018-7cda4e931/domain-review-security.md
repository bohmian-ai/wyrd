# Domain review — verification security

**Result: PASS** — no material security finding in the reviewed authorization, identity, source-selection, or reaction boundaries.

## Immutable subject and scope

Reviewed `22335c2c40a5ad24c27bfa0d4686f4da49f922c5...7cda4e93159ab328de2a74068d5e1978a2590818`, the original TASK-003a at the base commit, and approved specification revision 17. HEAD remained the candidate; the working tree had no tracked modification. This review writes only this report. No other review conclusions or superseded packets were consulted.

The security boundary reviewed is authenticated source invocation → recorded owner/subject/Verifier/group attribution → acknowledged source scheduling → binding-owner authority → exact tenant/source snapshot → pinned execution → frozen reactions. SQL implementation details are assessed here only for the resulting authority and isolation consequences; the separate SQL domain audit owns RLS/migration mechanics.

## Authority coverage

| Authority | Applicable obligations |
|---|---|
| AGENTS.md §§2, 4, 9–11 and architecture/agent-rules.md | Server-owned authority, exact identities, tenant capabilities, nonblocking audit, no widened client authority, portable bounded execution. |
| architecture/wyrd-security-posture.md, Security principles, Trust boundaries, Principal and credential lifecycle, Authorization and policy, Peer identity, Tenant and data isolation, Audit integrity and privacy | Verified credentials supply tenancy/owner; SYSTEM is attribution rather than additional authority; accepted authority survives execution; permissions precede protected work; audit staging never blocks requests. |
| architecture/wyrd-design.md, Runtime identity/Observation identity, Verifier, Trigger and Operator | Exact source identity and owner scope; one snapshot under ordinary query authorization; frozen execution and reactions; no direct-verification dispatch. |
| architecture/bifrost-design.md, Durability and visibility, Query/Read audit and terminal contract | ACK is not completeness; ordinary query admission and failure semantics; incomplete/degraded source data cannot become a successful snapshot; audit through the existing outbox. |
| architecture/references/README.md; doctrine/architecture-constraints.md; domain/evaluation.md; languages/agent-harness.md; languages/spec-driven-development.md; languages/maintainer-style.md | Server/domain ownership, sensitive evidence access, agent tool authorization, typed contracts, bounded reads, immutable review discipline and minimum correction scope. |
| Spec revision 17 INV-003–005, REQ-001–006, REQ-010–014; original TASK-003a A1/A4/A5/A7/A8/A11 | No producer owner lookup or new attribution input; exact eligible tuples; accepted owner authority; pinned retries; isolated verdict-change reactions; existing audit and staging behavior retained. |

## Source and boundary coverage

| Boundary | Source evidence and assessment | Result |
|---|---|---|
| Producer authentication and attribution | `components/verification/service.rs:473–749`: direct execution authorizes before running/staging; enqueue authorizes before producing its submission or staging evidence. `owner_card_uid` comes from `caller.principal.owner_service_uid()` for both input and result construction. `wyrd-runtime/src/principal.rs:199–214` permits only a resolved Card-bound Service owner, with no parent inference. The request contract gains no caller-asserted owner, tenant, binding or principal. Runner results retain the accepted principal's owner for submitted work (`verification/runner.rs:825–876`). | PASS |
| Post-ACK source scheduling | `scribe_outbox.rs:276–297, 765–805, 1014–1064`: only input/result variants yield validated typed activity; ownerless rows yield none. Scheduling follows acknowledged frames and excludes lost items. It completes before submission reservations are consumed. `verifier_executions.rs:216–238, 1706–1734`: bound arrays match the recorded exact owner, subject and source-Verifier against frozen source declarations under the slice's TenantConn. No raw payload selects the tenant. Audit, capture and server spans cannot schedule verification. | PASS |
| Exact binding/source authority | `components/cards/service.rs:1315–1445` freezes the resolved Trigger and source UIDs under the containing occurrence; the modified branch changes only the duration shape. `verifier_executions.rs:1750–1784` resolves the pending binding's activity and owning principal. `queries/verification.rs:539–565` retains the exact active owner/version and activity gate. `verification/authority.rs:260–301` recovers the current owner inside that tenant and requires SYSTEM; missing authority creates no execution and never substitutes another principal. | PASS |
| Snapshot query trust boundary | `verification/groups.rs:99–140, 224–315`: the caller is the binding owner's accepted authority with SYSTEM solely as executor. Every input/result arm filters the exact owner and subject/Verifier tuples plus the exact group. `ObservationWindow::text` uses SQL AST string-literal rendering (`verification/drift.rs:139–142`); no caller value becomes SQL syntax. Tenant scope is carried by the ordinary authenticated query context, independently of those filters. | PASS |
| Permission before evidence | `query/scheduled.rs:74–87, 137–159` and its dispatch path retain the ordinary public query service for an authenticated caller. `query/service.rs:80–99, 177–229` builds the context from the accepted principal, checks query capability and delegates resolved-object authorization to Oracle before source reads. No new privileged SYSTEM query context is used by grouping. The changed test-support stall does not replace production authorization. Optional check-Agent queries remain under accepted tool/query permissions. | PASS |
| No partial or unpinned judgment | `verification/groups.rs:164–190` discards an unsuccessful/degraded read; pinning follows the one successful statement. `Visible::executions` and `Visible::pin` preserve exact source IDs and aligned values. `verifier_executions.rs:1040–1099, 1820–1871, 2473–2506` computes/persists snapshot identity and reconstructs grouped work from stored inputs. `verification/runner.rs:442–472` evaluates those pinned inputs under stored authority and does not reread or widen source identity. | PASS |
| Reaction authority and data disclosure | `verifier_executions.rs:291–307, 1970–2045` compares only another settled group result for the same binding/group, retains lease fencing, suppresses replay dispatch and uses frozen Operator targets. `verification/engines.rs:40–57` emits verdict/count summaries rather than raw evidence. Operator delivery authority, credential screening and bounded context are retained; `operators.rs` changes only construction of the existing claim capacity. Direct verification has no dispatch path. | PASS |
| Nonblocking audit and sibling constructors | Production boot supplies the PostgreSQL owner to the existing outbox (`boot/mod.rs:880–889`), and AppState shares that same outbox (`state.rs:2276–2291`). Audited operations still call synchronous `stage`; background scheduling failure cannot change an already made permission decision. `oracle/peer_audit.rs:35–108, 127` preserves the durable sentinel check and system-versus-verified-tenant audit routing; its changed constructor passes `None` only to the recording fixture. `components/gateway/capture.rs:1509` likewise changes a recording-only constructor, with capture writes excluded from group activity. The changed publication-fault wrapper passes the real PostgreSQL owner, and the straddle fixture deliberately has no group ID. | PASS |
| Contracts and supporting surfaces | The range's Trigger, GroupSnapshot, OpenAPI, generated TypeScript, authored Python stub, config, fixture and journey changes remove the old grouping controls/projections. They add no new authentication surface, secret source, redirect, arbitrary destination, unsafe deserializer or external dependency. Public invocation and MCP write-scope checks remain with their existing owners. | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

None in this task's security boundary.

### Low / Defense In Depth

No optional hardening requested. Existing unchanged security debt is outside this acceptance audit.

### Positive Controls

- Owner metadata comes from the authenticated accepted principal, not caller input or inferred routing.
- Pending activity and snapshot SQL match exact pinned owner/subject/source-Verifier identities; group ID alone grants no access.
- Snapshot reads retain ordinary query authorization, tenant binding, terminal validation and denial audit.
- Pinned values and accepted authority survive queue claim/retry without role refresh or SYSTEM authority escalation.
- Only matching settled binding/group transitions create frozen, bounded Operator contexts; replay creates no second transition.
- Peer audit and gateway capture retain the process outbox and their original attribution boundaries.

## Verification evidence and limits

Static review inspected the changed security owners and immediate producer/consumer paths, existing authority recovery/query services, complete bodies of the changed snapshot/scheduling methods, changed constructor consumers, and relevant journey/integration assertions. CodeGraph was used before code navigation. This reviewer launched no tests. The orchestrator independently reran C13 with tracing against the immutable candidate: exactly one selected server test passed, plus the setup's two migration-idempotence tests; see `verification.md` in this directory for the exact command and nextest run identity. This corroborates C13's asserted paths, not unasserted negative variants.

Candidate evidence reports C1–C6/C11/C12 Rust journeys (9 selected), C1–C5/C11/C12 Python (11 cases), TypeScript equivalents, C7 OpenAPI, C8/C13 group integration, C9 settlement and C10 MCP passing. Inspected assertions substantiate ownerless exclusion and Verifier invocation denial in `grouped_support_resumes`, live tool/query denial without source disclosure in `grouped_coding_uses_declared_sources`, and query-failure rollback/no partial snapshot in the group integration tests. The separate existing `grouped_reads_need_the_owner_grant` journey explicitly verifies owner query denial and unrelated trace-only scope, but is not among the nine Rust selectors reported as run; its existence is supporting source evidence only. C13's inspected body directly tests scheduling/claims/capacity/arrival behavior; do not infer every wrong-tuple variant from its reported pass. Exact tuple exclusion is additionally established from the production predicates and existing SDK isolation paths.

No production deployment, adversarial network test, live-provider test, dependency-vulnerability scan or performance benchmark was performed. No manifest/lockfile changes require a new dependency audit in this range. REQ-015/AC-009 remain pending. Revision-17 ACK visibility, post-ACK notification loss, outbox loss/retention, postponement, overlapping snapshots and durable-backlog limits are accepted semantics and are not security findings.

**Final result: PASS.** Proposed material finding ledger: empty. This is a scoped static acceptance result, not a claim that the entire repository or deployment is free of vulnerabilities.
