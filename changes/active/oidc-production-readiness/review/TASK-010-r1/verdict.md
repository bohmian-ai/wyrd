# TASK-010 R1 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `04366e7fc28c466fcdcbc7279885cfee82a988a2`
- Approved specification: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- Routed prior finding: `FIND-TASK-004-12` from `TASK-004-r2`

The candidate remained the stated immutable commit throughout discovery,
follow-up, and structured validation. `.codegraph/` is absent. Every required
reviewer report was obtained.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| Real-server authorization code + S256 through `openid-client` 6.8.8 | The server path exists, but the current identity journey hand-builds the requests and the named client proof is deferred in the task evidence. | **FAIL — `FIND-TASK-010-1`** |
| Hashed, 60-second, single-use authorization code bound to tenant, principal, client, redirect, connection revision, and PKCE | Code attachment and redemption use the existing login-state and tenant-transaction owners; focused refusal and replay tests are recorded green. | PASS |
| Routed terminal-device race and mint-at-redemption behavior | Source now guards approval and atomically locks, deletes, issues, audits, and commits at redemption, but the required pause/terminal/resume proof is absent. | **FAIL — `FIND-TASK-010-2`** |
| RFC 8628 pending, slow-down, denied, expired, and one-use outcomes | Device route/service and recorded focused journey cover the ordinary states. | PASS, subject to `FIND-TASK-010-2` and `FIND-TASK-010-10` |
| RFC 8693 registered error behavior | Unsupported target audiences are flattened into `invalid_request`; `invalid_target` cannot be emitted. | **FAIL — `FIND-TASK-010-3`** |
| RFC 6749 authorization error redirect behavior | Duplicate non-binding request parameters return a local page after a valid client/redirect was available. | **FAIL — `FIND-TASK-010-4`** |
| Upstream provider denial completes the downstream authorization attempt | The callback wire requires `code`; a standard provider error is rejected before server-bound state recovery. | **FAIL — `FIND-TASK-010-5`** |
| RFC 6749 `client_secret_basic` interoperability | Secret verification is otherwise correct, but authentication-scheme matching is case-sensitive. | **FAIL — `FIND-TASK-010-6`** |
| Repository Rust documentation contract | Four new OAuth items lack required field/associated-type rustdoc or `# Errors`. | **FAIL — `FIND-TASK-010-7`** |
| Tenant isolation through `TenantConn` and RLS | Production capability types are correct, but the new active-refresh query duplicates tenant selection manually. | **FAIL — `FIND-TASK-010-8`** |
| Served machine-readable OAuth contract | Runtime accepts public `client_id` and confidential Basic authentication, but token/device/revoke OpenAPI operations omit them. | **FAIL — `FIND-TASK-010-9`** |
| RFC 8628 user-code attempt limiting in supported multi-replica topology | One server-local TCP-peer governor covers every auth route; proxies collapse clients and replicas bypass one another. | **FAIL — `FIND-TASK-010-10`** |
| Public refresh rotation, replay containment, and confidential non-rotation | Client-specific renewal exists, but every inactive CLI row is treated as theft and containment revokes unrelated principal sessions. | **FAIL — `FIND-TASK-010-11`** |
| Immediate connection lifecycle cutoff | Redemption rechecks the binding, but callback completion can mutate User/roles and commit a code or approval after deactivation. | **FAIL — `FIND-TASK-010-12`** |
| Canonical audit of successful login outcomes | Callback success with unchanged roles can commit a code or approval without a login-outcome audit row. | **FAIL — `FIND-TASK-010-13`** |
| Public-client rotation and confidential-client bounded non-rotation | The expected paths and database fields exist and recorded focused tests cover ordinary renewal. | PASS, subject to `FIND-TASK-010-11` |
| RFC 7009 revocation and 200 for unknown tokens | The form endpoint, client binding, chain revocation, and unknown-token behavior are present and covered by recorded focused tests. | PASS |
| Form-only requests, RFC response bodies, no-store, and `invalid_client` challenge | Common OAuth extraction and response owners implement these behaviors; recorded focused route tests are green. | PASS, subject to the wire findings above |
| RFC 8414 metadata | Static metadata publishes the implemented endpoints and recorded focused assertions are green. | PASS |
| Approved API-key exchange shape | RFC 8693 uses `subject_token_type=urn:wyrd:oauth:token-type:api_key`; `grant_type=wyrd_api_key` is rejected with no alias. | PASS |
| RFC 7523, delegation, API-key, and workload semantics remain with existing owners | The cumulative trace and recorded focused journeys show no alternate verifier, tenant selector, issuer, or token-content change. | PASS |
| Deleted protocols stay deleted | Private BFF routes, browser sessions, sealed completion, related migrations/queries, and narrowed rewrap columns are absent with no compatibility surface. | PASS |
| TASK-011 consumer boundary | UI/BFF failures caused only by deletion of `/internal/bff/v1` remain TASK-011 work and were not treated as TASK-010 findings. | PASS / out of scope |

