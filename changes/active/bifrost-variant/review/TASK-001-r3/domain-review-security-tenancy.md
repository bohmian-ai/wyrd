# Security, RBAC, Tenancy, and Trust-Boundary Domain Review

## Review Boundary

- Immutable base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Immutable candidate: `555308ba14058ddc56102d2f925298ef43858175`
- Authority: `changes/active/bifrost-variant/spec.md` revision 11, the original TASK-001 packet, and the R1/R2 remediation packets
- Domain: authorization before provider or storage IO, sensitive built-in access, tenant identity and tripwires, Arrow/Variant admission before WAL/ACK, distributed-query peer trust, audit projection/outbox compatibility, public error exposure, and dependency/trust-boundary changes in the cumulative diff

The candidate remained at the requested immutable commit before and after this review. This review does not reassess product behavior outside those security and trust boundaries.

## Authority and Source Coverage

| Concern | Authority and implementation traced | Result |
|---|---|---|
| Object authorization before IO | Spec INV-004 and Bifrost security authority; `oracle/planner.rs:206-275`, `oracle/mod.rs:3885-3918`, and provider registration/physical planning in `oracle/mod.rs:2939-3068` | Every catalog-resolved table UID is authorized as one query-wide set before metadata, manifest, or data materialization. Denial aborts the whole query. |
| Sensitive built-ins | Existing gateway payload permission model; `oracle/mod.rs:3297-3327`, `oracle/mod.rs:3921-3972`, `tables/gateway/calls.rs`, and recursive sensitivity metadata in `tables/fields.rs:117-145` and `tables/fields.rs:274-305` | Gateway payload projections, filters, and subqueries require payload-read authority before physical planning. Other built-ins remain covered by their table permission; no alternate leaf IO path was introduced. |
| Raw Arrow/Variant admission | Spec INV-001/INV-002 and R1; `tables/mod.rs:195-238`, `scribe/execution_lanes.rs:518-583`, `scribe/preprocess.rs:384-406`, and `scribe/ingress.rs:520-627` | Extension identity and every Variant value are validated during preprocessing before shard dispatch, WAL append, or acknowledgement. |
| Tenant isolation | Security posture and Bifrost design; `oracle/exec.rs:1218-1242`, `oracle/exec.rs:1500-1529`, `oracle/exec.rs:1655-1675`, `oracle/follower.rs:1432-1491`, and `oracle/analytical.rs:851-903`, `oracle/analytical.rs:1610-1672` | Execution requires tenant-proving footer metadata; key metadata cannot bypass it. Follower and analytical workers bind authenticated tenant, assignment tenant, table UID, cut, snapshot, permission, and reservation before provider resolution or IO. |
| Distributed peer boundary | Existing authenticated peer model plus `oracle/codec.rs:20-38`, `oracle/follower.rs:1213-1285`, and `oracle/follower.rs:1350-1491` | Codec and Variant SQL versions are included in the plan fingerprint. Full preflight is enforced before resolver/provider IO and repeated after awaits. No caller-supplied tenant substitution or cross-tenant cache key was added. |
| Audit and outbox compatibility | Repository audit authority; `tables/audit/projection.rs:86-171`, `tables/audit/projection.rs:174-231`, and `tables/audit/audit_log.rs:23-80` | Projection validates authenticated tenant, contiguous sequence, hashes, and enums. Converting `detail` to Variant preserves the original audit chain hashes. TASK-001 adds no audit sink, publisher, lease, permission decision path, or blocking audit dependency. |
| Public and late error exposure | Error-catalog authority; `wyrd-spec/src/error.rs:16-36`, `wyrd-spec/src/vala/api.rs:806-924`, `oracle/query_stream.rs:401-526`, `oracle/mod.rs:3658-3699`, `oracle/mod.rs:4232-4279`, `wyrd-tonic/src/query_conversion.rs:258-315`, and `wyrd-client/src/error.rs:127-235` | Failed terminals carry the complete catalog-produced RFC 9457 problem. Known typed errors retain catalog identity; unknown internal failures become a generic query-execution problem while their dependency detail remains in server logs. Variant problems expose bounded field/row/path/numeric metadata, not input JSON, tokens, credentials, filesystem paths, or provider errors. |
| Integer and supply-chain boundaries | Binding decision and `wyrd-queue/src/variant.rs:1-165`; workspace manifests and lockfile | Lexical JSON parsing preserves exact `i64`/`u64` values and refuses integers outside those ranges. `serde_json` arbitrary precision remains disabled. New specialized dependencies remain in server/queue tiers; git dependencies are pinned to immutable revisions rather than branches or wildcards. |

