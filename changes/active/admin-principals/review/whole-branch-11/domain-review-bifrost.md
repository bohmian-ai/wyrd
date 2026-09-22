# Bifrost domain review — cumulative R9 + R10

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `41e60be61958c92f562fceb5b7a03f40f611bbc8`
- Approved specification: `changes/active/admin-principals/spec.md`, revision 14
- Remediation tasks:
  - `changes/active/admin-principals/review/whole-branch-09/TASK-001-008-R9-close-validated-findings.md`
  - `changes/active/admin-principals/review/whole-branch-10/TASK-001-008-R10-correct-rfc8693-delegation.md`
- Accepted limit excluded from review: an already-issued tenant JWT may retain
  its snapshot authority until the five-minute expiry.

The candidate was `41e60be61958c92f562fceb5b7a03f40f611bbc8`
before and after this review.

## Reviewed boundary

This review traced a delegated token from issuance into every public Bifrost
admission path: the `/v1/bifrost/*` and `/v1/query` HTTP router group, the
public gRPC query adapter, the native gRPC ingest Gate, caller construction,
query object authorization, record-write RBAC, and the canonical query and
write audit projections. It also inspected the required
`query::service_b_acts_for_service_a_with_only_a_table_authority` journey and
the R9 successful-exchange credential-attribution behavior retained by R10.

## Authority and source coverage

| Boundary | Governing authority | Source inspected | Result |
|---|---|---|---|
| RFC 8693 subject/actor authority | Spec `REQ-012c`, `INV-013a`, `AC-020`; R10 §§1–3 | `wyrd-auth/src/exchange_api_key.rs`, `wyrd-auth/src/issuance.rs`, `wyrd-auth-issue`, `wyrd-auth-verify` | PASS |
| Audience confinement | R10 outcome, acceptance row “Automatic request verification”; security posture “Delegation and federation” | `wyrd-auth-verify/src/lib.rs:340-366`, `wyrd-server/src/http/router.rs:49-92`, `wyrd-server/src/http/middleware/authenticate.rs`, `vala-bifrost-redux/src/gate/auth.rs:112-126`, `wyrd-server/src/grpc/query.rs:83-99` | PASS |
| Subject authority and object-scoped query authorization | Spec `AC-020`; Bifrost design “Query authorization”; permission model delegated-request rule | `wyrd-server/src/query/service.rs`, `vala-bifrost-redux/src/oracle/mod.rs`, `wyrd-runtime/src/permission.rs` | PASS |
| Native ingest write authorization | R10 §3 (“audit and policy retain B and the full verified actor chain”); AGENTS §§2, 9, 11 | `vala-bifrost-redux/src/gate/mod.rs:437-459, 850-896`, `wyrd-server/src/bifrost/gate_audit.rs:28-64` | FAIL (`BIFROST-01`) |
| Query and write audit attribution | AGENTS canonical-audit rule; Bifrost design “Read audit”; R10 audit acceptance | `wyrd-server/src/audit/mod.rs`, `wyrd-server/src/oracle/query_audit.rs`, `wyrd-server/src/bifrost/gate_audit.rs`, `wyrd-spec/src/vala/audit_detail.rs` | FAIL (`BIFROST-01`) |
| Required end-to-end proof | Spec `AC-020`; R10 “Primary user journey”; AGENTS test taxonomy | `wyrd-testing/tests/bifrost/server/query.rs:1142-1321`; R10 evidence table | FAIL (`BIFROST-01`) |
| R9 actor credential attribution retained after R10 | R9 `FIND-admin-principals-R9-1`; R10 audit acceptance | `wyrd-auth/src/issuance.rs:557-609`, `wyrd-auth/src/exchange_api_key.rs:270-425` | PASS |

## Trace result

The audience boundary is coherent. `TokenVerifier::verify_on` accepts the
general `wyrd` audience plus the named surface; the HTTP router applies the
Bifrost surface only to the Bifrost and query routers while all other `/v1`
and MCP routes use the Wyrd surface. Both public gRPC query and native ingest
call the same Gate authentication function, which verifies on the Bifrost
surface and constructs the effective principal from the signed token without a
database read.