## Independent review results

| Report | Result |
| --- | --- |
| `task-review-behavior.md` | FAIL |
| `task-review-invariants.md` | FAIL |
| `standards-review.md` | FAIL |
| `maintainer-review.md` | FAIL |
| `system-review.md` | FAIL |
| `domain-review-security-oauth.md` | FAIL |
| `domain-review-tenancy-audit.md` | FAIL |
| `domain-review-persistence-concurrency.md` | FAIL |
| `findings-validation.md` | COMPLETE — 13 retained findings |

## Follow-up decision

A fresh focused follow-up was required because discovery materially disagreed
about callback lifecycle/audit closure, the routed device-race proof, the
server-local governor, and mandatory rustdoc. `followup-review.md` returned
`RESOLVED`: it confirmed both callback findings, confirmed the focused device
proof gap, revised the governor to a gateway-boundary `DRIFT` finding, and
confirmed all four rustdoc omissions. It proposed no new finding.

## Validated finding ledger

| Finding | Status | Classification | Correction boundary |
| --- | --- | --- | --- |
| `FIND-TASK-010-1` | CONFIRMED | MISSING | Add the task-named `openid-client` 6.8.8 real-server proof without restoring the BFF channel. |
| `FIND-TASK-010-2` | CONFIRMED | MISSING | Add the focused pause/terminal/resume device regression proof at the existing device test owner. |
| `FIND-TASK-010-3` | CONFIRMED | INCORRECT | Preserve unsupported token-exchange audience classification and emit RFC 8693 `invalid_target`. |
| `FIND-TASK-010-4` | REVISED | INCORRECT | Validate the unique client/redirect first, then redirect non-binding authorize errors safely. |
| `FIND-TASK-010-5` | REVISED | MISSING | Model provider success-or-error callback input and consume the existing state before downstream refusal. |
| `FIND-TASK-010-6` | CONFIRMED | INCORRECT | Compare the HTTP Basic scheme case-insensitively in the existing client-auth owner. |
| `FIND-TASK-010-7` | CONFIRMED | VIOLATION | Add the four mandatory rustdoc elements; add no check or wrapper. |
| `FIND-TASK-010-8` | REVISED | DRIFT | Delete the duplicate tenant predicate/bind and rely on existing RLS. |
| `FIND-TASK-010-9` | CONFIRMED | INCORRECT | Publish complete `client_id` and standard Basic alternatives in served OpenAPI. |
| `FIND-TASK-010-10` | REVISED | DRIFT | Delete the all-auth replica-local governor and use the existing gateway's native limiter only for user-code verification. |
| `FIND-TASK-010-11` | REVISED | DRIFT | Classify only rotated predecessors as replay and contain through the existing rotation chain. |
| `FIND-TASK-010-12` | CONFIRMED | INCORRECT | Fence callback completion with the existing connection-slot lock and exact active-binding predicate. |
| `FIND-TASK-010-13` | CONFIRMED | REGRESSION | Append one canonical successful-login outcome in the callback transaction independently of role mutation. |

The exact source evidence, consequences, decision-complete corrections, and
focused closure proofs are preserved in `findings-validation.md`.

## Prior finding closure

`FIND-TASK-004-12` is source-closed but not acceptance-closed. The candidate
removes callback-time credential issuance and makes live device redemption one
locked transaction, but the routed deterministic interleaving proof is absent.
`FIND-TASK-010-2` is the sole retained closure item; there is no duplicate
device-state implementation finding.

## Verification limits

- Discovery independently ran `git diff --check`, two exact OAuth contract
  tests, and the tenant-isolation, pool-construction, clippy-allow, unwrap, and
  Python-format checks; all passed.
- The task records green focused identity filters, auth/server/SQL tests,
  `test:principals:integration`, `test:sql`, platform and CLI journeys,
  codegen, docs, client-tier, unwrap, format, lint, Python-lint, and TypeScript
  typecheck lanes. Reviewers inspected the mapped sources rather than treating
  that table alone as proof.
- Several green tests encode or omit the retained behavior, including
  principal-wide refresh containment, local authorize failure, retrying 429s,
  and incomplete OpenAPI authentication. They do not clear those findings.
- Full unfiltered identity and every-language journeys are intentionally not
  task-review requirements. They remain change-review work. No missing
  required reviewer was converted into a verification limit.

## Verdict

**FIX_REQUIRED**

Thirteen bounded corrections remain. They fit approved specification revision
11, reuse existing owners and conventional standards, and require neither a
specification revision nor TASK-011 compatibility work. The self-contained
remediation task is `TASK-010-R1-authorization-server-corrections.md`.
