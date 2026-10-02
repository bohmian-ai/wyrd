# Cache concurrency domain review

## Subject and boundary

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `1ddc10e21054ddc158f461e8c6d8aa862c32a067`
- Candidate stability: `HEAD` resolved to the requested candidate before and after review.
- Scope: process-local Moka ownership, issuer cache keys, cold/expired miss
  coalescing, unknown-`kid` invalidation and one-retry behavior, clone sharing,
  races, and outage/recovery.
- Prior finding: `FIND-TASK-009-4` was reassessed against the cumulative
  candidate and remediation diff. `FIND-TASK-009-5` is withdrawn by lead
  direction and was not reopened.

The review applies the standing human direction that Wyrd uses conventional
standard mechanisms. It requires no distributed cache, generation protocol,
custom lock, cross-replica coordination, new setting, or stronger
all-interleaving guarantee.

## Authority and source coverage

Authorities read: `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/references/languages/spec-driven-development.md`,
`architecture/references/languages/maintainer-style.md`, approved specification
revision 11, TASK-009, the round-1 verdict and validation ledger,
`TASK-009-R1-relying-party-corrections.md`, and the lead withdrawal for
`FIND-TASK-009-5`.

| Boundary | Source evidence | Result |
|---|---|---|
| Cache owner and key | `crates/shared/wyrd-auth-oidc/src/relying_party.rs:343-395` owns one bounded five-minute `moka::future::Cache<String, Arc<ProviderMetadata>>`, keyed by exact issuer string and containing issuer metadata plus JWKS | PASS |
| Cold and expired misses | `RelyingParty::cached` uses Moka `try_get_with`, so overlapping same-issuer misses share the fetch and failures are not cached (`relying_party.rs:377-395`) | PASS |
| Explicit fresh discovery | Candidate connection testing alone uses `discover`, which fetches immediately and then replaces the issuer entry (`relying_party.rs:397-435`; `connections.rs:342-356`). Ordinary tenant and platform login use `cached` | PASS |
| Tenant owner sharing | `HumanConnections` owns the relying party and documents clone sharing (`connections.rs:84-104,145-177`); tenant begin and ordinary callback use that owner (`login.rs:100-120`; `callback.rs:151-178`) | PASS |
| Platform owner sharing | One `PlatformLogin` is built during boot and retained in `ServerAuth`; both served begin and callback borrow it (`boot/mod.rs:1585-1630`; `components/auth/state.rs:34-42`; `components/platform/identity.rs:644-739`) | PASS |
| Tenant/platform partitioning | Tenant `HumanConnections` and platform `PlatformLogin` each construct their own `RelyingParty`; caches are process-local and plane-separated while tenants sharing an issuer may safely share issuer metadata/JWKS | PASS |
| Unknown-`kid` retry | `redeem` verifies against the supplied cached generation, invalidates the issuer only on `UnknownKey`, re-enters `cached`, and verifies once more with no loop (`relying_party.rs:479-550`) | PASS |
| Clone behavior | `RelyingParty` derives `Clone` over Moka's shared cache; `the_cache_serves_a_discovered_provider` proves cloned owners return the same cached `Arc` (`relying_party.rs:343-353,1288-1303`) | PASS |
| Concurrent refresh proof | `overlapping_cache_misses_share_one_discovery` proves one discovery/JWKS fetch for cold and invalidated entries; `concurrent_rotated_key_redemptions_share_one_refresh` proves cloned overlapping redemptions share one refresh, rotated keys succeed, still-unknown keys fail, and each redemption performs one token exchange (`relying_party.rs:1305-1435`) | PASS |
| Outage and recovery | Coalesced fetch failures are returned and not cached, so the request fails closed and a later request can retry. A still-valid cached provider remains usable until TTL/invalidation; the served platform journey records callback success after discovery/JWKS become unavailable | PASS |
| Multi-replica semantics | Each process owns its cache; durable login state remains outside it. TASK-009 and remediation explicitly reject deployment-wide coordination or a one-request promise across replicas | PASS |

## Prior-finding closure

### `FIND-TASK-009-4` — CLOSED

The prior candidate performed `get` followed by unconditional discovery and
therefore multiplied cold, expired, and rotated-key fetches. The remediation
places the shared computation back in Moka's installed `try_get_with` path.
Unknown-key handling invalidates and re-enters that same path, while the local
control flow still permits only one verification retry per redemption. The
focused tests exercise the required overlapping race through clones and count
discovery, JWKS, and token requests.

This is the conventional process-local single-flight behavior requested by the
task. A caller arriving after a prior refresh has completed can cause another
refresh if it independently observed the old generation and invalidates later;
the remediation expressly excludes a stronger all-interleaving guarantee, and
adding generation state or custom locking would be DRIFT.

## Material findings

None.

## Verification and limits

I ran the focused final-candidate command:

```text
mise exec -- cargo nextest run --locked -p wyrd-auth-oidc --lib \
  -E 'test(=relying_party::tests::overlapping_cache_misses_share_one_discovery) | test(=relying_party::tests::concurrent_rotated_key_redemptions_share_one_refresh) | test(=relying_party::tests::a_still_unknown_key_fails_after_one_rediscovery)'
```

Result: **3 tests run, 3 passed, 44 skipped**.

The remediation evidence also records the two concurrency tests and the
sequential still-unknown-key test as individually selected and passing on code
candidate `0b516e235`, plus the unfiltered identity and Wyrd lanes. This review
did not rerun the five-minute wall-clock TTL; invalidation exercises the same
Moka miss path without adding a slow or custom-clock test. It did not test
cross-process request counts because those semantics are explicitly outside
the approved task.

## Overall result

**PASS** — `FIND-TASK-009-4` is closed. Cache ownership, issuer partitioning,
clone sharing, process-local miss coalescing, bounded unknown-key retry,
fail-closed outage behavior, and recovery all satisfy the task without an
extra mechanism.
