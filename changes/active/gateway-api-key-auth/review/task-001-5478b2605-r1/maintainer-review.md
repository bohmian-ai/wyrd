# Independent maintainer review

Subject: base `e0a926031155f263a8721c47d4662dfed8c125ea` to candidate `5478b2605acae3cecd2522941c30ee7f1674f0be`.

Overall result: **FAIL** — one bounded rustdoc correction. No production restructuring is required by this review.

## Authority and coverage

Read the assigned task-review role, immutable subject, approved specification, original task and appended verification record, repository instructions, agent rules, maintainer style, spec-driven development, Rust-core and testing guidance, and applicable identity, credential, delegation, audit and public-surface design/doctrine/security sections. Used CodeGraph before code searches. HEAD remained the candidate at the end of source inspection.

| Changed surface | Source and consumer coverage | Assessment |
|---|---|---|
| `components/gateway/api_keys.rs` | Complete module, every method and cache unit test; ingress caller; process field and initialization; `TokenGrants::api_key`, `api_key_exchange`, `ExchangeApiKey::execute`, token verification | Coherent stateful owner, minimal cache operations, explicit fixed deadline and key hashing; documentation omission below |
| `components/gateway/ingress.rs` | Complete changed authentication/classification symbols, router/layer composition, relevant native route annotations, unit test; shared extractor and verifier | Classification is local and synchronous; extraction precedes classification; protocol errors remain centralized |
| Auth routes and extraction | Complete API-key grant, transaction and error/audit tail, shared tenant-selection helper; access-token verification and credential extractor; sibling grant and OTLP principal paths from navigation | Existing grant is extended in visibility rather than copied; existing verification remains authoritative |
| `state.rs`, gateway exports, OpenAPI | Added process-owned `Arc` field and construction, gateway re-export, changed served descriptions | One process lifetime, no new public SDK contract or declaration |
| Rust journey harness | Complete changed `try_put`, `try_put_as`, `put`, `put_as`, `deploy`, `deploy_as`; owning `Journey` state/start/connect and adjacent request helpers | Existing harness extended for alternate caller; missing panic contract below |
| Rust native journeys | Complete added completion/request-id/audit/capture helpers and both added journeys; existing dialect tests around their setup and refusal conventions | Assertions follow observable dispatch, identity, retained decisions, ledger, capture and expiry; no new general fixture framework |
| Python stock clients | All added tests, module context, fixtures and existing stock-client helpers | Reuses existing real-server fixtures; tests instantiate stock clients with the key and inspect response/stream and upstream credentials |
| TypeScript stock client | Complete added test, shared fixture/setup, request and provider constants | Reuses existing server/provider owner; no binding or declaration change |
| Guide and active design | Complete changed fallback section and cross-service delegation paragraph, surrounding access-token and OTLP exception context | Explains fallback, provider credential separation and revocation window without creating a client API |
| Task artifact | Cumulative task revision and verification record | Evidence inspected as claims; it does not replace source |

## Existing-owner comparison

`verification/cache.rs::VerifierCache` caches immutable parsed Verifier Cards keyed by tenant/UID and charges serialized bytes. Its ownership and invalidation contract are different from a credential-hash cache with a fixed 60-second deadline. Reusing that concrete cache would couple unrelated domain values and would require generalization. The new implementation appropriately follows its existing short synchronous mutex plus installed `lru::LruCache` pattern instead.

`wyrd-client/src/auth.rs::AuthMiddleware` owns one client's credential, HTTP token exchange, proactive token refresh, optional credentials-file persistence and a single-flight gate. It cannot serve this server-side multi-key cache without a client-to-server round trip, wrong credential ownership, and different refresh/deadline semantics. No shared cache abstraction is earned here.

The full existing server exchange workflow remains in `TokenGrants::api_key`: parse and tenant connection, existing exchange owner, failure mapping/audit staging, commit and response construction. `GatewayApiKeys::authenticate` delegates that workflow and reuses the existing verifier on both hit and miss. It introduces the requested reuse policy, not a second grant implementation. `GatewayIngress` remains the one router-specific extraction, classification, principal insertion and envelope owner. Passing invocation-owned server state/request identity to the cache does not justify another service layer or cyclic retained ownership.

## Material finding

### MAINT-001 — VIOLATION: document newly introduced asynchronous partial progress and helper panic behavior

Locations:

- `crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:72-90`, `GatewayApiKeys::authenticate` rustdoc.
- `crates/wyrd/wyrd-testing/tests/gateway/harness.rs:233-236`, new `Journey::deploy_as` rustdoc.

Governing rule: `AGENTS.md` §16 requires relevant cancellation/partial-progress/retry documentation for async and durable operations, and `# Panics` wherever a panic remains possible. Missing or incomplete rustdoc on a changed item is a hard blocker. Maintainer Style's documentation section applies this to private and test helpers as well.

Evidence and concrete maintenance cost: cache authentication now awaits the existing grant, whose `TokenGrants::api_key` commits the key use/exchange before returning. A request cancelled around that commit can leave durable exchange progress and its staged decision without reaching cache insertion or the handler. Retry may exchange and audit again; authentication does not make inference execution idempotent. The new owner's docs explain hits, misses and errors but omit this relevant cancellation boundary. A maintainer changing request cancellation/retry behavior must currently infer it across owners. Separately, `deploy_as` awaits `put_as` twice; that helper asserts success, and its HTTP helper expects a successful send. The new helper's docs omit `# Panics`, although adjacent `put_as` explicitly documents the same failure conditions.

Smallest correction: add concise accurate rustdoc to these changed symbols. For authentication, describe possible committed grant/use/audit progress before cancellation and that an uncached retry can exchange again; retain existing grant and invocation behavior. For `deploy_as`, document that failed/refused credential or deployment administration requests panic. Do not add a cancellation manager, retry mechanism, fixture, or production behavior change.

Closure proof: inspect the comments against the existing grant commit/cache insertion and `put_as` call paths; run formatting if needed. No new runtime test is necessary for this documentation correction. Batch it with other validated remediation; this finding does not request a separate style-only implementation round.

## Non-blocking calibration

No additional style-only findings. In particular, the closed credential enum, a separate state-owning cache module, fixed private capacity, cloned secret token at the existing verification boundary, and alternate-administrator harness methods each have a concrete role. No proposed abstraction, generic cache, configuration surface or new dependency is justified.

## Verification limits

Reviewed supplied exact-test and scoped-lane results and their source assertions; did not rerun Cargo or live-server lanes during this independent static review. No generated declarations or SDK exports changed. This report assesses maintainability and owner reuse; independent security, concurrency, tenancy, behavior and system reviews own their assigned conclusions.
