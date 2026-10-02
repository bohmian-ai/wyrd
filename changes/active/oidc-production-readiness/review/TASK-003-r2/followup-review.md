# TASK-003 R2 focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved specification: revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/TASK-003-R1-production-ui-remediation.md`
- Human amendment:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  replaces only prior `FIND-TASK-003-1` and `R1-AC-01`.

This pass investigated only the three assigned conflicts. The candidate remained
at the immutable commit above.

## 1. Expired browser-session ciphertext and canonical inventory

### Source paths inspected

- `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:478-540`
- `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:22-89,220-279`
- `crates/wyrd/wyrd-sql/migrations/20261001000001_auth_browser_sessions.sql:19-84`
- `crates/wyrd/wyrd-sql/src/postgres.rs:250-269`
- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:318-433,449-567`
- `crates/wyrd/wyrd-server/src/boot/mod.rs:1507-1547,2840-2925`
- `crates/wyrd/wyrd-server/tests/identity_e2e.rs:2990-3225`
- approved `REQ-005`, prior `FIND-TASK-003-4`, and remediation
  `R1-AC-04`

### Resolution evidence

The runtime reachability half of the persistence review is correct: after
`absolute_expires_at`, the SECURITY DEFINER resolver returns no tenant, the
tenant lock query cannot return the row, and therefore read, authority,
renewal, and logout cannot open or revoke it. The row is eventually deleted by
the tenant-local purge that precedes a later session insertion.

That does not satisfy the approved inventory and keyless-boot obligation.
Expired rows are not purged at expiry or at boot. A tenant that creates no
later session can retain non-null access, refresh/API-key, and CSRF envelopes
indefinitely. All four inventory queries exclude those rows with
`absolute_expires_at > statement_timestamp()`, while keyless boot treats that
inventory as proof that no stored ciphertext needs a key.

The controlling text is broader than runtime reachability:

- approved `REQ-005` requires the keyring whenever browser sessions persist
  recoverable credentials;
- validated prior `FIND-TASK-003-4` explicitly rejects an inventory that is
  false "while ciphertext rows persist" and selects every non-null browser
  session sealed column at the canonical owner;
- the remediation correction repeats "every non-null sealed column" and
  `R1-AC-04` requires keyless boot refusal "while any session envelope
  remains."

The remediation's references to *live* sessions describe the user recovery
proof (read, renewal, CSRF, and logout); they do not narrow the separately
explicit all-non-null inventory or keyless condition. An expired envelope no
longer needs opening for session service, but it is still stored recoverable
ciphertext and still makes the canonical "none remains" conclusion false.

### Proposed finding

`BEH-R2-001` is confirmed and prior `FIND-TASK-003-4` remains open. The
smallest correction boundary is the existing `SealedSecretTable` browser
inventory: include every non-null envelope regardless of absolute expiry, or
atomically purge expired rows before the canonical count. The focused proof
must store an expired non-null row and show either that it is removed by the
canonical operation or that it contributes to `remaining` and blocks keyless
boot.

## 2. AC-003 provider topology and mixed-callback proof

### Source paths inspected

- `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:1-52,140-243,246-327`
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:101-205,460-540`
- `crates/wyrd/wyrd-auth/src/callback.rs:94-173,567-592`
- `crates/shared/wyrd-auth-oidc/src/provider.rs`
- `docker-compose.yml:36-65`
- `mise.toml:576-607`
- approved `AC-003`, original TASK-003 scenario 2 and acceptance text,
  prior `FIND-TASK-003-6`, remediation diagnosis/correction, and `R1-AC-06`
- the human issuer direction, to distinguish the conditional RFC 9207 check
  from the separate journey obligation

### Resolution evidence

The candidate proves independent sessions and successful switching between
two distinct issuers, but both issuers are realms served by the one Keycloak
service. Dex is started by the repository-managed identity lane and is never
used as either active switch-tenant provider. Whether a realm can generally be
called an OIDC provider is not the deciding ambiguity here: validated prior
`FIND-TASK-003-6` selected the already-running Keycloak **and Dex** services as
the required minimal topology, and the remediation directs the implementation
to reuse those providers. The current test therefore does not close that
approved different-provider-service proof.

