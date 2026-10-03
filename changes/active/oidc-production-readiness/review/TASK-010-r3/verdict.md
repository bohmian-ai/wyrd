# TASK-010 R3 verdict

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `fe51df0af6f6852fa7a8bc7276f3ce1d31c86daa`
- Candidate: `1f4466a9ae482eb1311f6e5206484758bc272ad8`
- Approved specification:
  `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task:
  `changes/active/oidc-production-readiness/tasks/TASK-010-authorization-server-grants.md`
- R1 remediation:
  `changes/active/oidc-production-readiness/review/TASK-010-r1/TASK-010-R1-authorization-server-corrections.md`
- Current remediation authority:
  `changes/active/oidc-production-readiness/review/TASK-010-r2/lead-direction-FIND-TASK-010-10.md`

The candidate remained at the stated commit throughout discovery and
independent validation. The repository has no `.codegraph/` directory, so the
reviewers used the immutable Git range and repository source directly.
`FIND-TASK-010-1` remains routed to TASK-011 by its standing lead direction
and was not reopened. The current `FIND-TASK-010-10` lead direction supersedes
`TASK-010-R2-public-edge-device-admission.md`.

## Reconciled acceptance matrix

| Obligation | Implementation and verification evidence | Result |
| --- | --- | --- |
| Authorization code grant uses the standard confidential `wyrd-ui` client, exact redirect binding, S256 PKCE, a digest-only code expiring within 60 seconds, and single-use redemption | Shared authorize, callback, login-state, client-authentication, and issuance owners; focused callback, refusal, and principals evidence | PASS |
| The production `openid-client` interoperability proof is not duplicated in TASK-010 | Standing human direction assigns the production BFF journey to TASK-011 and integrated change review | PASS / ROUTED |
| Device authorization stores approval rather than a credential and issues exactly once from a still-live row at redemption | `CliLogins` and its tenant SQL owner; deterministic denial/expiry overlap, refusal, and exactly-once tests | PASS |
| RFC 8628 user-code attempt handling follows the superseding lead correction | Image-local NGINX limit and startup assertions are deleted; the operator is told to limit `POST /auth/device` per client address at the public ingress; no application limiter, edge manifest, setting, or state was added | PASS |
| Public-client refresh rotates and contains replay to its chain; confidential-client refresh does not rotate and respects its absolute lifetime | Shared refresh owner, RLS-backed lookups, family locking, chain revocation, and focused refresh/concurrency evidence | PASS |
| Connection replacement or deactivation fences callback and renewal effects | Final callback and issuance transactions recheck the exact active connection under the existing lock order; focused rollback and cutoff evidence | PASS |
| Token, device, refresh, revocation, token-exchange, JWT-bearer, and metadata surfaces retain the required RFC wire behavior | Shared form/error/client owners, generated contracts, served OpenAPI, codegen, principals, workload, platform, and CLI evidence | PASS |
| Tenant identity, principal resolution, role changes, issuance, and audit remain server-owned and transactional | `TenantConn`, forced RLS, shared issuance, and the canonical `vala.audit_staging` append path; tenant, SQL, callback-audit, and boundary evidence | PASS |
| Private BFF/session/completion protocols and browser-session persistence remain deleted without compatibility replacements | Cumulative route, module, migration, query, sealing, schema, and consumer inspection | PASS |
| No unsupported mechanism, check, file, setting, option, grant, or protocol remains in the task implementation | Deletion-first validation found only RFC-defined behavior, existing repository owners, native Postgres controls, installed protocol libraries, and conventional operator-ingress documentation | PASS |
| Repository standards, mandatory rustdoc, declaration parity, and narrow task verification are satisfied | Standards and maintainer audits plus current format, boundary, codegen, docs, startup, migration, and focused persistence/audit results | PASS |

## Independent review results

| Report | Result | Material conclusion |
| --- | --- | --- |
| `task-review-behavior.md` | PASS | All task behavior and negative-flow obligations pass; no proposed finding. |
| `task-review-invariants.md` | PASS | Producer-to-consumer and state invariants pass; no proposed finding. |
| `standards-review.md` | PASS | Applicable repository authorities and changed surfaces pass; no material standards finding. |
| `maintainer-review.md` | PASS | Changed owners, callers, tests, docs, and declarations are maintainable; observations are non-blocking only. |
| `system-review.md` | PASS | Runtime, dependency, restart, and recovery paths preserve the task boundaries; no material finding. |
| `domain-review-security-oauth.md` | PASS | OAuth and trust-boundary behavior passes; no material finding. |
| `domain-review-tenancy-audit.md` | PASS | Tenant routing, SQL capabilities, and canonical audit ownership pass; no material finding. |
| `domain-review-persistence-concurrency.md` | PASS | Durable state, races, locking, RLS, transactions, and migrations pass; no material finding. |
| `domain-review-deployment-gateway.md` | PASS | The lead-directed image deletion and operator documentation are exact; no material finding. |
| `findings-validation.md` | COMPLETE — EMPTY LEDGER | Independent source validation confirmed the empty finding union and rejected non-blocking concerns. |

## Follow-up decision

No `followup-rev` was needed. The discovery reports proposed no material
finding, did not materially conflict, exposed no unreviewed reachable path,
and traced the repeated remediation through the same owners. The fresh
Ponytail validator independently confirmed that assessment from the complete
diff, source, callers, sibling consumers, writers, authorities, and prior
finding history.

## Validated finding ledger

The independently validated ledger is empty. No `FIND-TASK-010-*` finding
remains for TASK-010, and no remediation task is required.

Placement, naming, structure, wording, optional-hardening, and unrelated-debt
observations were not promoted into findings. No edge manifest, application
limiter, forwarded-header parser, rate-limit option, or persistent admission
state is required or permitted by the controlling direction.

## Prior-finding closure

- `FIND-TASK-010-1` remains routed to TASK-011 and is not reopened.
- `FIND-TASK-010-10` is closed by the superseding lead direction: the invalid
  in-image limiter and its assertions are deleted, the operator-owned ingress
  responsibility is documented, and no replacement Wyrd mechanism was added.
- `FIND-TASK-010-2` through `-9` and `-11` through `-13` remain independently
  source-closed by the cumulative candidate and focused evidence recorded in
  the discovery and validation reports.

## Verification evidence and change-review boundary

The task and review records provide green narrow evidence for authorization,
callback, device, refresh, revocation, SQL and clean migrations, served
OpenAPI, generated contracts, client-tier and tenant boundaries, official
image startup, documentation, formatting, and linting. R3 reviewers also ran
the official-image startup lane, documentation and code-generation checks,
repository boundary checks, and focused persistence/concurrency and callback
audit tests successfully.

Per the controlling narrowest-lane direction, unfiltered identity,
every-language, TASK-011 BFF, and full integrated journeys run once at change
review. Their absence here is the planned evidence boundary, not a TASK-010
verification defect or a reason to require duplicate task-local machinery.

No required reviewer or report is missing, no source or caller trace is
incomplete, and no authority disagreement remains unresolved.

## Verdict

**PASS**
