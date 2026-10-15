# Independent findings validation

Result: **three retained findings; bounded correction is possible under approved specification r1.** No unresolved authority or source blocker and no new material decision were identified. This is the validated ledger for the orchestrator, not its task verdict.

## Subject and evidence boundary

- Base: `e0a926031155f263a8721c47d4662dfed8c125ea`.
- Candidate: `5478b2605acae3cecd2522941c30ee7f1674f0be`.
- Approved specification: `changes/active/gateway-api-key-auth/spec.md`, revision 1.
- Original task: `changes/active/gateway-api-key-auth/tasks/TASK-001-gateway-api-key-auth.md`.
- HEAD remained the candidate; the reviewed source was not edited. The only output of this validator is this report.

Read the cumulative diff across all 15 changed paths and the original task, including its verification record. Read every required discovery report: behavior, invariants, standards, maintainer, system, security, tenancy and concurrency, plus the focused follow-up. Used CodeGraph before expanding source navigation. Checked the reported targets against their complete current bodies and traced the shared producers, consumers and credential writers below. A few discovery references have stale line ranges; this ledger identifies the actual current symbols and locations.

Applied AGENTS.md, architecture/agent-rules.md, the reference router, spec-driven-development and maintainer-style references, Rust/testing guidance, applicable Wyrd design/runtime-identity/delegation and doctrine authority, and security-posture/security-foundation rules. The supplied task record reports focused units, real-server journeys, stock-client tests and scoped checks passing. Those outcomes are supplied evidence, not independently rerun results. This validation ran `git diff --check e0a926031..5478b2605` successfully; it ran no Cargo, Python, TypeScript or broad verification lane.

## Claim-by-claim adjudication

| Discovery proposal or disputed claim | Adjudication | Source-based disposition |
|---|---|---|
| CONC-001 | REVISED | Retain as FIND-TASK-001-1, shared with SEC-001 and SYS-001. Narrow the concurrency explanation to the concrete orderings below; a revocation need not bypass an already-held use-row update lock. |
| SEC-001 | REVISED | Same deadline-origin defect, not a separate security correction. Preserve existing grant and JWT semantics; fix gateway reuse only. |
| SYS-001 | REVISED | Same defect and correction. No process shutdown, distributed invalidation, extra SQL check or availability change is justified. |
| INVREV-001 | CONFIRMED | Retain as FIND-TASK-001-2. J1 changes principal status rather than credential revocation state. Production sequential retirement currently works; the missing required journey proof is the defect. |
| REPO-001 | REVISED | Retain required rustdoc omissions as FIND-TASK-001-3, merged with MAINT-001. Include both materially changed deployment helpers. Do not turn the suggested analogous ingress prose into an additional mandatory correction. |
| MAINT-001 | CONFIRMED, deduplicated | Authentication cancellation/partial progress and deploy_as panics are supported and covered by FIND-TASK-001-3. There is no separate owner/refactor finding. |
| Follow-up supports two distinct revocation gaps | CONFIRMED | Credential-retirement proof and late-completion deadline origin have different correction targets. The follow-up introduces no fourth finding. |
| Behavior PASS establishes the full revocation obligation | REJECTED | Its ordinary fixed-expiry and post-warm suspension evidence proves neither credential retirement with an active principal nor overlapping exchange completion. Other supported acceptance rows need not be discarded. |
| Standards local imports/qualified signature and historical missing unit RED require correction | REJECTED as blocking findings | The report itself gives these no material consequence and no source-local material ID. They cannot create another remediation scope or replay historical TDD. |
| Token-expiry cap requires a new current-state JWT/revocation mechanism | REJECTED | Neither discovery nor source establishes that need. Existing verification runs on every use. Preserve the current lifetime cap; no new token parser, store read or revocation list is warranted. |

There are no prior FIND-* IDs in this first review attempt. The three IDs below are the first stable IDs; duplicates receive no additional IDs. No human-directed addition was relabeled as a reviewer finding.

## Shared owner and sibling-path trace

The only production caller of `GatewayApiKeys::authenticate` is `GatewayIngress::authenticate` at `crates/wyrd/wyrd-server/src/components/gateway/ingress.rs:138-147`. Its protocol layers cover existing public gateway routes. It receives the credential only after `extract_gateway_access_token` applies query/header restrictions, and inserts only a verified principal for the ordinary Caller/invocation pipeline. AppState owns and shares the cache at `state.rs:2198-2200,2273`. No other public authentication surface reads it. Management, MCP and Bifrost use their existing verifier layers in `http/router.rs:59-119`; OTLP's principal-only API-key path remains separate.

