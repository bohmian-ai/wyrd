# Domain review: security, RBAC, and delegation

**Result: FAIL**

Subject: `b22718f2cf5da0ebcb7f26bc376157e5eec7768f` → `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa`.

Approved authority: `changes/active/sub-act-attribution/spec.md`, revision 4, and `TASK-001-sub-act-attribution.md`.

## Boundary and source coverage

| Boundary | Source inspected | Assessment |
|---|---|---|
| Verified subject/current actor and effective authority | `wyrd-auth-verify::VerifiedToken`; runtime `AcceptedAuthority`, `delegated_attribution`; server `AuthenticatedPrincipal`, `Caller` | Current actor comes from the last verified flattened actor. Execution actor changes leave permission authority intact. |
| Delegation permission composition | `wyrd-auth/src/issuance.rs`, `exchange_api_key.rs` | Existing intersection remains authoritative; prior actor history is attribution rather than another source of grants. |
| Realtime Verifier → judge → gateway | verification `service.rs`, `direct.rs`, `eval.rs`; gateway `workflow.rs`, `invocation.rs` | Captured effective principal reaches gateway permission checks; SYSTEM changes attribution without replacing authority. |
| Deferred/background acceptance | SQL `verifier_runs.rs`; verification `authority.rs`, `scheduler.rs`, `observations.rs`, `runner.rs` | Accepted authority persists without credential ID; background owners resolve on the tenant transaction; missing authority accepts no run. |
| Tokenless input-read capability | verification `authority.rs`, `drift.rs`, `eval.rs`; Oracle query context and audit construction | Table-scoped SYSTEM capability stays separate from subject authority. Originating delegation history is lost: SEC-002. |
| Ingest and internal forwarding | Bifrost Gate authentication/dispatch, `ScribeIngressFrame`; server `scribe_outbox.rs`, `grpc/capture_peer.rs`; Oracle serialized context | External pair derives from verified authentication. Internal capture RPC remains restricted to trusted peer transport and fixed destinations. Audit content remains distinct from transport identity. |
| Tenant/platform/unresolved audit | server `audit/mod.rs`; auth `audit.rs`, `jwt_bearer.rs`, `platform_authz.rs`, `issue_api_key.rs`, issuance/exchange audit producers | Platform and unresolved sentinel handling follow the intended separation. Delegated API-key completion audit incorrectly records direct attribution: SEC-001. |
| Supporting verification | Recorded task evidence; queued-authority, realtime-authority, retained-audit and timeline test source | Reported green checks do not exercise the two audit gaps below. |

Applicable rules inspected include the approved specification, repository agent rules, security posture, spec-driven development and maintainer guidance. Security conclusions use implementation source and the cumulative diff, rather than the completion summary.

## Security Audit

### Critical

None identified.

### High

None identified.

### Medium

**SEC-001 — INCORRECT: delegated API-key issuance completion audit falsely records direct work**

- **Location:** `crates/wyrd/wyrd-auth/src/issue_api_key.rs:149–172`, especially line 160; sole production caller at `crates/wyrd/wyrd-server/src/components/auth/routes.rs:604–609`.
- **Violated obligations:** REQ-002 delegated registration attribution; REQ-010 authenticated audit subject/actor and full delegation history; INV-001 verified attribution.
- **Producer-to-consumer evidence:** A verified delegated request produces an `AuthenticatedPrincipal` and `audited_caller` retaining A/B. The route correctly passes `audited_caller.attribution()` into `IssueApiKey::execute`, so the key’s creation row records A/B. After commit, however, it passes only `caller.principal()` to `IssueApiKey::audit`. That principal is A. The audit owner constructs `Attribution::direct(actor.id)`, yielding A/A, and its credential-issuance detail retains no delegation chain.
- **Observable consequence:** B can perform permitted credential issuance for A, but the credential-specific completion event falsely states that A performed it directly. The earlier permission decision has A/B, so retained audit rows for the same operation disagree. That earlier row does not repair the misleading completion event identifying the issued key.
- **Sibling/caller assessment:** `IssueApiKey::audit` has one production caller. The sibling creation path already carries the correct pair; general tenant audit construction already projects verified delegation. Other direct-grant audit producers should remain direct.
- **Smallest correction:** Preserve the existing verified attribution and chain through the credential-issuance audit owner instead of reconstructing direct attribution from `Principal`. Keep the existing credential-specific completion event, key ID, expiry, permission, resource, and nonblocking staging. Use the established typed audit-delegation projection and existing optional-chain convention on the credential detail; do not add another audit pipeline or token parsing.
- **Focused closure proof:** Exercise delegated A/B issuance through `/auth/issue-key`, read the issued key’s retained completion event, and assert A/B plus the verified actor history. Also assert direct issuance remains A/A, the creation row agrees, and no plaintext credential appears in audit.

