# TASK-009 round-3 system-resilience review

## Subject and result

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `35a53faa216b10651d85c96ce12e34f382cac637`
- Candidate: `04597909203463820b2033c12956f5fe6fcfe1f4`
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-009-oidc-relying-party.md`
- Remediation: `changes/active/oidc-production-readiness/review/TASK-009-r2/TASK-009-R2-relying-party-corrections.md`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Overall result: **PASS**

The cumulative candidate contains provider failure within the affected login or
configuration request, preserves durable identity and login state in Postgres,
and recovers through ordinary retry, cold discovery after restart, or the
existing one-shot unknown-key rediscovery. It adds no readiness dependency,
retry loop, distributed cache, cross-replica invalidation service, health probe,
or provider-specific branch.

`FIND-TASK-009-5` and `FIND-TASK-009-14` are withdrawn by binding lead
direction. This review does not reopen either finding. In particular, it does
not treat the audience-column migration as a rolling-release defect or require
the nonstandard preflight, equality constraint, or dual-write mechanism that
the lead rejected.

## Deployed paths and process ownership

| Path | Deployed owner and dependencies | Failure, restart, and recovery assessment | Result |
|---|---|---|---|
| Tenant begin, connection-test sign-in, and callback | `boot::install_auth` constructs one `HumanConnections` per `wyrd-server` process. Its clones share one `RelyingParty` and Moka provider cache; connection revisions, one-time state, users, roles, and completed sessions remain in Postgres (`crates/wyrd/wyrd-server/src/boot/mod.rs:1585-1590`; `crates/wyrd/wyrd-auth/src/connections.rs:84-105`; `crates/wyrd/wyrd-auth/src/callback.rs:99-207`). | Discovery, JWKS, and token failures end only that login. Callback state is consumed before provider IO, so cancellation or a provider failure cannot replay it; the user begins again. A process restart loses only cached provider metadata and rediscovers from the durable connection. Other tenants and non-auth capabilities remain available. | PASS |
| Platform configuration, begin, and callback | One boot-owned `PlatformLogin` is stored in `ServerAuth`; platform configuration calls that owner's fresh `RelyingParty::discover`, and begin/callback use its shared cache (`crates/wyrd/wyrd-server/src/boot/mod.rs:1591-1603`; `crates/wyrd/wyrd-server/src/components/auth/state.rs:39-42`; `crates/wyrd/wyrd-server/src/components/platform/identity.rs:235-274`; `crates/wyrd/wyrd-auth/src/platform_login.rs:113-181,238-346`). | Failed or undecodable JWKS discovery returns before the durable upsert. Successful same-process configuration replaces the issuer cache before commit, so the next local begin/callback sees the fresh provider. A DB or audit failure leaves the durable row unchanged; any successful issuer-only cache refresh is non-authoritative and cannot select a connection or client. The global platform credential remains usable during an IdP outage. | PASS; `FIND-TASK-009-13` is closed. |
| Provider HTTP | Every discovery, JWKS, and token call flows through `ScreenedHttp`, which screens and pins resolved addresses, disables proxies and redirects, bounds DNS and the whole request, and caps decoded bodies at 1 MiB (`crates/shared/wyrd-auth-oidc/src/screening.rs:20-32,139-220,247-275`; `crates/shared/wyrd-auth-oidc/src/relying_party.rs:155-195,417-436,503-550`). | DNS failure, connect failure, timeout, redirect, provider `5xx`, oversized body, and cancellation return typed request errors without terminating the shared process. There is no retry amplification. An outage cannot fall through to another tenant or to platform trust. | PASS |
| Provider cache and key rotation | `RelyingParty::cached` uses Moka `try_get_with` to single-flight overlapping misses per issuer. An unknown `kid` invalidates the entry, re-enters that same cached path, and verifies once against the result (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:343-436,479-550`). | Concurrent cold misses share one fetch; failed initialization is not cached. Concurrent rotated-key callbacks share the process-local refresh where they overlap, and each callback performs at most one forced rediscovery. A restart or another replica starts cold, which is the approved process-local cache boundary. Cross-replica request-count and invalidation guarantees are deliberately absent. | PASS; prior cache-concurrency finding remains closed. |
| Workload issuer boot and administration | Workload paths remain separate from human login: they read typed discovery metadata through `ScreenedHttp::provider_metadata`, while `ExternalVerifier` and `JwksCache` fetch workload keys (`crates/shared/wyrd-auth-oidc/src/relying_party.rs:197-247`; `crates/wyrd/wyrd-server/src/boot/issuer.rs`; `crates/wyrd/wyrd-server/src/components/admin/routes.rs`). | A workload JWKS outage does not block metadata-only configuration or a restart with already durable issuer metadata. Assertion verification still fails closed until key retrieval recovers. Human full discovery does not replace this boundary. | PASS |
| Test-process concurrency | The candidate assigns `platform_admin_e2e` to the existing `postgres-fixtures` nextest group, whose six-process ceiling already represents the repository-managed Postgres capacity (`.config/nextest.toml:20-24,58-62`). | The change affects only test scheduling. It prevents simultaneous per-test servers from exhausting Postgres connections and does not serialize production work, alter product availability, or introduce a new harness mechanism. | PASS |