Both `/auth/token` and gateway misses call `TokenGrants::api_key` at `components/auth/routes.rs:237-258`. `api_key_exchange` at `:297-309` parses the key and opens its TenantConn. `ExchangeApiKey::execute` at `wyrd-auth/src/exchange_api_key.rs:159-177` calls `verify_api_key`, awaits `touch_api_key_last_used`, and then awaits `TenantTokenIssuer::issue`. The shared verifier also serves `ExchangeApiKey::authenticate` (the OTLP sibling) and `connections.rs::recovery_key_authorizes`; those siblings do not own this gateway cache. `api_key_by_prefix` and `touch_api_key_last_used` have no alternative production caller bypassing that shared verification/use sequence.

Credential retirement has two SQL callers: HTTP/MCP administration through `components/principals/routes.rs::revoke_credential_for`, and platform provisioning's `establish_tenant_administration` at `components/platform/provisioning.rs:443-521`, which retires old credentials when resuming provisioning. Both use `wyrd-sql/src/queries/auth/api_keys.rs::revoke_api_key`. The HTTP handler and MCP tool share `revoke_credential_for`; neither invalidates this cache. Listing shares `list_credentials_for` and `list_api_key_metadata`. Principal suspension is a separate writer: `auth/revoke.rs::revoke_principal` calls `wyrd-auth/src/revoke.rs::revoke_principal_in_conn`, whose machine branch changes service-account status through `suspend_service_account_principal`. Those are distinct durable authorities.

`TenantTokenIssuer::resolve` checks the active tenant/principal and resolves grants; it does not reread API-key revocation. `issue` stages its decision and returns the signed token; `TokenGrants::api_key` commits before converting it to the response. `ExchangedToken::into_response` at `issuance.rs:95-106` computes whole seconds remaining. The shared verifier at `wyrd-auth-verify/src/lib.rs:339-365,888-899` verifies self-contained claims with no store read. The gateway's ordinary invocation still calls its permission check/audit and tenant admission at `components/gateway/invocation.rs:424-483,652-689`. Fixing the cache must preserve all of these sibling and adjacent behaviors.

The existing `verification/cache.rs::VerifierCache` owns immutable tenant/UID-keyed Cards with a byte budget, not expiring secrets. Client `AuthMiddleware::bearer` owns one caller's credential/transport, renewal, optional disk persistence and single-flight. Neither can replace this explicitly required server multi-key cache without adding coupling or changing lifecycle. Reusing the installed LRU/SHA-256 mechanisms and existing complete grant is sufficient; no generic cache or new grant abstraction is justified.

## Deduplicated finding ledger

### FIND-TASK-001-1 — Late exchange completion starts a fresh reuse window

- Discovery IDs: CONC-001, SEC-001, SYS-001; corroborated by followup-review.
- Status: **REVISED**; classification: **INCORRECT**.
- Violated obligation: spec REQ-001, revoking a key takes effect within 60 seconds; task's explicit prohibition on extending that window.
- Exact changed location: `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:104-114` (`authenticate`), with deadline construction/replacement at `:149-163` (`insert`). Full `authenticate`, `get`, `insert`, their unit test and ingress caller were inspected.

The authority was last observed before cache admission, but `authenticate` samples its insertion time only after the awaited grant and local verification. `insert` then unconditionally uses `now + min(TOKEN_REUSE, lifetime)` and replaces any same-key entry. `get` compares only this deadline; successful hits verify the existing JWT locally.

The active-key lookup at `wyrd-sql/src/queries/auth/service_accounts.rs:352-380` is an ordinary SELECT with `revoked_at IS NULL`. The later use update at `:416-433` matches tenant/id without rechecking live state. A reachable ordering is: exchange reads a live credential; retirement updates and commits that credential before the exchange's use update; the use update then succeeds on the retired row, issuance finds the still-active principal, and the exchange commits successfully. Alternatively, exchange commits first; retirement commits while the successful exchange's completion is delayed before gateway insertion. The latter ordering alone is sufficient and does not depend on SQL lock assumptions. If an exchange already holds the use-row lock, retirement can wait for that transaction; this does not eliminate the post-commit interval. No claim that every retirement can commit through a held row lock is retained.

Let retirement commit at t=0 and that successful exchange reach insertion at t=2. The cache now permits hits until t=62; a fresh call at t=61 authenticates after the approved revocation ceiling. An older concurrent successful miss can create the same extension when it replaces the entry. A request timeout bounds a delay but does not remove the additional positive delay from the later 60-second reuse window. Existing U2 supplies an insertion time directly, and J1 revokes only after warming has returned; neither catches this origin error.

