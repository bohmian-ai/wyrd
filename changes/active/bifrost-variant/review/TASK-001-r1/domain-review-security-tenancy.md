# Security, RBAC, and Tenancy Domain Review

## Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `3cf911fce699bbfe197f8b95e72b13e2f551f766`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 10
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Reviewed boundary: sensitive Variant/Struct data, Oracle RBAC ordering, distributed peer authority, tenant binding/tripwires, gateway payload access, retained audit detail, client/agent result rendering, and changed dependency pins.

The candidate remained the repository `HEAD` throughout this review.

## Authority and Source Coverage

| Boundary | Governing authority | Source and caller coverage | Result |
|---|---|---|---|
| Server-owned identity and authorization | `AGENTS.md` §§2, 9; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` §§Security principles, Authorization and policy | `oracle/mod.rs` planning and scoped-table decisions; public query paths and gateway payload tests | PASS |
| Sensitive Variant/Struct access (INV-004, AC-008) | Spec INV-004/AC-008; `architecture/bifrost-design.md` §§Query: Oracle, Public surface; `architecture/references/domain/telemetry-observations.md` | Built-in declarations under `tables/{traces,logs,metrics,eval,verification,gateway,dev}`; `CanonicalField` sensitivity metadata; `authorize_payload_columns`; Variant SQL lowering | PASS |
| Tenant isolation (INV-006) | Spec INV-006; `architecture/wyrd-security-posture.md` §§Peer identity and distributed Oracle, Tenant and data isolation; `architecture/references/domain/olap-serving.md` | `oracle/{mod,follower,peer,analytical,codec,live}`; catalog binding and v3 validation; audit projection tenant check; unchanged footer/source binding paths | PASS |
| Distributed plan and worker authority | Spec Oracle registration/distributed-wire contract; `architecture/bifrost-design.md` §§Distributed analytical execution, Resource and failure invariants | `OracleExecution::session_state`; leader/worker session builders; `physical_plan_fingerprint`; `stage_body_digest`; follower preflight before plan decode/provider IO | PASS |
| Gateway payload confidentiality | `architecture/bifrost-design.md` §Public surface; `architecture/wyrd-security-posture.md` gateway capture rules | Renamed Variant payload columns in `tables/gateway/calls.rs`; optimized-plan traversal in `oracle/mod.rs`; capture still redacts secrets and derives tenant-bound object keys | PASS |
| Audit integrity/privacy | `AGENTS.md` §2; `architecture/agent-rules.md`; `architecture/wyrd-security-posture.md` §Audit integrity and privacy | `tables/audit/{audit_log,projection}.rs`; canonical staged JSON converted to Variant after same-tenant/range validation; original hash inputs are not recomputed or altered | PASS |
| Public terminals and stable errors | Spec REQ-017–REQ-019; `architecture/references/languages/agent-harness.md` | Rust/HTTP/gRPC client reconstruction, CLI/MCP/Python/TypeScript Variant JSON terminals, bounded MCP collection | PASS |
| Supply chain | Task dependency constraints; `AGENTS.md` §§4, 12 | Workspace manifests and lockfile; direct Arrow 59.3 Variant crates only in `wyrd-queue` and `vala-bifrost-redux`; immutable Iceberg/compaction git revisions | PASS |

## Security Trace

- Oracle still resolves and authorizes every table before physical planning and source IO. Variant `->`, `->>`, and `to_json` remain expressions over an already-authorized logical source column; TASK-001 adds no physical-leaf or side-channel reader.
- The gateway-specific payload decision remains on the optimized logical plan and examines scan projections and filters, including subqueries and `SELECT *`, before `create_physical_plan` (`oracle/mod.rs:3306-3324`, `3919-3982`). Renaming the columns to `request_payload` and `response_payload` updates the same canonical sensitive-column list used by that check.
- Every production execution/follower state resolves Variant functions through `OracleVariantSql`: the planning state, admitted `OracleExecution::session_state`, remote follower state, analytical worker state, and analytical leader state. The worker does not receive a weaker function registry than the leader.
- Peer/version disagreement fails before executable plan decode or tenant IO. Variant SQL version participates in the physical-plan fingerprint and analytical stage body digest; follower preflight compares the locally computed fingerprint before protobuf plan decode (`oracle/follower.rs:1432-1454`) and still checks authenticated tenant, exact table binding, role fences, reservation, assignments, and schema fingerprints.
- No tenant source changed from verified/authenticated bindings to query text, payload fields, object names, or peer bytes. Physical namespace/object prefix/footer checks remain in their existing owners. The new retained-audit conversion independently rejects a row whose `data_tenant_id` differs from the authenticated publication tenant before encoding detail (`tables/audit/projection.rs:86-140`).
- Audit `detail` is converted from the already-hashed canonical JSON text to the standard Arrow Variant representation. The projection preserves `entry_hash`, `prev_hash`, sequence, principal, permission, outcome, credential attribution, and decision time; invalid or over-limit detail stops the range rather than truncating or rewriting it.
- Query/client error projection exposes stable catalog fields (field, row, JSON pointer, numeric/depth/size limits), not input JSON, credentials, bearer tokens, tenant storage paths, or provider diagnostics. MCP JSON rendering retains its existing row/byte budgets.

## Drift Audit

No security-related `DRIFT` was found. The new controls are required by the approved task or reuse established mechanisms: DataFusion UDF/session registration, SHA-256 protocol/domain digests, authenticated peer preflight, existing logical-plan RBAC, standard Arrow/Parquet Variant crates, Iceberg v3, and immutable git dependency pins. No new security switch, compatibility path, policy engine, alternate audit sink, tenant selector, signing layer, or bespoke permission mechanism entered the diff.

## Verification

The task records successful final-candidate execution of the full task-local evidence, including the real-server Oracle journey `published::variant_sql_registry_covers_every_session`, built-in Rust/Python/TypeScript/MCP journeys, audit-hash checks, `codegen:check`, format/lints, and `git diff --check`.

This review additionally ran:

```text
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support \
  -E 'test(=oracle::tests::gateway_payload_columns_require_payload_read_authority) | \
      test(=oracle::tests::remote_variant_errors_keep_their_catalog_identity) | \
      test(=oracle::variant_sql::tests::variant_operators_and_functions_follow_the_contract) | \
      test(=tables::tests::variant_contract_and_builtin_schemas_are_stable)'
```

Result: 4 passed, 0 failed.

Verification limits: this independent pass did not rerun the Postgres-backed multi-process Oracle, SDK, MCP, OTLP, or Forge journeys. Their exact final-candidate commands and passing results are preserved in the task evidence; source inspection confirmed that the security assertions are on production owners rather than test-only substitutes.

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

- Authorization remains before provider construction, physical planning, peer dispatch, and source IO.
- Gateway payload projections and filters require the existing tenant-wide payload permission.
- Distributed workers bind tenant, exact table identity, permission digest, plan/body bytes, role fences, reservation, and protocol/function version before decode.
- Sensitive canonical signal fields retain their sensitivity metadata through Variant and nested Struct declarations.
- Audit publication preserves canonical hash inputs and rejects cross-tenant or malformed ranges before projection.
- Variant conversion is bounded and rejects invalid, oversized, over-deep, or out-of-range values without partial storage.
- No secrets, credentials, bearer material, unsafe deserialization, path traversal, command injection, SQL-string authorization, CORS/redirect change, or mutable dependency reference was introduced.

## Proposed Findings

None.

## Overall Result

**PASS**
