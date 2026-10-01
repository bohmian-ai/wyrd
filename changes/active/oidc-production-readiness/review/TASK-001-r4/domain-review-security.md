# Security / RBAC / OIDC Domain Review

## Result

**FAIL**

The candidate closes the prior live-login TLS, recovery-decision audit, and DNS-deadline findings. One material SSRF-policy gap remains: the OIDC screen's permissive profile admits two cloud instance-metadata addresses that Wyrd's security authority requires blocked in every profile.

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-complete`
- Base: `a5c8041a348a66bfb56fbac492383e8c688b0590`
- Candidate: `c2787b37d456cd2eeeee01e04a9f8bbfdf8866e5`
- Candidate tree: `e389f56318a664920b5ebead0aee45d20fda6b23`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 4
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-001-tenant-connections.md`
- Remediation inputs: `TASK-001-R1-production-readiness-gaps.md`, `TASK-001-R2-remaining-production-readiness-gaps.md`, and `TASK-001-R3-production-readiness-gaps.md`

The candidate commit and tree matched the supplied immutable subject before this report was written.

## Reviewed boundary

This review traced the cumulative tenant OIDC boundary from the authenticated administration routes through `identity_connections:write`, canonical allow/deny audit, tenant-scoped connection persistence, recovery-key verification, candidate qualification, active login and callback, exact connection-revision issuance and refresh cutoff, secret loading/sealing, and every shared provider fetch. It followed discovery, authorization destination, token exchange, JWKS refresh, redirects, proxy use, DNS resolution and pinning, address classification, TLS policy, response bounds, and public/log error redaction.

## Authority and source coverage

| Boundary | Authority | Source inspected | Result |
|---|---|---|---|
| Bearer-derived tenancy and route permission | `AGENTS.md` §§2, 9; security posture authorization and tenant-isolation rules; TASK-001 | `components/admin/identity.rs`; `connections.rs`; human-connection SQL/RLS and migration; journey source | PASS |
| Canonical audit and fail-closed mutation | `AGENTS.md`; `architecture/agent-rules.md`; REQ-017 | administration decisions; `begin_locked`; `recovery_key_authorizes`; activation rollback and attribution assertions | PASS |
| Recovery credential | TASK-001 activation contract; security posture credential attribution | `verify_api_key`; role resolution; recovery allow/deny event; rotation journey | PASS |
| Login/callback/session provenance | REQ-003/004; INV-003/004; R1/R2/R3 remediation | `begin_login`; `AuthorizationCodeExchange`; `TenantTokenIssuer`; `RefreshTokens`; login-state and refresh SQL | PASS |
| Provider scheme, redirect, proxy, DNS and address controls | INV-004; agent rules' SSRF requirements; security posture “Source credentials and SSRF defense” | `screening.rs`; `provider.rs`; `jwks.rs`; candidate probes; callback exchange; server profile selection | **FAIL (`SEC-R4-001`)** |
| Provider response and operation bounds | INV-004; R2/R3 remediation | fixed DNS/fetch deadline; `read_bounded_body`; all discovery/JWKS/token/probe consumers | PASS |
| Secret handling | REQ-005; security posture secret rules | connection wire views; sealed storage/rewrap; recovery input tracing; signing/sealing `read_secret_file`; redacted error mapping | PASS |
| Prior ledgers | R1/R2/R3 validated findings | prior findings-validation/verdict/remediation artifacts and cumulative candidate source | PASS except the newly identified address-classification gap below |

## Prior-finding closure

- `FIND-TASK-001-1` through `FIND-TASK-001-19`: the security-relevant callback qualification, client-auth probe, configured callback, proxy bypass, session provenance/cutoff, candidate-test decision, secret-presence, production scheme, and body-bound corrections remain present in the cumulative candidate.
- `FIND-TASK-001-20`: closed. `HumanConnections::begin_login` now applies the deployment scheme rule to the freshly discovered browser authorization endpoint before state insertion.
- `FIND-TASK-001-21`: closed by source inspection; the fallible bounded-body test helper has the required `# Errors` contract.
- `FIND-TASK-001-22`: closed. Activation appends an attributed Allowed or Denied decision for every resolved active recovery principal that reaches permission evaluation, in the locked mutation transaction; unresolved credentials retain indistinguishable handling.
- `FIND-TASK-001-23`: closed. The single system DNS lookup is covered by the existing fixed fetch deadline and maps timeout to the redacted unresolved result without changing the pinned answer set.

## Material finding

### SEC-R4-001 — VIOLATION / Medium: permissive OIDC screening admits cloud metadata addresses required to be blocked in every profile

