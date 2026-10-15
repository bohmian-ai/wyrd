# Independent invariant review

Result: **FAIL** — one required negative-journey proof exercises principal suspension instead of credential revocation. No production authentication bypass was identified.

## Subject and coverage

- Base: `e0a926031155f263a8721c47d4662dfed8c125ea`.
- Candidate: `5478b2605acae3cecd2522941c30ee7f1674f0be`; HEAD still matched at review.
- Original task: `changes/active/gateway-api-key-auth/tasks/TASK-001-gateway-api-key-auth.md`.
- Approved specification: `changes/active/gateway-api-key-auth/spec.md`, revision 1, with the task's explicit gateway-only clarification preserving OTLP.
- Inspected the cumulative production, test, documentation, and task-evidence diff. No other reviewer report was consulted.

Applied the task-review invariant role, repository rules, agent rules, spec-driven-development and maintainer guidance, testing workflow, Wyrd identity/delegation design, doctrine, and security authority. Used CodeGraph before source navigation and expanded the supplied map through the grant, issuance, verification, principal extraction, credential lifecycle, SQL lookup, gateway admission, client cache, and existing cache owners.

## Producer-to-consumer trace

1. `extract_gateway_access_token` remains the credential trust boundary: query restrictions and exactly one accepted header are enforced before classification. `GatewayCredential::classify` changes only the prefix dispatch, retaining ownership in a secret wrapper. Malformed prefixed values reach the grant; a failed grant never enters the JWT branch.
2. `GatewayApiKeys::authenticate` hashes the complete key for lookup. Both cached and freshly minted tokens go through the existing Wyrd-audience `verify_access_token`. Only a successful grant and successful verification can reach insertion; neither errors nor principals are cached.
3. A miss calls the same `TokenGrants::api_key` as `/auth/token`. Its existing parser selects `parsed.tenant_id`; its tenant connection owns the transaction; `ExchangeApiKey::execute` verifies the credential, records use, and calls `TenantTokenIssuer::issue`. The grant commits before returning. Existing issuance/refusal audit uses the shared non-blocking outbox.
4. The verifier result becomes `AuthenticatedPrincipal`, then `Caller::from_authenticated` derives tenant and principal together. `GatewayInvocation::admit_call` continues its per-call permission decision and tenant-scoped gateway snapshot/ledger work. Cache hits therefore skip the minting grant, while gateway authorization remains live on every request.
5. `AppState` shares the gateway cache through `Arc`. The mutex protects only synchronous lookup/insertion; no network or database await holds it. Each entry has a fixed admission deadline, 4096-entry LRU bounds, a hashed key, and a secret token. Reads change recency, not deadline. Concurrent misses can independently exchange; single-flight is not an approved requirement.
6. Management/MCP/Bifrost sibling consumers retain normal token verification. The new classification owner is only called by public gateway ingress. OTLP's existing separate API-key authentication owner is unchanged.

The new retained-state owner is justified: `VerifierCache` holds immutable Cards, and client `AuthMiddleware::bearer` owns one client's credentials, renewal, and transport concerns. Neither is a server multi-key credential cache. Reusing the installed LRU/SHA-256 mechanisms with a small gateway owner avoids a new generic cache layer or a duplicate grant workflow.

## Acceptance matrix

