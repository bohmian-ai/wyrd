# Domain Review — Security and OAuth

## Overall Result

**PASS** — the cumulative candidate satisfies TASK-010's OAuth and OIDC trust-boundary obligations, the prior security findings remain closed, and the superseding lead direction closes `FIND-TASK-010-10` in the conventional operator-owned way. The official image no longer carries a misleading replica-local user-code limiter, the startup lane no longer claims to prove that limiter, and the self-hosting guide tells the operator to rate-limit only `POST /auth/device` per client address at the public ingress. No application limiter, forwarded-address parser, edge manifest, rate-limit option, or persistent admission state was added.

`FIND-TASK-010-1` remains routed to TASK-011 by `review/TASK-010-r1/lead-direction-FIND-TASK-010-1.md` and was not reopened.

## Reviewed Boundary

Immutable subject:

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation and verdict: `review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`, `review/TASK-010-r1/verdict.md`
- R2 verdict and superseding direction: `review/TASK-010-r2/verdict.md`, `review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`

The candidate remained `HEAD` before and after review. `.codegraph/` is absent, so source, immutable Git diffs, callers, and tests were inspected directly.

The security review traced the complete cumulative path through:

- `/auth/authorize`, callback processing, authorization-code redemption, exact redirect binding, PKCE S256, one-use state/code handling, and provider issuer binding;
- `/auth/device_authorization`, `/auth/device`, device approval/denial, polling cadence, redemption-time issuance, and user-code brute-force ownership;
- `/auth/token` client identification and authorization-code, refresh, device-code, RFC 8693 token-exchange, API-key, and RFC 7523 JWT-bearer dispatch;
- `/auth/revoke`, refresh-client binding, confidential non-rotation, public rotation, replay containment, connection lifecycle fencing, and canonical audit;
- the existing shared external-token verifier for OIDC Core, RFC 7523, and RFC 8725-relevant signature, issuer, audience, expiry, `nbf`, asymmetric-algorithm, and `kid` checks;
- SQL-backed login, device, authorization-code, and refresh state under tenant RLS; and
- the R3 deployment correction in the official-image NGINX template, startup proof, and operator documentation.

## Authority and Source Coverage

| Authority or source | Security/OAuth coverage and conclusion |
|---|---|
| Approved spec revision 11 | Applied REQ-005, REQ-006, REQ-009, REQ-011, REQ-012, REQ-013, REQ-016, REQ-017, REQ-021; INV-001, INV-005, INV-007; AC-004, AC-005, AC-007, AC-009. No tenant, role, token-content, or audit ownership change was found. |
| Original TASK-010 and R1/R2 records | Rechecked the cumulative implementation and source closure of `FIND-TASK-010-2` through `-13`. `FIND-TASK-010-1` remains routed; the R2 task is superseded by lead direction for `FIND-TASK-010-10`. |
| `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md` | Applied verified-credential tenancy, RLS, one issuer/permission authority, secret redaction, fail-closed issuance, canonical audit, and narrow verification rules. |
| RFC 6749, RFC 7636, RFC 7009, RFC 8628, RFC 8693, RFC 7523, RFC 8414, RFC 9207, RFC 9700 | Checked the task-selected sections at the authorization, client-authentication, token, revocation, device, metadata, provider callback, and refresh boundaries. |
| OIDC Core and RFC 8725-relevant validation | Human ID-token verification stays in the installed `openidconnect` relying-party owner. Workload assertions reuse `ExternalVerifier::verify_external`; `crates/shared/wyrd-auth-verify/src/lib.rs:428-558` resolves the configured `(tenant, issuer)`, requires signature-valid `iss`, `aud`, and `exp`, validates a present `nbf`, requires `kid`, rejects symmetric algorithms, and verifies against screened JWKS. TASK-010 adds no second verifier or invented JWT profile. |
| Authorization-server wire and handlers | Reviewed `wyrd-spec/src/auth/{token,device,oidc}.rs`, `wyrd-server/src/auth/{authorize,oauth,callback,cli_login}.rs`, and `wyrd-server/src/components/auth/routes.rs`, including duplicate parameters, Basic parsing, no-store responses, safe redirects, grant/client pairing, and secret-bearing instrumentation. |
| Durable auth owners | Reviewed `wyrd-auth/src/{callback,cli_logins,refresh,issuance,exchange_api_key,jwt_bearer}.rs` and the corresponding `wyrd-sql` query owners for digest lookup, RLS, locks, lifecycle fencing, one-use redemption, and audit/issuance transaction boundaries. |
| Deployment authority and lead direction | `architecture/operations/deployment-and-release.md:21-24` assigns rate-limit integration to the gateway. `docker/official/extras/nginx/nginx.conf.template:35-62` has no device limiter; `scripts/server/test-startup.sh:5-14` no longer claims one; `docs/src/content/docs/self-hosting/sso-and-oidc.svx:19-24` gives the exact operator action and endpoint without shipping edge policy. |

## Prior-Finding Closure

