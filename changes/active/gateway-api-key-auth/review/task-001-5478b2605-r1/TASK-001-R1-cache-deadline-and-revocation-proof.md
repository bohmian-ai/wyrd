---
id: TASK-001-R1
kind: remediation
status: ready
spec: SPEC-gateway-api-key-auth
spec_revision: 1
parent_task: TASK-001
findings: [FIND-TASK-001-1, FIND-TASK-001-2, FIND-TASK-001-3]
---

# Bound gateway reuse to exchange start and prove credential retirement

Implementation skill: $wyrd-implement.

Approved spec: changes/active/gateway-api-key-auth/spec.md, revision 1.
Original task: changes/active/gateway-api-key-auth/tasks/TASK-001-gateway-api-key-auth.md.
Review authority: verdict.md and findings-validation.md in this directory.
Reviewed branch: change/gateway-api-key-auth.
Reviewed base: e0a926031155f263a8721c47d4662dfed8c125ea.
Reviewed candidate: 5478b2605acae3cecd2522941c30ee7f1674f0be.

This packet corrects three independently validated findings within approved behavior. It supersedes the original task's private instruction to start every reuse interval at cache admission: that instruction cannot extend the approved 60-second revocation ceiling. Preserve the original task/spec and review record as evidence. No specification revision or new concurrency contract is authorized or necessary.

## Diagnosis and selected correction

### FIND-TASK-001-1: asynchronous completion renews revoked authority

REQ-001 promises revocation within 60 seconds. In crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:104–114, authenticate awaits TokenGrants::api_key and verifies its JWT before sampling Instant::now for insertion. At :149–163, insert adds a new interval to that instant and replaces any same-key entry.

The existing exchange reads an active credential before awaiting its use update and issuance. The plain active-key SELECT in wyrd-sql/src/queries/auth/service_accounts.rs:352–380 and use UPDATE at :416–433 allow credential retirement to commit between them; issuance still succeeds for an active principal. Independently, retirement can commit after the exchange transaction commits but before its successful result reaches cache admission. A retire at t=0 and admission at t=2 produces a deadline t=62, so a new call t=61 succeeds outside the approved ceiling. Existing row locks and request timeouts do not close the post-commit interval. Current U2 tests insertion at a supplied time and J1 revokes after warming returns; neither proves correct propagation across exchange.

Keep GatewayApiKeys as the existing single owner of reusable authority. Anchor each miss's maximum reuse interval before its asynchronous grant begins and preserve that immutable origin through admission/replacement. The result must never be reusable beyond that origin +60 seconds; preserve the existing token-lifetime cap and verification on every use. Completion after the interval elapses must not create reusable cache authority. An older completion replacing a newer entry may expire early; adding efficiency mechanisms is unnecessary.

This corrects the producer of the extra authority once. Do not add guards in dialect handlers, SQL revocation checks, current-state reads on hits, token revocation lists, invalidation services, network serialization or single-flight. Preserve the successful in-flight grant result even when it cannot be cached, existing grant errors, commit ownership and non-blocking audit. Authentication of new requests after the bound is the invariant; changing issuance or cancelling accepted inference is outside this correction.

### FIND-TASK-001-2: the claimed revoked-key journey changes another authority

AC-002 and original Scenario 3 require actual key retirement. In crates/wyrd/wyrd-testing/tests/gateway/native.rs:774–792, J1 instead posts to /v1/principals/{id}/revoke. auth/revoke.rs and wyrd-auth/src/revoke.rs suspend the principal, leaving auth_api_keys.revoked_at unchanged. Issuance then rejects its inactive principal even if credential retirement or the active-key filter were broken. Later cache assertions at :1025–1059 consequently prove suspension/expiry, not the promised key lifecycle. Current production sequential retirement is not alleged to be broken.

Replace this journey's suspension step with existing public credential administration. List the warmed principal's credentials through GET /v1/principals/{id}/credentials, identify the warmed credential, and retire it through DELETE /v1/principals/{id}/credentials/{credential_id}, expecting 204. These HTTP and MCP operations share components/principals/routes.rs::revoke_credential_for and the existing revoke_api_key SQL owner; no new revocation implementation is needed. Verify with the public principal read that its principal remains active, so a different refusal cause cannot mask the credential seam.

Retain J1's real production TTL, immediate and later cached success, fixed-age expiry refusal and stable error, direct /auth/token rejection, request-correlated audit, tenant/principal separation, secrecy and zero-extra-dispatch assertions. Change directly affected descriptions/assertion text and execution evidence accurately. Use the existing Journey setup and public operations; do not create another minute-long journey or a replacement fixture.

