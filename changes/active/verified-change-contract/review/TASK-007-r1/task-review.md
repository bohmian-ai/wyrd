# TASK-007 Task Implementation Review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-vcc-t007`
- Base: `f8811ac5035c3aa165d34c38992f9889b3c9081f`
- Candidate: `2bd4ded8f212f7885b0dd78bcd450fa0ff118f3e`
- Approved specification: `changes/active/verified-change-contract/spec.md`, revision 35
- Original task: `changes/active/verified-change-contract/tasks/TASK-007-operator-connections-and-delivery.md`
- Candidate HEAD was the stated candidate at review start and immediately before this report was written.

## Proposed findings

### TREV-001 — VIOLATION: public key failures disclose secret-provider selectors

- Violated obligation: TASK-007 Scenario 4 and its acceptance criteria require responses, errors, logs, traces, audit, CLI argv, and parsed-argument `Debug` never to expose secret bytes **or secret selectors**; REQ-139 likewise excludes secret material from diagnostics.
- Location: `crates/wyrd/wyrd-server/src/components/operators/keys.rs:36-64`, especially `From<KeyError> for WyrdError`; `crates/wyrd/wyrd-server/tests/pg_operator_connection_routes.rs:497-535`.
- Evidence: `KeyError::Unavailable` includes a `location` containing the concrete env name, file path, or Vault mount/prefix/tenant/version. Its `Display` renders that location, and `From<KeyError>` copies `error.to_string()` into the public `WYRD_OPERATOR_503_KEY_UNAVAILABLE` detail. The route test positively requires the client-visible problem detail to contain `v1`, proving the disclosure is reachable through `POST /v1/operator-connections`.
- Observable consequence: an authorized connection writer receives deployment secret-location metadata such as a key file path, environment selector, or Vault selector in an HTTP/SDK/MCP error; the same value is also logged by the conversion.
- Required testable correction: keep provider location/context out of the public Wyrd error and other prohibited diagnostic surfaces, returning only a stable generic key-unavailable message. Add a focused route test for env, file, and Vault-shaped failures asserting that selector, path, prefix, tenant/version location, and secret bytes are absent while the stable error code remains.

### TREV-002 — INCORRECT: multi-tenant production can start without a readable active tenant KEK

- Violated obligation: REQ-147 requires multi-tenant production to use the external provider and **fail startup if it or the active key is unavailable**. Scenario 3 repeats that startup contract. The approved specification outranks the candidate's appended implementation-evidence statement that only connection writes refuse.
- Location: `crates/wyrd/wyrd-server/src/config.rs:1872-1915`, `crates/wyrd/wyrd-server/src/config.rs:2831-2841`, and `crates/wyrd/wyrd-server/src/boot/mod.rs:1445-1456`.
- Evidence: production validation checks only that the source enum is `Vault`, the address parses, and some token source is configured. It performs no key-provider read. Boot merely constructs `OperatorKeys`; the active key is first read on a later seal/open/rewrap. Therefore an unreachable Vault, missing active tenant key, malformed base64, or wrong-sized key does not prevent server startup.
- Observable consequence: a multi-tenant production replica may advertise readiness while unable to create/rotate connections or deliver Operators for tenants whose active KEK is unavailable, contrary to the approved fail-start contract.
- Required testable correction: at production boot, use the existing configured key owner/provider to prove the active version is readable and exactly 32 bytes for the applicable tenant set before readiness; fail boot on any unavailable required active key. Add a production boot test with a configured but unavailable/malformed active Vault key and a passing case with readable tenant keys. If the intended behavior is instead write-only refusal, revise and reapprove REQ-147 before implementation.

### TREV-003 — VIOLATION: HTTP credentials are decrypted and attached before effective-URL SSRF screening

- Violated obligation: REQ-149 and TASK-007's acceptance criteria require every effective templated HTTP URL and redirect to retain authority and pass resolve-screen-pin SSRF checks **before credentials are attached** on every attempt.
- Location: `crates/wyrd/wyrd-server/src/verification/operators.rs:399-420`, `481-526`, `663-692`, `749-784`, and `854-927`.
- Evidence: `OperatorWorker::attempt` opens the credential before delivery; `OperatorDelivery::send` calls `HttpRequest::render`, which inserts the bearer/basic/custom credential into `HeaderMap`; only afterward does `OperatorDelivery::http` call `client`, which resolves, screens, and pins the effective URL. Redirects reuse the already credential-bearing request and screen only after replacing its URL. The current mock tests prove blocked requests are not sent, but they do not prove the required ordering and the source establishes the opposite order.
- Observable consequence: blocked or unresolvable destinations cause Wyrd to decrypt and materialize credentials unnecessarily, violating the approved trust-boundary sequence even though the current request is refused before network transmission.
- Required testable correction: render the nonsecret effective request, validate stored authority, and resolve-screen-pin the effective URL first; only then open and attach the credential immediately before sending. Repeat that order for every redirect. Add a focused delivery check with an instrumented credential/key owner proving blocked/unresolved initial URLs and rejected redirects perform no credential open/attachment or provider call.

