# Whole-branch 08 domain review — Bifrost authentication and authorization

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate and reviewed HEAD: `eb9b2f69cb883fa508ed168f21cb868451e61b82`
- Approved authority: `changes/active/admin-principals/spec.md`, revision 12
- Remediation authority:
  `changes/active/admin-principals/review/whole-branch-07/TASK-001-008-R7-close-validated-findings.md`

CodeGraph was used before direct source inspection. The candidate remained
unchanged while this report was written.

## Reviewed boundary and authority coverage

| Boundary | Authority and source inspected | Result |
|---|---|---|
| HTTP authentication | Spec `REQ-012a`/`REQ-012b`, R7 `R7-AUTH-2`; `components/auth/token_extract.rs`, `principal_extractor.rs`, `caller_extractor.rs`, and auth middleware | PASS — the bearer is verified synchronously for signature, issuer, audience, expiry, and signed tenant; `Caller` is built from the verified `Principal` and delegation chain. The request path performs no auth-store read, role resolution, or positive-token-cache lookup. |
| gRPC ingest and query authentication | R7 `R7-AUTH-2`, `R7-AUTH-8`, `R7-AUTH-9`; `vala-bifrost-redux/src/gate/auth.rs`, `gate/mod.rs`, `wyrd-server/src/grpc/query.rs`, and server composition | PASS — both transports share the concrete crypto-only `TokenVerifier`; authentication happens once when the call or stream is established, and no expiry watcher or mid-stream reauthentication exists. |
| Token authority | Spec `REQ-012`, `REQ-017`, `AC-010`, `AC-012`; `wyrd-auth-verify::AccessTokenClaims`, `TokenVerifier::verify`, `PermissionSet`, and `Principal` | PASS — `permissions` is deserialized into `Principal.effective_permissions`; `roles` is retained only as metadata and is not consulted by the Bifrost authorization paths. |
| Query capability admission | Security posture authorization rules; `QueryAuthority::admit_capability`, `PermissionSet::covers_operation`, and `stream_query` | PASS — a scoped Bifrost query grant admits the operation without being mistaken for object-wide authority; absence of any query grant fails before Oracle dispatch. |
| Stable-table object authorization | R7 `R7-AUTH-3`; `bifrost-design.md` public-surface rules; `oracle::planner::pin_cut`, `authorize_resolved_tables`, `resolved_table_scope`, and permission subsumption | PASS — every prepared scan identity is tenant-bound and catalog-resolved to `{catalog, schema, table_uid}` before reader guards, provider registration, physical planning, admission charging, peer dispatch, or source IO; `All`, schema, and exact-table grants subsume only the expected targets, and one uncovered table refuses the whole query. |
| Distributed authorization binding | `bifrost-design.md`; `scoped_permission_digest`, analytical attempt construction, stage authority, and peer verification | PASS — the permission digest binds the admitted permission and the exact resolved table set, so a worker cannot widen the coordinator-approved objects. |
| Tenant isolation | Spec `REQ-014`, `AC-004`, `AC-010`; security posture, OLAP reference, Gate/Scribe context, Oracle context, catalog binding, object-key rules, and tenant tripwire | PASS — final tenant identity comes from verified claims, is carried unchanged through Gate/Oracle/Scribe, and remains protected by tenant-qualified catalog/storage state and the row tripwire. No request field selects or widens tenant scope. |
| Admission-time expiry and bounded completion | R7 `R7-AUTH-8`; `TokenVerifier`, gRPC query adapter, Oracle request deadline, and `an_expired_bearer_finishes_its_admitted_stream_but_opens_no_other` | PASS — the token must be unexpired when the stream opens; the admitted stream is governed by its existing Oracle deadline and is not reverified; the same expired bearer is refused at the next stream establishment. |
| Verification tier | AGENTS.md testing rules, testing-workflows reference, R7 focused proof, and appended implementation evidence | FAIL — see `BIFROST-R8-1`. |

## End-to-end flow traced

```text
HTTP X-Wyrd-Access-Token or gRPC x-wyrd-access-token
  -> concrete TokenVerifier verifies Ed25519/iss/aud/exp/tenant locally
  -> verified claims build Principal/Caller/AuthContext with PermissionSet
  -> QueryAuthority performs coarse bifrost_query:read capability admission
  -> Oracle resolves every scan to tenant-bound TableUid identity
  -> PermissionSet::contains checks exact table permissions with schema/All subsumption
  -> scoped permission + resolved table set enter the peer digest
  -> existing Oracle admission/deadline/tenant tripwire govern execution
```

No authentication database read, verifier cache, authorization epoch,
revocation checker, or runtime role resolution appears on this flow. Normal
catalog, audit, and query IO remains after authentication as intended.

## Material proposed finding

### `BIFROST-R8-1` — MISSING — the required scoped-permission proof does not traverse the real gRPC query boundary

- **Violated obligation:** The R7 focused proof requires a real Bifrost gRPC
  serving test in which an exact-table or schema-scoped token is allowed for
  its covered table and refused for another table. AGENTS.md makes the
  user-journey tier the primary cross-boundary proof.
- **Exact location:**
  `crates/wyrd/wyrd-testing/tests/bifrost/server/query.rs:928-1145` and
  `:1195-1270`; `crates/shared/wyrd-client/src/bifrost/query.rs:251-282`;
  `crates/wyrd/wyrd-server/src/grpc/query.rs:78-119`.
- **Evidence:** `tenant_scoped_roles_reach_only_their_granted_bifrost_tables`
  uses `Bifrost::query_only`, whose `QueryClient::query` posts to HTTP
  `/v1/query`; the separate gRPC expiry journey uses an `All` permission and
  proves admission-time expiry, but never presents an exact-table or
  schema-scoped token over `BifrostQueryService::query`.
- **Observable consequence:** The production gRPC adapter is structurally
  correct today, but the mandated cross-boundary proof would not catch a future
  gRPC-only loss, replacement, or widening of scoped JWT permissions while the
  HTTP matrix remains green.
- **Required testable correction:** Reuse the existing bound-server fixture,
  scoped clients/role seeding, registered `logs` and `traces` table UIDs, and
  generated `BifrostQueryServiceClient`; send the schema-scoped or exact-table
  bearer's raw access token in gRPC metadata, require its covered query to
  stream successfully, and require an uncovered query to fail with permission
  denied/query-forbidden before a response stream opens. Add no production
  abstraction or second authorization path.

## Verification limits

This was a static acceptance audit. I relied on the appended final-candidate
evidence for the recorded green lanes and exact selectors and did not rerun the
Postgres-backed journeys. The current source trace establishes the intended
runtime behavior; the retained finding is the missing mandatory gRPC scoped-
permission journey, not an observed authorization defect.

## Result

**FAIL** — the implementation flow is correct, but one explicit real-gRPC
closure proof required by the remediation packet is absent.
