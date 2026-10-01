---
id: TASK-003-R2
kind: remediation
status: ready
spec: SPEC-oidc-production-readiness
spec_revision: 5
requirements: [REQ-005, REQ-009, REQ-015, REQ-016, AC-003, AC-007]
depends_on: [TASK-003-R1]
parent_task: TASK-003
remediates: [FIND-TASK-003-4, FIND-TASK-003-5, FIND-TASK-003-6, FIND-TASK-003-10, FIND-TASK-003-11, FIND-TASK-003-12, FIND-TASK-003-13]
---

# Production UI review remediation, round 2

## Authority and immutable inputs

- Approved specification:
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
  (`SPEC-oidc-production-readiness`, revision 5)
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior remediation and explicit issuer amendment:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/`
- Reviewed base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Reviewed candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- R2 verdict and validated ledger:
  `changes/active/oidc-production-readiness/review/TASK-003-r2/{verdict,findings-validation}.md`

The human direction in `human-direction-FIND-TASK-003-1.md` remains binding:
RFC 9207 is conditional, and providers that do not advertise issuer-response
support must not be refused for that absence.

## Outcome

Finish TASK-003 without changing its product or security model: make the
canonical sealing inventory truthful for every stored browser envelope, prove
the existing private BFF channel over real TLS, complete the required
different-provider and standards-shaped callback journey, preserve issued
browser authority until exact expiry when proactive renewal is refused,
restore mandatory discovery rustdoc, bound chooser verification, and refuse an
explicitly empty upstream origin.

## Issue diagnoses and required corrections

### FIND-TASK-003-4 — Expired stored ciphertext is absent from canonical inventory

Browser-session insertion stores sealed access, refresh or API-key, and CSRF
values. Absolute expiry makes a row unusable but does not delete it. Purge runs
only before a later session insertion for that tenant. The four canonical
inventory queries require `absolute_expires_at > statement_timestamp()`, so an
expired row may retain non-null ciphertext indefinitely while
`SealedSecretRewrap` reports zero and keyless boot succeeds.

Keep the existing `SealedSecretTable`/`SealedSecretRewrap` owner and exact-byte
CAS mechanism. Inventory every non-null browser-session sealed value regardless
of absolute expiry; do not add a second purge or rotation engine. Update the
inventory documentation so its stored-ciphertext contract is explicit.

The correction must make the canonical report and keyless decision truthful
without changing runtime reachability of expired sessions or weakening the
existing live-session CAS/K2 recovery behavior.

### FIND-TASK-003-5 — The private channel has no real TLS exercise

`serverUrl` rejects non-loopback plaintext, but the HTTPS unit test replaces
fetch with an in-memory recorder and the production-built two-BFF journey uses
the test server's loopback HTTP URL. No secret-bearing authenticated BFF
operation has crossed an actual TLS connection.

Extend the existing identity UI journey with the repository-managed TLS
mechanism and the same Wyrd server/BFF processes. At least one ordinary
authenticated browser-session operation must reach Wyrd through a trusted
`https:` origin using the native fetch path. Preserve loopback HTTP coverage
and the pre-fetch non-loopback plaintext refusal. Add no bypass flag, alternate
client, second authentication path, or second journey harness. No separate
untrusted-certificate scenario is required.

### FIND-TASK-003-6 — Different-provider and mixed-callback proof is incomplete

The named multi-provider journey establishes two Keycloak realms, not the
Keycloak and Dex services selected by the prior remediation. Its attack helper
also rebuilds callbacks from only `code` and victim `state`, discarding the
provider-produced `iss`; for an advertising provider, the request is refused
by the missing-issuer rule before the intended state/PKCE boundary is proved.

Keep the existing identity lane, host, providers, and two BFF replicas. Use the
already-running Dex service as the second active provider without weakening
the existing connection-test contract. Preserve every genuine callback query
parameter and replace only victim `state`; for the same-issuer case retain the
genuine matching `iss` so the intended server-bound state/PKCE check decides
the refusal. Prove no completion or session is created. Add no provider
dependency or harness.

### FIND-TASK-003-10 — Refused proactive renewal cuts off valid authority early

Postgres marks an access token stale during the minute before expiry. Every BFF
read/authority call then attempts renewal. Connection, key, or policy refusal
wipes and commits the browser row even while `access_expires_at` is still in
the future. With the journey's 30-second token lifetime, the first request is
always proactive and the existing deactivation assertion pins immediate
logout, contradicting REQ-016's exact-expiry behavior.

Keep lifecycle ownership in `BrowserSessions`, the existing row lock, and the
PostgreSQL clock authority. Distinguish a still-valid token inside the renewal
margin from an expired token. If early renewal is refused, do not commit
tentative refresh/key-use mutations or browser-row revocation; expose only the
already-issued token until its stored expiry. The first post-expiry use must
refuse and revoke as today, without trying another credential. Infrastructure
failures remain fail closed.

### FIND-TASK-003-11 — New discovery items lack mandatory rustdoc

`RawProviderMetadata.authorization_response_iss_parameter_supported` is new
but undocumented. The materially modified fallible `parse_raw_metadata`
converts three endpoint URLs and returns `OidcError::Discovery` without
operation rustdoc or `# Errors`, violating the repository's private-item
documentation rule.

