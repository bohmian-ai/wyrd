# Tenant OIDC security domain review

## Subject and boundary

Reviewed the immutable range
`3fc085acf5b3a710d5dc80892bd2e664b3db6174..d861845f3f5d89aca413857dcfb8c1bbfaee349d`
against approved `SPEC-oidc-production-readiness` revision 4, original
`TASK-002`, and remediation tasks `TASK-002-R1` and `TASK-002-R2`. The
candidate remained the checked-out `HEAD` throughout this review.

The boundary was tenant human OIDC and its authority: login and callback
routing; state, PKCE, nonce, code exchange, discovery, JWKS, algorithm and
claim verification; `(issuer, sub)` identity binding; provider-group role
sync; audit atomicity; sealed completion and provider-secret handling;
connection-bound refresh; tenant/platform/workload separation; and the
security effect of the lead-authorized reuse and unrelated flaky-test
cleanups. This is an acceptance audit, not a general hardening review.

## Authority and source coverage

| Boundary | Governing authority | Source and evidence inspected | Result |
|---|---|---|---|
| Tenant and connection selection | REQ-006/007/015; INV-001/004; security posture tenant-identity rules | `wyrd-auth/src/login.rs`, `callback.rs`; `WyrdPostgres::login_state_tenant`; login-state migration and SQL transitions; server route adapters | PASS |
| State, PKCE, nonce, redirect and completion | TASK packet-local contract; REQ-007; AC-003/007 | `wyrd-spec/src/auth/oidc.rs`; `login.rs:84-188`; `callback.rs:96-266`; forced-RLS login-state table and one-use transition queries | PASS |
| Provider IO and SSRF | `AGENTS.md`; `architecture/agent-rules.md`; security posture source-URL controls; INV-004 | `discover_provider`, screened authorization/token/JWKS calls, bounded token response, proxy/redirect-disabled `ScreenedHttp`; refusal tests and recorded journey evidence | PASS |
| ID-token trust | REQ-007; security posture federation and algorithm policy; OIDC Core audience/authorized-party rules | `callback.rs:200-219,536-615`; shared `ExternalVerifier::verify_external_against`; fresh discovery algorithm set; nonce and `azp` tests | PASS |
| Identity and tenant RBAC | REQ-008/014/015/017; INV-002/003 | exact-`sub` input and stored-row checks; `ensure_user_identity`; mapped-role replacement; `auth.user.roles.sync`; same-email/distinct-subject and rollback proofs | PASS |
| Refresh cutoff | REQ-016; task renewal contract | `refresh.rs:110-167`; `TenantTokenIssuer::issue_human_session`; migration revocation of provenance-free human rows; switch/cutoff evidence | PASS |
| Secret and session exposure | REQ-005/009; security posture secret rules | `SecretString` callback inputs and PKCE state; sealed provider secrets and completions; fixed callback response; retired public authorization-code grant and token-printing login CLI | PASS |
| Human, workload and platform separation | REQ-013/015; INV-003 | tenant callback accepts only `HumanConnections`; API-key and workload grants remain separate; no platform fallback; machine-independence journey evidence | PASS |
| Authorized follow-up cleanups | Original task plus lead direction recorded in R2 evidence | `principal_event` reuse, unreachable local HMAC filter removal, callback test-helper reuse, sealing-key retention documentation, gateway/Forge flaky-test fixes | PASS |

## Prior-finding closure

