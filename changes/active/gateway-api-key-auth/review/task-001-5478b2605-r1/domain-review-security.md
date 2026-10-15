# Security and RBAC domain review

Overall: FAIL

Subject: base e0a926031155f263a8721c47d4662dfed8c125ea to candidate 5478b2605acae3cecd2522941c30ee7f1674f0be. HEAD remained the candidate during inspection. Only this report was written.

## Reviewed boundary and authority

Reviewed the complete cumulative diff, expanding the navigation map through credential extraction, the shared API-key grant, tenant-bound verification and issuance, JWT verification, Caller construction, route mounting, gateway admission and audit, capture, diagnostics, and the changed Rust/Python/TypeScript journeys and documentation.

Governing authority: approved specification REQ-001/REQ-002, INV-001–INV-003 and AC-001–AC-003; original TASK-001 including its explicit gateway-only scope; AGENTS.md security, audit, ownership and testing requirements; architecture/agent-rules.md; architecture/wyrd-design.md credential and delegation rules; architecture/wyrd-security-posture.md trust boundaries, API keys, token lifetime, authorization, audit and secret handling; architecture/wyrd-doctrine.mdx; the security foundation and principal-extraction/permission-check references; spec-driven-development and maintainer-style references. The approved gateway exception supersedes the earlier general bootstrap-only API-key wording. OTLP already has a separate static-key path and remains unchanged as explicitly directed by the task.

## Source and obligation coverage

| Boundary / obligation | Source evidence | Assessment |
|---|---|---|
| Classification after exactly-one-header extraction; no JWT fallback for refused keys | components/auth/token_extract.rs:185–245; components/gateway/ingress.rs:122–196 | PASS. Existing carrier counting, native-header restrictions and query restrictions execute first. Only the value prefix selects the new branch. |
| Complete existing grant rather than alternate principal construction | gateway/api_keys.rs:104–108; auth/routes.rs:143–155, 237–264, 310–325 | PASS. Both token endpoint and gateway call TokenGrants::api_key, including parsing, tenant transaction, grant execution, failure mapping, audit staging and commit. OTLP's principal-only helper is not used. |
| Key-bound tenant and current grant authority at issuance | wyrd-auth/src/exchange_api_key.rs:159–177, 221–242; wyrd-auth/src/issuance.rs:424–481, 565–625 | PASS. Parsed tenant selects TenantConn; full credential verifies within it; issuer resolves the stored principal and permissions. Cache identity is SHA-256 of the complete key. |
| Verify cached and newly exchanged tokens using the existing trust boundary | gateway/api_keys.rs:91–108; auth/token_extract.rs:143–153; wyrd-auth-verify/src/lib.rs:339–365 | PASS. Signature, configured key/issuer, Wyrd audience, expiry, tenant and claim promotion remain enforced. No cached principal bypass exists. |
| Authorize and audit every gateway call | auth/caller_extractor.rs:40–71; gateway/invocation.rs:428–477, 652–689 | PASS. Caller derives from the verified principal. The ordinary model permission check remains before dispatch and stages allowed/denied decisions on ScribeOutbox. A cache hit skips only the exchange. |
| Fixed bounded secret cache and revocation ceiling | gateway/api_keys.rs:99–114, 130–162 | FAIL: SEC-001 below. Ordinary hits do not slide the deadline, capacity is bounded, and failures are not cached; elapsed time of an in-flight exchange is omitted from the revocation bound. |
| Secret wrappers and no carrier forwarding | gateway/api_keys.rs; gateway/ingress.rs native handlers; gateway/invocation.rs request/admitted-call ownership; http/router.rs:149–166; changed native and stock-client journeys | PASS for the credential-header path. Raw credentials remain SecretString/SecretBearer; cache keys are digests. Request headers do not enter GatewayCallRequest or AdmittedCall, and root tracing records path without query or headers. Provider credentials are resolved by the existing engine. |
| Restrict fallback to public gateway routes | gateway/ingress.rs:201–231; http/router.rs:59–119; http/middleware/authenticate.rs; auth/token_extract.rs:25–65 | PASS. Gateway router alone calls the new cache. Management, MCP and Bifrost still use the ordinary token-only verification path. |
| Caller-facing fallback documentation | changed design delegation paragraph and stock-client guide fallback | PASS for preferred-token guidance, scope and secret exposure. The claimed revocation ceiling needs SEC-001 closed. |

## Proposed material finding

### SEC-001 — INCORRECT: the cache starts the revocation window after awaited exchange work

Violated obligation: REQ-001 requires revoking a key to take effect within 60 seconds; TASK-001 expressly says private synchronization cannot extend that bound.

Exact changed location: crates/wyrd/wyrd-server/src/components/gateway/api_keys.rs:104–114, especially the fresh Instant::now() at line 113; deadline construction at line 158.

Evidence and reachable path: authenticate awaits the complete grant, then verifies the minted token, and only then samples the start of its 60-second deadline. The existing grant validates the key, touches its use row, resolves principal/grants and signs the token, and then awaits transaction commit. Credential retirement updates the credential row through revoke_credential_for/revoke_api_key; principal suspension uses a separate principal update. Neither lifecycle invalidates the gateway cache. Once the exchange transaction commits, retirement can complete before the exchange future resumes and inserts its result; principal suspension can also overlap after issuance's authority read. Ordinary IO response delay or task scheduling therefore allows an exchange that authenticated before revocation to be inserted after revocation. It then remains reusable until insertion time plus 60 seconds. An overlapping older exchange can also replace an existing entry with that later deadline; all successful misses reach the same insert owner.

Observable consequence: a caller can continue using a revoked credential beyond the promised 60 seconds by the amount of post-validation exchange/completion delay, while every hit remains locally valid as the five-minute JWT. The verifier does not re-read credential state, by design. The current unit test supplies one admission time directly and the journey revokes only after its warm request has returned, so neither covers this overlap.

Smallest testable correction: keep the existing grant, verifier and gateway cache; bound reuse by an instant captured before starting that exchange, so delayed completion and overlapping exchange replacement cannot create authority whose reuse deadline is later than that exchange's start plus 60 seconds. Do not add revocation polling, another store, locks around IO, or new public configuration. Do not publish a reusable entry once its fixed bound has already passed. Preserve token verification, per-call authorization/audit and normal grant error behavior. A deterministic focused test should pause an exchange across revocation/completion or otherwise exercise the real production deadline propagation and prove that late completion cannot refresh authority beyond the original bound; retain the ordinary 59.999/60-second and non-sliding checks.

## Verification and limits

Inspected the actual changed tests and their assertions, including the real-server miss/hit exchange-audit attribution, underprivileged same-tenant key, second-tenant deployment/ledger separation, malformed credentials in all three dialect envelopes, no refused dispatch, warmed-key exclusions and secret checks. The recorded final evidence reports U1/U2/R1 and J1/J2/R2/R3 passing, three Python stock-key journeys, the TypeScript stock-key journey, adjacent stock-client regressions, formatting/lints and boundary checks. These results are supplied task evidence, not independently rerun results.

No Cargo or live-server lane was rerun during this read-only review. No live proxy, TLS termination or deployment logging configuration was inspected; the secret assessment covers repository-controlled request handling and the explicit test captures. The revocation journey suspends a principal rather than retiring one credential and begins revocation only after warming completes; it does not prove the concurrent completion boundary in SEC-001. No optional hardening or unrelated pre-existing security implementation was included as a finding.
