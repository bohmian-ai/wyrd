---
id: TASK-006-R2
kind: remediation
status: ready
spec: SPEC-benchmark-latency-diagnosis
spec_revision: 7
parent_task: TASK-006
remediates: [FIND-006-5, FIND-006-8, FIND-005-3]
requirements: [REQ-021, REQ-030, AC-024, AC-026, INV-001]
depends_on: []
---

# Preserve policy recovery and authentication errors

When a reload cannot be queued, the server must stop serving that obsolete
policy and recover through its existing background path. When a malformed token
fails authentication, the optimized tenant pre-check must preserve the existing
public refusal. Make those two bounded corrections and batch the remaining
mandatory documentation with them.

## Authority and subject

- Approved spec: `changes/active/benchmark-latency-diagnosis/spec.md`, revision 7.
- Original task: `changes/active/benchmark-latency-diagnosis/tasks/TASK-006-gateway-local-governance-and-agent-check-latency.md`.
- Original TASK-005 for shared FIND-005-3: `changes/active/benchmark-latency-diagnosis/tasks/TASK-005-full-path-benchmark-parity-and-optimization.md`.
- Prior candidate: `180879328ac9dde94d7504855e821980e5db0f18`.
- Reviewed candidate: `2ce7caec7d5a7bd51fb0ba1512f552b0f425ec0f`.
- Prior task: `../TASK-005-006-review-001-180879328/TASK-006-R1-governance-lifecycle-and-agent-correctness.md`.
- Validated findings: [findings-validation.md](findings-validation.md).

Root/nested AGENTS, the security error/tenant boundary, and the Gateway snapshot
owner govern these corrections. No specification change or new public decision
is required.

## FIND-006-5: a full queue can still lose a reload

REQ-030 and the prior remediation require a fenced or failed policy reload to
remain recoverable. `snapshots.rs::reloads` drains A before its asynchronous load.
While it waits, 4096 other tenant notifications can fill the bounded set and
advance generation. `install` correctly fences A, then `requeue` calls
`Entries::queue`, which silently drops A when full. The old A snapshot remains
available through `admitted` and `cached`; repeated A reads need not queue it
again. Healthy listener and usage refresh do not repair policy. Active A reads
can keep it in the LRU, so eventual eviction is not a recovery guarantee.

Exact correction boundary: `crates/wyrd/wyrd-server/src/components/gateway/snapshots.rs`,
particularly `Entries::queue` at lines 78–82, fenced install/requeue at 185–215,
and sibling `invalidate` at 261–277. This is the original lost-obligation defect
at a remaining capacity edge, not a request for unlimited queues. A supported
bulk configuration update during slow IO can otherwise leave quota, routing,
credential or capture policy obsolete indefinitely.

Reuse the existing cache/queue owner: make retention of an owed reload and
continued availability of its cached policy one atomic decision. If the bounded
queue cannot retain that tenant, evict its cached snapshot. Existing cache-miss
behavior then returns the retryable 503 and retries background loading after
capacity becomes available. Apply the rule to every writer of this same queue
obligation, including ordinary invalidation, not just fenced install. Preserve
listener fencing, bounded work and unrelated healthy tenants.

This conservative eviction may cause a tenant to reload even if another path
recently installed a usable value; that bounded availability cost is preferable
to retaining an untracked obsolete policy. Do not add per-entry versions, a
second queue, TTL, scheduler, synchronous SQL fallback or consumer guards merely
to avoid a conservative eviction. Changing this single authoritative transition
is smaller and more complete than patching individual readers.

## FIND-006-8: tenant-only parsing changes public authentication failures

INV-001 preserves stable errors. The optimized
`token_extract.rs::tenant_from_unverified_access_token` reads only the tenant.
A signed token with a valid tenant but malformed `permissions` previously failed
the full-claim shape pre-read with `401 WYRD_AUTH_401_UNAUTHENTICATED`.
It now reaches `verify_on`, whose JSON error maps through the unchanged HTTP
error owner to `400 WYRD_AUTH_400_BAD_TOKEN_FORMAT`. If the verifier is absent,
the same malformed claims now produce `503 WYRD_AUTH_503_VERIFY_UNAVAILABLE`.
Callers receive a different status, stable code and remediation from an internal
performance change. Both paths refuse access; no authentication bypass is alleged.

