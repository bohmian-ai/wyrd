# TASK-003 R2 structured finding validation

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `63c5bffc93cd2f7b5ed558e610a213efcc34fd49`
- Candidate: `05ff68fb47572e7d8e5fa34037042559bcfeac83`
- Approved specification: `SPEC-oidc-production-readiness` revision 5 at
  `d9a098b5f23eba53a9e11a63bf1dae5367e4fd20:changes/active/oidc-production-readiness/spec.md`
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-003-production-ui.md`
- Prior review and remediation:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/`
- Replacing human authority:
  `changes/active/oidc-production-readiness/review/TASK-003-r1/human-direction-FIND-TASK-003-1.md`
  replaces prior `FIND-TASK-003-1` and `R1-AC-01` only.

The candidate resolved to the stated object before and after source validation.
CodeGraph was unavailable because this repository has no `.codegraph/`
directory. No build, Cargo, mise, pnpm, provider, browser, or Postgres command
was run in this validation pass.

## Source-ID dispositions

| Discovery source | Disposition | Stable finding | Validation |
|---|---|---|---|
| `BEH-R2-001` | **CONFIRMED** | `FIND-TASK-003-4` | All four browser-session inventory statements exclude expired rows even though expiry does not purge them. The migration says purge happens only on that tenant's next session insertion, while the canonical boot owner claims to count every stored ciphertext. The prior all-non-null correction therefore remains incomplete. |
| `BEH-R2-002` | **CONFIRMED** | `FIND-TASK-003-5` | `serverUrl` enforces the scheme, but the HTTPS unit test substitutes an in-memory fetcher and the only built two-BFF journey points Node at the test server's loopback HTTP URL. No authenticated BFF request crosses TLS. |
| `BEH-R2-003` | **CONFIRMED** | `FIND-TASK-003-6` | The remediation explicitly selected the already-running Keycloak and Dex services; the journey activates two Keycloak realms instead. Its mixed-callback helper reconstructs only `code` and `state`, discarding the provider-produced `iss`, so an advertising provider is rejected by the missing-issuer branch before the intended mixed-callback boundary is proved. |
| `INV-R2-01` | **CONFIRMED** | `FIND-TASK-003-5` | Duplicate of the missing real TLS closure proof. |
| `INV-R2-02` | **CONFIRMED** | `FIND-TASK-003-10` | `lock_browser_session` declares every token inside the one-minute margin stale. `BrowserSessions::current` then renews and commits revocation on any policy refusal without checking whether the access token is still valid. The journey's 30-second TTL makes this early cutoff happen on every use. |
| `STD-R2-001` | **CONFIRMED** | `FIND-TASK-003-11` | The added private raw discovery field has no rustdoc, and the materially modified fallible `parse_raw_metadata` has neither operation rustdoc nor `# Errors`. `AGENTS.md` section 16 makes both hard blockers regardless of visibility. |
| `STD-R2-002` | **CONFIRMED** | `FIND-TASK-003-12` | `ServerSessions.metadata` turns every syntactically valid cookie hint into a concurrent database-backed private request through bare `Promise.all`. The collection is request-derived and has no application-level concurrency bound, contrary to the applicable TypeScript rule. |
| `STD-R2-003` | **CONFIRMED** | `FIND-TASK-003-13` | `WYRD_SERVER_URL || fallback` reinterprets an explicit empty deployment value as absence, then validates the fallback. The TypeScript authority requires nullish defaulting, and this is the security-sensitive origin used by every BFF secret-bearing call. |
| `MAINT-R2-001` | **REJECTED** | — | The inherited field name is imperfect after the row was generalized, but `SealedSecretRow` documents the value as the selected table's sealed bytes, every SQL branch aliases the selected value explicitly, and the adjacent `SealedSecretTable` determines its meaning. Renaming it changes no task behavior, invariant, or hard repository rule. It is optional cleanup, which this acceptance audit must omit. |
| `SYSTEM-R2-001` | **REVISED** | `FIND-TASK-003-5` | The missing TLS exercise is real and duplicates `BEH-R2-002`/`INV-R2-01`. Requiring a separate untrusted-certificate and endpoint-replacement recovery scenario exceeds `R1-AC-05`; the retained correction requires the smallest real trusted TLS session operation only. |

The security/identity domain report proposed no source-local finding. Its
conditional RFC 9207, fixed-cost API-key, tenant/RLS, CSRF, secret-exposure,
and server-owned identity conclusions were independently supported by source.
Its empty ledger does not close `FIND-TASK-003-5` or `FIND-TASK-003-6`: it
credits implementation controls where the approved remediation separately
requires specific real-system proof.

