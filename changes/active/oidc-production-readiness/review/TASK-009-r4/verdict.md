# TASK-009 round-4 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Current remediation:
  `changes/active/oidc-production-readiness/review/TASK-009-r3/TASK-009-R3-platform-login-boundary-and-contract.md`
- Cumulative range:
  `35a53faa216b10651d85c96ce12e34f382cac637..0bd3686e8bb763b07376f84661946aadc6200bfb`
- Latest remediation delta:
  `04597909203463820b2033c12956f5fe6fcfe1f4..0bd3686e8bb763b07376f84661946aadc6200bfb`

The candidate commit and tree remained unchanged through discovery and
independent validation. `.codegraph/` is absent, so reviewers used the Git
diff, repository search, and direct source and caller tracing.

`FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` remain
**WITHDRAWN — NOT REOPENED** by binding lead direction. No report renamed,
reintroduced, or indirectly required their corrections. Placement, naming,
structure, and wording alone were not treated as blocking findings, and no
review required a mechanism, check, file, setting, or option beyond standard
practice.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
|---|---|---|
| Tenant login, candidate test sign-in, and platform login use `openidconnect 4.0.1` for discovery, PKCE/state/nonce, code redemption, and human ID-token validation | `RelyingParty::{discover,authorize,redeem,verify}` is shared by the tenant and platform owners; recorded relying-party, callback, and identity proof covers success and refusals | PASS |
| Every human relying-party request uses the one screened, DNS-pinned, proxy-free, redirect-disabled, bounded transport | `ScreenedHttp` is the injected `AsyncHttpClient`; manifests keep the `oauth2` `reqwest` feature disabled; unsafe destination and redirect proofs remain green | PASS |
| ID-token and RFC 9207 failures refuse closed before authority is issued | Library verifier defaults plus the shared response-issuer check cover issuer, audience, `azp`, algorithm, key, time, nonce, and exact response `iss`; tenant and platform callback proofs cover negative paths | PASS |
| Unknown `kid` causes one bounded process-local rediscovery, and overlapping misses use the existing Moka single-flight path | `RelyingParty::cached` uses `try_get_with`; redemption invalidates and re-enters it once; three focused cache tests selected and passed | PASS |
| Workload setup remains metadata-only and workload assertions remain on `ExternalVerifier` | Workload admin and boot use `ScreenedHttp::provider_metadata`; production human callers use only `RelyingParty` | PASS |
| Platform setup performs full discovery through the process-owned relying party before persistence | Configure, begin, and callback share the boot-owned `PlatformLogin`; failed discovery changes no durable row and successful same-issuer setup refreshes the cache used by login | PASS |
| Platform setup publishes its reachable discovery/JWKS failure as `503/WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` | Handler rustdoc and `utoipa` agree with the existing error projection; exact served platform and OpenAPI tests each selected one test and passed | PASS — closes `FIND-TASK-009-16` |
| Human audience derives only from `client_id`; tenant and platform authority remain separate | No independent human audience input remains; tenant work uses `TenantConn`, platform work uses `OperatorPool`, and platform sessions confer no tenant authority | PASS |
| Hand-written human discovery, random PKCE/state/nonce, token POST, and human `ExternalVerifier` ownership are absent | Complete source and caller inspection finds the vetted library path and the workload-only verifier owner | PASS |
| Prohibited drift and regressions remain excluded | No provider branch, second client/cache, audience option, distributed coordination, compatibility mechanism, BFF/device/session redesign, or changed tenant selection, role mapping, issuance, or audit entered the task | PASS |

## Independent review results

| Report | Result |
|---|---|
| `task-review-behavior.md` | PASS — no proposed findings |
| `task-review-invariants.md` | PASS — no proposed findings |
| `standards-review.md` | PASS — no material findings |
| `maintainer-review.md` | PASS — no material findings or uncertainties |
| `system-review.md` | PASS — no material proposed findings |
| `domain-review-oidc-security.md` | PASS — no material findings |
| `domain-review-cache-concurrency.md` | PASS — no material findings |
| `domain-review-persistence-tenancy.md` | PASS — no material proposed findings |
| `findings-validation.md` | PASS — explicitly validated empty ledger |

The discovery reports agree on an empty finding union and collectively cover
behavior, invariants, repository standards, maintainability, deployed failure
and recovery paths, OIDC security, cache concurrency, persistence, and tenancy.
They reveal no conflicting claim, uncovered reachable path, or unresolved
shared source from the prior remediation rounds. A focused follow-up was
therefore not triggered, and no `followup-review.md` is required.

## Validated finding ledger

**Empty.** The independent Ponytail validator traced the live producers,
callers, sibling consumers, writers, failure paths, and existing mechanisms
against the complete cumulative candidate and latest remediation delta. It
confirmed that no new `FIND-TASK-009-17` is supported.

## Prior-finding closure

- `FIND-TASK-009-1` through `-4` and `-6` through `-13` remain **CLOSED** by
  the process-owned relying parties, RFC 9207 checks, workload-only verifier,
  native Moka coalescing, client-ID-derived audience, standard library
  behavior, corrected ownership/docs, and exact focused proof.
- `FIND-TASK-009-16` is **CLOSED**: runtime, route documentation, served
  OpenAPI, and served failure assertions agree on the existing discovery/JWKS
  `503` status, stable code, and problem schema.
- `FIND-TASK-009-5`, `FIND-TASK-009-14`, and `FIND-TASK-009-15` remain
  **WITHDRAWN — NOT REOPENED**.

## Verification limits

The immutable final candidate was verified through the repository-managed
Postgres wrapper with exact selectors for:

- `federated_platform_sign_in_runs_through_the_served_callback`: one selected,
  one passed; and
- `the_served_document_describes_the_composed_surface`: one selected, one
  passed.

The cache/concurrency domain review also selected and passed the three focused
Moka tests. Final-candidate formatting and lint evidence is recorded in the R3
remediation. Direct test invocations without the repository Postgres wrapper
failed only because `WYRD_TEST_DATABASE_ADMIN_URL` was unset; the correctly
wrapped exact runs passed and supply the relevant evidence.

Under binding human direction, task review uses the narrowest lanes covering
the task write set. Full user journeys and every-language sweeps run at change
review, so their absence here is not a task-review verification limit.

## Verdict

**PASS**

All required independent reports are complete, the validated finding ledger is
empty, every task obligation passes, non-goals remain excluded, prior findings
are closed or expressly withdrawn, and no unrelated implementation entered the
reviewed task.