Document only those two items: the raw field's absent-as-false wire semantics,
the conversion's workflow role, and the malformed endpoint conditions that
produce `OidcError::Discovery`. Add no wrapper or documentation abstraction.

### FIND-TASK-003-12 — Cookie hints create unbounded internal fan-out

`ServerSessions.metadata` collects request-cookie suffixes and sends the
entire distinct set through bare `Promise.all`. Each hint triggers an
authenticated private read and database work before invalid data is cleared.
The request-derived collection has no application-owned concurrency bound.

Keep `ServerSessions.metadata`, deduplication, invalid-cookie clearing, and the
existing `read` owner. Resolve distinct hints sequentially, giving the path a
native bound of one without a dependency, queue, worker, or new abstraction.

### FIND-TASK-003-13 — Empty upstream configuration becomes loopback

`WYRD_SERVER_URL || fallback` treats an explicit empty deployment value as
absence, validates the loopback fallback, and silently redirects every
secret-bearing BFF call to a local listener.

Use nullish-only fallback on the existing value. Absence keeps the current
local default; an explicit empty string reaches the existing native URL parser
and is refused before fetch. Add no configuration surface or validator.

## Constraints and preserved behavior

- Preserve approved specification revision 5, original TASK-003 behavior, and
  the explicit provider-agnostic issuer decision.
- Keep identity, tenant binding, credentials, roles, sealing, session
  lifecycle, connection lifecycle, and audit server-owned.
- Preserve one common callback, exact public origin, single-use state,
  provider/network screening, exact `iss` comparison when present, and
  conditional absence behavior from the human amendment.
- Keep browser tokens, API keys, refresh tokens, provider secrets, tenant UUID,
  and the BFF service key out of page data, URLs, JavaScript storage, logs,
  traces, errors, audit payloads, and generated artifacts.
- Preserve Secure, HttpOnly, SameSite=Lax, host-only cookies; exact origin and
  constant-time CSRF checks; replica-safe Postgres authority; tenant RLS;
  transaction ownership; exact-byte sealing CAS; and current five-minute
  access-token maximum.
- Keep every invalid API-key class at exactly one shared expensive
  verification and one indistinguishable refusal.
- Reuse the existing owners, providers, TLS/test support, and identity journey.
  Add no dependency, Cargo feature, test harness, callback route, credential
  path, role mapper, local password authority, UI membership store, sealing
  engine, TLS bypass, or compatibility alias.
- Do not weaken, ignore, delete, or relabel a gate or assertion to obtain green
  evidence.

## Explicit non-goals

- No SAML, SCIM, LDAP, social login, hosted signup, or commercial onboarding.
- No change to conditional RFC 9207 behavior or the documented residual risk
  for non-advertising providers.
- No instantaneous revocation of already-issued access authority through
  connection lifecycle changes.
- No new browser-visible identity or credential data.
- No general auth, UI, persistence, configuration, or test-harness refactor.
- No optional naming cleanup outside the seven validated findings.

## Acceptance criteria

