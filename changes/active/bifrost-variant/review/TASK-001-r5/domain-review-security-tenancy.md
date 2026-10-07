# TASK-001 r5 Security, RBAC, and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `0e37748f3a27d3bcec4713e6210e97328e045886`
- Candidate tree: `f2a42aafa72ea842fe8427a5dd724fad0b5c3c19`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 12
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Remediation under review: `changes/active/bifrost-variant/review/TASK-001-r4/TASK-001-R4-close-final-variant-contract-gaps.md`

The candidate and tree matched the requested immutable subject before and
after this review.

## Reviewed Boundary

This review covered the cumulative TASK-001 diff and the r4 remediation at the
security-sensitive boundaries only:

- public query and write RBAC before protected work;
- object-scoped query authorization and the additional gateway-payload gate;
- tenant identity from authenticated principals through catalog bindings,
  Scribe frames, Oracle cuts, peer claims, assignments, footer checks, and
  worker failures;
- schema, catalog, provider, and distributed execution paths that could widen
  authority or expose sensitive metadata;
- malformed Variant and schema refusal before shard dispatch, WAL append, ACK,
  or durable rows;
- late local and worker error identity, public problem projection, and
  no-partial-result behavior;
- gateway capture redaction, sensitive-column classification, audit hash
  preservation, and the Python RBAC regression assertion.

TASK-002 authoring and TASK-003 shredding/leaf-pushdown behavior remain outside
this task and were not treated as missing security work.

## Authority and Source Coverage

| Boundary | Authority | Source and consumer coverage | Result |
|---|---|---|---|
| Identity, authorization, and tenancy | `AGENTS.md`; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-security-posture.md` | Public query/write services, Gate, `AuthorizedQueryContext`, catalog binding, Oracle planner and peer authority | PASS |
| Bifrost query and ingest trust boundaries | `architecture/bifrost-design.md`; `architecture/references/domain/olap-serving.md` | Oracle planning/provider/worker paths; Scribe admission, decode, preprocessing, shard dispatch, WAL/ACK boundary | PASS |
| Stable public errors | `architecture/references/languages/errors.md`; spec REQ-019 | `QueryCatalogError`, query terminals, protobuf conversion, shared client reconstruction, Python stable-code assertion | PASS |
| Sensitive Variant/Struct data | Spec INV-004 and AC-008; task scenario 2 | Gateway payload declarations and plan traversal; Variant SQL; nullable Struct remediation; Rust/Python/TypeScript/MCP consumers | PASS |
| Prior review and remediation closure | r1-r4 security reports, verdicts, validation ledgers, and remediation packets | Original pre-ACK validation and distributed-error findings, r4 follow-on corrections, and rejected `SEC-R4-001` proposal | PASS |

CodeGraph was used first to locate the live authorization, catalog, Scribe, and
distributed-error paths. The complete cumulative diff and candidate source at
the relevant owners were then inspected.

## Trust-Boundary Trace

1. Public query admission checks the authenticated principal's coarse query
   capability before Oracle receives the request. Oracle resolves the complete
   tenant-bound table set and checks every resolved `TableUid` before
   materializing metadata, manifests, or data objects
   (`oracle/planner.rs:227-263`; `oracle/mod.rs:3886-3919`). One uncovered table
   refuses the whole query; no narrowed plan or rows are returned.
2. The additional `vala.gateway.calls` payload decision walks the optimized
   logical plan, including projections, filters, `SELECT *`, and subqueries,
   before `create_physical_plan` invokes `TableProvider::scan`
   (`oracle/mod.rs:3306-3327,3922-3985`). Variant operators and `to_json` remain
   expressions over the same gated root column and add no alternate reader.
3. The r4 `SEC-R4-001` proposal was independently rechecked and remains
   rejected. Cut materialization and Scribe participant discovery may precede
   the payload-column gate, but the approved TASK-001 obligation is refusal
   before provider/source execution. The gate still precedes physical scan
   construction, follower execution, and row IO. Requiring a second
   schema-only planning architecture or zero catalog/roster work would broaden
   the approved task without closing a data-access gap.
4. Tenant identity continues to come from the verified principal. Scribe
   compares `authenticated_tenant` with the principal-bound physical binding
   before row preparation (`scribe/ingress.rs:408-450`); Oracle and analytical
   peers bind tenant, table identities, snapshot/cut, nodes and fences,
   reservation, permission digest, and request body before provider or storage
   work. No SQL text, Arrow metadata, Variant content, object key, or worker
   error selects the effective tenant.
5. Raw Arrow input reaches the table-owned validator during row preparation.
   Duplicate/server-owned fields, the complete built-in schema phase, Variant
   extension identity, and Variant bytes are refused before preprocessing and
   before `try_send` transfers anything to the shard (`execution_lanes.rs:533-621`;
   `scribe/ingress.rs:552-619`). WAL append and durable ACK remain shard-owned,
   after that transfer. The r4 changes preserve the earlier schema-error
   precedence and cap hostile encoded depth before upstream recursive
   validation (`wyrd-queue/src/variant.rs:167-196,752-790`).
6. Catalogued local and worker failures travel through the single tagged
   `BifrostError` envelope; malformed, unknown, or ordinary human text becomes
   the generic execution failure (`oracle/mod.rs:4241-4307`). The tenant
   invariant is a fieldless public error, so a cross-tenant refusal discloses
   no foreign tenant, object path, SQL, or provider diagnostic. The terminal
   carries the RFC 9457 problem bytes and the protobuf decoder rejects invalid
   problem payloads and invalid terminal combinations
   (`wyrd-tonic/src/query_conversion.rs:272-304,456-490`). Failed collection
   does not return previously streamed rows.
7. Gateway capture continues to redact secrets and replace binary payloads with
   digest references before Variant encoding. The two gateway payload columns
   retain `PayloadClass::Sensitive` and the explicit payload-read list. The r4
   nullable-child changes remove fabricated values under absent Struct parents;
   they do not weaken sensitivity or introduce a leaf bypass.
8. The Python negative write journey now asserts the structured error's `code`
   field rather than matching presentation text, and still proves the denied
   batch leaves no durable row
   (`sdks/wyrd-sdk-python/tests/integration/bifrost/test_bifrost_e2e.py:350-366`).

## Verification

The following focused commands ran against the immutable candidate and passed:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority) | \
      test(=oracle::tests::catalog_errors_keep_their_identity_locally_and_remotely) | \
      test(=scribe::execution_lanes::tests::canonical_validator_refusal_keeps_its_catalogued_code) | \
      test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'
# 4 passed

mise exec -- cargo nextest run --locked -p wyrd-queue --lib \
  -E 'test(=variant::tests::raw_depth_is_bounded_before_full_validation) | \
      test(=variant::tests::variant_extension_requires_empty_metadata) | \
      test(=variant::tests::numeric_range_outranks_depth_in_any_key_order)'
# 3 passed

mise exec -- cargo nextest run --locked -p wyrd-tonic --lib \
  -E 'test(=query_conversion::tests::failed_terminal_problem_round_trips)'
# 1 passed

git diff --check 80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2..0e37748f3a27d3bcec4713e6210e97328e045886
# passed
```

