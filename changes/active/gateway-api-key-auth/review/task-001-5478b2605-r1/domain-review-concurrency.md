# Concurrency domain review

Overall result: **FAIL**.

Subject: base `e0a926031155f263a8721c47d4662dfed8c125ea`, candidate
`5478b2605acae3cecd2522941c30ee7f1674f0be`. HEAD still names the candidate.
Reviewed the complete cumulative diff, approved specification r1, original
TASK-001, and the shared subject/navigation map. This report covers shared
process ownership, locking, overlapping exchanges, cache age and eviction.

## Authority and source coverage

Applied AGENTS.md, architecture/agent-rules.md, the reference router,
spec-driven-development and maintainer-style references, Rust/testing
references, and the applicable identity/delegation and credential sections of
wyrd-design.md, wyrd-doctrine.mdx, wyrd-security-posture.md and
v1/00-foundations/security.md. REQ-001's explicit revocation bound governs the
cache even though TASK-001 describes expiry after cache admission.

| Boundary | Source and traced consumers | Assessment |
|---|---|---|
| Process lifetime and sharing | state.rs AppState::new and gateway_api_keys; ingress.rs GatewayIngress::layer/authenticate | PASS: AppState clones share one Arc cache; no request-local recreation or detached task owns it. Restart clears it. |
| Lock and eviction | gateway/api_keys.rs GatewayApiKeys::{get,insert,with_capacity}; comparison with verification/cache.rs VerifierCache | PASS: one Mutex serializes LRU mutation; no guard crosses an await. Capacity is 4096 entries. Expired entries are removed on lookup, and eviction owns/drops cached secrets. |
| Identity partition and hits | GatewayApiKeys::hash/get; verify_access_token; GatewayIngress::authenticate | PASS: complete-key SHA-256 identifies entries. A token is cloned out of the lock and verified before yielding its principal; hits change recency but not the stored deadline. Eviction while a hit owns its clone is safe. |
| Concurrent misses and failure | GatewayApiKeys::authenticate; TokenGrants::api_key/api_key_exchange; ExchangeApiKey::execute; issuer and SQL queries | FAIL on deadline origin below. Multiple misses can exchange independently and each stages its own exchange audit; no requirement calls for single-flight. Failed exchanges insert nothing. Out-of-order success replaces the same-key entry, making the late-admission issue relevant to every exchange. |
| Revocation producer | principals/routes.rs revoke_credential_for; SQL auth/api_keys.rs revoke_api_key; service_accounts.rs api_key_by_prefix/touch_api_key_last_used | Credential revocation commits independently of gateway cache admission. Revoking one credential leaves its principal active, so its already-issued JWT remains verifiable. |
| Existing owners | VerifierCache; client AuthMiddleware reuse map; TokenGrants and issuer | A separate gateway expiring-token cache is required by the spec. The immutable-Card cache and client single-credential/disk workflow do not own this behavior. Grant, verification, transaction and audit orchestration remain with existing owners. |
| Proof and remaining changed surfaces | api_keys.rs unit test; native.rs new journeys and helpers; harness.rs; Python/TS stock-client changes; OpenAPI and guide changes | Existing proof covers sequential fixed expiry, partition, bounds and warmed-then-revoked calls. Other changed surfaces add no cache synchronization or lifetime owner. No overlapping exchange/revocation proof exists. |

## Proposed material finding

### CONC-001 — Late exchange completion grants a new revocation window

- Classification: **INCORRECT**.
- Violated obligation: approved REQ-001, “Revoking a key takes effect within
  60 seconds”; TASK-001 also says synchronization cannot extend that window.
- Changed location: `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:106-117`
  (`authenticate` starts the cache clock only after the awaited grant), and
  `:158-163` (`insert` adds a fresh 60 seconds to that instant).
- Producer-to-consumer evidence: `ExchangeApiKey::execute` authenticates the
  key before awaiting its use-record update and issuance
  (`wyrd-auth/src/exchange_api_key.rs:165-177`). The lookup is a plain SELECT
  filtering `revoked_at IS NULL`, without a row lock
  (`wyrd-sql/src/queries/auth/service_accounts.rs:352-380`), and the subsequent
  last-used UPDATE does not recheck revocation (`:416-432`).
  `revoke_credential_for` commits key revocation independently
  (`principals/routes.rs:509-557`). `TokenGrants::api_key` awaits issuance and
  commit before returning (`auth/routes.rs:237-260`). Cache admission is later
  still. The verifier checks the minted JWT locally; it does not reread this
  key's current revocation state.
- Reachable interleaving: a cold exchange reads an active key; another request
  revokes that credential while hashing/SQL work or scheduling delays the
  exchange; the original exchange completes for the still-active principal.
  If revocation commits at t=0 and completion/admission occurs at t=2, the
  new entry lasts until t=62. A new request at t=61 is accepted from cache,
  despite revocation having committed more than 60 seconds earlier. A delayed
  response after grant commit has the same result. No long suspension or
  synthetic host load is needed: any positive late-admission delay extends
  the stated bound. Overlapping misses can likewise let an older operation
  overwrite an entry with a newly started deadline.
- Observable consequence: a revoked static key authorizes fresh gateway
  requests beyond the approved 60-second revocation bound. The current
  journey revokes only after its warming request finishes, so it cannot expose
  this interleaving.
- Smallest testable correction: keep the existing gateway cache, grant,
  verifier, and short lock. Anchor each exchange's maximum reuse deadline at
  an instant captured before that exchange starts, rather than at completion;
  preserve that deadline across admission and replacement, cap by actual
  remaining token validity, and do not make an already elapsed result reusable.
  No single-flight mechanism, SQL ownership change, distributed invalidation,
  or new public option is needed. This changes a private timing choice to
  satisfy the approved bound; the task's admission wording must not override
  that bound.
- Focused closure proof: use controlled instants to represent an exchange
  beginning at t=0 and admission at t=2; show that a lookup at t=60 is a miss,
  and that admission after t=60 cannot create a live entry. Include an older
  exchange completing after a newer same-key exchange, verifying that the
  older result gains no fresh 60-second age. Retain the existing fixed-age,
  per-key and sequential real-server revocation coverage. A focused seam test
  should establish that authenticate supplies the pre-exchange instant, not
  merely test insert with a hand-selected correct argument.

## Verification limits

Static review only; no Cargo or broad verification lanes were run, in
accordance with the shared-review constraints. Inspected the task's recorded
unit, journey and stock-client outcomes as supplied evidence, not independent
execution. The finding follows the production call and SQL ordering; it was
not dynamically reproduced. No speculative single-flight, fairness, lock
poison recovery, or distributed-cache requirements were added.

Skipped independent test execution. The material risk is acceptance of a
revoked key beyond the specified window when exchange and revocation overlap.
