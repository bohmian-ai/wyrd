# Security and tenancy domain review — R3

Result: **PASS**. Material proposed findings: **none**.

## Immutable subject and scope

Repository `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/agents-trace-processor`.
Original base `dac4c340f835676adc29f3dcc01f98e1d3d51465`; candidate
`1cf675c58d01cba0e714102c75cb7d89e7a678b5`. HEAD matched before inspection
and at report completion. This is a cumulative original-task security audit,
including all remediations and the authorized main merge
`1b769759dfd92a82e60082fe80460166bdd860f8`. Fix/imported-main contrasts were
navigation aids, not replacement bases. Unrelated imported main functionality
is not independently certified.

Reviewed boundary: selected shared-client credentials, exporter freshness,
bridge content/errors, gateway/provider credential separation, authenticated
subject/current actor, Card/tenant authorization, implicit OTLP identity and
Scribe stamps, real public delegation, and authenticated query/readback. No
current peer discovery report or implementation chat summary was read.
Historical validated findings and remediation packets were hypotheses checked
against source.

## Authority and navigation

Read task-review skill, AGENTS.md, agent rules, reference router,
spec-driven development, maintainer style, Wyrd design, doctrine, Bifrost design,
security posture, telemetry-observations, architecture-constraints, OLAP-serving
and testing-workflows references, approved spec r2 and original TASK-001.
Stronger architecture controls subordinate reference details. The applicable
rules are verified tenant/publisher identity; attribution grants no authority;
delegation narrows permission; present Cards are authorized; selected clients
retain shared refresh; bridge/errors exclude sensitive payloads; native
instrumentation keeps its disclosed content ownership; server-owned table/query
isolation; and no new auth/transport/queue/persistent protocol.

CodeGraph was used before code navigation. Incomplete broad results were
expanded with source/caller searches. Only the assigned report was written.

## Boundary coverage

| Obligation / surface | Source evidence | Result |
|---|---|---|
| REQ-011/017, INV-001: exporter uses the state's selected client | Python `src/state/mod.rs:181-223` obtains `self.inner.client()`; `python/wyrd/otel.py:191-213,311-355` closes over it and requests `access_token()` per request. TS `src/index.ts:2924-2940` projects the native state client; `src/otel.ts:25-47` supplies async per-export headers for all signals. Run/Card values choose no credentials; there is no token cache. | PASS |
| REQ-017/AC-013: ambient and provider credential separation | Python framework `app.py:160-256,350-389`, parent `test_framework_telemetry.py:42-79,260-299`; TS gateway journey `agents-trace-processor.test.ts:371-395`; `WyrdClient.httpAuth`, `src/index.ts:1543-1551`. Direct models use independent keys. Wyrd auth hooks target the Wyrd gateway. Upstream assertions require its configured provider key and no `x-wyrd` headers. | PASS |
| REQ-010/014, INV-003: payload allowlist | Python `otel.py:358-541`, TS `index.ts:2552-2708`: explicit names/model/usage mapping, start-time correlation, fixed `_OTHER`/interruption statuses; no prompt/completion/tool arguments/results/raw error messages. Python Responses handles withheld response content through safe direct usage. | PASS |
| REQ-013, INV-003: scrubbed failure projection | Python native `state/mod.rs:508-542` keeps exception type only; TS `index.ts:2638-2649` inspects timeout text only for classification and projects type/name. Shared `wyrd-client/src/state.rs:2088-2164` keeps native stable code as secondary detail. Raw provider messages/payloads are not copied. | PASS |
| INV-001: verified subject/current actor | `wyrd-auth-verify/src/lib.rs:339-365,767-790,908-933` verifies token/tenant/audience and flattens actor claims earliest first. Runtime `request_context.rs:60-65` selects current actor. Gate `auth.rs:55-85,164-174`; HTTP `http/otlp.rs:531-538`; gRPC `grpc/otlp.rs:265-287` preserve verified chain. | PASS |
| INV-001/AC-011: write permission, Card scope and tenant | Gate `mod.rs:549-621,823-930` authorizes traces/logs/metrics before dispatch. `gate/attribution.rs:86-137` resolves unbound Cards with authenticated-tenant `TenantConn`; bounded callers retain signed scope. Scribe `ingress.rs:61-80` refuses tenant/principal disagreement; `execution_lanes.rs:1084-1120,1228-1270` checks scope and stamps managed columns. | PASS |
| FIND-9: publisher separation and same-pair replay | Gate `mod.rs:133-154,978-999` hashes tenant, both `auth.attribution()` members, table, logical digest and accepted correlation digest, then passes that same pair to Scribe. Request/time/token/credential refresh/older actor history do not enter the ID. Shared canonical dispatch covers all signals and both transports. | PASS |
| Native/outbox sibling preservation | Native `gate/mod.rs:1031-1084` retains caller UUIDv7 IDs. `scribe/preprocess.rs:424-435,464-508` keeps separate logical/correlation roles and excludes managed stamps from logical content. Outbox `scribe_outbox.rs:796-818` already hashes both pair members under its own domain. No duplicate replay or authorization owner appears. | PASS |
| FIND-12: current schema readback | `otlp/negative.rs:417-549` queries/extracts both publisher columns, groups pair plus Run and requires complete markers once per group. All seven OTLP files were swept: main/support/negative, trace gRPC/HTTP, logs and metrics. No obsolete `principal_id` row read remains. `support.rs:1025-1063` forwards public Bifrost queries and requires a successful terminal. | PASS |
| Query tenant authority | Shared Bifrost query/facade, server query service and `oracle/mod.rs:214-234` preserve checked principal/tenant equality. Framework reads use trace timeline or bound Run parameters; new Rust SQL uses fixed fixture names. No query authorization replacement. | PASS |