The persistence/concurrency domain report also proposed no source-local
finding. Its row-lock, RLS, transaction, CAS, and live-session recovery claims
were supported. Its empty ledger is not adopted for the two disputed edges:
expired ciphertext remains stored outside the canonical inventory, and a
refused proactive renewal commits session revocation before the issued access
token expires. The focused follow-up traced both paths and its resolutions of
`BEH-R2-001`, `BEH-R2-003`, and `INV-R2-02` are confirmed.

## Final deduplicated finding ledger

### FIND-TASK-003-4 — Canonical rewrap omits stored expired-session ciphertext

- **Sources:** `BEH-R2-001`; focused follow-up section 1
- **Status:** **CONFIRMED**
- **Classification:** `INCORRECT`
- **Violated obligation:** approved `REQ-005`; prior remediation
  `FIND-TASK-003-4`; `R1-AC-04`, which requires every non-null browser-session
  envelope in the canonical inventory and keyless refusal while any such
  envelope remains.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/human_connections.rs:381-424,467-540`;
  producer/purge at
  `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:22-26,205-239`;
  consumer at `crates/wyrd/wyrd-auth/src/sealing.rs:81-129` and
  `crates/wyrd/wyrd-server/src/boot/mod.rs:1506-1549`.
- **Producer-to-consumer evidence:** session insertion stores sealed access,
  refresh or API-key, and CSRF values. Absolute expiry makes the session
  unreachable but performs no write. Purge occurs only before a later session
  insertion for the same tenant. Each browser inventory query requires
  `absolute_expires_at > statement_timestamp()`, so an expired row can retain
  non-null ciphertext indefinitely while `SealedSecretRewrap::run` reports
  zero and keyless boot succeeds. The table constraint already guarantees that
  revoked rows have null sealed columns.
- **Observable consequence:** the canonical report and key-retirement/keyless
  boot decision can claim no ciphertext remains while the database still
  stores recoverable browser credential envelopes.
- **Decision-complete smallest correction:** keep the existing
  `SealedSecretTable`/`SealedSecretRewrap` owner and exact-byte CAS. Make each
  browser-session inventory branch select every non-null sealed value,
  regardless of absolute expiry; do not add a second purge or rotation engine.
  Align the live-only rustdoc with that stored-ciphertext contract.
- **Focused closure proof:** store an already-expired, non-revoked browser
  session with non-null mode-appropriate envelopes; prove the canonical pass
  counts/rewraps all of them and that keyless boot refuses until those
  envelopes are wiped. Retain the existing live-session CAS and K2-only
  journey.

### FIND-TASK-003-5 — The secret-bearing BFF channel still lacks real TLS proof

- **Sources:** `BEH-R2-002`, `INV-R2-01`, `SYSTEM-R2-001`
- **Status:** **REVISED**
- **Classification:** `MISSING`
- **Violated obligation:** TASK-003's packet-local TLS boundary and prior
  remediation `R1-AC-05`, which explicitly requires a real
  production-shaped TLS channel exercise in addition to URL-policy tests.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:3-24`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:8-64`;
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:87-121,246-307`.
- **Producer-to-consumer evidence:** all `ServerSessions` channel and API calls
  derive their origin from `serverUrl` and send the raw BFF key or returned
  authority through native `fetch`. The unit test accepts an `https:` string
  but replaces fetch with a recording function that opens no connection. The
  real two-process Node journey receives `WyrdTestServer::base_url()`, a
  loopback `http:` origin, and therefore exercises only the local exemption.
- **Observable consequence:** the required proof remains green if the built
  Node BFF cannot establish or authenticate the deployed HTTPS hop; the URL
  guard alone does not prove the secret-bearing path works over TLS.
- **Decision-complete smallest correction:** extend the existing identity UI
  journey, reusing repository-managed TLS material and the same Wyrd/BFF
  processes, so at least one ordinary authenticated browser-session operation
  reaches the Wyrd endpoint through a trusted `https:` origin. Keep the native
  fetch path, existing loopback journey coverage, and non-loopback plaintext
  refusal; add no bypass flag, alternate client, or second auth path. A
  separate untrusted-certificate/replacement scenario is not required for this
  finding.
- **Focused closure proof:** the built Node BFF completes a real private
  session operation over the TLS origin and receives the server response while
  the existing unit test still proves non-loopback plaintext is refused before
  fetch.