### TREV-004 — DRIFT: the candidate changes implementation/review workflow skills unrelated to Operator delivery

- Violated obligation: task scope is Operator connections and delivery; task review accepts no unrelated change in the base-to-candidate range. The task neither requires nor authorizes changing repository agent workflows.
- Location: `.agents/skills/wyrd-implement/SKILL.md:56-78`, `.agents/skills/wyrd-task-review/SKILL.md:80-85`, and the mirrored `.claude/skills/...` files in the candidate diff.
- Evidence: the candidate adds general unexpected-failure/diagnostician policy to two skills and mirrors it for another harness. No TASK-007 requirement, scenario, consumer, or failure diagnosis depends on those edits.
- Observable consequence: accepting TASK-007 would silently alter repository-wide implementation and review process for every later change, expanding the task's blast radius and coupling an unrelated policy change to Operator delivery.
- Required testable correction: remove the four skill-file changes from this task candidate. If the policy is desired, deliver it as its own approved change with its own review.

### TREV-005 — VIOLATION: production Vault configuration permits plaintext transport of the Vault token and KEK

- Violated obligation: TASK-007 prohibits secret leakage and weakening security; INV-007 applies the repository security posture, whose external trust boundaries require encryption in transit. Multi-tenant production requires Vault specifically, making its credential transport part of this task's production path.
- Location: `crates/wyrd/wyrd-server/src/config.rs:1903-1906` and `crates/wyrd/wyrd-server/src/components/operators/keys.rs:122-170`.
- Evidence: configuration accepts both `http` and `https` Vault addresses in every profile. `vault_key` then sends the `X-Vault-Token` header and receives the plaintext/base64 KEK over that URL. Thus a valid multi-tenant production configuration can use `http://...` and transmit both secrets without TLS.
- Observable consequence: a network observer or intermediary can obtain the Vault token and tenant KEK, defeating the envelope-encryption boundary and exposing every Operator credential protected by that key.
- Required testable correction: require HTTPS Vault addresses in production (retaining only an explicitly bounded local-development exception if needed) and add configuration tests that reject plaintext Vault transport for multi-tenant production.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-097: failed binding-created completion settles and inserts one unique dispatch per distinct frozen Operator in one transaction; all other outcomes/direct runs do not | `verifier_runs.rs` `COMPLETE_RUN_SQL`, `INSERT_DISPATCH_SQL`, and `VerifierRunQueue::complete` | `pg_operator_delivery::failed_verdict_fans_out_to_every_provider_independently`; SQL tests named in task evidence | PASS |
| REQ-098/REQ-099: separate generic leased Operator worker, independent durable status/retry/error, no direct Verifier call/broker/Alert, provider acceptance boundary | `verification/operators.rs`; `operator_dispatches.rs`; existing dispatch table | Delivery integration tests and status assertions in `pg_operator_delivery.rs` | PASS |
| REQ-138: bounded immutable failure context with closed template fields and no raw detail/secret | `OperatorFailureContext`; SQL `jsonb_build_object`; 512-character summary clipping | Operator contract unit tests and delivery assertions | PASS |
| REQ-139: per-attempt latest tenant/provider/name credential resolution, safe indistinguishable connection failures, no cache/env selector in Card | `OperatorWorker::credential`; connection-backed Operator contracts | revocation/key-outage and multi-replica rotation tests | FAIL (TREV-001 diagnostic selector disclosure) |
| REQ-147: UUIDv7, forced-RLS ciphertext-only envelope storage, canonical AAD, fresh DEK/nonce, versioned external KEK, rotation/rewrap | migration; `wyrd-crypt::{seal,open,rewrap}`; `OperatorKeys`; SQL queries | crypto unit test; connection row inspection; rewrap journey | FAIL (TREV-002 startup; TREV-005 transport) |
| REQ-148: typed create/list/get/update/disable over HTTP, Rust/Python/TS/CLI/MCP; read/write RBAC; transactional audit; no read-secret | typed routes and `OperatorConnectionControl`; shared client and thin projections | route, Rust SDK, Python, TypeScript, CLI, MCP evidence recorded in task | PASS |
| REQ-150: closed provider-tagged create/PATCH unions, immutable provider/name, redacted read authority, typed UUIDv7 ID; Card connection authority shapes | `operator_connection.rs`; `card/operator.rs` | contract unit tests, generated schemas, route journey | PASS |
| REQ-149: exact active provider/name/origin/auth authority at registration and attempt; forbidden headers; effective URL/redirect SSRF before credential attachment | `OperatorSpec::matches_authority`; `resolve.rs::check_operator`; `ScreenedHttp` | registration mismatch test and delivery redirect test | FAIL (TREV-003) |
| REQ-140: Slack bot token/channel/chat.postMessage and JSON `ok`; terminal configuration errors and transient provider errors | `OperatorDelivery::slack` | local mock fan-out/provider failure journey; live smoke gated | PASS |
| REQ-141: PagerDuty Events API v2 route, severity, summary, subject source, stable dedup | `OperatorDelivery::send` PagerDuty arm | local mock payload assertions; live smoke gated | PASS |
| REQ-142: bounded attempts/timeouts/deadline/backoff/Retry-After, response bound, pinned URL, stable dispatch identity, at-least-once ambiguity | `RuntimeLimits`; `OperatorDispatchQueue`; `OperatorDelivery` | slow endpoint/retry/exhaustion/redirect journey | FAIL (TREV-003 ordering) |
| REQ-143: Workflow remains typed but verification `on_failure` registration rejects it; worker never reports it delivered | registration `check_operator`; terminal worker arm | registration compatibility test | PASS |
| REQ-145: connection read/write RBAC split and transactional allow/deny audit; internal worker mechanics unaudited | `OperatorConnectionControl`; internal worker has no permission evaluation | route RBAC/audit journey and broader recorded authorization lanes | PASS |
| REQ-146: 16 global/4 per tenant, three attempts, 30-second attempt, five-minute deadline, 30s/2m backoff, 30-second drain, restart/health/metrics | `RuntimeLimits`; permits; runtime supervisor; metrics | fairness, slow endpoint, restart, shutdown delivery tests | PASS |
| REQ-152/AC-033: PostgreSQL coordination clock for dispatch timestamps, deadlines, eligibility, claims, leases, retry | SQL consistently uses `statement_timestamp()` and returns DB decisions/remaining intervals | SQL/delivery tests move rows rather than clocks | PASS |
| INV-006: Cards contain no secret/mutable runtime state | connection names replace env/secret fields; secrets live only in encrypted connection rows | schemas and fixtures | PASS |
| INV-007: tenant isolation and canonical audit/security apply | forced RLS, `TenantConn`, permission/audit owner | tenant isolation and RBAC journey | FAIL (TREV-005 weakens encrypted transport) |
| INV-011: Trigger/Verifier judge; Operators act independently afterward | settlement creates durable tasks after acknowledged failed result | fan-out journey | PASS |
| INV-013: inline and referenced Operators have identical semantics | frozen UID/digest target resolution and shared delivery path | fan-out fixtures include both target shapes | PASS |
| INV-015: PostgreSQL owns verification time predicates | queue SQL | recorded SQL lane | PASS |
| AC-029: full local mock-provider fan-out, provider failures/rate limit/redirect/revocation/key outage and independent status; live smoke gated | provider worker/adapters and tests | task records local test PASS; live Slack/PagerDuty is ignored/gated | FAIL (TREV-003) |
| AC-030: authorization, fairness, bounds, timeout/retry/deadline, shutdown, restart, health/metrics | runtime, permits, queue, service authorization | task records relevant integration/journey lanes PASS | PASS |
| AC-031: public admin journey, ciphertext/redaction/RLS, closed failures, multi-replica next-attempt rotation, every surface | connection stack and SDK/CLI/MCP projections | task records surface journeys and Postgres inspection PASS | FAIL (TREV-001, TREV-002, TREV-005) |
| Task security constraint: no secret bytes or selectors in public/diagnostic surfaces | redacted view types and secret-bearing wrappers exist | one route test explicitly expects selector fragment in public detail | FAIL (TREV-001) |
| Task boundary: SSRF screen/pin before secret attachment every attempt | screening/pinning exists | current source ordering is credential-open/attach then screen | FAIL (TREV-003) |
| Non-goals: no env selectors in Cards, plaintext persistence/read-secret, new cipher/dependency/cache/watcher/broker/Alert/upload lifecycle/executable Workflow/exactly-once claim/checked-in OpenAPI | diff uses existing crypto/reqwest, runtime OpenAPI, Postgres dispatch; no prohibited resource/API found | static diff inspection and recorded codegen/boundary lanes | PASS |
| Scope closure: no unrelated changes | four repository workflow-skill files changed | diff inspection | FAIL (TREV-004) |

