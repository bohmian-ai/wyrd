# Task 002 verdict

**SPEC_REVISION_REQUIRED**

Task 002 requires conventional configured GitHub Enterprise Server support and verified tenant association. The candidate sends PKCE for every GitHub App authorization and relies on provider enforcement to bind the returned code to the Wyrd transaction. GitHub’s published App feature definition enables that enforcement for GitHub.com and Enterprise Cloud and excludes Enterprise Server. On Enterprise Server’s documented base flow, a tenant writer with their own valid Wyrd transaction could submit a stolen, unused code issued to the same deployment App and callback. The tenant and browser checks authenticate the Wyrd transaction, while the provider token obtained from the substituted code supplies the installation evidence. This can associate an otherwise unclaimed installation with the wrong tenant. The approved packet needs an explicit Enterprise Server support and authorization-code binding decision. Live Enterprise Server behavior remains unverified. [Focused protocol validation](followup-pkce-validation.md) · [Independent findings validation](findings-validation.md)

The remaining findings have correction boundaries within the existing spec. The spec verdict precedes a remediation task because the provider support and security decision is still open.

## Immutable subject

| Item | Reviewed value |
|---|---|
| Base | `395274d58bf76a2075f245ca3a5c1bd044925027`, parent of the first task obligations commit |
| Candidate | `3dd1555a0f40117ee512ecaaff1f5b51845ef3b6` |
| Range | `git diff 395274d58...3dd1555a0` |
| Approved authority | [Spec revision 4](../../spec.md), [contracts](../../contracts.md) and [App design](../../design/approved-runtime-and-github.md). The approved amendment is `46af20f00` inside the range. |
| Task | [002 GitHub App connection](../../tasks/002-github-app-connection.md) |
| Round | First acceptance review of the complete range. There is no prior task verdict, remediation, carried-forward closure, or convergence assessment. |

## Independent review and coverage

The [scope](scope.md) records the owners, producers, consumers and required specialists. Independent validation confirmed that the selected roles cover the task’s material boundaries. The unchanged queue has no GitHub-backed work before task 006. Existing audit staging remains on the shared permission path. No changed Bifrost storage, model execution or UI surface required a separate specialist.

| Role | Result and report |
|---|---|
| Behavior | FAIL · [Full acceptance and non-goals](task-review-behavior.md) |
| Lifecycle invariants | FAIL · [Browser, slot and token authority](task-review-invariants.md) |
| Repository standards | FAIL · [Cross-tier and nested authority coverage](standards-review.md) |
| Maintainability | FAIL · [Owner, reuse and documentation coverage](maintainer-review.md) |
| System resilience | FAIL · [Deployment and recovery](system-review.md) |
| Security | FAIL · [Credentials, authorization and trust](domain-review-security.md) |
| PostgreSQL | FAIL · [Tenant isolation, persistence and concurrency](domain-review-postgres.md) |
| Focused follow-up | RESOLVED · [Webhook and association overlap](followup-review.md) |
| Separate validation of new PKCE claim | REVISED · [Enterprise Server protocol evidence and consequence](followup-pkce-validation.md) |
| Independent findings validation | SPEC_REVISION_REQUIRED · [Source validation, applicability, correction burden and proof](findings-validation.md) |

The [claim comparison](claim-comparison.md) groups overlapping reports by cause. The webhook follow-up narrowed two implementation findings to the existing slot owner. The separately assigned PKCE reviewer validated the protocol claim raised during independent validation. The validator then reconciled it against the original scope and security authority.

## Validated findings