### FIND-TASK-001-3: changed operations omit required lifecycle contracts

AGENTS.md §16 and agent-rules explicitly make missing relevant rustdoc blocking. GatewayApiKeys::authenticate at api_keys.rs:72–90 omits cancellation/partial progress across the shared grant. The issuer can stage an audit decision independently of SQL commit, and use/commit can complete or become uncertain before cancellation prevents returned authority/cache insertion. An uncached retry can exchange and audit again. Do not claim all cancellations commit, SQL/audit atomicity, or inference idempotency.

Journey::deploy and deploy_as at harness.rs:219–243 perform independently awaited credential and deployment writes via put_as/try_put_as. Send/refused-write failures panic; cancellation or failure after the first write can leave a credential without its deployment. Add accurate panic and partial-progress contracts to both changed helpers, and the relevant cancellation/retry description to authentication. Update the cache clock description to match FIND-TASK-001-1. Preserve runtime behavior; no rollback, retry, cancellation owner, transaction or extra helper is required for documentation.

## Constraints and expected reach

The bounded write set is the gateway cache and its inline tests, existing native journey, changed deployment-helper rustdoc and directly affected evidence. Private implementation and test organization remain implementer-owned. Reuse installed dependencies, existing public credential operations, existing managed test server and controlled time. No new public API, SDK behavior, setting, header, grant, feature, dependency, persistent/distributed cache or storage change.

Preserve full-key hashing, secret wrappers, bounded capacity, short locks outside IO, no failure caching, normal token verification, tenant isolation, error envelopes, existing grant/audit semantics, per-request gateway authorization and stock-client compatibility. Management, MCP and Bifrost must not gain fallback; existing OTLP behavior remains untouched. Historical missing RED evidence cannot be recreated by rewriting history. Do not widen this remediation to local style notes or unrelated refactors.

## Acceptance and focused proof

| Finding | Observable closure |
|---|---|
| FIND-TASK-001-1 | A miss started t=0 and completed t=2 has no reusable authority at t=60; completion after t=60 cannot admit a live entry. An older same-key completion cannot gain a fresh interval. A focused test exercises the production miss-to-admission propagation and fails if that production path again captures its origin after exchange. Existing fixed-age, token-lifetime, key isolation and eviction proof remains. |
| FIND-TASK-001-2 | Existing J1 retires the actual warmed credential through public operations, verifies the principal remains active, retains cached acceptance within the bound and rejects the retired key after expiry without dispatch. |
| FIND-TASK-001-3 | All three cited changed methods document their evidenced cancellation/partial-progress/panic contracts accurately; no runtime semantics change is introduced for prose. |

For executable corrections, capture a focused behavioral RED before the minimum change, GREEN afterward, and retain adjacent focused proof. Use controlled instants and the existing owning test boundary to exercise propagation; directly inserting with a test-selected correct timestamp alone does not prove the production caller uses the right origin. No synthetic load, production timing knob, new test framework or dependency. Do not prescribe a private helper/control-flow/fixture shape; choose the smallest local seam that executes the production timing decision.

Extend and run existing U2:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::api_keys::tests::gateway_api_key_cache_is_bounded_isolated_and_expires)'
```

Planned exact propagation test; confirm collection and update the command/evidence if its local name changes:

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::api_keys::tests::delayed_exchange_keeps_its_original_reuse_deadline)'
```