- **Violated obligation:** `architecture/agent-rules.md` requires cloud-metadata and link-local addresses blocked in every profile. `architecture/wyrd-security-posture.md` repeats that cloud metadata is always rejected, while only ordinary loopback/private/CGNAT/ULA ranges may be admitted outside Production. INV-004 and TASK-001 require that shared rule on every issuer, discovery, JWKS, and token fetch.
- **Location:** `crates/shared/wyrd-auth-oidc/src/screening.rs:287-307`, reached through `ScreenedHttp::client_for` at `:138-181`; policy selection is `crates/wyrd/wyrd-server/src/config.rs:138-151`. The existing correct repository classifier is `crates/wyrd/wyrd-gateway/src/endpoint.rs:141-159` with proof at `:226-244`.
- **Evidence:** `is_always_blocked` recognizes IPv4/IPv6 link-local, broadcast, and documentation addresses, but not Alibaba metadata `100.100.100.200` or AWS IMDS IPv6 `fd00:ec2::254`. Those addresses are caught only incidentally by `is_cgnat`/`is_unique_local_v6`, which `ScreenedHttp::is_blocked` applies only under `BlockInternal`. `AllowInternal` therefore admits both. The server deliberately selects `AllowInternal` for the default Development profile and documents it for self-hosted/enterprise providers. In contrast, the gateway's existing `always_blocked` names these same two metadata addresses exactly while leaving neighboring CGNAT/ULA addresses profile-dependent.
- **Reachable scenario and consequence:** an authorized tenant connection administrator on a development, self-hosted, or enterprise deployment can configure an issuer or a public discovery document whose effective JWKS/token URL targets either metadata address. Candidate testing and later login/callback refreshes build a client and send the server-side request because the permissive screen approves the literal address. The response is not directly returned, limiting this to blind SSRF, but it still permits requests into cloud credential/identity control planes, leaks reachability through timing and outcome, and can trigger any state-changing or compatibility endpoint accepting the emitted GET/POST. This crosses from tenant authority to deployment/cloud authority and violates the explicit always-block boundary.
- **Required testable correction:** in the shared OIDC `is_always_blocked` owner, add the exact metadata addresses `100.100.100.200` and `fd00:ec2::254` (after the existing IPv4-mapped normalization), matching the already-established gateway policy. Do not move all CGNAT/ULA into the always-block set: neighboring internal addresses must remain available under `AllowInternal`. Extend the existing address-policy unit test to prove both native and IPv4-mapped metadata spellings are refused under both policies while adjacent `100.100.100.201` and `fd00:ec2::253` remain permitted only under `AllowInternal`; retain the candidate/provider journey coverage.

## Security audit summary

### Critical

None.

### High

None.

### Medium

- `SEC-R4-001` — permissive provider screening does not always reject two cloud metadata addresses.

### Low / Defense in Depth

None reported; optional hardening and unrelated debt were excluded.

### Positive Controls

- Every connection route takes tenancy from a verified bearer and performs tenant persistence through `TenantConn`/RLS.
- The dedicated permission is distinct from service-account administration; provider IO follows a committed decision, and candidate stamping requires a fresh transactional decision.
- Activation verifies a same-tenant live recovery API key at constant refusal cost and records its resolved principal/credential permission decision in the mutation transaction.
- Provider clients disable redirects and ambient proxies, resolve once under a deadline, reject any blocked answer, pin the accepted address set, and apply a fixed total request timeout.
- Production server fetches and live browser authorization destinations require HTTPS; the permissive policy retains only the intended local-provider HTTP behavior.
- Discovery, JWKS, token exchange, and candidate probes share the decoded 1 MiB response cap.
- Login state is random, single-use, expiring, and binds nonce, PKCE verifier, configured callback, issuer, and exact connection revision. Issuance and refresh recheck that revision under the tenant slot lock.
- Client secrets, recovery keys, signing keys, and sealing keys are excluded from response/audit/tracing fields; mounted signing and sealing files use the bounded open-handle owner-only reader.

## Verification limits

- This was a time-bounded static review. Per assignment, no Cargo or `mise` lane was run; recorded implementation evidence and test source were inspected but not independently executed.
- No controlled Okta, Entra, Keycloak, cloud metadata, DNS rebinding, or ambient-proxy environment was exercised.
- The complete cumulative diff is broad. Inspection focused on the stated security/RBAC/OIDC trust boundaries and their direct callers; unrelated Bifrost, tooling, documentation, and repository-style changes were not audited for security except where they affected those boundaries.
- The response-side impact of `SEC-R4-001` is blind SSRF in the inspected paths; no direct response exfiltration path was established. The finding is retained because the explicit architecture contract forbids cloud metadata reachability in every profile and the request crosses a deployment authority boundary.