**Minimum correction.** Keep `GatewayApiKeys` as the single reuse owner. Capture each miss's reuse origin before beginning its asynchronous grant and carry that immutable origin through admission/replacement, so no result can be reused after its own pre-exchange origin plus 60 seconds. Retain the existing token-lifetime cap and verification on every use. A completion whose reuse interval has elapsed must confer no reusable cache authority. An older completion may cause early expiry/re-exchange; preserving the newest entry for efficiency is not an additional requirement. The source fix belongs at gateway cache admission because that owner alone creates the extra reusable authority; do not alter credential verification SQL or add checks to individual dialect handlers.

Preserve the existing in-flight grant result, errors, transaction ownership, local JWT verification, audit staging, per-call authorization, cache capacity and short locks. Do not reject an otherwise successful in-flight grant solely to satisfy cache reuse, introduce a current-state check on hits, poll revocation, serialize network IO, add single-flight, or change unrelated token/sibling service behavior. Update directly affected cache rustdoc so its clock description matches the correction.

**Focused closure proof.** Extend the current controlled-time cache proof to cover delayed admission, completion after the bound, and an older same-key completion replacing a newer one. Additionally exercise the production miss-to-admission timing propagation with a deterministic focused test: represent exchange start t=0 and completion t=2 and prove the stored authority misses at t=60, rather than granting a new window through t=62. This test must fail if the production path once again samples its reuse origin after exchange; a direct call to `insert` with an already-correct test timestamp alone is insufficient. Use the existing owning module/test boundary and controlled instants, with no host load, public knob, new fixture framework or dependency. The exact new test name below is planned and must be confirmed as collected before recording execution. Existing J1 supplies the real-server credential lifecycle after FIND-TASK-001-2; another minute-long journey is unnecessary.

### FIND-TASK-001-2 — Claimed key revocation journey suspends its principal

- Discovery ID: INVREV-001; corroborated by followup-review.
- Status: **CONFIRMED**; classification: **MISSING**.
- Violated obligation: spec AC-002; original task Scenario 3's existing credential lifecycle; AGENTS.md §11 negative-journey coverage.
- Exact changed location: `crates/wyrd/wyrd-testing/tests/gateway/native.rs:774-792`, within `gateway_api_key_exchange_is_scoped_cached_and_revocable`; later hit/expiry/no-dispatch assertions at `:1025-1059`. The entire journey and its helpers were inspected.

J1 warms the key and posts to `/v1/principals/{revoked_id}/revoke` with `principal_kind: service`. The route suspends the principal, leaving the credential row intact. `service_account_by_id` filters active status, so the issuer subsequently refuses independently of `auth_api_keys.revoked_at`. The assertion that `/auth/token` fails therefore does not establish credential retirement. The journey would still pass if the credential-retirement writer or its active-key filter ceased to work. This is a required proof gap, not evidence that the current sequential retirement implementation is broken.

**Minimum correction.** Replace this journey's suspension step with the existing public credential lifecycle: list the bootstrapped principal's metadata through `GET /v1/principals/{id}/credentials`, identify that warmed key's credential, and retire it through `DELETE /v1/principals/{id}/credentials/{credential_id}` (204). Leave its principal and roles active. Preserve the existing immediate cached success, later non-sliding hit, real production TTL, expired-key stable refusal, `/auth/token` refusal and zero extra provider dispatch. Verify through the public principal read that this principal remains active, so suspension cannot again become the test's hidden refusal source. Reuse Journey's HTTP/admin setup and existing list/delete owners; no production change or second long journey is needed. Update the journey's directly affected description/assertion text and evidence accurately.

**Focused closure proof.** Rerun this exact existing J1 under the managed Postgres wrapper with normal telemetry. Its successful retirement and active-principal control, together with its retained cache-hit and expiry assertions, must prove the credential lookup seam rather than another lifecycle's refusal. This correction remains separate from FIND-TASK-001-1: changing the revoked state alone does not prove overlapping admission timing, and correcting timing alone does not supply the missing negative journey.

### FIND-TASK-001-3 — Required lifecycle rustdoc is incomplete

- Discovery IDs: REPO-001 and MAINT-001.
- Status: **REVISED** (deduplicated scope); classification: **VIOLATION**.
- Violated obligation: AGENTS.md §16 and agent-rules' explicit hard rustdoc requirement; maintainer-style documentation guidance.
- Exact changed locations: `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:72-90` (`GatewayApiKeys::authenticate`); `crates/wyrd/wyrd-testing/tests/gateway/harness.rs:219-243` (`Journey::deploy` and `deploy_as`).

Authentication's rustdoc describes verification/cache behavior and Errors but omits relevant async progress: its awaited shared grant may stage an exchange decision and commit the key's use before cancellation or an uncertain commit prevents the caller from receiving authority or caching the result. A retry with no reusable entry can exchange and audit again. Audit staging is outside the SQL transaction; documentation must not claim audit/use atomicity or a committed exchange for every cancellation. The full grant/issuer/commit bodies above substantiate these conditions.