### FIND-TASK-003-6 — The required provider topology and mixed-callback browser proof remain incomplete

- **Sources:** `BEH-R2-003`; focused follow-up section 2
- **Status:** **CONFIRMED**
- **Classification:** `MISSING`
- **Violated obligation:** approved `AC-003`, original TASK-003 scenario 2,
  prior remediation `FIND-TASK-003-6`, and `R1-AC-06` require concurrent
  different real providers plus credible wrong-provider and same-issuer
  cross-tenant callback refusal in the existing real browser journey.
- **Exact location:**
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:27-52,180-224,246-320`;
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:101-205,460-544`.
- **Producer-to-consumer evidence:** the host starts against the repository's
  Keycloak/Dex identity lane but activates both switch tenants from different
  realms of the same Keycloak service. `providerLogin` returns the complete
  callback URL; both attack cases extract only `code`, then
  `expectMixedCallbackRefused` constructs a new `{code,state}` query. The
  provider-produced `iss` is discarded, so an advertising provider is refused
  by the missing-issuer rule before the intended state/PKCE mix-up path is
  established.
- **Observable consequence:** provider-service-specific BFF switching and a
  standards-shaped mixed callback can regress while the named primary journey
  continues to claim the full AC-003/R1-AC-06 topology.
- **Decision-complete smallest correction:** keep the existing identity lane,
  host, and two BFF replicas. Use its already-running Dex service as the second
  active provider without weakening the existing connection-test contract.
  Preserve every genuine callback query parameter and replace only the victim
  `state`; in the same-issuer case retain the genuine matching `iss` so the
  intended state/PKCE binding, not missing-issuer validation, decides refusal.
  Add no provider dependency or second harness.
- **Focused closure proof:** the named multi-provider journey establishes and
  switches independent Keycloak and Dex sessions on both replicas, then proves
  the complete wrong-provider and same-issuer callbacks create no completion
  or session. Retain the provider-replacement and earlier UI journeys.

### FIND-TASK-003-10 — Refused proactive renewal revokes still-valid browser authority early

- **Sources:** `INV-R2-02`; focused follow-up section 3
- **Status:** **CONFIRMED**
- **Classification:** `INCORRECT`
- **Violated obligation:** approved `REQ-016` and TASK-003 require an
  old-connection browser session to stop renewing immediately but retain its
  already issued access authority until that token expires, absent an
  independently stronger principal or tenant block.
- **Exact location:**
  `crates/wyrd/wyrd-sql/src/queries/auth/browser_sessions.rs:45-63,241-278`;
  `crates/wyrd/wyrd-auth/src/browser_sessions.rs:52-54,320-390,435-567`;
  pinned proof at
  `crates/wyrd/wyrd-server/tests/identity_ui_e2e.rs:246-267` and
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/production-auth.integration.test.ts:379-396`.
- **Producer-to-consumer evidence:** Postgres marks a token stale for the whole
  minute before expiry. Every BFF read/authority call enters
  `BrowserSessions::current`, which attempts renewal. A connection/key/policy
  refusal then wipes the browser row and commits the transaction even when
  `access_expires_at` is still in the future. With the journey's 30-second
  token lifetime, every first read is proactive and the existing deactivation
  assertion positively pins immediate logout.
- **Observable consequence:** connection replacement/deactivation or API-key
  revocation can end a browser session up to one minute before the issued
  access token's approved expiry, contradicting the snapshot-authority
  lifecycle and UI documentation contract.
- **Decision-complete smallest correction:** keep lifecycle ownership in
  `BrowserSessions`, the existing row lock, and the PostgreSQL clock authority.
  Distinguish a still-valid token inside the proactive margin from an expired
  token. If early renewal is refused, do not commit tentative refresh/key-use
  mutations or browser-row revocation; serve only the already-issued token
  until its stored expiry. On the first post-expiry use, refuse and revoke as
  today, without trying another credential. Infrastructure failures remain
  fail closed.
- **Focused closure proof:** a real-store session bound to a connection (and
  the equivalent API-key mode where practical) is used after renewal becomes
  disallowed but before access expiry, then refused on its first post-expiry
  use. Assert no successor token is issued, no tentative renewal mutation is
  committed, and the existing concurrency/ordinary-renewal paths remain green.

### FIND-TASK-003-11 — New OIDC discovery items violate mandatory rustdoc rules

- **Sources:** `STD-R2-001`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** `AGENTS.md` section 16 and
  `architecture/agent-rules.md` require substantive rustdoc on every new or
  materially modified Rust item, including private fields/helpers, and
  `# Errors` on every fallible function.