**SEC-002 — MISSING: tokenless Verifier input-read audit loses accepted delegation history**

- **Location:** `crates/wyrd/wyrd-server/src/verification/authority.rs:172–177`; callers in `verification/drift.rs:896` and `verification/eval.rs:565–582`.
- **Violated obligations:** REQ-008 retained originating delegation history; REQ-010 full delegation history on authenticated audit decisions.
- **Producer-to-consumer evidence:** A manual delegated run durably retains `AcceptedAuthority { principal: A, delegation_chain: [B, …] }`. Drift passes only `run.authority.principal` to `SystemReadAuthority::serving`; Eval similarly accepts only the principal for reader construction. `SystemReadAuthority::resolve` creates a new query context with an empty chain, and `serving` adds only subject ID and kind. Oracle’s read-decision construction explicitly serializes `context.delegation_chain` at `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3401`; consequently it emits an empty history.
- **Observable consequence:** An ordinary queued manual run accepted as A/B produces input-read decisions attributed A/SYSTEM with no record of B or earlier originating actors, despite their presence in the accepted run. The loss occurs before local execution or authenticated forwarding, so either topology preserves the incomplete audit context.
- **Sibling/caller assessment:** Both Drift and Eval construct this capability through `SystemReadAuthority`; both lose the same information. Oracle already supports `AuthorizedQueryContext::with_delegation_chain`, serializes that context, and projects its chain into read-decision/security-violation detail. The shared producer is the correction boundary.
- **Smallest correction:** Carry the run’s accepted delegation history into the existing served SYSTEM read context, using Oracle’s existing delegation-chain mechanism. Keep subject A and execution actor SYSTEM; preserve the separate narrow table-scoped SYSTEM permission set. Do not replace it with A’s permissions, recover fresh permissions, or reinterpret historical actors as authority.
- **Focused closure proof:** Execute a delegated queued manual Drift run and inspect its retained input-read decision: A/SYSTEM and the exact accepted actor history. Cover the shared Eval reader construction with the same invariant. Assert the capability’s table grants remain unchanged and the populated context survives serialization.

### Low / Defense In Depth

No optional hardening recommendations.

### Positive Controls

- `AcceptedAuthority::new` removes credential ID while retaining the verified effective permission snapshot and originating chain.
- `Caller::executed_by_system` changes recorded execution actor without replacing the effective principal.
- Judge gateway calls reuse `GatewayInvocation`; model and fallback permission decisions inspect the captured principal.
- Background authority uses tenant-scoped identity resolution without observation-writer or SYSTEM-authority fallback.
- Tokenless Verifier reads retain explicit table-UID grants rather than gaining general query or gateway authority.
- Unresolved authentication audit uses the reserved sentinel pair; platform decisions preserve their actual platform principal.
- Audit writes remain on the existing nonblocking outbox path.

## Verification limits

This was static discovery against the cumulative candidate. HEAD remained `518a5d8b822a4e649a5fe90b14f3eb2b1fc8a0aa` at the final check. No runtime checks or Cargo commands were run. The task’s reported V1–V11 and static results are supporting evidence, not independently rerun results.

No source or report files were edited by the security reviewer. The orchestrator persisted this returned report. SEC-001 and SEC-002 concern audit accountability; neither establishes a permission escalation.