## Public delegation and trusted publisher trace

`OtlpJourney` (`support.rs:827-865`) bootstraps an admin Service.
`token_with_permissions` (`:919-941`) seeds record-write permission, bootstraps
a distinct Service and publicly exchanges its key. Harness `server.rs:3025`
uses its existing machine bootstrap, whose fixture Service has a Card root.
Actors are therefore eligible Card-bound Services rather than invented test
principals. `server.rs:3498-3540::delegate/token` posts both access tokens to the
real `/auth/token` route.

`wyrd-auth/src/exchange_api_key.rs:272-315,362-389` verifies both in one tenant,
rejects delegated/unbound/self actors, and appends the actor. Issuance
`issuance.rs:214-241` preserves subject Card scope and intersects actor current
permissions with subject permissions. The admin subject and each write actor
retain the required write capability. The test requests Wyrd audience, accepted
on Bifrost by `TokenVerifier::validation`; both transports reach the intended
allowed ingest path.

Runtime attribution produces `(S,S)`, `(S,A)`, `(S,B)`. Each now reaches a
distinct implicit fence while identical same-pair rows converge across HTTP
and gRPC. Scribe stamps exactly those pairs. The journey keeps two direct
subjects, two different actors for the first subject, the existing Run-separated
group and every marker once in each of five groups. FIND-9's producer and
FIND-12's removed-column consumer are closed without changing their separate
causes or adding downstream guards.

## Negative proof and prior closure

FIND-11 now catches only public `WyrdError` at the child entrypoint
(`app.py:383-389`), emits its actual structured code and reraises. Parent
`test_framework_telemetry.py:281-299` requires nonzero exit and exactly
`WYRD_CLIENT_401_NO_CREDENTIALS`. Unexpected exceptions remain failures;
positive gateway/export/readback and upstream credential assertions remain.
FIND-10's settlement test now documents its Panics contract without changing
production logic.

Earlier security-relevant closure remains intact: safe withheld Responses
usage; bounded managed flushing retaining its original provider/client; fresh
post-entry flush barrier; same-state setup serialization; TS post-await setup
guard and optional package boundary. Concurrency/package specialists own their
full mechanical audit; this pass checked no credential reselection, authority
widening or payload leakage was introduced.

Python/TS framework negative journeys still exercise denied export,
foreign-tenant Card attribution, sampling and content sentinels
(`test_framework_telemetry.py:234-258`; TS integration `:327-369`). Foreign
uncorrelated rows may persist only in their own tenant; foreign Card assertions
do not select the original tenant. Token-freshness tests require successive
bearers and no setup-time token retrieval (`test_observe_surface.py:1034-1051`;
`telemetry.test.ts:186-212`). Log/metric exporters share the same auth
closure/config as spans, so no per-signal auth correction is needed.

Current root-owned `verification.md` records fresh PASS on the pinned candidate:
derived-ID exact unit selected/passed one; exact delegated attribution journey
selected/passed one; Python managed three-framework cases plus exact
credential-code gateway case passed four without skips; TS managed OpenAI live
flush case passed one; telemetry units/package/runtime checks also passed.
This is dynamic authorized HTTP/gRPC replay/readback and public credential-code
proof. Full privacy/auth matrices remain source-checked with committed earlier
execution evidence; they were not freshly rerun by this reviewer.

## Finding ledger and limits

**Empty material finding ledger. PASS for the assigned security/tenancy
boundary.** No new permission, tenant, public API, persistent-data or
compatibility decision is needed for closure. Cross-version rolling replay was
not dynamically tested; no new migration/replay subsystem is inferred from the
remediation.

Skipped source/task/spec/index/HEAD edits, staging, builds, tests, installs,
indexing, live providers/OIDC/cloud credentials and advisory scans. Root-owned
fresh verification is distinguished above from historical evidence. Native
instrumentation can export content it emits, as the approved contract and
recipes disclose; this is not a universal redaction guarantee.