- **Exact location:**
  `crates/shared/wyrd-auth-oidc/src/provider.rs:18-30,53-90`.
- **Producer-to-consumer evidence:** the remediation added
  `RawProviderMetadata.authorization_response_iss_parameter_supported` with
  `serde(default)` but no owning-field explanation of absent-as-false wire
  semantics. It materially modified `parse_raw_metadata`, which parses three
  endpoint URLs and returns `OidcError::Discovery`, but the helper has no
  rustdoc or `# Errors` section. `OidcProvider::discover` is their sole caller;
  the public projection's documentation does not satisfy the explicit
  private-item rule.
- **Observable consequence:** the candidate violates a hard merge rule at the
  provider trust boundary even though compilation and existing tests pass.
- **Decision-complete smallest correction:** document only the added raw field
  and `parse_raw_metadata`: state the absent-is-false wire behavior, the
  conversion's role, and which malformed endpoint fields produce
  `OidcError::Discovery`. Add no wrapper or documentation abstraction.
- **Focused closure proof:** direct source inspection plus the owning format
  and lint/doc gates; retain the existing discovery projection test.

### FIND-TASK-003-12 — Cookie hints fan out through unbounded concurrent private reads

- **Sources:** `STD-R2-002`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** the applicable TypeScript concurrency authority
  requires an explicit bound for independent work and forbids bare
  `Promise.all` over an unbounded collection.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/auth/server-sessions.ts:186-217,283-306`;
  caller at the same file's `sessionMetadata` and tenant layout consumers.
- **Producer-to-consumer evidence:** `metadata` collects every distinct
  syntactically valid request-cookie suffix, then maps the entire set through
  `Promise.all`. Each `read` sends an authenticated internal request that
  performs server/session/tenant-store work before an invalid hint is cleared.
  No application-owned count or concurrency limit exists.
- **Observable consequence:** one request with many forged but valid-looking
  cookie names concurrently amplifies into many private-channel and database
  reads.
- **Decision-complete smallest correction:** retain `ServerSessions.metadata`
  and the existing `read` owner, but resolve the distinct hints sequentially.
  This supplies an explicit bound of one with native language behavior and no
  dependency, queue, worker, or new abstraction. Preserve deduplication,
  invalid-cookie clearing, and server-returned metadata.
- **Focused closure proof:** extend the focused metadata test with multiple
  forged hints and a recording read/channel boundary; assert maximum in-flight
  resolution is one and the final rendered/cleared result is unchanged.

### FIND-TASK-003-13 — Empty upstream configuration bypasses validation via falsey defaulting

- **Sources:** `STD-R2-003`
- **Status:** **CONFIRMED**
- **Classification:** `VIOLATION`
- **Violated obligation:** the TypeScript authority requires nullish semantics
  for defaults, and security-sensitive configuration must validate the
  effective configured value rather than silently reinterpret it.
- **Exact location:**
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.ts:14-24` and the
  contrary expectation at
  `crates/wyrd/wyrd-server/wyrd-ui/src/lib/server/upstream.test.ts:35-47`.
- **Producer-to-consumer evidence:** every readiness, login, private session,
  and normal API call uses `serverUrl`. Its `||` expression maps an explicitly
  empty environment value to `http://127.0.0.1:8080`, so the URL and TLS checks
  validate the fallback rather than the supplied invalid deployment value.
- **Observable consequence:** malformed production configuration silently
  targets a loopback listener instead of refusing startup/use, potentially
  reaching the wrong local process or producing misleading outage behavior.
- **Decision-complete smallest correction:** change the existing default to
  nullish-only fallback (`??`), so absence retains the local default and an
  explicit empty string reaches the existing native URL parser and is refused.
  Add no new config surface or validator.
- **Focused closure proof:** one focused upstream test proves an absent value
  uses the loopback default, while an explicit empty value throws before a
  recording fetcher observes a request; retain HTTPS, loopback, and plaintext
  refusal cases.

## Validation result

**FIX_REQUIRED**

The validated ledger contains the still-open prior findings
`FIND-TASK-003-4`, `FIND-TASK-003-5`, and `FIND-TASK-003-6`, plus new bounded
findings `FIND-TASK-003-10` through `FIND-TASK-003-13`. The explicit human
issuer-binding replacement is satisfied and receives no finding ID. No
retained correction requires a new product, public API, architecture,
security, compatibility, cross-service, concurrency-semantics,
resource-ownership, or persistent-data decision; each stays within an existing
owner and approved behavior.