Both deployment helper bodies were read through `put_as` and `try_put_as`. `deploy` is materially changed into a forwarding method, and new `deploy_as` performs two independently awaited successful-administration assertions. Send failure or refused writes panic, yet both lack `# Panics`. Cancellation/failure after the credential write may leave that credential stored without the deployment. Adjacent `put_as` already documents its panic contract; inheriting the code through a method call does not supply the changed helper's required rustdoc.

**Minimum correction.** Add concise accurate cancellation/partial-progress/retry documentation to cache authentication, and panic plus non-atomic/partial-progress documentation to both deployment helpers. Preserve runtime behavior. Authentication cannot promise inference idempotency; these methods do not own it. No cancellation manager, rollback, retries, new transaction, helper or test is warranted. REPO-001's suggested analogous ingress addition is not required to close these specific evidenced omissions. Batch these comments with the other remediation, with no separate style-only round.

**Focused closure proof.** Inspect the completed comments against the full method bodies and callees already traced; run Rust formatting/lints as part of the combined correction. Prose alone needs no new runtime test.

## Authority resolution and correction limits

The approved spec's REQ-001 explicitly fixes a 60-second revocation ceiling. Its cache requirement and the design's “at most 60 seconds” reuse wording do not require a delayed exchange to receive a full new interval after asynchronous completion. The derived task's “60 seconds after admission” sentence is too prescriptive to satisfy that ceiling for overlapping work; the same task expressly forbids extending the revocation window. The spec-driven-development hierarchy puts approved behavior above the derived private mechanism and permits correction of private task conflicts under that same spec. The original task is evidence, not authority to weaken REQ-001.

A conservative pre-exchange origin satisfies the existing bound: every successful exchange that could have observed a credential before retirement began no later than that retirement, so its origin plus 60 seconds is no later than retirement plus 60 seconds. This needs no new concurrency contract or public decision. It does not withdraw already-issued caller JWTs, change the grant's in-flight semantics, or reinterpret the security posture's snapshot authority. The remediation packet should explicitly supersede the derived admission-origin instruction with this spec-preserving bound; it must not edit approved behavior or silently lengthen the revocation ceiling. **SPEC_REVISION_REQUIRED is not indicated.**

The OTLP wording conflict is already resolved by the task's explicit user-confirmed instruction preserving existing OTLP behavior; it is not a new finding. The correction does not expand API-key acceptance to management, MCP or Bifrost. No security standard research or version-sensitive library inference is necessary for these repository-local source/order findings.

## Minimum verification for combined remediation

Run Cargo-backed checks sequentially. Preserve the supplied full-task evidence for change review; do not rerun full gateway, SDK, principal, SQL or repository suites solely for this bounded correction.

Existing controlled-time test (extend it without losing isolation/expiry/capacity assertions):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::api_keys::tests::gateway_api_key_cache_is_bounded_isolated_and_expires)'
```

Planned focused production-propagation proof (confirm exact collection; keep it IO-free in the owning module with a private local timing/exchange test seam, without exposing a production configuration option or adding a fixture framework):

```bash
mise exec -- cargo nextest run --locked -p wyrd-server --lib -E 'test(=components::gateway::api_keys::tests::delayed_exchange_keeps_its_original_reuse_deadline)'
```

Corrected existing credential-lifecycle journey:

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:all:inner && WYRD_TEST_GATEWAY_PROVIDER_KEY=sk-native-upstream cargo nextest run --locked -p wyrd-testing --test gateway -P journey --run-ignored=all -E "test(=native::gateway_api_key_exchange_is_scoped_cached_and_revocable)"'
```

For the selected Rust cache/test/rustdoc write set: `mise run fmt`, `mise run lints`, `mise run check:deps`, and `git diff --check`. The dependency check proves the private timing seam does not widen shipped test-support boundaries. No manifests, public schema, served OpenAPI description, Python/TypeScript API or docs-site content needs changing to close these findings, so their conditional lanes are not part of this minimum remediation. Any expanded write set must earn its own verification rather than weakening the focused proof above.

## Completeness and limits

All source-local material proposals were adjudicated, including unique INVREV-001 and rustdoc proposals. No missing required report or unresolved contradictory finding remains. The acceptance rows concerning the full revocation ceiling, credential-retirement negative proof and mandatory lifecycle rustdoc cannot currently pass; the other discovery acceptance evidence is not contradicted by these findings. No optional improvements are carried into the ledger.

Skipped runtime reproduction and broad verification. Remaining risk: overlapping successful exchange completion can extend revoked-key reuse, while existing journey proof targets suspension rather than retirement.