Verification evidence below means the recorded outcomes in the task packet plus independent inspection of test assertions. I did not rerun Cargo, Python, or TypeScript lanes during this read-only review.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-003: only `wyrd_sk_` selects exchange; other values preserve JWT verification | `ingress.rs:122–157,171–196` | Classification unit test; existing extraction and native access-token journeys recorded passing | PASS |
| Preserve accepted carriers, Bearer syntax, exactly-one-header and query restrictions (INV-002) | Unchanged `token_extract.rs:185–245`, called before classification | Existing extraction negatives plus J1 mixed/wrong/missing/query carrier refusals | PASS |
| Reuse complete grant, tenant selection, token minting, transaction commit, and mapped errors | `api_keys.rs:111–124`; `auth/routes.rs:237–258,297–309` | J1 cold request and retained exchange-audit identity; existing grant source | PASS |
| INV-001: parsed key selects tenant; complete-key hash prevents authority reuse | `auth/routes.rs:301–309`; `api_keys.rs:127–149` | Unit distinct-key tests; J1 reader refusal and second-tenant deployment/ledger isolation | PASS |
| Fixed 60-second admission deadline, no renewal by reads, count-bounded process-local cache | `api_keys.rs:33–54,138–176`; shared state in `state.rs:2198–2200,2273` | Controlled-time unit assertions at 59.999s and 60s, repeated hits, earlier token lifetime, and eviction | PASS |
| Successful exchanges only; no failure authority; token verification on every use | `api_keys.rs:98–124` | Unit/cache and J1 negative/refusal proof; source establishes insertion follows both success checks | PASS |
| INV-003: miss records exchange decision; hit skips only exchange audit | Existing grant/issuer outbox path; `api_keys.rs:105–124` | J1 request-correlated cold audit and three hit audit checks | PASS |
| Invalid/malformed keys return grant's stable error in each protocol, with no dispatch | `ingress.rs:94–110,138–157`; unchanged grant error mapping | J1 malformed-key loop checks all three envelopes and unchanged upstream count | PASS |
| AC-002 / Scenario 3: a revoked credential is refused after cache expiry | Production path re-enters existing credential-verifying grant on expiry | J1 suspends the principal instead of retiring the credential; see INVREV-001 | FAIL |
| Raw key excluded from provider credentials, durable capture, diagnostics, responses | Secret wrappers throughout new owner; only principal/body enters invocation | J1 checks upstream headers, response/refusal bodies, capture cells, span attributes; official-client tests check provider key isolation | PASS |
| Cached key still refused by mounted management, MCP, Bifrost routes | New owner used only from ingress; general extractor unchanged | J2 warms key, tests actual mounted surfaces with both headers, compares access-token requests | PASS |
| AC-001: stock OpenAI Python/TypeScript, Anthropic Python, Gemini Python use URL/key only on invocation | New stock-client tests use clients' ordinary key/base-URL options | P1–P3 buffered/streaming and T1 two buffered turns recorded passing | PASS |
| Preserve existing access-token and transport-hook behavior | Existing branch remains; no SDK implementation changes | R2/R3 and adjacent Python/TypeScript files recorded passing | PASS |
| REQ-002: fallback after preferred token path, exposure and window explained, environment example, design exception | Guide appended section; design cross-service-delegation paragraph | Documentation inspection; `docs:check` recorded passing | PASS |
| No OTLP changes or expanded non-gateway credential acceptance | Production diff confined to gateway plus internal grant visibility and comments | Diff inspection; J2 | PASS |
| No new headers, grant, SDK API, config, dependency/feature, persistence, alias, migration or provider adapter | Complete cumulative diff and unchanged manifests | Diff inspection | PASS |
| AC-004: scoped format/lints/checks and exact proof recorded | Task evidence names focused commands and final outcomes | U1/U2/R1, J1/J2/R2/R3, P1–P3/T1, adjacent regressions, scoped lint/boundary/docs/OpenAPI checks recorded passing; review `git diff --check` passed | PASS |

## Proposed finding

### INVREV-001 — Credential revocation is not exercised by the claimed revocation journey

- Classification: **MISSING** (required security lifecycle proof).
- Obligation: AC-002, Scenario 3, and the task acceptance row requiring a warmed key revoked through the existing credential lifecycle to fail once its cache entry expires. AGENTS.md §11 requires the negative user flow at journey tier.
- Exact changed location: `crates/wyrd/wyrd-testing/tests/gateway/native.rs:774–792`; its later expiry assertions are at `1025–1058`.
- Evidence: the test calls `POST /v1/principals/{revoked_id}/revoke` with `principal_kind: service`. The owner at `auth/revoke.rs:1,20–23,106–114` suspends the principal. That leaves credential retirement unexercised. Actual credential revocation has a distinct owner: `components/principals/routes.rs:450–556`, exposed as `DELETE /principals/{principal_id}/credentials/{credential_id}`, which calls `revoke_api_key` and commits `auth_api_keys.revoked_at`. The credential producer/consumer seam is `wyrd-sql/src/queries/auth/service_accounts.rs:352–378`, whose lookup filters `k.revoked_at IS NULL`, then issuance resolves principal status separately. A suspended principal makes issuance fail even if credential retirement or its lookup filter stops working.
- Observable consequence: the advertised key-rotation/refusal contract has no matching end-to-end negative proof. This journey would continue passing if retiring a credential failed to stop re-exchange while the principal remained active. This is a proof defect, not a claim that current production code permits that failure.
- Sibling owners traced: `/auth/token` and gateway share `TokenGrants::api_key`; OTLP uses the grant's principal-only sibling; HTTP/MCP credential retirement share `revoke_credential_for`; whole-principal suspension owns a different durable state. No new revocation implementation is needed.
- Smallest correction: change J1's existing revocation step to retire the warmed key through existing public credential metadata/listing and delete operations, leaving its principal active. Keep the real cache TTL, the immediate cached success, later hit, expired-key refusal, stable error, and zero-dispatch assertions. Retain the grant rejection check. Do not add production synchronization, a second revocation helper, configuration, or another minute-long journey.
- Focused closure proof: run the existing exact J1 command from the task under the managed Postgres wrapper, with its normal telemetry. Show the credential retirement succeeds and the unchanged active principal's warmed credential stops working after fixed expiry. The corrected test must target credential state rather than use principal suspension as a proxy.

## Verification limits and non-blocking notes

The task records U1 and U2 were not captured as RED, and J1's RED was obtained by temporarily forcing JWT classification. Those historical limits are explicit; they do not independently establish a reachable production defect. I did not manufacture a later RED or request implementation changes solely to rewrite history.

T1 covers buffered OpenAI calls; Python P1 covers direct-key streaming. Trace inspection asserts span attributes rather than every possible event/log sink. The source path's secret wrappers and scrubbed existing grant instrumentation support the secrecy result; the report does not claim an independent exhaustive logging experiment.

No broad or simultaneous verification lane was run. Review source remained immutable; only this report was written.