Relevant source: server `components/auth/token_extract.rs:95–106,159–172`,
shared `AccessTokenClaims::unverified_tenant` at auth-verify `lib.rs:803–826`,
and server `http/error.rs:237–291`. Sibling token-exchange and Gate callers share
the tenant helper but have their own established refusal projection.

Preserve the fast tenant-only successful-request path. At the existing HTTP
authentication extraction/error owner, restore the previous full-claim shape
classification before returning failed-verification or unavailable-verifier
outcomes. Reuse `AccessTokenClaims` decoding and the existing unauthenticated
error; do not add a second successful-request parse, verification, cache or new
claim profile. Keep global JWT error mappings and the shared verifier unchanged.
For token exchange, restore its existing low-volume full-claim subject precheck
at that route owner: malformed subjects must retain the prior 400 classification,
including when tenant storage is unavailable. Tenant-storage failure must not
mask that known malformed-subject refusal; preserve the route's existing
issuer/verifier-configuration precedence, including its prior unavailable error.
Gate already performed tenant-only
selection; retain its existing verification/error projection rather than adding
another precheck there. Do not duplicate correction in Gateway or gRPC consumers
of the shared request-auth owner. Mere movement of pre-SQL work is not a separate defect.
The original parser restored on every success would be mechanically smaller but
would discard the demonstrated optimization. A local error-path correction
preserves both required behavior and the successful path's reduced work.

## FIND-005-3: bounded new documentation subset

The original exact list is closed. New optimization items still violate the
mandatory changed-item rustdoc rule. This task owns the following subset;
TASK-005-R2 owns the disjoint RequestJson rows. Add accurate concise contracts only:

| Candidate location | Required correction |
|---|---|
| Skald spec `wire/common.rs:109,116,195,206` | Errors on SharedSchema serialization/deserialization and SharedObjectSchema fallible conversion/deserialization. |
| Skald runtime `provider.rs:27,30`; providers `trait_.rs:28,31` | Errors on defining send/stream traits; forwarding implementations inherit the contract. |
| Providers clients `openai.rs:676`, `anthropic.rs:122`, `google.rs:126`, `vertex.rs:120` | Errors on changed public send_native entry points matching actual variant/auth/transport/provider/decode failures. |
| Agent integration `agent_timeout.rs::agent_run_emits_genai_spans_without_payloads` | Add the changed test's panic contract. |
| Vala `blocks/agent.rs:726` unavailable-Gateway classification test | Add the assertion panic contract. |
| SQL integration `pg_gateway_usage.rs:23`, this_minute | Document the timestamp expect invariant under Panics. |

Use the validated ledger if a functional fix deletes one of these items; do not
add replacement boilerplate, doc checkers or a documentation-only review round.

## Ordered behavioral proof

1. Extend the existing snapshot fixture to reproduce a cached A, drained/in-flight
   A load, full queue of other tenants and fenced A completion. RED must show
   A still serving with no owed reload. After the correction, A must remain owed
   or be unavailable, and recover to current policy through ordinary read/background
   loading without another A edit. Exercise full-queue direct invalidation too;
   assert bounded queue size and continued availability of an unrelated healthy
   tenant. Use the in-process fixture, not thousands of PG tenants or host load.
2. In the existing server authentication fixture, submit the signed token with
   valid tenant and malformed permissions. Assert the previous 401/code and no
   authenticated handler, both with a configured and unavailable verifier. Record
   RED then GREEN. Retain valid credentials, valid-shape bad signature, malformed
   envelope, wrong tenant and wrong surface classifications. Prove exchange
   malformed-subject 400 with normal and unavailable storage, and preserve the
   Gate mapping without broadening its established contract.
3. Inspect the exact documentation batch and new changed items against root rules.
   Documentation requires static proof, not artificial runtime tests.

