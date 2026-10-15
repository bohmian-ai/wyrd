# Security and tenancy domain review

## Subject and reviewed boundary

- Base: `ef4f6b0251e9d11c2712765f6db90545e40d590b`.
- Candidate: `4a6af5320ab8c236fbe640ff0dc5ddf544bdcde2`.
- Candidate tree: `036d647413d0acfd1afdebff2b7057b45ac00609`.
- Approved authority: `changes/active/verify-attribution/spec.md`, revision 3.
- Reviewed the original TASK-001, the prior verdict and R1 remediation task,
  the complete cumulative security-sensitive implementation, and the R1 and
  post-merge diffs affecting trace routing, authenticated identity, ingest
  links, timeline reads, SDK projections, and the outage proof.
- Governing authority: AGENTS.md tenant/server/contract rules,
  `architecture/wyrd-security-posture.md`,
  `architecture/references/domain/telemetry-observations.md`, SPEC r3
  `REQ-005` through `REQ-008`, and `INV-001`, `INV-002`, and `INV-004`.

## Boundary and authority coverage

| Boundary | Source and caller path inspected | Security result |
| --- | --- | --- |
| HTTP identity to tenant route | `http::router::request_span` -> `serve_request`; `AuthenticatedPrincipal::from_verified` -> `bind_tenant_route` | PASS: the route starts unbound and receives tenant and principal only from `VerifiedToken`; unauthenticated or failed authentication cannot stage a tenant copy. |
| gRPC ingest identity to tenant route | public gRPC request span -> Gate `insert_batch` -> `authenticate` -> `TokenVerifier::verify_on` -> `AuthContext::from_verified` | PASS: an unverified token payload selects only the expected verification tenant; the bound tenant and principal come from the successfully verified token. |
| Shared or attacker-chosen trace IDs | `TenantRoutes::{start,bind,end}` and `ServerSpanSink::stage` | PASS: routing is keyed by the server span's `(TraceId, SpanId)`, identity is held out of exported attributes, and the sink stages under the verified route tenant and principal. A shared inbound trace ID does not select a tenant. |
| Producer contexts | Gate `producer_links`, `link_remote_span`, server sampler, and tenant route opening | PASS: IDs and the sampled bit can affect tenant eligibility only after authentication; malformed IDs and flags are ignored; producer context cannot select tenant/principal, alter row admission, or change the operator sampler's server-random decision. `tracestate` and reserved flag bits remain excluded as required by R1/INV-004. |
| Tenant copy payload | `in_catalog`, `safe_attribute`, `tenant_copy`, and `OperatorExport` | PASS: tenant export is catalog- and attribute-allowlisted, exception payloads are reduced to type, error descriptions are removed, resources retain only service name, and the tenant remains out-of-band. No credential, query text, internal address, path, or storage-key field was found entering the tenant copy. |
| Timeline read | `Bifrost::trace_timeline` -> parameterized `Bifrost::sql` -> authenticated `/v1/query` -> `oracle_context`/Oracle authority | PASS: the caller supplies only a typed, nonzero trace ID bound as `$1`; authorization and tenant context are derived from the authenticated `Caller`, and the query executes through the existing tenant-qualified Bifrost authority. The one-hop link closure never bypasses that tenant authority. |
| Cross-tenant proof | `action_timeline::another_tenant_cannot_read_the_timeline` plus the routing tests named by TASK-001 | PASS from source and recorded pre-merge evidence: the owner reads rows and another tenant receives an empty timeline; current core routing/query source is unchanged by the two later merges. |
| R1 outage proof and credentials | `pg_verification_runtime::sustained_scribe_outage_caps_tenant_spans`, `traced_tenant`, authenticated HTTP/MCP probes, gRPC health, and test-only superuser assertions | PASS: requests and MCP use a tenant token through the normal verification path; the token is not logged or placed in assertions; superuser access is confined to the integration fixture for aggregate retained-row assertions and does not enter production code. |
| Post-merge SDK timeline calls | Python `Bifrost.trace_timeline` TypedDict projection; TypeScript generated `SpanRecord`; action-timeline callers | PASS by source: both remain thin projections of the shared Rust timeline call and add no tenant selector, alternate transport, or authorization path. The shape changes expose only the already-approved `SpanRecord` fields. |
| Post-merge stock-client auth used by gateway journeys | Python `HttpAuth` and TypeScript `httpAuth` callers | PASS for TASK-001: the journey points the stock client at `WyrdClient.server_url`, strips placeholder provider credentials, and sends the Wyrd token in the dedicated header. This is separately approved ambient-auth behavior and does not change timeline tenancy or trace routing. |

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None within the approved TASK-001 boundary. No speculative hardening is needed
to accept this task.

### Positive Controls

- Tenant and principal routing values originate only from verified credentials;
  trace IDs, links, headers, and request bodies cannot select them.
- A route remains inert until authentication binds it, and the reserved system
  tenant is explicitly excluded from tenant span export.
- Operator sampling for remote client traces uses a server-generated random
  trace ID, while `OperatorExport` excludes record-only tenant spans and clears
  client `tracestate`.
- The timeline SQL is fixed and parameterized, and it reuses the authenticated,
  tenant-qualified Bifrost query path rather than introducing a privileged read.
- Tenant telemetry is allowlist-projected before durable staging, and the
  outbox stamps the authenticated principal out of band.
- The R1 recovery test uses normal authenticated entry points and the existing
  role-separated cluster seam; it adds no production fault switch, credential
  bypass, or alternate data path.

## Material findings

No material security, RBAC, credential-exposure, injection, or tenant-isolation
finding was identified. The four prior findings are closed without weakening a
security boundary; the R1 changes to byte accounting, deduplication, test proof,
and rustdoc do not alter authorization or tenant selection.

## Verification limits

No Cargo or mise command was run in this parallel review. The focused R1 tests,
cross-tenant journey, and Python/TypeScript action-timeline journeys were not
rerun on candidate `4a6af5320`; therefore current runtime proof remains the
orchestrator's stated verification gap even though the inspected post-merge
source introduces no security or tenancy regression.

## Overall result

**PASS**