The [independent ledger](findings-validation.md#validated-ledger) holds exact locations, source traces, affected callers, correction boundaries and closure proof. FIND-002-16 requires a specification decision.

| Finding | Validated disposition |
|---|---|
| FIND-002-1 | REVISED — INCORRECT — An old slot can hand out a token after committed revocation |
| FIND-002-2 | REVISED — INCORRECT — Stored GitHub identity is lost at consumption |
| FIND-002-3 | CONFIRMED — INCORRECT — Startup accepts a public key as a signing key |
| FIND-002-4 | CONFIRMED — INCORRECT — OAuth web exchange negotiates the wrong JSON media type |
| FIND-002-5 | REVISED — INCORRECT — Out-of-order suspension events can strand active access |
| FIND-002-6 | CONFIRMED — INCORRECT — A routed removal can miss a concurrent verified selection |
| FIND-002-7 | CONFIRMED — INCORRECT — Production screening rejects a configured private GHES |
| FIND-002-8 | CONFIRMED — VIOLATION — Setup gives a browser binding credential without no-store |
| FIND-002-9 | CONFIRMED — DRIFT — Local HMAC duplicates an installed primitive |
| FIND-002-10 | CONFIRMED — VIOLATION — Changed Rust items lack required or accurate documentation |
| FIND-002-11 | CONFIRMED — VIOLATION — Selected SQL tests report green without PostgreSQL |
| FIND-002-12 | CONFIRMED — VIOLATION — Real-server journey placement bypasses required source markers |
| FIND-002-13 | CONFIRMED — VIOLATION — The published setup instructions are invalid shell and lack connected proof |
| FIND-002-14 | CONFIRMED — VIOLATION — Changes to clear failed tests lack independent diagnoses |
| FIND-002-15 | REVISED — VIOLATION — The proof record lacks some required exact named commands |
| FIND-002-16 | REVISED — INCORRECT — GitHub Enterprise Server does not supply the required authorization-code binding |

## Reconciled acceptance matrix

Abbreviations are as in the task and behavior review. These dispositions update each behavior row and combine only rows with common evidence. Positive tests assert specific behavior under the mock or repository infrastructure. A PASS may coexist with an adjacent mandatory repo correction.

| Obligation | Candidate evidence and verification | Disposition |
|---|---|---|
| O1 | CFG :6819–6842 and :7006–7044 validate env, file, TOML and redaction. Four focused config cases passed. | PASS |
| O2 | RJ :559–573 refuses GitHub without config and successfully reads Cards. PY and TS also assert typed unavailability. | PASS |
| O3, O16 | CFG tests cover partial and malformed config but public PEM is parseable and boot does not sign. | FAIL FIND-002-3 |
| O4 | CFG custom/default origin and RJ host evidence cover unchanged mock config. Saved identity, production private address selection and the authorization capability for supported Enterprise Server hosts fail. | FAIL FIND-002-2, FIND-002-7, FIND-002-16 |
| O5 | Served OpenAPI asserts five paths, typed schemas and webhook security. SRV :218–238 exercises the tenant API. | PASS |
| O6 | RJ :164–177 and SQL :607–638,683 cover committed receipt, TTL, browser, actor and tenant. Current setup and consume omit the saved provider identity check. Enterprise Server cannot bind its authorization code to that state under the documented App grant. | FAIL FIND-002-2, FIND-002-16 |
| O7, O8 | RJ :164–199,227–238 completes both local paths. Both call the exchange whose mock always returns JSON. | FAIL FIND-002-4 |
| O9 | RJ :174–177 observes not connected after setup binds the hint. | PASS |
| O10 | RJ :193–199,357,498 checks provider installation, repository intersection and negative cases. OAuth negotiation, concurrent acknowledged removal and Enterprise Server code authority fail. | FAIL FIND-002-4, FIND-002-6, FIND-002-16 |
| O11–O14 | RJ :209–214,298,316,368 and SQL :616–654 show spoof refusal, consumption, browser/actor and tenant failures. PY :118 and TS :116 also reject a foreign principal. | PASS |
| O15, O15a | Migration enforces both unique constraints. SQL :317–381 commits exactly one winner under both contention shapes. RJ :404–411,439 tests conflict and same-installation refresh. | PASS |
| O15b–O15d | SQL :560–574,661 and RJ :386–398 show separate OIDC/GitHub consume. SQL :183,195 and RJ :427 show old callback refusal after release/reconnect. SQL :425,450 and :236 establish force-RLS flags and row isolation. | PASS |
| O15e | OIDC trust table and predicates retain their behavior. The exact real-provider runs prove tenant login, refusal and candidate connection test sign-in. The in-process browser callback asserts session creation and audit. | PASS |
| O15f | SQL :248–269 and RJ :492–493 establish genuine suspension keeps the unique claim. Wrong ordering of suspension is covered in O21. | PASS |
| O17 | CFG redaction, SRV :199–201 durable-row check and RJ :240–250 protect key/token disclosure. Setup's separate binding credential cache rule fails. | PASS for the promised key/token redaction. Repo gate FAIL FIND-002-8. |
| O18 | The typed view exposes only the approved shape, but can project the wrong host's association. | FAIL FIND-002-2 |
| O19 | SRV :132–178 asserts scope and sequential refusal. Old host and post-provider stale decisions remain reachable. | FAIL FIND-002-1, FIND-002-2 |
| O20 | SRV :133–143,296 asserts one repository and exact three permissions. :201 verifies no stored token. Callback drops the user token at service :495. | PASS for scope/transience. Authority is O19. |
| O21 | Bad signature is rejected before SQL, and RJ :478–536 proves ordered lifecycle updates. Reordering and the routed removal's callback race fail. | FAIL FIND-002-5, FIND-002-6 |
| O22, O23 | SRV :161–178,281 and RJ :444 prove sequential refusals and claim reuse. A concurrent mint can survive committed access loss. | FAIL FIND-002-1, FIND-002-5, FIND-002-6 as applicable. |
| O24 | SRV :283–293 proves ordinary pending work survives. No current queued execution takes GitHub repository identity. Task 006 adds that input to the existing owner. | PASS for the current queue scope. Source-IO failure is O22/O23. |
| O25 | RJ :253–255 preserves Card count and SRV :203 asserts zero bindings. | PASS |
| O26–O28 | Guide specifies configured permissions/events and served URLs. Required architecture sections exist. SRV :271–279 proves allowed/denied audit decisions and the writer permission is shared with identity connections. | PASS for these obligations. Implementation deficiencies and setup proof are separate. |
| O29 | Shared Rust handle and thin projections, served HTTP plus Rust/Python/TS, CLI and MCP journey assertions. MCP :95,166–185 proves discovery and scoped writes. TypeScript required gates passed. | PASS for surfaces. Repo gate FAIL FIND-002-12 for test placement. |
| O30 | CLI journey proves verbs and typed JSON through the server. Published setup shell snippets and local example proof are absent/invalid. | FAIL FIND-002-13 |
| O31 | SQL conditional write assertions pass under the managed database. The test-local env guard can bypass them, and prior no-overlap proof expects the unsafe removal behavior. | FAIL FIND-002-6, FIND-002-11 |
| O32–O33 | Migration adds one table with forced RLS and separated OIDC state. SQL :436–479 proves pinned search path, narrow resolution, restoration and no ordinary cross-tenant read. | PASS |
| O34 | Begin, association and disconnect responses follow commit. RJ :193–199,225,444 observes durable committed state. The missing credential fence is captured by O23. | PASS for commit ordering. |
| O35 | App signing, webhooks and tenant policy have the approved owner, with no GitHub library or old pin. Installed HMAC reuse remains an explicit repository issue. | PASS for ownership. Repo gate FAIL FIND-002-9. |
| O36 | Closed request shape, one new table, no UI, Cards/bindings, PR admission or source mutation, no new permission category or operator credentials supplied by tenants. | PASS |
| O37 | Corrected fixture/test assertions have no independent diagnosis report in the immutable record. | FAIL FIND-002-14 |
| Repository proof and rustdoc | Changed-item docs are explicitly mandatory. Passing narrowed commands, TypeScript gates and OIDC proof close their observed coverage. Exact named proof still needs executable selectors as described above. | FAIL FIND-002-10, FIND-002-15 |


## Verification limits

[Independent verification](verification.md) records the exact commands and outcomes. Formatting, tenant isolation, focused GitHub SQL/server/Rust SDK/CLI/MCP/OpenAPI tests, three OIDC journeys with the repository identity providers, and TypeScript format/lint/type checks passed. The task’s evidence supplies the Python and TypeScript journey runs and the remaining broad checks. The complete identity lane and live GitHub Enterprise Server were outside the rerun. Existing passing assertions do not exercise the interleavings in the validated ledger. The historical absence of failing-first evidence remains recorded in the task.

## Subsequent human-directed resolution

After this immutable revision 4 review, the user approved the conventional
GitHub.com scope recorded in spec revision 5. The decision is documented in
[scope-resolution.md](scope-resolution.md), and
[TASK-002-R1](002-R1-conventional-github-app.md) supplies the correction packet.
This historical verdict does not establish implementation closure under the
revised scope.