Existing snapshot regression command:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::snapshots::tests::reloads_that_do_not_install_stay_owed)'
```

Run the narrow snapshot and extraction families after adding the new cases:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/components::gateway::snapshots::tests::/) | test(/components::auth::token_extract::tests::/)'
```

New test names/selectors are planned. Confirm actual target and names after
adding them and record exact focused commands for each before RED. If the
selected HTTP fixture needs Postgres, use the existing repository setup wrapper;
do not emulate a server-owned wire mapping solely inside shared verifier tests.
Run `mise run fmt`, narrow clippy for the executable server write set and affected
documentation targets, and only directly affected regression families. The
passing SQL overlap/drain, Prompt export, schema/request equivalence and canonical
performance evidence carry forward unless changed dependencies invalidate them.
No full journeys, aggregate gate, new benchmark or profile run is required here.

Acceptance maps directly to the three findings: no obsolete policy survives a
lost queue obligation; previous authentication failures retain their public
classification while successful parsing stays lean; the bounded mandatory docs
are complete. Preserve all accepted quota, capture, tenant, validation and
deadline behavior. Route directly to `$wyrd-implement`.

## Implementation evidence

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-006-5: a reload the full queue cannot hold stops serving; every writer covered | `snapshots.rs` `Entries::queue` forgets the tenant's snapshot when the full queue cannot hold it (shared by `admitted`, `requeue`, `invalidate`, `reset`); module docs updated | `components::gateway::snapshots::tests::reloads_the_full_queue_cannot_hold_stop_serving`: RED `a fenced reload is not lost`, then GREEN; fenced install, direct invalidation, bounded queue, healthy tenant, recovery via miss | PASS |
| FIND-006-8: malformed claims keep 401 with configured and absent verifier | `token_extract.rs` `verify_access_token` decodes full claims only on a refusal (`AccessTokenClaims::unverified`); success path unchanged tenant-only | `components::auth::token_extract::tests::malformed_claims_are_refused_as_unauthenticated`: RED 400 vs 401, then GREEN; valid verifies, bad signature `401_INVALID_TOKEN`, malformed envelope 400, valid without verifier 503 | PASS |
| FIND-006-8: exchange malformed subject keeps 400 before storage | `routes.rs` `tenant_from_unverified_access_token` uses full-claim `AccessTokenClaims::unverified`; call order issuer → verifier → subject check → `tenant_conn` unchanged, so storage cannot mask it | `components::auth::routes::tests::rejects_malformed_claims_beside_a_valid_tenant`: RED with tenant-only parse, GREEN | PASS |
| Gate mapping unchanged | `vala-bifrost-redux` untouched; still `unverified_tenant` | `cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/gate::/)'` 38 passed | PASS |
| Shared decoder | `wyrd-auth-verify` `AccessTokenClaims::unverified` beside `unverified_tenant`, sharing one private payload decoder; `TokenVerifier` and JWT error mappings unchanged | wyrd-auth-verify lib tests | PASS |
| FIND-005-3 subset | `wire/common.rs` serialize/deserialize/try_from Errors; `ProviderClient`/`Provider` send/stream Errors; four `send_native` Errors; Panics on `agent_run_emits_genai_spans_without_payloads`, `unavailable_gateway_is_busy_and_other_problems_are_classified`, `this_minute` | source inspection; `cargo doc` reports no warning in touched files; `mise run lints` | PASS |

Commands:

```sh
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(/components::gateway::snapshots::tests::/) | test(/components::auth::token_extract::tests::/) | test(/components::auth::routes::tests::/)'  # 12 passed
mise exec -- cargo nextest run --locked -p wyrd-auth-verify -p wyrd-gateway -p wyrd-skald-spec -p wyrd-skald-providers -p vala-verify --lib  # 328 passed
mise run fmt && mise run lints  # exit 0
git diff --check
```

Limits: the exchange storage-unavailable case is proven by call order plus the
helper test, not a Postgres route test. Not run: PG integration tests whose
files changed only in docs, journeys, and gate. Non-goals respected: no
per-entry versions, second queue, TTL, or second success-path parse.