Verification limit: this independent pass did not rerun the serialized
Postgres-backed pre-ACK journey, the multi-process worker-tenant journey, or
the Python integration journey. The task's final-candidate evidence records
those exact journeys as passing, and this review inspected their assertions
and production owners end to end.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None required by the approved task or a reachable regression.

### Positive Controls

- Public and object-scoped authorization fail closed before protected source
  execution; the sensitive gateway payload gate covers projections, filters,
  wildcards, and subqueries.
- Tenant authority is derived from verified identity and repeated through
  catalog, Scribe, Oracle, peer, assignment, footer, and audit boundaries.
- Distributed permissions bind the exact authorized table set; worker errors
  cannot widen authority or select an audit tenant.
- Cross-tenant footer failures stop the whole query, preserve the tenant
  invariant code, disclose no foreign identity, and return no partial result.
- Built-in Arrow schema and Variant validation complete before shard dispatch,
  WAL append, ACK, and durable rows, including hostile depth, foreign
  extension metadata, and competing schema failures.
- Stable late errors use catalog problem documents across HTTP/gRPC/SDK
  terminals; unknown dependency text collapses to the generic execution error.
- Gateway capture remains redacted and payload-gated; audit detail conversion
  preserves the canonical hash inputs and adds no alternate audit path.
- No secret, bearer material, raw payload, tenant storage path, unsafe
  deserialization, SQL/command injection path, CORS/redirect change, mutable
  dependency reference, or new security bypass was introduced.

## Proposed Findings

None.

## Overall Result

**PASS** — no exploitable security defect, RBAC or tenant-isolation regression,
sensitive-data exposure, partial-result acceptance, or pre-ACK write gap was
found in the cumulative candidate or r4 remediation.