Run corrected J1 with the managed environment and tracing enabled. Inspect traces before diagnosing failures:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gateway_api_key_exchange_is_scoped_cached_and_revocable)"'
```

For this Rust cache/test/rustdoc write set, run mise run fmt, mise run lints, mise run check:deps and git diff --check. Run Cargo-backed lanes sequentially. check:deps protects the shipped/test-support boundary if a private timing seam is used. Rustdoc closure is source inspection against the complete relevant bodies and callees; no new runtime test for prose.

Do not require full gateway/SDK/principal/SQL suites or the repository aggregate for this remediation; full change verification occurs once at change review. No schema, served OpenAPI, Python/TypeScript API or docs-site change is needed for these findings. If the actual write set expands, justify and run its narrow additional checks. Record exact collected selectors, RED cause, GREEN and final lane outcomes honestly; later task review assesses the complete cumulative candidate and all three stable findings.

## Implementation Evidence

Status: `IMPLEMENTED` on `change/gateway-api-key-auth`. Fix commits are
`7f64a7138` (cache deadline) and `8325a0e3a` (journey and harness rustdoc).

### Reuse revalidation

`GatewayApiKeys` is still the only owner of reused authority. `authenticate`
now builds the grant future and the verifier closure and hands them to a
private `reuse_or_exchange`, which owns the timing decision:

- it reads the clock once before the exchange is polled, and that reading is
  the origin;
- `insert` sets the deadline to origin + min(60s, token lifetime);
- a completion whose window has already closed is still returned to its own
  request but is never inserted.

Production passes `Instant::now`. The test passes a controlled clock and an
exchange future that moves that clock forward.

No new dependency, setting, single-flight, invalidation, or SQL/dialect guard
was added. Grant, audit, verification, and error semantics are unchanged. J1
retires the key through the existing `GET` and `DELETE
/v1/principals/{id}/credentials` routes and reads the principal through `GET
/v1/principals`. No fixture or extra journey was added.

### TDD record

| Finding | RED | GREEN |
|---|---|---|
| FIND-TASK-001-1 (deadline) | The seam was first written with the clock read after the exchange, so it failed at `api_keys.rs:250` (`left: Some("jwt-slow")`, `right: None`): still reusable at t=60 after an exchange that began at t=0 and finished at t=2 | Moving the clock read before the exchange makes it pass |
| FIND-TASK-001-1 (late admission) | Removing the window check before insert (temporary edit, then restored) fails `api_keys.rs:278` (`left: 1`, `right: 0`): the late completion took a cache slot | Passes with the check in place |
| FIND-TASK-001-2 | No RED is possible: sequential credential retirement already works in production. The finding was a journey that proved the wrong lifecycle step | J1 passes (62.7s) with credential retirement and the principal still `active` |
| FIND-TASK-001-3 | Rustdoc only | Checked against the bodies of `TokenGrants::api_key` and `put_as`/`try_put_as` |

### Acceptance matrix

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-TASK-001-1: an exchange that begins at t=0 and finishes at t=2 has no reusable token at t=60; a completion after t=60 admits no live entry; an older completion gains no fresh interval | `api_keys.rs` `reuse_or_exchange` (origin taken before `exchange.await`, window checked before `insert`) and `insert(.., origin)` | `delayed_exchange_keeps_its_original_reuse_deadline` drives the production miss path with a controlled clock. RED and the mutation are recorded above. U2 passes: fixed age, token lifetime, key isolation, eviction | PASS |
| FIND-TASK-001-2: J1 retires the warmed credential through public operations, the principal stays active, the cached key is accepted within the bound and the retired key is rejected after expiry without dispatch | `native.rs` J1: credential list (exactly one, prefix matches the warmed key), `DELETE` → 204, principal `status == "active"`. All earlier TTL, audit, refusal, tenant, secrecy, and dispatch assertions are kept | J1 and J2 pass under `WYRD_LOG=info,wyrd_server=debug` | PASS |
| FIND-TASK-001-3: cancellation, partial-progress, and panic contracts | `GatewayApiKeys::authenticate`: "# Cancellation and retries" (audit may be staged or use committed with nothing returned or cached; no rollback; a retry exchanges and audits again; no dedup). `Journey::deploy`/`deploy_as`: "# Panics" plus the credential-without-deployment partial write. Cache clock docs now describe the start-of-exchange origin | Source inspection; `lints` (clippy, `-D warnings`) passes | PASS |

### Commands (final outcomes)

- Unit tests:

  ```bash
  mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::api_keys::tests::delayed_exchange_keeps_its_original_reuse_deadline) | test(=components::gateway::api_keys::tests::gateway_api_key_cache_is_bounded_isolated_and_expires) | test(=components::gateway::ingress::tests::gateway_credentials_select_key_exchange_or_token_verification)'
  ```

  Result: 3 passed.
- Journeys:

  ```bash
  mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_LOG=info,wyrd_server=debug WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gateway_api_key_exchange_is_scoped_cached_and_revocable) | test(=native::gateway_api_keys_remain_rejected_outside_ingress)"'
  ```

  Result: 2 passed (J1 62.7s).
- `mise run fmt`, `mise run lints`, `mise run check:deps`, and
  `git diff --check`: all pass. `fmt` reflowed lines after the journey run;
  the change was whitespace only.

### Scope and limits

- The write set is `api_keys.rs`, `native.rs` (J1 only), `harness.rs`
  (rustdoc only), and this evidence.
- No public API, SDK, setting, header, grant, dependency, schema, docs-site,
  or OTLP change.
- The retirement race itself is covered by the controlled-clock propagation
  test. It has not been reproduced end to end against SQL.
- U1 and U2 still have no RED from before implementation. That history cannot
  be recreated.
