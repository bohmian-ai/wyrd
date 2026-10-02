# Cache and concurrency domain review

## Subject and reviewed boundary

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `0bd3686e8bb763b07376f84661946aadc6200bfb`
- Candidate tree: `86fb8e11681b4d2e60bba2aa7ee3bf2c4f134153`
- Authority: approved `SPEC-oidc-production-readiness` revision 11,
  `TASK-009-oidc-relying-party.md`, the cumulative remediation record through
  `TASK-009-R3-platform-login-boundary-and-contract.md`, `AGENTS.md`, and
  `architecture/agent-rules.md`.
- Boundary: the process-local Moka provider cache, clone and process lifetime,
  cold and invalidated misses, forced discovery, unknown-key refresh,
  cancellation/failure recovery, tenant/platform ownership, and replica scope.

The commit and tree matched the immutable subject before and after this review.
This repository has no `.codegraph/` index, so source navigation used the
repository's ordinary search and commit-qualified diff. `FIND-TASK-009-5`,
`FIND-TASK-009-14`, and `FIND-TASK-009-15` are withdrawn by binding lead
direction and were not reopened directly or indirectly. Placement, naming,
structure, and wording were not treated as blocking concerns.

## Authority and source coverage

| Boundary | Source evidence | Assessment |
|---|---|---|
| Cache owner and lifetime | `crates/shared/wyrd-auth-oidc/src/relying_party.rs:343-375` owns one bounded five-minute `moka::future::Cache<String, Arc<ProviderMetadata>>`; `RelyingParty` clones share that cache. | PASS |
| Cold and invalidated misses | `RelyingParty::cached` (`relying_party.rs:377-395`) uses Moka's native `try_get_with`, so overlapping same-issuer misses in one process share the initializer and failed initialization is not retained. | PASS |
| Fresh discovery | `RelyingParty::discover` (`relying_party.rs:397-435`) fetches metadata and JWKS before inserting, replaces the same issuer entry on success, and leaves the prior entry intact on fetch failure. Tenant candidate testing and platform configuration call this existing operation. | PASS |
| Unknown-key retry | `RelyingParty::redeem` (`relying_party.rs:479-550`) invalidates only after the library reports `UnknownKey`, re-enters the cached path once, and performs one terminal verification with no retry loop. This satisfies the approved per-redemption bound without promising a global request count. | PASS |
| Overlapping refresh behavior | `overlapping_cache_misses_share_one_discovery` and `concurrent_rotated_key_redemptions_share_one_refresh` (`relying_party.rs:1374-1440`) exercise cloned handles, count discovery/JWKS requests, cover rotated and still-unknown keys, and prove the intended overlapping process-local single-flight behavior. The approved remediation expressly excludes a stronger all-interleaving or distributed guarantee. | PASS |
| Tenant owner sharing | `HumanConnections` owns one `RelyingParty` and is cheap to clone (`crates/wyrd/wyrd-auth/src/connections.rs:84-105,148-178`). Tenant begin, callback, candidate testing, and BFF composition use that owner rather than constructing request-local caches. | PASS |
| Platform owner sharing | `PlatformLogin` owns one `RelyingParty` (`crates/wyrd/wyrd-auth/src/platform_login.rs:90-141`); boot constructs one process owner and stores it in `ServerAuth` (`crates/wyrd/wyrd-server/src/boot/mod.rs:1585-1627`; `components/auth/state.rs:34-42`). Begin, callback, and configuration borrow it. | PASS |
| Platform configuration and next login | `configure_connection` performs full discovery through the process-owned platform relying party before the durable write (`components/platform/identity.rs:249-284`). The served platform proof covers unavailable/undecodable JWKS without durable replacement and same-issuer cache refresh used by subsequent begin/callback. | PASS |
| Plane and tenant isolation | Tenant human login and platform login own separate caches. Tenant connections sharing an issuer may share only issuer-global provider metadata/JWKS; connection client IDs, secrets, mappings, state, and revisions remain outside the cache. | PASS |
| Cancellation, outage, and recovery | A cancelled or failed fetch publishes no provider entry; a failed `try_get_with` initializer is not cached. A rediscovery failure returns before token acceptance. Callback state is already single-use before provider IO, so interruption fails the request closed and recovery is a new standard login attempt. | PASS |
| Restart and replicas | The cache is deliberately process-local. Restart causes ordinary rediscovery, and replicas may fetch independently; durable connection and login state remain in Postgres. The task and remediations require no cross-replica invalidation, lock, cache, or request-count guarantee. | PASS |
| Round-three delta | `045979092..0bd3686e8` changes the platform configuration error declaration and focused proof, then restores the lead-directed `PlatformLogin::relying_party` accessor. It changes no cache algorithm, ownership, lifetime, invalidation, or retry behavior. | PASS |

## Material findings

None.

The prior process-local cache-concurrency defect remains closed. The candidate
uses the already installed Moka single-flight path and preserves exactly one
forced rediscovery per redemption. Requiring cache generations, a custom lock,
cross-replica coordination, or an all-interleaving request-count promise would
exceed both standard relying-party practice and the approved task.

## Verification evidence and limits

Focused candidate verification run during this review:

```text
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
  -E 'test(=relying_party::tests::overlapping_cache_misses_share_one_discovery) | test(=relying_party::tests::concurrent_rotated_key_redemptions_share_one_refresh) | test(=relying_party::tests::the_cache_serves_a_discovered_provider)'
3 tests run: 3 passed, 44 skipped
```

The orchestrator also reported the correctly wrapped exact candidate selectors
green for `federated_platform_sign_in_runs_through_the_served_callback` and
`the_served_document_describes_the_composed_surface` (one selected test each).
Direct unwrapped attempts were environment/setup failures only and provide no
negative product evidence.

No full identity journey or every-language suite was run in this domain pass.
Under standing human direction, this task review uses the narrowest relevant
lanes; full journeys run at change review. No verification limit prevents a
cache/concurrency conclusion.

## Overall result

**PASS**
