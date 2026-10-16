# Focused follow-up: signed subject scope correction boundary

Result: **RESOLVED**.

Immutable subject: base `f2a726c90edbf8956c7b91c843379175ee20eac0`, candidate `99882cf824e70a601033aba4cfa5c5b27a8dfc7c`; HEAD checked equal to candidate. Authority: approved r8 `changes/active/verifier-building-blocks/spec.md`, original `tasks/TASK-005-server-integration.md`, AGENTS.md, agent rules, Wyrd design and security posture. Owner clarification applies: nothing shipped; no historical-data migration or compatibility requirement is introduced. Only this report was written.

## Question and conclusion

SEC-01 proposes enforcing the signed UID scope synchronously at `VerificationControl::enqueue`; I-01 instead proposes resolving the subject Card and refusing at batched acceptance; BEH-01 permits an earlier control check with a registry read. All describe the same reachable missing authority decision. **Use the existing signed UID mapping at shared manual enqueue admission, before staging execution inputs or creating accepted work. No registry read and no asynchronous authorization outcome are necessary.** This restores the established admission boundary using current authority semantics and leaves the existing asynchronous target-availability/replay/conflict decisions intact.

Do not copy `Principal::authorizes_card` onto a fabricated CardRef: that method compares semantic identity and needs a real CardRef. The request already supplies the exact UID and the server already has the signed UID correlation mechanism.

## Source proof: authority production and transport

1. `wyrd-auth/src/card_scope.rs:53-109::resolve_card_ref_scope` resolves root and observation-target descendants in a tenant connection, attaches each row's exact `card_uid`, and retains the resolved root. `wyrd-auth/src/issuance.rs:616-633` uses this walk for Card-bound machine principals before issuance. This is not an inferred UID or a client-controlled lookup.
2. `wyrd-auth-issue/src/lib.rs:277-310::build_access_claims` preserves these scope members and the root when constructing signed access claims. `wyrd-spec/src/reference.rs:278-307` donates a resolved member UID to a duplicate authored root, so root deduplication does not discard its UID.
3. `wyrd-spec/src/reference.rs:358-374` serializes/deserializes scope through CardRef text; `CardRef::fmt` at :488-504 writes the optional `#uid`, and its parser at :511-542 restores it. The UID survives the actual token wire path, rather than existing only in an issuance-local struct.
4. `wyrd-auth-verify/src/lib.rs:830-882` projects verified machine claims into `PrincipalKind` and seeds an omitted scope from its signed root. `Principal::card_attribution` (`wyrd-runtime/src/principal.rs:215-234`) distinguishes bound Service/Agent/SYSTEM scopes from AnyRegistered human/admin/Card-free automation. `CardRefScope::uids` (`reference.rs:337-349`) explicitly defines the permitted row `card_uid` correlations: unresolved members contribute no UID.
5. Delegation does not authorize via the actor's graph. `wyrd-auth/src/issuance.rs:209-245::into_access_grant` retains the effective subject principal, intersects permissions, and puts the actor in attribution history with an empty actor scope. `Caller::from_authenticated` (`caller_extractor.rs:45-65`) takes effective principal, tenant, and chain from one verified result. The scope decision must use this effective principal; do not union actor/subject scopes or introduce another delegation profile.
6. `AcceptedAuthority::new` (`wyrd-runtime/src/request_context.rs:85-105`) captures that principal and chain, dropping only credential identity. Its serde representation preserves scope UIDs by the same CardRef encoding. Queue insertion stores the accepted authority; claim and `Caller::accepted` (`caller_extractor.rs:68-86`) restore it without expanding scope, with SYSTEM changing recorded executor only.

The identity-oriented `CardRefScope::authorizes` at `reference.rs:331-335` is not the sole scope API. `Principal::authorizes_card` delegates to it, while the UID-oriented request here is covered directly by the existing signed correlation API. `GatewayInvocation::attribute` (`components/gateway/invocation.rs:758-791`) already uses `CardAttribution::Scoped(scope) => scope.uids().contains(card_uid)` for precisely this boundary. Bifrost Gate's attribution path distinguishes scoped and unbound callers and passes signed `CardRefScope::uids()` downstream (`gate/attribution.rs:1-8,85-95`; `gate/mod.rs:804,843,882`). These owners establish that no new scope model or registry resolver is needed.

## Reachability and consumers