## Verification limits

- This reviewer inspected the complete base-to-candidate file list and the task-critical contract, crypto, connection CRUD, SQL/migration, registration, dispatch, delivery, SSRF, configuration/boot, SDK/CLI/MCP, and test paths. The 117-file/14k-line range prevented line-by-line inspection of every generated schema and every thin language projection within the hard 20-minute reviewer cap.
- No test lane was rerun by this reviewer. The candidate task records exit-zero results for the requested broad lanes and a focused delivery target (5 passed, 1 gated/ignored); those results were treated as available evidence, not independently reproduced evidence.
- The credentialed Slack/PagerDuty live smoke remains gated and was not run. This is an accepted release-evidence boundary, but real-provider acceptance is not independently established here.
- Generated schemas/stubs were sampled through their owners and recorded `codegen:check`; individual generated JSON files were not manually audited.
- No independently validated failure diagnosis accompanied the unrelated workflow-skill edits; they are scope drift rather than task evidence.

## Overall result

**FAIL**

The core encrypted connection and durable fan-out machinery is substantially present, but five material findings remain. TREV-001, TREV-002, TREV-003, and TREV-005 violate explicit security or production behavior; TREV-004 is removable unrelated drift. The bounded implementation/security corrections do not require a new product contract, except that retaining the candidate's write-only key-unavailability behavior would require revising and reapproving REQ-147.