## Failure and recovery assessment

### Crash, restart, and rolling replacement

No provider cache entry is durable or authoritative. On crash or replacement,
Postgres retains the configured connection, login state, principals, and
sessions; a new process rediscovers the issuer on its first relevant request.
An in-flight request can be lost and its one-time state may already be spent,
but no unverified identity or session is committed. The caller starts a new
authorization-code flow, which is the conventional fail-closed recovery.

During replica overlap, each process has its own cache. A callback landing on a
different or restarted replica may require ordinary cold discovery and can fail
during a concurrent IdP outage even when the begin replica had a warm cache.
The task and remediation explicitly preserve a process-local cache and forbid a
cross-replica coordinator; this is not a promised availability guarantee. A
provider endpoint change converges through the five-minute cache TTL, fresh
configuration on the receiving process, or the bounded unknown-key path.

The immediate audience-column removal is retained under
`review/TASK-009-r2/lead-direction-FIND-TASK-009-14.md`: no Wyrd release exists,
and the lead expressly rejected reopening the overlap mechanism as a task
finding. No other cumulative TASK-009 schema or protocol change creates a
system-resilience issue.

### Dependency outage, timeout, and cancellation

The screened adapter has one bounded DNS lookup and one bounded request per
endpoint, with no automatic retry. Discovery can therefore occupy more than one
request timeout because discovery and JWKS are separate standard calls, but the
work remains finite and request-scoped. Provider errors map to fail-closed
`401` or retryable `503` responses as appropriate; they do not panic the server
or fail readiness.

Tenant callback state commits as consumed before external redemption. Platform
state is likewise taken before provider redemption. Cancellation after the IdP
accepts a code can leave the remote outcome unknown, but Wyrd commits no local
session until verification and issuance complete. Retrying the same callback
cannot reuse the consumed state.

### Cache recovery, bad keys, and concurrent discovery

`RelyingParty::discover` performs a fresh standard `openidconnect` discovery
and inserts only a successfully decoded provider plus JWKS. `cached` coalesces
ordinary misses. Invalid signatures, untrusted audiences, or known-key
verification failures do not trigger network retries. Only `UnknownKey`
invalidates and rediscovers, once per redemption; a second miss fails closed.
This is the exact bounded recovery the task requires and avoids both retry
storms and stale-key acceptance.

Fresh configuration calls are intentionally not globally serialized with login
traffic. A concurrent discover can change the issuer's process-local snapshot,
but provider metadata is issuer-scoped and does not carry the durable client,
secret, tenant, or platform principal selection. Durable connection reads still
choose those values. Adding generations, locks, or cross-process invalidation
would be unsupported drift, not a required correction.

### Failure propagation across capabilities

Human tenant and platform relying parties are separate process owners, and the
workload verifier retains its own workload-only key path. A failure in one
issuer's discovery or token exchange does not poison another issuer's cache,
select another connection, block the global platform credential, or take down
the shared server. Store and audit failures refuse the affected mutation or
issuance instead of establishing partial authority.

## Recovery and proof assessment

The remediation evidence records successful exact selectors for:

- `relying_party::tests::id_token_refusals_fail_closed`;
- all `auth::callback` library tests;
- workload boot discovery and workload administration tests;
- `federated_platform_sign_in_runs_through_the_served_callback`, including
  unavailable and undecodable JWKS, same-issuer cache replacement, cached
  callback completion during outage, and unknown-key failure during outage;
- `an_operator_configures_and_removes_federated_platform_sign_in` and
  `a_connection_cannot_name_an_unresolvable_issuer`; and
- the served `tenant_callback_refusal_journey`.

The recorded narrow lanes all pass on the remediation candidate: `fmt`,
`lints`, `codegen:check`, `docs:check`, the three boundary checks,
`check:workspace-hack`, `test:shared`, `test:principals:integration`,
`test:identity:journey`, and `test:wyrd`. The `platform_admin_e2e` full binary's
diagnosed Postgres connection exhaustion is covered by the repository's
existing `postgres-fixtures` ceiling, and the focused platform journeys pass
under it.

This review did not rerun those lanes; it inspected the candidate source and
the committed exact-command evidence. Under the standing verification rule in
commit `518026d54dad2a678794317075cb1bfd08d527b5`, a task review requires the
narrowest lanes covering its write set. Full every-language and user-journey
sweeps belong to final change review, so their absence here is not a gap. No
changed Python, TypeScript, SDK, or generated contract exists in the r2
implementation delta.

Residual proof limits are the approved operational boundaries, not findings:
there is no crash-injection test between remote code redemption and local
commit, no cross-replica cache continuity test, and no all-interleaving cache
proof. The code deliberately promises none of those mechanisms, and requiring
them would violate the standing direction to use standard comparable-project
behavior without bespoke coordination.

## Findings

No material system-resilience findings.

## Overall result

**PASS**