- HTTP enqueue (`components/verification/routes.rs:236-244`), MCP enqueue (`mcp/verification.rs:231`), and SDK submissions all converge on `VerificationControl::enqueue` (`service.rs:553-615`). It checks only exact Verifier invocation (`:569`), captures accepted authority (`:576-579`), and stages inputs with the requested subject UID (`:587-612`). A bound caller with the Verifier grant and unrelated same-tenant subject therefore passes.
- `VerifierExecutionQueue::accept` (`queries/verifier_executions.rs:1105-1172`) checks replay/conflict, resolves tenant-local target availability/readiness, and inserts pending without checking the accepted principal's subject scope. Tenant resolution is necessary but does not authorize the narrower graph attribution.
- `SubmissionSink::write` (`verification/submissions.rs:83-128`) commits those acceptance outcomes and logs refusals; it currently has no request permission/audit context. Adding an authorization decision here would require relocating the request decision and audit work unnecessarily.
- `VerifierRunner::dispatch` (`runner.rs:383-416`) restores accepted authority and sends `run.card_uid` as the engine's subject. A pure JSON/assert Verifier needs no protected model/tool call. `result` (`runner.rs:736-759`) builds the retained result with that same UID. Neither later domain permissions nor SYSTEM Scribe mechanics replace enqueue's missing subject admission. The gateway also distinguishes server-resolved attribution from caller-asserted attribution (`gateway/invocation.rs:112-126`), so gateway protection cannot be treated as a downstream substitute.
- Sibling authority consumers remain separate: direct execute has the explicit pre-existing scope exception in base service documentation; scheduled acceptance keeps binding-owner authority; public ingest/gateway own their existing attribution decisions. Do not change those behaviors to close this manual-enqueue regression.

## Error timing and audit

Approved INV-003 requires signed scope preservation; INV-004 and agent rules require permission decisions to block and stage non-blocking allow/deny audit. Spec REQ-013's no-Postgres-write request rule does not prohibit pure admission checks. Its 202 staging promise applies after authorization; it explicitly moves replay/conflict into asynchronous batched acceptance and does not require accepting unauthorized work into Scribe first. Existing enqueue already synchronously refuses missing exact Verifier permission.

Base `VerificationControl::start_run` evaluated scope before accepting work and returned `PermissionDeniedRbac` with a denied audit. Reuse that established denial category and the shared Scribe audit stage. Current `authorize_run` (`service.rs:767-798`) already audits its own exact-Verifier decision. The correction must produce an accurate denied subject-scope decision when that later check fails; it must not leave only an allowed audit or pretend queue mechanics performed a permission check. Private factoring of these existing decisions is implementation-owned.

For scoped callers, requested UIDs outside their signed mapping are refused before staging, without querying whether the UID exists. Existing async queue target/readiness checks still decide whether an authorized mapped UID remains available. Unbound callers keep existing registry/availability acceptance behavior. No new error code, public request field, storage row, route, authority refresh, or lifecycle is required.

## Reconciliation of discovery claims

| Claim | Disposition | Reason |
|---|---|---|
| SEC-01: synchronous signed UID admission | Supported | Exact UID request, signed mappings survive token/authority wire paths, and existing gateway uses the same mechanism. |
| I-01: must resolve a Card and authorize during batched acceptance | Correction revised | Its defect diagnosis is supported, but the prescribed read/async boundary is unnecessary. Signed UID membership already answers this request's authority question. Keep batched replay/availability decisions where they are. |
| BEH-01: shared control owner, real Card identity, read allowed | Boundary supported; read omitted | Shared control is the right owner. A registry read is allowed by the write prohibition but adds no required authority here. The exact signed UID is already available. |

No new independent finding is added. SEC-01/I-01/BEH-01 should consolidate into one signed-subject-scope finding during validation.

## Smallest closure proof

Use existing real authenticated route/SDK fixtures with a Card-bound workload, an exact grant for an assert-only Verifier, and both an in-scope Card and an unrelated active same-tenant Card. Out-of-scope enqueue must synchronously return the existing permission denial, record a denied scope decision through shared audit, and stage no execution inputs or queue work. In-scope enqueue succeeds and executes. Retain unbound-caller success and a delegated case demonstrating the effective subject's scope controls even when the actor's graph differs. Preserve existing asynchronous replay/conflict, authorized-target unavailability, direct scope exception, and scheduled behavior. Add no harness or broad aggregate solely for this correction; run exact focused tests and the narrow owning verification route lane.

Static source follow-up only; no gate or live provider checks rerun. Remaining risk is the confirmed reachable scope omission until corrected and independently validated; this report does not implement a fix or complete the task.
