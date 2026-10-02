# TASK-009 round-3 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Candidate tree: `0905c9a3bf16004c9c1d4d1405c2c11a95e6619a`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`

The candidate object and tree remained unchanged throughout review. The
complete base-to-candidate range and the remediation delta
`1ddc10e21054ddc158f461e8c6d8aa862c32a067..04597909203463820b2033c12956f5fe6fcfe1f4`
were available. `.codegraph/` is absent, so reviewers used commit-qualified Git
source and direct caller tracing.

`FIND-TASK-009-5` and `FIND-TASK-009-14` remain **WITHDRAWN — MUST NOT
REOPEN**. This verdict requires no equivalent provider-error redaction,
migration preflight, overlap column, or dual write.

## Reconciled acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Tenant, connection-test, and platform human flows use `openidconnect` through the one screened transport | `RelyingParty::{discover,authorize,redeem,verify}`; `HumanConnections`; `PlatformLogin`; `ScreenedHttp` | Recorded relying-party, callback, identity, and served platform tests; `test:shared`, `test:wyrd` | PASS |
| PKCE, state, nonce, issuer, audience, `azp`, algorithm, signature, expiry, subject, and RFC 9207 failures refuse closed | Library primitives and verifier defaults; shared response-issuer check; trust-all additional-audience override absent | Exact `id_token_refusals_fail_closed`; tenant refusal and issuer-binding journeys | PASS |
| Unknown `kid` performs one bounded process-local rediscovery and concurrent misses share the existing Moka cache | `RelyingParty::cached` and the single invalidation/re-entry path | Exact unknown-key and concurrent cache tests | PASS |
| Unsafe URLs, redirects, JWKS failures, and provider outages do not bypass screening or fall back across tenants/planes | `ScreenedHttp`; typed error projection; no alternate HTTP client/provider branch | Exact unsafe-destination, redirect, outage, workload metadata-only, and served platform refusal proofs | PASS |
| Human verification no longer uses `ExternalVerifier`; workload RFC 7523 verification still does | Corrected workload-only docs and production `jwt-bearer` wiring; callback fixture no longer builds the workload verifier | Callback target and `test:wyrd` evidence | PASS |
| Platform setup performs full discovery through the process-owned cache before durable replacement | Platform configure uses the boot-owned platform login capability and persists the advertised JWKS URI only after successful discovery | Served platform test covers unavailable/undecodable JWKS and same-issuer replacement | PASS, with owner-surface drift in `FIND-TASK-009-15` |
| Public route declarations and maintained docs match reachable platform setup failures | Runtime maps unavailable or undecodable discovery/JWKS to existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` | Existing failure cases assert only non-success; served OpenAPI omits this operation's `503` | FAIL — `FIND-TASK-009-16` |
| Human audience is derived only from client ID; workload audience remains separate | Platform request/view/row and `CodeRedemption` have no independent human audience | Codegen, SQL, relying-party, and served platform evidence | PASS |
| Tenant/platform authority, SQL capability types, audit, secret sealing, and durable lifecycle remain with existing owners | `TenantConn`/`OperatorPool` boundaries, audited platform transaction, tenant revision checks, sealed secrets | SQL, callback, platform, principals, and boundary evidence | PASS |
| No provider-specific branch, second cache, audience option, compatibility service, or other nonstandard mechanism entered the task | Cumulative source and dependency inspection | Formatting, lint, codegen, workspace-hack, and boundary lanes | PASS |
| Task verification is proportional to the final write set | Exact changed-test selectors and affected Rust/server/contract lanes are recorded green | Full journeys and every-language sweeps are reserved for final change review under `518026d54` | PASS |

## Independent review results

| Report | Result | Reconciliation |
|---|---|---|
| `task-review-behavior.md` | PASS | No behavior findings. |
| `task-review-invariants.md` | PASS | No invariant findings. |
| `standards-review.md` | PASS | No repository-rule findings. |
| `maintainer-review.md` | FAIL | Proposed `MNT-R3-001` and `MNT-R3-002`; both independently confirmed. |
| `system-review.md` | PASS | No resilience findings. |
| `domain-review-oidc-security.md` | PASS | No OIDC/security findings. |
| `domain-review-cache-concurrency.md` | PASS | No cache/concurrency findings. |
| `domain-review-persistence-tenancy.md` | FAIL | Proposed inert human TTL cleanup; independently rejected as pre-existing, task-unrelated public/schema debt. |
| `followup-review.md` | RESOLVED | Resolved the three conflicting discovery claims without adding a finding. |
| `findings-validation.md` | Two retained findings | Confirmed `FIND-TASK-009-15` and `FIND-TASK-009-16`; rejected `PERSIST-TEN-R3-001`. |

The focused follow-up was required because the maintainer and persistence
claims materially conflicted with otherwise clean discovery reports. It
resolved all three uncertainties from source. No additional follow-up was
needed.

## Validated finding ledger

| Finding | Status | Classification | Required correction boundary |
|---|---|---|---|
| `FIND-TASK-009-15` | CONFIRMED | DRIFT | Delete the one-caller public `PlatformLogin::relying_party` dependency accessor and put the fresh-discovery/JWKS-URI operation on the existing `PlatformLogin` owner, delegating to its existing relying party and cache. |
| `FIND-TASK-009-16` | CONFIRMED | INCORRECT | Make platform configuration rustdoc, comments, `utoipa` responses, served failure assertions, and served OpenAPI proof describe the existing `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` outcome for unavailable or undecodable discovery/JWKS data. |

`PERSIST-TEN-R3-001` is omitted from the ledger. The validator found the human
TTL surfaces already inert at the base, conventional in comparable projects,
and outside TASK-009; deleting them would contradict this task's explicit
no-operator-visible-change outcome.

## Prior-finding closure

- `FIND-TASK-009-1` through `-4` and `-6` through `-13` remain closed on the
  source and verification evidence recorded in `findings-validation.md`.
  `FIND-TASK-009-13` is functionally closed; findings `-15` and `-16` are
  independent owner/declaration defects left at that correction seam.
- `FIND-TASK-009-5` and `FIND-TASK-009-14` remain **WITHDRAWN — MUST NOT
  REOPEN**.

## Verification limits

Every required reviewer and report was available. The remediation record
contains exact focused selectors and the narrow repository lanes covering the
candidate write set. Reviewers independently reran the focused ID-token and
cache/concurrency tests successfully. Under the binding rule from commit
`518026d54`, another full user-journey or every-language sweep is not required
at task review; those run once at final change review.

The retained gaps are narrower than those existing results: the current served
platform failure cases do not pin the stable `503` response, the served OpenAPI
test does not pin that response declaration, and the one-caller dependency
accessor remains public. Their closure uses existing tests and owner lanes only.

## Verdict

**FIX_REQUIRED**

The two bounded findings are decision-complete in
`TASK-009-R3-platform-login-boundary-and-contract.md`. No specification
revision is required.