The mixed-callback assertions also prove a narrower fact than their comments
and acceptance mapping claim. `providerLogin` returns the complete genuine
provider callback, but each attack extracts only `code`; the helper constructs
`{code, state}` and drops every other provider-produced parameter, including
RFC 9207 `iss`. Consequently:

- when the state-selected provider advertises issuer-response support, the
  callback is refused by the approved missing-`iss` rule before code exchange;
- if it does not advertise support, the wrong-realm code can be refused by the
  victim realm's token endpoint, and the same-issuer/client code can be refused
  by PKCE because it was minted for the other flow's challenge.

The assertions do not retain `iss` and do not assert which branch refused the
request, so source alone cannot credit them as proof that a standards-shaped
wrong-tenant callback is rejected by issuer binding or that a
standards-shaped same-issuer callback reaches and is rejected by the intended
state/PKCE binding. The human issuer direction approves missing-`iss` refusal
for an advertising provider as product behavior; it does not redefine a
provider-produced mixed callback to omit `iss`, nor does it replace
`FIND-TASK-003-6` or `R1-AC-06`.

### Proposed finding

`BEH-R2-003` is confirmed and prior `FIND-TASK-003-6` remains open. Reuse the
already-running Dex service for the second active provider tenant. For each
mix-up proof, preserve the provider's complete callback query and replace only
the victim `state`; assert the refusal and absence of a completion/session.
This is a proof correction at the existing journey, not a product change or a
new harness.

## 3. Proactive renewal refusal before access-token expiry

### Source paths inspected

- `crates/wyrd/wyrd-auth/src/browser_sessions.rs:45-54,318-433,435-567`
- `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:45-89,220-279`
- `crates/wyrd/wyrd-auth/src/refresh.rs:104-218`
- `crates/wyrd/wyrd-auth/src/exchange_api_key.rs:151-193,196-254`
- `crates/wyrd/wyrd-server/src/components/auth/bff.rs:248-318`
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:192-329`
- `crates/wyrd/wyrd-server/wyrd-ui/src/hooks.server.ts:10-31`
- `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:246-267`
- `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:379-396`
- approved `REQ-016`, TASK-003 packet-local session contract and scenario 2,
  and remediation preserved-behavior/non-goal text

### Resolution evidence

The path is reachable on every protected UI request. The SvelteKit hook calls
`sessions/read`; mutating/API paths later call `sessions/authority`. Both enter
`BrowserSessions::current`. SQL reports `access_fresh = false` throughout the
last minute, not only after expiry. `current` then attempts renewal and, for
any policy refusal, wipes the browser-session credentials and commits before
returning `401`.

For SSO, deactivation or replacement is rechecked by the ordinary refresh
issuance path. The refresh workflow may already have consumed the presented
refresh row in the transaction before issuance detects the inactive
connection; `current` then commits that state together with browser-session
revocation. For API-key mode, key revocation or another admission refusal has
the same browser-session result. No caller falls back to the stored access
token.

This is not covered by the approved "existing stronger guard" exception.
`REQ-016` names principal or tenant blocking as that exception, then separately
states that connection deletion/deactivation/replacement blocks **renewal** and
that the BFF session requires login **once its current access token expires**.
TASK-003 likewise says failed/old-connection refresh ends the session at
access expiry, and the remediation explicitly preserves the current
access-token lifetime and rejects instantaneous-revocation claims.

The journey makes the defect deterministic: it configures a 30-second access
TTL, wholly inside the one-minute renewal margin, and expects immediate logout
after connection deactivation. Thus it positively proves the unapproved early
cutoff rather than the required expiry boundary.

### Proposed finding

`INV-R2-02` is confirmed as a new bounded correctness finding. The correction
belongs in the existing `BrowserSessions`/SQL clock boundary: distinguish a
still-valid token inside the proactive margin from an expired token. A refused
early renewal must not commit partial renewal mutations or revoke the browser
row; it may serve the already-issued token only until its stored expiry. The
first use after expiry must refuse and end the session, without trying another
credential. Focused real-store proof must cover pre-expiry use after
deactivation/replacement and the first post-expiry refusal.

## Follow-up result

**RESOLVED** — approved authority and current source resolve all three
conflicts. `BEH-R2-001` and `BEH-R2-003` are confirmed prior-finding closure
gaps; `INV-R2-02` is confirmed as a new bounded finding. No Cargo, mise, pnpm,
Postgres, provider, or browser lane was run, as directed; this was a static
source and authority follow-up.
