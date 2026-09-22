# Bifrost domain review — cumulative admin-principals through R11

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14
- Reviewed remediation:
  - `changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
  - `changes/active/admin-principals/review/whole-branch-11/TASK-001-008-R11-close-r9-r10-findings.md`
- Accepted limit excluded from review: an already-issued tenant JWT may retain
  its snapshot authority until its five-minute expiry.
- The user explicitly authorized ancillary changes needed to make the full
  repository gate execute real tests and pass; those changes are not treated as
  Bifrost scope drift.

The candidate was `261168087376095fa5ad9d66946e755f3baa8fe4` when this
review began and when this report was written.

## Reviewed boundary

This review traced the reachable delegated-authentication flow from
`WyrdClient::on_behalf_of` into the Rust, Python, and TypeScript Bifrost
facades, the `/v1/bifrost/*` and `/v1/query` HTTP surfaces, public Bifrost gRPC
query admission, native gRPC ingest Gate admission, Oracle object
authorization, and canonical read/write audit attribution. It also checked
that direct Service-B behavior and zero-argument environment construction are
preserved. The accepted five-minute revocation window was not reconsidered.

## Authority and source coverage

| Boundary | Governing authority | Source and proof inspected | Result |
|---|---|---|---|
| RFC 8693 subject/actor and permission attenuation | Spec `REQ-012c`, `INV-013a`, `AC-020`; R10 delegation task | `wyrd-auth/src/issuance.rs`; `wyrd-runtime/src/permission.rs`; verifier claim projection; primary delegation journey | PASS |
| Bifrost audience confinement | Spec `REQ-012a`, `REQ-012c`, `AC-020`; R10 automatic-verification acceptance | `wyrd-auth-verify/src/lib.rs`; `wyrd-server/src/http/router.rs`; `http/middleware/authenticate.rs`; `vala-bifrost-redux/src/gate/auth.rs`; `wyrd-server/src/grpc/query.rs` | PASS |
| HTTP and gRPC caller construction | AGENTS server and tenancy rules; architecture permission-check authority | `components/auth/{principal_extractor,caller_extractor}.rs`; `gate/auth.rs`; `grpc/query.rs` | PASS |
| Oracle query authorization and attribution | Bifrost design query authorization and read-audit rules; spec `AC-020` | `query/service.rs`; `vala-bifrost-redux/src/oracle/mod.rs`; `wyrd-server/src/oracle/query_audit.rs`; primary and forwarded-query journeys | PASS |
| Gate record-write authorization and attribution | AGENTS canonical-audit rule; R11 `FIND-admin-principals-R11-5` | `vala-bifrost-redux/src/gate/mod.rs`; `wyrd-server/src/bifrost/gate_audit.rs`; primary delegation journey | PASS |
| Shared Bifrost facade composition | AGENTS shared-client ownership; spec `REQ-047`; R11 `FIND-admin-principals-R11-4` | `wyrd-client` Bifrost facade/scope; Python Bifrost and WyrdClient projections; TypeScript public/native projections; Python/TypeScript integration journeys | PASS |
| Direct behavior preservation | R11 acceptance for optional-client composition | Rust primary journey; Python delegated-client journey; TypeScript delegated-client journey including environment-only construction and conflicting-input refusal | PASS |

## Trace result

`WyrdClient::on_behalf_of` reuses B's existing authentication state, exchanges
A's access token as `subject_token` with B as `actor_token`, and returns another
shared Rust client whose delegated credential supplies both HTTP and gRPC
bearers. Issuance keeps A as the top-level principal, appends B as the outer
actor, and uses `PermissionSet::intersection` so wildcard, schema, exact-table,
and disjoint authority all narrow rather than amplify.

`TokenVerifier::verify_on` checks signature, issuer, expiry, tenant, and the
surface audience without a database lookup. General Wyrd routes accept only
`aud=wyrd`; the `/v1/bifrost/*` and `/v1/query` group plus Bifrost gRPC
adapters verify on the Bifrost surface, which accepts `aud=wyrd` or
`aud=bifrost`. The verified principal is A and its verified initiator-first
actor chain is carried into `Caller` or Gate `AuthContext`; neither HTTP nor
gRPC reconstructs authority from request data.

Oracle performs coarse route admission and then authorizes the catalog-resolved
table identities against A's attenuated permissions. Its read-decision detail
retains the verified actor chain for both local and forwarded execution. Gate
checks `bifrost_record:write` against the same effective A principal, commits
allowed and denied decisions before returning, and
`PostgresGateAudit` now attaches `DelegationAttribution` when B is present while
leaving direct-B events unchanged.

Rust already composes `Bifrost` from `&WyrdClient`. Python's optional
`client=` and TypeScript's optional `{ client }` pass the wrapped Rust client to
that owner; neither binding implements token exchange, caching, retry, headers,
or Bifrost transport independently. Omitting the client retains existing
environment/config resolution, and supplying both a client and transport or
credential options returns the stable validation error.

## Prior-finding closure

| Finding | Closure evidence | Result |
|---|---|---|
| `FIND-admin-principals-R11-4` | Python `Bifrost(client=delegated)` and TypeScript `Bifrost.connect({ client: delegated })` both reach the shared Rust facade; their integration journeys prove delegated read, denied delegated write, and successful direct-B write. TypeScript additionally proves environment-only construction and conflict refusal; Python separately proves conflict refusal. | CLOSED |
| `FIND-admin-principals-R11-5` | `PostgresGateAudit::append_write_decision` projects a non-empty verified chain through `AuditDetail::DelegationAttribution`; the primary Rust journey drives the delegated token through native gRPC ingest, proves denial before effect under A, asserts B in the denial audit, then proves direct B writes with no delegation detail. | CLOSED |

## Findings

No material Bifrost finding remains. The reviewed paths are reachable, satisfy
the approved delegation behavior, reuse the existing shared owners, and do not
introduce a second client, verifier, authorization, audit, or Bifrost
architecture.

## Verification coverage and limits

The R11 evidence records the exact primary Rust delegation journey as one
selected passing test, the Python delegated-client scenarios as two passing
focused tests, and the TypeScript delegated-client scenario as one passing
focused test. It also records all nine `test:bifrost` lanes, the complete
Python and TypeScript integration suites, `test:wyrd`, `test:sql`, format,
lints, client-tier/PyO3/tenant-isolation checks, code generation, and docs as
passing. The later candidate reports a green full `mise run gate`, with the
formerly vacuous environment-gated tests made executable and the identity
suite run through its environment-owning lane.

There is no separate delegated-token end-to-end test for the public Bifrost
gRPC query adapter. This is not a retained finding: the adapter calls the same
`gate::auth::authenticate` Bifrost verifier used by the journey-proven native
ingest path, copies the same principal and actor chain into the same `Caller`,
and dispatches through the same journey-proven `query::service` and Oracle
authorization path. Requiring another journey would duplicate proof without
closing a distinct reachable seam.

## Overall result

**PASS** — delegated and direct Bifrost authentication, authorization,
audience confinement, subject/actor attribution, permission narrowing, and
cross-language facade composition satisfy the approved task, and no reachable
task-required Bifrost problem remains.
