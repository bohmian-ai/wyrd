# Security Audit

## Subject and reviewed boundary

- Base: `3fc085acf5b3a710d5dc80892bd2e664b3db6174`; candidate: `bae424cc647dad4be80e7debec976d0b7b3e4cf8` (HEAD at inspection). Approved spec revision 5 and original TASK-002 govern this review.
- Traced anonymous tenant login and callback through state, screened provider IO, ID-token verification, tenant User resolution, role replacement, sealed one-use completion, and refresh/revocation. Followed connection activation and the shared workload assertion verifier to their authorization and audit owners. Inspected the complete cumulative diff's security owners and the latest `454bdb90e..bae424cc6` correction diff.

## Authority and source coverage

| Boundary | Authority | Source inspected | Result |
|---|---|---|---|
| Tenant selection, OIDC state, callback, and provider trust | Spec REQ-006–008, INV-001/004; security posture; TASK-002 | `wyrd-auth/{login,callback,connections}.rs`, `wyrd-sql/queries/auth/login_state.rs`, server auth routes, screened HTTP owner | PASS |
| JWT and workload assertion separation | Spec REQ-007/013, INV-003/004; OIDC Core, RFC 7523, RFC 8725 requirements named in R10 packet | `wyrd-auth-verify/src/lib.rs`, `wyrd-auth/jwt_bearer.rs`, server JWT bearer route and verifier tests | PASS |
| Sealing and one-use completion | Spec rev 5 REQ-005, REQ-009/011, AC-007; R10 human-directed packet | `wyrd-auth/{connections,login,callback,sealing}.rs`, login-state SQL and Postgres tests, keyring boot | PASS |
| User role/session authority and lifecycle | Spec REQ-008/014/016, INV-002/003; security posture | callback, issuance, refresh, revoke, role SQL and identity journeys | PASS |
| Authorization audit for connection activation | Spec REQ-017; `AGENTS.md` audit rule; `architecture/agent-rules.md`; security posture | `wyrd-server/components/admin/identity.rs`, `wyrd-server/audit/mod.rs`, `wyrd-auth/connections.rs` | **FAIL** |
| Auth test governor retry | TASK-002 negative journeys; repository test rules | `wyrd-server/tests/identity_e2e.rs` and shared auth governor behavior | PASS |

### Critical

None.

### High

None.

### Medium

- **SEC-TASK-002-R11-1** `[crates/wyrd/wyrd-auth/src/connections.rs:469]` **VIOLATION — keyless activation loses the allowed authorization audit.** `activate_candidate` first evaluates `identity_connections:write` at `identity.rs:308`; `authorize_recording_denial` returns an *uncommitted* Allowed event (`audit/mod.rs:214-253`). The new `require_keyring()?` returns before `begin_locked` appends that event (`connections.rs:659-669`). An authorized administrator repeatedly attempting activation on a keyless deployment therefore gets `sealing_key_missing` with no canonical record of the allowed permission decision, contrary to REQ-017 and the transactional audit rule. Use the existing activation transaction and `commit_refusal` path to record the decision before returning the same keyless refusal; audit failure must itself refuse. Add a focused keyless activation check that inspects the canonical audit row and proves an injected append failure cannot activate the connection. The current test at `connections.rs:1322` checks only the error reason.

### Low / Defense In Depth

None.

### Positive Controls

- The shared verifier requires signed `exp`, `iss`, and `aud`, validates a present `nbf`, and keeps OIDC-only `iat` and Subject Identifier checks on the ID-token entry. Workload tests exercise a valid assertion and missing binding claims.
- Secretless-provider activation and login refuse a missing sealing key. Completed credentials are sealed, tenant and initiator bound, expiring, and deleted on redemption; retained keys open in-flight completions while a dropped key fails closed.
- Callback role replacement, initial issuance, refresh rotation, and User revocation use the tenant-qualified refresh-family lock. The revocation correction takes it before reading User status.
- Callback state chooses the tenant without a Host or provider fallback; provider calls are screened and pinned, and role/issuance work uses tenant RLS and canonical audit in one transaction.
- The journey retry helper retries only admission `429` responses after `retry-after`; the governor refuses before a handler can consume login state.

## Verification limits and prior closure

This was a static, review-only pass. I did not rerun Cargo, Postgres, Docker, or live IdP lanes. The R10 implementation packet records focused JWT, keyless, sealed-completion, refresh/revocation, identity-journey (27/27), lint, format, and tenant-isolation checks; those are reported evidence, not fresh execution. Fresh cumulative `git diff --check` passed. TASK-003 browser caller/cookie redemption, TASK-004 CLI verifier claim, and live-provider qualification remain downstream and are not claimed as TASK-002 proof.

The source still closes earlier security findings on exact OIDC subject and `azp`, advertised asymmetric algorithm, tenant state and RLS, role audit, refresh-family serialization, human-only refresh, and raw-pool ownership. R10 rustdoc findings 23/24 are outside this security domain and their corrected comments do not affect these paths. The new finding is limited to the R11 keyless activation precheck.

## Overall result

**FAIL** — one material audit gap remains in the keyless activation path.