## Trust-Boundary Trace

1. A query is parsed against the catalog, yielding tenant-scoped table bindings and stable table UIDs.
2. The complete table set is authorized before Oracle materializes Iceberg metadata, manifests, or data providers. Sensitive gateway payload expressions receive their additional established permission check before physical-plan creation.
3. Distributed assignments carry authenticated tenant, table binding, snapshot/cut, permission, reservation, and plan/version fingerprints. Followers refuse mismatches before resolver/provider IO, then repeat preflight after asynchronous boundaries.
4. Parquet reads require tenant-proving footer metadata before row-group decoding. The key-metadata shortcut is deliberately refused because it cannot prove tenant ownership.
5. Native writes decode Arrow batches and validate Variant extension identity and payloads before dispatch to the shard that owns WAL and acknowledgement.
6. Query failures cross the HTTP/gRPC/SDK boundary only as catalog problems. Server diagnostics retain internal dependency context without placing it in the public problem.

## Drift Audit

No security or trust-boundary drift was found. The cumulative change reuses Wyrd's established permission engine, tenant identifiers and tripwires, authenticated peer model, audit projection/publisher path, RFC 9457 error catalog, and standard Arrow/Parquet/Iceberg mechanisms. It adds no bespoke policy engine, permission cache, tenant selector, audit relay, request-signing scheme, compatibility route, security setting, or dedicated security check absent from Wyrd and comparable projects.

The binding decisions were applied as review authority: Iceberg row lineage relies only on standard Iceberg v3 handling; there is no duplicate-ID scan or metrics gate; `serde_json` arbitrary precision is off; integers outside `i64`/`u64` are refused; every late failure carries the full catalog problem; and Interactive-only Python/TypeScript late-failure journeys are an accepted test limit rather than a security defect.

## Verification

Independent focused commands run against the immutable candidate:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority) | test(=oracle::query_stream::tests::late_catalog_error_keeps_its_identity) | test(~tables::audit::projection::tests::)'
10 passed, 0 failed

mise exec -- cargo nextest run --locked -p wyrd-tonic --lib \
  -E 'test(=query_conversion::tests::failed_terminal_problem_round_trips)'
1 passed, 0 failed

mise exec -- cargo nextest run --locked -p wyrd-queue --lib \
  -E 'test(=variant::tests::json_text_classifies_integers_from_their_tokens)'
1 passed, 0 failed
```

The final-candidate TASK-001/R1/R2 evidence was also checked for the real-server raw-admission journey, Rust/Python/TypeScript/MCP late failures, and Rust multi-pod Interactive and Analytical execution. This independent domain pass did not rerun the serialized Postgres, multi-pod, Python, TypeScript, or MCP journeys; that is a verification limit only because their preserved evidence addresses the same immutable candidate and the relevant implementations and call sites were inspected. The accepted Python/TypeScript Interactive-only coverage is not escalated.

## Security Audit

### Critical

- None.

### High

- None.

### Medium

- None.

### Low / Defense In Depth

- None required for TASK-001.

### Positive Controls

- Authorization uses catalog-bound tenant and table identities and completes before storage or provider IO.
- Sensitive gateway payload access has a distinct fail-closed permission check over optimized projections, predicates, and subqueries.
- Tenant ownership is proved from Parquet footer metadata before decoding, and an optimization that cannot prove it is rejected.
- Distributed workers validate identity, scope, fences, permission, reservation, and version fingerprints before IO and after asynchronous gaps.
- Malformed or mis-typed Variant Arrow input is rejected before WAL append or acknowledgement.
- Public late errors are produced by the stable error catalog; unclassified dependency failures do not disclose internal details.
- Audit projection preserves tenant and chain-integrity checks while using the sole established audit publication path.
- Dependency pins and JSON-number configuration avoid moving supply-chain references and silent lossy integer coercion.

## Proposed Findings

None.

## Overall Result

**PASS** — no exploitable security defect, authorization/tenancy regression, audit-path violation, sensitive-data exposure, or security-domain drift was found in the cumulative TASK-001 candidate.