| Criterion | Findings closed | Required observable result |
|---|---|---|
| R2-AC-01 | `FIND-TASK-003-4` | An expired, non-revoked row with non-null mode-appropriate envelopes is counted and rewrapped by the canonical pass; keyless boot refuses while any such envelope remains; live-session CAS and K2-only recovery remain green. |
| R2-AC-02 | `FIND-TASK-003-5` | A production-built BFF performs an authenticated browser-session operation over a real trusted TLS connection; loopback HTTP remains usable and non-loopback HTTP remains refused before fetch. |
| R2-AC-03 | `FIND-TASK-003-6` | The existing two-BFF journey establishes and switches independent Keycloak and Dex sessions and proves complete wrong-provider and same-issuer callbacks, with genuine `iss` retained, create no completion or session. |
| R2-AC-04 | `FIND-TASK-003-10` | After connection/key renewal becomes disallowed, the current token remains usable only until its stored expiry; no successor or tentative renewal mutation commits; the first post-expiry use refuses and ends the session. |
| R2-AC-05 | `FIND-TASK-003-11` | The raw discovery flag and fallible parser have accurate substantive rustdoc and `# Errors`; discovery projection behavior is unchanged. |
| R2-AC-06 | `FIND-TASK-003-12` | Multiple valid-looking cookie hints never exceed one in-flight server verification, while deduplication, invalid-cookie clearing, rendered choices, and switching remain unchanged. |
| R2-AC-07 | `FIND-TASK-003-13` | An absent upstream value retains the loopback default; an explicit empty value is refused before fetch; HTTPS, literal loopback, and non-loopback plaintext cases remain correct. |

## Focused proof

Use Red-Green-Refactor for executable corrections and retain the expected RED
failure plus final GREEN result in the implementation evidence.

1. Extend the existing sealing/keyless proof with
   `boot::sealing_boot_pg_tests::keyless_boot_refuses_while_an_expired_browser_session_envelope_remains`
   and run its exact repository-pinned selector:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-server --lib \
     -E 'test(=boot::sealing_boot_pg_tests::keyless_boot_refuses_while_an_expired_browser_session_envelope_remains)'
   ```

   Also retain `browser_session_sealing_rotation_journey` through the existing
   repository-managed identity setup.

2. Extend the existing real UI scenario `production SSO crosses replicas` so
   at least one private session operation uses the trusted TLS origin, then run:

   ```bash
   mise exec -- env WYRD_IDENTITY_TARGET=ui \
     WYRD_IDENTITY_FILTER='production SSO crosses replicas' \
     mise run test:identity:journey
   ```

3. Correct the existing `production multi-provider tenant switch` topology and
   callback mutation, then run:

   ```bash
   mise exec -- env WYRD_IDENTITY_TARGET=ui \
     WYRD_IDENTITY_FILTER='production multi-provider tenant switch' \
     mise run test:identity:journey
   ```

4. Add
   `browser_sessions::pg_tests::proactive_renewal_refusal_preserves_authority_until_expiry`
   and run its exact selector through the repository-managed Postgres setup:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-auth --lib \
     -E 'test(=browser_sessions::pg_tests::proactive_renewal_refusal_preserves_authority_until_expiry)'
   ```

5. Retain discovery projection coverage after the documentation correction:

   ```bash
   mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
     -E 'test(=provider::tests::discover_projects_authorization_response_issuer_support)'
   ```

6. Add focused Vitest cases `production chooser bounds server verification`
   and `empty upstream value is refused before fetch`, then run:

   ```bash
   mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
     src/lib/server/auth/session.test.ts -t 'production chooser bounds server verification'
   mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run \
     src/lib/server/upstream.test.ts -t 'empty upstream value is refused before fetch'
   ```

7. Retain the R1 filtered UI journeys, issuer-binding journey, fixed-cost API
   key tests, chooser/context tests, live sealing rotation, and provider
   replacement journey so remediation cannot trade away already-closed
   findings.

## Broader verification

After the focused proof is green, run the narrow complete set for the touched
surfaces:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise run test:identity:journey
mise run test:wyrd
mise run test:sql
mise run codegen:check
mise run check:tenant-isolation
mise run docs:check
mise run fmt
mise run lints
git diff --check
```

If a named Rust test requires repository-managed Postgres setup, run the exact
selector inside the owning setup wrapper rather than weakening it to an
in-memory substitute. A red gate blocks completion; diagnose and correct the
failure without weakening the gate.

Route this task directly to `$wyrd-implement`. A later `$wyrd-task-review`
must reassess the complete original base-to-remediated-candidate range against
TASK-003, both remediation rounds, and the explicit human issuer direction.
