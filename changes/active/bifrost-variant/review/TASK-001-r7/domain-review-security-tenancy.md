# TASK-001 r7 domain review — security, tenancy, and trust boundaries

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `6147cc617d81f2c03464043be698ab2565e9d745`
- Candidate tree: `7b7bb069ecbe8600e09cc8b6ea4e938709614718`
- Approved contract: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation evidence: `changes/active/bifrost-variant/review/TASK-001-r6/TASK-001-R6-close-variant-canonicality-and-proof-gaps.md`

The reviewed checkout matched the candidate commit and tree before and after this pass.

## Security Audit

### Critical

None.

### High

None.

### Medium

None.

### Low / Defense In Depth

None required by TASK-001 or its reachable security boundaries.

### Positive Controls

- Raw Arrow Variant values reach one server-side trust-boundary validator before acknowledgement; `tables::validate_declared_variants` recursively checks every declared Variant and delegates hostile bytes to `EncodedVariant::from_bytes` (`crates/vala/vala-bifrost-redux/src/tables/mod.rs:197-254,392-438`).
- `EncodedVariant::validate` applies the 8 MiB bound before walking bytes, uses an explicit-stack scan, rejects shared/overlapping object regions, duplicate resolved names, non-canonical decimals, and non-finite floats, and does not invoke upstream recursive validation after a depth violation (`crates/shared/wyrd-queue/src/variant.rs:242-285,932-1075`).
- The raw scanner exposes only bounded catalog detail (field, row, pointer, numeric class, depth, and size), not Variant payload bytes, credentials, object paths, or foreign tenant identity (`crates/shared/wyrd-queue/src/variant.rs:43-108`).
- Scribe binds the authenticated tenant to the principal before materialization and WAL work, while the gateway capture peer service is confined to the mutually authenticated peer listener, refuses the system tenant, and allowlists only `vala.gateway.calls` and `vala.traces.spans` (`crates/vala/vala-bifrost-redux/src/scribe/ingress.rs:41-75`; `crates/wyrd/wyrd-server/src/grpc/capture_peer.rs:1-10,45-78`).
- Gateway capture constructs its internal frame with the reserved capture principal and the same tenant in both principal and authenticated bindings; partial `resolved_model` values are refused by the shared whole-Struct validator before ACK, and the peer journey proves no rejected row becomes queryable (`crates/wyrd/wyrd-server/src/components/gateway/capture.rs:264-305`; `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs:77-116`; `crates/wyrd/wyrd-testing/tests/gateway/peer.rs:122-189,242-349`).
- Oracle authorizes every catalog-resolved table UID before provider or source work, applies the extra gateway-payload permission to whole columns and nested Variant expressions, and binds the authorized scope set into the distributed permission digest (`crates/vala/vala-bifrost-redux/src/oracle/mod.rs:3890-4011`).
- Published, hot, and staged scans share the footer-tenant proof before row-group decoding; failures become `QueryTenantInvariant` without returning a foreign identity and retain that catalog identity through worker/coordinator error propagation (`crates/vala/vala-bifrost-redux/src/oracle/exec.rs:1093-1169,1205-1260`; `crates/vala/vala-bifrost-redux/src/oracle/mod.rs:4240-4307`).
- Failed terminals carry the derive-backed catalog problem; the shared Rust collector rejects previously buffered batches, Python and TypeScript project that same owner, and uncatalogued execution failures stay the generic query-execution error (`crates/shared/wyrd-client/src/bifrost/query.rs:111-149,2047-2084,2561-2594`).
- Sensitive Variant/Struct classifications remain attached to the built-in schemas, including gateway payloads, verification summaries, Eval payloads, agent traces, and signal payloads; no R6 change creates a new read path or strips those declarations.
- The R6 range adds no dependency, credential, crypto, TLS, environment-variable, migration, SQL-construction, command-execution, redirect, callback, or external-network surface.

## Boundary coverage

| Boundary | Source and proof inspected | Result |
|---|---|---|
| Tenant derivation and Scribe admission | `scribe/ingress.rs`, `tables/mod.rs`, gateway capture frame construction | PASS |
| Footer tenant refusal | shared published/hot/staged footer loader, local and multi-pod refusal journeys | PASS |
| RBAC object and sensitive payload authorization | `authorize_resolved_tables`, `authorize_payload_columns`, scoped permission digest, gateway negative journey | PASS |
| Peer authentication and gateway table confinement | peer-only service mounting contract, mTLS journey, system-tenant and table allowlist | PASS |
| Hostile raw Variant resource safety | size-first borrowed validation, explicit-stack scan, canonical slots/names/numbers, renderer reuse | PASS |
| Sensitive Variant fields | built-in classifications and unchanged Oracle permission path | PASS |
| Distributed error identity | `QueryCatalogError`, Iceberg error mapping, worker tenant-refusal proof | PASS |
| No partial successful result | terminal-required protocol and bounded collection rejection after buffered rows | PASS |
| SDK error preservation | shared Rust error projection plus Rust/Python/TypeScript late-error journeys | PASS |
| Secrets and diagnostic exposure | changed logs/errors and public problem details | PASS |
| Supply chain and deployment | cumulative manifests plus dependency-free R6 range | PASS |

## Verification assessment and limits

The recorded R6 evidence covers the 68-test `wyrd-queue` library, the 847-test Postgres-backed `vala-bifrost-redux` library, focused Oracle and gateway tests, the two server journeys, the peer gateway journey, and format/lint/codegen/docs gates; this independent pass inspected those exact test owners and production call paths but did not rerun serialized Postgres, multi-pod, Python, TypeScript, or MCP journeys. Streaming APIs intentionally may deliver authorized batches before a failed terminal; the contract enforced here is that the stream raises the exact terminal error and no collecting SDK operation returns those batches as a successful partial result.

## Findings

No material security, RBAC, tenant-isolation, sensitive-data, hostile-input, distributed-error, or partial-success finding was identified.

## Overall result

**PASS** — candidate `6147cc617d81f2c03464043be698ab2565e9d745` at tree `7b7bb069ecbe8600e09cc8b6ea4e938709614718` satisfies the reviewed security, tenancy, and trust-boundary obligations of TASK-001.
