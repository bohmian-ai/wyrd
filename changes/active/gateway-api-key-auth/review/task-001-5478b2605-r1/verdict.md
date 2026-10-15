# TASK-001 verdict

Verdict: **FIX_REQUIRED**

## Immutable subject

Repository: /home/thorrester/Documents/GitHub/wyrd-worktrees/gateway-api-key-auth
Branch: change/gateway-api-key-auth
Base: e0a926031155f263a8721c47d4662dfed8c125ea
Candidate: 5478b2605acae3cecd2522941c30ee7f1674f0be
Reviewed range: git diff e0a926031..5478b2605, all 15 changed paths.
Approved specification: changes/active/gateway-api-key-auth/spec.md, revision 1.
Original task: changes/active/gateway-api-key-auth/tasks/TASK-001-gateway-api-key-auth.md.

HEAD remained the candidate throughout review. Production, specification and original task were not modified. No commit was made. The new review directory contains every required report and this verdict. This is the first review; no prior stable findings require closure.

## Reconciled acceptance matrix

Locations below are repository-relative; server means crates/wyrd/wyrd-server/src and journeys means crates/wyrd/wyrd-testing/tests/gateway. Runtime outcomes are supplied task evidence, independently assessed against source, not review reruns.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-001 / AC-003: only wyrd_sk_ credentials select exchange; other credentials retain JWT verification | server/components/gateway/ingress.rs GatewayCredential::classify and GatewayIngress::authenticate | U1, R1, R2/R3 source and recorded passes | PASS |
| REQ-001: existing carriers and native error envelopes across the three gateway dialects; no exchange-to-JWT fallback | Shared ingress extractor, match arms and error renderer | J1 malformed-key dialect refusals and no-dispatch assertions; P1–P3/T1 | PASS |
| REQ-001 / INV-003: complete existing grant, minting, tenant transaction, commit and exchange audit; hits skip only exchange | GatewayApiKeys::authenticate calls TokenGrants::api_key and shared verifier; existing issuer/outbox | J1 request-correlated retained cold/hit audit; source traces in discovery and validation reports | PASS |
| REQ-001: revoking a key takes effect within 60 seconds | server/components/gateway/api_keys.rs:104–114,149–163 samples reuse origin after awaited grant completion | Existing U2 and J1 omit delayed-admission ordering; source-proven late completion starts a new window | FAIL — FIND-TASK-001-1 |
| Cache is process-local, hash-keyed, bounded, fixed-age on reads, successful-token-only, and verifies each use | GatewayApiKeys::{get,insert,hash,authenticate}; shared AppState Arc | U2 isolation, TTL, shorter lifetime and eviction assertions; J1 permission denial; recorded passes | PASS, subject to deadline-origin failure above |
| INV-001: tenant derives from verified key/token, no cross-key or cross-tenant authority reuse | Existing grant parser/TenantConn, verifier and Caller; full-key hash | J1 second principal denial, tenant-isolated deployment and ledger evidence; tenancy review | PASS |
| INV-002: exactly one accepted header, unchanged query/native-header/Bearer restrictions | Existing extraction runs before classification; executable extraction unchanged | R1 plus J1 mixed, wrong, missing and query carrier refusals | PASS |
| AC-002: credential retirement, while its principal remains active, refuses after cache expiry | Expiry re-enters existing grant, but J1 invokes whole-principal suspension | journeys/native.rs:774–792 uses POST principal revoke; active-key retirement seam is untested by this journey | FAIL — FIND-TASK-001-2 |
| REQ-001: raw key absent from upstream credentials, responses, capture and diagnostics | Secret wrappers, hash-only index, existing provider configuration and capture owners | J1 provider/response/capture/span assertions; P1–P3/T1 upstream credential checks | PASS |
| REQ-001 / AC-002: management, MCP and Bifrost do not gain fallback | Cache called only from public gateway ingress; sibling verifier layers unchanged | J2 warms key then tests mounted excluded surfaces with both bearer carriers | PASS |
| User-confirmed scope: existing OTLP behavior preserved | No OTLP implementation or test changes | Complete diff inspection; original task explicitly preserves existing OTLP exception | PASS |
| AC-001: stock Python OpenAI, Anthropic and Google GenAI and TypeScript OpenAI use URL/key only on invocation | Added tests use ordinary client constructors; Wyrd setup remains in fixtures | P1–P3 buffered/streaming and T1 buffered outcomes recorded passing | PASS |
| Existing token and transport-hook behavior retained | Supplied-token branch, SDKs and invocation/relay machinery remain unchanged | R2/R3 and adjacent Python/TypeScript regressions recorded passing | PASS |
| REQ-002: fallback follows preferred token guidance, explains exposure/window, uses environment example; design records narrow exception | Existing guide fallback section and design Cross-service delegation | Documentation inspection and recorded docs:check pass | PASS |
| No new public headers, grants, SDK APIs, configuration, persistence, migrations, aliases, adapters, dependencies or features | Complete cumulative diff; unchanged manifests and lockfiles | Discovery and validation owner comparisons | PASS |
| Mandatory changed-item lifecycle rustdoc | GatewayApiKeys::authenticate; Journey::{deploy,deploy_as} | Missing relevant cancellation, partial-progress and panic contracts confirmed from complete bodies/callees | FAIL — FIND-TASK-001-3 |
| AC-004: scoped checks and focused command evidence | Original task final evidence and exact selectors match applicable owners | Recorded format/lints/boundaries/docs/OpenAPI and focused runtime passes; review diff checks clean | PASS for supplied execution evidence; does not close findings |