| Finding | Security closure at the cumulative candidate | Result |
|---|---|---|
| `FIND-TASK-002-1` | Public authoring and stored human-connection decode both require exact OIDC `sub`; durable lookup remains tenant-local `(issuer, sub)`, and email is non-authoritative. | CLOSED |
| `FIND-TASK-002-2` | The callback requires matching string `azp` whenever present and for every multi-audience token, after generic issuer/audience/signature verification and before persistence. | CLOSED |
| `FIND-TASK-002-3` | A real role-set change appends one canonical `auth.user.roles.sync` row in the same transaction as role replacement, issuance, and completion; append failure rolls all of it back. | CLOSED |
| `FIND-TASK-002-4` | The callback admits only the header algorithm freshly advertised by that provider, then the shared verifier independently rejects every symmetric algorithm before JWKS lookup and verifies signature/issuer/audience/time. | CLOSED |
| `FIND-TASK-002-5` | Login-state transitions rely on forced RLS and do not duplicate tenant predicates; cross-tenant transition proof remains recorded. | CLOSED |
| `FIND-TASK-002-6` | The sole cross-tenant state-owner lookup is the narrow `WyrdPostgres` operation over the app pool and returns only the owner of an unconsumed, unexpired 256-bit state hash. | CLOSED |
| `FIND-TASK-002-7` | The live PKCE verifier is a `SecretString`; raw SQL decode is private and wrapped immediately; fresh Debug-redaction proof passed. | CLOSED |
| `FIND-TASK-002-8` / `-9` | R2's documentation/import corrections do not change executable security behavior. The relevant security contracts now document the actual state transitions and redaction boundary. | CLOSED (non-security rules) |

The post-R2 `principal_event` extraction preserves the same operation,
principal kind, resource, permission, and outcome used by the prior role-sync
event. Removing the callback's duplicate HMAC filter does not widen accepted
tokens: `ExternalVerifier::verify_external_against` still rejects HS256,
HS384, and HS512 before `kid` lookup. The test-helper changes alter no
production path. The runbook now correctly retains the old sealing key for the
two-minute completion TTL after the last old-key writer stops; no secret bytes
or alternate rotation path were added. The two unrelated flaky-test fixes do
not touch authentication, authorization, tenancy, secrets, or production code.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by the approved task. No speculative hardening item was retained.

### Positive Controls

- State is generated from 256 bits of OS randomness, stored only as SHA-256,
  consumed and committed before provider IO, and cannot be replayed.
- Tenant, connection revision, issuer, client, redirect, PKCE verifier, nonce,
  and redemption binding all come from server-owned state; headers, paths,
  email and provider-selected claims cannot redirect authority.
- Fresh discovery, token and JWKS calls are DNS-screened, address-pinned,
  bounded, proxy-free, redirect-disabled, and fail closed.
- ID tokens are checked for provider-advertised algorithm, asymmetric
  signature, `kid`, issuer, audience, expiry/skew, nonce, and OIDC authorized
  party before identity or role persistence.
- Unknown groups and missing tenant roles grant nothing; human connection
  default roles do not grant authority. Role changes and token issuance share
  the canonical audit transaction.
- Human refresh families carry exact connection provenance; replacement,
  deactivation or deletion prevents a successor while existing access remains
  limited to the approved five-minute snapshot.
- Provider secrets and completed Wyrd credentials are sealed at rest and
  redacted from diagnostics and response projections. The callback returns
  only a fixed same-origin redirect or static page, never token JSON.

## Verification performed and limits

Fresh focused checks passed on the final candidate:

- `auth::human_connection::tests::human_subject_must_be_exactly_sub`
- `queries::auth::login_state::tests::login_state_debug_redacts_the_pkce_verifier`
- `auth::callback::pg_tests::verify_authorized_party_enforces_azp_for_the_client`

The R1 and R2 evidence tables record green focused Postgres tests for
advertised-algorithm refusal, same-email/distinct-subject identity, changed and
unchanged role-sync audit, audit-failure rollback, state isolation and owner
lookup; all four named identity journeys; the 27-test identity lane; principals,
SQL, codegen, boundary, format, lint and docs lanes. R2's final evidence also
records `mise run test:wyrd` with 2,213 passing tests after the two authorized
flaky-test repairs.

I did not rerun the complete IdP/Postgres journeys or every broad lane within
this reviewer's time limit; I independently inspected the relevant production
bodies, callers, migration, focused tests, and the final reuse diffs. TASK-003's
BFF completion HTTP route and TASK-004's CLI handoff persistence remain
intentional non-goals; this candidate supplies their sealed one-use owner
primitive without exposing it as a public route.

## Overall result

**PASS** — the cumulative candidate satisfies the tenant OIDC security/RBAC
boundary, closes all prior security findings, and introduces no new material
in-scope vulnerability or security regression.
