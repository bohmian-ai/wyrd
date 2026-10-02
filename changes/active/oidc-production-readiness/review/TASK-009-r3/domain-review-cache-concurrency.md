# Cache and concurrency domain review

## Subject and reviewed boundary

- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`
  revision 11, `TASK-009-oidc-relying-party.md`, and
  `TASK-009-R2-relying-party-corrections.md`.
- Sensitive boundary: the process-local Moka cache owned by `RelyingParty`,
  its tenant and platform owners, cold and expired misses, unknown-key
  refresh, same-issuer replacement, cancellation/outage behavior, and process
  or replica restart.
- Candidate stability: the requested commit and tree resolved throughout this
  pass (`045979092...`, tree `0905c9a3...`). Review used commit-qualified
  source because the shared review worktree HEAD contains review artifacts.
  CodeGraph is not indexed in this repository.

`FIND-TASK-009-5` and `FIND-TASK-009-14` remain withdrawn by lead direction
and were not reopened. This review applies the standing direction that Wyrd
uses standard/library behavior: no generation protocol, distributed cache,
cross-replica invalidation, cache setting, or coordination mechanism is
required or proposed.

## Authority and source coverage

| Boundary | Evidence | Assessment |
|---|---|---|
| Cache shape and lifecycle | `relying_party.rs:343-415` owns one bounded, five-minute `moka::future::Cache<String, Arc<ProviderMetadata>>`; clones share it. `HumanConnections` and `PlatformLogin` are each built once per process and own their respective relying party. | PASS |
| Cold and expired misses | `RelyingParty::cached` at `relying_party.rs:377-395` uses Moka `try_get_with`, whose installed API serializes same-key initialization and does not cache errors. `overlapping_cache_misses_share_one_discovery` (`:1374-1402`) covers cold and invalidated bursts through cloned handles. | PASS |
| Forced discovery and replacement | `RelyingParty::discover` (`:397-415`) fetches first and inserts only on success, so failure leaves the prior entry intact. Tenant candidate testing calls it from `connections.rs:342-366`. The r2 delta routes served platform configuration through the boot-owned `PlatformLogin` cache at `components/platform/identity.rs:235-274`; the stored row is written only after full discovery/JWKS succeeds. | PASS |
| Unknown-key rotation | `RelyingParty::redeem` (`relying_party.rs:479-550`) invalidates once only after `NoMatchingKey`, re-enters `cached`, verifies once against that result, and has no retry loop. `concurrent_rotated_key_redemptions_share_one_refresh` (`:1404-1440`) covers successful and still-unknown concurrent bursts and proves one shared process-local refresh. | PASS |
| Tenant/platform callers | Tenant begin/callback share `HumanConnections::relying_party`; platform begin/callback/configure share `PlatformLogin::relying_party` (`platform_login.rs:83-141,156-181,238-318`). Tenant and platform caches remain separate, while same-issuer tenants correctly share issuer-global discovery metadata within the tenant owner. | PASS |
| Outage and cancellation | `fetch` and `discover` publish no partial entry (`relying_party.rs:397-435`); `cached` does not retain a failed initializer. Verification failure and rediscovery failure return without accepting a token. Callback state is consumed before remote exchange, so cancellation or outage requires a fresh standard login attempt rather than replaying local state. | PASS |
| Restart and replicas | The cache is intentionally in-memory and process-owned; restart causes ordinary rediscovery. Each replica may discover independently. Durable login state and connection selection remain in Postgres. Neither OIDC/openidconnect behavior nor the approved task promises cross-process request coalescing or cache coherence. | PASS |
| Same-issuer configuration | Full platform configuration refreshes the current process's cache before the row commit. The recorded served proof changes the published key for the same issuer, reconfigures, then completes begin/callback during discovery/JWKS outage from that refreshed entry. Provider metadata is issuer-global and contains no tenant/client secret or client-specific state. | PASS |
| Latest remediation delta | `1ddc10e2..045979092` changes no cache algorithm: it removes a nonstandard audience override and makes platform configuration use the existing process-owned `discover`; no second cache, lock, option, retry policy, or invalidation service was introduced. | PASS |

## Recovery and concurrency assessment

The task's “exactly one forced re-discovery” is a per-redemption bound, while
Moka provides ordinary process-local same-key single-flight. The candidate
meets both. A replica restart discards the optimization and safely rediscovers;
an IdP outage fails the affected login request closed without taking the
server process or unrelated issuers offline. A failed forced discovery is not
cached, so later independent login attempts can recover when the provider
does. Cross-replica request counts and immediate distributed invalidation are
not standard relying-party guarantees and are outside the approved task.

## Material findings

None.

The prior cache/concurrency finding remains closed: the cumulative candidate
uses the existing Moka mechanism for overlapping misses and refreshes, and the
focused concurrent tests exercise the source path rather than a parallel test
implementation. No standard-required cache or concurrency mechanism is
missing, and no nonstandard mechanism entered the remediation.

## Verification evidence and limits

Focused verification rerun against the candidate-equivalent source in the
shared worktree:

```text
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
  -E 'test(=relying_party::tests::overlapping_cache_misses_share_one_discovery) | test(=relying_party::tests::concurrent_rotated_key_redemptions_share_one_refresh) | test(=relying_party::tests::the_cache_serves_a_discovered_provider)'
3 tests run: 3 passed, 44 skipped
```

The immutable remediation record also reports the served platform
configuration journey, the shared and server lanes, and the unfiltered
identity journey green. I did not rerun broad user-journey or every-language
sweeps; under standing human direction those belong once at change review,
not this task-domain pass. No Python, TypeScript, SDK, or generated contract
source changed in r2, so their absence here is not a verification gap.

## Overall result

**PASS**