| Finding | Security/OAuth conclusion |
|---|---|
| `FIND-TASK-010-1` | **ROUTED.** TASK-011 owns the production `openid-client` 6.8.8 proof; not reopened. |
| `FIND-TASK-010-2` | **CLOSED.** Deterministic denial and expiry/delete interleavings retain redemption as the only mint point. |
| `FIND-TASK-010-3` | **CLOSED.** Unsupported RFC 8693 audiences are classified as `invalid_target` before generic decoding. |
| `FIND-TASK-010-4` | **CLOSED.** Client and exact registered redirect are uniquely established before a redirect can carry a non-binding parameter error. |
| `FIND-TASK-010-5` | **CLOSED.** Provider refusal consumes bound state once, applies issuer binding, mints nothing, and returns a downstream OAuth error without reflecting provider detail. |
| `FIND-TASK-010-6` | **CLOSED.** HTTP Basic scheme matching is ASCII case-insensitive; decoding, client identity, secret verification, and conflicting form-client refusal remain constrained. |
| `FIND-TASK-010-7` | **CLOSED.** The cited OAuth items have substantive rustdoc and no suppression was added. |
| `FIND-TASK-010-8` | **CLOSED.** Refresh lookup relies on `TenantConn` RLS rather than a second manual tenant selector. |
| `FIND-TASK-010-9` | **CLOSED.** The served OpenAPI contract publishes public form client identification and confidential HTTP Basic alternatives. |
| `FIND-TASK-010-10` | **CLOSED BY LEAD DIRECTION.** The incorrect image-local NGINX mechanism and its startup assertions are deleted. The operator note identifies `POST /auth/device`, RFC 8628 section 5.1, the public ingress, and per-client-address limiting. No edge manifest or application limiter is required or present. |
| `FIND-TASK-010-11` | **CLOSED.** Only a row whose revocation reason is `rotated` triggers chain-scoped replay containment; ordinary expiry, logout, and prior containment do not widen revocation. |
| `FIND-TASK-010-12` | **CLOSED.** Callback completion takes the existing family and connection-slot fences and rechecks the exact active binding before durable identity, role, code/approval, audit, and commit effects. |
| `FIND-TASK-010-13` | **CLOSED.** Every successful non-test callback appends `auth.login` in the final tenant transaction even when roles do not change; append failure prevents final effects. |

## Acceptance Assessment

| Security or trust-boundary obligation | Source and proof evidence | Result |
|---|---|---|
| Authorization code is short-lived, digest-only, one-use, and bound to client, exact redirect, tenant, principal, connection, and S256 verifier | `wyrd-auth/src/callback.rs:478-573`; authorization and callback refusal tests recorded in TASK-010 and R1/R2 | PASS |
| Confidential `wyrd-ui` and public `wyrd-cli` cannot substitute for one another | `wyrd-server/src/auth/oauth.rs:266-354`; grant dispatch in `components/auth/routes.rs:112-190`; focused OAuth tests | PASS |
| Device callback stores approval only; token is minted once at live redemption | `wyrd-auth/src/cli_logins.rs:257-376`; SQL poll/delete transaction and deterministic race tests | PASS |
| User-code brute-force ownership follows standard self-hosted practice | Image-local limit deleted; operator instruction at `sso-and-oidc.svx:23`; no in-image/application/manifest mechanism remains | PASS |
| Public refresh rotates and contains replay only within its chain; confidential refresh does not rotate and respects absolute expiry | `wyrd-auth/src/refresh.rs:92-252`; refresh, revocation, and concurrency tests recorded green | PASS |
| Provider callback, refresh, and redemption cannot outlive connection replacement/deactivation | callback and issuance lifecycle fences plus old-connection journey evidence | PASS |
| RFC 8693 and RFC 7523 reuse the existing token and external-assertion authorities without weakening signature, tenant, audience, or binding checks | `components/auth/routes.rs:150-190`, `wyrd-auth/src/{exchange_api_key,jwt_bearer}.rs`, `wyrd-auth-verify` verifier | PASS |
| Secrets and bearer material are absent from logs, redirects, audit, and recoverable storage | secret newtypes, `skip_all`/secret-skipping instrumentation, digest-only code/refresh storage, static completion pages | PASS |
| Grant issuance and security-significant outcomes retain canonical audit semantics | shared `TenantTokenIssuer`, callback/device/refresh audit paths and prior focused failure proofs | PASS |
| Deleted private BFF/session/sealed-completion paths remain absent; no compatibility surface replaced them | cumulative diff and repository search | PASS |

## Verification Limits

- `mise run docs:check` passed during this review, including generation, links, build, and accessibility.
- `git diff --check` reports only a blank line at EOF in the prior review artifact `review/TASK-010-r2/verdict.md`. That is a non-behavioral formatting issue in review prose and, under the standing direction that placement, naming, structure, and wording findings never block, it is not a security finding and does not change this result.
- The superseding lead-direction record reports `mise run test:server:startup`, `mise run docs:check`, `mise run fmt`, and `mise run lints` green for the correction. The startup lane was not rerun independently in this domain review.
- R1/R2 and TASK-010 record green focused PostgreSQL, auth, OpenAPI, SQL, codegen, boundary, startup, and filtered identity lanes. Source inspection was used to retain each prior closure rather than accepting those summaries alone.
- No full identity sweep, every-language sweep, or integrated TASK-011 journey was run. Those belong to change review under the standing narrowest-lane direction.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None. No optional mechanism or speculative hardening is required by this review.

### Positive Controls

- Authorization redirects are possible only after unique client and exact registered redirect binding; untrusted request headers never choose the redirect or tenant.
- Authorization codes, device codes, refresh tokens, and API keys use digest or memory-hard verification rather than recoverable credential storage.
- Device approval and authorization-code redemption mint through the one shared tenant issuer and commit with their canonical audit evidence.
- OIDC issuer binding occurs before provider token redemption when RFC 9207 supplies or requires `iss`; verified ID-token issuer and audience remain authoritative afterward.
- JWT-bearer verification reuses the existing tenant-scoped external verifier and server-owned workload binding; the assertion cannot name its own Wyrd Card or permissions.
- Refresh replay containment is serialized, chain-scoped, and does not revoke unrelated logins.
- The replica-local rate limiter was removed instead of being made more complex; the operator-owned public ingress is documented as the conventional owner.

## Material Findings

None.

**Result: PASS**
