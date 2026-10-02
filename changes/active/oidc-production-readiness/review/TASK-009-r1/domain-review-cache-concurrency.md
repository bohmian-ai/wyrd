# Cache, concurrency, and recovery domain review

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `6578921d8ced6316c850e4d8f16bd101630a7056`
- Candidate stability: `HEAD` and the requested candidate both resolved to
  `6578921d8ced6316c850e4d8f16bd101630a7056` before and after inspection.
- Authorities read: `AGENTS.md`, `architecture/agent-rules.md`,
  `architecture/wyrd-design.md`, `architecture/wyrd-security-posture.md`,
  `architecture/references/languages/spec-driven-development.md`,
  `architecture/references/languages/maintainer-style.md`, approved spec
  revision 11, TASK-009, the T1 research report, and TASK-004 round-2 lead
  routing (including FIND-TASK-004-13).
- Standing direction applied: a mechanism beyond standards or comparable
  common practice is DRIFT and is not required here. The review requires no
  distributed cache, distributed lock, new setting, or cross-replica
  coordination.

## Boundary and source coverage

| Boundary | Source and caller evidence | Result |
|---|---|---|
| Cache identity and contents | `relying_party.rs:49-57,291-375`: bounded, five-minute `moka::future::Cache<String, Arc<ProviderMetadata>>`, keyed by the exact issuer string and containing the discovered metadata plus JWKS | PASS |
| Clone and owner lifecycle | `relying_party.rs:291-321`; `connections.rs:84-177`; `platform_login.rs:82-123`: `RelyingParty` clones share its `moka` cache; `HumanConnections` owns one process-local tenant-login cache; `PlatformLogin` owns a separate platform cache | PASS |
| Tenant login and callback | `login.rs:95-149`; `callback.rs:134-208`: begin and ordinary callback use the same `HumanConnections` cache; connection-test callbacks deliberately force discovery before validating the exact candidate | PASS, subject to CACHE-1 |
| Connection test | `connections.rs:314-390`; `callback.rs:152-171`: test begin and completion rediscover through the same screened transport and bind the exact candidate revision | PASS |
| Platform login | `platform_login.rs:138-164,220-305`: begin and callback share the platform relying party, and an unknown key uses the same bounded retry path | PASS, subject to CACHE-1 |
| Admin and boot consumers | `wyrd-server/src/components/admin/routes.rs:735-771`; `wyrd-server/src/boot/issuer.rs:100-183,220-260`: one-shot admin discovery and bounded boot retry remain outside the login cache and fail closed | PASS |
| Cancellation, outage, and recovery | `relying_party.rs:348-375,419-488,1171-1200`: failed discovery leaves a prior entry intact; a callback consumes state before provider IO and therefore requires a new login after cancellation/outage; no unverified token proceeds | PASS, subject to CACHE-1 outage amplification |
| Multi-replica behavior | Each server process owns its own cache through its constructed `HumanConnections` and `PlatformLogin`; durable login state remains in Postgres. No approved authority asks for cache coherence across replicas | PASS; process-local caches are conventional and sufficient |
| Unknown-key proof | `relying_party.rs:1015-1082` proves one sequential successful refresh and one sequential still-unknown refusal; `1225-1240` proves sequential clone sharing | INCOMPLETE for concurrent callbacks (CACHE-1) |

## Exactly-one interpretation

TASK-009's “exactly one forced re-discovery” is a **per-redemption retry
bound**: after verification reports `NoMatchingKey`, that redemption may
refresh once and then either verify or fail closed. It is not a deployment-wide
promise that all replicas perform one network request in total. Independent
process caches and one refresh per replica are standard and match the approved
architecture.

That interpretation does not permit avoidable duplicate refreshes inside one
process. The replaced cache explicitly used `moka::Cache::try_get_with` to
coalesce same-key work, and process-local single-flight is ordinary cache
behavior. Preserving it needs no new mechanism or option.

## Material finding

### CACHE-1 — REGRESSION: concurrent callbacks no longer coalesce issuer discovery or unknown-key refresh

- **Violated obligation:** TASK-009 requires the existing `moka`-backed
  per-issuer metadata/JWKS cache, one bounded unknown-key refresh, unchanged
  user behavior, and fail-closed outage handling. The base cache's established
  contract coalesced concurrent same-key misses through
  `moka::Cache::try_get_with` (`35a53faa2:crates/shared/wyrd-auth-oidc/src/jwks.rs:232-317`).
- **Location:** `crates/shared/wyrd-auth-oidc/src/relying_party.rs:331-375`
  and `:477-485`.
- **Evidence:** `cached` performs a separate `get` followed by `discover`, and
  its own rustdoc states that concurrent misses each discover and the last
  insertion wins. `redeem` holds the stale `Arc<ProviderMetadata>` supplied by
  its caller; every concurrent callback that observes that generation and gets
  `UnknownKey` calls `discover` unconditionally. There is no cache-generation
  recheck or coalesced computation. The two unknown-key tests run one redemption
  at a time, so their two-request assertions cannot expose this path.
- **Reachable consequence:** after TTL expiry, a burst of callbacks produces
  one discovery-document and JWKS fetch per callback. During signing-key
  rotation, every callback that exchanged a token against the stale generation
  can repeat both requests. During an IdP slowdown or outage this changes a
  bounded shared miss into O(concurrent callbacks) load and can prolong the
  dependency failure. Authentication still fails closed, but the candidate
  regresses the cache's established concurrency and recovery behavior.
- **Required correction:** use the existing process-local `moka` cache to
  coalesce discovery for one issuer, including callbacks refreshing the same
  observed cache generation. A callback that finds another callback already
  installed a newer generation should verify against it; each redemption must
  still make at most one refresh attempt before refusing. Keep caches local to
  each replica and keep tenant and platform owners separate. Do not add a
  distributed cache, distributed lock, public option, configuration file, or
  retry loop.
- **Focused proof:** prime one issuer with the old key, concurrently redeem
  multiple tokens naming the rotated key through clones of one
  `RelyingParty`, and assert one process-local refreshed discovery/JWKS fetch,
  successful verification for the rotated key, and no second refresh for a
  still-unknown key. Also cover concurrent cold/expired-cache reads so one
  issuer miss is coalesced.

## Verification limits

- The recorded unit and journey lanes exercised commit `b97fa1c46`, before
  workspace-hack regeneration. Commit `9ef532660` changed the effective test
  feature union for crypto dependencies (`crypto-bigint`, `ed25519-dalek`,
  `elliptic-curve`, and `rsa`, while removing the direct `signature` entry).
  Rerunning only `mise run lints` and `mise run check:workspace-hack` proves the
  regenerated graph compiles and is current; it does not execute ID-token
  algorithm, signature, JWKS rotation, or identity journeys under the final
  candidate. That is not sufficient final behavioral evidence for this
  domain. At minimum, the relying-party tests and the identity journey must be
  rerun after the regeneration; CACHE-1 additionally needs the focused
  concurrency proof above.
- No concurrent unknown-key test exists in the candidate. The sequential tests
  credibly establish the per-redemption one-retry bound but not coalescing or
  race behavior.
- `git diff --check` passed. No dynamic test was rerun by this read-only domain
  review.

## Overall result

**FAIL** — CACHE-1 is a reachable process-local cache regression. The
per-redemption security bound, fail-closed behavior, issuer keying, clone
lifecycle, and independent multi-replica caches are otherwise sound.
