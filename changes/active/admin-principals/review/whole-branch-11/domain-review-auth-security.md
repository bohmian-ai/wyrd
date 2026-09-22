# Auth, RBAC, tenancy, and audit domain review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 14
- Remediation tasks: `whole-branch-09/TASK-001-008-R9-close-validated-findings.md` and `whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
- Reviewed boundary: R9 CLI secret sourcing, credential identifiers, and delegation audit attribution; R10 token-exchange input, subject/actor construction, permission attenuation, directed invoke policy, tenant equality, audience admission, HTTP/gRPC caller projection, audit failure behavior, and SDK delegation ownership.

The five-minute stateless-JWT revocation window is approved behavior and was not treated as a finding.

## Authority and source coverage

| Boundary | Authority | Source and proof inspected | Result |
|---|---|---|---|
| CLI secrets | `INV-002`, R9 `FIND-admin-principals-R8-2` | `wyrd-cli/src/auth/{refresh,trusted_issuer}.rs`, shipped Clap scan, shared client construction, CLI focused evidence | PASS |
| Credential identifiers | R9 `FIND-admin-principals-R9-2` | UUID DTOs in `wyrd-spec`, Rust client revoke signatures, CLI edge parsing, OpenAPI/MCP evidence | PASS |
| RFC 8693 request and identities | `REQ-012c`, `INV-013a`, R10 RFC request/JWT criteria | `TokenRequest`, `DelegateToken::execute`, `policy_context`, `TenantGrant::into_access_grant`, issue/verify projections | PASS |
| Tenant isolation | `REQ-031`, `INV-003`, `INV-007` | subject-derived `TenantConn`, verification of both input tokens against that tenant, actor current-state issuance through the same transaction | PASS |
| Directed invoke policy | `REQ-012c`, R10 §2 | A-to-B `AuthzCheckContext`, fail-closed handling of every non-allow decision, production stub rejection | PASS, subject to Finding AUTH-1 availability |
| Permission attenuation | `REQ-012c`, `INV-013a`, R10 §3 | `PermissionSet::intersection`, issuance grant construction, wildcard/schema/table/disjoint tests, Bifrost journey | PASS |
| Audience binding | `REQ-012c`, R10 automatic verification | `TokenVerifier::verify_on`, HTTP router grouping, Bifrost gRPC gate, cross-surface journey | PASS |
| Audit attribution and failure | `REQ-037`, `INV-011`, R9 delegation finding, R10 audit criterion | allowed/denied exchange paths, actor credential attribution, canonical append, commit-before-return, Bifrost decision attribution, audit-failure tests | PASS |
| Production reachability | R10 outcome and client-helper acceptance, repository completion standard | token route preview guard, production validation, test-server defaults | FAIL — AUTH-1 |
| Shared SDK owner | `REQ-047`, R10 §4 | Rust `WyrdClient::on_behalf_of`, `AuthMiddleware`, Python and TypeScript projections and tests | PASS |
| Removed authority | `REQ-012c`, R10 removal criterion | live-source scan for `delegation:issue`, `Resource::Delegation`, `Action::Issue`, and `requested_subject` | PASS |

## Security Audit

### Critical

None.

### High

None.

### Medium

- **AUTH-1 — MISSING — the corrected delegation flow cannot run on a production API server.**
  - Violated obligation: R10's observable A-to-B delegation outcome and client-helper acceptance require the corrected `/auth/token` exchange to be usable by deployed clients; no approved authority limits it to development.
  - Location: `crates/wyrd/wyrd-server/src/components/auth/routes.rs:176-185`; `crates/wyrd/wyrd-server/src/state.rs:2420-2442`; `crates/wyrd/wyrd-testing/src/server.rs:566-575,4088-4127`.
  - Evidence: the token-exchange arm returns `preview_disabled()` unless `allow_preview` is true, while `production_validate` rejects every API-serving production state where `allow_preview` is true. The proving journey runs under `WyrdTestServer`, whose default explicitly enables preview auth and uses the development profile, so it cannot catch the production contradiction.
  - Observable consequence: every production Rust/Python/TypeScript `on_behalf_of` call is refused before either token is verified; the reviewed feature exists only in development/test despite being documented as the standard service-to-service flow.
  - Required testable correction: remove delegation's dependency on the broad preview-auth flag while preserving the existing typed endpoint, production verifier/policy/audit checks, and any unrelated preview grant guards; add one production-shaped route test with a non-stub policy/audit configuration proving token exchange is reachable while production validation still rejects actual stub or preview-only state.

### Low / Defense In Depth

None.

### Positive Controls

- Subject and actor tokens are both cryptographically verified against the same tenant, and the actor's active principal plus current grants are re-read through the existing `TenantTokenIssuer` transaction.
- The JWT has the correct RFC 8693 identity shape: A remains top-level `sub`, B is outer `act.sub`, earlier actors preserve order, and `act` is not consulted as authority.
- Bifrost-audience tokens are refused by general Wyrd and MCP admission, while HTTP Bifrost/query and the gRPC gate share the same verifier and preserve the verified actor chain for audit.
- Allowed and denied delegation decisions use the canonical audit append; the successful exchange is recorded under A, names B and B's credential, and no token is returned until the transaction commits.
- R9 removes secret-valued CLI arguments from the shipped tree, keeps secret-bearing values redacted, and preserves UUID credential IDs through contracts and clients.
- `delegation:issue`, its runtime-admin coupling, and `requested_subject` have no live production-code residue.

## Verification limits

- This was a static, review-only audit; no lanes were rerun.
- The supplied evidence demonstrates the correct A/B claim order and several malformed inputs, but it does not contain a focused policy that allows A-to-B and rejects the same valid tokens reversed as B-to-A. The implementation does pass subject and actor to the policy in the correct directed positions, so this is a proof gap for the orchestrator/task reviewer rather than a separate security finding here.
- The approved five-minute stateless revocation behavior was intentionally excluded as instructed.
- Dependency/tooling and non-auth data-plane changes were outside this domain review.

## Overall result

**FAIL** — one bounded material finding remains: `AUTH-1`.