## Independent reports and reconciliation

| Role | Report | Discovery result |
|---|---|---|
| behavior-rev | task-review-behavior.md | PASS |
| invariant-rev | task-review-invariants.md | FAIL: INVREV-001 |
| repo-rev | standards-review.md | FAIL: REPO-001 |
| maintainer-rev | maintainer-review.md | FAIL: MAINT-001 |
| system-rev | system-review.md | FAIL: SYS-001 |
| domain-rev security/RBAC | domain-review-security.md | FAIL: SEC-001 |
| domain-rev tenancy | domain-review-tenancy.md | PASS |
| domain-rev concurrency | domain-review-concurrency.md | FAIL: CONC-001 |
| followup-rev | followup-review.md | RESOLVED |
| ponytail-rev | findings-validation.md | Three retained findings; no unresolved blocker |

Fresh discovery agents worked independently from the same subject/navigation map. A fresh focused follow-up was required because behavior acceptance conflicted with the invariant proof gap and domain timing findings. It distinguished principal suspension from credential retirement and established reachable late admission through actual SQL/commit ordering. A fresh validator then inspected source, adjudicated every proposal and consolidated common causes. Discovery agreement was not used as proof.

The behavioral PASS is reconciled by the two specific revocation failures; its remaining source-supported rows stand. REPO-001 and MAINT-001 merge into one mandatory rustdoc correction. Local import/style and historical missing U1/U2 RED notes are not blocking findings and are not remediation scope. Existing immutable-Card and client credential caches cannot satisfy the requested server reuse lifecycle; no duplicate grant/cache framework finding remains.

## Validated ledger

The complete independent ledger, caller/writer traces and rejected proposals are preserved in findings-validation.md. Only its retained findings govern this verdict.

| Stable ID | Status / class | Discovery sources | Exact correction boundary and consequence |
|---|---|---|---|
| FIND-TASK-001-1 | REVISED / INCORRECT | CONC-001, SEC-001, SYS-001 | api_keys.rs:104–114,149–163: a successful exchange admitted after credential retirement receives a fresh TTL and permits fresh calls after retirement +60 seconds. Anchor reusable authority before that exchange begins; preserve existing grant and in-flight result. |
| FIND-TASK-001-2 | CONFIRMED / MISSING | INVREV-001 | native.rs:774–792,1025–1059: J1 suspends the principal, so it passes independently of credential retirement. Replace that lifecycle step with existing credential list/delete and verify principal remains active; preserve current TTL/audit/refusal assertions. |
| FIND-TASK-001-3 | REVISED / VIOLATION | REPO-001, MAINT-001 | api_keys.rs:72–90 and harness.rs:219–243: required authentication cancellation/retry and deployment panic/partial-write contracts are missing. Complete accurate rustdoc without changing behavior. |

No new product or security decision is required. The approved revocation ceiling governs the derived task's overly specific admission-origin wording. The remediation supersedes that private instruction under the existing specification; it does not alter the approved bound or token snapshot semantics. OTLP remains outside this correction under the original task's user-confirmed scope.

## Verification limits and disposition

All required reports are complete. Reviewers inspected the full cumulative diff, changed owners and relevant callers/tests. Review ran diff checks and identity checks; runtime, outage and race reproduction were not rerun. Recorded lane outcomes remain supplied evidence. In particular, the overlapping exchange/retirement consequence is source-traced, not dynamically reproduced, and the proposed focused propagation test is planned, not collected. U1/U2 pre-implementation RED was not recorded, T1 is buffered-only, and Python covers direct-key streaming. These limits are preserved rather than converted into new scope.

Route the single remediation packet TASK-001-R1-cache-deadline-and-revocation-proof.md directly to $wyrd-implement. Later review must inspect the complete cumulative candidate against the original task, this verdict/ledger and that remediation. No merge, push, deployment or implementation is performed by this review.