The authorization identity is also correct. A delegated token verifies to A
as the effective principal, its attenuated permission set drives both Oracle
read authorization and Gate record-write RBAC, and B remains in the verified
delegation chain. Oracle carries that chain into read decisions and security
violations. The successful R9/R10 exchange audit remains attributed to A,
names B, attaches B's credential when present, and leaves the delegated JWT
without a credential id.

The one broken seam is the production Gate write-audit adapter: it receives
the verified chain in `AuthContext` but discards it when constructing the
audit event.

## Proposed finding

### `BIFROST-01` — delegated native-ingest decisions discard the actor chain

- Classification: **INCORRECT**
- Violated obligation: R10 §3 requires Bifrost audit to retain B and the full
  verified actor chain; the repository audit rules require the authorization
  decision to remain attributable through the canonical audit path.
- Exact location:
  - `crates/wyrd/wyrd-server/src/bifrost/gate_audit.rs:42-52`
  - proof gap at
    `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:1196-1202,1242-1287,1289-1321`
- Evidence: `gate::auth::authenticate` preserves
  `verified_token.delegation_chain` in `AuthContext`, and Gate passes that
  complete context to `append_write_decision`; `PostgresGateAudit` then builds
  `AuditEvent::new(...)` without `AuditDetail::DelegationAttribution`, unlike
  the existing shared HTTP audit builder and Oracle read audit. A delegated
  native-ingest allow or deny therefore commits under A with no durable record
  that B performed the operation.
- Reachability: `BifrostGrpcTransport` sends the delegated client's bearer to
  `BifrostIngestService::insert_batch`; Gate authenticates it, evaluates
  `bifrost_record:write`, and calls this production audit adapter for both
  allow and deny outcomes.
- Why existing proof falls short: the required journey gives B
  `bifrost_table:write` and tests an HTTP DDL registration denial. It never
  opens the delegated client's gRPC ingest path, never exercises the changed
  Bifrost-audience Gate acceptance, and queries only exchange/read audit rows.
  Thus it passes while the native write decision loses B.
- Observable consequence: retained audit can show that A attempted or
  performed a Bifrost record write but cannot identify B as the acting service,
  breaking delegated-operation accountability.
- Testable correction: reuse the existing
  `AuditDetail::DelegationAttribution` and
  `wyrd_runtime::audit_delegation_chain` projection in
  `PostgresGateAudit`; attach it only when the verified chain is non-empty so
  direct-call audit bytes remain unchanged. Revise the existing primary
  journey rather than adding a harness: give B the existing
  `bifrost_record:write` capability in addition to the shared table read,
  send a real batch through a `Bifrost` built from the delegated client, prove
  the write is rejected before effect because A lacks that permission, prove
  B's direct client can write the same batch, and assert the denied Gate audit
  row is attributed to A with B in its chain. This single scenario also proves
  that a `bifrost`-audience delegated token reaches the gRPC Gate.

## Verification coverage and limits

The supplied evidence reports the exact primary journey as selecting one test
and passing, the Bifrost server journey and integration lanes passing, the
focused verifier audience tests passing, and the full required format, lint,
boundary, schema, docs, and whitespace lanes passing. Source inspection agrees
with the HTTP confinement, local verification, read attenuation, and read
audit claims. No supplied test exercises a delegated `bifrost`-audience token
through native gRPC ingest or checks the resulting write-decision actor chain;
that missing proof is part of `BIFROST-01`, not an unrelated verification
note. Strict rustdoc failure in pre-existing `vala-bifrost-redux` documentation
is outside this finding.

## Overall result

**FAIL** — the audience and authorization flow is correct, but delegated
native-ingest authorization decisions are not durably attributable to their
actor, and the required journey currently avoids the path that exposes the
defect.
